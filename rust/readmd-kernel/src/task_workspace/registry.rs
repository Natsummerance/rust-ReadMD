//! Bounded cross-process admission and a fixed-size, torn-write-safe cursor.
//! Scheduling metadata never grants ownership or deletion rights.
use super::*;

pub(super) const MAX_TASKS: usize = 128;
const MAX_SCAN: usize = 8192;
const MAX_RESERVED: u64 = 2 * 1024 * 1024 * 1024;
const FREE_MARGIN: u64 = 32 * 1024 * 1024;
const SLOT: usize = 72;
const CURSOR_SIZE: usize = 64 + 2 * SLOT;
const MAGIC: &[u8] = b"ReadMD recovery cursor v1";

pub(super) fn default_budget() -> u64 {
    256 * 1024 * 1024
}
pub(super) fn budget(purpose: &str) -> u64 {
    match purpose {
        "ocr-recognize" => 64 * 1024 * 1024,
        "ocr-probe" | "typst-probe" => 128 * 1024 * 1024,
        _ => default_budget(),
    }
}
fn busy() -> io::Error {
    io::Error::new(io::ErrorKind::WouldBlock, "Task storage is reserved")
}
fn full() -> io::Error {
    io::Error::new(io::ErrorKind::StorageFull, "Insufficient task storage")
}
pub(super) fn nofollow(options: &mut OpenOptions) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        #[cfg(target_os = "linux")]
        const NOFOLLOW: i32 = 0x20000;
        #[cfg(target_os = "macos")]
        const NOFOLLOW: i32 = 0x100;
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        const NOFOLLOW: i32 = 0;
        options.custom_flags(NOFOLLOW);
    }
    #[cfg(not(unix))]
    let _ = options;
}

pub(super) fn gate(root: &Path, name: &str) -> io::Result<File> {
    let path = root.join(name);
    match owner(&path, false) {
        Ok(file) => Ok(file),
        Err(e) if e.kind() == io::ErrorKind::NotFound => owner(&path, true).map_err(|e| {
            if e.kind() == io::ErrorKind::AlreadyExists || e.raw_os_error() == Some(32) {
                busy()
            } else {
                e
            }
        }),
        Err(e) if e.raw_os_error() == Some(32) => Err(busy()),
        Err(e) => Err(e),
    }
}

pub(super) fn candidates(root: &Path) -> io::Result<Vec<String>> {
    let mut names = Vec::new();
    for (index, item) in fs::read_dir(root)?.enumerate() {
        if index >= MAX_SCAN {
            return Err(busy());
        }
        let item = item?;
        if let Some(name) = item.file_name().to_str().filter(|name| valid_uuid(name)) {
            names.push(name.to_owned());
        }
    }
    names.sort_unstable();
    Ok(names)
}

pub(super) fn reserved(root: &Path, names: &[String], expected_profile: &str) -> io::Result<u64> {
    if names.len() >= MAX_TASKS {
        return Err(busy());
    }
    let mut total = 0u64;
    for name in names {
        let dir = root.join(name);
        // A foreign or in-progress record occupies a conservative reservation;
        // never inspect its work tree or treat it as a reclaimable task.
        let value = (|| -> io::Result<u64> {
            plain(&dir, true)?;
            let directory = open_parent(&dir)?;
            identity_file(&directory, &Kind::Directory)?;
            let mut options = OpenOptions::new();
            options.read(true);
            nofollow(&mut options);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                options.share_mode(7).custom_flags(0x00200000);
            }
            let file = options.open(dir.join("record.jsonl"))?;
            identity_file(&file, &Kind::File)?;
            let mut first = Vec::new();
            use std::io::BufRead;
            std::io::BufReader::new(file.take(16 * 1024)).read_until(b'\n', &mut first)?;
            if first.last() != Some(&b'\n') {
                return Err(invalid());
            }
            let Event::Header { value: header } = serde_json::from_slice(&first)? else {
                return Err(invalid());
            };
            if header.schema != 1
                || header.id != *name
                || header.profile != expected_profile
                || (header.reserved_bytes != 0 && header.reserved_bytes != budget(&header.purpose))
            {
                return Err(invalid());
            }
            Ok(if header.reserved_bytes == 0 {
                default_budget()
            } else {
                header.reserved_bytes
            })
        })()
        .unwrap_or_else(|_| default_budget());
        total = total.checked_add(value).ok_or_else(full)?;
    }
    Ok(total)
}

fn fits(existing: u64, request: u64, free: Option<u64>) -> io::Result<()> {
    let total = existing.checked_add(request).ok_or_else(full)?;
    if request == 0 || total > MAX_RESERVED {
        return Err(busy());
    }
    if free.is_some_and(|free| free < total.saturating_add(FREE_MARGIN)) {
        return Err(full());
    }
    Ok(())
}

/// The gate is held through header publication. Counting declarations rather
/// than only current file sizes also accounts for workers that have yet to write.
pub(super) fn admit(root: &Path, profile: &str, purpose: &str, temp: &Path) -> io::Result<File> {
    // Only header publication holds this lock. Serialize short simultaneous
    // creations instead of reporting an ordinary export as a write failure.
    // Never wait indefinitely behind a stalled/abandoned creator.
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(100);
    let gate = loop {
        match gate(root, "allocation.lock") {
            Ok(file) => break file,
            Err(e)
                if e.kind() == io::ErrorKind::WouldBlock
                    && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            Err(e) => return Err(e),
        }
    };
    let names = candidates(root)?;
    let existing = reserved(root, &names, profile)?;
    fits(existing, budget(purpose), free_bytes(root)?)?;
    fits(existing, budget(purpose), free_bytes(temp)?)?;
    Ok(gate)
}

