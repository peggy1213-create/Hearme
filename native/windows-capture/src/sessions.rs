//! Meeting-app detection: enumerate OS processes (with parent links, for process-tree
//! reasoning) and the set of processes currently **rendering** audio (via
//! `IAudioSessionManager2`). The Windows analogue of the macOS helper's `ProcessList`.
//!
//! A known app "is producing audio" when any active audio session's process is the
//! app's process or a descendant of it — this catches browser renderer children and
//! the new Teams' WebView2 children. For process-loopback capture we target the app's
//! **root** PID and `INCLUDE_TARGET_PROCESS_TREE` covers the rest.

use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};
use windows::core::{Interface, Result};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Media::Audio::{
    eConsole, eRender, AudioSessionStateActive, IAudioSessionControl2, IAudioSessionManager2,
    IMMDeviceEnumerator, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};

use crate::known_apps::{KnownApp, KnownAppsConfig};

/// A process known to the OS, used to detect meeting apps and to resolve
/// process-tree membership for the "producing audio" check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcInfo {
    pub pid: u32,
    pub parent_pid: u32,
    pub exe: String,
}

/// Snapshot all processes via Toolhelp. Returns an empty vec on failure (the caller
/// then simply reports nothing running rather than crashing).
pub fn snapshot_processes() -> Vec<ProcInfo> {
    let mut out = Vec::new();
    unsafe {
        let snapshot: HANDLE = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(h) => h,
            Err(_) => return out,
        };

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                out.push(ProcInfo {
                    pid: entry.th32ProcessID,
                    parent_pid: entry.th32ParentProcessID,
                    exe: wide_to_string(&entry.szExeFile),
                });
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }
    out
}

fn wide_to_string(wide: &[u16]) -> String {
    let len = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..len])
}

/// The PIDs of processes that currently have an **active** render audio session on the
/// default output endpoint. Returns an empty set if there is no render device or COM
/// enumeration fails.
pub fn active_audio_pids() -> HashSet<u32> {
    unsafe { active_audio_pids_inner() }.unwrap_or_default()
}

unsafe fn active_audio_pids_inner() -> Result<HashSet<u32>> {
    let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
    let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
    let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
    let sessions = manager.GetSessionEnumerator()?;
    let count = sessions.GetCount()?;

    let mut pids = HashSet::new();
    for i in 0..count {
        let control = sessions.GetSession(i)?;
        let state = control.GetState()?;
        if state != AudioSessionStateActive {
            continue;
        }
        if let Ok(control2) = control.cast::<IAudioSessionControl2>() {
            if let Ok(pid) = control2.GetProcessId() {
                if pid != 0 {
                    pids.insert(pid);
                }
            }
        }
    }
    Ok(pids)
}

/// Build a pid -> ProcInfo map for ancestry walks.
fn by_pid(procs: &[ProcInfo]) -> HashMap<u32, &ProcInfo> {
    procs.iter().map(|p| (p.pid, p)).collect()
}

/// True if `pid` or any ancestor is a process whose exe belongs to `app`. Depth-capped
/// so a corrupt parent chain can't loop forever.
fn pid_in_app_tree(pid: u32, app: &KnownApp, index: &HashMap<u32, &ProcInfo>) -> bool {
    let mut current = pid;
    for _ in 0..64 {
        let Some(proc) = index.get(&current) else {
            return false;
        };
        if app.matches(&proc.exe) {
            return true;
        }
        if proc.parent_pid == 0 || proc.parent_pid == current {
            return false;
        }
        current = proc.parent_pid;
    }
    false
}

/// The root PID to target for process-loopback capture: a process matching `app` whose
/// parent is **not** also a match (so `INCLUDE_TARGET_PROCESS_TREE` covers the
/// children). Falls back to any matching process, or `None` if the app isn't running.
pub fn app_root_pid(app: &KnownApp, procs: &[ProcInfo]) -> Option<u32> {
    let index = by_pid(procs);
    let matching: Vec<&ProcInfo> = procs.iter().filter(|p| app.matches(&p.exe)).collect();
    if matching.is_empty() {
        return None;
    }
    let root = matching
        .iter()
        .find(|p| {
            index
                .get(&p.parent_pid)
                .map(|parent| !app.matches(&parent.exe))
                .unwrap_or(true)
        })
        .copied();
    Some(root.unwrap_or(matching[0]).pid)
}

