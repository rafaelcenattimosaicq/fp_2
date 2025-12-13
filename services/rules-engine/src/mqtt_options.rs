use rumqttc::{MqttOptions, TlsConfiguration, Transport};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsPaths {
    pub ca_cert_path: String,
    pub client_cert_path: String,
    pub client_key_path: String,
}

// reads tls cert paths from env vars, returns None if none are set
pub fn tls_paths_from_env_lookup(
    mut get: impl FnMut(&str) -> Option<String>,
) -> Result<Option<TlsPaths>, String> {
    let x = get("MQTT_TLS_CA_CERT_PATH");
    let y = get("MQTT_TLS_CLIENT_CERT_PATH");
    let z = get("MQTT_TLS_CLIENT_KEY_PATH");

    let ok = x.is_some() || y.is_some() || z.is_some();
    if !ok {
        return Ok(None);
    }

    let ca_cert_path = x.ok_or_else(|| "MQTT_TLS_CA_CERT_PATH required when enabling TLS".to_string())?;
    let client_cert_path =
        y.ok_or_else(|| "MQTT_TLS_CLIENT_CERT_PATH required when enabling TLS".to_string())?;
    let client_key_path =
        z.ok_or_else(|| "MQTT_TLS_CLIENT_KEY_PATH required when enabling TLS".to_string())?;

    Ok(Some(TlsPaths { ca_cert_path, client_cert_path, client_key_path }))
}

