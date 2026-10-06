//! Regression coverage for failures reproduced with the installed payload.
use crate::{paths, speech};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

#[test]
fn startup_skips_large_documents_without_disabling_full_graph_indexing() {
    let directory = tempfile::tempdir().unwrap();
    let workspace = directory.path().join("documents");
    std::fs::create_dir_all(&workspace).unwrap();
    let file = workspace.join("large.md");
    std::fs::write(&file, format!("# Large document\n{}", "ordinary text\n".repeat(11000))).unwrap();
    let paths = paths::AppPaths::with_dirs(&directory.path().join("data"), &workspace, &directory.path().join("assets"));
    let app = crate::App::bootstrap(paths).unwrap();
    assert_eq!(app.store.stats().unwrap()["docs"], 0);
    let indexed = crate::content::reindex_workspace(&app, 4000).unwrap();
    assert_eq!(indexed["indexed"], 1);
    assert_eq!(app.store.stats().unwrap()["docs"], 1);
    assert_eq!(std::fs::read_to_string(&file).unwrap().lines().count(), 11001);
}

#[test]
fn pdf_cancel_does_not_enter_a_parser_or_create_assets() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("scan.pdf");
    std::fs::write(&file, b"%PDF-1.7\nmalformed input must never be parsed").unwrap();
    let result = speech::with_cancel(Arc::new(|| true), || crate::convert::convert_triple(file.to_str().unwrap(), true));
    assert!(result.text.is_empty());
    assert_eq!(result.error.as_deref(), Some("cancelled"));
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn damaged_office_containers_never_fall_back_to_plaintext_success() {
    let directory = tempfile::tempdir().unwrap();
    for extension in ["docx", "xlsx", "pptx"] {
        let file = directory.path().join(format!("damaged.{extension}"));
        std::fs::write(&file, b"not a ZIP: this text is not an Office document").unwrap();
        let result = crate::convert::convert_triple(file.to_str().unwrap(), true);
        assert!(result.text.is_empty(), "{extension} decoded corrupt bytes as content");
        assert!(result.error.is_some());
    }
}

#[test]
fn exported_html_reimports_its_markdown_without_running_javascript() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("export.html");
    let markdown = "# Export round trip\n\nChinese 中文 & special < characters.\n\n| A | B |\n|---|---|\n| 1 | 2 |\n";
    crate::mdexport::export_html(markdown, "", file.to_str().unwrap(), &serde_json::json!({}), "round trip", directory.path()).unwrap();
    let result = crate::convert::convert_triple(file.to_str().unwrap(), true);
    assert!(result.error.is_none());
    assert!(result.text.contains("Export round trip"));
    assert!(result.text.contains("中文"));
    assert!(result.text.contains("| 1 | 2 |"));
}

#[test]
fn cancellation_reaches_native_worker_after_the_calling_scope_ends() {
    let flag = Arc::new(AtomicBool::new(false));
    let worker_flag = flag.clone();
    let check = speech::with_cancel(Arc::new(move || worker_flag.load(Ordering::SeqCst)), speech::cancellation_check);
    assert!(!speech::cancelled());
    flag.store(true, Ordering::SeqCst);
    assert!(std::thread::spawn(move || check()).join().unwrap());
}

#[test]
fn assets_entry_root_works_for_absolute_and_relative_install_directories() {
    let directory = tempfile::tempdir().unwrap();
    let assets = directory.path().join("ReadMD/assets");
    std::fs::create_dir_all(&assets).unwrap();
    std::fs::write(assets.join("index.html"), b"<head></head>").unwrap();
    assert_eq!(paths::resolve_assets_directory(&assets.join(".")), paths::canonicalize_or_clean(&assets));
}

#[cfg(unix)]
#[test]
fn an_index_symlink_does_not_grant_access_to_an_external_directory() {
    let directory = tempfile::tempdir().unwrap();
    let assets = directory.path().join("assets");
    let outside = directory.path().join("private");
    std::fs::create_dir_all(&assets).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("index.html"), b"private").unwrap();
    std::os::unix::fs::symlink(outside.join("index.html"), assets.join("index.html")).unwrap();
    assert_eq!(paths::resolve_assets_directory(&assets), paths::canonicalize_or_clean(&assets));
}

#[cfg(windows)]
#[test]
fn static_windows_roots_ignore_case_but_keep_the_directory_boundary() {
    use std::path::Path;
    assert!(paths::path_starts_with(Path::new(r"C:\ReadMD\assets\boot.js"), Path::new(r"c:\readmd\assets")));
    assert!(!paths::path_starts_with(Path::new(r"C:\ReadMD\assets-private\secret"), Path::new(r"c:\readmd\assets")));
}
