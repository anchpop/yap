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
# cmake: the g2p dependency builds the anchpop/espeak-ng fork from source
# (statically linked, phoneme data embedded) — the transcription grader uses
# it to phonemize what the user heard and what they wrote.
RUN apt-get update && apt-get install -y \
    libopus-dev \
    pkg-config \
    cmake \
    && rm -rf /var/lib/apt/lists/*
COPY --from=planner /app/recipe.json recipe.json
COPY --from=planner /app/Cargo.toml Cargo.toml
COPY Cargo.lock .
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json
# out/ before the sources: this layer (rkyv packs included) is huge and rarely
# changes — keep it above the every-commit source layers so it isn't re-created
# (and re-exported to the cache registry) on every deploy.
COPY out out
COPY yap-ai-backend yap-ai-backend
COPY language-utils language-utils
COPY libraries/google-tts libraries/google-tts
COPY libraries/autograde-core libraries/autograde-core
# Drop target/ within the same layer: it's ~hundreds of MB of incremental
# artifacts that change every commit, and mode=max cache export would ship
# all of it to the registry each deploy.
RUN cargo build --release --bin ai-backend \
    && cp target/release/ai-backend /app/ai-backend \
    && cp -r "$(find /usr/local/cargo/git/checkouts -type d -path '*/g2p-*/*/python' | head -1)" /app/g2p-python \
    && rm -rf target

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

# No espeak install: the engine and its data are inside the binary (g2p
# unpacks the data into $HOME/.cache on first use).
#
# Thai and Korean G2P are Python inside the g2p crate: pinned uv projects it
# unpacks beside its data and runs with `uv run --locked`. Without uv those
# two languages lose their readable-IPA line (the crate reports the backend
# as unavailable and the grader continues without it). Install uv and warm
# its cache with the managed Python and every wheel the two lockfiles pin,
# so the crate's first call builds its environment from the cache in well
# under a second and never needs the network. Same $HOME as at runtime.
COPY --from=ghcr.io/astral-sh/uv:0.10.2 /uv /usr/local/bin/uv
COPY --from=builder /app/g2p-python /opt/g2p-python
RUN uv python install 3.13 \
    && for p in thai korean; do uv sync --locked --project /opt/g2p-python/$p; done \
    && rm -rf /opt/g2p-python/*/.venv
COPY --from=builder /app/ai-backend /usr/local/bin/ai-backend
ENTRYPOINT ["/usr/local/bin/ai-backend"]
