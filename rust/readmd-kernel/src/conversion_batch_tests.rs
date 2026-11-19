use super::*;
use crate::server::call_in_process;
use std::time::Instant;

fn make_app(root: &Path) -> Arc<App> {
    Arc::new(
        App::bootstrap(crate::paths::AppPaths::with_dirs(
            &root.join("data"),
            &root.join("workspace"),
            &root.join("assets"),
        ))
        .unwrap(),
    )
}

fn fixture() -> (tempfile::TempDir, Arc<App>, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(temp.path()).unwrap();
    let source = root.join("source");
    let destination = root.join("destination");
    for dir in [
        &source,
        &destination,
        &root.join("data"),
        &root.join("workspace"),
        &root.join("assets"),
    ] {
        fs::create_dir(dir).unwrap();
    }
    (temp, make_app(&root), source, destination)
}

fn batch_post(app: &Arc<App>, input: Value) -> (u16, Value) {
    let mut body = input;
    body["confirm"] = json!(true);
    body["workbench"] = json!(true);
    call_in_process(app, "POST", "/api/convert/batch", Some(&body))
}

fn progress_path(job: &str) -> String {
    format!("/api/convert/progress?job={job}")
}

fn finished(app: &Arc<App>, job: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    let path = progress_path(job);
    loop {
        let (status, body) = call_in_process(app, "GET", &path, None);
        assert_eq!(status, 200, "{body}");
        if body["finished"] == true {
            return body;
        }
        assert!(Instant::now() < deadline, "{body}");
        std::thread::sleep(Duration::from_millis(12));
    }
}

fn docx_fixture(path: &Path, text: &str) {
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:body></w:document>"#
    );
    let parts: Vec<(String,Vec<u8>)>=vec![("[Content_Types].xml".into(),br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.to_vec()),("word/document.xml".into(),xml.into_bytes())];
    let entries: Vec<_> = parts
        .into_iter()
        .map(|(path, data)| crate::mdexport::ZipEntry {
            path,
            data,
            compress: true,
        })
        .collect();
    fs::write(path, crate::mdexport::write_zip(&entries).unwrap()).unwrap();
}

