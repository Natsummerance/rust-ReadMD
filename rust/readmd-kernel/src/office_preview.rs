//! Read-only DOCX/PPTX tabs. The document path is used only to capture the
//! selected file; page turns use the immutable layout of that specific open.
use crate::{
    document_operations::{self, Observation, UnsupportedReason},
    server::{app_token_authorized, Request, Response},
    ApiError, ApiResult, App,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use uuid::Uuid;

#[path = "office_page_cache.rs"]
mod page_cache;

const MAX_VIEWS: usize = 16;
const MAX_LAYOUT_BYTES: usize = 128 * 1024 * 1024;
const VIEW_IDLE: Duration = Duration::from_secs(30 * 60);

#[derive(Debug)]
struct Layout {
    pdf: Vec<u8>,
    source: String,
}

#[cfg(test)]
#[path = "office_page_cache_integration_tests.rs"]
mod page_cache_tests;

struct View {
    layout: Arc<Layout>,
    used: Instant,
}

#[derive(Default)]
struct Retained {
    views: HashMap<Uuid, View>,
    pages: page_cache::Cache,
}

impl Retained {
    fn remove(&mut self, id: Uuid) -> Option<View> {
        self.pages.remove_view(id);
        self.views.remove(&id)
    }

    fn expire(&mut self) {
        let expired = self
            .views
            .iter()
            .filter(|(_, view)| view.used.elapsed() >= VIEW_IDLE)
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        for id in expired {
            self.remove(id);
        }
    }
}

#[derive(Default)]
pub(crate) struct Views(Mutex<Retained>);

impl Views {
    fn remember(&self, layout: Layout) -> ApiResult<Uuid> {
        if layout.pdf.len() > MAX_LAYOUT_BYTES {
            return Err(error("office_preview_too_large"));
        }
        let mut retained = self.0.lock().unwrap_or_else(|e| e.into_inner());
        retained.expire();
        let mut total: usize = retained.views.values().map(|v| v.layout.pdf.len()).sum();
        while retained.views.len() >= MAX_VIEWS || total + layout.pdf.len() > MAX_LAYOUT_BYTES {
            let Some(oldest) = retained
                .views
                .iter()
                .min_by_key(|(_, v)| v.used)
                .map(|(id, _)| *id)
            else {
                break;
            };
            if let Some(view) = retained.remove(oldest) {
                total -= view.layout.pdf.len();
            }
        }
        let id = Uuid::new_v4();
        retained.views.insert(
            id,
            View {
                layout: Arc::new(layout),
                used: Instant::now(),
            },
        );
        Ok(id)
    }
    fn get(&self, id: Uuid) -> ApiResult<Arc<Layout>> {
        let mut retained = self.0.lock().unwrap_or_else(|e| e.into_inner());
        retained.expire();
        let view = retained
            .views
            .get_mut(&id)
            .ok_or_else(|| error("office_preview_expired"))?;
        view.used = Instant::now();
        Ok(Arc::clone(&view.layout))
    }

    fn page(
        &self,
        id: Uuid,
        layout: &Arc<Layout>,
        index: usize,
        render: impl FnOnce() -> ApiResult<(usize, Vec<u8>)>,
    ) -> ApiResult<Arc<page_cache::Page>> {
        if crate::speech::cancelled() {
            return Err(error("cancelled"));
        }
        {
            let mut retained = self.0.lock().unwrap_or_else(|e| e.into_inner());
            if !retained
                .views
                .get(&id)
                .is_some_and(|view| Arc::ptr_eq(&view.layout, layout))
            {
                return Err(error("office_preview_expired"));
            }
            if let Some(page) = retained.pages.get(id, index) {
                return Ok(page);
            }
        }
        // Rasterization owns its existing worker/workspace limits. Never hold
        // the cache mutex across a worker call or block another view's close.
        let (count, bytes) = render()?;
        if crate::speech::cancelled() {
            return Err(error("cancelled"));
        }
        let page = Arc::new(page_cache::Page { count, bytes });
        let mut retained = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if !retained
            .views
            .get(&id)
            .is_some_and(|view| Arc::ptr_eq(&view.layout, layout))
        {
            return Err(error("office_preview_expired"));
        }
        retained.pages.insert(id, index, Arc::clone(&page));
        Ok(page)
    }

    fn close(&self, id: Uuid) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).remove(id);
    }
}

