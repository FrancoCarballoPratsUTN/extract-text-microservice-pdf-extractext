#[derive(Debug, PartialEq, Eq)]
pub struct PdfBytes {
    bytes: Vec<u8>,
}

impl PdfBytes {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum DomainError {
    Base64Decode,
    InvalidPdfSignature,
    PdfParse,
    Extraction,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PageText {
    pub page_number: u32,
    pub text: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ExtractedDocument {
    pub pages: Vec<PageText>,
}

impl ExtractedDocument {
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn text(&self) -> String {
        let capacity: usize = self.pages.iter().map(|page| page.text.len()).sum();
        let mut text = String::with_capacity(capacity);
        for page in &self.pages {
            text.push_str(&page.text);
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::{DomainError, ExtractedDocument, PageText, PdfBytes};

    fn sample_document() -> ExtractedDocument {
        ExtractedDocument {
            pages: vec![
                PageText {
                    page_number: 1,
                    text: "hola ".to_string(),
                },
                PageText {
                    page_number: 2,
                    text: "mundo".to_string(),
                },
            ],
        }
    }

    #[test]
    fn page_count_matches_number_of_pages() {
        let document = sample_document();

        assert_eq!(document.page_count(), 2);
    }

    #[test]
    fn text_concatenates_pages_in_page_number_order() {
        let document = sample_document();

        assert_eq!(document.text(), "hola mundo");
    }

    #[test]
    fn empty_document_has_zero_pages_and_empty_text() {
        let document = ExtractedDocument { pages: Vec::new() };

        assert_eq!(document.page_count(), 0);
        assert_eq!(document.text(), "");
    }

    #[test]
    fn pdf_bytes_exposes_decoded_bytes() {
        let pdf = PdfBytes::new(vec![0x25, b'P', b'D', b'F']);

        assert_eq!(pdf.as_bytes(), &[0x25, b'P', b'D', b'F']);
    }

    #[test]
    fn domain_error_variants_are_equatable_and_distinct() {
        assert_eq!(DomainError::Base64Decode, DomainError::Base64Decode);
        assert_ne!(DomainError::Base64Decode, DomainError::InvalidPdfSignature);
        assert_ne!(DomainError::PdfParse, DomainError::Extraction);
    }
}
