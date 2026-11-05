//! Hearme desktop core.
//!
//! Turns the capture helper's raw audio into (a) a recording on disk and (b) a live
//! stream to the gateway. See CLAUDE.md, `packages/shared/CAPTURE_HELPER.md`,
//! `packages/shared/PROTOCOL.md` and `packages/shared/MEETING_FOLDER.md`.

pub mod app;
pub mod audio;
pub mod clock;
pub mod engine;
pub mod events;
pub mod frame;
pub mod gateway;
pub mod keychain;
pub mod meeting;
pub mod recording;
pub mod sidecar;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    app::build()
        .run(tauri::generate_context!())
        .expect("error while running Hearme");
}
