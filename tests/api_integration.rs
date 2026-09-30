use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
    response::Response,
};
use extract::{
    api::router::build_router,
    config::{Config, Extractor},
    domain::test_support,
};
use serde_json::Value;
use tower::ServiceExt;

fn config_with_limit(limit_bytes: usize) -> Config {
    Config {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        body_limit_bytes: limit_bytes,
        thread_count: 2,
        max_decompressed_bytes: 8 * 1024 * 1024,
        extractor: Extractor::Lean,
    }
}

async fn post_extract_raw(app: axum::Router, body: Vec<u8>, content_type: &str) -> Response {
    app.oneshot(
        Request::post("/extract")
            .header(header::CONTENT_TYPE, content_type)
            .body(Body::from(body))
            .unwrap(),
    )
    .await
    .unwrap()
}

async fn post_extract_no_content_type(app: axum::Router, body: Vec<u8>) -> Response {
    app.oneshot(Request::post("/extract").body(Body::from(body)).unwrap())
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

    let response = post_extract_raw(
        app,
        "x".repeat(2048).into_bytes(),
        "application/octet-stream",
    )
    .await;

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
    let body = test_support::valid_pdf_bytes(2);

    let response = post_extract_raw(app, body, "application/pdf").await;

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
async fn extract_accepts_octet_stream_content_type() {
    let app = build_router(config_with_limit(64 * 1024 * 1024));
    let body = test_support::valid_pdf_bytes(2);

    let response = post_extract_raw(app, body, "application/octet-stream").await;

    assert_eq!(response.status(), StatusCode::OK);
    let json = read_json(response).await;
    assert_eq!(json["page_count"], 2);
}

#[tokio::test]
async fn extract_without_content_type_returns_415_problem_json() {
    let app = build_router(config_with_limit(64 * 1024 * 1024));
    let body = test_support::valid_pdf_bytes(2);

    let response = post_extract_no_content_type(app, body).await;

    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "application/problem+json"
    );

    let json = read_json(response).await;
    assert_eq!(json["status"], 415);
    assert_eq!(json["title"], "Unsupported Media Type");
}

#[tokio::test]
async fn extract_with_unsupported_content_type_returns_415_problem_json() {
    let app = build_router(config_with_limit(64 * 1024 * 1024));
    let body = test_support::valid_pdf_bytes(2);

    let response = post_extract_raw(app, body, "text/plain").await;

    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "application/problem+json"
    );

    let json = read_json(response).await;
    assert_eq!(json["status"], 415);
    assert_eq!(json["title"], "Unsupported Media Type");
}

