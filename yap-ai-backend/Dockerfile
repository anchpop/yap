# Pin the Rust version (keep in sync with rust-toolchain.toml) AND the distro.
# The distro must match the runtime stage below: a floating tag once silently
# moved from bookworm to trixie, producing a binary that needed a newer glibc
# than the runtime had — the machine crash-looped and clients hung forever.
FROM lukemathwalker/cargo-chef:latest-rust-1.96-trixie AS chef
WORKDIR /app

# Build the custom espeak-ng fork (anchpop/espeak-ng — French-liaison patches).
# The translation grader shells out to it to phonemize the challenge sentence.
# Built on trixie so libespeak-ng.so matches the trixie-slim runtime's glibc.
# `--without-pcaudiolib`/`--without-mbrola` drop the audio backends we never use
# (we only ever call `-q --ipa`), keeping the binary free of audio .so deps.
FROM debian:trixie-slim AS espeak-builder
# Bump to pull in newer fork commits (anchpop/espeak-ng@master).
ARG ESPEAK_NG_REF=724808c5a83f9ef95fdd0db886ba7ba537ff224a
RUN apt-get update && apt-get install -y \
    git build-essential autoconf automake libtool pkg-config \
    && rm -rf /var/lib/apt/lists/*
RUN git clone https://github.com/anchpop/espeak-ng.git /src \
    && cd /src \
    && git checkout "$ESPEAK_NG_REF" \
    && ./autogen.sh \
    && ./configure --prefix=/usr --without-pcaudiolib --without-mbrola \
    && make -j"$(nproc)" \
    && make DESTDIR=/espeak-install install

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY yap-ai-backend/Cargo.toml yap-ai-backend/Cargo.toml
COPY language-utils/Cargo.toml language-utils/Cargo.toml
COPY libraries/google-tts/Cargo.toml libraries/google-tts/Cargo.toml
COPY libraries/autograde-core/Cargo.toml libraries/autograde-core/Cargo.toml
COPY libraries/espeak/Cargo.toml libraries/espeak/Cargo.toml
# Trim workspace to only the crates needed for ai-backend (google-tts,
# autograde-core, and espeak are path dependencies pulled in transitively, so
# their manifests must be present too — autograde-core depends on espeak).
RUN sed -i '/^members = \[/,/^\]/c\members = [\n    "yap-ai-backend",\n    "language-utils",\n    "libraries/google-tts",\n    "libraries/autograde-core",\n    "libraries/espeak",\n]' Cargo.toml
# cargo-chef needs stub source files to prepare the recipe
RUN mkdir -p yap-ai-backend/src && echo "fn main() {}" > yap-ai-backend/src/main.rs
RUN mkdir -p language-utils/src && touch language-utils/src/lib.rs
RUN mkdir -p libraries/google-tts/src && touch libraries/google-tts/src/lib.rs
RUN mkdir -p libraries/autograde-core/src && touch libraries/autograde-core/src/lib.rs
RUN mkdir -p libraries/espeak/src && touch libraries/espeak/src/lib.rs
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
COPY libraries/espeak libraries/espeak
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

# Install the espeak-ng fork: binary, shared lib, and phoneme data. The grader
# resolves the binary via ESPEAK_NG_BIN and the data dir's parent via
# ESPEAK_NG_DATA_PATH (espeak appends `espeak-ng-data` itself).
COPY --from=espeak-builder /espeak-install/usr/bin/espeak-ng /usr/local/bin/espeak-ng
COPY --from=espeak-builder /espeak-install/usr/lib/ /usr/local/lib/
COPY --from=espeak-builder /espeak-install/usr/share/espeak-ng-data/ /usr/local/share/espeak-ng-data/
RUN ldconfig
ENV ESPEAK_NG_BIN=/usr/local/bin/espeak-ng \
    ESPEAK_NG_DATA_PATH=/usr/local/share
# Fail the build loudly if espeak can't phonemize — otherwise a broken install
# degrades silently (the grader just omits IPA) and rots unnoticed.
RUN espeak-ng --version \
    && espeak-ng --path="$ESPEAK_NG_DATA_PATH" -v fr -q --ipa -x "on est" | grep -q .

COPY --from=builder /app/target/release/ai-backend /usr/local/bin
ENTRYPOINT ["/usr/local/bin/ai-backend"]
