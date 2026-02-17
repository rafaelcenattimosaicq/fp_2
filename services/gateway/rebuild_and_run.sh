#!/bin/bash
set -e

pkill -f "gateway gateway.yaml" 2>/dev/null || true
sleep 1

cd /mnt/mac/Users/rafaelrccenatti/Projects/uol_final_project/services/gateway
cargo build --release 2>&1 | tail -5

RUST_LOG=gateway=info nohup ./target/release/gateway gateway.yaml --headless > /tmp/gateway.log 2>&1 &
echo "Gateway PID: $!"

sleep 20

cat /tmp/gateway.log

echo ""
pgrep -f "gateway gateway.yaml" && echo "Gateway is running" || echo "Gateway NOT running"
