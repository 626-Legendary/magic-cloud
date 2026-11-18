#!/bin/bash
set -euo pipefail
project=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$project"
bash -n deploy/scripts/storage.sh
docker run --rm -v "$project:/work:ro" -w /work python:3.13-slim python -m unittest discover -s tests -p 'test_*.py'
docker run --rm -v "$project/backend:/work" -v magic-cloud-cargo:/usr/local/cargo/registry -w /work -e CARGO_BUILD_JOBS=2 rust:1-bookworm sh -c 'rustup component add rustfmt clippy && cargo fmt --check && cargo check --locked && cargo test --locked && cargo clippy --locked --all-targets -- -D warnings'
docker run --rm -v "$project/frontend:/work" -w /work node:24-alpine sh -c 'npm ci && npm run build && npm test && npm audit --audit-level=moderate'
docker compose --env-file .env.example config --quiet
docker compose --env-file .env.example --profile tunnel config --quiet
docker build -t magic-cloud:validation .
