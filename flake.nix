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

        # Single source of truth: matches rust-toolchain.toml (channel +
        # components + targets) so the dev shell can never drift from the
        # pinned compiler (previously `stable.latest` lagged behind and broke
        # `cfg_select`).
        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
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
            # audiopus_sys (via opus <- google-tts) links system Opus through
            # pkg-config; without it the bundled CMake build mis-installs to
            # lib64/ and the linker can't find -lopus. cmake is a fallback.
            cmake
            libopus
            libopus.dev

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

          # generate-data's Google translator (gcp_auth) needs a service-account
          # JSON, not the API keys in .env. The account is git-crypt'd in the repo.
          shellHook = ''
            export GOOGLE_APPLICATION_CREDENTIALS="$PWD/secrets/gcp-service-account.json"
          '';
        };
      });
}