pub fn apply_mtls_transport_from_files(options: &mut MqttOptions, paths: &TlsPaths) -> Result<(), String> {
    // not sure if theres a better way to do this with rumqttc
    let buf1 = std::fs::read(&paths.ca_cert_path)
        .map_err(|e| format!("failed reading MQTT_TLS_CA_CERT_PATH: {e}"))?;
    let buf2 = std::fs::read(&paths.client_cert_path)
        .map_err(|e| format!("failed reading MQTT_TLS_CLIENT_CERT_PATH: {e}"))?;
    let buf3 = std::fs::read(&paths.client_key_path)
        .map_err(|e| format!("failed reading MQTT_TLS_CLIENT_KEY_PATH: {e}"))?;

    options.set_transport(Transport::Tls(TlsConfiguration::Simple {
        ca: buf1,
        alpn: None,
        client_auth: Some((buf2, buf3)),
    }));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn tls_paths_none_when_env_missing() {
        let env: HashMap<String, String> = HashMap::new();
        let parsed = tls_paths_from_env_lookup(|k| env.get(k).cloned()).expect("ok");
        assert_eq!(parsed, None);
    }

    #[test]
    fn tls_paths_errors_on_partial_env() {
        let env: HashMap<String, String> =
            [("MQTT_TLS_CA_CERT_PATH".to_string(), "/tmp/ca.pem".to_string())]
                .into_iter()
                .collect();
        let err = tls_paths_from_env_lookup(|k| env.get(k).cloned()).unwrap_err();
        assert!(err.contains("MQTT_TLS_CLIENT_CERT_PATH"));
    }

    #[test]
    fn apply_mtls_transport_reads_files_and_sets_transport() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ca_path = dir.path().join("ca.pem");
        let cert_path = dir.path().join("client.pem");
        let key_path = dir.path().join("client.key");

        std::fs::write(&ca_path, b"CA").expect("write ca");
        std::fs::write(&cert_path, b"CERT").expect("write cert");
        std::fs::write(&key_path, b"KEY").expect("write key");

        let paths = TlsPaths {
            ca_cert_path: ca_path.to_string_lossy().to_string(),
            client_cert_path: cert_path.to_string_lossy().to_string(),
            client_key_path: key_path.to_string_lossy().to_string(),
        };

        let mut opts = MqttOptions::new("id", "localhost", 1883);
        apply_mtls_transport_from_files(&mut opts, &paths).expect("apply");

        match opts.transport() {
            Transport::Tls(TlsConfiguration::Simple {
                ca,
                alpn,
                client_auth,
            }) => {
                assert_eq!(ca, b"CA");
                assert_eq!(alpn, None);
                let (cert, key) = client_auth.expect("client auth");
                assert_eq!(cert, b"CERT");
                assert_eq!(key, b"KEY");
            }
            _ => panic!("unexpected transport variant"),
        }
    }

    #[test]
    fn tls_paths_errors_when_only_cert_provided() {
        let env: HashMap<String, String> =
            [("MQTT_TLS_CLIENT_CERT_PATH".to_string(), "/tmp/cert.pem".to_string())]
                .into_iter()
                .collect();
        let err = tls_paths_from_env_lookup(|k| env.get(k).cloned()).unwrap_err();
        assert!(
            err.contains("MQTT_TLS_CA_CERT_PATH"),
            "should complain about missing CA cert"
        );
    }

    #[test]
    fn tls_paths_errors_when_only_key_provided() {
        let env: HashMap<String, String> =
            [("MQTT_TLS_CLIENT_KEY_PATH".to_string(), "/tmp/key.pem".to_string())]
                .into_iter()
                .collect();
        let err = tls_paths_from_env_lookup(|k| env.get(k).cloned()).unwrap_err();
        assert!(
            err.contains("MQTT_TLS_CA_CERT_PATH"),
            "should complain about missing CA cert"
        );
    }

    #[test]
    fn tls_paths_errors_when_key_missing() {
        let env: HashMap<String, String> = [
            ("MQTT_TLS_CA_CERT_PATH".to_string(), "/tmp/ca.pem".to_string()),
            ("MQTT_TLS_CLIENT_CERT_PATH".to_string(), "/tmp/cert.pem".to_string()),
        ]
        .into_iter()
        .collect();
        let err = tls_paths_from_env_lookup(|k| env.get(k).cloned()).unwrap_err();
        assert!(
            err.contains("MQTT_TLS_CLIENT_KEY_PATH"),
            "should complain about missing client key"
        );
    }

    #[test]
    fn tls_paths_ok_when_all_provided() {
        let env: HashMap<String, String> = [
            ("MQTT_TLS_CA_CERT_PATH".to_string(), "/certs/ca.pem".to_string()),
            ("MQTT_TLS_CLIENT_CERT_PATH".to_string(), "/certs/client.pem".to_string()),
            ("MQTT_TLS_CLIENT_KEY_PATH".to_string(), "/certs/client.key".to_string()),
        ]
        .into_iter()
        .collect();
        let result = tls_paths_from_env_lookup(|k| env.get(k).cloned())
            .expect("should succeed")
            .expect("should be Some");
        assert_eq!(result.ca_cert_path, "/certs/ca.pem");
        assert_eq!(result.client_cert_path, "/certs/client.pem");
        assert_eq!(result.client_key_path, "/certs/client.key");
    }

    #[test]
    fn apply_mtls_errors_on_missing_ca_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cert_path = dir.path().join("client.pem");
        let key_path = dir.path().join("client.key");
        std::fs::write(&cert_path, b"CERT").expect("write cert");
        std::fs::write(&key_path, b"KEY").expect("write key");

        let paths = TlsPaths {
            ca_cert_path: dir.path().join("nonexistent.pem").to_string_lossy().to_string(),
            client_cert_path: cert_path.to_string_lossy().to_string(),
            client_key_path: key_path.to_string_lossy().to_string(),
        };

        let mut opts = MqttOptions::new("id", "localhost", 1883);
        let err = apply_mtls_transport_from_files(&mut opts, &paths).unwrap_err();
        assert!(
            err.contains("MQTT_TLS_CA_CERT_PATH"),
            "error should mention the CA cert env var name"
        );
    }

    #[test]
    fn apply_mtls_errors_on_missing_cert_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ca_path = dir.path().join("ca.pem");
        let key_path = dir.path().join("client.key");
        std::fs::write(&ca_path, b"CA").expect("write ca");
        std::fs::write(&key_path, b"KEY").expect("write key");

        let paths = TlsPaths {
            ca_cert_path: ca_path.to_string_lossy().to_string(),
            client_cert_path: dir.path().join("missing.pem").to_string_lossy().to_string(),
            client_key_path: key_path.to_string_lossy().to_string(),
        };

        let mut opts = MqttOptions::new("id", "localhost", 1883);
        let err = apply_mtls_transport_from_files(&mut opts, &paths).unwrap_err();
        assert!(
            err.contains("MQTT_TLS_CLIENT_CERT_PATH"),
            "error should mention the client cert env var name"
        );
    }

    #[test]
    fn apply_mtls_errors_on_missing_key_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ca_path = dir.path().join("ca.pem");
        let cert_path = dir.path().join("client.pem");
        std::fs::write(&ca_path, b"CA").expect("write ca");
        std::fs::write(&cert_path, b"CERT").expect("write cert");

        let paths = TlsPaths {
            ca_cert_path: ca_path.to_string_lossy().to_string(),
            client_cert_path: cert_path.to_string_lossy().to_string(),
            client_key_path: dir.path().join("missing.key").to_string_lossy().to_string(),
        };

        let mut opts = MqttOptions::new("id", "localhost", 1883);
        let err = apply_mtls_transport_from_files(&mut opts, &paths).unwrap_err();
        assert!(
            err.contains("MQTT_TLS_CLIENT_KEY_PATH"),
            "error should mention the client key env var name"
        );
    }

    #[test]
    fn tls_paths_equality() {
        let a = TlsPaths {
            ca_cert_path: "/a".to_string(),
            client_cert_path: "/b".to_string(),
            client_key_path: "/c".to_string(),
        };
        let b = a.clone();
        assert_eq!(a, b, "cloned TlsPaths should be equal");
    }

    #[test]
    fn tls_paths_debug_contains_fields() {
        let paths = TlsPaths {
            ca_cert_path: "/ca".to_string(),
            client_cert_path: "/cert".to_string(),
            client_key_path: "/key".to_string(),
        };
        let debug = format!("{paths:?}");
        assert!(debug.contains("/ca"), "debug output should contain ca path");
        assert!(debug.contains("/cert"), "debug output should contain cert path");
        assert!(debug.contains("/key"), "debug output should contain key path");
    }
}
