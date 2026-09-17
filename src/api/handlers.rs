use axum::{body::Bytes, http::StatusCode, response::Json};

use super::contract::HealthStatus;

pub async fn health() -> Json<HealthStatus> {
    Json(HealthStatus { status: "ok" })
}

pub async fn extract(_body: Bytes) -> StatusCode {
    StatusCode::NOT_IMPLEMENTED
}
