# Pin the Rust version (keep in sync with rust-toolchain.toml) AND the distro.
# The distro must match the runtime stage below: a floating tag once silently
# moved from bookworm to trixie, producing a binary that needed a newer glibc
# than the runtime had — the machine crash-looped and clients hung forever.
FROM lukemathwalker/cargo-chef:latest-rust-1.96-trixie AS chef
WORKDIR /app

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY yap-ai-backend/Cargo.toml yap-ai-backend/Cargo.toml
COPY language-utils/Cargo.toml language-utils/Cargo.toml
COPY libraries/google-tts/Cargo.toml libraries/google-tts/Cargo.toml
COPY libraries/autograde-core/Cargo.toml libraries/autograde-core/Cargo.toml
# Trim workspace to only the crates needed for ai-backend (google-tts and
# autograde-core are path dependencies of yap-ai-backend, so their manifests
# must be present too).
RUN sed -i '/^members = \[/,/^\]/c\members = [\n    "yap-ai-backend",\n    "language-utils",\n    "libraries/google-tts",\n    "libraries/autograde-core",\n]' Cargo.toml
# cargo-chef needs stub source files to prepare the recipe
RUN mkdir -p yap-ai-backend/src && echo "fn main() {}" > yap-ai-backend/src/main.rs
RUN mkdir -p language-utils/src && touch language-utils/src/lib.rs
RUN mkdir -p libraries/google-tts/src && touch libraries/google-tts/src/lib.rs
RUN mkdir -p libraries/autograde-core/src && touch libraries/autograde-core/src/lib.rs
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
RUN apt-get update && apt-get install -y \
    libopus-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*
COPY --from=planner /app/recipe.json recipe.json
COPY --from=planner /app/Cargo.toml Cargo.toml
COPY Cargo.lock .
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json
# Copy actual source and data
COPY yap-ai-backend yap-ai-backend
COPY language-utils language-utils
COPY libraries/google-tts libraries/google-tts
COPY libraries/autograde-core libraries/autograde-core
COPY out out
RUN cargo build --release --bin ai-backend

# Runtime stage with required libraries.
# NOTE: must be the same distro as the builder stage above (or newer), or the
# binary fails at startup with "GLIBC_x.yz not found" and crash-loops.
FROM debian:trixie-slim AS runtime
WORKDIR /app

# Install required runtime dependencies (libssl3 was renamed libssl3t64 in trixie)
RUN apt-get update && apt-get install -y \
    libssl3t64 \
    libopus0 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/ai-backend /usr/local/bin
ENTRYPOINT ["/usr/local/bin/ai-backend"]
