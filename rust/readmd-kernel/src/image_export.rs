//! Render publication pages through the same layout used by PDF export.
use crate::mdexport::{ExportResult, ZipEntry};
use image::{ImageEncoder, Rgb, RgbImage};
use std::{fs, path::Path};

const BUDGET: &str = "export_snapshot_budget";
const FAILED: &str = "export_snapshot_render_failed";
const MAX_PIXELS: u64 = 48_000_000;
const MAX_PAGES: usize = 128;

pub(crate) fn is_image(format: &str) -> bool {
    matches!(format, "png" | "png-pages")
}

pub(crate) fn layout_format(format: &str) -> &str {
    if is_image(format) { "pdf" } else { format }
}

pub(crate) fn output_extension(format: &str) -> &str {
    match format {
        "png-pages" => "zip",
        "presentation" => "html",
        other => other,
    }
}

fn encode(image: &RgbImage) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(image.as_raw(), image.width(), image.height(), image::ExtendedColorType::Rgb8)
        .map_err(|_| FAILED)?;
    Ok(bytes)
}

fn crop_blank_rows(image: RgbImage) -> RgbImage {
    let background = *image.get_pixel(0, 0);
    let mut rows = image.rows().enumerate().filter_map(|(i, mut row)|
        row.any(|pixel| *pixel != background).then_some(i as u32));
    let Some(first) = rows.next() else { return image; };
    let last = rows.next_back().unwrap_or(first);
    let top = first.saturating_sub(16);
    let bottom = last.saturating_add(17).min(image.height());
    if top == 0 && bottom == image.height() { return image; }
    image::imageops::crop_imm(&image, 0, top, image.width(), bottom - top).to_image()
}

fn join_pages(pages: &[RgbImage]) -> Result<RgbImage, String> {
    let width = pages.iter().map(RgbImage::width).max().ok_or(FAILED)?;
    let height: u64 = pages.iter().map(|page| u64::from(page.height())).sum();
    if height * u64::from(width) > MAX_PIXELS { return Err(BUDGET.into()); }
    let mut image = RgbImage::from_pixel(width, height as u32, Rgb([255; 3]));
    let mut y = 0;
    for page in pages {
        crate::publication_budget::check()?;
        image::imageops::replace(&mut image, page, ((width - page.width()) / 2).into(), y);
        y += i64::from(page.height());
    }
    Ok(image)
}

pub(crate) fn export(
    format: &str, target: &Path, data: &Path, assets: &Path,
    layout: impl FnOnce(&Path) -> Result<ExportResult, String>,
) -> Result<ExportResult, String> {
    let parent = target.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    let scratch = tempfile::Builder::new().prefix(".readmd-image-").tempdir_in(parent).map_err(|_| FAILED)?;
    let pdf = scratch.path().join("layout.pdf");
    let rendered = layout(&pdf)?;
    if !rendered.ok { return Err(FAILED.into()); }
    if fs::metadata(&pdf).map_err(|_| FAILED)?.len() > 32 * 1024 * 1024 { return Err(BUDGET.into()); }
    let source = fs::read(&pdf).map_err(|_| FAILED)?;
    let count = lopdf::Document::load_mem(&source).map_err(|_| FAILED)?.get_pages().len();
    if count == 0 || count > MAX_PAGES { return Err(BUDGET.into()); }
    let mut pages = Vec::new();
    let mut images = Vec::new();
    let mut pixels = 0;
    let mut output_bytes = 0usize;
    for index in 0..count {
        crate::publication_budget::check()?;
        let raster = raster_page(&source, &pdf, index, data, assets)?;
        let page = image::load_from_memory(&raster).map_err(|_| FAILED)?.into_rgb8();
        pixels += u64::from(page.width()) * u64::from(page.height());
        if pixels > MAX_PIXELS { return Err(BUDGET.into()); }
        if format == "png" {
            pages.push(crop_blank_rows(page));
        } else {
            let data = encode(&page)?;
            output_bytes = output_bytes.saturating_add(data.len());
            if output_bytes > crate::export_snapshot::MAX_SNAPSHOT_BYTES { return Err(BUDGET.into()); }
            images.push(ZipEntry { path: format!("page-{:03}.png", index + 1), data, compress: false });
        }
    }
    let bytes = if format == "png" { encode(&join_pages(&pages)?)? } else { crate::mdexport::write_zip(&images)? };
    if bytes.len() > crate::export_snapshot::MAX_SNAPSHOT_BYTES { return Err(BUDGET.into()); }
    crate::publication_budget::check()?;
    crate::content::write_bytes_atomic(target, &bytes).map_err(|_| FAILED)?;
    Ok(ExportResult {
        ok: true, path: Some(target.to_string_lossy().into_owned()), size: Some(bytes.len() as u64),
        warns: rendered.warns, error: None, canceled: Some(false),
    })
}

fn raster_page(source: &[u8], pdf: &Path, index: usize, data: &Path, assets: &Path) -> Result<Vec<u8>, String> {
    #[cfg(target_os = "macos")]
    {
        let _ = (pdf, data, assets);
        crate::macos_pdf_render::render(source, index)
    }
    #[cfg(target_os = "windows")]
    {
        let _ = (pdf, data, assets);
        crate::ocr_winrt::render_pdf_page_png(source, index)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = source;
        crate::pdf_workspace::raster_pack_page(data, assets, pdf, index).map(|(_, bytes)| bytes)
    }
}
