#!/bin/bash
cat > /tmp/test-nes-worker.yaml <<'EOF'
logLevel: LOG_DEBUG
coordinatorHost: 10.0.10.196
localWorkerHost: 100.88.85.75
coordinatorPort: 8080
rpcPort: 40000
dataPort: 40001
numberOfSlots: 65535
physicalSources:
  - logicalSourceName: telemetry_0x0007
    physicalSourceName: edge-0x0007-GW-EDGE-001
    type: MQTT_SOURCE
    configuration:
      url: "tcp://localhost:1883"
      topic: "telemetry/nes"
      clientId: "nes-edge-worker"
      userName: "nes"
      qos: 1
      cleanSession: true
      inputFormat: CSV
      flushIntervalMS: 1000
      numberOfBuffersToProduce: 0
EOF

cat /tmp/test-nes-worker.yaml

echo ""
docker rm -f nes-worker-test 2>/dev/null
docker run --rm --name nes-worker-test \
  --network=host \
  -v /tmp/test-nes-worker.yaml:/tmp/nes-worker.yaml:ro \
  nebulastream/nes-executable-image:latest \
  nesWorker --configPath=/tmp/nes-worker.yaml 2>&1 | head -100

echo ""
echo "Exit code: $?"
