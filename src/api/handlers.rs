use std::time::Instant;

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, Uri, header},
    response::{IntoResponse, Json, Response},
};

use crate::app::{extract_service, state::AppState};

use super::{
    contract::{ExtractResponse, HealthStatus},
    problem_details::ProblemDetails,
};

pub async fn health() -> Json<HealthStatus> {
    Json(HealthStatus { status: "ok" })
}

pub async fn extract(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    if !is_pdf_content_type(&headers) {
        return ProblemDetails::unsupported_media_type().into_response();
    }

    let started = Instant::now();
    let result = extract_service::extract(body, &state).await;
    let duration_ms = started.elapsed().as_millis() as u64;

    match result {
        Ok(document) => {
            let response = ExtractResponse::from_extracted(document, duration_ms);
            let serialize_started = Instant::now();
            let json = match serde_json::to_vec(&response) {
                Ok(json) => json,
                Err(error) => {
                    tracing::error!(%error, "failed to serialize extract response");
                    return ProblemDetails::internal(
                        "Serialization Failed",
                        "the extract response could not be serialized",
                    )
                    .into_response();
                }
            };
            let serialize_ms = serialize_started.elapsed().as_millis() as u64;
            tracing::debug!(serialize_ms, "extract response serialized");
            ([(header::CONTENT_TYPE, "application/json")], json).into_response()
        }
        Err(error) => ProblemDetails::from_domain(error).into_response(),
    }
}

fn is_pdf_content_type(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            let media_type = value
                .split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase();
            matches!(
                media_type.as_str(),
                "application/pdf" | "application/octet-stream"
            )
        })
}

pub async fn not_found(uri: Uri) -> ProblemDetails {
    ProblemDetails::not_found_with_instance(uri.path())
}
