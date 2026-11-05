//! App wiring: state, the driver task that routes helper frames and engine events to
//! the audio pipeline / recorder / UI, and the Tauri commands the UI calls
//! (Prompt 5 §5).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{mpsc, oneshot};

use crate::audio::{self, Pipeline};
use crate::engine::{EngineConfig, EngineEvent, TranscriptionEngine};
use crate::events::{self, names};
use crate::frame::{HelperEvent, Source, Track};
use crate::gateway::GatewayEngine;
use crate::meeting::{self, MeetingFile, MeetingPaths, MeetingStatus, Platform};
use crate::recording::{self, OggRecorder};
use crate::sidecar::{Helper, HelperMessage};

const NO_AUDIO_WARN_AFTER: Duration = Duration::from_secs(4);
const META_WRITE_DEBOUNCE: Duration = Duration::from_secs(2);

/// Runtime settings (resolved from env with sane defaults).
#[derive(Debug, Clone)]
pub struct Settings {
    pub meetings_root: PathBuf,
    pub gateway_url: String,
}

impl Settings {
    fn from_env() -> Self {
        let meetings_root = std::env::var_os("HEARME_MEETINGS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(default_meetings_root);
        let gateway_url = std::env::var("HEARME_GATEWAY_URL")
            .unwrap_or_else(|_| "ws://127.0.0.1:8787/v1/live".to_string());
        Self {
            meetings_root,
            gateway_url,
        }
    }
}

fn default_meetings_root() -> PathBuf {
    if let Some(docs) = dirs_documents() {
        docs.join("Hearme Meetings")
    } else {
        std::env::temp_dir().join("Hearme Meetings")
    }
}

/// Documents dir without pulling in a crate: `%USERPROFILE%\Documents` on Windows,
/// `$HOME/Documents` elsewhere.
fn dirs_documents() -> Option<PathBuf> {
    #[cfg(windows)]
    let home = std::env::var_os("USERPROFILE");
    #[cfg(not(windows))]
    let home = std::env::var_os("HOME");
    home.map(|h| PathBuf::from(h).join("Documents"))
}

/// One in-progress meeting: its pipeline, recorder, engine and metadata file.
pub struct ActiveMeeting {
    meeting_id: String,
    paths: MeetingPaths,
    pipeline: Pipeline,
    recorder: OggRecorder,
    file: MeetingFile,
    engine: Box<dyn TranscriptionEngine>,
    last_flush: Instant,
    last_meta_write: Instant,
    last_audio: Option<Instant>,
    no_audio_warned: bool,
}

impl ActiveMeeting {
    /// Process one captured audio block: DSP → recorder + live stream.
    fn feed_audio(
        &mut self,
        track: Track,
        host_time_ns: u64,
        rate: u32,
        channels: u8,
        samples: &[f32],
    ) {
        self.last_audio = Some(Instant::now());
        self.no_audio_warned = false;
        let frames = self.pipeline.process_audio(
            track,
            host_time_ns,
            rate,
            channels,
            samples,
            &mut self.recorder,
        );
        for f in frames {
            self.engine.submit(f);
        }
        if self.last_flush.elapsed() >= Duration::from_secs(10) {
            if let Err(e) = self.recorder.flush() {
                tracing::warn!("recorder flush failed: {e}");
            }
            self.last_flush = Instant::now();
        }
    }

    fn add_segment(&mut self, seg: meeting::TranscriptSegment) {
        self.file.live_segments.push(seg);
        if self.last_meta_write.elapsed() >= META_WRITE_DEBOUNCE {
            self.persist_meta();
        }
    }

    fn persist_meta(&mut self) {
        if let Err(e) = meeting::write_meeting_json(&self.paths, &self.file) {
            tracing::warn!("meeting.json write failed: {e}");
        }
        self.last_meta_write = Instant::now();
    }

    fn levels(&self) -> (f32, f32) {
        self.pipeline.levels()
    }

    fn aec_name(&self) -> &'static str {
        self.pipeline.aec_name()
    }

    fn set_echo_cancellation(&mut self, enabled: bool) {
        self.pipeline.set_echo_cancellation(enabled);
    }

