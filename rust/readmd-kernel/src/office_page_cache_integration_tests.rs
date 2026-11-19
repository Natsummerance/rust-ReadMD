//! The retained-layout lifecycle gates the cache before any PDFium/runtime or
//! temporary workspace is needed. Expired/closed views cannot serve old PNGs.
use super::*;
use std::cell::Cell;

fn retained(views: &Views) -> (Uuid, Arc<Layout>) {
    let id = views
        .remember(Layout {
            pdf: b"immutable PDF snapshot".to_vec(),
            source: "a".repeat(64),
        })
        .unwrap();
    (id, views.get(id).unwrap())
}

#[test]
fn back_and_forward_page_turns_only_raster_each_requested_page_once() {
    let views = Views::default();
    let (id, _) = retained(&views);
    let calls = Cell::new(0);
    assert_eq!(
        calls.get(),
        0,
        "remembering a layout must not prefetch pages"
    );
    for index in [0, 1, 0, 1, 2, 1, 0] {
        let layout = views.get(id).unwrap();
        let image = views
            .page(id, &layout, index, || {
                calls.set(calls.get() + 1);
                Ok((3, vec![index as u8; 5]))
            })
            .unwrap();
        assert_eq!(image.count, 3);
        assert_eq!(image.bytes, vec![index as u8; 5]);
    }
    assert_eq!(
        calls.get(),
        3,
        "cached page turns must not rewrite the PDF or raster it again"
    );
}

#[test]
fn real_endpoint_serves_cached_png_without_an_installed_raster_runtime_and_keeps_headers() {
    let root = tempfile::tempdir().unwrap();
    let app = Arc::new(
        App::bootstrap(crate::paths::AppPaths::with_dirs(
            &root.path().join("profile"),
            root.path(),
            &root.path().join("missing-assets"),
        ))
        .unwrap(),
    );
    let (id, layout) = retained(&app.office_views);
    let png = b"\x89PNG\r\n\x1a\nowned page bytes".to_vec();
    app.office_views
        .page(id, &layout, 0, || Ok((2, png.clone())))
        .unwrap();
    let request = |body: Value| Request {
        method: "POST".into(),
        path: "/api/office/preview".into(),
        query: Default::default(),
        headers: HashMap::from([("x-readmd-app-token".into(), app.app_token.clone())]),
        body: serde_json::to_vec(&body).unwrap(),
    };
    let response = endpoint(&app, &request(json!({"view":id,"page":0}))).unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(response.body, png);
    for (key, value) in [
        ("Content-Type", "image/png"),
        ("Cache-Control", "no-store"),
        ("X-ReadMD-Page", "0"),
        ("X-ReadMD-Pages", "2"),
        ("X-ReadMD-Source", layout.source.as_str()),
    ] {
        assert!(
            response
                .headers
                .iter()
                .any(|(name, actual)| name == key && actual == value),
            "{key}"
        );
    }
    endpoint(&app, &request(json!({"view":id,"close":true}))).unwrap();
    assert!(
        matches!(endpoint(&app, &request(json!({"view":id,"page":0}))),Err(error) if error.status==410)
    );
}

#[test]
fn idle_and_capacity_eviction_drop_cached_bytes_even_if_a_reader_holds_the_pdf() {
    for idle in [true, false] {
        let views = Views::default();
        let (id, layout) = retained(&views);
        let image = views.page(id, &layout, 0, || Ok((2, vec![4; 4]))).unwrap();
        let weak = Arc::downgrade(&image);
        drop(image);
        if idle {
            views.0.lock().unwrap().views.get_mut(&id).unwrap().used =
                Instant::now() - VIEW_IDLE - Duration::from_secs(1);
        } else {
            for _ in 0..MAX_VIEWS {
                retained(&views);
            }
        }
        assert_eq!(views.get(id).unwrap_err().status, 410);
        assert!(
            weak.upgrade().is_none(),
            "retired layouts must immediately free their cached PNGs"
        );
        assert!(
            !layout.pdf.is_empty(),
            "an in-flight PDF reference must not pin retired PNGs"
        );
        let calls = Cell::new(0);
        assert!(
            matches!(views.page(id,&layout,0,||{calls.set(1);Ok((2,vec![1]))}),Err(error) if error.status==410)
        );
        assert_eq!(calls.get(), 0);
    }
}

#[test]
fn closed_during_raster_and_failed_rasters_never_repopulate_the_cache() {
    let views = Views::default();
    let (id, layout) = retained(&views);
    assert!(views
        .page(id, &layout, 0, || Err(error("office_preview_failed")))
        .is_err());
    let result = views.page(id, &layout, 0, || {
        views.close(id);
        Ok((2, vec![7; 4]))
    });
    assert!(matches!(result, Err(error) if error.status == 410));
    assert_eq!(views.get(id).unwrap_err().status, 410);
    let (reopened, layout) = retained(&views);
    let calls = Cell::new(0);
    views
        .page(reopened, &layout, 0, || {
            calls.set(1);
            Ok((2, vec![9; 4]))
        })
        .unwrap();
    assert_eq!(
        calls.get(),
        1,
        "a separate open must render its own snapshot"
    );
}
