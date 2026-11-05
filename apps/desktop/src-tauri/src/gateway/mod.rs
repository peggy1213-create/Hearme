//! The cloud transcription engine: streams voiced Opus frames to the Hearme gateway
//! over one WebSocket per meeting and relays back live transcript segments. Keeps
//! sent frames in an on-disk [`outbox`] until acked; on disconnect it keeps the
//! recording going (that is the recorder's job, independent of this), reconnects
//! with backoff, and resends from `lastSeq + 1`. After `offline_cutoff` offline it
//! stops buffering for live mode and lets the final pass handle the recording
//! (Prompt 5 / PROTOCOL.md).

pub mod outbox;
pub mod protocol;

use std::time::{Duration, Instant};

use base64::Engine as _;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::http::{header::AUTHORIZATION, HeaderValue};
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};

use crate::audio::StreamFrame;
use crate::engine::{
    ConnectionStatus, EngineConfig, EngineEvent, StreamEncoder, TranscriptionEngine,
};
use crate::frame::Track;
use outbox::{Outbox, Pending};
use protocol::{ClientMessage, ServerMessage, PROTOCOL_VERSION};

/// A command fed to the engine task.
enum Ctrl {
    Frame(Box<StreamFrame>),
    Finish,
}

/// The cloud gateway engine.
pub struct GatewayEngine {
    tx: mpsc::UnboundedSender<Ctrl>,
}

impl GatewayEngine {
    /// Spawn the engine task. Must be called from within a Tokio runtime (Tauri
    /// provides one). `events` receives connection status, segments, and the end.
    pub fn start(cfg: EngineConfig, events: mpsc::UnboundedSender<EngineEvent>) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = run(cfg, rx, events).await {
                tracing::warn!("gateway engine stopped: {e}");
            }
        });
        Self { tx }
    }
}

impl TranscriptionEngine for GatewayEngine {
    fn submit(&self, frame: StreamFrame) {
        let _ = self.tx.send(Ctrl::Frame(Box::new(frame)));
    }
    fn finish(&self) {
        let _ = self.tx.send(Ctrl::Finish);
    }
    fn name(&self) -> &'static str {
        "gateway"
    }
}

fn track_idx(track: Track) -> usize {
    match track {
        Track::Mic => 0,
        Track::Meeting => 1,
    }
}

/// Per-meeting engine state that survives reconnects.
struct Session {
    next_seq: [u64; 2],
    /// Highest seq the gateway has acked per track; the resume cursor.
    gateway_last_seq: [i64; 2],
    enc: [StreamEncoder; 2],
}

impl Session {
    fn new() -> anyhow::Result<Self> {
        Ok(Self {
            next_seq: [0, 0],
            gateway_last_seq: [-1, -1],
            enc: [StreamEncoder::new()?, StreamEncoder::new()?],
        })
    }

    /// Assign a seq, Opus-encode, and record to the outbox. Returns the frame to send.
    fn ingest(&mut self, frame: &StreamFrame, outbox: &mut Outbox) -> Pending {
        let i = track_idx(frame.track);
        let seq = self.next_seq[i];
        self.next_seq[i] += 1;
        let opus = self.enc[i].encode(&frame.samples).unwrap_or_default();
        let p = Pending {
            seq,
            timestamp_ms: frame.timestamp_ms,
            opus,
        };
        let _ = outbox.record(frame.track, p.clone());
        p
    }
}

fn audio_msg(track: Track, p: &Pending) -> Message {
    let data = base64::engine::general_purpose::STANDARD.encode(&p.opus);
    Message::Text(
        ClientMessage::Audio {
            track: track.as_str().into(),
            seq: p.seq,
            timestamp_ms: p.timestamp_ms,
            data,
        }
        .to_json(),
    )
}