/// Is `app` currently producing audio? True if any active render session belongs to the
/// app's process tree.
pub fn app_producing(app: &KnownApp, procs: &[ProcInfo], active: &HashSet<u32>) -> bool {
    let index = by_pid(procs);
    active.iter().any(|&pid| pid_in_app_tree(pid, app, &index))
}

/// The exe name reported for an app (its root process's exe, else its first configured
/// exe name). Used for the protocol's `bundleId` field, which on Windows carries the
/// executable name.
pub fn app_exe_name(app: &KnownApp, procs: &[ProcInfo]) -> String {
    if let Some(root) = app_root_pid(app, procs) {
        if let Some(p) = procs.iter().find(|p| p.pid == root) {
            return p.exe.clone();
        }
    }
    app.exe_names.first().cloned().unwrap_or_default()
}

/// Build the `sources` payload: one entry per known app, with running / producing state
/// resolved from the live process and session lists. Mirrors the macOS
/// `ProcessList.sources`.
pub fn sources(config: &KnownAppsConfig, procs: &[ProcInfo], active: &HashSet<u32>) -> Vec<Value> {
    config
        .apps
        .iter()
        .map(|app| {
            let running = procs.iter().any(|p| app.matches(&p.exe));
            let producing = app_producing(app, procs, active);
            json!({
                "sourceId": app.id,
                "name": app.name,
                "bundleId": app_exe_name(app, procs),
                "running": running,
                "producingAudio": producing,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, exes: &[&str]) -> KnownApp {
        KnownApp {
            id: id.to_string(),
            name: id.to_string(),
            kind: "app".to_string(),
            exe_names: exes.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn procs() -> Vec<ProcInfo> {
        vec![
            ProcInfo {
                pid: 1,
                parent_pid: 0,
                exe: "explorer.exe".into(),
            },
            ProcInfo {
                pid: 10,
                parent_pid: 1,
                exe: "chrome.exe".into(),
            }, // root browser
            ProcInfo {
                pid: 11,
                parent_pid: 10,
                exe: "chrome.exe".into(),
            }, // renderer child
            ProcInfo {
                pid: 20,
                parent_pid: 1,
                exe: "ms-teams.exe".into(),
            },
            ProcInfo {
                pid: 21,
                parent_pid: 20,
                exe: "msedgewebview2.exe".into(),
            }, // Teams child
        ]
    }

    #[test]
    fn root_pid_picks_tree_root_not_child() {
        let chrome = app("meet-chrome", &["chrome.exe"]);
        assert_eq!(app_root_pid(&chrome, &procs()), Some(10));
    }

    #[test]
    fn producing_follows_descendant_sessions() {
        let teams = app("teams", &["ms-teams.exe"]);
        // The WebView2 child (21) is what renders audio; its ancestor is ms-teams.exe.
        let active: HashSet<u32> = [21].into_iter().collect();
        assert!(app_producing(&teams, &procs(), &active));
    }

    #[test]
    fn not_producing_when_no_tree_session_active() {
        let teams = app("teams", &["ms-teams.exe"]);
        let active: HashSet<u32> = [999].into_iter().collect();
        assert!(!app_producing(&teams, &procs(), &active));
    }

    #[test]
    fn exe_name_is_root_process_exe() {
        let chrome = app("meet-chrome", &["chrome.exe"]);
        assert_eq!(app_exe_name(&chrome, &procs()), "chrome.exe");
    }

    #[test]
    fn root_pid_none_when_not_running() {
        let zoom = app("zoom", &["Zoom.exe"]);
        assert_eq!(app_root_pid(&zoom, &procs()), None);
    }
}
