//! Auth-token storage in the OS keychain (macOS Keychain, Windows Credential
//! Manager) via the `keyring` crate. Tokens never touch the meeting folder, logs or
//! any config file (CLAUDE.md: vendor keys live only on the gateway; the desktop
//! app holds only its own gateway auth token).

const SERVICE: &str = "com.hearme.desktop";
const ACCOUNT: &str = "gateway-token";

/// Store the gateway auth token.
pub fn store_token(token: &str) -> anyhow::Result<()> {
    let entry = keyring::Entry::new(SERVICE, ACCOUNT)?;
    entry.set_password(token)?;
    Ok(())
}

/// Load the gateway auth token, or `None` if none is stored.
pub fn load_token() -> anyhow::Result<Option<String>> {
    let entry = keyring::Entry::new(SERVICE, ACCOUNT)?;
    match entry.get_password() {
        Ok(t) => Ok(Some(t)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Remove the stored token (sign-out).
pub fn clear_token() -> anyhow::Result<()> {
    let entry = keyring::Entry::new(SERVICE, ACCOUNT)?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}
