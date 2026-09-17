use super::DomainError;

pub fn validate_pdf_signature(bytes: &[u8]) -> Result<(), DomainError> {
    if bytes.starts_with(b"%PDF-") {
        Ok(())
    } else {
        Err(DomainError::InvalidPdfSignature)
    }
}

#[cfg(test)]
mod tests {
    use super::validate_pdf_signature;
    use crate::domain::DomainError;

    #[test]
    fn accepts_valid_pdf_header() {
        assert!(validate_pdf_signature(b"%PDF-1.7\n").is_ok());
    }

    #[test]
    fn rejects_prefix_without_pdf_magic() {
        assert_eq!(
            validate_pdf_signature(b"PK\x03\x04 anything"),
            Err(DomainError::InvalidPdfSignature)
        );
    }
}
