// ReadMD's narrow native entry point for the pinned Office layout source build.
// The document model, pagination, painting and vector PDF writer are upstream.
use std::path::{Path, PathBuf};

const VERSION: &str = "readmd-office-render office-layout 0.8.6 96e218055a41261c3c12c0ea1b07f143424f368d";

fn run() -> Result<(), &'static str> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == "--version" {
        println!("{VERSION}");
        return Ok(());
    }
    if args.len() != 4 || args[0] != "--input" || args[2] != "--output" {
        return Err("expected --input ABSOLUTE_DOCX_OR_PPTX --output ABSOLUTE_PDF");
    }
    let input = PathBuf::from(&args[1]);
    let output = PathBuf::from(&args[3]);
    validate_paths(&input, &output)?;
    ibis_office_app::readmd_embed::export_pdf(&input, &output)
        .map_err(|_| "the Office layout engine could not render this document")?;
    Ok(())
}

fn validate_paths(input: &Path, output: &Path) -> Result<(), &'static str> {
    if !input.is_absolute() || !output.is_absolute() {
        return Err("input and output must be absolute");
    }
    let ext = input.extension().and_then(|v| v.to_str()).unwrap_or("");
    if !matches!(ext, "docx" | "pptx") || output != input.with_extension("pdf") {
        return Err("output must be beside the DOCX/PPTX snapshot with the same stem");
    }
    let source = std::fs::symlink_metadata(input).map_err(|_| "input is unavailable")?;
    if !source.is_file() || source.len() == 0 || source.len() > 64 * 1024 * 1024 {
        return Err("input must be a regular document within 64 MiB");
    }
    // The host may precreate this empty file and keep its handle open. The
    // upstream writer truncates it in place; no atomic replacement or rename.
    match std::fs::symlink_metadata(output) {
        Ok(stat) if stat.is_file() && stat.len() == 0 => {}
        Ok(_) => return Err("output must be absent or a regular empty file"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("output is unavailable"),
    }
    Ok(())
}

fn main() {
    if let Err(message) = run() {
        eprintln!("readmd-office-render: {message}");
        std::process::exit(1);
    }
}
