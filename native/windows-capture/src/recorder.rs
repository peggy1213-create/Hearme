//! `record` debug subcommand: capture a process's output (by exe name, process tree)
//! plus the mic for a fixed time and write `meeting.wav` + `mic.wav`, so a human can
//! listen and confirm the two tracks are clean and aligned. The macOS helper selects by
//! `--bundle`; on Windows we select by `--process <exe>` (per docs/prompts/03).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::known_apps::KnownApp;
use crate::meeting;
use crate::mic::{self, FrameCallback};
use crate::sessions;
use crate::wasapi::ComApartment;
use crate::wav::WavWriter;

pub struct Options {
    pub process: String,
    pub seconds: f64,
    pub out_dir: PathBuf,
}

type LazyWriter = Arc<Mutex<Option<WavWriter>>>;

pub fn run(options: Options) -> i32 {
    // COM on this thread for the diagnostic session listing below.
    let _com = ComApartment::new();

    let app = KnownApp {
        id: "record".to_string(),
        name: options.process.clone(),
        kind: "app".to_string(),
        exe_names: vec![options.process.clone()],
    };

    let procs = sessions::snapshot_processes();
    let Some(root_pid) = sessions::app_root_pid(&app, &procs) else {
        eprintln!("record: no running process matches \"{}\"", options.process);
        let active = sessions::active_audio_pids();
        let mut names: Vec<String> = procs
            .iter()
            .filter(|p| active.contains(&p.pid))
            .map(|p| p.exe.clone())
            .collect();
        names.sort();
        names.dedup();
        eprintln!(
            "record: processes currently playing audio: {}",
            names.join(", ")
        );
        return 2;
    };

    if let Err(e) = std::fs::create_dir_all(&options.out_dir) {
        eprintln!("record: cannot create out dir: {e}");
        return 1;
    }
    let meeting_path = options.out_dir.join("meeting.wav");
    let mic_path = options.out_dir.join("mic.wav");

    let meeting_writer: LazyWriter = Arc::new(Mutex::new(None));
    let mic_writer: LazyWriter = Arc::new(Mutex::new(None));

    let mw = Arc::clone(&meeting_writer);
    let meeting_cb: FrameCallback = Box::new(move |samples, _ns, rate, ch| {
        append_lazy(&mw, &meeting_path, rate, ch as u16, samples);
    });

    let mic_w = Arc::clone(&mic_writer);
    let mic_cb: FrameCallback = Box::new(move |samples, _ns, rate, ch| {
        append_lazy(&mic_w, &mic_path, rate, ch as u16, samples);
    });

    let meeting_cap = match meeting::start(root_pid, meeting_cb) {
        Ok((cap, _)) => cap,
        Err(e) => {
            eprintln!("record: failed to start meeting capture: {e:?}");
            return 1;
        }
    };
    let mic_cap = match mic::start(None, mic_cb) {
        Ok((cap, _)) => Some(cap),
        Err(e) => {
            eprintln!("record: mic capture unavailable ({e:?}); recording meeting only");
            None
        }
    };

    eprintln!(
        "record: capturing pid {root_pid} (process tree) for {}s to {} …",
        options.seconds,
        options.out_dir.display()
    );
    std::thread::sleep(std::time::Duration::from_secs_f64(options.seconds));

    meeting_cap.stop();
    if let Some(m) = mic_cap {
        m.stop();
    }

    let wrote_meeting = finalize(&meeting_writer);
    let wrote_mic = finalize(&mic_writer);
    eprintln!(
        "record: done. meeting.wav={} mic.wav={}",
        if wrote_meeting { "ok" } else { "empty" },
        if wrote_mic { "ok" } else { "empty" }
    );
    if !wrote_meeting {
        eprintln!("record: no meeting audio — is the app actually playing sound right now?");
    }
    0
}

fn append_lazy(
    writer: &LazyWriter,
    path: &std::path::Path,
    rate: u32,
    channels: u16,
    samples: &[f32],
) {
    let Ok(mut guard) = writer.lock() else {
        return;
    };
    if guard.is_none() {
        *guard = WavWriter::create(path, rate, channels).ok();
    }
    if let Some(w) = guard.as_mut() {
        w.append(samples);
    }
}

fn finalize(writer: &LazyWriter) -> bool {
    if let Ok(mut guard) = writer.lock() {
        if let Some(w) = guard.as_mut() {
            w.close();
            return true;
        }
    }
    false
}
