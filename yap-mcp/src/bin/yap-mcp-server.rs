//! yap-mcp-server: the remote MCP server (streamable HTTP + OAuth), usable as
//! a claude.ai custom connector. See `yap_mcp::remote` for the architecture.

use std::sync::Arc;

use yap_mcp::remote::{RemoteApp, RemoteConfig, serve};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let _ = dotenvy::from_path(concat!(env!("CARGO_MANIFEST_DIR"), "/../.env"));
    let _ = dotenvy::dotenv();

    let config = RemoteConfig::from_env()?;
    let app = Arc::new(RemoteApp::new(config));
    serve(app).await
}
