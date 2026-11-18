FROM node:24-alpine AS frontend
WORKDIR /src
COPY frontend/package*.json ./
RUN npm ci
COPY frontend/index.html frontend/tsconfig.json frontend/vite.config.ts ./
COPY frontend/src/ ./src/
COPY frontend/public/ ./public/
ARG APP_VERSION=1.0.1
RUN VITE_APP_VERSION="${APP_VERSION}" npm run build

FROM rust:1-bookworm AS backend
WORKDIR /src
ARG CARGO_BUILD_JOBS=2
ARG TARGETARCH
ENV CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS}
COPY backend/Cargo.toml backend/Cargo.lock backend/schema.sql ./
COPY backend/src/ ./src/
RUN --mount=type=cache,id=magic-cloud-registry,target=/usr/local/cargo/registry,sharing=locked --mount=type=cache,id=magic-cloud-target-${TARGETARCH},target=/src/target,sharing=locked cargo build --release --locked && cp target/release/magic-cloud /magic-cloud

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates ffmpeg curl sqlite3 && rm -rf /var/lib/apt/lists/* && groupadd -g 10001 magic && useradd -u 10001 -g magic -M magic
COPY --from=backend /magic-cloud /app/magic-cloud
COPY --from=frontend /src/dist /app/public
USER 10001:10001
ENV STATIC_DIR=/app/public STORAGE_ROOT=/data PORT=5080
EXPOSE 5080
ENTRYPOINT ["/app/magic-cloud"]
