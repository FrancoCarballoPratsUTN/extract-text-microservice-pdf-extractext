use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::app::state::AppState;

use super::problem_details::ProblemDetails;

pub async fn enforce_payload_limit(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let response = next.run(request).await;

    if response.status() == StatusCode::PAYLOAD_TOO_LARGE {
        ProblemDetails::payload_too_large(state.config.body_limit_bytes).into_response()
    } else {
        response
    }
}
