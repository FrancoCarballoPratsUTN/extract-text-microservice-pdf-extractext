pub mod base64_decoder;
pub mod model;

pub use base64_decoder::decode_base64;
pub use model::{DomainError, ExtractedDocument, PageText, PdfBytes};
