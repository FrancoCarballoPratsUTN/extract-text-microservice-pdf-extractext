//! RFC 9457 Problem Details: única forma de error de la API (`application/problem+json`).
//!
//! Mapeo exhaustivo `DomainError → ProblemDetails`:
//!
//! | Error                    | status | title                  | detail                                                        |
//! |--------------------------|--------|------------------------|---------------------------------------------------------------|
//! | `Base64Decode`           | 400    | Invalid Base64         | the request body is not valid base64                          |
//! | `InvalidPdfSignature`    | 400    | Invalid PDF Signature  | the decoded payload does not start with the `%PDF-` magic signature |
//! | `PdfParse`               | 422    | Unprocessable PDF      | the payload is not a well-formed PDF document                 |
//! | `Extraction`             | 500    | Extraction Failed      | the PDF text could not be extracted                           |
//!
//! Además: body mayor al límite → `413 Payload Too Large`; ruta inexistente → `404 Not Found`.

use axum::{
    Json,
    http::{StatusCode, header::CONTENT_TYPE},
    response::{IntoResponse, Response},
};
use serde::Serialize;

use crate::domain::DomainError;

#[derive(Debug, Clone, Serialize)]
pub struct ProblemDetails {
    #[serde(rename = "type")]
    r#type: &'static str,
    title: &'static str,
    status: u16,
    detail: String,
    instance: String,
}

impl ProblemDetails {
    pub fn from_domain(error: DomainError) -> Self {
        match error {
            DomainError::Base64Decode => {
                Self::bad_request("Invalid Base64", "the request body is not valid base64")
            }
            DomainError::InvalidPdfSignature => Self::bad_request(
                "Invalid PDF Signature",
                "the decoded payload does not start with the %PDF- magic signature",
            ),
            DomainError::PdfParse => Self::unprocessable(
                "Unprocessable PDF",
                "the payload is not a well-formed PDF document",
            ),
            DomainError::Extraction => {
                Self::internal("Extraction Failed", "the PDF text could not be extracted")
            }
        }
    }

    pub fn payload_too_large(limit_bytes: usize) -> Self {
        Self {
            r#type: "about:blank",
            title: "Payload Too Large",
            status: StatusCode::PAYLOAD_TOO_LARGE.as_u16(),
            detail: format!("request body exceeds the {limit_bytes} byte limit"),
            instance: "/extract".to_string(),
        }
    }

    pub fn not_found() -> Self {
        Self::not_found_with_instance("/")
    }

    pub fn invalid_request_body() -> Self {
        Self::bad_request(
            "Invalid Request Body",
            "request body must be a JSON object with a document_base64 string field",
        )
    }

    pub fn not_found_with_instance(instance: &str) -> Self {
        Self {
            r#type: "about:blank",
            title: "Not Found",
            status: StatusCode::NOT_FOUND.as_u16(),
            detail: "the requested resource does not exist".to_string(),
            instance: instance.to_string(),
        }
    }

    fn bad_request(title: &'static str, detail: &str) -> Self {
        Self::with_status(StatusCode::BAD_REQUEST, title, detail)
    }

    fn unprocessable(title: &'static str, detail: &str) -> Self {
        Self::with_status(StatusCode::UNPROCESSABLE_ENTITY, title, detail)
    }

    fn internal(title: &'static str, detail: &str) -> Self {
        Self::with_status(StatusCode::INTERNAL_SERVER_ERROR, title, detail)
    }

    fn with_status(status: StatusCode, title: &'static str, detail: &str) -> Self {
        Self {
            r#type: "about:blank",
            title,
            status: status.as_u16(),
            detail: detail.to_string(),
            instance: "/extract".to_string(),
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

#[cfg(test)]
mod tests {
    use axum::{
        body::to_bytes,
        http::{StatusCode, header::CONTENT_TYPE},
        response::IntoResponse,
    };
    use serde_json::Value;

    use super::ProblemDetails;
    use crate::domain::DomainError;

    async fn body_json(response: axum::response::Response) -> Value {
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn base64_decode_error_maps_to_400_bad_request() {
        let problem = ProblemDetails::from_domain(DomainError::Base64Decode);
        let response = problem.clone().into_response();
        let json = body_json(response).await;

        assert_eq!(problem.status, 400);
        assert_eq!(json["status"], 400);
        assert_eq!(json["type"], "about:blank");
        assert!(json["title"].is_string());
        assert!(json["detail"].is_string());
        assert!(json["instance"].is_string());
    }

    #[tokio::test]
    async fn invalid_signature_error_maps_to_400_bad_request() {
        let problem = ProblemDetails::from_domain(DomainError::InvalidPdfSignature);
        let response = problem.clone().into_response();
        let json = body_json(response).await;

        assert_eq!(problem.status, 400);
        assert_eq!(json["status"], 400);
    }

    #[tokio::test]
    async fn pdf_parse_error_maps_to_422_unprocessable() {
        let problem = ProblemDetails::from_domain(DomainError::PdfParse);
        let response = problem.clone().into_response();
        let json = body_json(response).await;

        assert_eq!(problem.status, 422);
        assert_eq!(json["status"], 422);
    }

    #[tokio::test]
    async fn extraction_error_maps_to_500_internal_error() {
        let problem = ProblemDetails::from_domain(DomainError::Extraction);
        let response = problem.clone().into_response();
        let json = body_json(response).await;

        assert_eq!(problem.status, 500);
        assert_eq!(json["status"], 500);
    }

    #[tokio::test]
    async fn every_response_uses_problem_json_content_type_and_matching_status() {
        let cases = [
            (
                ProblemDetails::from_domain(DomainError::Base64Decode),
                StatusCode::BAD_REQUEST,
            ),
            (
                ProblemDetails::from_domain(DomainError::InvalidPdfSignature),
                StatusCode::BAD_REQUEST,
            ),
            (
                ProblemDetails::from_domain(DomainError::PdfParse),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                ProblemDetails::from_domain(DomainError::Extraction),
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
            (
                ProblemDetails::payload_too_large(1024),
                StatusCode::PAYLOAD_TOO_LARGE,
            ),
            (ProblemDetails::not_found(), StatusCode::NOT_FOUND),
        ];

        for (problem, expected_status) in cases {
            let response = problem.into_response();

            assert_eq!(response.status(), expected_status);
            assert_eq!(response.headers()[CONTENT_TYPE], "application/problem+json");
        }
    }

    #[tokio::test]
    async fn payload_too_large_reports_the_configured_limit() {
        let problem = ProblemDetails::payload_too_large(52428800);
        let response = problem.clone().into_response();
        let json = body_json(response).await;

        assert_eq!(problem.status, 413);
        assert_eq!(json["status"], 413);
        assert_eq!(json["title"], "Payload Too Large");
        assert!(json["detail"].is_string());
        assert!(json["detail"].to_string().contains("52428800"));
    }

    #[tokio::test]
    async fn not_found_has_404_status_and_title() {
        let problem = ProblemDetails::not_found();
        let response = problem.clone().into_response();
        let json = body_json(response).await;

        assert_eq!(problem.status, 404);
        assert_eq!(json["status"], 404);
        assert_eq!(json["title"], "Not Found");
    }
}
