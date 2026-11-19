//! Reuse the installer's real directory pins and reparse checks for capacity.
use super::*;

fn query_error() -> ApiError {
    ApiError::new(422, "capability_install_failed")
        .noted("reason", "space_query")
        .noted("operation", "volume_identity")
}
#[derive(Debug)]
pub(super) struct Anchor {
    tree: super::super::paths::Tree,
    volume: u64,
}
impl Anchor {
    pub(super) fn read(directory: &Path) -> ApiResult<(Self, u64, u64, String)> {
        real_directory(directory)?;
        let tree = super::super::paths::Tree::new(directory).map_err(|_| query_error())?;
        let (canonical, volume) = tree.capacity_anchor().map_err(|_| query_error())?;
        let (available, unit) = super::capacity(&canonical)?;
        let anchor = Self { tree, volume };
        anchor.verify()?;
        Ok((anchor, available, unit, format!("windows:{volume:016x}")))
    }
    pub(super) fn verify(&self) -> ApiResult<()> {
        let (_, volume) = self.tree.capacity_anchor().map_err(|_| query_error())?;
        if volume != self.volume {
            return Err(query_error());
        }
        Ok(())
    }
}
