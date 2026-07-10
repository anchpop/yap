# Build with the repo root as context: `flyctl deploy --config yap-mcp/fly.toml`
# Pin the Rust version (keep in sync with rust-toolchain.toml) AND the distro —
# builder and runtime must be the same distro or the binary hits glibc
# mismatches at startup (see yap-ai-backend/Dockerfile for the war story).
FROM lukemathwalker/cargo-chef:latest-rust-1.96-trixie AS chef
WORKDIR /app

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY yap-mcp/Cargo.toml yap-mcp/Cargo.toml
COPY yap-frontend-rs/Cargo.toml yap-frontend-rs/Cargo.toml
COPY language-utils/Cargo.toml language-utils/Cargo.toml
COPY libraries/weapon/Cargo.toml libraries/weapon/Cargo.toml
COPY libraries/imdex_map/Cargo.toml libraries/imdex_map/Cargo.toml
COPY libraries/eyedee/Cargo.toml libraries/eyedee/Cargo.toml
COPY libraries/autograde-core/Cargo.toml libraries/autograde-core/Cargo.toml
# Trim the workspace to yap-mcp's path-dependency closure (yap-frontend-rs
# pulls in weapon, imdex_map, eyedee, and autograde-core).
RUN sed -i '/^members = \[/,/^\]/c\members = [\n    "yap-mcp",\n    "yap-frontend-rs",\n    "language-utils",\n    "libraries/weapon",\n    "libraries/imdex_map",\n    "libraries/eyedee",\n    "libraries/autograde-core",\n]' Cargo.toml
# cargo-chef needs stub files for every target declared in the manifests
RUN mkdir -p yap-mcp/src && echo "fn main() {}" > yap-mcp/src/main.rs
RUN mkdir -p yap-frontend-rs/src yap-frontend-rs/benches \
    && touch yap-frontend-rs/src/lib.rs \
    && echo "fn main() {}" > yap-frontend-rs/benches/simulation.rs \
    && echo "fn main() {}" > yap-frontend-rs/benches/profile_next_cards.rs
RUN mkdir -p language-utils/src && touch language-utils/src/lib.rs
RUN mkdir -p libraries/weapon/src/bin \
    && touch libraries/weapon/src/lib.rs \
    && echo "fn main() {}" > libraries/weapon/src/bin/weapon-scope.rs
RUN mkdir -p libraries/imdex_map/src && touch libraries/imdex_map/src/lib.rs
RUN mkdir -p libraries/eyedee/src && touch libraries/eyedee/src/lib.rs
RUN mkdir -p libraries/autograde-core/src && touch libraries/autograde-core/src/lib.rs
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
COPY --from=planner /app/Cargo.toml Cargo.toml
COPY Cargo.lock .
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json
# out/ before the sources: yap-frontend-rs include_str!s the showcase.json and
# language_data.hash files, and this layer (rkyv packs included) is huge and
# rarely changes — keep it above the every-commit source layers.
COPY out out
COPY yap-mcp yap-mcp
COPY yap-frontend-rs yap-frontend-rs
COPY language-utils language-utils
COPY libraries/weapon libraries/weapon
COPY libraries/imdex_map libraries/imdex_map
COPY libraries/eyedee libraries/eyedee
COPY libraries/autograde-core libraries/autograde-core
# Drop target/ within the same layer: it's ~hundreds of MB of incremental
# artifacts that change every commit, and mode=max cache export would ship
# all of it to the registry each deploy.
RUN cargo build --release -p yap-mcp \
    && cp target/release/yap-mcp /app/yap-mcp-bin \
    && rm -rf target

FROM debian:trixie-slim AS runtime
WORKDIR /app
RUN apt-get update && apt-get install -y \
    libssl3t64 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/yap-mcp-bin /usr/local/bin/yap-mcp
# Language packs are read from disk at runtime (lazily, per course).
COPY out /app/out
ENV YAP_OUT_DIR=/app/out
# The review widget is built by CI (pnpm, outside Docker) and copied in;
# the server serves it as the MCP Apps ui:// resource.
COPY yap-mcp/widget/dist/index.html /app/widget/index.html
ENV YAP_WIDGET_HTML=/app/widget/index.html
ENTRYPOINT ["/usr/local/bin/yap-mcp", "serve"]
