use std::env;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    if env::args().any(|arg| arg == "--version") {
        println!("extract {VERSION}");
        return;
    }

    println!("extract {VERSION} · scaffold only: HTTP server not yet wired");
}
