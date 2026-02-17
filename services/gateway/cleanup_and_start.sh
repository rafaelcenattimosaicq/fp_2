#!/bin/bash
set -e

pkill -f "gateway.*headless" 2>/dev/null || true
pkill -f "gateway gateway.yaml" 2>/dev/null || true
sleep 2

docker ps -a --filter name=nes-worker -q | xargs -r docker rm -f 2>/dev/null || true
sleep 1

ps aux | grep gateway | grep -v grep && echo "WARN: gateway still running" || echo "OK: no gateway processes"
docker ps --filter name=nes-worker -q | head -1 | grep -q . && echo "WARN: worker still running" || echo "OK: no worker containers"
ss -tlnp | grep -E "4000[01]" && echo "WARN: ports still in use" || echo "OK: ports free"

echo ""
cd /mnt/mac/Users/rafaelrccenatti/Projects/uol_final_project/services/gateway
RUST_LOG=gateway=info nohup ./target/release/gateway gateway.yaml --headless > /tmp/gateway.log 2>&1 &
echo "Gateway PID: $!"

sleep 45

echo ""
cat /tmp/gateway.log

echo ""
docker ps --filter name=nes-worker

echo ""
curl -s http://nes-coordinator.iot.local:8081/v1/nes/topology 2>/dev/null | python3 -m json.tool 2>/dev/null || echo "Topology unavailable"
