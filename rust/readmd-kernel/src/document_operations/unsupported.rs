use super::*;
pub(super) fn valid_leaf(_: &OsStr) -> bool {
    false
}
pub(super) fn info(_: &fs::File) -> io::Result<Info> {
    Err(unsupported())
}
pub(super) fn unlock(_: &fs::File) {}
pub(super) struct Directory;
impl Directory {
    pub(super) fn open(_: &Path) -> io::Result<Self> {
        Err(unsupported())
    }
    pub(super) fn path(&self) -> &Path {
        unreachable!()
    }
    pub(super) fn identity(&self) -> &FileIdentity {
        unreachable!()
    }
    pub(super) fn revalidate(&self) -> io::Result<()> {
        Err(unsupported())
    }
    pub(super) fn open_file(&self, _: &OsStr) -> io::Result<fs::File> {
        Err(unsupported())
    }
    pub(super) fn exists(&self, _: &OsStr) -> io::Result<bool> {
        Err(unsupported())
    }
    pub(super) fn try_lock(&self, _: &OsStr) -> io::Result<Option<fs::File>> {
        Err(unsupported())
    }
    pub(super) fn lock_still_named(&self, _: &OsStr, _: &FileIdentity) -> io::Result<bool> {
        Err(unsupported())
    }
    pub(super) fn rename_noreplace(&self, _: &OsStr, _: &OsStr) -> io::Result<()> {
        Err(unsupported())
    }
    pub(super) fn sync(&self) -> io::Result<()> {
        Err(unsupported())
    }
}
