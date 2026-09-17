use std::sync::Arc;

use rayon::ThreadPool;

use crate::domain::{DomainError, ExtractedDocument, decode_base64, extract_document, parse_pdf};

use super::state::AppState;

pub async fn extract(payload: Vec<u8>, state: &AppState) -> Result<ExtractedDocument, DomainError> {
    let bytes = payload.len();
    let max_decompressed_bytes = state.config.max_decompressed_bytes;
    let pool = Arc::clone(&state.pool);

    let span = tracing::info_span!(
        "extract",
        bytes,
        page_count = tracing::field::Empty,
        duration_ms = tracing::field::Empty,
    );

    let outcome = tokio::task::spawn_blocking(move || {
        span.in_scope(|| run_extraction(payload, pool, max_decompressed_bytes, &span))
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
    payload: Vec<u8>,
    pool: Arc<ThreadPool>,
    max_decompressed_bytes: usize,
    span: &tracing::Span,
) -> Result<ExtractedDocument, DomainError> {
    let started = std::time::Instant::now();
    let result = (|| {
        let pdf = decode_base64(&payload)?;
        let parsed = parse_pdf(&pdf)?;
        Ok(extract_document(&parsed, max_decompressed_bytes, &pool))
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
        config::Config,
        domain::{DomainError, test_support},
    };

    fn test_state() -> AppState {
        let config = Config {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            body_limit_bytes: 1024,
            thread_count: 1,
            max_decompressed_bytes: 1024 * 1024,
        };
        AppState::new(config)
    }

    fn payload_for(pdf: Vec<u8>) -> Vec<u8> {
        base64_simd::STANDARD.encode_to_string(&pdf).into_bytes()
    }

    #[tokio::test]
    async fn extracts_text_from_every_page_in_order() {
        let state = test_state();
        let payload = payload_for(test_support::valid_pdf_bytes(3));

        let document = extract(payload, &state).await.unwrap();

        assert_eq!(document.page_count(), 3);
        assert_eq!(document.pages[0].page_number, 1);
        assert_eq!(document.pages[1].page_number, 2);
        assert_eq!(document.pages[2].page_number, 3);
        assert!(document.pages[0].text.contains("Page 1"));
        assert!(document.pages[1].text.contains("Page 2"));
        assert!(document.pages[2].text.contains("Page 3"));
    }

    #[tokio::test]
    async fn rejects_invalid_base64_payload() {
        let state = test_state();

        let result = extract(b"!!!not base64!!!".to_vec(), &state).await;

        assert_eq!(result, Err(DomainError::Base64Decode));
    }

    #[tokio::test]
    async fn rejects_payload_without_pdf_signature() {
        let state = test_state();
        let payload = base64_simd::STANDARD
            .encode_to_string(b"not a pdf file at all")
            .into_bytes();

        let result = extract(payload, &state).await;

        assert_eq!(result, Err(DomainError::InvalidPdfSignature));
    }

    #[tokio::test]
    async fn rejects_corrupt_pdf_with_valid_signature() {
        let state = test_state();
        let payload = base64_simd::STANDARD
            .encode_to_string(b"%PDF-1.4\nnot a valid pdf body at all\n")
            .into_bytes();

        let result = extract(payload, &state).await;

        assert_eq!(result, Err(DomainError::PdfParse));
    }

    #[tokio::test]
    async fn isolates_unreadable_page_without_aborting_extraction() {
        let state = test_state();
        let payload = payload_for(test_support::pdf_with_unreadable_page(3, 1));

        let document = extract(payload, &state).await.unwrap();

        assert_eq!(document.page_count(), 3);
        assert!(document.pages[0].text.contains("Page 1"));
        assert_eq!(document.pages[1].text, "");
        assert!(document.pages[2].text.contains("Page 3"));
    }
}
