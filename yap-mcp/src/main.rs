//! yap-mcp: an MCP stdio server exposing a yap.town account to chat clients.
//!
//! Reviews, dictionary search, adding words, comprehensible example sentences,
//! and stats — all backed by the same event stream the web app syncs.

use rmcp::ServiceExt as _;
use yap_mcp::server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // stdout carries the MCP protocol; all logging must go to stderr.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(env_logger::Target::Stderr)
        .init();

    // The repo root .env (same one inspect-user uses) holds the service role
    // key; fall back to a .env in the working directory.
    let _ = dotenvy::from_path(concat!(env!("CARGO_MANIFEST_DIR"), "/../.env"));
    let _ = dotenvy::dotenv();

    let config = server::Config::from_env()?;
    let state = server::YapState::load(config).await?;
    let yap = server::YapMcp::new(state);

    log::info!("yap-mcp ready, serving over stdio");
    let service = yap.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