#[test]
fn batch_http_outputs_open_after_app_restart_and_retry_keeps_completed_files() {
    let (temp, app, source, destination) = fixture();
    let selected = source.join("报告.txt");
    let duplicate = source.join(".").join("报告.txt");
    let second = source.join("报告.csv");
    let missing = source.join("缺页.txt");
    fs::write(&selected, "# First input\n\nFirst batch result.").unwrap();
    fs::write(&second, "name,value\nalpha,2\nbeta,3\n").unwrap();
    fs::write(source.join("报告.md"), b"an existing user file").unwrap();
    let (status, start) = batch_post(
        &app,
        json!({
            "paths": [selected, duplicate, second, missing],
            "output_parent": destination,
            "overwrite": true,
        }),
    );
    assert_eq!(status, 200, "{start}");
    assert_eq!(start["total"], 3);
    let job = start["job"].as_str().unwrap();
    let first = finished(&app, job);
    let out = PathBuf::from(first["output_dir"].as_str().unwrap());
    assert_eq!(out.parent().unwrap(), destination);
    assert_eq!(first["items"][0]["status"], "ok");
    assert_eq!(first["items"][1]["status"], "ok");
    assert_eq!(first["items"][2]["error_code"], "file_not_found");
    assert_eq!(first["items"][0]["fidelity_report"]["schema"], 1);
    assert_eq!(
        first["items"][0]["fidelity_report"]["inspection"],
        "unavailable"
    );
    assert_eq!(
        first["items"][0]["fidelity_report"]["extraction"]["status"],
        "complete"
    );
    assert!(first["items"][2].get("fidelity_report").is_none());
    let first_md = out.join("001 - 报告.md");
    let second_md = out.join("002 - 报告.md");
    let missing_md = out.join("003 - 缺页.md");
    assert_eq!(first["items"][0]["out"], json!(first_md));
    assert_eq!(first["items"][1]["out"], json!(second_md));
    assert!(!missing_md.exists());
    assert_eq!(
        fs::read(source.join("报告.md")).unwrap(),
        b"an existing user file"
    );
    assert_eq!(fs::read_dir(&source).unwrap().count(), 3);
    assert!(fs::read_to_string(&second_md).unwrap().contains("alpha"));

    drop(app);
    let app = make_app(&dunce::canonicalize(temp.path()).unwrap());
    let (status, reopened) = call_in_process(&app, "GET", "/api/convert/progress?latest=1", None);
    assert_eq!(status, 200, "{reopened}");
    assert_eq!(first, reopened);
    let p = percent_encoding::utf8_percent_encode(
        first_md.to_str().unwrap(),
        percent_encoding::NON_ALPHANUMERIC,
    );
    let (status, read) = call_in_process(&app, "GET", &format!("/api/file?p={p}"), None);
    assert_eq!(status, 200, "{read}");
    assert!(
        read["content"]
            .as_str()
            .unwrap()
            .contains("First batch result")
    );

    fs::write(
        &selected,
        b"source changed after first successful conversion",
    )
    .unwrap();
    let (status, later) = batch_post(&app, json!({"paths": [selected]}));
    assert_eq!(status, 200, "{later}");
    let later_job = later["job"].as_str().unwrap();
    let later_receipt = finished(&app, later_job);
    assert_ne!(later_receipt["output_dir"], first["output_dir"]);
    let (status, history) = call_in_process(&app, "GET", "/api/convert/progress?history=1", None);
    assert_eq!(status, 200, "{history}");
    assert_eq!(history["batches"][0]["job"], later_job);
    assert_eq!(history["batches"][1]["job"], job);
    assert_eq!(history["batches"][1]["fidelity_reports"], 2);
    assert!(history["batches"][1].get("items").is_none());
    let (status, previous) = call_in_process(&app, "GET", &progress_path(job), None);
    assert_eq!(status, 200);
    assert_eq!(previous, first);

    fs::write(&first_md, b"manual edits to completed output").unwrap();
    fs::write(&missing, b"valid document restored for retry").unwrap();
    let (status, retry) = batch_post(&app, json!({"retry": job, "attempt": 1}));
    assert_eq!(status, 200, "{retry}");
    let final_job = finished(&app, job);
    assert_eq!(final_job["attempt"], 2);
    assert_eq!(final_job["done"], 3);
    assert_eq!(
        final_job["items"][0]["fidelity_report"],
        first["items"][0]["fidelity_report"]
    );
    assert_eq!(final_job["items"][2]["out"], json!(missing_md));
    assert!(
        fs::read_to_string(&missing_md)
            .unwrap()
            .contains("valid document restored")
    );
    assert_eq!(
        fs::read(&first_md).unwrap(),
        b"manual edits to completed output"
    );
    assert_eq!(fs::read_dir(&source).unwrap().count(), 4);

    let (status, stale_retry) = batch_post(&app, json!({"retry": job, "attempt": 1}));
    assert_eq!(status, 409, "{stale_retry}");
    assert_eq!(stale_retry["error_code"], "batch_attempt_changed");
}

