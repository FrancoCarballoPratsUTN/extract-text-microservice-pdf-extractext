pub mod base64_decoder;
pub mod model;
pub mod page_extractor;
pub mod pdf_parser;
pub mod pdf_utils;
pub mod test_support;

pub use base64_decoder::decode_base64;
pub use model::{DomainError, ExtractedDocument, PageText, PdfBytes};
pub use page_extractor::{extract_document, extract_page};
pub use pdf_parser::{ParsedPdf, parse_pdf};
