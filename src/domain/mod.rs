pub(crate) mod content_scanner;
pub mod model;
pub mod page_extractor;
pub mod page_extractor_lean;
pub mod pdf_parser;
pub mod pdf_utils;
#[doc(hidden)]
pub mod test_support;

pub use model::{DomainError, ExtractedDocument, PageText, PdfBytes};
pub use page_extractor::{extract_document, extract_page};
pub use page_extractor_lean::extract_document_lean;
pub use pdf_parser::{ParsedPdf, parse_pdf};
