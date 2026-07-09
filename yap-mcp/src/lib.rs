//! yap-mcp: exposing a yap.town account over the Model Context Protocol.
//!
//! The [`server`] module holds the tool implementations and per-user state;
//! [`sync`] talks to Supabase; [`deck`] replays events into deck state.
//! `main.rs` wraps it all in a stdio transport for local use.

pub mod deck;
pub mod remote;
pub mod server;
pub mod sync;
