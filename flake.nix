{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { nixpkgs, flake-utils, rust-overlay, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rustfmt" "clippy" ];
          targets = [ "wasm32-unknown-unknown" ];
        };
      in {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            # Rust
            rustToolchain
            wasm-pack

            # Native build deps for Rust crates (openssl-sys, rusqlite, etc.)
            gcc
            pkg-config
            openssl
            openssl.dev
            sqlite
            sqlite.dev

            # Node / frontend
            nodejs_22
            pnpm

            # Python / NLP
            python313
            uv

            # Dev tools
            cargo-flamegraph
          ];

          env = {
            PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
            LIBRARY_PATH = "${pkgs.sqlite.out}/lib";
          };
        };
      });
}