#[cfg(windows)]
fn free_bytes(path: &Path) -> io::Result<Option<u64>> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{core::PCWSTR, Win32::Storage::FileSystem::GetDiskFreeSpaceExW};
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut bytes = 0;
    unsafe {
        GetDiskFreeSpaceExW(PCWSTR(path.as_ptr()), Some(&mut bytes), None, None)
            .map_err(|e| io::Error::from_raw_os_error(e.code().0 & 0xffff))?;
    }
    Ok(Some(bytes))
}
#[cfg(not(windows))]
fn free_bytes(_path: &Path) -> io::Result<Option<u64>> {
    Ok(None)
}

pub(super) struct Cursor {
    file: File,
    profile: [u8; 32],
    sequence: u64,
    last: String,
}
impl Cursor {
    pub(super) fn open(root: &Path, profile: &str) -> io::Result<Self> {
        let path = root.join("recovery.cursor");
        let mut options = OpenOptions::new();
        options.read(true).write(true);
        nofollow(&mut options);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(0).custom_flags(0x00200000);
        }
        let (mut file, created) = match options.open(&path) {
            Ok(f) => (f, false),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                (options.create_new(true).open(&path)?, true)
            }
            Err(e) => return Err(e),
        };
        identity_file(&file, &Kind::File)?;
        let profile: [u8; 32] = Sha256::digest(profile.as_bytes()).into();
        let mut expected = [0u8; 64];
        expected[..MAGIC.len()].copy_from_slice(MAGIC);
        expected[32..].copy_from_slice(&profile);
        if created {
            let mut initial = [0u8; CURSOR_SIZE];
            initial[..64].copy_from_slice(&expected);
            file.write_all(&initial)?;
            file.sync_all()?;
        }
        if file.metadata()?.len() != CURSOR_SIZE as u64 {
            return Err(invalid());
        }
        file.seek(SeekFrom::Start(0))?;
        let mut bytes = [0u8; CURSOR_SIZE];
        file.read_exact(&mut bytes)?;
        if bytes[..64] != expected {
            return Err(invalid());
        }
        let mut sequence = 0;
        let mut last = String::new();
        for slot in bytes[64..].chunks_exact(SLOT) {
            let mut digest = Sha256::new();
            digest.update(profile);
            digest.update(&slot[..40]);
            if digest.finalize().as_slice() != &slot[40..] {
                continue;
            }
            let next = u64::from_le_bytes(slot[..8].try_into().map_err(|_| invalid())?);
            let name = std::str::from_utf8(&slot[8..40]).map_err(|_| invalid())?;
            if next > sequence && valid_uuid(name) {
                sequence = next;
                last = name.into();
            }
        }
        Ok(Self {
            file,
            profile,
            sequence,
            last,
        })
    }
    pub(super) fn order(&self, names: &mut [String]) {
        let pivot = names.partition_point(|name| name <= &self.last);
        names.rotate_left(pivot);
    }
    pub(super) fn advance(&mut self, last: &str) -> io::Result<()> {
        if !valid_uuid(last) {
            return Err(invalid());
        }
        let sequence = self.sequence.checked_add(1).ok_or_else(invalid)?;
        let mut bytes = [0u8; SLOT];
        bytes[..8].copy_from_slice(&sequence.to_le_bytes());
        bytes[8..40].copy_from_slice(last.as_bytes());
        let mut digest = Sha256::new();
        digest.update(self.profile);
        digest.update(&bytes[..40]);
        bytes[40..].copy_from_slice(&digest.finalize());
        self.file
            .seek(SeekFrom::Start(64 + (sequence % 2) * SLOT as u64))?;
        self.file.write_all(&bytes)?;
        self.file.sync_data()?;
        self.sequence = sequence;
        self.last = last.into();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn declarations_and_free_space_fail_closed() {
        assert!(fits(
            MAX_RESERVED - default_budget(),
            default_budget(),
            Some(u64::MAX)
        )
        .is_ok());
        assert_eq!(
            fits(MAX_RESERVED, 1, Some(u64::MAX)).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(
            fits(
                0,
                default_budget(),
                Some(default_budget() + FREE_MARGIN - 1)
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::StorageFull
        );
        assert!(fits(0, default_budget(), Some(default_budget() + FREE_MARGIN)).is_ok());
        assert_eq!(
            fits(u64::MAX, default_budget(), None).unwrap_err().kind(),
            io::ErrorKind::StorageFull
        );
    }
    #[test]
    fn cursor_survives_torn_slot_and_never_overwrites_foreign_state() {
        let root = tempfile::tempdir().unwrap();
        let one = uuid::Uuid::new_v4().simple().to_string();
        let two = uuid::Uuid::new_v4().simple().to_string();
        let mut cursor = Cursor::open(root.path(), "profile").unwrap();
        cursor.advance(&one).unwrap();
        cursor.advance(&two).unwrap();
        drop(cursor);
        let path = root.path().join("recovery.cursor");
        let mut file = OpenOptions::new().write(true).open(&path).unwrap();
        file.seek(SeekFrom::Start(64)).unwrap();
        file.write_all(&[0xff; 12]).unwrap();
        drop(file);
        let cursor = Cursor::open(root.path(), "profile").unwrap();
        assert_eq!(cursor.last, one);
        drop(cursor);
        fs::write(&path, b"User file").unwrap();
        assert!(Cursor::open(root.path(), "profile").is_err());
        assert_eq!(fs::read(path).unwrap(), b"User file");
    }
}
