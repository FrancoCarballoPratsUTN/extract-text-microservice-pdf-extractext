use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
    response::Response,
};
use extract::{api::router::build_router, config::Config, domain::test_support};
use serde_json::Value;
use tower::ServiceExt;

fn config_with_limit(limit_bytes: usize) -> Config {
    Config {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        body_limit_bytes: limit_bytes,
        thread_count: 2,
        max_decompressed_bytes: 64 * 1024 * 1024,
    }
}

fn base64_payload(pdf: Vec<u8>) -> String {
    base64_simd::STANDARD.encode_to_string(&pdf)
}

async fn post_extract(app: axum::Router, body: String) -> Response {
    app.oneshot(
        Request::post("/extract")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .unwrap(),
    )
    .await
    .unwrap()
}

async fn read_json(response: Response) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn health_returns_200_with_json_status_ok() {
    let app = build_router(config_with_limit(1024));

    let response = app
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, serde_json::json!({ "status": "ok" }));
}

#[tokio::test]
async fn oversized_body_returns_413_problem_json() {
    let app = build_router(config_with_limit(1024));

    let response = app
        .oneshot(
            Request::post("/extract")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("x".repeat(2048)))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "application/problem+json"
    );

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["type"], "about:blank");
    assert_eq!(json["title"], "Payload Too Large");
    assert_eq!(json["status"], 413);
    assert!(json["detail"].is_string());
}

#[tokio::test]
async fn unknown_route_returns_404_problem_json_with_instance() {
    let app = build_router(config_with_limit(1024));

    let response = app
        .oneshot(Request::get("/nonexistent").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "application/problem+json"
    );

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], 404);
    assert_eq!(json["title"], "Not Found");
    assert_eq!(json["instance"], "/nonexistent");
}

#[tokio::test]
async fn extract_valid_pdf_returns_200_with_document_json() {
    let app = build_router(config_with_limit(64 * 1024 * 1024));
    let body = serde_json::json!({
        "document_base64": base64_payload(test_support::valid_pdf_bytes(2))
    });

    let response = post_extract(app, body.to_string()).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");

    let json = read_json(response).await;
    assert_eq!(json["page_count"], 2);
    assert_eq!(json["pages"][0]["page_number"], 1);
    assert_eq!(json["pages"][1]["page_number"], 2);
    assert!(
        json["pages"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Page 1")
    );
    assert!(
        json["pages"][1]["text"]
            .as_str()
            .unwrap()
            .contains("Page 2")
    );
    assert!(json["text"].as_str().unwrap().contains("Page 2"));
    assert!(json["duration_ms"].is_number());
}

#[tokio::test]
async fn extract_malformed_base64_returns_400_problem_json() {
    let app = build_router(config_with_limit(1024));
    let body = serde_json::json!({ "document_base64": "!!!not base64!!!" });

    let response = post_extract(app, body.to_string()).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "application/problem+json"
    );

    let json = read_json(response).await;
    assert_eq!(json["status"], 400);
    assert_eq!(json["title"], "Invalid Base64");
}

#[tokio::test]
async fn extract_payload_without_pdf_signature_returns_400_problem_json() {
    let app = build_router(config_with_limit(1024));
    let body = serde_json::json!({
        "document_base64": base64_payload(b"not a pdf file at all".to_vec())
    });

    let response = post_extract(app, body.to_string()).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "application/problem+json"
    );

    let json = read_json(response).await;
    assert_eq!(json["status"], 400);
    assert_eq!(json["title"], "Invalid PDF Signature");
}

#[tokio::test]
async fn extract_corrupt_pdf_returns_422_problem_json() {
    let app = build_router(config_with_limit(1024));
    let body = serde_json::json!({
        "document_base64": base64_payload(b"%PDF-1.4\nnot a valid pdf body at all\n".to_vec())
    });

    let response = post_extract(app, body.to_string()).await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "application/problem+json"
    );

    let json = read_json(response).await;
    assert_eq!(json["status"], 422);
    assert_eq!(json["title"], "Unprocessable PDF");
}

#[tokio::test]
async fn extract_malformed_json_body_returns_400_problem_json() {
    let app = build_router(config_with_limit(1024));

    let response = post_extract(app, "not json at all".to_string()).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "application/problem+json"
    );

    let json = read_json(response).await;
    assert_eq!(json["status"], 400);
    assert_eq!(json["title"], "Invalid Request Body");
}
