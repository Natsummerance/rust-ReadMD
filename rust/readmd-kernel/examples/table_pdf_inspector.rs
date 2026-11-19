//! Maintenance-only reader for owned synthetic export fixtures; not a product
//! route, authorization boundary or release executable. Reuses pdf-extract
//! 0.12.1 (MIT) and serde_json already present in this package/Cargo.lock.
use std::{
    fs::{File, Metadata, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

const MAX_PDF_BYTES: u64 = 16 * 1024 * 1024;
const MAX_JSON_BYTES: usize = 1024 * 1024 - 1;
type Result<T> = std::result::Result<T, &'static str>;

struct Input {
    file: File,
    #[cfg(windows)]
    _parents: Vec<File>,
}
fn plain(metadata: &Metadata, directory: bool) -> Result<()> {
    if metadata.file_type().is_symlink()
        || if directory {
            !metadata.is_dir()
        } else {
            !metadata.is_file()
        }
    {
        return Err("pdf_inspector_input_invalid");
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
            return Err("pdf_inspector_input_invalid");
        }
    }
    Ok(())
}

#[cfg(unix)]
fn open_input(path: &Path) -> Result<Input> {
    use std::{
        ffi::CString,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::{ffi::OsStrExt, fs::OpenOptionsExt},
        },
    };
    let parts = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(v) => Some(v),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut parent = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open("/")
        .map_err(|_| "pdf_inspector_input_invalid")?;
    for (index, part) in parts.iter().enumerate() {
        let directory = index + 1 < parts.len();
        let name = CString::new(part.as_bytes()).map_err(|_| "pdf_inspector_input_invalid")?;
        let flags = libc::O_RDONLY
            | libc::O_NOFOLLOW
            | libc::O_CLOEXEC
            | if directory {
                libc::O_DIRECTORY
            } else {
                libc::O_NONBLOCK
            };
        // Resolve each next component against the already opened directory.
        let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err("pdf_inspector_input_invalid");
        }
        let next = unsafe { File::from_raw_fd(fd) };
        plain(
            &next.metadata().map_err(|_| "pdf_inspector_input_invalid")?,
            directory,
        )?;
        parent = next;
    }
    Ok(Input { file: parent })
}

#[cfg(windows)]
fn open_input(path: &Path) -> Result<Input> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    let mut parents = Vec::new();
    // Keep checked ancestors open without share-delete during this bounded
    // read. No directory enumeration or new native FFI is needed.
    for parent in path
        .ancestors()
        .skip(1)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        let handle = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(parent)
            .map_err(|_| "pdf_inspector_input_invalid")?;
        plain(
            &handle
                .metadata()
                .map_err(|_| "pdf_inspector_input_invalid")?,
            true,
        )?;
        parents.push(handle);
    }
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ.0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)
        .map_err(|_| "pdf_inspector_input_invalid")?;
    plain(
        &file.metadata().map_err(|_| "pdf_inspector_input_invalid")?,
        false,
    )?;
    Ok(Input {
        file,
        _parents: parents,
    })
}

#[cfg(not(any(unix, windows)))]
fn open_input(_: &Path) -> Result<Input> {
    Err("pdf_inspector_platform_unavailable")
}

fn inspect() -> Result<Vec<u8>> {
    let mut args = std::env::args_os().skip(1);
    let path = PathBuf::from(args.next().ok_or("pdf_inspector_usage")?);
    if args.next().is_some()
        || !path.is_absolute()
        || path.file_name().is_none()
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err("pdf_inspector_usage");
    }
    #[cfg(windows)]
    for component in path.components() {
        if let Component::Prefix(prefix) = component {
            use std::path::Prefix;
            if !matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)) {
                return Err("pdf_inspector_input_invalid");
            }
        }
    }
    let mut input = open_input(&path)?;
    let before = input
        .file
        .metadata()
        .map_err(|_| "pdf_inspector_input_invalid")?;
    plain(&before, false)?;
    if before.len() == 0 || before.len() > MAX_PDF_BYTES {
        return Err("pdf_inspector_input_limit");
    }
    let mut bytes = Vec::with_capacity(before.len() as usize);
    (&mut input.file)
        .take(MAX_PDF_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "pdf_inspector_input_invalid")?;
    let after = input
        .file
        .metadata()
        .map_err(|_| "pdf_inspector_input_invalid")?;
    if bytes.len() as u64 != before.len()
        || after.len() != before.len()
        || before.modified().ok() != after.modified().ok()
        || !bytes.starts_with(b"%PDF-")
    {
        return Err("pdf_inspector_input_invalid");
    }
    let text =
        pdf_extract::extract_text_from_mem(&bytes).map_err(|_| "pdf_inspector_extract_failed")?;
    if text.trim().is_empty() {
        return Err("pdf_inspector_text_empty");
    }
    if text.len() > MAX_JSON_BYTES {
        return Err("pdf_inspector_output_limit");
    }
    let json = serde_json::to_vec(&serde_json::json!({"text":text}))
        .map_err(|_| "pdf_inspector_output_invalid")?;
    if json.len() > MAX_JSON_BYTES {
        return Err("pdf_inspector_output_limit");
    }
    Ok(json)
}
fn main() {
    // Parser failures produce a bounded JSON error, never a path/backtrace or
    // a successful empty result. The parent driver also has a process timeout.
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(inspect).unwrap_or(Err("pdf_inspector_extract_failed"));
    let failed = result.is_err();
    let bytes = result.unwrap_or_else(|code| {
        serde_json::to_vec(&serde_json::json!({"error_code":code})).unwrap()
    });
    let mut stdout = std::io::stdout().lock();
    if stdout
        .write_all(&bytes)
        .and_then(|_| stdout.write_all(b"\n"))
        .and_then(|_| stdout.flush())
        .is_err()
        || failed
    {
        std::process::exit(2);
    }
}
