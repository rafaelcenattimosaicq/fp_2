#!/bin/bash
pkill -f "gateway.*headless" 2>/dev/null || true
pkill -f "gateway gateway.yaml" 2>/dev/null || true
docker ps -a --filter name=nes-worker -q | xargs -r docker rm -f 2>/dev/null || true
echo "Gateway and worker killed"