    /// Finalize: close the recording and mark the meeting file. Consumes self.
    fn finalize(mut self, status: MeetingStatus) {
        self.engine.finish();
        if let Err(e) = self.recorder.finalize() {
            tracing::warn!("recorder finalize failed: {e}");
        }
        self.file.meeting.status = status;
        self.file.meeting.ended_at = Some(meeting::now_iso());
        if let Err(e) = meeting::write_meeting_json(&self.paths, &self.file) {
            tracing::warn!("final meeting.json write failed: {e}");
        }
    }
}

/// Tauri-managed application state.
pub struct AppState {
    helper: Helper,
    active: Arc<Mutex<Option<ActiveMeeting>>>,
    engine_tx: mpsc::UnboundedSender<EngineEvent>,
    settings: Settings,
    pending_sources: Arc<Mutex<Vec<oneshot::Sender<Vec<Source>>>>>,
    aec_default: Arc<AtomicBool>,
}

fn emit<T: Serialize + Clone>(app: &AppHandle, name: &str, payload: T) {
    if let Err(e) = app.emit(name, payload) {
        tracing::debug!("emit {name} failed: {e}");
    }
}

/// Resolve the capture-helper binary: `HEARME_HELPER_PATH`, else next to the app
/// executable (the bundled sidecar location), else the bare name on `PATH`.
fn resolve_helper() -> PathBuf {
    if let Some(p) = std::env::var_os("HEARME_HELPER_PATH") {
        return PathBuf::from(p);
    }
    let name = if cfg!(windows) {
        "tapcapture.exe"
    } else {
        "tapcapture"
    };
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(name);
            if candidate.exists() {
                return candidate;
            }
        }
    }
    PathBuf::from(name)
}

fn gen_meeting_id() -> String {
    use std::sync::atomic::AtomicU64;
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:x}-{n:x}")
}

/// The driver task: routes helper frames + engine events to the active meeting and
/// the UI, and emits levels ~10/s plus a no-audio watchdog.
async fn driver(
    app: AppHandle,
    mut helper_rx: mpsc::UnboundedReceiver<HelperMessage>,
    mut engine_rx: mpsc::UnboundedReceiver<EngineEvent>,
    active: Arc<Mutex<Option<ActiveMeeting>>>,
    pending_sources: Arc<Mutex<Vec<oneshot::Sender<Vec<Source>>>>>,
) {
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            msg = helper_rx.recv() => match msg {
                Some(msg) => handle_helper(msg, &app, &active, &pending_sources),
                None => break,
            },
            ev = engine_rx.recv() => if let Some(ev) = ev { handle_engine(ev, &app, &active) },
            _ = tick.tick() => tick_levels(&app, &active),
        }
    }
}

fn handle_helper(
    msg: HelperMessage,
    app: &AppHandle,
    active: &Arc<Mutex<Option<ActiveMeeting>>>,
    pending_sources: &Arc<Mutex<Vec<oneshot::Sender<Vec<Source>>>>>,
) {
    match msg {
        HelperMessage::Audio {
            track,
            host_time_ns,
            sample_rate,
            channels,
            samples,
        } => {
            if let Ok(mut guard) = active.lock() {
                if let Some(m) = guard.as_mut() {
                    m.feed_audio(track, host_time_ns, sample_rate, channels, &samples);
                }
            }
        }
        HelperMessage::Event(ev) => handle_helper_event(ev, app, active, pending_sources),
        HelperMessage::Exited { code, restarting } => {
            tracing::warn!("capture helper exited (code {code:?}); restarting={restarting}");
            if restarting {
                emit(
                    app,
                    names::WARNING,
                    events::Warning {
                        kind: events::WarningKind::HelperCrashed,
                        message: "Audio capture helper restarted".into(),
                    },
                );
                emit(
                    app,
                    names::RECORDING_STATUS,
                    recording_status(active, events::RecordingState::Restarting),
                );
            }
        }
    }
}

