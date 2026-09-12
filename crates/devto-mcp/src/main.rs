//! An MCP server for authorship on DEV (dev.to).
//!
//! Speaks newline-delimited JSON-RPC on stdin/stdout. Nothing is written to stdout except
//! responses: a stray `println!` corrupts the stream and the client sees a parse error
//! rather than a message, so diagnostics go to stderr.

mod config;
mod knowledge;
mod protocol;
mod server;
mod tools;

use std::io::{BufRead, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use devto_client::{Config as ClientConfig, DevtoClient};

use crate::config::Config;
use crate::server::Server;

fn main() {
    let config = Config::from_env();

    eprintln!(
        "{} {} — instance {}, authenticated {}, publish {}",
        server::SERVER_NAME,
        env!("CARGO_PKG_VERSION"),
        config.base_url,
        config.is_authenticated(),
        config.capabilities.publish,
    );
    if !config.is_authenticated() {
        eprintln!(
            "no DEVTO_API_KEY: public reads only. Generate a key at \
             https://dev.to/settings/extensions"
        );
    }

    let client = DevtoClient::new(ClientConfig {
        base_url: config.base_url.clone(),
        api_key: config.api_key.clone(),
        ..ClientConfig::default()
    });

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let mut server = Server::new(config, client, now);
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(e) => {
                eprintln!("stdin closed: {e}");
                break;
            }
        };
        if let Some(response) = server.handle_line(&line) {
            if writeln!(stdout, "{response}").is_err() || stdout.flush().is_err() {
                break;
            }
        }
    }
}
