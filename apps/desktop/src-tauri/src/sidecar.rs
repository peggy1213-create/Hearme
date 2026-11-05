//! Managing the native capture helper (`tapcapture`) as a sidecar process.
//!
//! The core writes JSON command lines to the helper's stdin and reads
//! length-prefixed binary frames from its stdout (CAPTURE_HELPER.md). A supervisor
//! task owns the child: it forwards parsed frames to the driver, writes commands,
//! logs the helper's stderr (logs only — never audio/transcript content), and
//! **restarts the helper if it crashes**, re-issuing the active `start` so capture
//! resumes without user action.

use std::path::PathBuf;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::frame::{self, FrameReader, FrameType, HelperEvent, Track};

/// A message from the helper to the driver.
#[derive(Debug, Clone)]
pub enum HelperMessage {
    /// One block of captured audio for a track (native rate, interleaved f32).
    Audio {
        track: Track,
        host_time_ns: u64,
        sample_rate: u32,
        channels: u8,
        samples: Vec<f32>,
    },
    /// A helper event (sources, capture_started, permission_suspected_missing, …).
    Event(HelperEvent),
    /// The helper process exited; the supervisor will restart it unless shutting down.
    Exited { code: Option<i32>, restarting: bool },
}

/// Handle to the running helper. Dropping it shuts the helper down (no restart).
pub struct Helper {
    cmd_tx: mpsc::UnboundedSender<String>,
}

impl Helper {
    /// Start the supervisor for the helper binary at `binary`. Frames and events flow
    /// to `out`. Must be called from within a Tokio runtime.
    pub fn spawn(binary: PathBuf, out: mpsc::UnboundedSender<HelperMessage>) -> Helper {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        tauri::async_runtime::spawn(supervise(binary, cmd_rx, out));
        Helper { cmd_tx }
    }

    fn send(&self, line: String) {
        let _ = self.cmd_tx.send(line);
    }

    pub fn list_sources(&self) {
        self.send(frame::command_line("list_sources", None, None));
    }

    pub fn start_capture(&self, source_id: &str, mic_device_id: Option<&str>) {
        self.send(frame::command_line("start", Some(source_id), mic_device_id));
    }

    pub fn stop_capture(&self) {
        self.send(frame::command_line("stop", None, None));
    }

    pub fn ping(&self) {
        self.send(frame::command_line("ping", None, None));
    }
}

/// The supervisor loop: (re)spawn the child until the command channel closes.
async fn supervise(
    binary: PathBuf,
    mut cmd_rx: mpsc::UnboundedReceiver<String>,
    out: mpsc::UnboundedSender<HelperMessage>,
) {
    // The active `start` line, re-sent after a crash so capture resumes.
    let mut active_start: Option<String> = None;

    loop {
        let mut child = match Command::new(&binary)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("cannot start capture helper {}: {e}", binary.display());
                let _ = out.send(HelperMessage::Exited {
                    code: None,
                    restarting: false,
                });
                return;
            }
        };

        let mut stdin = child.stdin.take().expect("piped stdin");
        let mut stdout = child.stdout.take().expect("piped stdout");
        if let Some(stderr) = child.stderr.take() {
            tauri::async_runtime::spawn(log_stderr(stderr));
        }

        // Re-issue the active start on a restart.
        if let Some(line) = &active_start {
            let _ = stdin.write_all(line.as_bytes()).await;
            let _ = stdin.flush().await;
        }

        let mut reader = FrameReader::new();
        let mut buf = vec![0u8; 16 * 1024];
        let mut shutting_down = false;

        loop {
            tokio::select! {
                read = stdout.read(&mut buf) => {
                    match read {
                        Ok(0) => break,           // EOF — the child exited
                        Ok(n) => {
                            reader.push(&buf[..n]);
                            while let Some(f) = reader.next_frame() {
                                forward_frame(f, &out);
                            }
                        }
                        Err(_) => break,
                    }
                }
                cmd = cmd_rx.recv() => {
                    match cmd {
                        Some(line) => {
                            if line.contains("\"start\"") {
                                active_start = Some(line.clone());
                            } else if line.contains("\"stop\"") {
                                active_start = None;
                            }
                            if stdin.write_all(line.as_bytes()).await.is_err()
                                || stdin.flush().await.is_err()
                            {
                                break; // pipe broke; treat as a crash
                            }
                        }
                        None => {
                            // Handle dropped → shut the helper down for good.
                            shutting_down = true;
                            let _ = child.start_kill();
                            break;
                        }
                    }
                }
            }
        }

        let code = child.wait().await.ok().and_then(|s| s.code());
        if shutting_down {
            let _ = out.send(HelperMessage::Exited {
                code,
                restarting: false,
            });
            return;
        }
        // Unexpected exit: report and restart after a short delay.
        let _ = out.send(HelperMessage::Exited {
            code,
            restarting: true,
        });
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

fn forward_frame(f: frame::DecodedFrame, out: &mpsc::UnboundedSender<HelperMessage>) {
    match f.header.frame_type {
        FrameType::Audio => {
            let samples = frame::payload_to_f32(&f.payload);
            let _ = out.send(HelperMessage::Audio {
                track: f.header.track,
                host_time_ns: f.header.host_time_ns,
                sample_rate: f.header.sample_rate,
                channels: f.header.channels,
                samples,
            });
        }
        FrameType::Event => match frame::parse_event(&f.payload) {
            Ok(ev) => {
                let _ = out.send(HelperMessage::Event(ev));
            }
            Err(e) => tracing::debug!("bad helper event: {e}"),
        },
    }
}

/// Forward the helper's stderr to the tracing log. Per CAPTURE_HELPER.md the helper
/// never writes audio or transcript content here, only diagnostics.
async fn log_stderr(stderr: tokio::process::ChildStderr) {
    let mut lines = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        tracing::debug!(target: "tapcapture", "{line}");
    }
}
