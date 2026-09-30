use lopdf::Document;
use rayon::prelude::*;

use super::{ExtractedDocument, PageText, ParsedPdf};

pub fn extract_page(document: &Document, page_number: u32, limit: usize) -> PageText {
    let text = document
        .extract_text_with_limit(&[page_number], limit)
        .unwrap_or_default();
    PageText { page_number, text }
}

pub fn extract_document(
    parsed: &ParsedPdf,
    limit: usize,
    pool: &rayon::ThreadPool,
) -> ExtractedDocument {
    let pages = pool.install(|| {
        (1..=parsed.page_count() as u32)
            .into_par_iter()
            .map(|page_number| extract_page(parsed.document(), page_number, limit))
            .collect()
    });
    ExtractedDocument { pages }
}

#[cfg(test)]
mod tests {
    use rayon::{ThreadPool, ThreadPoolBuilder};

    use super::extract_document;
    use crate::domain::test_support;

    fn make_pool() -> ThreadPool {
        ThreadPoolBuilder::new().num_threads(2).build().unwrap()
    }

    #[test]
    fn extracts_text_from_every_page_preserving_page_numbers() {
        let pdf = test_support::valid_pdf(3);
        let parsed = crate::domain::parse_pdf(pdf.as_bytes()).unwrap();
        let pool = make_pool();

        let result = extract_document(&parsed, 1024 * 1024, &pool);

        assert_eq!(result.pages.len(), 3);
        assert_eq!(result.pages[0].page_number, 1);
        assert_eq!(result.pages[1].page_number, 2);
        assert_eq!(result.pages[2].page_number, 3);
        assert!(result.pages[0].text.contains("Page 1"));
        assert!(result.pages[1].text.contains("Page 2"));
        assert!(result.pages[2].text.contains("Page 3"));
    }

    #[test]
    fn isolates_unreadable_page_as_empty_text() {
        let pdf = test_support::broken_pdf(3, 1);
        let parsed = crate::domain::parse_pdf(pdf.as_bytes()).unwrap();
        let pool = make_pool();

        let result = extract_document(&parsed, 1024 * 1024, &pool);

        assert_eq!(result.pages.len(), 3);
        assert!(result.pages[0].text.contains("Page 1"));
        assert_eq!(result.pages[1].text, "");
        assert!(result.pages[2].text.contains("Page 3"));
    }

    #[test]
    fn extracts_dense_content_from_every_page_in_order() {
        let pdf = test_support::dense_pdf(10, 4000);
        let parsed = crate::domain::parse_pdf(pdf.as_bytes()).unwrap();
        let pool = make_pool();

        let result = extract_document(&parsed, 1024 * 1024, &pool);

        assert_eq!(result.pages.len(), 10);
        for page in &result.pages {
            assert!(page.text.contains(&format!("Page {}", page.page_number)));
            assert!(
                page.text.len() > 2000,
                "dense page {} too short ({} chars)",
                page.page_number,
                page.text.len()
            );
        }
        assert_eq!(result.pages[0].page_number, 1);
        assert_eq!(result.pages[9].page_number, 10);
    }

    #[test]
    fn extraction_under_one_second_for_dense_ten_page_fixture() {
        let pdf = test_support::dense_pdf(10, 4000);
        let parsed = crate::domain::parse_pdf(pdf.as_bytes()).unwrap();
        let pool = make_pool();

        let started = std::time::Instant::now();
        let result = extract_document(&parsed, 1024 * 1024, &pool);
        let elapsed = started.elapsed();

        assert_eq!(result.pages.len(), 10);
        assert!(
            elapsed.as_millis() < 1000,
            "dense extraction took {}ms",
            elapsed.as_millis()
        );
    }
}

