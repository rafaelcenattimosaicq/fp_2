use rumqttc::{AsyncClient, Event, EventLoop, Incoming, MqttOptions, QoS};
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

fn spawn_broker(port: u16) -> Child {
    let exe = env!("CARGO_BIN_EXE_mqtt-broker");
    Command::new(exe)
        .env("MQTT_BIND_ADDR", "127.0.0.1")
        .env("MQTT_PORT", port.to_string())
        .env("RUST_LOG", "error")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn broker")
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
async fn broker_routes_publish_to_subscriber() {
    let port = free_port();
    let mut child = spawn_broker(port);

    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    assert!(
        wait_for_port(addr, Duration::from_secs(2)),
        "broker didn't listen on {addr}"
    );

    let mut sub_opts = MqttOptions::new("sub", "127.0.0.1", port);
    sub_opts.set_keep_alive(Duration::from_secs(5));
    let (sub_client, mut sub_loop) = AsyncClient::new(sub_opts, 10);

    wait_for_connack(&mut sub_loop).await;
    sub_client
        .subscribe("test/topic", QoS::AtLeastOnce)
        .await
        .expect("subscribe");
    wait_for_suback(&mut sub_loop).await;

    let mut pub_opts = MqttOptions::new("pub", "127.0.0.1", port);
    pub_opts.set_keep_alive(Duration::from_secs(5));
    let (pub_client, mut pub_loop) = AsyncClient::new(pub_opts, 10);
    wait_for_connack(&mut pub_loop).await;

    pub_client
        .publish("test/topic", QoS::AtLeastOnce, false, "hello")
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

    assert_eq!(got.as_deref(), Some("hello"));
}