#[test]
fn cancel_during_second_parse_keeps_first_result_and_discards_current_extraction() {
    let (_temp, app, source, _destination) = fixture();
    let paths = [
        source.join("a.txt"),
        source.join("b.txt"),
        source.join("c.txt"),
    ];
    for path in &paths {
        fs::write(path, b"owned source").unwrap();
    }
    let (lease, directory, mut batch) = prepare_batch(&app, &json!({"paths": paths})).unwrap();
    let output = batch.output_dir.clone();
    let job = batch.id.clone();
    let cancellation = Arc::new(Mutex::new(false));
    ACTIVE
        .lock()
        .unwrap()
        .insert(directory.clone(), cancellation.clone());
    let data = app.paths.data_dir.clone();
    let outcome = run_items(
        &data,
        &lease,
        &directory,
        &mut batch,
        &cancellation,
        |_, path, _, _| {
            if path == paths[1] {
                let (status, progress) = call_in_process(&app, "GET", &progress_path(&job), None);
                assert_eq!(status, 200);
                assert_eq!(progress["items"][1]["status"], "running");
                let (status, cancel) =
                    call_in_process(&app, "POST", "/api/task/cancel", Some(&json!({"id": job})));
                assert_eq!(status, 200, "{cancel}");
                assert_eq!(cancel["state"], "cancelling");
                assert!(crate::speech::cancelled());
            }
            assert_ne!(path, paths[2], "cancelled row must not run");
            crate::convert::ConvertTriple {
                text: "test extraction".into(),
                engine: "test".into(),
                error: None,
                assets: Vec::new(),
            }
        },
    );
    ACTIVE.lock().unwrap().remove(&directory);
    outcome.unwrap();
    drop(lease);
    let done = finished(&app, &job);
    assert_eq!(done["items"][0]["status"], "ok");
    assert_eq!(done["items"][0]["fidelity_report"]["schema"], 1);
    for idx in [1, 2] {
        assert_eq!(done["items"][idx]["status"], "canceled");
        assert!(done["items"][idx].get("out").is_none());
        assert!(done["items"][idx].get("fidelity_report").is_none());
    }
    assert_eq!(fs::read_dir(output).unwrap().count(), 1);
    for path in paths {
        assert_eq!(fs::read(path).unwrap(), b"owned source");
    }
}

#[test]
fn recovery_recognizes_only_the_exact_prepared_publication() {
    let (_temp, app, source, _) = fixture();
    for (same_file, file_content) in [
        (true, b"expected".as_slice()),
        (false, b"expected".as_slice()),
        (false, b"third-party file".as_slice()),
    ] {
        let paths = [source.join("first.txt"), source.join("second.txt")];
        for path in &paths {
            fs::write(path, b"owned source").unwrap();
        }
        let (lease, dir, mut batch) = prepare_batch(&app, &json!({"paths": paths})).unwrap();
        let job = batch.id.clone();
        let target = PathBuf::from(batch.items[0].result.planned_out.as_deref().unwrap());
        let staged = tempfile::NamedTempFile::new_in(&batch.output_dir).unwrap();
        fs::write(staged.path(), b"expected").unwrap();
        let files::IdentityObservation::Regular(identity) =
            files::observe_identity(staged.path()).unwrap()
        else {
            panic!("temporary file identity unavailable");
        };
        batch.items[0].result.status = "running".into();
        batch.items[0].result.engine = Some("txtmd".into());
        let extracted = crate::convert::ConvertTriple {
            text: "expected".into(),
            engine: "txtmd".into(),
            error: None,
            assets: Vec::new(),
        };
        let report =
            crate::convert::fidelity_source::report_if_current(&paths[0], &extracted, None);
        batch.items[0].result.fidelity_report = Some(report.clone());
        batch.pending = Some(Publication {
            length: None,
            index: 0,
            identity: identity.as_str().to_owned(),
            sha256: format!("{:x}", Sha256::digest(b"expected")),
        });
        persist(&lease, &dir, &batch).unwrap();
        if same_file {
            staged.persist_noclobber(&target).unwrap();
        } else {
            fs::write(&target, file_content).unwrap();
        }
        drop(lease);

        let recovered = finished(&app, &job);
        assert_eq!(
            recovered["items"][0]["status"],
            if same_file { "ok" } else { "error" }
        );
        assert_eq!(recovered["items"][1]["error_code"], "batch_interrupted");
        if same_file {
            assert_eq!(recovered["items"][0]["fidelity_report"], report);
        } else {
            assert!(recovered["items"][0].get("fidelity_report").is_none());
        }
        assert_eq!(fs::read(&target).unwrap(), file_content);
        if !same_file {
            let (status, reply) = batch_post(&app, json!({"retry": job, "attempt": 1}));
            assert_eq!(status, 200, "{reply}");
            let result = finished(&app, &job);
            assert_eq!(result["items"][0]["error_code"], "output_exists");
            assert_eq!(result["items"][1]["status"], "ok");
            assert_eq!(fs::read(&target).unwrap(), file_content);
        }
    }
}

