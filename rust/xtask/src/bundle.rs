//! `assets/readmd.boot.js`: the classic-script sources joined with `\n;\n`,
//! with canonical LF line endings, plus one trailing `\n` when needed.
//! Windows CRLF checkouts and Unix LF checkouts must produce the same bundle.

use std::fs;
use std::path::Path;

/// Load order matters: later files use globals defined by earlier ones.
pub const SOURCES: &[&str] = &[
    "vendor/marked.min.js",
    "js/core/modal.js",
    "js/core/task-feedback.js",
    "js/core/state.js",
    "js/core/i18n.js",
    "js/core/dialog.js",
    "js/core/settings.js",
    "js/core/modules.js",
    "js/core/tabs.js",
    "js/core/history.js",
    "js/core/dragdrop.js",
    "js/reader/formula.js",
    "js/reader/fixes.js",
    "js/reader/toc.js",
    "js/reader/search.js",
    "js/reader/folder.js",
    "js/reader/render.js",
    "js/editor/preview.js",
    "js/editor/image.js",
    "js/editor/md-transforms.js",
    "js/editor/editor.js",
    "js/features/ai.js",
    "js/features/document-history.js",
    "js/features/share.js",
    "js/features/convert.js",
    "js/features/batch.js",
    "js/features/pet-batch.js",
    "js/features/pet-workbench.js",
    "js/features/pet-companion-actions.js",
    "js/features/ocr.js",
    "js/features/web.js",
    "js/features/clipboard.js",
    "js/features/export.js",
    "js/features/updater.js",
    "js/features/graph.js",
    "js/shell/command-palette.js",
    "js/shell/shell.js",
    "js/features/ai-inline.js",
    "js/features/pet-stage.js",
    "app.js",
];

pub fn build(root: &Path) -> Vec<u8> {
    let assets = root.join("assets");
    let chunks: Vec<Vec<u8>> = SOURCES
        .iter()
        .filter_map(|s| fs::read(assets.join(s)).ok())
        .map(|bytes| normalize_crlf(&bytes))
        .collect();
    let mut body = chunks.join(&b"\n;\n"[..]);
    if !matches!(body.last(), Some(b'\n') | Some(b'\r')) {
        body.push(b'\n');
    }
    body
}

fn normalize_crlf(bytes: &[u8]) -> Vec<u8> {
    bytes.iter().enumerate().filter_map(|(i, &byte)| {
        (byte != b'\r' || bytes.get(i + 1) != Some(&b'\n')).then_some(byte)
    }).collect()
}

/// Write (or, with `check`, compare) the bundle.  `Ok(false)` = out of date.
pub fn bundle(root: &Path, check: bool) -> Result<bool, String> {
    let missing: Vec<&str> = SOURCES.iter().copied().filter(|s| !root.join("assets").join(s).is_file()).collect();
    if !missing.is_empty() {
        return Err(format!("missing boot sources: {}", missing.join(", ")));
    }
    let out = root.join("assets").join("readmd.boot.js");
    let body = build(root);
    if check {
        let current = fs::read(&out).unwrap_or_default();
        if normalize_crlf(&current) == body {
            println!("[OK] assets/readmd.boot.js is up to date");
            return Ok(true);
        }
        println!("[FAIL] assets/readmd.boot.js is stale; run `cargo xtask bundle-boot`");
        return Ok(false);
    }
    fs::write(&out, &body).map_err(|e| format!("write {}: {e}", out.display()))?;
    println!("[SYNC] Rebundled assets/readmd.boot.js ({} bytes)", body.len());
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn boot_bundle_is_identical_across_checkouts_and_detects_source_changes() {
        let root=std::env::temp_dir().join(format!("readmd-bundle-checkout-{}",std::process::id()));
        let unix=root.join("unix");let windows=root.join("windows");
        for source in SOURCES {
            let text=format!("// {source}\nwindow.bundleCheck = 1;\n");
            for (tree,content) in [(&unix,text.clone()),(&windows,text.replace('\n',"\r\n"))] {
                let file=tree.join("assets").join(source);
                fs::create_dir_all(file.parent().unwrap()).unwrap();fs::write(file,content).unwrap();
            }
        }
        let expected=build(&unix);
        assert_eq!(expected,build(&windows));
        fs::write(windows.join("assets/readmd.boot.js"),String::from_utf8(expected).unwrap().replace('\n',"\r\n")).unwrap();
        assert!(bundle(&windows,true).unwrap());
        fs::write(windows.join("assets/app.js"),"window.bundleCheck = 2;\r\n").unwrap();
        assert!(!bundle(&windows,true).unwrap());
        fs::remove_dir_all(root).unwrap();
    }
}
