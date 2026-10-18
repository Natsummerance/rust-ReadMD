//! One bounded request per process. The desktop supervisor owns its lifetime.
#[path = "../pdfium_worker.rs"]
mod worker;
fn main() {
    if let Err(code) = worker::run() {
        // Do not log document paths, passwords or content.
        eprintln!("{code}");
        std::process::exit(1);
    }
}
