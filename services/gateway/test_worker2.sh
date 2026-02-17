#!/bin/bash
docker rm -f nes-worker-test 2>/dev/null

timeout 30 docker run --rm --name nes-worker-test \
  --network=host \
  -v /tmp/test-nes-worker.yaml:/tmp/nes-worker.yaml:ro \
  nebulastream/nes-executable-image:latest \
  nesWorker --configPath=/tmp/nes-worker.yaml 2>&1

EXIT_CODE=$?
echo ""
echo "Exit code: $EXIT_CODE"
if [ $EXIT_CODE -eq 124 ]; then
  echo "(timeout - worker was still running, which is GOOD)"
fi