fn error(code: &str) -> ApiError {
    let code = if code == "office_preview_cancelled" {
        "cancelled"
    } else {
        code
    };
    let status = match code {
        "office_preview_unavailable" | "pdf_engine_unavailable" => 503,
        "office_preview_expired" => 410,
        "office_preview_busy" => 429,
        "office_preview_too_large" => 413,
        "file_not_found" => 404,
        "office_preview_changed" | "cancelled" => 409,
        "office_preview_timeout" => 504,
        "unsupported_format" => 415,
        _ => 422,
    };
    ApiError::new(status, code)
}

// JSON body paths are already decoded. A literal "%20" in a filename is data.
fn source_path(app: &App, raw: &str) -> ApiResult<PathBuf> {
    let path = Path::new(raw);
    if raw.is_empty() {
        return Err(error("invalid_path"));
    }
    let joined = app.paths.workspace.join(path);
    let resolved = dunce::canonicalize(joined).map_err(|_| error("file_not_found"))?;
    if !path.is_absolute() {
        app.paths
            .check_doc_allowed(&resolved)
            .map_err(|_| ApiError::new(403, "path_denied"))?;
    }
    Ok(resolved)
}

fn layout(app: &App, raw: &str, expected: Option<&str>) -> ApiResult<Layout> {
    let path = source_path(app, raw)?;
    let ext = path
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(ext.as_str(), "docx" | "pptx") {
        return Err(error("unsupported_format"));
    }
    let observed = document_operations::observe(&path, document_operations::MAX_CAPTURE_BYTES)
        .map_err(|_| error("office_preview_failed").noted("stage", "capture"))?;
    let file = match observed {
        Observation::Regular(file) => file,
        Observation::Missing => return Err(error("file_not_found")),
        Observation::Unsupported(UnsupportedReason::TooLarge) => {
            return Err(error("office_preview_too_large"))
        }
        Observation::Unsupported(_) => return Err(error("office_preview_failed")),
    };
    let fp = file.fingerprint();
    // An evicted layout can only be rebuilt from the same file and revision.
    let source =
        crate::crypto::sha256_hex(format!("{}\0{}", fp.identity.as_str(), fp.revision).as_bytes());
    if expected.is_some_and(|previous| previous != source) {
        return Err(error("office_preview_changed"));
    }
    let name = path.file_name().ok_or_else(|| error("invalid_path"))?;
    let output = crate::office_render::render_pdf(name, &ext, file.bytes(), &app.paths)
        .map_err(|code| error(code).noted("stage", "layout"))?;
    if crate::speech::cancelled() {
        return Err(error("cancelled"));
    }
    if !document_operations::matches_publication(
        &path,
        fp.identity.as_str(),
        &fp.revision,
        Some(file.bytes().len() as u64),
    )
    .unwrap_or(false)
    {
        return Err(error("office_preview_changed"));
    }
    Ok(Layout {
        pdf: output,
        source,
    })
}

fn page_image(app: &Arc<App>, layout: &[u8], page: usize) -> ApiResult<(usize, Vec<u8>)> {
    let runtime = crate::capability_delivery::office_pdfium(app).map_err(|s| {
        if s == "cancelled" {
            error("cancelled")
        } else {
            error("pdf_engine_unavailable")
        }
    })?;
    let temp = crate::task_workspace::Workspace::create(&app.paths.data_dir, "office-page")
        .map_err(|_| error("office_preview_failed").noted("stage", "page_workspace"))?;
    temp.run_to_completion(|| {
        let pdf = temp.path().join("page-source.pdf");
        crate::task_workspace::write(&pdf, layout)
            .map_err(|_| error("office_preview_failed").noted("stage", "page_copy"))?;
        crate::pdf_workspace::raster_with_runtime(&runtime, &pdf, page).map_err(|s| {
            match s.as_str() {
                "cancelled" => error("cancelled"),
                "pdf_busy" => error("office_preview_busy"),
                _ => error("office_preview_failed")
                    .noted("stage", "raster")
                    .noted("cause", s),
            }
        })
    })
    .map_err(|_| error("office_preview_failed").noted("stage", "page_finish"))?
}

fn page_response(bytes: Vec<u8>, id: Uuid, page: usize, count: usize, source: &str) -> Response {
    Response::raw_bytes(200, "image/png", bytes)
        .header("Cache-Control", "no-store")
        .header("X-ReadMD-View", &id.to_string())
        .header("X-ReadMD-Source", source)
        .header("X-ReadMD-Page", &page.to_string())
        .header("X-ReadMD-Pages", &count.to_string())
}

