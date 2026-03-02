#!/usr/bin/env bash
set -euo pipefail

REGION="${AWS_REGION:-eu-west-1}"

ECR_URL=$(cd infra && terraform output -json ecr_repository_urls | jq -r '.nebulastream')

if [ -z "$ECR_URL" ] || [ "$ECR_URL" = "null" ]; then
    echo "ERROR: Could not find nebulastream ECR URL in terraform outputs."
    exit 1
fi

aws ecr get-login-password --region "$REGION" \
    | docker login --username AWS --password-stdin "$ECR_URL"

docker build \
    -f nebulastream/docker/single-node-worker/SingleNodeWorker.dockerfile \
    -t "${ECR_URL}:latest" \
    nebulastream/

docker push "${ECR_URL}:latest"
