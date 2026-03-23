# AURA - Edge Computing Platform for Secure IoT Device Management

Final project for BSc Computer Science - University of London (CM3070)

**Live Demo:** https://d38pg7j6ywtmmc.cloudfront.net/

> **Note:** The live demo runs with simulated gateways and devices for evaluation purposes. Two virtual wind turbines (0x0008, 0x0009) publish telemetry every 5 seconds via an ECS Fargate task. In production, real Raspberry Pi gateways connect to physical Modbus devices.

## Components

- **Edge Gateway** (`services/gateway/`) — Rust application for Modbus RTU polling, MQTT telemetry, NES worker lifecycle, and VPN provisioning
- **Cloud Desktop** (`cloud-desktop/`) — React/TypeScript dashboard with Tauri v2 backend for device monitoring, query engine, and alert management
- **Lambda APIs** (`services/lambdas/`) — Rust AWS Lambda handlers for devices, firmware, policies, and VPN
- **ESP32 Firmware** (`firmware/`) — Wind turbine motor controller with Hall sensor RPM and Modbus RTU emulation
- **Infrastructure** (`infra/`) — Terraform modules for AWS VPC, ECS Fargate, API Gateway, and NAT/VPN relay

## Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [Node.js](https://nodejs.org/) v18+
- [Docker](https://www.docker.com/products/docker-desktop/)
- [Tailscale](https://tailscale.com/download)
- Tauri v2 prerequisites: [macOS](https://v2.tauri.app/start/prerequisites/#macos) | [Linux](https://v2.tauri.app/start/prerequisites/#linux)

## Development Setup

### Cloud Desktop

```bash
cd cloud-desktop
cp .env.example .env
npm install
npm run tauri dev
```

### Edge Gateway

```bash
cd services/gateway
cargo run
```

The gateway requires a `gateway.yaml` config file in the working directory. An example:

```yaml
gateway_id: "GW-DEV-001"
serial:
  port: "emulated"
  baud_rate: 9600
  slave_id: 254
mqtt:
  broker_url: "mqtt://localhost:1883"
  topic: "controller_app/events"
  qos: 1
registry:
  url: "http://localhost:8088"
  enrollment_token: "change-me"
devices_api_url: "https://71waxvgo68.execute-api.us-east-1.amazonaws.com"
devices_dir: "./devices"
poll_interval_ms: 5000
```

Use `port: "emulated"` for development without hardware.

### Lambda APIs

```bash
cd services/lambdas
cargo build --release
```

### ESP32 Firmware

Open `firmware/modbus-simulator/modbus-simulator.ino` in Arduino IDE with ESP32 board support installed. Upload to ESP32 via USB.

## Running Tests

```bash
# Gateway (Rust)
cd services/gateway && cargo test

# Cloud Desktop (TypeScript)
cd cloud-desktop && npm test

# Lambda APIs
cd services/lambdas && cargo test
```

## Releases

Pre-built binaries are available at [Releases](https://github.com/rafaelcenattimosaicq/fp_2/releases).

## Third-Party Notice

**NebulaStream** is not part of this project. It is an open-source distributed stream processing engine developed by the [DIMA group at TU Berlin](https://www.2dima.tu-berlin.de/). The original source code is available at [https://github.com/nebulastream](https://github.com/nebulastream). In this project, NebulaStream was compiled from source to build a Docker image used as an edge worker for distributed query processing. All NebulaStream code remains under its original license.
