use crate::domain::PdfBytes;

pub fn valid_pdf_bytes(page_count: usize) -> Vec<u8> {
    _valid_pdf_bytes(page_count)
}

pub fn pdf_with_unreadable_page(page_count: usize, broken_page: usize) -> Vec<u8> {
    _pdf_with_unreadable_page(page_count, broken_page)
}

pub fn valid_pdf(page_count: usize) -> PdfBytes {
    PdfBytes::new(_valid_pdf_bytes(page_count))
}

pub fn broken_pdf(page_count: usize, broken_page: usize) -> PdfBytes {
    PdfBytes::new(_pdf_with_unreadable_page(page_count, broken_page))
}

fn _valid_pdf_bytes(page_count: usize) -> Vec<u8> {
    let mut bytes = b"%PDF-1.4\n".to_vec();
    let mut objects: Vec<(usize, usize)> = Vec::new();

    fn push_object(num: usize, raw: &[u8], objects: &mut Vec<(usize, usize)>, bytes: &mut Vec<u8>) {
        let offset = bytes.len();
        bytes.extend_from_slice(format!("{num} 0 obj\n").as_bytes());
        bytes.extend_from_slice(raw);
        bytes.extend_from_slice(b"\nendobj\n");
        objects.push((offset, num));
    }

    push_object(
        1,
        b"<< /Type /Catalog /Pages 2 0 R >>",
        &mut objects,
        &mut bytes,
    );

    let kids: Vec<String> = (0..page_count)
        .map(|i| format!("{} 0 R", 3 + 2 * i))
        .collect();
    let pages_raw = format!(
        "<< /Type /Pages /Kids [{}] /Count {page_count} >>",
        kids.join(" ")
    );
    push_object(2, pages_raw.as_bytes(), &mut objects, &mut bytes);

    for i in 0..page_count {
        let page_id = 3 + 2 * i;
        let content_id = 4 + 2 * i;
        let page_raw = format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents {content_id} 0 R \
             /Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >> >>"
        );
        push_object(page_id, page_raw.as_bytes(), &mut objects, &mut bytes);

        let text = format!("BT /F1 12 Tf 72 720 Td (Page {}) Tj ET", i + 1);
        let content_raw = format!(
            "<< /Length {} >>\nstream\n{text}\nendstream",
            text.len() + 1
        );
        push_object(content_id, content_raw.as_bytes(), &mut objects, &mut bytes);
    }

    let size = objects.len() + 1;
    let xref_offset = bytes.len();
    let mut xref = format!("xref\n0 {size}\n0000000000 65535 f \n");
    for (offset, _) in &objects {
        xref.push_str(&format!("{offset:010} 00000 n \n"));
    }
    xref.push_str(&format!(
        "trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n"
    ));
    bytes.extend_from_slice(xref.as_bytes());
    bytes
}

fn _pdf_with_unreadable_page(page_count: usize, broken_page: usize) -> Vec<u8> {
    assert!(broken_page < page_count, "broken_page index out of range");

    let mut bytes = b"%PDF-1.4\n".to_vec();
    let mut objects: Vec<(usize, usize)> = Vec::new();

    fn push_object(num: usize, raw: &[u8], objects: &mut Vec<(usize, usize)>, bytes: &mut Vec<u8>) {
        let offset = bytes.len();
        bytes.extend_from_slice(format!("{num} 0 obj\n").as_bytes());
        bytes.extend_from_slice(raw);
        bytes.extend_from_slice(b"\nendobj\n");
        objects.push((offset, num));
    }

    push_object(
        1,
        b"<< /Type /Catalog /Pages 2 0 R >>",
        &mut objects,
        &mut bytes,
    );

    let kids: Vec<String> = (0..page_count)
        .map(|i| format!("{} 0 R", 3 + 2 * i))
        .collect();
    let pages_raw = format!(
        "<< /Type /Pages /Kids [{}] /Count {page_count} >>",
        kids.join(" ")
    );
    push_object(2, pages_raw.as_bytes(), &mut objects, &mut bytes);

    for i in 0..page_count {
        let page_id = 3 + 2 * i;
        let content_id = 4 + 2 * i;
        let page_raw = format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents {content_id} 0 R \
             /Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >> >>"
        );
        push_object(page_id, page_raw.as_bytes(), &mut objects, &mut bytes);

        if i == broken_page {
            let bad_content =
                b"<< /Length 4 /Filter /FlateDecode >>\nstream\nnot-valid-flate\nendstream";
            push_object(content_id, bad_content, &mut objects, &mut bytes);
        } else {
            let text = format!("BT /F1 12 Tf 72 720 Td (Page {}) Tj ET", i + 1);
            let content_raw = format!(
                "<< /Length {} >>\nstream\n{text}\nendstream",
                text.len() + 1
            );
            push_object(content_id, content_raw.as_bytes(), &mut objects, &mut bytes);
        }
    }

    let size = objects.len() + 1;
    let xref_offset = bytes.len();
    let mut xref = format!("xref\n0 {size}\n0000000000 65535 f \n");
    for (offset, _) in &objects {
        xref.push_str(&format!("{offset:010} 00000 n \n"));
    }
    xref.push_str(&format!(
        "trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n"
    ));
    bytes.extend_from_slice(xref.as_bytes());
    bytes
}
