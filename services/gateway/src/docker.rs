use bollard::container::{
    Config, CreateContainerOptions, RemoveContainerOptions, StartContainerOptions,
    StopContainerOptions,
};
use bollard::image::CreateImageOptions;
use bollard::models::HostConfig;
use bollard::Docker;
use futures::StreamExt;
use std::time::Duration;
use thiserror::Error;

use crate::config::GatewayConfig;

const IMAGE_PULL_RETRIES: u32 = 3;
const IMAGE_PULL_RETRY_DELAY: Duration = Duration::from_secs(5);

#[derive(Debug, Error)]
pub enum DockerError {
    #[error("cannot connect to Docker daemon: {0}")]
    Connection(#[from] bollard::errors::Error),

    #[error("failed to pull image '{image}': {reason}")]
    ImagePull {
        image: String,
        reason: String,
    },
}

const CONTAINER_PREFIX: &str = "gateway";

fn mqtt_container_name(gateway_id: &str) -> String {
    format!("{CONTAINER_PREFIX}-{gateway_id}-mqtt")
}

fn registry_container_name(gateway_id: &str) -> String {
    format!("{CONTAINER_PREFIX}-{gateway_id}-registry")
}

fn bridge_container_name(gateway_id: &str) -> String {
    format!("{CONTAINER_PREFIX}-{gateway_id}-bridge")
}

fn rules_container_name(gateway_id: &str) -> String {
    format!("{CONTAINER_PREFIX}-{gateway_id}-rules-engine")
}

pub async fn pull_image(docker: &Docker, image: &str) -> Result<(), DockerError> {
    for idx in 1..=IMAGE_PULL_RETRIES {
        let x = CreateImageOptions {
            from_image: image,
            ..Default::default()
        };

        let mut s = docker.create_image(Some(x), None, None);
        let mut tmp = None;
        while let Some(r) = s.next().await {
            match r {
                Ok(_) => {}
                Err(e) => {
                    tmp = Some(e.to_string());
                }
            }
        }

        if tmp.is_none() {
            return Ok(());
        }

        if idx < IMAGE_PULL_RETRIES {
            tokio::time::sleep(IMAGE_PULL_RETRY_DELAY).await;
        } else {
            return Err(DockerError::ImagePull {
                image: image.to_string(),
                reason: tmp.unwrap_or_else(|| "unknown error".to_string()),
            });
        }
    }
    unreachable!()
}

async fn ensure_container(
    docker: &Docker,
    name: &str,
    image: &str,
    container_config: Config<String>,
) -> Result<String, DockerError> {
    match docker.inspect_container(name, None).await {
        Ok(data) => {
            let ok = data
                .state
                .as_ref()
                .and_then(|s| s.running)
                .unwrap_or(false);
            if ok {
                return Ok(data.id.unwrap_or_default());
            }
            docker
                .remove_container(
                    name,
                    Some(RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await?;
        }
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 404, ..
        }) => {}

        Err(e) => return Err(DockerError::Connection(e)),
    }

    if docker.inspect_image(image).await.is_err() {
        pull_image(docker, image).await?;
    }

    let co = CreateContainerOptions {
        name: name.to_string(),
        ..Default::default()
    };
    let r = docker
        .create_container(Some(co), container_config)
        .await?;
    docker
        .start_container(&r.id, None::<StartContainerOptions<String>>)
        .await?;
    Ok(r.id)
}

pub struct DockerGuard {
    docker: Docker,
    container_names: Vec<String>,
}

impl DockerGuard {
    pub async fn cleanup(&self) {
        for name in &self.container_names {
            let _ = self
                .docker
                .stop_container(name, Some(StopContainerOptions { t: 10 }))
                .await;
            let _ = self
                .docker
                .remove_container(
                    name,
                    Some(RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await;
        }
    }

    pub fn cleanup_blocking(&self) {
        if let Ok(rt) = tokio::runtime::Runtime::new() {
            rt.block_on(self.cleanup());
        }
    }
}

const MQTT_USER: &str = "gateway";

fn generate_mqtt_password() -> String {
    // not cryptographically secure
    use std::time::{SystemTime, UNIX_EPOCH};
    let s = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0u64, |d| {
            #[allow(clippy::cast_possible_truncation, reason = "nanos since epoch will not exceed u64 for centuries")]
            let n = d.as_nanos() as u64;
            n
        });
    let mut v = s;
    (0..32)
        .fold(String::with_capacity(64), |mut buf, _| {
            v = v.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            #[allow(clippy::cast_possible_truncation, reason = "intentionally extracting low 8 bits from shifted u64")]
            let b = (v >> 33) as u8;
            use std::fmt::Write;
            let _ = write!(buf, "{b:02x}");
            buf
        })
}

const DOCKER_CONNECT_RETRIES: u32 = 5;
const DOCKER_CONNECT_DELAY: Duration = Duration::from_secs(2);

#[allow(clippy::too_many_lines)]
pub async fn ensure_containers(
    cfg: &GatewayConfig,
    state: &crate::state::SharedState,
) -> Result<DockerGuard, DockerError> {
    let docker = Docker::connect_with_local_defaults()?;

    let mut last_err = None;
    for _attempt in 1..=DOCKER_CONNECT_RETRIES {
        match docker.ping().await {
            Ok(_) => { last_err = None; break; }
            Err(e) => {
                last_err = Some(e);
                tokio::time::sleep(DOCKER_CONNECT_DELAY).await;
            }
        }
    }
    if let Some(e) = last_err {
        return Err(DockerError::Connection(e));
    }
    let update_status = |state: &crate::state::SharedState,
                          name: &str,
                          status: crate::state::ServiceStatus| {
        let mut s = state.write().unwrap();
        s.set_service_status(name, status.clone());
        s.push_log(crate::state::LogLevel::Info, format!("{name}: {status}"));
        drop(s);
    };

    let gateway_id = &cfg.gateway_id;
    let mut cnames = Vec::new();

    let mqtt_password = generate_mqtt_password();

    let mqtt_name = mqtt_container_name(gateway_id);
    let mqtt_image = &cfg.docker.mqtt_image;

    let mosquitto_conf = "listener 1883 127.0.0.1\n\
         allow_anonymous false\n\
         password_file /mosquitto/config/passwd\n\
         log_dest stdout\n\
         log_type error\n\
         log_type warning\n\
         log_type notice\n".to_string();

    let mqtt_config = Config {
        image: Some(mqtt_image.clone()),
        cmd: Some(vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            format!(
                "echo '{conf}' > /mosquitto/config/mosquitto.conf && \
                 mosquitto_passwd -b -c /mosquitto/config/passwd {user} {pass} && \
                 chmod 644 /mosquitto/config/passwd && \
                 mosquitto -c /mosquitto/config/mosquitto.conf",
                conf = mosquitto_conf.replace('\'', "'\\''"),
                user = MQTT_USER,
                pass = mqtt_password,
            ),
        ]),
        host_config: Some(HostConfig {
            network_mode: Some("host".to_string()),
            ..Default::default()
        }),
        ..Default::default()
    };
    update_status(state, "MQTT broker", crate::state::ServiceStatus::Starting);
    if let Err(e) = ensure_container(&docker, &mqtt_name, mqtt_image, mqtt_config).await {
        update_status(state, "MQTT broker", crate::state::ServiceStatus::Error(e.to_string()));
        return Err(e);
    }
    update_status(state, "MQTT broker", crate::state::ServiceStatus::Running);
    cnames.push(mqtt_name);

