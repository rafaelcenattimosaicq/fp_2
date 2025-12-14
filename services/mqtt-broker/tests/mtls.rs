use rumqttc::{
    AsyncClient, Event, EventLoop, Incoming, MqttOptions, QoS, TlsConfiguration, Transport,
};
use std::net::{SocketAddr, TcpStream};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("local addr")
        .port()
}

fn wait_for_port(addr: SocketAddr, timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(50)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

fn spawn_broker_mtls(
    port: u16,
    ca_path: &std::path::Path,
    cert_path: &std::path::Path,
    key_path: &std::path::Path,
) -> Child {
    let exe = env!("CARGO_BIN_EXE_mqtt-broker");
    Command::new(exe)
        .env("MQTT_ENABLE_PLAINTEXT", "false")
        .env("MQTT_TLS_BIND_ADDR", "127.0.0.1")
        .env("MQTT_TLS_PORT", port.to_string())
        .env("MQTT_TLS_CA_CERT_PATH", ca_path)
        .env("MQTT_TLS_CERT_PATH", cert_path)
        .env("MQTT_TLS_KEY_PATH", key_path)
        .env("RUST_LOG", "error")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn broker")
}

fn write_pem(path: &std::path::Path, pem: &str) {
    std::fs::write(path, pem.as_bytes()).expect("write pem");
}

fn gen_ca() -> (rcgen::Certificate, rcgen::KeyPair) {
    let mut params = rcgen::CertificateParams::new(Vec::<String>::new()).expect("ca params");
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    params.key_usages = vec![
        rcgen::KeyUsagePurpose::KeyCertSign,
        rcgen::KeyUsagePurpose::DigitalSignature,
        rcgen::KeyUsagePurpose::CrlSign,
    ];
    let key_pair = rcgen::KeyPair::generate().expect("ca key");
    let cert = params.self_signed(&key_pair).expect("ca cert");
    (cert, key_pair)
}

fn gen_leaf(
    dns_names: Vec<String>,
    is_client: bool,
    ca_cert: &rcgen::Certificate,
    ca_key: &rcgen::KeyPair,
) -> (rcgen::Certificate, rcgen::KeyPair) {
    let mut params = rcgen::CertificateParams::new(dns_names).expect("leaf params");
    params.key_usages = vec![
        rcgen::KeyUsagePurpose::DigitalSignature,
        rcgen::KeyUsagePurpose::KeyEncipherment,
    ];
    params.extended_key_usages = if is_client {
        vec![rcgen::ExtendedKeyUsagePurpose::ClientAuth]
    } else {
        vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth]
    };
    params
        .distinguished_name
        .push(rcgen::DnType::OrganizationName, "tenant1");
    let key_pair = rcgen::KeyPair::generate().expect("leaf key");
    let cert = params
        .signed_by(&key_pair, ca_cert, ca_key)
        .expect("leaf cert");
    (cert, key_pair)
}

async fn wait_for_connack(eventloop: &mut EventLoop) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        if tokio::time::Instant::now() > deadline {
            panic!("timed out waiting for ConnAck");
        }
        match eventloop.poll().await {
            Ok(Event::Incoming(Incoming::ConnAck(_))) => return,
            Ok(_) => {}
            Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
        }
    }
}

async fn wait_for_suback(eventloop: &mut EventLoop) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        if tokio::time::Instant::now() > deadline {
            panic!("timed out waiting for SubAck");
        }
        match eventloop.poll().await {
            Ok(Event::Incoming(Incoming::SubAck(_))) => return,
            Ok(_) => {}
            Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn broker_accepts_only_mtls_clients() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (ca_cert, ca_key) = gen_ca();
    let (server_cert, server_key) =
        gen_leaf(vec!["localhost".to_string()], false, &ca_cert, &ca_key);
    let (client_cert, client_key) = gen_leaf(vec!["client".to_string()], true, &ca_cert, &ca_key);

    let ca_pem = ca_cert.pem();
    let server_cert_pem = server_cert.pem();
    let server_key_pem = server_key.serialize_pem();
    let client_cert_pem = client_cert.pem();
    let client_key_pem = client_key.serialize_pem();

    let ca_path = dir.path().join("ca.pem");
    let server_cert_path = dir.path().join("server.pem");
    let server_key_path = dir.path().join("server.key");
    let client_cert_path = dir.path().join("client.pem");
    let client_key_path = dir.path().join("client.key");

    write_pem(&ca_path, &ca_pem);
    write_pem(&server_cert_path, &server_cert_pem);
    write_pem(&server_key_path, &server_key_pem);
    write_pem(&client_cert_path, &client_cert_pem);
    write_pem(&client_key_path, &client_key_pem);

    let port = free_port();
    let mut child = spawn_broker_mtls(port, &ca_path, &server_cert_path, &server_key_path);
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    assert!(
        wait_for_port(addr, Duration::from_secs(2)),
        "broker didn't listen on {addr}"
    );

    let ca_bytes = std::fs::read(&ca_path).expect("read ca");
    let client_cert_bytes = std::fs::read(&client_cert_path).expect("read client cert");
    let client_key_bytes = std::fs::read(&client_key_path).expect("read client key");

    let mut sub_opts = MqttOptions::new("sub-mtls", "localhost", port);
    sub_opts.set_keep_alive(Duration::from_secs(5));
    sub_opts.set_transport(Transport::Tls(TlsConfiguration::Simple {
        ca: ca_bytes.clone(),
        alpn: None,
        client_auth: Some((client_cert_bytes.clone(), client_key_bytes.clone())),
    }));

    let (sub_client, mut sub_loop) = AsyncClient::new(sub_opts, 10);
    wait_for_connack(&mut sub_loop).await;
    sub_client
        .subscribe("test/topic", QoS::AtLeastOnce)
        .await
        .expect("subscribe");
    wait_for_suback(&mut sub_loop).await;

    let mut pub_opts = MqttOptions::new("pub-mtls", "localhost", port);
    pub_opts.set_keep_alive(Duration::from_secs(5));
    pub_opts.set_transport(Transport::Tls(TlsConfiguration::Simple {
        ca: ca_bytes,
        alpn: None,
        client_auth: Some((client_cert_bytes, client_key_bytes)),
    }));

    let (pub_client, mut pub_loop) = AsyncClient::new(pub_opts, 10);
    wait_for_connack(&mut pub_loop).await;
    pub_client
        .publish("test/topic", QoS::AtLeastOnce, false, "hello-mtls")
        .await
        .expect("publish");

    let mut got = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);

    loop {
        if tokio::time::Instant::now() > deadline {
            break;
        }

        tokio::select! {
            _ = pub_loop.poll() => {}
            event = sub_loop.poll() => {
                if let Ok(Event::Incoming(Incoming::Publish(p))) = event {
                    got = Some(String::from_utf8_lossy(&p.payload).to_string());
                    break;
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(10)) => {}
        }
    }

    let _ = child.kill();
    let _ = child.wait();

    assert_eq!(got.as_deref(), Some("hello-mtls"));
}