#[tokio::test]
async fn extract_accepts_content_type_with_parameters() {
    let app = build_router(config_with_limit(64 * 1024 * 1024));
    let body = test_support::valid_pdf_bytes(2);

    let response = post_extract_raw(app, body, "application/pdf; charset=binary").await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn extract_non_pdf_raw_body_returns_400_problem_json() {
    let app = build_router(config_with_limit(64 * 1024 * 1024));

    let response = post_extract_raw(
        app,
        b"not a pdf file at all".to_vec(),
        "application/octet-stream",
    )
    .await;

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
    let app = build_router(config_with_limit(64 * 1024 * 1024));

    let response = post_extract_raw(
        app,
        b"%PDF-1.4\nnot a valid pdf body at all\n".to_vec(),
        "application/pdf",
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "application/problem+json"
    );

    let json = read_json(response).await;
    assert_eq!(json["status"], 422);
    assert_eq!(json["title"], "Unprocessable PDF");
}

mod real_socket {
    use std::net::SocketAddr;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::{Config, Value, build_router, config_with_limit, test_support};

    async fn boot(config: Config) -> SocketAddr {
        let app = build_router(config.clone());
        let listener = tokio::net::TcpListener::bind(config.bind_addr)
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        addr
    }

    struct HttpResponse {
        status: u16,
        content_type: String,
        body: String,
    }

    async fn send(addr: SocketAddr, request: String) -> HttpResponse {
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();

        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).await.unwrap();
        let text = String::from_utf8_lossy(&raw).into_owned();

        let (head, body) = text.split_once("\r\n\r\n").unwrap();
        let mut head_lines = head.lines();
        let status: u16 = head_lines
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse()
            .unwrap();
        let content_type = head_lines
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-type:")
                    .map(|value| value.trim().to_string())
            })
            .unwrap_or_default();

        HttpResponse {
            status,
            content_type,
            body: body.to_string(),
        }
    }

    fn get(path: &str) -> String {
        format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
    }

    fn post_raw(path: &str, content_type: &str, body: &str) -> String {
        format!(
            "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    fn post_no_content_type(path: &str, body: &str) -> String {
        format!(
            "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    #[tokio::test]
    async fn real_server_returns_health_json_over_tcp() {
        let addr = boot(config_with_limit(1024)).await;

        let response = send(addr, get("/health")).await;

        assert_eq!(response.status, 200);
        assert_eq!(response.content_type, "application/json");
        assert_eq!(response.body, r#"{"status":"ok"}"#);
    }

    #[tokio::test]
    async fn real_server_extracts_pdf_over_tcp() {
        let addr = boot(config_with_limit(64 * 1024 * 1024)).await;
        let body = String::from_utf8_lossy(&test_support::valid_pdf_bytes(2)).into_owned();

        let response = send(addr, post_raw("/extract", "application/pdf", &body)).await;

        assert_eq!(response.status, 200);
        assert_eq!(response.content_type, "application/json");

        let json: Value = serde_json::from_str(&response.body).unwrap();
        assert_eq!(json["page_count"], 2);
        assert_eq!(json["pages"][0]["page_number"], 1);
        assert!(json["text"].as_str().unwrap().contains("Page 2"));
        assert!(json["duration_ms"].is_number());
    }

    #[tokio::test]
    async fn real_server_requires_content_type_over_tcp() {
        let addr = boot(config_with_limit(64 * 1024 * 1024)).await;
        let body = String::from_utf8_lossy(&test_support::valid_pdf_bytes(2)).into_owned();

        let response = send(addr, post_no_content_type("/extract", &body)).await;

        assert_eq!(response.status, 415);
        assert_eq!(response.content_type, "application/problem+json");

        let json: Value = serde_json::from_str(&response.body).unwrap();
        assert_eq!(json["status"], 415);
        assert_eq!(json["title"], "Unsupported Media Type");
    }

    #[tokio::test]
    async fn real_server_rejects_non_pdf_body_over_tcp() {
        let addr = boot(config_with_limit(1024)).await;

        let response = send(
            addr,
            post_raw(
                "/extract",
                "application/octet-stream",
                "not a pdf file at all",
            ),
        )
        .await;

        assert_eq!(response.status, 400);
        assert_eq!(response.content_type, "application/problem+json");

        let json: Value = serde_json::from_str(&response.body).unwrap();
        assert_eq!(json["status"], 400);
        assert_eq!(json["title"], "Invalid PDF Signature");
    }

    #[tokio::test]
    async fn real_server_rejects_oversized_body_over_tcp() {
        let addr = boot(config_with_limit(1024)).await;

        let response = send(
            addr,
            post_raw("/extract", "application/octet-stream", &"x".repeat(4096)),
        )
        .await;

        assert_eq!(response.status, 413);
        assert_eq!(response.content_type, "application/problem+json");
    }

    #[tokio::test]
    async fn real_server_returns_404_problem_json_over_tcp() {
        let addr = boot(config_with_limit(1024)).await;

        let response = send(addr, get("/does/not/exist")).await;

        assert_eq!(response.status, 404);
        assert_eq!(response.content_type, "application/problem+json");
    }

    #[tokio::test]
    async fn concurrent_real_servers_bind_distinct_ephemeral_ports() {
        let addr_a = boot(config_with_limit(1024)).await;
        let addr_b = boot(config_with_limit(2048)).await;

        assert_ne!(addr_a, addr_b);

        let response_a = send(addr_a, get("/health")).await;
        let response_b = send(addr_b, get("/health")).await;

        assert_eq!(response_a.status, 200);
        assert_eq!(response_b.status, 200);
    }
}
