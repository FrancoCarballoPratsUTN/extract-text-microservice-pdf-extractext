use std::{env, net::SocketAddr, process::ExitCode};

use extract::{api::router::build_router, config::Config};
use tracing_subscriber::EnvFilter;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() -> ExitCode {
    if env::args().any(|arg| arg == "--version") {
        println!("extract {VERSION}");
        return ExitCode::SUCCESS;
    }

    init_tracing();

    let config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => {
            tracing::error!(%error, "invalid configuration");
            return ExitCode::FAILURE;
        }
    };

    let app = build_router(config.clone());
    let addr: SocketAddr = config.bind_addr;

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(error) => {
            tracing::error!(%error, "failed to bind address");
            return ExitCode::FAILURE;
        }
    };

    tracing::info!(
        %addr,
        threads = config.thread_count,
        body_limit_bytes = config.body_limit_bytes,
        "extract service ready"
    );

    match axum::serve(listener, app).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "server error");
            ExitCode::FAILURE
        }
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .init();
}
