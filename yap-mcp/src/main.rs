//! yap-mcp: an MCP server exposing a yap.town account to chat clients.
//!
//! Reviews, dictionary search, adding words, comprehensible example sentences,
//! and stats — all backed by the same event stream the web app syncs.
//!
//! Runs in one of two modes:
//! - `yap-mcp` (or `yap-mcp stdio`): stdio transport for local clients like
//!   Claude Code/Desktop, bound to one account (YAP_USER_EMAIL + service key).
//! - `yap-mcp serve`: remote streamable-HTTP server with OAuth, usable as a
//!   claude.ai custom connector. Multi-user; see `yap_mcp::remote`.

use std::sync::Arc;

use anyhow::bail;
use rmcp::ServiceExt as _;
use yap_mcp::{remote, server};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // In stdio mode stdout carries the MCP protocol, so all logging must go
    // to stderr (which is also fine for serve mode).
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(env_logger::Target::Stderr)
        .init();

    // The repo root .env (same one inspect-user uses) holds the secrets;
    // fall back to a .env in the working directory.
    let _ = dotenvy::from_path(concat!(env!("CARGO_MANIFEST_DIR"), "/../.env"));
    let _ = dotenvy::dotenv();

    match std::env::args().nth(1).as_deref() {
        None | Some("stdio") => run_stdio().await,
        Some("serve") => run_serve().await,
        Some(other) => bail!("unknown mode '{other}': expected 'stdio' (the default) or 'serve'"),
    }
}

async fn run_stdio() -> anyhow::Result<()> {
    let config = server::Config::from_env()?;
    let state = server::YapState::load(config).await?;
    let yap = server::YapMcp::new(state);

    log::info!("yap-mcp ready, serving over stdio");
    let service = yap.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}

async fn run_serve() -> anyhow::Result<()> {
    let config = remote::RemoteConfig::from_env()?;
    let app = Arc::new(remote::RemoteApp::new(config));
    remote::serve(app).await
}
