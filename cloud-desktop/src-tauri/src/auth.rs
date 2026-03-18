
const SERVICE: &str = "cloud-desktop";
const ACCESS_KEY: &str = "access_token";
const REFRESH_KEY: &str = "refresh_token";

pub fn save_tokens(access: &str, refresh: &str) -> Result<(), String> {
    let acc_entry = keyring::Entry::new(SERVICE, ACCESS_KEY).map_err(|e| e.to_string())?;
    acc_entry.set_password(access).map_err(|e| e.to_string())?;

    let ref_entry = keyring::Entry::new(SERVICE, REFRESH_KEY).map_err(|e| e.to_string())?;
    if let Err(e) = ref_entry.set_password(refresh) {
        let _ = acc_entry.delete_credential();
        return Err(e.to_string());
    }

    Ok(())
}

pub fn get_access_token() -> Result<String, String> {
    let ent = keyring::Entry::new(SERVICE, ACCESS_KEY).map_err(|e| e.to_string())?;
    ent.get_password().map_err(|e| e.to_string())
}

pub fn get_refresh_token() -> Result<String, String> {
    let ent = keyring::Entry::new(SERVICE, REFRESH_KEY).map_err(|e| e.to_string())?;
    ent.get_password().map_err(|e| e.to_string())
}


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

    static KC_LOCK: Mutex<()> = Mutex::new(());

    fn cleanup() {
        let _ = clear_tokens();
    }

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

    #[test]
    fn clear_tokens_removes_entries() {
        let _g = KC_LOCK.lock().expect("test mutex poisoned");
        cleanup();

        save_tokens("to_be_cleared", "to_be_cleared").expect("save should succeed");
        clear_tokens().expect("clear should succeed");

        let res = get_access_token();
        assert!(res.is_err(), "should fail after clear_tokens");
    }

    #[test]
    fn get_token_when_none_exists() {
        let _g = KC_LOCK.lock().expect("test mutex poisoned");
        cleanup();

        let res = get_access_token();
        assert!(res.is_err(), "should fail when no token stored");
    }
}
