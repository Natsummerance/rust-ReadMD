use super::Artifact;
use crate::{image_export, paths::AppPaths, publication_budget};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::path::Path;

const MAX_PDF_BYTES: u64 = 32 * 1024 * 1024;

fn read_pdf(path: &Path) -> Result<Vec<u8>, String> {
    publication_budget::check()?;
    if path.metadata().map_err(|_| "preview_expired")?.len() > MAX_PDF_BYTES {
        return Err("preview_too_large".into());
    }
    // Artifacts are immutable files owned by the preview cache.
    let bytes = std::fs::read(path).map_err(|_| "preview_expired")?;
    publication_budget::check()?;
    Ok(bytes)
}

// Older PDFium workers send their native BMP raster. Preserve legacy packs
// while returning a real PNG to page-image consumers on every platform.
fn preview_png(bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Ok(bytes);
    }
    let raster = image::load_from_memory_with_format(&bytes, image::ImageFormat::Bmp)
        .map_err(|_| "preview_generation_failed")?;
    publication_budget::check()?;
    let mut png = std::io::Cursor::new(Vec::new());
    raster
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|_| "preview_generation_failed")?;
    Ok(png.into_inner())
}

// Match the visible sheet used by the native page rasterizers. Page trees
// can define each of these attributes at a different ancestor.
fn sheet_bounds(doc: &lopdf::Document, id: lopdf::ObjectId) -> Option<[f64; 4]> {
    let mut node = id;
    let mut media = None;
    let mut crop = None;
    let mut rotation = None;
    for _ in 0..64 {
        let dict = doc.get_object(node).ok()?.as_dict().ok()?;
        let resolved = |key: &[u8]| {
            doc.dereference(dict.get(key).ok()?)
                .ok()
                .map(|(_, value)| value)
        };
        let rect = |key: &[u8]| {
            let values = resolved(key)?.as_array().ok()?;
            let coord = |i: usize| {
                let (_, v) = doc.dereference(values.get(i)?).ok()?;
                Some(f64::from(v.as_float().ok()?))
            };
            let bounds = [coord(0)?, coord(1)?, coord(2)?, coord(3)?];
            (values.len() == 4
                && bounds.iter().all(|v| v.is_finite())
                && bounds[0] < bounds[2]
                && bounds[1] < bounds[3])
                .then_some(bounds)
        };
        media = media.or_else(|| rect(b"MediaBox"));
        crop = crop.or_else(|| rect(b"CropBox"));
        rotation = rotation.or_else(|| resolved(b"Rotate")?.as_i64().ok());
        if media.is_some() && crop.is_some() && rotation.is_some() {
            break;
        }
        let Ok(parent) = dict.get(b"Parent").and_then(|value| value.as_reference()) else {
            break;
        };
        node = parent;
    }
    let media = media?;
    let [x0, y0, x1, y1] = crop.unwrap_or(media);
    let (x0, y0, x1, y1) = (
        x0.max(media[0]),
        y0.max(media[1]),
        x1.min(media[2]),
        y1.min(media[3]),
    );
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    if rotation.unwrap_or(0).rem_euclid(180) == 90 {
        Some([0.0, 0.0, y1 - y0, x1 - x0])
    } else {
        Some([x0, y0, x1, y1])
    }
}

fn describe_page(
    doc: &lopdf::Document,
    id: lopdf::ObjectId,
    number: u32,
    include_text: bool,
) -> (Vec<f64>, String) {
    let dimensions = sheet_bounds(doc, id).map(Vec::from).unwrap_or_default();
    let text = if include_text {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| doc.extract_text(&[number])))
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default()
    } else {
        String::new()
    };
    (dimensions, text)
}

pub(crate) fn preview_for_request(
    artifact: &Artifact,
    body: &Value,
    paths: &AppPaths,
) -> Result<Value, String> {
    payload(
        artifact,
        body["delivery"].as_str() == Some("pages"),
        &paths.data_dir,
        &paths.assets_dir,
    )
}

