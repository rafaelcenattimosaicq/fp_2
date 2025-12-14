# mqtt-broker

MQTT broker service (TCP/1883) built on `rumqttd`.

## mTLS quickstart

- Generate local dev certs: `./services/scripts/gen-mtls-certs.sh`
- Rotate local dev certs (backs up previous): `./services/scripts/rotate-mtls-certs.sh`
- Run: `docker compose -f services/docker-compose.mtls.yml up --build`

## Environment variables

- `MQTT_BIND_ADDR` (default `0.0.0.0`)
- `MQTT_PORT` (default `1883`)
- `MQTT_ENABLE_PLAINTEXT` (default `true`)
- `MQTT_TLS_BIND_ADDR` (default `MQTT_BIND_ADDR`)
- `MQTT_TLS_PORT` (default `8883`)
- `MQTT_TLS_CA_CERT_PATH` (required for mTLS)
- `MQTT_TLS_CERT_PATH` (required for TLS)
- `MQTT_TLS_KEY_PATH` (required for TLS)
- `RUST_LOG` (default `mqtt_broker=info`)