fn handle_helper_event(
    ev: HelperEvent,
    app: &AppHandle,
    active: &Arc<Mutex<Option<ActiveMeeting>>>,
    pending_sources: &Arc<Mutex<Vec<oneshot::Sender<Vec<Source>>>>>,
) {
    match ev {
        HelperEvent::Sources { sources } => {
            if let Ok(mut waiters) = pending_sources.lock() {
                for tx in waiters.drain(..) {
                    let _ = tx.send(sources.clone());
                }
            }
            emit(app, names::SOURCES, events::SourcesPayload { sources });
        }
        HelperEvent::SourceStarted {
            source_id,
            bundle_id,
            name,
        } => emit(
            app,
            names::SOURCE_STARTED,
            events::SourceChange {
                source_id,
                name,
                bundle_id,
            },
        ),
        HelperEvent::SourceStopped {
            source_id,
            bundle_id,
            name,
        } => emit(
            app,
            names::SOURCE_STOPPED,
            events::SourceChange {
                source_id,
                name,
                bundle_id,
            },
        ),
        HelperEvent::CaptureStarted { .. } => emit(
            app,
            names::RECORDING_STATUS,
            recording_status(active, events::RecordingState::Recording),
        ),
        HelperEvent::PermissionSuspectedMissing { track } => emit(
            app,
            names::WARNING,
            events::Warning {
                kind: events::WarningKind::Permission,
                message: format!("Audio permission may be missing for the {track} track"),
            },
        ),
        HelperEvent::MicBlocked { .. } => emit(
            app,
            names::WARNING,
            events::Warning {
                kind: events::WarningKind::Permission,
                message: "Microphone is blocked by a system privacy setting".into(),
            },
        ),
        HelperEvent::Error { code, message } => {
            tracing::warn!("helper error {code}: {message}");
        }
        HelperEvent::CaptureStopped { .. }
        | HelperEvent::DeviceChanged { .. }
        | HelperEvent::Pong
        | HelperEvent::Other => {}
    }
}

fn handle_engine(ev: EngineEvent, app: &AppHandle, active: &Arc<Mutex<Option<ActiveMeeting>>>) {
    match ev {
        EngineEvent::Connection(status) => emit(
            app,
            names::CONNECTION_STATUS,
            events::ConnectionStatusPayload { status },
        ),
        EngineEvent::Segment(segment) => {
            emit(
                app,
                names::SEGMENT,
                events::SegmentPayload {
                    segment: segment.clone(),
                },
            );
            if let Ok(mut guard) = active.lock() {
                if let Some(m) = guard.as_mut() {
                    m.add_segment(segment);
                }
            }
        }
        EngineEvent::Ended { segment_count } => {
            tracing::info!("live session ended ({segment_count} segments)");
        }
        EngineEvent::Error {
            code,
            message,
            fatal,
        } => {
            tracing::warn!("engine error {code} (fatal={fatal}): {message}");
        }
    }
}

fn tick_levels(app: &AppHandle, active: &Arc<Mutex<Option<ActiveMeeting>>>) {
    let mut warn_no_audio = false;
    if let Ok(mut guard) = active.lock() {
        if let Some(m) = guard.as_mut() {
            let (mic, meeting) = m.levels();
            emit(app, names::LEVELS, events::Levels { mic, meeting });
            if m.pipeline.started() {
                let stale = m
                    .last_audio
                    .map(|t| t.elapsed() > NO_AUDIO_WARN_AFTER)
                    .unwrap_or(true);
                if stale && !m.no_audio_warned {
                    m.no_audio_warned = true;
                    warn_no_audio = true;
                }
            }
        }
    }
    if warn_no_audio {
        emit(
            app,
            names::WARNING,
            events::Warning {
                kind: events::WarningKind::NoAudio,
                message: "Capture started but no audio is arriving".into(),
            },
        );
    }
}

fn recording_status(
    active: &Arc<Mutex<Option<ActiveMeeting>>>,
    state: events::RecordingState,
) -> events::RecordingStatus {
    let guard = active.lock().ok();
    let m = guard.as_ref().and_then(|g| g.as_ref());
    events::RecordingStatus {
        state,
        meeting_id: m.map(|m| m.meeting_id.clone()),
        folder_path: m.map(|m| m.paths.dir.to_string_lossy().to_string()),
        echo_cancellation: m.map(|m| m.aec_name()).unwrap_or("off").to_string(),
    }
}

// --- Tauri commands ---

