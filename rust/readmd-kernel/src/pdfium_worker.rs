use base64::{engine::general_purpose::STANDARD, Engine};
use pdfium_render::prelude::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path};

fn fail<T>(_: T) -> &'static str {
    "pdf_operation_failed"
}
fn index(v: &Value, key: &str) -> Result<usize, &'static str> {
    v[key]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or("pdf_invalid_operation")
}
fn number(v: &Value, key: &str) -> Result<f32, &'static str> {
    v[key]
        .as_f64()
        .filter(|n| n.is_finite() && n.abs() <= 100000.0)
        .map(|n| n as f32)
        .ok_or("pdf_invalid_operation")
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, &'static str> {
    v[key]
        .as_str()
        .filter(|s| s.len() <= 65536 && !s.contains('\0'))
        .ok_or("pdf_invalid_operation")
}
fn supported_font(font: &PdfFont<'_>, source: &str) -> bool {
    if source.chars().all(|c| c == ' ' || c.is_ascii_graphic()) {
        return true;
    }
    font.data()
        .ok()
        .and_then(|bytes| {
            ttf_parser::Face::parse(&bytes, 0).ok().map(|face| {
                source
                    .chars()
                    .all(|c| c.is_whitespace() || face.glyph_index(c).is_some())
            })
        })
        .unwrap_or(false)
}

