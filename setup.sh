#!/bin/bash

# Setup script for Yap development environment
# This script installs necessary tools and builds the project

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

# Start setup
echo "======================================"
echo "   Yap Development Environment Setup"
echo "======================================"
echo ""

# Check and install Rust/Cargo
if command_exists cargo; then
    print_status "Cargo is already installed"
else
    print_info "Installing Rust and Cargo..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
    print_status "Rust and Cargo installed"
fi

# Add wasm32 target
print_info "Adding wasm32-unknown-unknown target..."
rustup target add wasm32-unknown-unknown
print_status "wasm32-unknown-unknown target added"

# Check and install pnpm
if command_exists pnpm; then
    print_status "pnpm is already installed"
else
    print_info "Installing pnpm..."
    if command_exists npm; then
        npm install -g pnpm
    elif command_exists curl; then
        curl -fsSL https://get.pnpm.io/install.sh | sh -
    else
        print_error "Neither npm nor curl found. Please install pnpm manually."
        exit 1
    fi
    print_status "pnpm installed"
fi

# Check and install wasm-pack
if command_exists wasm-pack; then
    print_status "wasm-pack is already installed"
else
    print_info "Installing wasm-pack and wasm-opt via cargo..."
    curl https://drager.github.io/wasm-pack/installer/init.sh -sSf | bash
    print_status "wasm-pack installed"
fi

# Update PATH in ~/.bashrc (or ~/.zshrc for macOS)
print_info "Updating PATH in shell configuration..."

# Determine which shell config file to use
SHELL_CONFIG="$HOME/.bashrc"

print_info "Using shell config: $SHELL_CONFIG"

# Add cargo bin to PATH if not already there
if ! grep -q '.cargo/bin' "$SHELL_CONFIG"; then
    echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> "$SHELL_CONFIG"
    print_status "Added cargo bin to PATH"
fi

# Add .local/bin to PATH if not already there
if ! grep -q '.local/bin' "$SHELL_CONFIG"; then
    echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$SHELL_CONFIG"
    print_status "Added ~/.local/bin to PATH"
fi

# Export PATH for current session
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"

# Build the Rust project
print_info "Building Rust project..."
cargo build --release
print_status "Rust project built"

# Format Rust code
print_info "Formatting Rust code..."
cargo fmt
print_status "Rust code formatted"

# Disable wasm-opt in Cargo.toml for faster builds
print_info "Configuring wasm-opt settings for faster builds..."
cd yap-frontend-rs

# Add wasm-opt = false to Cargo.toml if not already present
if ! grep -q "\[package.metadata.wasm-pack.profile.release\]" Cargo.toml; then
    echo "" >> Cargo.toml
    echo "[package.metadata.wasm-pack.profile.release]" >> Cargo.toml
    echo "wasm-opt = false" >> Cargo.toml
    print_status "Disabled wasm-opt for release builds in Cargo.toml"
else
    if ! grep -q "wasm-opt = false" Cargo.toml; then
        sed -i '' '/\[package.metadata.wasm-pack.profile.release\]/a\
wasm-opt = false' Cargo.toml
        print_status "Disabled wasm-opt for release builds in Cargo.toml"
    else
        print_status "wasm-opt already disabled for release builds"
    fi
fi

# Build the WASM module
print_info "Building WASM module in yap-frontend-rs..."
wasm-pack build --release --features local-backend
cd ..
print_status "WASM module built"

# Install and build frontend
print_info "Installing frontend dependencies..."
cd yap-frontend
pnpm install
print_status "Frontend dependencies installed"

print_info "Building frontend..."
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

echo ""
echo "======================================"
echo -e "${GREEN}   Setup completed successfully!${NC}"
echo "======================================"
echo ""
echo "To start the development server, run:"
echo "  cd yap-frontend && pnpm dev"
echo ""
echo "To rebuild the WASM module after changes:"
echo "  cd yap-frontend-rs && wasm-pack build --features local-backend"
echo ""
echo "Note: wasm-opt is disabled in Cargo.toml for faster builds"
echo ""
