use std::{fs, path::Path};

use extract::domain::test_support::{DENSE_PDF_PATH, dense_pdf_bytes};

fn main() {
    let out = Path::new(DENSE_PDF_PATH);

    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).expect("fixture directory should be creatable");
    }

    let bytes = dense_pdf_bytes(500, 24_000);
    fs::write(out, &bytes).expect("fixture should be writable");
    eprintln!("wrote {}: {} bytes", out.display(), bytes.len());
}
