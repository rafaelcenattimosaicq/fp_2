#!/bin/bash
set -e

echo "=== AURA Gateway Installer ==="

# Install Docker
if ! command -v docker &>/dev/null; then
    echo "[1/6] Installing Docker..."
    curl -fsSL https://get.docker.com | sh
    sudo usermod -aG docker $USER
else
    echo "[1/6] Docker already installed"
fi

# Install Tailscale
if ! command -v tailscale &>/dev/null; then
    echo "[2/6] Installing Tailscale..."
    curl -fsSL https://tailscale.com/install.sh | sh
else
    echo "[2/6] Tailscale already installed"
fi

# Install GUI deps
echo "[3/6] Installing GUI dependencies..."
sudo apt-get install -y -qq libwayland-client0 libwayland-cursor0 libwayland-egl1 libxkbcommon0 libgl1-mesa-dri libgl1-mesa-glx libx11-6 libxcursor1 libxrandr2 libxi6 2>/dev/null || true

# Download and install gateway
echo "[4/6] Installing AURA Gateway..."
cd /tmp
curl -sL -o gateway.deb "https://github.com/rafaelcenattimosaicq/final_project/releases/download/v0.2.0/gateway_0.1.0-1_arm64.deb"
sudo apt --fix-broken install -y -qq 2>/dev/null || true
sudo dpkg -i gateway.deb

# Write default config
echo "[5/6] Writing default config..."
sudo mkdir -p /etc/aura
sudo tee /etc/aura/gateway.yaml > /dev/null << 'YAML'
gateway_id: "GW-DEMO-001"
serial:
  port: "emulated"
  baud_rate: 9600
  slave_id: 254
mqtt:
  broker_url: "mqtt://mqtt-broker.iot.local:1883"
  topic: "controller_app/events"
  qos: 1
registry:
  url: "http://localhost:8088"
  enrollment_token: "change-me"
devices_api_url: "https://71waxvgo68.execute-api.us-east-1.amazonaws.com"
devices_dir: "./devices"
poll_interval_ms: 5000
vpn:
  provisioner_url: "https://87dzpnwsh9.execute-api.us-east-1.amazonaws.com"
  pre_shared_secret: "4MRVGA3NgERr0wHC05thGhI4dsa@uV2L708tCzekla4"
worker:
  image: "nebulastream/nes-executable-image:latest"
  binary_path: "nesWorker"
  coordinator_host: "nebulastream.iot.local"
  coordinator_port: 8080
  local_worker_host: "0.0.0.0"
  logical_source_name: "telemetry_0x0007"
  physical_source_name: "edge-0x0007-GW-DEMO-001"
  mqtt_broker_url: "tcp://mqtt-broker.iot.local:1883"
  mqtt_topic: "controller_app/events"
  max_schema_fields: 5
YAML

# Symlink so gateway finds config from anywhere
ln -sf /etc/aura/gateway.yaml /usr/local/bin/gateway.yaml 2>/dev/null || true

# Pull NES image in background
echo "[6/6] Pulling NES worker image (background)..."
sudo docker pull nebulastream/nes-executable-image:latest &>/dev/null &

echo ""
echo "=== DONE ==="
echo "Run:  cd /usr/local/bin && sudo gateway"
echo ""
