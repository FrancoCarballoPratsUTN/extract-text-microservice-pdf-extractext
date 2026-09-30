use extract::domain::{
    ExtractedDocument, ParsedPdf, PdfBytes, extract_document, extract_document_lean, parse_pdf,
    test_support,
    test_support::SAMPLE_PDF_PATH,
};
use rayon::ThreadPoolBuilder;

fn parsed(bytes: &[u8]) -> ParsedPdf {
    parse_pdf(bytes).expect("fixture parses")
}

fn pool() -> rayon::ThreadPool {
    ThreadPoolBuilder::new().num_threads(2).build().unwrap()
}

fn bytes_of(pdf: PdfBytes) -> Vec<u8> {
    pdf.as_bytes().to_vec()
}

fn run_both(fixture: PdfBytes) -> (ExtractedDocument, ExtractedDocument) {
    let parsed = parsed(&bytes_of(fixture));
    let p = pool();
    let lopdf = extract_document(&parsed, 1024 * 1024, &p);
    let lean = extract_document_lean(&parsed, 1024 * 1024, &p);
    (lopdf, lean)
}

#[test]
fn lean_matches_lopdf_on_simple_pages() {
    let (lopdf, lean) = run_both(test_support::valid_pdf(3));

    assert_eq!(lean, lopdf);
}

#[test]
fn lean_matches_lopdf_on_broken_page_isolation() {
    let (lopdf, lean) = run_both(test_support::broken_pdf(3, 1));

    assert_eq!(lean.page_count(), lopdf.page_count());
    assert_eq!(lean.pages[0], lopdf.pages[0]);
    assert_eq!(lean.pages[1], lopdf.pages[1]);
    assert_eq!(lean.pages[2], lopdf.pages[2]);
}

#[test]
fn lean_matches_lopdf_on_dense_content() {
    let (lopdf, lean) = run_both(test_support::dense_pdf(10, 4000));

    assert_eq!(lean, lopdf);
}

#[test]
fn lean_preserves_page_numbers_in_order() {
    let (_, lean) = run_both(test_support::valid_pdf(3));

    assert_eq!(lean.pages.len(), 3);
    assert_eq!(lean.pages[0].page_number, 1);
    assert_eq!(lean.pages[1].page_number, 2);
    assert_eq!(lean.pages[2].page_number, 3);
    assert!(lean.text().contains("Page 1"));
    assert!(lean.text().contains("Page 3"));
}

#[test]
fn lean_matches_lopdf_on_the_real_sample_with_nonempty_text() {
    let sample = std::fs::read(SAMPLE_PDF_PATH).expect("sample fixture exists");
    let parsed = parsed(&sample);
    let p = pool();
    let lopdf = extract_document(&parsed, 1024 * 1024, &p);
    let lean = extract_document_lean(&parsed, 1024 * 1024, &p);

    assert_eq!(lean.page_count(), lopdf.page_count());
    assert_eq!(lean.page_count(), 250);
    assert!(!lean.text().is_empty());
    assert_eq!(lean, lopdf);
}
