use axum::{
    body::Bytes,
    http::{StatusCode, Uri},
    response::Json,
};

use super::{contract::HealthStatus, problem_details::ProblemDetails};

pub async fn health() -> Json<HealthStatus> {
    Json(HealthStatus { status: "ok" })
}

pub async fn extract(_body: Bytes) -> StatusCode {
    StatusCode::NOT_IMPLEMENTED
}

pub async fn not_found(uri: Uri) -> ProblemDetails {
    ProblemDetails::not_found_with_instance(uri.path())
}
