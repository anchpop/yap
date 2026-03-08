#!/bin/bash
set -euo pipefail

# Build the dictionary data generator and generate JSON from rkyv language packs
echo "==> Building generate-dictionary-data..."
cargo build --release --bin generate-dictionary-data

echo "==> Generating dictionary JSON from language packs..."
cargo run --release --bin generate-dictionary-data

# Build the Astro static site (outputs directly to yap-frontend/public/dictionary/)
echo "==> Installing dictionary site dependencies..."
cd dictionary-site
pnpm install

echo "==> Building dictionary static pages..."
NODE_OPTIONS="--max-old-space-size=8192" npx astro build

echo "==> Dictionary build complete!"
echo "    Pages available at /d/"