fn apply(doc: &mut PdfDocument<'_>, ops: &[Value], request: &Value) -> Result<(), &'static str> {
    if ops.len() > 4096 {
        return Err("pdf_limit");
    }
    for op in ops {
        let kind = text(op, "kind")?;
        let font = if kind == "text" && op["font"].as_str().is_some_and(|s| !s.is_empty()) {
            let name = text(op, "font")?;
            let path = request["fonts"][name].as_str().ok_or("pdf_missing_glyph")?;
            if fs::metadata(path).map_err(fail)?.len() > 32 * 1024 * 1024 {
                return Err("pdf_limit");
            }
            let bytes = fs::read(path).map_err(fail)?;
            let face = ttf_parser::Face::parse(&bytes, 0).map_err(|_| "pdf_missing_glyph")?;
            if !face.is_outline_embedding_allowed()
                || !matches!(
                    face.permissions(),
                    Some(ttf_parser::Permissions::Installable | ttf_parser::Permissions::Editable)
                )
            {
                return Err("pdf_missing_glyph");
            }
            if !text(op, "text")?
                .chars()
                .all(|c| c.is_whitespace() || face.glyph_index(c).is_some())
            {
                return Err("pdf_missing_glyph");
            }
            Some(
                doc.fonts_mut()
                    .load_true_type_from_bytes(&bytes, true)
                    .map_err(fail)?,
            )
        } else {
            None
        };
        let page_index = i32::try_from(index(op, "page")?).map_err(|_| "pdf_invalid_operation")?;
        let mut page = doc
            .pages()
            .get(page_index)
            .map_err(|_| "pdf_invalid_page")?;
        match kind {
            "text" => {
                let mut object = page.objects().get(index(op, "object")?).map_err(fail)?;
                let target = object.as_text_object_mut().ok_or("pdf_invalid_operation")?;
                // OCR/formula semantic layers cannot be edited independently
                // of the visible image or vector art they describe.
                if !target.is_visible() {
                    return Err("pdf_unsupported_object");
                }
                let replacement = text(op, "text")?;
                let original = target.bounds().map_err(fail)?;
                if let Some(font) = font {
                    if !matches!(target.render_mode(), PdfPageTextRenderMode::FilledUnstroked) {
                        return Err("pdf_unsupported_object");
                    }
                    let mut new =
                        PdfPageTextObject::new(doc, replacement, font, target.unscaled_font_size())
                            .map_err(fail)?;
                    new.apply_matrix(target.matrix().map_err(fail)?)
                        .map_err(fail)?;
                    new.set_fill_color(target.fill_color().map_err(fail)?)
                        .map_err(fail)?;
                    drop(object);
                    page.objects_mut()
                        .remove_object_at_index(index(op, "object")?)
                        .map_err(fail)?;
                    object = page
                        .objects_mut()
                        .insert_object_at_index(index(op, "object")?, PdfPageObject::Text(new))
                        .map_err(fail)?;
                } else {
                    if !supported_font(&target.font(), replacement) {
                        return Err("pdf_missing_glyph");
                    }
                    target.set_text(replacement).map_err(fail)?;
                }
                let target = object.as_text_object().ok_or("pdf_invalid_operation")?;
                if target.text().trim() != replacement.trim() {
                    return Err("pdf_missing_glyph");
                }
                let after = target.bounds().map_err(fail)?;
                // Do not silently shrink or spill into adjacent content.
                if !op["allow_overflow"].as_bool().unwrap_or(false)
                    && after.width().value > original.width().value + 1.0
                {
                    return Err("pdf_text_overflow");
                }
                drop(object);
                page.regenerate_content().map_err(fail)?;
            }
            "translate" => {
                let mut object = page.objects().get(index(op, "object")?).map_err(fail)?;
                object
                    .translate(
                        PdfPoints::new(number(op, "x")?),
                        PdfPoints::new(number(op, "y")?),
                    )
                    .map_err(fail)?;
                drop(object);
                page.regenerate_content().map_err(fail)?;
            }
            "image" => {
                let asset = text(op, "asset")?;
                if asset.len() != 64 || !asset.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err("pdf_invalid_operation");
                }
                let path = Path::new(text(request, "asset_root")?).join(asset);
                let w = i32::try_from(index(op, "width")?).map_err(fail)?;
                let h = i32::try_from(index(op, "height")?).map_err(fail)?;
                if w <= 0 || h <= 0 || (w as u64) * (h as u64) > 4_000_000 {
                    return Err("pdf_limit");
                }
                let expected = (w as u64) * (h as u64) * 4;
                if fs::metadata(&path).map_err(fail)?.len() != expected {
                    return Err("pdf_invalid_operation");
                }
                let mut bytes = fs::read(path).map_err(fail)?;
                if format!("{:x}", Sha256::digest(&bytes)) != asset {
                    return Err("pdf_invalid_operation");
                }
                for p in bytes.chunks_exact_mut(4) {
                    p.swap(0, 2);
                }
                let bitmap =
                    PdfBitmap::from_bytes(w, h, PdfBitmapFormat::BGRA, &mut bytes).map_err(fail)?;
                let mut object = page.objects().get(index(op, "object")?).map_err(fail)?;
                object
                    .as_image_object_mut()
                    .ok_or("pdf_invalid_operation")?
                    .set_bitmap(&bitmap)
                    .map_err(fail)?;
                drop(object);
                page.regenerate_content().map_err(fail)?;
            }
            "remove" => {
                if page.objects().get(index(op, "object")?).map_err(fail)?
                    .as_text_object().is_some_and(|text| !text.is_visible()) {
                    return Err("pdf_unsupported_object");
                }
                page.objects_mut()
                    .remove_object_at_index(index(op, "object")?)
                    .map_err(fail)?;
                page.regenerate_content().map_err(fail)?;
            }
            "rotate" => {
                let turns = index(op, "turns")?;
                page.set_rotation(match turns % 4 {
                    0 => PdfPageRenderRotation::None,
                    1 => PdfPageRenderRotation::Degrees90,
                    2 => PdfPageRenderRotation::Degrees180,
                    _ => PdfPageRenderRotation::Degrees270,
                });
            }
            "delete_page" => {
                if doc.pages().len() <= 1 {
                    return Err("pdf_last_page");
                }
                page.delete().map_err(fail)?;
            }
            "note" => {
                let mut note = page
                    .annotations_mut()
                    .create_text_annotation(text(op, "text")?)
                    .map_err(fail)?;
                let x = number(op, "x")?;
                let y = number(op, "y")?;
                note.set_bounds(PdfRect::new_from_values(y, x, y + 20.0, x + 20.0))
                    .map_err(fail)?;
            }
            "field" => {
                let annotations = page.annotations_mut();
                let mut annotation = annotations.get(index(op, "annotation")?).map_err(fail)?;
                let field = annotation
                    .as_form_field_mut()
                    .ok_or("pdf_invalid_operation")?;
                if field.is_read_only() {
                    return Err("pdf_read_only");
                }
                if let Some(target) = field.as_text_field_mut() {
                    target.set_value(text(op, "text")?).map_err(fail)?;
                } else if let Some(target) = field.as_checkbox_field_mut() {
                    target
                        .set_checked(op["checked"].as_bool().ok_or("pdf_invalid_operation")?)
                        .map_err(fail)?;
                } else {
                    return Err("pdf_unsupported_object");
                }
            }
            "annotation" | "remove_annotation" => {
                let annotations = page.annotations_mut();
                let mut annotation = annotations.get(index(op, "annotation")?).map_err(fail)?;
                if annotation.annotation_type() != PdfPageAnnotationType::Text {
                    return Err("pdf_unsupported_object");
                }
                if kind == "annotation" {
                    annotation.set_contents(text(op, "text")?).map_err(fail)?;
                } else {
                    annotations.delete_annotation(annotation).map_err(fail)?;
                }
            }
            _ => return Err("pdf_invalid_operation"),
        }
    }
    Ok(())
}

