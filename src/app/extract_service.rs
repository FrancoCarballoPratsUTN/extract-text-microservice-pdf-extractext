use std::sync::Arc;

use axum::body::Bytes;
use rayon::ThreadPool;

use crate::{
    config::Extractor,
    domain::{DomainError, ExtractedDocument, extract_document, extract_document_lean, parse_pdf},
};

use super::state::AppState;

pub async fn extract(payload: Bytes, state: &AppState) -> Result<ExtractedDocument, DomainError> {
    let bytes = payload.len();
    let max_decompressed_bytes = state.config.max_decompressed_bytes;
    let extractor = state.config.extractor;
    let pool = Arc::clone(&state.pool);

    let span = tracing::info_span!(
        "extract",
        bytes,
        page_count = tracing::field::Empty,
        parse_ms = tracing::field::Empty,
        extract_ms = tracing::field::Empty,
        duration_ms = tracing::field::Empty,
    );

    let outcome = tokio::task::spawn_blocking(move || {
        span.in_scope(|| {
            run_extraction(
                payload.as_ref(),
                pool,
                max_decompressed_bytes,
                extractor,
                &span,
            )
        })
    })
    .await;
    match outcome {
        Ok(result) => result,
        Err(join_error) => {
            tracing::error!(%join_error, "blocking extraction task failed");
            Err(DomainError::Extraction)
        }
    }
}

fn run_extraction(
    payload: &[u8],
    pool: Arc<ThreadPool>,
    max_decompressed_bytes: usize,
    extractor: Extractor,
    span: &tracing::Span,
) -> Result<ExtractedDocument, DomainError> {
    let started = std::time::Instant::now();
    let result = (|| {
        let parsed_started = std::time::Instant::now();
        let parsed = parse_pdf(payload)?;
        let parse_ms = parsed_started.elapsed().as_millis() as u64;
        let extract_started = std::time::Instant::now();
        let document = match extractor {
            Extractor::Lean => extract_document_lean(&parsed, max_decompressed_bytes, &pool),
            Extractor::Lopdf => extract_document(&parsed, max_decompressed_bytes, &pool),
        };
        let extract_ms = extract_started.elapsed().as_millis() as u64;
        span.record("parse_ms", parse_ms);
        span.record("extract_ms", extract_ms);
        Ok(document)
    })();
    let duration_ms = started.elapsed().as_millis() as u64;

    if let Ok(document) = &result {
        span.record("page_count", document.page_count());
    }
    span.record("duration_ms", duration_ms);
    result
}

#[cfg(test)]
mod tests {
    use crate::{
        app::{extract_service::extract, state::AppState},
        config::{Config, Extractor},
        domain::{DomainError, test_support},
    };

    fn test_state() -> AppState {
        let config = Config {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            body_limit_bytes: 1024,
            thread_count: 2,
            max_decompressed_bytes: 1024 * 1024,
            extractor: Extractor::Lean,
        };
        AppState::new(config)
    }

    #[tokio::test]
    async fn extracts_text_from_every_page_in_order() {
        let document = extract(
            axum::body::Bytes::from(test_support::valid_pdf_bytes(3)),
            &test_state(),
        )
        .await
        .unwrap();

        assert_eq!(document.page_count(), 3);
        assert_eq!(document.pages[0].page_number, 1);
        assert_eq!(document.pages[1].page_number, 2);
        assert_eq!(document.pages[2].page_number, 3);
        assert!(document.pages[0].text.contains("Page 1"));
        assert!(document.pages[1].text.contains("Page 2"));
        assert!(document.pages[2].text.contains("Page 3"));
    }

    #[tokio::test]
    async fn rejects_payload_without_pdf_signature() {
        let payload = b"not a pdf file at all".to_vec();

        let result = extract(axum::body::Bytes::from(payload), &test_state()).await;

        assert_eq!(result, Err(DomainError::InvalidPdfSignature));
    }

    #[tokio::test]
    async fn rejects_corrupt_pdf_with_valid_signature() {
        let payload = b"%PDF-1.4\nnot a valid pdf body at all\n".to_vec();

        let result = extract(axum::body::Bytes::from(payload), &test_state()).await;

        assert_eq!(result, Err(DomainError::PdfParse));
    }

    #[tokio::test]
    async fn isolates_unreadable_page_without_aborting_extraction() {
        let document = extract(
            axum::body::Bytes::from(test_support::pdf_with_unreadable_page(3, 1)),
            &test_state(),
        )
        .await
        .unwrap();

        assert_eq!(document.page_count(), 3);
        assert!(document.pages[0].text.contains("Page 1"));
        assert_eq!(document.pages[1].text, "");
        assert!(document.pages[2].text.contains("Page 3"));
    }
}