#[tauri::command]
async fn list_sources(state: tauri::State<'_, AppState>) -> Result<Vec<Source>, String> {
    let (tx, rx) = oneshot::channel();
    state
        .pending_sources
        .lock()
        .map_err(|_| "state poisoned")?
        .push(tx);
    state.helper.list_sources();
    match tokio::time::timeout(Duration::from_secs(3), rx).await {
        Ok(Ok(sources)) => Ok(sources),
        _ => Err("capture helper did not respond".into()),
    }
}

#[derive(serde::Deserialize)]
pub struct StartArgs {
    #[serde(rename = "sourceId")]
    source_id: String,
    title: Option<String>,
}

#[tauri::command]
async fn start_meeting(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    args: StartArgs,
) -> Result<String, String> {
    if state.active.lock().map_err(|_| "state poisoned")?.is_some() {
        return Err("a meeting is already being recorded".into());
    }

    // Warn (but proceed) on low disk.
    if let Some(free) = meeting::free_space_bytes(&state.settings.meetings_root) {
        if free < meeting::LOW_DISK_BYTES {
            emit(
                &app,
                names::WARNING,
                events::Warning {
                    kind: events::WarningKind::DiskSpace,
                    message: format!("Low disk space: {} MB free", free / 1_000_000),
                },
            );
        }
    }

    let id = gen_meeting_id();
    let title = args.title.unwrap_or_default();
    let platform = Platform::from_source_id(&args.source_id);
    let (paths, file) =
        meeting::create_meeting(&state.settings.meetings_root, &id, &title, platform)
            .map_err(|e| format!("cannot create meeting folder: {e}"))?;
    meeting::write_meeting_json(&paths, &file).map_err(|e| e.to_string())?;

    let recorder = OggRecorder::create(&paths).map_err(|e| e.to_string())?;

    let aec_on = state.aec_default.load(Ordering::Relaxed);
    let pipeline = Pipeline::new(audio::make_aec(), aec_on);

    let mut cfg = EngineConfig::new(state.settings.gateway_url.clone(), id.clone(), platform);
    cfg.outbox_dir = paths.dir.join(".hearme");
    cfg.token = crate::keychain::load_token().ok().flatten();
    let engine = GatewayEngine::start(cfg, state.engine_tx.clone());

    let now = Instant::now();
    let meeting = ActiveMeeting {
        meeting_id: id.clone(),
        paths,
        pipeline,
        recorder,
        file,
        engine: Box::new(engine),
        last_flush: now,
        last_meta_write: now,
        last_audio: None,
        no_audio_warned: false,
    };
    *state.active.lock().map_err(|_| "state poisoned")? = Some(meeting);

    state.helper.start_capture(&args.source_id, None);
    emit(
        &app,
        names::RECORDING_STATUS,
        recording_status(&state.active, events::RecordingState::Starting),
    );
    Ok(id)
}

#[tauri::command]
async fn stop_meeting(app: AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.helper.stop_capture();
    let meeting = state
        .active
        .lock()
        .map_err(|_| "state poisoned")?
        .take()
        .ok_or("no meeting is being recorded")?;
    // Finalize off the async thread: it does blocking file IO and Opus flushing.
    tauri::async_runtime::spawn_blocking(move || meeting.finalize(MeetingStatus::Stopped))
        .await
        .map_err(|e| e.to_string())?;
    emit(
        &app,
        names::RECORDING_STATUS,
        events::RecordingStatus {
            state: events::RecordingState::Stopped,
            meeting_id: None,
            folder_path: None,
            echo_cancellation: "off".into(),
        },
    );
    Ok(())
}

#[tauri::command]
fn set_echo_cancellation(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    state.aec_default.store(enabled, Ordering::Relaxed);
    if let Ok(mut guard) = state.active.lock() {
        if let Some(m) = guard.as_mut() {
            m.set_echo_cancellation(enabled);
        }
    }
    emit(
        &app,
        names::RECORDING_STATUS,
        recording_status(&state.active, events::RecordingState::Recording),
    );
    Ok(())
}

#[tauri::command]
fn open_recording_folder(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let dir = {
        let guard = state.active.lock().map_err(|_| "state poisoned")?;
        guard
            .as_ref()
            .map(|m| m.paths.dir.clone())
            .unwrap_or_else(|| state.settings.meetings_root.clone())
    };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    open_in_file_manager(&dir).map_err(|e| e.to_string())
}