pub(super) fn payload(
    artifact: &Artifact,
    paged: bool,
    data: &Path,
    assets: &Path,
) -> Result<Value, String> {
    let pdf = read_pdf(&artifact.pdf)?;
    let document = lopdf::Document::load_mem(&pdf).map_err(|_| "pdf_invalid")?;
    let pages = document.get_pages();
    let first = pages.values().next().ok_or("invalid_page_range")?;
    let budget = publication_budget::capture();
    budget.check()?;
    let rendered = image_export::raster_page(&pdf, &artifact.pdf, 0, data, assets);
    budget.check()?;
    let first_page = match rendered {
        Ok(bytes) => Some(STANDARD.encode(preview_png(bytes)?)),
        Err(error) if budget.interrupted(&error) => return Err(error),
        // Legacy consumers can display the PDF themselves; the desktop client
        // can install its page renderer or retry and must see the actual error.
        Err(error) if paged => return Err(error),
        Err(_) => None,
    };
    let need_pdf = !paged;
    let (dimensions, page_text) =
        describe_page(&document, *first, 1, paged && first_page.is_some());
    let text = if paged && first_page.is_some() {
        page_text
    } else {
        crate::pdf_text::extract_document(&document, &pdf).unwrap_or_default()
    };
    let mut value = json!({
        "ok":true, "key":artifact.key, "format":artifact.format, "firstPage":first_page,
        "pages":pages.len(), "dimensions":dimensions, "text":text,
        "mode":if artifact.format=="pdf" {"artifact"} else {"shared-layout"},
        "previewEngine":artifact.projection_engine, "sourceMap":artifact.source_map,
        "warnings":artifact.warnings
    });
    if need_pdf {
        value["pdf"] = Value::String(STANDARD.encode(pdf));
    }
    budget.check()?;
    Ok(value)
}

pub(crate) fn page_for_request(
    artifact: &Artifact,
    index: usize,
    paths: &AppPaths,
) -> Result<Value, String> {
    page(artifact, index, &paths.data_dir, &paths.assets_dir)
}

pub(super) fn page(
    artifact: &Artifact,
    index: usize,
    data: &Path,
    assets: &Path,
) -> Result<Value, String> {
    publication_budget::run(
        "pdf",
        &json!({"pdf":{"engine":artifact.projection_engine}}),
        || {
            if index == 0 {
                return Err("invalid_page_range".into());
            }
            let pdf = read_pdf(&artifact.pdf)?;
            let doc = lopdf::Document::load_mem(&pdf).map_err(|_| "pdf_invalid")?;
            let n = u32::try_from(index).map_err(|_| "invalid_page_range")?;
            let id = doc
                .get_pages()
                .get(&n)
                .copied()
                .ok_or("invalid_page_range")?;
            let (dimensions, text) = describe_page(&doc, id, n, true);
            let png = preview_png(image_export::raster_page(
                &pdf,
                &artifact.pdf,
                index - 1,
                data,
                assets,
            )?)?;
            publication_budget::check()?;
            Ok(
                json!({"ok":true, "key":artifact.key, "page":index, "png":STANDARD.encode(png),"dimensions":dimensions,"text":text}),
            )
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Document, Object};

    #[test]
    fn thumbnail_and_full_preview_use_the_rotated_visible_sheet() {
        let mut doc = Document::new();
        let media = doc.add_object(Object::from(vec![
            0.into(),
            0.into(),
            600.into(),
            800.into(),
        ]));
        let root = doc.add_object(dictionary! {"MediaBox"=>media,"Rotate"=>-90});
        let branch = doc.add_object(dictionary! {"Parent"=>root,"CropBox"=>vec![10.into(), 20.into(), 510.into(), 900.into()]});
        let rotated = doc.add_object(dictionary! {"Parent"=>branch});
        let upright = doc.add_object(dictionary! {"Parent"=>branch,"Rotate"=>0});
        assert_eq!(sheet_bounds(&doc, rotated), Some([0., 0., 780., 500.]));
        assert_eq!(sheet_bounds(&doc, upright), Some([10., 20., 510., 800.]));
    }
}
