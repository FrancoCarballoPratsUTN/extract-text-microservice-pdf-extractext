use std::time::Instant;

use axum::{
    body::Bytes,
    extract::State,
    http::Uri,
    response::{IntoResponse, Json, Response},
};

use crate::app::{extract_service, state::AppState};

use super::{
    contract::{ExtractRequest, ExtractResponse, HealthStatus},
    problem_details::ProblemDetails,
};

pub async fn health() -> Json<HealthStatus> {
    Json(HealthStatus { status: "ok" })
}

pub async fn extract(State(state): State<AppState>, body: Bytes) -> Response {
    let request: ExtractRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(_) => return ProblemDetails::invalid_request_body().into_response(),
    };

    let started = Instant::now();
    let result = extract_service::extract(request.document_base64.into_bytes(), &state).await;
    let duration_ms = started.elapsed().as_millis() as u64;

    match result {
        Ok(document) => {
            Json(ExtractResponse::from_extracted(document, duration_ms)).into_response()
        }
        Err(error) => ProblemDetails::from_domain(error).into_response(),
    }
}

pub async fn not_found(uri: Uri) -> ProblemDetails {
    ProblemDetails::not_found_with_instance(uri.path())
}