#[cfg(unix)]
#[test]
fn workbench_collect_refuses_to_follow_unselected_directory_links_and_report_partial_success() {
    let (_temp, app, source, destination) = fixture();
    fs::write(source.join("selected.txt"), b"fixture").unwrap();
    fs::write(destination.join("outside.txt"), b"fixture").unwrap();
    std::os::unix::fs::symlink(&destination, source.join("linked-directory")).unwrap();
    std::os::unix::fs::symlink(
        destination.join("outside.txt"),
        source.join("linked-file.txt"),
    )
    .unwrap();
    let p = percent_encoding::utf8_percent_encode(
        source.to_str().unwrap(),
        percent_encoding::NON_ALPHANUMERIC,
    );
    let url = format!("/api/convert/collect?workbench=1&dir={p}");
    let (status, result) = call_in_process(&app, "GET", &url, None);
    assert_eq!(status, 200);
    assert_eq!(result["files"], json!([source.join("selected.txt")]));
    for index in 0..MAX_ITEMS {
        fs::write(
            source.join(format!("source-{index:03}.txt")),
            b"another fixture",
        )
        .unwrap();
    }
    let (status, failed) = call_in_process(&app, "GET", &url, None);
    assert_eq!(status, 413);
    assert_eq!(failed["error_code"], "batch_too_many_files");
}

#[test]
fn retry_refuses_a_replaced_output_directory_and_never_creates_a_source_sibling() {
    let (_temp, app, source, destination) = fixture();
    let missing = source.join("missing.txt");
    let (status, reply) = batch_post(
        &app,
        json!({"paths": [missing], "output_parent": destination}),
    );
    assert_eq!(status, 200);
    let job = reply["job"].as_str().unwrap();
    let completed = finished(&app, job);
    let output = PathBuf::from(completed["output_dir"].as_str().unwrap());
    let old = destination.join("moved");
    fs::rename(&output, &old).unwrap();
    fs::create_dir(&output).unwrap();
    fs::write(&missing, b"now a real file").unwrap();
    let (status, rejected) = batch_post(&app, json!({"retry": job, "attempt": 1}));
    assert_eq!(status, 422);
    assert_eq!(rejected["error_code"], "batch_output_unavailable");
    assert_eq!(fs::read_dir(output).unwrap().count(), 0);
    assert_eq!(fs::read_dir(old).unwrap().count(), 0);
    assert!(!source.join("missing.md").exists());
}

#[test]
fn receipts_require_the_current_app_token_and_profile() {
    let (_temp, app, source, _) = fixture();
    let path = source.join("safe.txt");
    fs::write(&path, b"safe fixture").unwrap();
    let (_lease, _, batch) = prepare_batch(&app, &json!({"paths": [path]})).unwrap();
    let request = Request {
        method: "GET".into(),
        path: "/api/convert/progress".into(),
        query: Default::default(),
        headers: Default::default(),
        body: Vec::new(),
    };
    assert_eq!(
        progress(&app, &request, &batch.id).err().unwrap().status,
        403
    );
    let (_other_temp, other, _, _) = fixture();
    let (status, _) = call_in_process(&other, "GET", &progress_path(&batch.id), None);
    assert_eq!(status, 404);
}

#[test]
fn moving_output_preserves_relative_markdown_links_and_images() {
    let (_temp, _app, source, destination) = fixture();
    let original = concat!(
        "[page](chapter.md#title) ![image][image-ref]\n\n",
        "[image-ref]: <figures/A B.png> \"title\"\n\n",
        "[remote](https://example.org/a) [anchor](#title) ![staged](/raw?p=staged)\n\n",
        "\x60\x60\x60md\n[literal](sample.md)\n\x60\x60\x60\n",
    );
    let result = relocate_relative_links(
        original.to_owned(),
        &source.join("sub dir").join("sample.txt"),
        &destination.join("batch"),
    );
    assert!(result.contains("[page](../../source/sub%20dir/chapter.md#title)"));
    assert!(result.contains("[image-ref]: <../../source/sub%20dir/figures/A B.png> \"title\""));
    assert!(
        result
            .contains("[remote](https://example.org/a) [anchor](#title) ![staged](/raw?p=staged)")
    );
    assert!(result.contains("\n[literal](sample.md)\n"));
}

