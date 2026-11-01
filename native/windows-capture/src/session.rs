//! Protocol mode: read JSON commands on stdin, emit binary frames on stdout. All
//! capture state is mutated on a single control thread fed by one channel (stdin
//! commands, a 1 s source monitor, and device-change notifications), so commands, the
//! monitor and device callbacks never race — the Windows analogue of the macOS helper's
//! serial control queue. Audio callbacks run on their own threads and only write frames
//! (stdout is mutex-guarded) and feed their own silence guard.

use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

use crate::clock;
use crate::device_watcher::DeviceWatcher;
use crate::frame::{encode_audio_frame, encode_event_frame, Track};
use crate::known_apps::{KnownApp, KnownAppsConfig};
use crate::meeting::{self, MeetingError};
use crate::mic::{self, MicError};
use crate::protocol::{event, parse_command, CommandParseError};
use crate::sessions;
use crate::silence_guard::SilenceGuard;
use crate::wasapi::{ComApartment, FormatInfo};

/// Serialized stdout: every frame (audio or event) is written here under a mutex so the
/// capture threads and the control thread never interleave bytes.
pub struct StdoutWriter {
    inner: Mutex<std::io::Stdout>,
}

impl StdoutWriter {
    pub fn new() -> Arc<StdoutWriter> {
        Arc::new(StdoutWriter {
            inner: Mutex::new(std::io::stdout()),
        })
    }

    fn write_bytes(&self, bytes: &[u8]) {
        if let Ok(mut out) = self.inner.lock() {
            let _ = out.write_all(bytes);
            let _ = out.flush();
        }
    }

    pub fn write_audio(&self, track: Track, host_time_ns: u64, rate: u32, ch: u8, samples: &[f32]) {
        self.write_bytes(&encode_audio_frame(track, host_time_ns, rate, ch, samples));
    }

    pub fn emit(&self, json: &[u8], track: Track) {
        self.write_bytes(&encode_event_frame(json, clock::now_ns(), track));
    }
}

/// Messages delivered to the single control thread.
pub enum ControlMsg {
    Command(String),
    MonitorTick,
    DeviceChanged(&'static str), // "input" | "output"
    Shutdown,
}

pub struct Session {
    out: Arc<StdoutWriter>,
    config: KnownAppsConfig,
    active_app: Option<KnownApp>,
    active_root_pid: Option<u32>,
    mic: Option<crate::wasapi::Capture>,
    meeting: Option<crate::wasapi::Capture>,
    mic_device_id: Option<String>,
    playing: Arc<AtomicBool>,
    producing: HashMap<String, bool>,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    pub fn new() -> Session {
        Session {
            out: StdoutWriter::new(),
            config: KnownAppsConfig::load(),
            active_app: None,
            active_root_pid: None,
            mic: None,
            meeting: None,
            mic_device_id: None,
            playing: Arc::new(AtomicBool::new(false)),
            producing: HashMap::new(),
        }
    }

    /// Run the control loop until stdin EOF (the core closed the pipe) or `stop`.
    pub fn run(&mut self) {
        // COM for this (control) thread, needed by the device watcher and session
        // enumeration. Capture threads initialize their own apartments.
        let _com = ComApartment::new();

        let (tx, rx) = mpsc::channel::<ControlMsg>();
        spawn_stdin_reader(tx.clone());
        spawn_monitor(tx.clone());
        let _watcher = DeviceWatcher::register(tx.clone()).ok();

        for msg in rx {
            match msg {
                ControlMsg::Command(line) => self.dispatch(&line),
                ControlMsg::MonitorTick => self.monitor_tick(),
                ControlMsg::DeviceChanged(kind) => self.handle_device_change(kind),
                ControlMsg::Shutdown => {
                    self.stop_capture(false);
                    break;
                }
            }
        }
    }

