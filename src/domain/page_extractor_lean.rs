use std::collections::BTreeMap;

use lopdf::{Document, Encoding};

use rayon::prelude::*;

use super::content_scanner::extract_page_text;
use super::{ExtractedDocument, PageText, ParsedPdf};

/// Extracts text from every page of an already-parsed PDF in parallel.
///
/// This mirrors `lopdf::Document::extract_text_with_limit` semantics exactly
/// (same operator handling, same "any hard error empties the page" rule) but
/// resolves the page tree **once** instead of once per page, which is the
/// repeated `Document::get_pages()` walk the stock per-page loop performs on
/// every page call.
pub fn extract_document_lean(
    parsed: &ParsedPdf,
    limit: usize,
    pool: &rayon::ThreadPool,
) -> ExtractedDocument {
    let document = parsed.document();
    let pages = document.get_pages();

    let pages = pool.install(|| {
        (1..=parsed.page_count() as u32)
            .into_par_iter()
            .map(|page_number| PageText {
                page_number,
                text: extract_page_lean(document, &pages, page_number, limit),
            })
            .collect()
    });

    ExtractedDocument { pages }
}

fn extract_page_lean(
    document: &Document,
    pages: &BTreeMap<u32, (u32, u16)>,
    page_number: u32,
    limit: usize,
) -> String {
    let Some(page_id) = pages.get(&page_number).copied() else {
        return String::new();
    };

    // Any hard error (fonts, content decoding, or a text operation) empties the
    // page, matching `extract_text_with_limit`'s chunk-error propagation.
    let Ok(fonts) = document.get_page_fonts(page_id) else {
        return String::new();
    };

    let mut page_error = false;
    let mut encodings: BTreeMap<Vec<u8>, Encoding> = BTreeMap::new();
    for (name, font) in fonts {
        match font.get_font_encoding_with_limit(document, limit) {
            Ok(encoding) => {
                encodings.insert(name, encoding);
            }
            Err(_) => page_error = true,
        }
    }

    let Ok(content_data) = document.get_page_content_with_limit(page_id, limit) else {
        return String::new();
    };

    // The scanner mirrors `Content::decode` + the operator loop below it, so a
    // decode failure (malformed inline image) still empties the page.
    if page_error {
        return String::new();
    }
    extract_page_text(&content_data, &encodings)
}

#[cfg(test)]
mod tests {
    use rayon::{ThreadPool, ThreadPoolBuilder};

    use super::extract_document_lean;
    use crate::domain::{extract_document, parse_pdf, test_support};

    fn pool() -> ThreadPool {
        ThreadPoolBuilder::new().num_threads(2).build().unwrap()
    }

    fn both(fixture: crate::domain::PdfBytes) -> (String, String) {
        let parsed = parse_pdf(fixture.as_bytes()).unwrap();
        let pool = pool();
        let lopdf = extract_document(&parsed, 1024 * 1024, &pool).text();
        let lean = extract_document_lean(&parsed, 1024 * 1024, &pool).text();
        (lopdf, lean)
    }

    #[test]
    fn lean_matches_lopdf_on_simple_pages() {
        let (lopdf, lean) = both(test_support::valid_pdf(3));
        assert_eq!(lean, lopdf);
    }

    #[test]
    fn lean_matches_lopdf_on_dense_content() {
        let (lopdf, lean) = both(test_support::dense_pdf(10, 4000));
        assert_eq!(lean, lopdf);
    }
}