fn bitmap_bmp(bitmap: &PdfBitmap<'_>) -> Result<Vec<u8>, &'static str> {
    let (width, height) = (bitmap.width(), bitmap.height());
    if width <= 0 || height <= 0 || (width as u64) * (height as u64) > 8_000_000 {
        return Err("pdf_limit");
    }
    let rgba = bitmap.as_rgba_bytes();
    let mut out = vec![0u8; 54];
    out[..2].copy_from_slice(b"BM");
    out[2..6].copy_from_slice(&((54 + rgba.len()) as u32).to_le_bytes());
    out[10..14].copy_from_slice(&54u32.to_le_bytes());
    out[14..18].copy_from_slice(&40u32.to_le_bytes());
    out[18..22].copy_from_slice(&width.to_le_bytes());
    out[22..26].copy_from_slice(&(-height).to_le_bytes());
    out[26..28].copy_from_slice(&1u16.to_le_bytes());
    out[28..30].copy_from_slice(&32u16.to_le_bytes());
    for p in rgba.chunks_exact(4) {
        out.extend_from_slice(&[p[2], p[1], p[0], 255]);
    }
    Ok(out)
}
fn device_bounds(
    page: &PdfPage<'_>,
    rect: PdfRect,
    config: &PdfRenderConfig,
) -> Result<Value, &'static str> {
    let points = [
        (rect.left(), rect.top()),
        (rect.right(), rect.top()),
        (rect.left(), rect.bottom()),
        (rect.right(), rect.bottom()),
    ];
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for (x, y) in points {
        let (px, py) = page.points_to_pixels(x, y, config).map_err(fail)?;
        xs.push(px);
        ys.push(py);
    }
    let (left, right) = (*xs.iter().min().unwrap(), *xs.iter().max().unwrap());
    let (top, bottom) = (*ys.iter().min().unwrap(), *ys.iter().max().unwrap());
    Ok(json!({"x":left,"y":top,"width":right-left,"height":bottom-top}))
}

fn destination_data(destination: &PdfDestination<'_>, pages: i32) -> Option<Value> {
    let page = destination.page_index().ok()?;
    if page < 0 || page >= pages { return None; }
    let (x, y) = match destination.view_settings().ok()? {
        PdfDestinationViewSettings::SpecificCoordinatesAndZoom(x, y, _) => (x, y),
        PdfDestinationViewSettings::FitPageHorizontallyToWindow(y)
        | PdfDestinationViewSettings::FitBoundsHorizontallyToWindow(y) => (None, y),
        PdfDestinationViewSettings::FitPageVerticallyToWindow(x)
        | PdfDestinationViewSettings::FitBoundsVerticallyToWindow(x) => (x, None),
        PdfDestinationViewSettings::FitPageToRectangle(r) => (Some(r.left()), Some(r.top())),
        _ => (None, None),
    };
    Some(json!({"kind":"internal","page":page,"x":x.map(|p|p.value),"y":y.map(|p|p.value)}))
}

