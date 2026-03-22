# AURA - Edge Computing Platform for Secure IoT Device Management

Final project for BSc Computer Science - University of London (CM3070)

## Components

- **Edge Gateway** (`services/gateway/`) — Rust application for Modbus RTU polling, MQTT telemetry, NES worker lifecycle, and VPN provisioning
- **Cloud Desktop** (`cloud-desktop/`) — React/TypeScript dashboard with Tauri v2 backend for device monitoring, query engine, and alert management
- **Lambda APIs** (`services/lambdas/`) — Rust AWS Lambda handlers for devices, firmware, policies, and VPN
- **ESP32 Firmware** (`firmware/`) — Wind turbine motor controller with Hall sensor RPM and Modbus RTU emulation
- **Infrastructure** (`infra/`) — Terraform modules for AWS VPC, ECS Fargate, API Gateway, and NAT/VPN relay

## Third-Party Notice

**NebulaStream** is not part of this project. It is an open-source distributed stream processing engine developed by the [DIMA group at TU Berlin](https://www.2dima.tu-berlin.de/). The original source code is available at [https://github.com/nebulastream](https://github.com/nebulastream). In this project, NebulaStream was compiled from source to build a Docker image used as an edge worker for distributed query processing. All NebulaStream code remains under its original license.

## Releases

Pre-built binaries are available at [Releases](https://github.com/rafaelcenattimosaicq/fp_2/releases).