async fn run(
    cfg: EngineConfig,
    mut rx: mpsc::UnboundedReceiver<Ctrl>,
    events: mpsc::UnboundedSender<EngineEvent>,
) -> anyhow::Result<()> {
    let mut outbox = Outbox::open(&cfg.outbox_dir)?;
    let mut session = Session::new()?;
    let mut finishing = false;
    let mut ended = false;
    let mut disconnected_since: Option<Instant> = None;
    let mut backoff = Duration::from_millis(500);

    while !ended {
        let _ = events.send(EngineEvent::Connection(ConnectionStatus::Connecting));
        match connect(&cfg).await {
            Ok(ws) => {
                disconnected_since = None;
                backoff = Duration::from_millis(500);
                let _ = events.send(EngineEvent::Connection(ConnectionStatus::Connected));
                match session_loop(
                    ws,
                    &cfg,
                    &mut rx,
                    &mut session,
                    &mut outbox,
                    &mut finishing,
                    &events,
                )
                .await
                {
                    Outcome::Ended | Outcome::Fatal => ended = true,
                    Outcome::Disconnected => {}
                }
            }
            Err(e) => {
                tracing::debug!("gateway connect failed: {e}");
            }
        }
        if ended {
            break;
        }

        // Offline handling: start the clock, and give up live buffering after the
        // cutoff (the final pass transcribes from the recording instead).
        let since = disconnected_since.get_or_insert_with(Instant::now);
        if since.elapsed() > cfg.offline_cutoff {
            let _ = events.send(EngineEvent::Connection(ConnectionStatus::Offline));
            break;
        }
        let _ = events.send(EngineEvent::Connection(ConnectionStatus::Reconnecting));

        // Keep consuming control while we wait, so frames are still buffered to the
        // outbox and a Finish is noticed.
        drain_while_waiting(backoff, &mut rx, &mut session, &mut outbox, &mut finishing).await;
        backoff = (backoff * 2).min(Duration::from_secs(15));
    }

    let _ = events.send(EngineEvent::Connection(ConnectionStatus::Stopped));
    Ok(())
}

enum Outcome {
    Ended,
    Fatal,
    Disconnected,
}

async fn connect(
    cfg: &EngineConfig,
) -> anyhow::Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
> {
    let mut req = cfg.gateway_url.as_str().into_client_request()?;
    if let Some(token) = &cfg.token {
        // Bearer header, never the URL query (privacy).
        req.headers_mut().insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}"))?,
        );
    }
    let (ws, _resp) = tokio_tungstenite::connect_async(req).await?;
    Ok(ws)
}

fn start_msg(cfg: &EngineConfig) -> Message {
    Message::Text(
        ClientMessage::Start {
            protocol_version: PROTOCOL_VERSION,
            meeting_id: cfg.meeting_id.clone(),
            platform: cfg.platform,
            languages: cfg.languages.clone(),
            sample_rate: crate::audio::STREAM_RATE,
            codec: "opus".into(),
            vocabulary: cfg.vocabulary.clone(),
        }
        .to_json(),
    )
}

async fn session_loop(
    ws: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    cfg: &EngineConfig,
    rx: &mut mpsc::UnboundedReceiver<Ctrl>,
    session: &mut Session,
    outbox: &mut Outbox,
    finishing: &mut bool,
    events: &mpsc::UnboundedSender<EngineEvent>,
) -> Outcome {
    let (mut sink, mut stream) = ws.split();

    if sink.send(start_msg(cfg)).await.is_err() {
        return Outcome::Disconnected;
    }
    // Resume: resend everything the gateway has not acked yet (from lastSeq + 1).
    for track in [Track::Mic, Track::Meeting] {
        let last = session.gateway_last_seq[track_idx(track)];
        for p in outbox.resend_after(track, last) {
            if sink.send(audio_msg(track, &p)).await.is_err() {
                return Outcome::Disconnected;
            }
        }
    }
    // If a Finish arrived during a previous disconnect, re-send Stop now.
    if *finishing && sink.send(stop_msg(cfg)).await.is_err() {
        return Outcome::Disconnected;
    }

    loop {
        tokio::select! {
            ctrl = rx.recv() => {
                match ctrl {
                    Some(Ctrl::Frame(frame)) => {
                        let p = session.ingest(&frame, outbox);
                        if sink.send(audio_msg(frame.track, &p)).await.is_err() {
                            return Outcome::Disconnected;
                        }
                    }
                    Some(Ctrl::Finish) | None => {
                        *finishing = true;
                        if sink.send(stop_msg(cfg)).await.is_err() {
                            return Outcome::Disconnected;
                        }
                    }
                }
            }
            msg = stream.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match protocol::parse_server(&text) {
                            Ok(ServerMessage::Ack { track, last_seq }) => {
                                if let Some(t) = parse_track(&track) {
                                    session.gateway_last_seq[track_idx(t)] = last_seq;
                                    let _ = outbox.ack(t, last_seq);
                                }
                            }
                            Ok(ServerMessage::Segment { segment }) => {
                                let _ = events.send(EngineEvent::Segment(segment));
                            }
                            Ok(ServerMessage::Error { code, message, fatal }) => {
                                let _ = events.send(EngineEvent::Error { code, message, fatal });
                                if fatal { return Outcome::Fatal; }
                            }
                            Ok(ServerMessage::Ended { segment_count, .. }) => {
                                let _ = events.send(EngineEvent::Ended { segment_count });
                                return Outcome::Ended;
                            }
                            Err(e) => tracing::debug!("unparsable server message: {e}"),
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => return Outcome::Disconnected,
                    Some(Ok(_)) => {} // ping/pong/binary ignored
                    Some(Err(_)) => return Outcome::Disconnected,
                }
            }
        }
    }
}