fn open_in_file_manager(dir: &std::path::Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        std::process::Command::new("explorer").arg(dir).spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(dir).spawn()?;
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(dir).spawn()?;
    }
    Ok(())
}

/// Build the Tauri app: managed state, startup (recover crashed meetings, spawn the
/// helper + driver), and the command handlers.
pub fn build() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .setup(|app| {
            let settings = Settings::from_env();
            let _ = std::fs::create_dir_all(&settings.meetings_root);

            // Recover any meeting a previous run left mid-recording.
            let recovered = recording::recover_unfinished(&settings.meetings_root);
            if !recovered.is_empty() {
                tracing::info!("recovered {} unfinished meeting(s)", recovered.len());
            }

            let (helper_tx, helper_rx) = mpsc::unbounded_channel();
            let (engine_tx, engine_rx) = mpsc::unbounded_channel();
            let active = Arc::new(Mutex::new(None));
            let pending_sources = Arc::new(Mutex::new(Vec::new()));

            let helper = Helper::spawn(resolve_helper(), helper_tx);

            app.manage(AppState {
                helper,
                active: active.clone(),
                engine_tx,
                settings,
                pending_sources: pending_sources.clone(),
                aec_default: Arc::new(AtomicBool::new(true)),
            });

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(driver(
                handle,
                helper_rx,
                engine_rx,
                active,
                pending_sources,
            ));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_sources,
            start_meeting,
            stop_meeting,
            set_echo_cancellation,
            open_recording_folder
        ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::no_aec;
    use crate::frame::Track;

    /// A no-op engine so ActiveMeeting can be tested without a gateway.
    struct NullEngine;
    impl TranscriptionEngine for NullEngine {
        fn submit(&self, _f: crate::audio::StreamFrame) {}
        fn finish(&self) {}
        fn name(&self) -> &'static str {
            "null"
        }
    }

    fn active_for(dir: &std::path::Path) -> ActiveMeeting {
        let (paths, file) =
            meeting::create_meeting(dir, "mid", "Driver Test", Platform::Zoom).unwrap();
        meeting::write_meeting_json(&paths, &file).unwrap();
        let recorder = OggRecorder::create(&paths).unwrap();
        let now = Instant::now();
        ActiveMeeting {
            meeting_id: "mid".into(),
            paths,
            pipeline: Pipeline::new(no_aec(), false),
            recorder,
            file,
            engine: Box::new(NullEngine),
            last_flush: now,
            last_meta_write: now - META_WRITE_DEBOUNCE,
            last_audio: None,
            no_audio_warned: false,
        }
    }

    #[test]
    fn feed_audio_records_and_finalize_marks_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let mut m = active_for(dir.path());
        let tone: Vec<f32> = (0..48_000).map(|i| (i as f32 * 0.05).sin() * 0.3).collect();
        m.feed_audio(Track::Meeting, 0, 48_000, 1, &tone);
        m.feed_audio(Track::Mic, 0, 48_000, 1, &tone);
        assert!(m.last_audio.is_some());
        let paths = m.paths.clone();
        m.finalize(MeetingStatus::Stopped);

        let file = meeting::read_meeting_json(&paths.meta()).unwrap();
        assert_eq!(file.meeting.status, MeetingStatus::Stopped);
        assert!(file.meeting.ended_at.is_some());
        // The three audio files exist and are non-trivial.
        for p in [paths.mic(), paths.meeting(), paths.mixed()] {
            assert!(std::fs::metadata(&p).unwrap().len() > 1000, "{p:?}");
        }
    }

    #[test]
    fn add_segment_appends_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let mut m = active_for(dir.path());
        m.add_segment(meeting::TranscriptSegment {
            id: "s0".into(),
            meeting_id: "mid".into(),
            track: "mic".into(),
            speaker: "me".into(),
            speaker_name: None,
            start_ms: 0,
            end_ms: 100,
            text: "hi".into(),
            lang: "en".into(),
            is_final: true,
            pass: "live".into(),
            confidence: None,
        });
        let paths = m.paths.clone();
        m.finalize(MeetingStatus::Stopped);
        let file = meeting::read_meeting_json(&paths.meta()).unwrap();
        assert_eq!(file.live_segments.len(), 1);
        assert_eq!(file.live_segments[0].text, "hi");
    }
}