fn safe_link_uri(uri: &str) -> bool {
    let lower = uri.to_ascii_lowercase();
    uri.len() <= 8192 && !uri.chars().any(char::is_control)
        && ["https://", "http://", "mailto:"].iter().any(|prefix| lower.starts_with(prefix))
}
fn normalize_annotations(path: &Path, password: &str) -> Result<(), &'static str> {
    // PDFium's annotation constructor appends a direct dictionary. Other PDF
    // readers require annotation dictionaries to be indirect objects. Normalize
    // only these entries; keep page content streams and existing object ids.
    if fs::metadata(path).map_err(fail)?.len() > 256 * 1024 * 1024 {
        return Err("pdf_limit");
    }
    let mut document =
        lopdf::Document::load_with_password(path, password).map_err(|_| "pdf_save_failed")?;
    if document.objects.len() > 500_000 {
        return Err("pdf_limit");
    }
    if document.is_encrypted() {
        document.decrypt(password).map_err(|_| "pdf_save_failed")?;
    }
    let state = document.encryption_state.clone();
    if document.get_pages().is_empty() {
        return Err("pdf_save_failed");
    }
    let mut changed = false;
    for id in document.get_pages().into_values() {
        let Some(raw) = document
            .get_object(id)
            .ok()
            .and_then(|o| o.as_dict().ok())
            .and_then(|d| d.get(b"Annots").ok())
            .cloned()
        else {
            continue;
        };
        let (array_id, array) = match raw {
            lopdf::Object::Reference(id) => (
                Some(id),
                document
                    .get_object(id)
                    .ok()
                    .and_then(|o| o.as_array().ok())
                    .cloned(),
            ),
            lopdf::Object::Array(array) => (None, Some(array)),
            _ => (None, None),
        };
        let Some(mut array) = array else { continue };
        let mut page_changed = false;
        for item in &mut array {
            if let lopdf::Object::Dictionary(dictionary) = item {
                let reference = document.add_object(dictionary.clone());
                *item = lopdf::Object::Reference(reference);
                page_changed = true;
            }
        }
        if page_changed {
            changed = true;
            let array = lopdf::Object::Array(array);
            if let Some(id) = array_id {
                document.objects.insert(id, array);
            } else {
                document
                    .get_object_mut(id)
                    .map_err(fail)?
                    .as_dict_mut()
                    .map_err(fail)?
                    .set("Annots", array);
            }
        }
    }
    if changed {
        if let Some(state) = state {
            document.encrypt(&state).map_err(|_| "pdf_save_failed")?;
        }
        document.save(path).map_err(|_| "pdf_save_failed")?;
    }
    Ok(())
}
fn execute(request: &Value) -> Result<Value, &'static str> {
    let library = text(request, "library")?;
    let bindings = Pdfium::bind_to_library(library).map_err(|_| "pdf_engine_unavailable")?;
    // Pdfium::new moves this Box into a process-wide OnceCell without moving
    // its allocation. It stays alive until this one-request worker exits.
    // The wrapper's permission enum omits AES revisions 5/6; use PDFium's
    // official permission bits for every security revision instead.
    let native = bindings.as_ref() as *const dyn PdfiumLibraryBindings;
    let pdfium = Pdfium::new(bindings);
    if request["action"] == "probe" {
        let mut doc = pdfium.create_new_pdf().map_err(fail)?;
        let font = doc.fonts_mut().helvetica();
        let mut page = doc
            .pages_mut()
            .create_page_at_end(PdfPagePaperSize::a4())
            .map_err(fail)?;
        page.objects_mut()
            .create_text_object(
                PdfPoints::new(30.0),
                PdfPoints::new(700.0),
                "ReadMD PDF engine probe",
                font,
                PdfPoints::new(12.0),
            )
            .map_err(fail)?;
        let bitmap = page
            .render_with_config(&PdfRenderConfig::new().set_target_width(128))
            .map_err(fail)?;
        if page
            .text()
            .map_err(fail)?
            .all()
            .contains("ReadMD PDF engine probe")
            && bitmap.width() == 128
        {
            return Ok(json!({"ok":true,"engine":"pdfium","version":"7881"}));
        }
        return Err("pdf_engine_unavailable");
    }
    let mut doc = pdfium
        .load_pdf_from_file(
            text(request, "pdf")?,
            request["password"].as_str().filter(|s| !s.is_empty()),
        )
        .map_err(|_| "pdf_open_failed")?;
    let (permissions, security_revision) = unsafe {
        let bindings = &*native;
        let document = bindings.FPDF_LoadDocument(
            text(request, "pdf")?,
            request["password"].as_str().filter(|s| !s.is_empty()),
        );
        if document.is_null() {
            return Err("pdf_open_failed");
        }
        let permissions = bindings.FPDF_GetDocPermissions(document);
        let revision = bindings.FPDF_GetSecurityHandlerRevision(document);
        bindings.FPDF_CloseDocument(document);
        (permissions, revision)
    };
    let can_edit = permissions & 8 != 0;
    let can_annotate = permissions & 32 != 0;
    let can_fill = permissions & 256 != 0 || security_revision == 2 && can_annotate;
    let can_assemble = can_edit || permissions & 1024 != 0;
    let signatures = doc.signatures().len();
    let ops = request["operations"]
        .as_array()
        .ok_or("pdf_invalid_operation")?;
    if !ops.is_empty() {
        if ops
            .iter()
            .any(|op| !match op["kind"].as_str().unwrap_or("") {
                "note" | "annotation" | "remove_annotation" => can_annotate,
                "field" => can_fill,
                "rotate" | "delete_page" => can_assemble,
                _ => can_edit,
            })
        {
            return Err("pdf_read_only");
        }
        if signatures > 0 && !request["allow_signatures"].as_bool().unwrap_or(false) {
            return Err("pdf_signed");
        }
        apply(&mut doc, ops, request)?;
    }
    if request["action"] == "save" {
        let output = Path::new(text(request, "output")?);
        doc.save_to_file(output).map_err(|_| "pdf_save_failed")?;
        normalize_annotations(output, request["password"].as_str().unwrap_or(""))?;
        return Ok(json!({"ok":true,"pages":doc.pages().len()}));
    }
    let n = i32::try_from(request["page"].as_u64().unwrap_or(0))
        .map_err(fail)?
        .min(doc.pages().len() - 1);
    let page = doc.pages().get(n).map_err(|_| "pdf_invalid_page")?;
    let width = request["width"].as_i64().unwrap_or(1200).clamp(320, 2400) as i32;
    let config = PdfRenderConfig::new()
        .set_target_width(width)
        .set_maximum_height(3200)
        .render_annotations(true);
    let bitmap = page.render_with_config(&config).map_err(fail)?;
    let mut objects = Vec::new();
    if page.objects().len() > 20000 {
        return Err("pdf_limit");
    }
    for (id, object) in page.objects().iter().enumerate() {
        let Ok(bounds) = object.bounds() else {
            continue;
        };
        let mut item = json!({"id":id,"kind":"object","bounds":device_bounds(&page,bounds.to_rect(),&config)?});
        if let Some(text) = object.as_text_object() {
            item["kind"] = json!("text");
            item["text"] = json!(text.text());
            item["font"] = json!(text.font().name());
            item["readonly"] = json!(!text.is_visible());
        } else if object.as_image_object().is_some() {
            item["kind"] = json!("image");
        }
        objects.push(item);
    }
    if page.annotations().len() > 20000 || page.links().len() > 20000 { return Err("pdf_limit"); }
    let mut links = Vec::new();
    for (id, link) in page.links().iter().enumerate() {
        let Ok(bounds) = link.rect() else { continue };
        let mut target = link.destination().and_then(|d| destination_data(&d, doc.pages().len()));
        if target.is_none() {
            if let Some(action) = link.action() {
                target = match action {
                    PdfAction::LocalDestination(action) => action.destination().ok().and_then(|d| destination_data(&d, doc.pages().len())),
                    PdfAction::Uri(action) => action.uri().ok().filter(|uri| safe_link_uri(uri))
                        .map(|uri| json!({"kind":"external","uri":uri})),
                    _ => None,
                };
            }
        }
        if let Some(target) = target {
            links.push(json!({"id":id,"bounds":device_bounds(&page,bounds,&config)?,"target":target}));
        }
    }
    let mut annotations = Vec::new();
    for (id, annotation) in page.annotations().iter().enumerate() {
        let bounds = annotation.bounds().map_err(fail)?;
        let mut item = json!({"id":id,"kind":if annotation.annotation_type()==PdfPageAnnotationType::Text{"note"}else{"unsupported_annotation"},"text":annotation.contents().unwrap_or_default(),"bounds":device_bounds(&page,bounds,&config)?});
        if let Some(field) = annotation.as_form_field() {
            item["name"] = json!(field.name().unwrap_or_default());
            item["readonly"] = json!(field.is_read_only());
            if let Some(t) = field.as_text_field() {
                item["kind"] = json!("field");
                item["text"] = json!(t.value().unwrap_or_default());
            } else if let Some(t) = field.as_checkbox_field() {
                item["kind"] = json!("checkbox");
                item["checked"] = json!(t.is_checked().map_err(fail)?);
            } else {
                item["kind"] = json!("unsupported_field");
            }
        }
        annotations.push(item);
    }
    let origin = page.pixels_to_points(0, 0, &config).map_err(fail)?;
    let px = page.pixels_to_points(1, 0, &config).map_err(fail)?;
    let py = page.pixels_to_points(0, 1, &config).map_err(fail)?;
    Ok(
        json!({"ok":true,"pages":doc.pages().len(),"page":n,"can_edit":can_edit,"permissions":{"content":can_edit,"annotations":can_annotate,"forms":can_fill,"pages":can_assemble},"signatures":signatures,"rotation":page.rotation().map_err(fail)? as i32,
        "width":bitmap.width(),"height":bitmap.height(),"image":STANDARD.encode(bitmap_bmp(&bitmap)?),"objects":objects,"annotations":annotations,"links":links,
        "coordinates":{"origin":[origin.0.value,origin.1.value],"x":[px.0.value-origin.0.value,px.1.value-origin.1.value],"y":[py.0.value-origin.0.value,py.1.value-origin.1.value]}}),
    )
}
pub fn run() -> Result<(), &'static str> {
    let mut args = std::env::args_os().skip(1);
    let input = args.next().ok_or("pdf_invalid_operation")?;
    let output = args.next().ok_or("pdf_invalid_operation")?;
    if args.next().is_some() {
        return Err("pdf_invalid_operation");
    }
    if fs::metadata(&input).map_err(fail)?.len() > 2 * 1024 * 1024 {
        return Err("pdf_limit");
    }
    let mut request: Value =
        serde_json::from_slice(&fs::read(input).map_err(fail)?).map_err(fail)?;
    let mut password = String::new();
    std::io::stdin()
        .lock()
        .take(1025)
        .read_to_string(&mut password)
        .map_err(fail)?;
    if password.len() > 1024 || password.contains('\0') {
        return Err("pdf_invalid_operation");
    }
    if !password.is_empty() {
        request["password"] = json!(password);
    }
    let result = match execute(&request) {
        Ok(v) => v,
        Err(code) => json!({"ok":false,"error_code":code}),
    };
    fs::write(output, serde_json::to_vec(&result).map_err(fail)?).map_err(fail)
}
