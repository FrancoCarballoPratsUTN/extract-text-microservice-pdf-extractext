use serde::{Deserialize, Serialize};

use crate::domain::{ExtractedDocument, PageText};

#[derive(Debug, Serialize)]
pub struct HealthStatus {
    pub status: &'static str,
}

#[derive(Debug, Deserialize)]
pub struct ExtractRequest {
    pub document_base64: String,
}

#[derive(Debug, Serialize)]
pub struct ExtractResponse {
    pub page_count: usize,
    pub pages: Vec<ExtractPage>,
    pub text: String,
    pub duration_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct ExtractPage {
    pub page_number: u32,
    pub text: String,
}

impl ExtractResponse {
    pub fn from_extracted(document: ExtractedDocument, duration_ms: u64) -> Self {
        let page_count = document.page_count();
        let text = document.text();
        let pages = document.pages.into_iter().map(ExtractPage::from).collect();
        Self {
            page_count,
            pages,
            text,
            duration_ms,
        }
    }
}

impl From<PageText> for ExtractPage {
    fn from(page: PageText) -> Self {
        Self {
            page_number: page.page_number,
            text: page.text,
        }
    }
}
