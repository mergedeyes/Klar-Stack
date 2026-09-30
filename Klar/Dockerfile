# check=skip=SecretsUsedInArgOrEnv
# The secret ENV vars below are declared *empty* on purpose (see the ENV
# block) so Bunny lists them in the dashboard; no value is baked into the
# image, so BuildKit's SecretsUsedInArgOrEnv warning is a false positive.
# --- Build stages (cargo-chef) ---
# Dependencies are compiled in their own layer that only depends on
# Cargo.toml/Cargo.lock, so CI (which persists layers via the GitHub
# Actions cache) only recompiles our own crate on a normal source change.
# cargo-chef also discards the placeholder artifacts for our crate after
# `cook`, which avoids the stale-binary bug the old hand-rolled dummy
# main.rs trick had.
FROM rust:1.97-slim-bookworm AS chef

WORKDIR /app

# System deps needed to compile common Rust crates (openssl-sys, etc.)
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN cargo install cargo-chef --locked

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder

# SQLX_OFFLINE=true forces sqlx macros to verify queries against .sqlx/
# instead of connecting to a live database during compilation
ENV SQLX_OFFLINE=true

COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --locked --recipe-path recipe.json

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY .sqlx ./.sqlx
COPY migrations ./migrations
RUN cargo build --release --locked

# --- Runtime stage ---
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Binary name matches [package].name in Cargo.toml ("Klar")
COPY --from=builder /app/target/release/Klar /app/Klar

# Non-root user for defense in depth
RUN useradd --system --create-home --shell /usr/sbin/nologin appuser
USER appuser

# Env vars, declared here so Bunny auto-detects and pre-fills them in the
# dashboard (only ENV, not ARG, ends up in the image config Bunny reads).
# Non-secret defaults are set — verify they match your setup; empty values
# (secrets + deployment-specific) are filled in the Bunny dashboard.
# An empty declared value is NOT the same as unset: config.rs guards the
# ones where empty would be unsafe.
ENV HOST=0.0.0.0 \
    PORT=8080 \
    DATABASE_URL="" \
    JWT_SECRET="" \
    BASE_URL=https://www.klarsocial.eu \
    EMAIL_PROVIDER=scaleway \
    SMTP_HOST=fr-par \
    SMTP_PORT=587 \
    SMTP_PASS="" \
    SMTP_FROM=noreply@klarsocial.eu \
    REDIS_URL=redis://127.0.0.1:6379 \
    STORAGE_PROVIDER=s3 \
    S3_STORAGE_ENDPOINT=https://de-s3.storage.bunnycdn.com \
    S3_STORAGE_BUCKET="" \
    S3_STORAGE_ACCESS_KEY="" \
    S3_STORAGE_SECRET_ACCESS_KEY="" \
    S3_PUBLIC_STORAGE_URL=https://cdn.klarsocial.eu \
    BUNNY_API_KEY="" \
    BUNNY_TOKEN_KEY="" \
    EVIDENCE_S3_STORAGE_BUCKET="" \
    EVIDENCE_S3_STORAGE_SECRET_ACCESS_KEY="" \
    EVIDENCE_ENCRYPTION_KEY="" \
    EVIDENCE_RETENTION_DAYS=183 \
    TRUSTED_PROXY_HOPS=1 \
    DEBUG_CLIENT_IP=false

EXPOSE 8080

CMD ["/app/Klar"]
