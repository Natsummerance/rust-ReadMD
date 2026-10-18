//! A storage protocol, not an application version. Older clients do not know
//! this namespace and must never write the trees protected by the new locks.
use std::path::{Path, PathBuf};

pub const PROTOCOL: &str = "managed-v2";
pub fn root(data: &Path) -> PathBuf {
    data.join("capabilities").join(PROTOCOL)
}
