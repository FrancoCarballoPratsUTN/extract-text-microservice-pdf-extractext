use axum::{
    Router,
    extract::DefaultBodyLimit,
    middleware,
    routing::{get, post},
};
use tower_http::compression::{CompressionLayer, CompressionLevel};

use crate::{
    api::{handlers, middleware as api_middleware},
    app::state::AppState,
    config::Config,
};

pub fn build_router(config: Config) -> Router {
    let state = AppState::new(config.clone());

    Router::new()
        .route("/health", get(handlers::health))
        .route("/extract", post(handlers::extract))
        .fallback(handlers::not_found)
        .layer(DefaultBodyLimit::max(config.body_limit_bytes))
        .layer(CompressionLayer::new().quality(CompressionLevel::Fastest))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            api_middleware::enforce_payload_limit,
        ))
        .with_state(state)
}