    state.write().unwrap().mqtt_credentials = Some((MQTT_USER.to_string(), mqtt_password.clone()));

    tokio::time::sleep(Duration::from_secs(2)).await;

    let registry_name = registry_container_name(gateway_id);
    let registry_image = &cfg.docker.registry_image;
    update_status(state, "Registry", crate::state::ServiceStatus::Pulling);
    if let Err(e) = pull_image(&docker, registry_image).await {
        update_status(state, "Registry", crate::state::ServiceStatus::Error(e.to_string()));
        return Err(e);
    }

    let registry_config = Config {
        image: Some(registry_image.clone()),
        host_config: Some(HostConfig {
            network_mode: Some("host".to_string()),
            ..Default::default()
        }),
        env: Some(vec![
            "REGISTRY_BIND_ADDR=0.0.0.0".to_string(),
            "REGISTRY_PORT=8088".to_string(),
            "REGISTRY_BACKEND=sqlite".to_string(),
            "REGISTRY_SQLITE_PATH=/tmp/registry.db".to_string(),
            "REGISTRY_MQTT_ENABLED=true".to_string(),
            "MQTT_HOST=127.0.0.1".to_string(),
            "MQTT_PORT=1883".to_string(),
            format!("MQTT_USERNAME={MQTT_USER}"),
            format!("MQTT_PASSWORD={mqtt_password}"),
        ]),
        ..Default::default()
    };
    update_status(state, "Registry", crate::state::ServiceStatus::Starting);
    if let Err(e) = ensure_container(&docker, &registry_name, registry_image, registry_config).await {
        update_status(state, "Registry", crate::state::ServiceStatus::Error(e.to_string()));
        return Err(e);
    }
    update_status(state, "Registry", crate::state::ServiceStatus::Running);
    cnames.push(registry_name);

