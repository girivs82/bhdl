//! BHDL Language Server - Main Entry Point
//!
//! Starts the LSP server and listens on stdin/stdout for LSP protocol messages.
//!
//! Usage:
//!   bhdl-lsp
//!
//! The server communicates via JSON-RPC over stdin/stdout, following the
//! Language Server Protocol specification.

use tower_lsp::{LspService, Server};
use bhdl_lsp::BhdlLanguageServer;

#[tokio::main]
async fn main() {
    // Initialize logging
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    log::info!("Starting BHDL Language Server v{}", env!("CARGO_PKG_VERSION"));

    // Create LSP service
    let (service, socket) = LspService::new(BhdlLanguageServer::new);

    // Start server on stdin/stdout
    Server::new(tokio::io::stdin(), tokio::io::stdout(), socket)
        .serve(service)
        .await;

    log::info!("BHDL Language Server shutting down");
}
