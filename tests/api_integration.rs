use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use extract::{api::router::build_router, config::Config};
use tower::ServiceExt;

fn config_with_limit(limit_bytes: usize) -> Config {
    Config {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        body_limit_bytes: limit_bytes,
        thread_count: 2,
        max_decompressed_bytes: 64 * 1024 * 1024,
    }
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
