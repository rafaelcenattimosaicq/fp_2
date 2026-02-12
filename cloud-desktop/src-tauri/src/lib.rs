mod auth;

#[tauri::command]
#[allow(clippy::needless_pass_by_value, reason = "Tauri IPC deserialises into owned String")]
fn greet(name: String) -> String {
    format!("Hello {name} from Rust!")
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value, reason = "Tauri IPC requires owned types")]
fn save_auth_tokens(access_token: String, refresh_token: String) -> Result<(), String> {
    auth::save_tokens(&access_token, &refresh_token)
}


#[tauri::command]
fn get_auth_token() -> Result<String, String> {
    auth::get_access_token()
}

#[tauri::command]
fn get_refresh_token() -> Result<String, String> {
    auth::get_refresh_token()
}

#[tauri::command]
fn clear_auth_tokens() -> Result<(), String> {
    auth::clear_tokens()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
#[allow(clippy::missing_panics_doc, reason = "entry point - panic is intentional if Tauri fails to boot")]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            greet,
            save_auth_tokens,
            get_auth_token,
            get_refresh_token,
            clear_auth_tokens,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    // basic sanity: the greeting template hasn't been accidentally changed
    #[test]
    fn greet_returns_formatted_string() {
        let res = greet("World".to_string());
        assert_eq!(res, "Hello World from Rust!");
    }

    // empty name edge-case, should not panic
    #[test]
    fn greet_handles_empty_name() {
        let res = greet(String::new());
        assert_eq!(res, "Hello  from Rust!");
    }

    // cognito display names in BR locale often contain accented chars
    #[test]
    fn greet_handles_unicode_name() {
        let res = greet("José".to_string());
        assert_eq!(res, "Hello José from Rust!");
    }
}
