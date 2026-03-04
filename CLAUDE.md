# CLAUDE.md

## Project structure
- `services/gateway/` — Rust IoT gateway (Modbus RTU, MQTT, Docker orchestration)
- `cloud-desktop/` — React+Tauri desktop app for monitoring
- `evaluation/` — Python benchmarks
- `infra/` — Terraform modules (AWS)

## Dev commands
- Gateway: `cd services/gateway && cargo build --release`
- Frontend: `cd cloud-desktop && npm install && npm run dev`
- Tests: `cargo test` / `npm test`
