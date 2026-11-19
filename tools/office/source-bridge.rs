// ReadMD adapter, added as a module to the verified upstream source.
// No layout or rendering algorithm is copied or replaced here.
use std::path::Path;

/// Render one host-owned snapshot. Called on the process main thread, required
/// by GPUI's native text/graphics platforms. This does not run the editor,
/// updater, recent-file/settings migration or the general-purpose CLI.
pub fn export_pdf(input: &Path, output: &Path) -> Result<(), String> {
    if output != input.with_extension("pdf") {
        return Err("the output must match the snapshot stem".into());
    }
    ibis_calc::eval::convert::set_clock(crate::clock::calc_clock);
    // Even an embedded IMAGE formula cannot invoke the upstream curl path.
    // Packaged pictures and system/embedded fonts still use the normal renderer.
    crate::render::web_image::set_fetcher(Some(std::sync::Arc::new(|_| {
        Err(crate::render::web_image::Failure::Connect)
    })));
    let args = crate::Args {
        path: Some(input.to_path_buf()),
        out_dir: input.parent().map(Path::to_path_buf),
        ..Default::default()
    };
    crate::run_export(&args, "pdf")
}
