use lopdf::Document;

use super::{DomainError, PdfBytes, pdf_utils::validate_pdf_signature};

#[derive(Debug)]
pub struct ParsedPdf {
    document: Document,
    page_count: usize,
}

impl ParsedPdf {
    pub fn new(document: Document) -> Self {
        let page_count = document.get_pages().len();
        Self {
            document,
            page_count,
        }
    }

    pub fn page_count(&self) -> usize {
        self.page_count
    }

    pub fn document(&self) -> &Document {
        &self.document
    }

    pub fn into_document(self) -> Document {
        self.document
    }
}

pub fn parse_pdf(pdf: &PdfBytes) -> Result<ParsedPdf, DomainError> {
    validate_pdf_signature(pdf.as_bytes())?;
    let document = Document::load_mem(pdf.as_bytes()).map_err(|_| DomainError::PdfParse)?;
    Ok(ParsedPdf::new(document))
}

#[cfg(test)]
mod tests {
    use super::parse_pdf;
    use crate::domain::{DomainError, PdfBytes, test_support};

    #[test]
    fn parses_valid_pdf_and_reports_page_count() {
        let pdf = test_support::valid_pdf(2);

        let parsed = parse_pdf(&pdf).unwrap();

        assert_eq!(parsed.page_count(), 2);
    }

    #[test]
    fn single_page_pdf_has_page_count_one() {
        let pdf = test_support::valid_pdf(1);

        assert_eq!(parse_pdf(&pdf).unwrap().page_count(), 1);
    }

    #[test]
    fn rejects_bytes_without_pdf_signature() {
        let pdf = PdfBytes::new(b"not a pdf file".to_vec());

        assert!(matches!(
            parse_pdf(&pdf),
            Err(DomainError::InvalidPdfSignature)
        ));
    }

    #[test]
    fn rejects_corrupt_pdf_with_valid_signature() {
        let pdf = PdfBytes::new(b"%PDF-1.4\nnot a valid pdf body at all\n".to_vec());

        assert!(matches!(parse_pdf(&pdf), Err(DomainError::PdfParse)));
    }
}