fn handle(app: &Arc<App>, v: &Value) -> ApiResult<Response> {
    let page = v
        .get("page")
        .map(|p| {
            p.as_u64()
                .filter(|n| *n < 10000)
                .map(|n| n as usize)
                .ok_or_else(|| error("invalid_page"))
        })
        .transpose()?;
    if let Some(view) = v.get("view") {
        let id = view
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| error("office_preview_expired"))?;
        if v.get("path").is_some() {
            return Err(error("invalid_path"));
        }
        if v["close"] == true {
            app.office_views.close(id);
            return Ok(Response::json(&json!({"ok":true})));
        }
        let page = page.ok_or_else(|| error("invalid_page"))?;
        let layout = app.office_views.get(id)?;
        let image = app
            .office_views
            .page(id, &layout, page, || page_image(app, &layout.pdf, page))?;
        return Ok(page_response(
            image.bytes.clone(),
            id,
            page,
            image.count,
            &layout.source,
        ));
    }
    if v.get("close").is_some() {
        return Err(error("invalid_path"));
    }
    let raw = v["path"].as_str().ok_or_else(|| error("invalid_path"))?;
    let expected = v
        .get("expected_source")
        .map(|value| {
            value
                .as_str()
                .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
                .ok_or_else(|| error("invalid_request"))
        })
        .transpose()?;
    let layout = layout(app, raw, expected)?;
    if let Some(page) = page {
        let (count, bytes) = page_image(app, &layout.pdf, page)?;
        if crate::speech::cancelled() {
            return Err(error("cancelled"));
        }
        let id = app.office_views.remember(layout)?;
        let retained = app.office_views.get(id)?;
        let image = app
            .office_views
            .page(id, &retained, page, || Ok((count, bytes)))
            .map_err(|error| {
                app.office_views.close(id);
                error
            })?;
        Ok(page_response(
            image.bytes.clone(),
            id,
            page,
            image.count,
            &retained.source,
        ))
    } else {
        Ok(Response::raw_bytes(200, "application/pdf", layout.pdf)
            .header("Cache-Control", "no-store"))
    }
}

pub(crate) fn endpoint(app: &Arc<App>, req: &Request) -> ApiResult<Response> {
    if req.method != "POST" {
        return Err(ApiError::new(405, "method_not_allowed"));
    }
    if !app_token_authorized(app, req) {
        return Err(ApiError::new(403, "forbidden"));
    }
    let body = req.json()?;
    let task = body["task_id"].as_str().and_then(crate::cancel::register);
    match task {
        Some(task) => crate::speech::with_cancel(task.checker(), || handle(app, &body)),
        None => handle(app, &body),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_paths_keep_literal_percent_and_do_not_grant_relative_parent_access() {
        let temp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(temp.path()).unwrap();
        let docs = root.join("docs");
        std::fs::create_dir(&docs).unwrap();
        let app = Arc::new(
            App::bootstrap(crate::paths::AppPaths::with_dirs(
                &root.join("profile"),
                &docs,
                &root.join("assets"),
            ))
            .unwrap(),
        );
        let path = docs.join("一%20二.docx");
        std::fs::write(&path, b"selected").unwrap();
        std::fs::write(docs.join("一 二.docx"), b"other").unwrap();
        std::fs::write(root.join("private.docx"), b"outside").unwrap();
        assert_eq!(source_path(&app, "一%20二.docx").unwrap(), path);
        assert_eq!(
            source_path(&app, "../private.docx").unwrap_err().status,
            403
        );
        let req = Request {
            method: "POST".into(),
            path: "/api/office/preview".into(),
            headers: Default::default(),
            query: Default::default(),
            body: serde_json::to_vec(&json!({"path":path})).unwrap(),
        };
        assert!(matches!(endpoint(&app, &req), Err(e) if e.status == 403));
        let (status, listing) = crate::server::call_in_process(
            &app,
            "GET",
            &format!(
                "/api/list?p={}&view=reader",
                crate::convert::py_url_quote(docs.to_str().unwrap(), "")
            ),
            None,
        );
        assert_eq!(status, 200);
        assert_eq!(listing["files"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn retained_layouts_are_bounded_and_close_is_idempotent() {
        let views = Views::default();
        let id = views
            .remember(Layout {
                pdf: vec![1],
                source: String::new(),
            })
            .unwrap();
        let reading = views.get(id).unwrap();
        for n in 0..MAX_VIEWS {
            views
                .remember(Layout {
                    pdf: vec![n as u8],
                    source: String::new(),
                })
                .unwrap();
        }
        assert_eq!(views.get(id).unwrap_err().status, 410);
        assert_eq!(reading.pdf.as_slice(), &[1]);
        views.close(id);
        views.close(id);
    }
}
