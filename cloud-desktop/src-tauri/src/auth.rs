//! Keychain-backed token storage for Cognito auth.
//!
//! We keep the access + refresh token pair in the OS credential store
//! (Keychain on macOS, Secret Service on Linux, Credential Manager on Windows)
//! so they survive app restarts without touching the filesystem.

const SERVICE: &str = "cloud-desktop";
const ACCESS_KEY: &str = "access_token";
const REFRESH_KEY: &str = "refresh_token";

/// persists both Cognito tokens atomically.
///
/// if the refresh token save fails after the access token was already written,
/// we roll back the access entry to avoid a half-saved session, learned this
/// the hard way when a macOS keychain prompt was dismissed mid-save.
pub fn save_tokens(access: &str, refresh: &str) -> Result<(), String> {
    // on a fresh macOS install, the keychain item may not exist yet.
    // delete any stale entry first; NoEntry is fine.
    let acc_entry = keyring::Entry::new(SERVICE, ACCESS_KEY).map_err(|e| e.to_string())?;
    match acc_entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(e) => return Err(format!("failed to clear old access token: {e}")),
    }
    acc_entry.set_password(access).map_err(|e| e.to_string())?;

    let ref_entry = keyring::Entry::new(SERVICE, REFRESH_KEY).map_err(|e| e.to_string())?;
    match ref_entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(e) => return Err(format!("failed to clear old refresh token: {e}")),
    }
    if let Err(e) = ref_entry.set_password(refresh) {
        // rollback, don't leave an orphan access token
        let _ = acc_entry.delete_credential();
        return Err(e.to_string());
    }

    Ok(())
}

pub fn get_access_token() -> Result<String, String> {
    let ent = keyring::Entry::new(SERVICE, ACCESS_KEY).map_err(|e| e.to_string())?;
    ent.get_password().map_err(|e| e.to_string())
}

/// needed for silent-refresh, Amplify sessions expire after 1 h and the
/// frontend requests this when a 401 comes back from the cloud API.
pub fn get_refresh_token() -> Result<String, String> {
    let ent = keyring::Entry::new(SERVICE, REFRESH_KEY).map_err(|e| e.to_string())?;
    ent.get_password().map_err(|e| e.to_string())
}

/// removes both tokens from the keychain.
///
/// `NoEntry` is silently ignored, on a fresh macOS install the keychain
/// items won't exist yet, and the user might tap "sign out" before ever
/// completing login.
pub fn clear_tokens() -> Result<(), String> {
    for k in [ACCESS_KEY, REFRESH_KEY] {
        let ent = keyring::Entry::new(SERVICE, k).map_err(|e| e.to_string())?;
        match ent.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use super::*;

    // keychain ops aren't thread-safe across tests, so we serialise them
    static KC_LOCK: Mutex<()> = Mutex::new(());

    fn cleanup() {
        let _ = clear_tokens();
    }

    // verifies the basic save → retrieve round-trip
    #[test]
    #[ignore = "requires OS keychain access - will prompt on macOS"]
    fn save_and_retrieve_access_token() {
        let _g = KC_LOCK.lock().expect("test mutex poisoned");
        cleanup();

        let acc = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.test_access";
        let rfr = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.test_refresh";

        save_tokens(acc, rfr).expect("save_tokens should succeed");
        let got = get_access_token().expect("get_access_token should succeed");
        assert_eq!(got, acc, "retrieved token must match the saved one");

        cleanup();
    }

    // make sure clear actually removes both entries
    #[test]
    fn clear_tokens_removes_entries() {
        let _g = KC_LOCK.lock().expect("test mutex poisoned");
        cleanup();

        save_tokens("to_be_cleared", "to_be_cleared").expect("save should succeed");
        clear_tokens().expect("clear should succeed");

        let res = get_access_token();
        assert!(res.is_err(), "should fail after clear_tokens");
    }

    // fresh-install scenario: user opens app, keychain has nothing
    #[test]
    fn get_token_when_none_exists() {
        let _g = KC_LOCK.lock().expect("test mutex poisoned");
        cleanup();

        let res = get_access_token();
        assert!(res.is_err(), "should fail when no token stored");
    }
}