    let rules_name = rules_container_name(gateway_id);
    let rules_image = &cfg.docker.rules_engine_image;
    update_status(state, "Rules engine", crate::state::ServiceStatus::Pulling);
    if let Err(e) = pull_image(&docker, rules_image).await {
        update_status(state, "Rules engine", crate::state::ServiceStatus::Error(e.to_string()));
        return Err(e);
    }

    let rules_config = Config {
        image: Some(rules_image.clone()),
        host_config: Some(HostConfig {
            network_mode: Some("host".to_string()),
            ..Default::default()
        }),
        env: Some(vec![
            "MQTT_HOST=127.0.0.1".to_string(),
            "MQTT_PORT=1883".to_string(),
            format!("MQTT_USERNAME={MQTT_USER}"),
            format!("MQTT_PASSWORD={mqtt_password}"),
            "POLICY_TOPIC_FILTER=policies/#".to_string(),
            "TELEMETRY_TOPIC_FILTER=telemetry/#".to_string(),
        ]),
        ..Default::default()
    };
    update_status(state, "Rules engine", crate::state::ServiceStatus::Starting);
    if let Err(e) = ensure_container(&docker, &rules_name, rules_image, rules_config).await {
        update_status(state, "Rules engine", crate::state::ServiceStatus::Error(e.to_string()));
        return Err(e);
    }
    update_status(state, "Rules engine", crate::state::ServiceStatus::Running);
    cnames.push(rules_name);

    let bridge_name = bridge_container_name(gateway_id);
    let bridge_image = &cfg.docker.rules_engine_image;

    // check if the bridge image is already available
    let _bridge_image_present = docker.inspect_image(bridge_image).await.is_ok();

    let bridge_env = vec![
        "MQTT_HOST=127.0.0.1".to_string(),
        "MQTT_PORT=1883".to_string(),
        format!("MQTT_USERNAME={MQTT_USER}"),
        format!("MQTT_PASSWORD={mqtt_password}"),
        "MQTT_TOPIC_FILTER=controller_app/events".to_string(),
        "TCP_BIND_ADDR=0.0.0.0".to_string(),
        "TCP_PORT=50501".to_string(),
    ];
    let bridge_config = Config {
        image: Some(bridge_image.clone()),
        cmd: Some(vec!["mqtt_to_tcp_bridge".to_string()]),
        host_config: Some(HostConfig {
            network_mode: Some("host".to_string()),
            ..Default::default()
        }),
        env: Some(bridge_env),
        ..Default::default()
    };
    update_status(state, "MQTT-to-TCP bridge", crate::state::ServiceStatus::Starting);
    if let Err(e) = ensure_container(&docker, &bridge_name, bridge_image, bridge_config).await {
        update_status(state, "MQTT-to-TCP bridge", crate::state::ServiceStatus::Error(e.to_string()));
        return Err(e);
    }
    update_status(state, "MQTT-to-TCP bridge", crate::state::ServiceStatus::Running);
    cnames.push(bridge_name);

    if let Some(worker_cfg) = &cfg.worker {
        let worker_image = &worker_cfg.image;
        update_status(state, "NES worker", crate::state::ServiceStatus::Pulling);
        if let Err(e) = pull_image(&docker, worker_image).await {
            update_status(state, "NES worker", crate::state::ServiceStatus::Error(e.to_string()));
        } else {
            update_status(state, "NES worker", crate::state::ServiceStatus::Pending);
        }
    }

