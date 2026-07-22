# --- Build stage ---
FROM rust:1.97-slim AS builder

WORKDIR /app

# System deps needed to compile common Rust crates (openssl-sys, etc.)
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Cache dependencies separately from source for faster rebuilds.
# Copying only manifests first means this layer is reused unless deps change.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release --locked || true
RUN rm -rf src

# Now copy the real source, .sqlx (needed since build has no live DB access),
# and migrations (bundled into the binary or run at startup, depending on your setup)
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
