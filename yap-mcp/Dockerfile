# Build with the repo root as context: `flyctl deploy --config yap-mcp/fly.toml`
# Pin the Rust version (keep in sync with rust-toolchain.toml) AND the distro —
# builder and runtime must be the same distro or the binary hits glibc
# mismatches at startup (see yap-ai-backend/Dockerfile for the war story).
FROM rust:1.96-trixie AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p yap-mcp --bin yap-mcp-server

FROM debian:trixie-slim AS runtime
WORKDIR /app
RUN apt-get update && apt-get install -y \
    libssl3t64 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/yap-mcp-server /usr/local/bin/yap-mcp-server
# Language packs are read from disk at runtime (lazily, per course).
COPY out /app/out
ENV YAP_OUT_DIR=/app/out
ENTRYPOINT ["/usr/local/bin/yap-mcp-server"]
