//! Hearme desktop core (stub).
//!
//! Later prompts add commands that drive the native capture helpers
//! (`native/macos-capture`, `native/windows-capture`), the gateway WebSocket
//! client, and local meeting-folder storage. See CLAUDE.md.

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running Hearme");
}
