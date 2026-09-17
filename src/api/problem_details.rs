use axum::{
    Json,
    http::{StatusCode, header::CONTENT_TYPE},
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ProblemDetails {
    #[serde(rename = "type")]
    r#type: &'static str,
    title: &'static str,
    status: u16,
    detail: String,
}

impl ProblemDetails {
    pub fn payload_too_large(limit_bytes: usize) -> Self {
        Self {
            r#type: "about:blank",
            title: "Payload Too Large",
            status: StatusCode::PAYLOAD_TOO_LARGE.as_u16(),
            detail: format!("request body exceeds the {limit_bytes} byte limit"),
        }
    }
}

impl IntoResponse for ProblemDetails {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let mut response = Json(self).into_response();
        *response.status_mut() = status;
        response.headers_mut().insert(
            CONTENT_TYPE,
            "application/problem+json"
                .parse()
                .expect("valid content type"),
        );
        response
    }
}
