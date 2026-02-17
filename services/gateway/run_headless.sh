#!/bin/bash
pkill -f "gateway gateway.yaml" 2>/dev/null
sleep 1
cd /mnt/mac/Users/rafaelrccenatti/Projects/uol_final_project/services/gateway
RUST_LOG=gateway=info nohup ./target/release/gateway gateway.yaml --headless > /tmp/gateway.log 2>&1 &
echo "Gateway PID: $!"
sleep 8
tail -30 /tmp/gateway.log
pgrep -f "gateway gateway.yaml" && echo "Gateway is running" || echo "Gateway NOT running"
