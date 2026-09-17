mod config;

use std::env;
use std::process::ExitCode;

use config::Config;
use tracing_subscriber::EnvFilter;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    if env::args().any(|arg| arg == "--version") {
        println!("extract {VERSION}");
        return ExitCode::SUCCESS;
    }

    init_tracing();

    match Config::from_env() {
        Ok(config) => {
            tracing::info!(?config, "configuration loaded");
            tracing::info!(
                threads = config.thread_count,
                "scaffold up: HTTP server lands in T3"
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            tracing::error!(%error, "invalid configuration");
            ExitCode::FAILURE
        }
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