fn stop_msg(cfg: &EngineConfig) -> Message {
    Message::Text(
        ClientMessage::Stop {
            meeting_id: cfg.meeting_id.clone(),
        }
        .to_json(),
    )
}

fn parse_track(s: &str) -> Option<Track> {
    match s {
        "mic" => Some(Track::Mic),
        "meeting" => Some(Track::Meeting),
        _ => None,
    }
}

/// While disconnected, keep reading control so frames are buffered to the outbox and
/// a Finish is remembered; returns after roughly `wait`.
async fn drain_while_waiting(
    wait: Duration,
    rx: &mut mpsc::UnboundedReceiver<Ctrl>,
    session: &mut Session,
    outbox: &mut Outbox,
    finishing: &mut bool,
) {
    let deadline = Instant::now() + wait;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return;
        }
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Some(Ctrl::Frame(frame))) => {
                session.ingest(&frame, outbox); // buffered for resend on reconnect
            }
            Ok(Some(Ctrl::Finish)) | Ok(None) => *finishing = true,
            Err(_) => return, // timed out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::STREAM_FRAME;

    fn frame(track: Track, seq_hint: u64) -> StreamFrame {
        StreamFrame {
            track,
            seq: seq_hint,
            timestamp_ms: seq_hint * 20,
            samples: vec![0.1; STREAM_FRAME],
        }
    }

    #[test]
    fn ingest_assigns_monotonic_per_track_seq_and_buffers() {
        let dir = tempfile::tempdir().unwrap();
        let mut outbox = Outbox::open(dir.path()).unwrap();
        let mut s = Session::new().unwrap();

        let p0 = s.ingest(&frame(Track::Mic, 99), &mut outbox);
        let p1 = s.ingest(&frame(Track::Mic, 99), &mut outbox);
        let m0 = s.ingest(&frame(Track::Meeting, 0), &mut outbox);
        assert_eq!(p0.seq, 0);
        assert_eq!(p1.seq, 1);
        assert_eq!(m0.seq, 0); // independent per track
        assert!(!p0.opus.is_empty());

        // Both buffered for resume until acked.
        assert_eq!(outbox.resend_after(Track::Mic, -1).len(), 2);
        assert_eq!(outbox.resend_after(Track::Meeting, -1).len(), 1);
    }

    #[test]
    fn resume_cursor_drops_acked_frames() {
        let dir = tempfile::tempdir().unwrap();
        let mut outbox = Outbox::open(dir.path()).unwrap();
        let mut s = Session::new().unwrap();
        for _ in 0..4 {
            s.ingest(&frame(Track::Mic, 0), &mut outbox);
        }
        // Gateway acked through seq 1.
        s.gateway_last_seq[track_idx(Track::Mic)] = 1;
        outbox.ack(Track::Mic, 1).unwrap();
        let resend = outbox.resend_after(Track::Mic, s.gateway_last_seq[track_idx(Track::Mic)]);
        assert_eq!(resend.iter().map(|p| p.seq).collect::<Vec<_>>(), vec![2, 3]);
    }

    #[test]
    fn audio_msg_is_base64_opus() {
        let p = Pending {
            seq: 3,
            timestamp_ms: 60,
            opus: vec![1, 2, 3, 4],
        };
        if let Message::Text(t) = audio_msg(Track::Meeting, &p) {
            let v: serde_json::Value = serde_json::from_str(&t).unwrap();
            assert_eq!(v["type"], "audio");
            assert_eq!(v["track"], "meeting");
            assert_eq!(v["seq"], 3);
            let data = v["data"].as_str().unwrap();
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(data)
                .unwrap();
            assert_eq!(decoded, vec![1, 2, 3, 4]);
        } else {
            panic!("expected text message");
        }
    }
}