    Ok(DockerGuard {
        docker,
        container_names: cnames,
    })
}

const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(30);

pub async fn health_check_loop(
    cfg: GatewayConfig,
    _state: crate::state::SharedState,
) {
    let docker = match Docker::connect_with_local_defaults() {
        Ok(d) => d,
        Err(_e) => {
            return;
        }
    };

    let gateway_id = &cfg.gateway_id;

    let names = vec![
        mqtt_container_name(gateway_id),
        registry_container_name(gateway_id),
        rules_container_name(gateway_id),
        bridge_container_name(gateway_id),
    ];

    loop {
        tokio::time::sleep(HEALTH_CHECK_INTERVAL).await;

        for container_name in &names {
            // here fixme: should actually restart the container not just log, RC
            let _ = check_container(&docker, container_name).await;
        }
    }
}

async fn check_container(docker: &Docker, name: &str) -> Result<(), DockerError> {
    match docker.inspect_container(name, None).await {
        Ok(_info) => {
            Ok(())
        }
        Err(bollard::errors::Error::DockerResponseServerError { status_code: 404, .. }) => {
            Ok(())
        }
        Err(e) => Err(DockerError::Connection(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_names_include_gateway_id() {
        let mqtt = mqtt_container_name("GW-EDGE-001");
        let registry = registry_container_name("GW-EDGE-001");
        let bridge = bridge_container_name("GW-EDGE-001");
        let rules = rules_container_name("GW-EDGE-001");

        assert_eq!(mqtt, "gateway-GW-EDGE-001-mqtt");
        assert_eq!(registry, "gateway-GW-EDGE-001-registry");
        assert_eq!(bridge, "gateway-GW-EDGE-001-bridge");
        assert_eq!(rules, "gateway-GW-EDGE-001-rules-engine");
    }

    async fn force_remove(docker: &Docker, name: &str) {
        let _ = docker
            .stop_container(name, Some(StopContainerOptions { t: 0 }))
            .await;
        let _ = docker
            .remove_container(
                name,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await;
    }

    async fn assert_running(docker: &Docker, name: &str) {
        let info = docker
            .inspect_container(name, None)
            .await
            .expect("container should exist");
        let running = info
            .state
            .as_ref()
            .and_then(|s| s.running)
            .unwrap_or(false);
        assert!(running, "container '{name}' should be running");
    }

    fn sleeping_container_config() -> Config<String> {
        Config {
            image: Some("alpine:latest".to_string()),
            cmd: Some(vec![
                "sleep".to_string(),
                "3600".to_string(),
            ]),
            ..Default::default()
        }
    }

    #[tokio::test]
    #[ignore]
    async fn ensure_container_starts_and_cleanup_removes() {
        let docker = Docker::connect_with_local_defaults().unwrap();
        let name = "gateway-test-start-001";
        force_remove(&docker, name).await;

        let id = ensure_container(&docker, name, "alpine:latest", sleeping_container_config())
            .await
            .expect("ensure_container should succeed");
        assert!(!id.is_empty());
        assert_running(&docker, name).await;

        let guard = DockerGuard {
            docker: Docker::connect_with_local_defaults().unwrap(),
            container_names: vec![name.to_string()],
        };
        guard.cleanup().await;

        let result = docker.inspect_container(name, None).await;
        assert!(result.is_err(), "container should not exist after cleanup");
    }

    #[tokio::test]
    #[ignore]
    async fn ensure_container_is_idempotent() {
        let docker = Docker::connect_with_local_defaults().unwrap();
        let name = "gateway-test-idempotent-001";
        force_remove(&docker, name).await;

        let id1 = ensure_container(&docker, name, "alpine:latest", sleeping_container_config())
            .await
            .expect("first call should succeed");
        let id2 = ensure_container(&docker, name, "alpine:latest", sleeping_container_config())
            .await
            .expect("second call should succeed (idempotent)");

        assert_eq!(id1, id2, "should reuse the same container");
        assert_running(&docker, name).await;

        force_remove(&docker, name).await;
    }

    #[tokio::test]
    #[ignore]
    async fn ensure_container_recreates_stopped() {
        let docker = Docker::connect_with_local_defaults().unwrap();
        let name = "gateway-test-recreate-001";
        force_remove(&docker, name).await;

        let id1 = ensure_container(&docker, name, "alpine:latest", sleeping_container_config())
            .await
            .expect("initial create should succeed");
        docker
            .stop_container(name, Some(StopContainerOptions { t: 0 }))
            .await
            .expect("manual stop should succeed");

        let id2 = ensure_container(&docker, name, "alpine:latest", sleeping_container_config())
            .await
            .expect("should recreate stopped container");

        assert_ne!(id1, id2);
        assert_running(&docker, name).await;

        force_remove(&docker, name).await;
    }

    #[tokio::test]
    #[ignore]
    async fn cleanup_removes_all_containers() {
        let docker = Docker::connect_with_local_defaults().unwrap();
        let name = "gateway-test-cleanup-001";
        force_remove(&docker, name).await;

        ensure_container(&docker, name, "alpine:latest", sleeping_container_config())
            .await
            .expect("create should succeed");

        let guard = DockerGuard {
            docker: Docker::connect_with_local_defaults().unwrap(),
            container_names: vec![name.to_string()],
        };
        guard.cleanup().await;

        let result = docker.inspect_container(name, None).await;
        assert!(result.is_err(), "container should not exist after cleanup");
    }

    #[tokio::test]
    #[ignore]
    async fn pull_image_succeeds_for_valid_image() {
        let docker = Docker::connect_with_local_defaults().unwrap();
        let result = pull_image(&docker, "alpine:latest").await;
        assert!(result.is_ok());
    }
}