#[test]
fn large_batch_output_recovery_streams_the_recorded_length_and_identity() {
    let temp = tempfile::tempdir().unwrap();
    let path = dunce::canonicalize(temp.path()).unwrap().join("large.md");
    let length = files::MAX_CAPTURE_BYTES + 1;
    fs::File::create(&path).unwrap().set_len(length).unwrap();
    let files::IdentityObservation::Regular(id) = files::observe_identity(&path).unwrap() else {
        panic!("expected an ordinary file");
    };
    let mut hash = Sha256::new();
    let zeros = [0u8; 65536];
    for _ in 0..length / 65536 {
        hash.update(zeros);
    }
    hash.update(&zeros[..(length % 65536) as usize]);
    let expected = format!("{:x}", hash.finalize());
    assert!(files::matches_publication(&path, id.as_str(), &expected, Some(length)).unwrap());
    assert!(!files::matches_publication(&path, id.as_str(), &expected, Some(length - 1)).unwrap());
    assert!(!files::matches_publication(&path, id.as_str(), &expected, None).unwrap());
}

#[test]
fn batch_fidelity_binds_successful_docx_and_never_invents_a_binding_for_errors_or_changed_source() {
    let (_temp, app, source, _) = fixture();
    let path = source.join("fidelity.docx");
    for scenario in ["stable", "changed", "error", "empty"] {
        docx_fixture(&path, "Original paragraph");
        let original_bytes = fs::read(&path).unwrap();
        let revision = format!("{:x}", Sha256::digest(&original_bytes));
        let (lease, dir, mut batch) = prepare_batch(&app, &json!({"paths":[path]})).unwrap();
        run_items(
            &app.paths.data_dir,
            &lease,
            &dir,
            &mut batch,
            &Arc::new(Mutex::new(false)),
            |data, source, language, captured| {
                assert_eq!(captured, Some(original_bytes.as_slice()));
                if scenario == "changed" {
                    docx_fixture(&path, "Replaced paragraph");
                }
                if matches!(scenario, "stable" | "changed") {
                    return convert_item(data, source, language, captured);
                }
                crate::convert::ConvertTriple {
                    text: if scenario == "error" {
                        String::new()
                    } else if scenario == "empty" {
                        "  \n".into()
                    } else {
                        "Original paragraph".into()
                    },
                    engine: "docx".into(),
                    error: (scenario == "error").then(|| "conversion_failed".into()),
                    assets: Vec::new(),
                }
            },
        )
        .unwrap();
        let report = &batch.items[0].result.fidelity_report;
        if matches!(scenario, "error" | "empty") {
            assert_eq!(batch.items[0].result.status, "error");
            assert!(report.is_none());
            assert!(!Path::new(batch.items[0].result.planned_out.as_deref().unwrap()).exists());
        } else {
            assert_eq!(batch.items[0].result.status, "ok");
            let text = fs::read_to_string(batch.items[0].result.out.as_deref().unwrap()).unwrap();
            assert!(text.contains("Original paragraph"), "{text}");
            assert!(!text.contains("Replaced paragraph"));
            let report = report.as_ref().unwrap();
            assert_eq!(report["schema"], 1);
            if scenario == "stable" {
                assert_eq!(report["source_revision"], revision);
            } else {
                assert_eq!(report["inspection"], "unavailable");
                assert!(report.get("source_revision").is_none());
            }
            let stored = read_receipt(&dir, &batch.id).unwrap();
            assert_eq!(
                stored.items[0].result.fidelity_report,
                batch.items[0].result.fidelity_report
            );
        }
    }
}

