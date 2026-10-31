//! Other apps in the notch. Each integration talks to its app the way the
//! app offers for that (Blitzit: its official MCP connection). More apps
//! (Obsidian, Msty) can follow the same pattern with their own notch page.

pub mod blitzit;
pub mod blitzit_sync;
pub(crate) mod http;
mod mcp;
mod oauth;
mod secret;
mod tools;
