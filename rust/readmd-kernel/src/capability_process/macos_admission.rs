//! Cooperative installation admission only. This is not a filesystem lease
//! protecting document renames against uncooperative external writers.
use super::*;
use std::os::unix::fs::{DirBuilderExt, MetadataExt};

fn invalid_namespace() -> ApiError {
    ApiError::new(422, "capability_install_failed").noted("reason", "space_admission_namespace")
}
fn identity(file: &fs::File, path: &Path) -> ApiResult<()> {
    let before = file.metadata().map_err(|_| invalid_namespace())?;
    let now = fs::symlink_metadata(path).map_err(|_| invalid_namespace())?;
    if now.file_type().is_symlink() || before.dev() != now.dev() || before.ino() != now.ino() {
        return Err(invalid_namespace());
    }
    Ok(())
}
pub(super) struct Admission {
    guard: Guard,
    file_path: PathBuf,
    directories: Vec<(PathBuf, fs::File)>,
}
impl Admission {
    pub(super) fn acquire() -> ApiResult<Self> {
        // NSHomeDirectory's explicit native-test namespace is shared by all
        // profiles. READMD_DATA_DIR must never select a separate budget gate.
        let home = std::env::var_os("CFFIXED_USER_HOME").filter(|p| !p.is_empty())
            .map(PathBuf::from).unwrap_or_else(crate::paths::home_dir);
        Self::at(&home)
    }
    fn at(home: &Path) -> ApiResult<Self> {
        use std::os::unix::fs::OpenOptionsExt;
        if !home.is_absolute() { return Err(invalid_namespace()); }
        let home = home.canonicalize().map_err(|_| invalid_namespace())?;
        let mut directory = home;
        let mut directories = Vec::new();
        for component in [None, Some("Library"), Some("Application Support"), Some("readmd"), Some("shared-install-admission-v1")] {
            if let Some(component) = component {
                directory.push(component);
                match fs::DirBuilder::new().mode(0o700).create(&directory) {
                    Ok(()) => (),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                    Err(_) => return Err(invalid_namespace()),
                }
            }
            real_directory(&directory).map_err(|_| invalid_namespace())?;
            let file = fs::OpenOptions::new().read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY).open(&directory).map_err(|_| invalid_namespace())?;
            let metadata = file.metadata().map_err(|_| invalid_namespace())?;
            if metadata.uid() != unsafe { libc::geteuid() }
                || (component == Some("shared-install-admission-v1") && metadata.mode() & 0o077 != 0) {
                return Err(invalid_namespace());
            }
            identity(&file, &directory)?;
            directories.push((directory.clone(), file));
        }
        let file_path = directory.join("writers.lock");
        // Do not open a known alien object for writing, even though open()
        // never truncates. A final no-follow open and identity check follow.
        if let Ok(metadata) = fs::symlink_metadata(&file_path) { validate_file(&metadata)?; }
        let file = open(&file_path, Mode::Exclusive).map_err(|_| invalid_namespace())?
            .ok_or_else(|| ApiError::new(409, "capability_install_busy"))?;
        let admission = Self { guard: Guard { _file: file }, file_path, directories };
        admission.verify()?;
        Ok(admission)
    }
    pub(super) fn verify(&self) -> ApiResult<()> {
        for (path, file) in &self.directories { identity(file, path)?; }
        validate_file(&self.guard._file.metadata().map_err(|_| invalid_namespace())?)?;
        identity(&self.guard._file, &self.file_path)?;
        Ok(())
    }
}
fn validate_file(metadata: &fs::Metadata) -> ApiResult<()> {
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() != 0
        || metadata.nlink() != 1 || metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
        return Err(invalid_namespace());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn different_profiles_share_one_gate_without_blocking_resource_readers() {
        let root = tempfile::tempdir().unwrap();
        let a = root.path().join("profile-a"); let b = root.path().join("profile-b");
        fs::create_dir(&a).unwrap(); fs::create_dir(&b).unwrap();
        let profile_a = install_admission(&a).unwrap();
        let shared = Admission::at(root.path()).unwrap();
        let profile_b = install_admission(&b).unwrap();
        assert_eq!(Admission::at(root.path()).err().unwrap().code, "capability_install_busy");
        let reader = try_acquire(&b, "fonts", Mode::Shared).unwrap().unwrap();
        shared.verify().unwrap();
        drop((shared, profile_a, profile_b, reader));
        drop(Admission::at(root.path()).unwrap());
    }
    #[test]
    fn coordination_replacement_and_unknown_files_are_preserved_and_rejected() {
        let root = tempfile::tempdir().unwrap();
        let admission = Admission::at(root.path()).unwrap();
        let held = admission.file_path.with_extension("held");
        fs::rename(&admission.file_path, &held).unwrap();
        fs::write(&admission.file_path, b"external occupant").unwrap();
        assert!(admission.verify().is_err());
        assert!(Admission::at(root.path()).is_err());
        assert_eq!(fs::read(&admission.file_path).unwrap(), b"external occupant");
        drop(admission);
        assert!(held.is_file());
    }
    #[test]
    fn coordinator_directory_symlinks_do_not_redirect_writes() {
        let root = tempfile::tempdir().unwrap(); let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("Library")).unwrap();
        assert!(Admission::at(root.path()).is_err());
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    }
    #[test]
    fn shared_admission_survives_different_process_profiles_and_releases_after_termination() {
        use std::{process::{Command, Stdio}, time::{Duration, Instant}};
        const CHILD: &str = "READMD_SHARED_INSTALL_CHILD_HOME";
        if let Some(home) = std::env::var_os(CHILD) {
            let home = PathBuf::from(home);
            let profile = home.join("child-profile"); fs::create_dir(&profile).unwrap();
            let _profile = install_admission(&profile).unwrap();
            let _shared = Admission::at(&home).unwrap();
            fs::write(home.join("ready"), b"ready").unwrap();
            loop { std::thread::sleep(Duration::from_millis(25)); }
        }
        struct Child(std::process::Child);
        impl Drop for Child { fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); } }
        let root = tempfile::tempdir().unwrap();
        let mut child = Child(Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "capability_process::macos_admission::tests::shared_admission_survives_different_process_profiles_and_releases_after_termination", "--test-threads=1"])
            .env(CHILD, root.path()).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap());
        let deadline = Instant::now() + Duration::from_secs(10);
        while !root.path().join("ready").is_file() {
            assert!(child.0.try_wait().unwrap().is_none(), "child exited before acquiring admission");
            assert!(Instant::now() < deadline, "child admission timeout");
            std::thread::sleep(Duration::from_millis(10));
        }
        let profile = root.path().join("parent-profile"); fs::create_dir(&profile).unwrap();
        let _profile = install_admission(&profile).unwrap();
        let start = Instant::now();
        assert_eq!(Admission::at(root.path()).err().unwrap().code, "capability_install_busy");
        assert!(start.elapsed() < Duration::from_millis(200));
        child.0.kill().unwrap(); child.0.wait().unwrap();
        drop(Admission::at(root.path()).unwrap());
        assert_eq!(fs::read_dir(&profile).unwrap().count(), 1); // locks only, no payload
    }
}
