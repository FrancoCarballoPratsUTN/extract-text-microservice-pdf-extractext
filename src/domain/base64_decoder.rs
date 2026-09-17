use base64_simd::{Out, STANDARD};

use super::{DomainError, PdfBytes};

fn decode_base64_vec(payload: &[u8]) -> Result<Vec<u8>, DomainError> {
    let size = STANDARD
        .decoded_length(payload)
        .map_err(|_| DomainError::Base64Decode)?;
    let mut bytes = vec![0u8; size];
    STANDARD
        .decode(payload, Out::from_slice(bytes.as_mut_slice()))
        .map_err(|_| DomainError::Base64Decode)?;
    Ok(bytes)
}

pub fn decode_base64(payload: &[u8]) -> Result<PdfBytes, DomainError> {
    decode_base64_vec(payload).map(PdfBytes::new)
}

#[cfg(test)]
mod tests {
    use super::{decode_base64, decode_base64_vec};
    use crate::domain::DomainError;

    #[test]
    fn decodes_known_vector_to_exact_original_bytes() {
        let pdf = decode_base64(b"SGVsbG8sIFdvcmxkIQ==").unwrap();

        assert_eq!(pdf.as_bytes(), b"Hello, World!");
    }

    #[test]
    fn rejects_invalid_base64_without_panicking() {
        let result = decode_base64(b"!!!not base64!!!");

        assert_eq!(result, Err(DomainError::Base64Decode));
    }

    #[test]
    fn preallocates_exact_capacity_matching_decoded_length() {
        let decoded = decode_base64_vec(b"SGVsbG8sIFdvcmxkIQ==").unwrap();

        assert_eq!(decoded.len(), b"Hello, World!".len());
        assert_eq!(decoded.capacity(), b"Hello, World!".len());
    }
}
