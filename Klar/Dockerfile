# --- Build stage ---
FROM rust:1.97-slim-bookworm AS builder

WORKDIR /app

# System deps needed to compile common Rust crates (openssl-sys, etc.)
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests and full source together — no dummy-build caching trick.
# With no persisted BuildKit cache across CI runs, that trick only cost a
# wasted double-compile for zero benefit, and was the source of a bug where
# Cargo's object cache relinked a stale artifact from the placeholder main.rs.
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY .sqlx ./.sqlx
COPY migrations ./migrations

# SQLX_OFFLINE=true forces sqlx macros to verify queries against .sqlx/
# instead of connecting to a live database during compilation
ENV SQLX_OFFLINE=true
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

ENV PORT=8080
EXPOSE 8080

CMD ["/app/Klar"]
