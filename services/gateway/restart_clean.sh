#!/bin/bash
set -e

pkill -f "gateway gateway.yaml" 2>/dev/null || true
sleep 1

pkill -x mosquitto 2>/dev/null || true
sleep 2

printf "listener 1883 0.0.0.0\nallow_anonymous true\nmax_inflight_messages 20\n" > /tmp/mosquitto.conf
mosquitto -d -c /tmp/mosquitto.conf -v
sleep 1

timeout 2 bash -c 'echo -n "" | nc -w1 localhost 1883 && echo "TCP OK"' 2>/dev/null || echo "TCP connect test skipped"

cd /mnt/mac/Users/rafaelrccenatti/Projects/uol_final_project/services/gateway
RUST_LOG=gateway=debug nohup ./target/release/gateway gateway.yaml --headless > /tmp/gateway.log 2>&1 &
echo "Gateway PID: $!"

sleep 15

head -80 /tmp/gateway.log

echo ""
pgrep -f "gateway gateway.yaml" && echo "Gateway is running" || echo "Gateway NOT running"
pgrep -x mosquitto && echo "Mosquitto is running" || echo "Mosquitto NOT running"
