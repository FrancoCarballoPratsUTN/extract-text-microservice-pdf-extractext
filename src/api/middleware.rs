use axum::{
    body::Body,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use http_body_util::BodyExt;

use crate::app::state::AppState;

use super::problem_details::ProblemDetails;

pub async fn enforce_payload_limit(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    if let Some(content_length) = request
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok())
        && content_length > state.config.body_limit_bytes
    {
        drain(request.into_body()).await;
        return ProblemDetails::payload_too_large(state.config.body_limit_bytes).into_response();
    }

    let response = next.run(request).await;

    if response.status() == StatusCode::PAYLOAD_TOO_LARGE {
        ProblemDetails::payload_too_large(state.config.body_limit_bytes).into_response()
    } else {
        response
    }
}

/// Consumes the rest of the request body so the client finishes uploading and
/// reliably receives the early `413` response on a live connection.
async fn drain(mut body: Body) {
    while let Some(Ok(_)) = body.frame().await {}
}
