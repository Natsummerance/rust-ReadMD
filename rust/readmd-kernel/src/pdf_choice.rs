//! Native single-selection AcroForm combo boxes. PDFium owns export values and
//! appearances; the journal records an option index, never a displayed label.
use pdfium_render::prelude::*;
use serde_json::{json, Value};
use std::collections::HashSet;

pub fn options(field: &PdfFormComboBoxField<'_>) -> Option<Vec<Value>> {
    if field.has_editable_text_box()
        || field.is_multiselect()
        || field.options().is_empty()
        || field.options().len() > 256
    {
        return None;
    }
    let mut bytes = 0usize;
    field
        .options()
        .iter()
        .map(|option| {
            let label = option.label().cloned().unwrap_or_default();
            bytes = bytes.saturating_add(label.len());
            (bytes <= 65536)
                .then(|| json!({"index":option.index(),"label":label,"selected":option.is_set()}))
        })
        .collect()
}

pub fn select(
    doc: &PdfDocument<'_>,
    page: &PdfPage<'_>,
    page_index: i32,
    annotation_index: usize,
    option: usize,
    bindings: &dyn PdfiumLibraryBindings,
) -> Result<(), &'static str> {
    if doc.pages().len() > 512
        || doc
            .form()
            .is_none_or(|f| f.form_type() != PdfFormType::Acrobat)
    {
        return Err("pdf_unsupported_object");
    }
    let annotation = page
        .annotations()
        .get(annotation_index)
        .map_err(|_| "pdf_invalid_operation")?;
    let field = annotation.as_form_field().ok_or("pdf_invalid_operation")?;
    if field.is_read_only() {
        return Err("pdf_read_only");
    }
    let combo = field.as_combo_box_field().ok_or("pdf_unsupported_object")?;
    let choices = options(combo).ok_or("pdf_unsupported_object")?;
    if option >= choices.len() {
        return Err("pdf_invalid_operation");
    }
    let name = field
        .name()
        .filter(|s| !s.is_empty())
        .ok_or("pdf_unsupported_object")?;
    drop(annotation);

    // The wrapper doesn't expose its form handle. Use the public PDFium ABI
    // with a separate, pinned environment; no JS, network or action callbacks.
    let mut info = Box::pin(unsafe { std::mem::zeroed::<FPDF_FORMFILLINFO>() });
    info.version = 2;
    info.xfa_disabled = 1;
    let form = unsafe {
        bindings.FPDFDOC_InitFormFillEnvironment(
            bindings.get_handle_from_document(doc),
            info.as_mut().get_mut(),
        )
    };
    if form.is_null() {
        return Err("pdf_operation_failed");
    }
    let mut related = Vec::new();
    let mut loaded = false;
    let result = (|| {
        let mut controls = HashSet::new();
        let mut expected = None;
        for n in 0..doc.pages().len() {
            let other = if n == page_index {
                None
            } else {
                Some(doc.pages().get(n).map_err(|_| "pdf_invalid_page")?)
            };
            let candidate = other.as_ref().unwrap_or(page);
            let mut matches = false;
            for (id, annotation) in candidate.annotations().iter().enumerate() {
                let Some(field) = annotation
                    .as_form_field()
                    .filter(|f| f.name().as_deref() == Some(&name))
                else {
                    continue;
                };
                if field.is_read_only() {
                    return Err("pdf_read_only");
                }
                if field.as_combo_box_field().and_then(options).as_ref() != Some(&choices) {
                    return Err("pdf_unsupported_object");
                }
                let raw = unsafe {
                    bindings.FPDFPage_GetAnnot(bindings.get_handle_from_page(candidate), id as i32)
                };
                if raw.is_null() {
                    return Err("pdf_operation_failed");
                }
                let count = unsafe { bindings.FPDFAnnot_GetFormControlCount(form, raw) };
                let index = unsafe { bindings.FPDFAnnot_GetFormControlIndex(form, raw) };
                let linked = unsafe { bindings.FPDFAnnot_HasKey(raw, "Parent") != 0 };
                unsafe {
                    bindings.FPDFPage_CloseAnnot(raw);
                }
                if !(1..=64).contains(&count)
                    || count > 1 && !linked
                    || !(0..count).contains(&index)
                    || expected.is_some_and(|n| n != count)
                    || !controls.insert(index)
                {
                    return Err("pdf_unsupported_object");
                }
                expected = Some(count);
                matches = true;
            }
            if matches {
                if let Some(other) = other {
                    related.push(other);
                }
            }
        }
        if expected != Some(controls.len() as i32) {
            return Err("pdf_unsupported_object");
        }
        // Every linked widget must have a page view before committing, or
        // PDFium updates /V but leaves unopened sibling /AP streams stale.
        for p in std::iter::once(page).chain(related.iter()) {
            unsafe {
                bindings.FORM_OnAfterLoadPage(bindings.get_handle_from_page(p), form);
            }
        }
        loaded = true;
        let raw = unsafe {
            bindings.FPDFPage_GetAnnot(bindings.get_handle_from_page(page), annotation_index as i32)
        };
        if raw.is_null() {
            return Err("pdf_operation_failed");
        }
        let selected = unsafe {
            bindings.FORM_SetFocusedAnnot(form, raw) != 0
                && bindings.FORM_SetIndexSelected(
                    form,
                    bindings.get_handle_from_page(page),
                    option as i32,
                    1,
                ) != 0
        };
        let committed = unsafe { bindings.FORM_ForceToKillFocus(form) != 0 };
        unsafe {
            bindings.FPDFPage_CloseAnnot(raw);
        }
        if selected && committed {
            Ok(())
        } else {
            Err("pdf_operation_failed")
        }
    })();
    if loaded {
        for p in std::iter::once(page).chain(related.iter()) {
            unsafe {
                bindings.FORM_OnBeforeClosePage(bindings.get_handle_from_page(p), form);
            }
        }
    }
    unsafe {
        bindings.FPDFDOC_ExitFormFillEnvironment(form);
    }
    result
}
