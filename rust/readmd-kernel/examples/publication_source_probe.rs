//! Development-only source projection for diagnosing managed engine output.
//! This does not execute an engine or certify a native/export workflow.
use readmd_kernel::{md_ast, publication, publication_fonts, typst_writer};
use std::{fs, io::Write, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 && args.len() != 5 { return Err("Expected source.md options.json NEW-output-directory [font-profile assets-directory]".into()); }
    let source_path = fs::canonicalize(&args[0])?;
    let options_path = fs::canonicalize(&args[1])?;
    let output = PathBuf::from(&args[2]);
    if !output.is_absolute() || output.exists() { return Err("Output must be a new absolute directory".into()); }
    let source = fs::read_to_string(&source_path)?;
    let options: serde_json::Value = serde_json::from_slice(&fs::read(options_path)?)?;
    // Explicit diagnostic profile only. Keep the resource lease through both
    // preparation and rendering, just as the managed publication path does.
    let _font_scope = if args.len() == 5 {
        let profile = fs::canonicalize(&args[3])?;
        let assets = fs::canonicalize(&args[4])?;
        if !profile.is_dir() || !assets.is_dir() { return Err("Font profile and assets must be directories".into()); }
        let scope = publication_fonts::scope(&profile, &assets);
        if publication_fonts::current().is_none() { return Err("Explicit managed publication fonts are unavailable".into()); }
        Some(scope)
    } else { None };
    let base = source_path.parent().ok_or("Source parent missing")?.to_str().ok_or("Invalid source path")?;
    let prepared = publication::prepare_with_options(&source, base, &options)?;
    let doc = md_ast::parse(&prepared.content);
    fs::create_dir(&output)?;
    let typst = typst_writer::render(&doc, &options, base, &output, "Publication diagnostic")?;
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(output.join("document.typ"))?;
    file.write_all(typst.text.as_bytes())?;
    println!("Generated diagnostic sources only; not native acceptance.");
    Ok(())
}
