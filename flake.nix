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
            mold
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
            # Native Linux builds in this dev shell use mold without imposing
            # a mold dependency on non-Nix CI or passing ELF flags to wasm32.
            CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS =
              "--cfg=web_sys_unstable_apis -C link-arg=-fuse-ld=mold";
          };

          # manylinux wheels in the NLP venv (numpy, torch) dlopen the system
          # libstdc++/zlib, which NixOS doesn't provide globally; torch also
          # needs the NixOS driver dir for libcuda.so.1 or it silently falls
          # back to CPU. Appended (not set via `env`) so an inherited
          # LD_LIBRARY_PATH survives.
          LD_LIBRARY_PATH_EXTRA = "${pkgs.stdenv.cc.cc.lib}/lib:${pkgs.zlib}/lib:/run/opengl-driver/lib";

          # generate-data's Google translator (gcp_auth) needs a service-account
          # JSON, not the API keys in .env. The account is git-crypt'd in the repo.
          shellHook = ''
            export LD_LIBRARY_PATH="$LD_LIBRARY_PATH_EXTRA''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
            export GOOGLE_APPLICATION_CREDENTIALS="$PWD/secrets/gcp-service-account.json"

            # Cache mirroring via osmo: generate-data warms/flushes .cache to the
            # `yap-cache` R2 bucket when these are set. Credentials are rendered by
            # sops-nix to /run/secrets/r2/* (owner andrep). Only export each var if its
            # secret file is readable, so machines/CI without the secrets are unaffected.
            export YAP_CACHE_BUCKET="yap-cache"
            for pair in account_id:R2_ACCOUNT_ID access_key_id:R2_ACCESS_KEY_ID secret_access_key:R2_SECRET_ACCESS_KEY; do
              secret_file="/run/secrets/r2/''${pair%%:*}"
              [ -r "$secret_file" ] && export "''${pair##*:}=$(cat "$secret_file")"
            done
          '';
        };
      });
}
