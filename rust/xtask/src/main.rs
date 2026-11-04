//! Repository maintenance tasks, run as `cargo xtask <command>` from `rust/`.
//!
//! * `bundle-boot [--check]` — rebuild `assets/readmd.boot.js` from its sources.
//! All commands resolve the current repository, or accept `--root DIR`.
//! * `sync-version [VERSION] [--check]` — propagate the version from `.env` /
//!   `VERSION` (or the argument) to every manifest, page and README, then
//!   rebundle.  `--check` only reports files that are out of sync.
//! * `release-asset-sync --assets-dir DIR --tag TAG --commit SHA [--repo R]` —
//!   stage-then-swap GitHub Release assets (port of `tools/release_asset_sync.py`).
//! * `pet-package [--platform P] [--arch A] [--skip-build] [--output DIR]` —
//!   stage and zip the native pet runtime (port of `build-package.py`).
//!
//! This is the Rust port of `tools/sync_version.py`; the replacement rules
//! are the same, and each file keeps its own line endings.

mod bundle;
mod pet_package;
mod release_sync;
mod version;

use std::path::PathBuf;
use std::process::ExitCode;

fn repo_root(args: &mut Vec<String>) -> Result<PathBuf, String> {
    let mut explicit = None; let mut at = 1;
    while at < args.len() {
        if args[at] == "--root" {
            if explicit.is_some() || at + 1 >= args.len() { return Err("provide --root DIR exactly once".into()); }
            explicit = Some(PathBuf::from(args.remove(at + 1))); args.remove(at);
        } else { at += 1; }
    }
    let cwd = std::env::current_dir().map_err(|_| "cannot resolve task working directory")?;
    bundle_root(&cwd, explicit.as_deref())
}

// A shared target can reuse an xtask compiled in another worktree. Its baked
// manifest directory must not select where a new boot bundle is written.
fn bundle_root(cwd: &std::path::Path, explicit: Option<&std::path::Path>) -> Result<PathBuf, String> {
    let is_repo = |root: &std::path::Path| root.join("assets/index.html").is_file()
        && root.join("rust/xtask/src/bundle.rs").is_file();
    if let Some(root) = explicit {
        if !root.is_absolute() || !is_repo(root) { return Err("--root requires an absolute ReadMD repository directory".into()); }
        return Ok(root.to_path_buf());
    }
    cwd.ancestors().find(|root| is_repo(root)).map(std::path::Path::to_path_buf)
        .ok_or_else(|| "run xtask inside its repository or provide --root DIR".into())
}

fn run_bundle(args: &[String], root: &std::path::Path) -> Result<bool, String> {
    if args.len() > 2 || args.get(1).is_some_and(|arg| arg != "--check") {
        return Err("usage: bundle-boot [--check] [--root DIR]".into());
    }
    bundle::bundle(root, args.get(1).is_some())
}

fn usage() -> ExitCode {
    eprintln!("usage: cargo xtask <bundle-boot [--check] | sync-version [VERSION] [--check] | release-asset-sync --assets-dir DIR --tag TAG --commit SHA [--repo R] | pet-package [--platform P] [--arch A] [--skip-build] [--output DIR]> [--root DIR]");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let root = match repo_root(&mut args) {
        Ok(root) => root,
        Err(error) => { eprintln!("[ERROR] {error}"); return ExitCode::from(2); }
    };
    let check = args.iter().any(|a| a == "--check");
    let positional: Vec<&String> = args.iter().skip(1).filter(|a| !a.starts_with("--")).collect();
    match args.first().map(String::as_str) {
        Some("bundle-boot") => match run_bundle(&args, &root) {
            Ok(true) => ExitCode::SUCCESS,
            Ok(false) => ExitCode::from(1),
            Err(e) => {
                eprintln!("[ERROR] {e}");
                ExitCode::from(2)
            }
        },
        Some("sync-version") => {
            let ver = positional
                .first()
                .map(|s| s.to_string())
                .unwrap_or_else(|| version::load_env_version(&root));
            match version::sync_all(&root, &ver, check) {
                Ok(true) => ExitCode::SUCCESS,
                Ok(false) => ExitCode::from(1),
                Err(e) => {
                    eprintln!("[ERROR] {e}");
                    ExitCode::from(2)
                }
            }
        }
        Some("release-asset-sync") => match release_sync::main(&root, &args[1..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("[ERROR] {e}");
                ExitCode::from(1)
            }
        },
        Some("pet-package") => match pet_package::main(&root, &args[1..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("[ERROR] {e}");
                ExitCode::from(1)
            }
        },
        _ => usage(),
    }
}

#[cfg(test)]
mod root_tests {
    use super::*;
    fn fixture(root: &std::path::Path, marker: &str) {
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::create_dir_all(root.join("rust/xtask/src")).unwrap();
        std::fs::write(root.join("assets/index.html"), "owned fixture").unwrap();
        std::fs::write(root.join("rust/xtask/src/bundle.rs"), "owned fixture").unwrap();
        for name in bundle::SOURCES {
            let file = root.join("assets").join(name); std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, if *name == "app.js" { marker } else { "" }).unwrap();
        }
    }
    #[test]
    fn a_shared_tool_bundles_the_current_worktree_without_rewriting_another() {
        let directory = tempfile::tempdir().unwrap(); let a = directory.path().join("a"); let b = directory.path().join("b");
        fixture(&a, "OWNED_WORKTREE_A"); fixture(&b, "OWNED_WORKTREE_B");
        std::fs::write(a.join("assets/readmd.boot.js"), "PRESERVE_A_BOOT").unwrap();
        let root = bundle_root(&b.join("rust/xtask/src"), None).unwrap(); assert_eq!(root, b);
        assert!(bundle::bundle(&root, false).unwrap());
        let body = std::fs::read_to_string(b.join("assets/readmd.boot.js")).unwrap();
        assert!(body.contains("OWNED_WORKTREE_B")); assert!(!body.contains("OWNED_WORKTREE_A"));
        assert_eq!(std::fs::read_to_string(a.join("assets/readmd.boot.js")).unwrap(), "PRESERVE_A_BOOT");
        assert!(bundle::bundle(&root, true).unwrap());
    }
    #[test]
    fn an_explicit_bundle_root_is_checked_and_unrelated_directories_fail_closed() {
        let directory = tempfile::tempdir().unwrap(); let a = directory.path().join("a"); fixture(&a, "OWNED_A");
        assert_eq!(bundle_root(directory.path(), Some(&a)).unwrap(), a);
        assert!(bundle_root(directory.path(), None).is_err());
        assert!(bundle_root(directory.path(), Some(std::path::Path::new("a"))).is_err());
        assert!(bundle_root(directory.path(), Some(&directory.path().join("absent"))).is_err());
    }
    #[test]
    fn explicit_root_is_removed_without_becoming_a_version_or_release_argument() {
        let directory = tempfile::tempdir().unwrap(); fixture(directory.path(), "OWNED_ROOT");
        let mut args = vec!["sync-version".into(), "0.0.6".into(), "--root".into(), directory.path().to_string_lossy().into_owned(), "--check".into()];
        assert_eq!(repo_root(&mut args).unwrap(), directory.path());
        assert_eq!(args, ["sync-version", "0.0.6", "--check"]);
        assert!(repo_root(&mut vec!["bundle-boot".into(), "--root".into()]).is_err());
        assert!(repo_root(&mut vec!["bundle-boot".into(), "--root".into(), directory.path().to_string_lossy().into_owned(), "--root".into(), directory.path().to_string_lossy().into_owned()]).is_err());
    }
}
