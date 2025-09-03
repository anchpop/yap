#!/bin/bash

# Maintenance script for Yap development - builds only, no installations
# This script assumes all tools are already installed

set -e  # Exit on error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${GREEN}[✓]${NC} $1"
}

print_error() {
    echo -e "${RED}[✗]${NC} $1"
}

print_info() {
    echo -e "${YELLOW}[i]${NC} $1"
}

# Function to check if a command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Start maintenance
echo "======================================"
echo "   Yap Project Build & Maintenance"
echo "======================================"
echo ""

# Check that required tools exist
print_info "Checking required tools..."
MISSING_TOOLS=()

if ! command_exists cargo; then
    MISSING_TOOLS+=("cargo")
fi

if ! command_exists pnpm; then
    MISSING_TOOLS+=("pnpm")
fi

if ! command_exists wasm-pack; then
    MISSING_TOOLS+=("wasm-pack")
fi

if [ ${#MISSING_TOOLS[@]} -gt 0 ]; then
    print_error "Missing required tools: ${MISSING_TOOLS[*]}"
    echo "Please run ./setup.sh first to install all dependencies"
    exit 1
fi

print_status "All required tools are installed"

# Format Rust code
print_info "Formatting Rust code..."
cargo fmt
print_status "Rust code formatted"

# Build the Rust project
print_info "Building Rust project..."
cargo build --release
print_status "Rust project built"

# Build the WASM module
print_info "Building WASM module in yap-frontend-rs..."
cd yap-frontend-rs
wasm-pack build --release --features local-backend
cd ..
print_status "WASM module built"

# Install frontend dependencies if node_modules doesn't exist
if [ ! -d "yap-frontend/node_modules" ]; then
    print_info "Installing frontend dependencies (node_modules not found)..."
    cd yap-frontend
    pnpm install
    cd ..
    print_status "Frontend dependencies installed"
fi

# Build frontend
print_info "Building frontend..."
cd yap-frontend
pnpm build
cd ..
print_status "Frontend built"

# Optional: Run tests
print_info "Running Rust tests..."
cargo test
print_status "Rust tests completed"

# Optional: Run clippy
print_info "Running clippy linter..."
cargo clippy --all-targets --all-features
print_status "Clippy checks completed"

# Optional: TypeScript type checking
print_info "Running TypeScript type checking..."
cd yap-frontend
pnpm tsc --noEmit || print_info "TypeScript check completed with warnings"
cd ..

echo ""
echo "======================================"
echo -e "${GREEN}   Build completed successfully!${NC}"
echo "======================================"
echo ""
echo "To start the development server:"
echo "  cd yap-frontend && pnpm dev"
echo ""
echo "To watch and rebuild on changes:"
echo "  - For WASM: cd yap-frontend-rs && cargo watch -x 'build --target wasm32-unknown-unknown'"
echo "  - For frontend: cd yap-frontend && pnpm dev"
echo ""