#[test]
fn interrupted_publication_keeps_its_bound_report_and_old_receipts_never_backfill_from_changed_input()
 {
    let (_temp, app, source, _) = fixture();
    let path = source.join("interrupted.docx");
    for with_report in [true, false] {
        docx_fixture(&path, "Original input");
        let before = crate::convert::fidelity_source::observe(&path);
        let converted = crate::convert::ConvertTriple {
            text: "Original input".into(),
            engine: "docx".into(),
            error: None,
            assets: Vec::new(),
        };
        let report =
            crate::convert::fidelity_source::report_if_current(&path, &converted, before.as_ref());
        let (lease, dir, mut batch) = prepare_batch(&app, &json!({"paths":[path]})).unwrap();
        let target = PathBuf::from(batch.items[0].result.planned_out.as_deref().unwrap());
        let stage = tempfile::NamedTempFile::new_in(&batch.output_dir).unwrap();
        fs::write(stage.path(), converted.text.as_bytes()).unwrap();
        let files::IdentityObservation::Regular(identity) =
            files::observe_identity(stage.path()).unwrap()
        else {
            panic!("missing staged identity")
        };
        batch.items[0].result.status = "running".into();
        batch.items[0].result.engine = Some(converted.engine.clone());
        batch.items[0].result.fidelity_report = with_report.then(|| report.clone());
        batch.pending = Some(Publication {
            index: 0,
            identity: identity.as_str().into(),
            sha256: format!("{:x}", Sha256::digest(converted.text.as_bytes())),
            length: Some(converted.text.len() as u64),
        });
        persist(&lease, &dir, &batch).unwrap();
        stage.persist_noclobber(&target).unwrap();
        docx_fixture(&path, "Changed after conversion");
        drop(lease);
        let recovered = finished(&app, &batch.id);
        assert_eq!(recovered["items"][0]["status"], "ok");
        assert_eq!(fs::read_to_string(&target).unwrap(), "Original input");
        if with_report {
            assert_eq!(recovered["items"][0]["fidelity_report"], report);
            assert_ne!(
                report["source_revision"],
                format!("{:x}", Sha256::digest(fs::read(&path).unwrap()))
            );
        } else {
            assert!(recovered["items"][0].get("fidelity_report").is_none());
        }
    }
}

#[test]
fn legacy_batch_reports_successful_extraction_and_refuses_to_overwrite_on_later_failure() {
    let (_temp, app, source, _) = fixture();
    let path = source.join("legacy.txt");
    fs::write(&path, "Original extraction\n").unwrap();
    let input = json!({"paths":[path],"confirm":true,"overwrite":false});
    let (status, start) = call_in_process(&app, "POST", "/api/convert/batch", Some(&input));
    assert_eq!(status, 200, "{start}");
    let job = start["job"].as_str().unwrap();
    let first = finished(&app, job);
    assert_eq!(first["items"][0]["status"], "ok");
    assert_eq!(first["items"][0]["fidelity_report"]["schema"], 1);
    assert_eq!(
        first["items"][0]["fidelity_report"]["inspection"],
        "unavailable"
    );
    let target = PathBuf::from(first["items"][0]["out"].as_str().unwrap());
    fs::write(&target, "User-edited output").unwrap();
    fs::write(&path, "Changed source input").unwrap();
    let (status, start) = call_in_process(&app, "POST", "/api/convert/batch", Some(&input));
    assert_eq!(status, 200);
    let later = finished(&app, start["job"].as_str().unwrap());
    assert_ne!(later["items"][0]["status"], "ok");
    assert!(later["items"][0].get("fidelity_report").is_none());
    assert_eq!(fs::read_to_string(&target).unwrap(), "User-edited output");
    let (status, unchanged) = call_in_process(&app, "GET", &progress_path(job), None);
    assert_eq!(status, 200);
    assert_eq!(unchanged, first);
}

#[cfg(unix)]
#[test]
fn receipt_reobserve_never_accepts_a_stably_hardlinked_record() {
    let (_temp, app, source, _) = fixture();
    let path = source.join("source.txt");
    fs::write(&path, "source").unwrap();
    let (_lease, dir, batch) = prepare_batch(&app, &json!({"paths":[path]})).unwrap();
    let receipt = dir.join(RECORD_FILE);
    let before = fs::read(&receipt).unwrap();
    fs::hard_link(&receipt, dir.join("second-link.json")).unwrap();
    assert_eq!(read_receipt(&dir, &batch.id).err().unwrap().status, 507);
    assert_eq!(fs::read(&receipt).unwrap(), before);
    assert!(!Path::new(batch.items[0].result.planned_out.as_deref().unwrap()).exists());
}