    fn dispatch(&mut self, line: &str) {
        let command = match parse_command(line) {
            Ok(Some(c)) => c,
            Ok(None) => return,
            Err(CommandParseError::MissingCmd) => {
                self.out.emit(
                    &event::error("bad_request", "command missing cmd"),
                    Track::Meeting,
                );
                return;
            }
            Err(CommandParseError::NotJson) => {
                self.out.emit(
                    &event::error("bad_request", "unparseable command"),
                    Track::Meeting,
                );
                return;
            }
        };

        match command.cmd.as_str() {
            "list_sources" => {
                let procs = sessions::snapshot_processes();
                let active = sessions::active_audio_pids();
                let srcs = sessions::sources(&self.config, &procs, &active);
                self.out.emit(&event::sources(srcs), Track::Meeting);
            }
            "start" => match command.source_id {
                Some(id) => self.start_capture(&id, command.mic_device_id),
                None => self.out.emit(
                    &event::error("bad_request", "start requires sourceId"),
                    Track::Meeting,
                ),
            },
            "stop" => self.stop_capture(true),
            "ping" => self.out.emit(&event::pong(), Track::Meeting),
            _ => self.out.emit(
                &event::error("bad_request", "unknown command"),
                Track::Meeting,
            ),
        }
    }

    fn start_capture(&mut self, source_id: &str, mic_device_id: Option<String>) {
        let Some(app) = self.config.apps.iter().find(|a| a.id == source_id).cloned() else {
            self.out.emit(
                &event::error("bad_request", "unknown sourceId"),
                Track::Meeting,
            );
            return;
        };
        self.stop_capture(false);

        let procs = sessions::snapshot_processes();
        let Some(root_pid) = sessions::app_root_pid(&app, &procs) else {
            self.out.emit(
                &event::error("bad_request", "source not running"),
                Track::Meeting,
            );
            return;
        };
        let active = sessions::active_audio_pids();
        self.playing.store(
            sessions::app_producing(&app, &procs, &active),
            Ordering::Relaxed,
        );
        self.mic_device_id = mic_device_id;

        // Meeting track (required). An unsupported build or hard failure aborts start.
        let format = match self.spawn_meeting(root_pid) {
            Ok(f) => f,
            Err(MeetingError::Unsupported(msg)) => {
                self.out
                    .emit(&event::error("fatal_unsupported", &msg), Track::Meeting);
                return;
            }
            Err(MeetingError::Failed(msg)) => {
                self.out.emit(
                    &event::error("fatal_capture", &format!("meeting: {msg}")),
                    Track::Meeting,
                );
                return;
            }
        };

        // Mic track. A privacy block is reported but does not abort the meeting capture.
        match self.spawn_mic() {
            Ok(()) => {}
            Err(MicError::Blocked) => {
                self.out.emit(
                    &event::mic_blocked("microphone access is blocked in Windows privacy settings"),
                    Track::Mic,
                );
            }
            Err(MicError::Failed(msg)) => {
                self.out.emit(
                    &event::error("mic_failed", &format!("mic: {msg}")),
                    Track::Mic,
                );
            }
        }

        self.active_app = Some(app.clone());
        self.active_root_pid = Some(root_pid);
        self.out.emit(
            &event::capture_started(&app.id, format.sample_rate, 1),
            Track::Meeting,
        );
    }

    fn spawn_meeting(&mut self, root_pid: u32) -> Result<FormatInfo, MeetingError> {
        let audio_out = Arc::clone(&self.out);
        let event_out = Arc::clone(&self.out);
        let playing = Arc::clone(&self.playing);
        let mut guard = SilenceGuard::new(Track::Meeting);

        let cb: mic::FrameCallback = Box::new(move |samples, ns, rate, ch| {
            audio_out.write_audio(Track::Meeting, ns, rate, ch, samples);
            if guard.feed(samples, rate, playing.load(Ordering::Relaxed)) {
                event_out.emit(
                    &event::permission_suspected_missing(Track::Meeting),
                    Track::Meeting,
                );
            }
        });
        let (capture, format) = meeting::start(root_pid, cb)?;
        self.meeting = Some(capture);
        Ok(format)
    }

    fn spawn_mic(&mut self) -> Result<(), MicError> {
        let audio_out = Arc::clone(&self.out);
        let cb: mic::FrameCallback = Box::new(move |samples, ns, rate, ch| {
            audio_out.write_audio(Track::Mic, ns, rate, ch, samples);
        });
        let (capture, _format) = mic::start(self.mic_device_id.clone(), cb)?;
        self.mic = Some(capture);
        Ok(())
    }

    fn stop_capture(&mut self, emit_stopped: bool) {
        let had_active = self.active_app.clone();
        if let Some(m) = self.meeting.take() {
            m.stop();
        }
        if let Some(m) = self.mic.take() {
            m.stop();
        }
        self.active_app = None;
        self.active_root_pid = None;
        self.playing.store(false, Ordering::Relaxed);
        if emit_stopped {
            if let Some(app) = had_active {
                self.out
                    .emit(&event::capture_stopped(Some(&app.id)), Track::Meeting);
            }
        }
    }

    fn monitor_tick(&mut self) {
        let procs = sessions::snapshot_processes();
        let active = sessions::active_audio_pids();

        // Emit source_started / source_stopped on producing-audio transitions.
        for app in &self.config.apps {
            let now = sessions::app_producing(app, &procs, &active);
            let was = self.producing.get(&app.id).copied().unwrap_or(false);
            if now != was {
                let exe = sessions::app_exe_name(app, &procs);
                if now {
                    self.out.emit(
                        &event::source_started(&app.id, &exe, &app.name),
                        Track::Meeting,
                    );
                } else {
                    self.out.emit(
                        &event::source_stopped(&app.id, &exe, &app.name),
                        Track::Meeting,
                    );
                }
                self.producing.insert(app.id.clone(), now);
            }
        }

        // Keep the active capture current: track playing state and re-target the meeting
        // app if it restarted (new root PID), or stop if it disappeared.
        if let Some(app) = self.active_app.clone() {
            self.playing.store(
                sessions::app_producing(&app, &procs, &active),
                Ordering::Relaxed,
            );
            match sessions::app_root_pid(&app, &procs) {
                Some(pid) if Some(pid) != self.active_root_pid => {
                    if let Some(m) = self.meeting.take() {
                        m.stop();
                    }
                    match self.spawn_meeting(pid) {
                        Ok(_) => self.active_root_pid = Some(pid),
                        Err(_) => self.out.emit(
                            &event::error("internal", "failed to re-target meeting capture"),
                            Track::Meeting,
                        ),
                    }
                }
                Some(_) => {}
                None => {
                    // The app exited while capturing.
                    self.stop_capture(true);
                }
            }
        }
    }

    fn handle_device_change(&mut self, kind: &'static str) {
        self.out.emit(&event::device_changed(kind), Track::Meeting);
        // Process loopback is not tied to an output device, so only an input change
        // requires re-pointing (restart the mic on the new default), and only while
        // capturing.
        if kind == "input" && self.mic.is_some() {
            if let Some(m) = self.mic.take() {
                m.stop();
            }
            if let Err(e) = self.spawn_mic() {
                match e {
                    MicError::Blocked => self.out.emit(
                        &event::mic_blocked(
                            "microphone access is blocked in Windows privacy settings",
                        ),
                        Track::Mic,
                    ),
                    MicError::Failed(msg) => self.out.emit(
                        &event::error("mic_failed", &format!("mic: {msg}")),
                        Track::Mic,
                    ),
                }
            }
        }
    }
}

fn spawn_stdin_reader(tx: Sender<ControlMsg>) {
    std::thread::Builder::new()
        .name("tapcapture-stdin".into())
        .spawn(move || {
            use std::io::BufRead;
            let stdin = std::io::stdin();
            for line in stdin.lock().lines() {
                match line {
                    Ok(l) => {
                        if tx.send(ControlMsg::Command(l)).is_err() {
                            return;
                        }
                    }
                    Err(_) => break,
                }
            }
            // EOF: the core closed the pipe. Shut down cleanly.
            let _ = tx.send(ControlMsg::Shutdown);
        })
        .expect("spawn stdin reader");
}

fn spawn_monitor(tx: Sender<ControlMsg>) {
    std::thread::Builder::new()
        .name("tapcapture-monitor".into())
        .spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            if tx.send(ControlMsg::MonitorTick).is_err() {
                return;
            }
        })
        .expect("spawn monitor");
}
