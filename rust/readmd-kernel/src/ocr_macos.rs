//! Offline Vision OCR, isolated in the current ReadMD executable. The parent
//! owns admission, deadline, cancellation and scratch cleanup. Only bytes and
//! bounded text cross the process boundary; no Objective-C object is Send.
use std::{
    fs,
    io::{Cursor, Read},
    sync::atomic::AtomicUsize,
    time::{Duration, Instant},
};

const MAX_INPUT: u64 = 64 * 1024 * 1024;
const MAX_OUTPUT: u64 = 1024 * 1024;
static ACTIVE: AtomicUsize = AtomicUsize::new(0);

fn check(deadline: Instant) -> Result<(), String> {
    if crate::speech::cancelled() {
        Err("cancelled".into())
    } else if Instant::now() >= deadline {
        Err("ocr_timeout".into())
    } else {
        Ok(())
    }
}

pub(crate) fn available() -> bool {
    probe().is_ok()
}

pub(crate) fn probe() -> Result<(), String> {
    static READY: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    if READY.get().is_some() {
        return Ok(());
    }
    // First system-model initialization can be slower than a warm recognition.
    // Failed/cancelled/busy probes remain retryable, never cached as absence.
    let text = run(
        include_bytes!("../../../assets/engines/ocr-probe.png"),
        Some("en"),
        Instant::now() + Duration::from_secs(45),
    )?;
    if !text.contains("READMD") || !text.contains("12345") {
        return Err("ocr_probe_failed".into());
    }
    let _ = READY.set(());
    Ok(())
}

pub(crate) fn image(bytes: &[u8], language: Option<&str>) -> Result<String, String> {
    run(bytes, language, Instant::now() + Duration::from_secs(90))
}

fn is_worker_executable(path: &std::path::Path) -> bool {
    path.file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("readmd"))
}

fn run(bytes: &[u8], language: Option<&str>, deadline: Instant) -> Result<String, String> {
    check(deadline)?;
    if bytes.len() as u64 > MAX_INPUT {
        return Err("ocr_invalid_image".into());
    }
    let paths = crate::ocr_portable::paths();
    let _slot = crate::capability_process::reserve_worker(&paths.data_dir, "ocr", &ACTIVE)
        .map_err(|e| e.code)?;
    let executable = std::env::current_exe().map_err(|_| "ocr_no_engine")?;
    // Library test harnesses and other embedders are not the worker protocol.
    if !is_worker_executable(&executable) {
        return Err("ocr_no_engine".into());
    }
    let work = crate::task_workspace::Workspace::create(&paths.data_dir, "macos-ocr")
        .map_err(|_| "ocr_failed")?;
    work.scope(|| {
        check(deadline)?;
        let input = work.path().join("input");
        crate::task_workspace::write(&input, bytes).map_err(|_| "ocr_failed")?;
        let input = fs::File::open(input).map_err(|_| "ocr_failed")?;
        let output = tempfile::NamedTempFile::new_in(work.path()).map_err(|_| "ocr_failed")?;
        let errors = tempfile::NamedTempFile::new_in(work.path()).map_err(|_| "ocr_failed")?;
        for file in [&output, &errors] {
            crate::task_workspace::track_created(file.path(), file.as_file())
                .map_err(|_| "ocr_failed")?;
        }
        let mut command = crate::silent_command(executable);
        command
            .arg("--readmd-macos-ocr-worker")
            .arg(language.unwrap_or("auto"))
            .env_clear()
            .env("CFFIXED_USER_HOME", work.path())
            .env("TMPDIR", work.path())
            .env("LANG", "en_US.UTF-8");
        let mut child = crate::worker_sandbox::spawn_managed_with_input(
            &mut command,
            work.path(),
            &input,
            output.as_file(),
            errors.as_file(),
        )
        .map_err(|_| "ocr_failed")?;
        loop {
            check(deadline)?; // ManagedWorker drops/kills/reaps on every exit.
            if [output.as_file(), errors.as_file()]
                .iter()
                .any(|file| file.metadata().map_or(true, |m| m.len() > MAX_OUTPUT))
            {
                return Err("ocr_failed".into());
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    let mut result = String::new();
                    output
                        .reopen()
                        .map_err(|_| "ocr_failed")?
                        .take(MAX_OUTPUT + 1)
                        .read_to_string(&mut result)
                        .map_err(|_| "ocr_failed")?;
                    check(deadline)?;
                    let value: serde_json::Value =
                        serde_json::from_str(&result).map_err(|_| "ocr_failed")?;
                    if status != 0 {
                        return Err(match value["error"].as_str() {
                            Some("ocr_invalid_image") => "ocr_invalid_image",
                            Some("ocr_language_unavailable") => "ocr_language_unavailable",
                            _ => "ocr_failed",
                        }
                        .into());
                    }
                    return value["text"]
                        .as_str()
                        .filter(|s| s.len() as u64 <= MAX_OUTPUT / 2)
                        .map(str::to_owned)
                        .ok_or_else(|| "ocr_failed".into());
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(20)),
                Err(_) => return Err("ocr_failed".into()),
            }
        }
    })
}

/// Private executable protocol. It cannot open an arbitrary input path, start
/// the GUI/server, or change a document. Parent termination stops recognition.
#[doc(hidden)]
pub fn worker_main(args: &[String]) -> i32 {
    let result = (|| {
        if args.len() != 1 || args[0].len() > 32 {
            return Err("ocr_failed".into());
        }
        let mut bytes = Vec::new();
        std::io::stdin()
            .take(MAX_INPUT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "ocr_failed")?;
        recognize(&bytes, Some(&args[0]))
    })();
    let (value, status) = match result {
        Ok(text) => (serde_json::json!({"text": text}), 0),
        Err(error) => (serde_json::json!({"error": error}), 1),
    };
    if serde_json::to_writer(std::io::stdout().lock(), &value).is_err() {
        return 1;
    }
    status
}

fn recognize(bytes: &[u8], language: Option<&str>) -> Result<String, String> {
    use objc2::AnyThread;
    use objc2_foundation::{NSArray, NSData, NSDictionary, NSString};
    use objc2_vision::{
        VNImageRequestHandler, VNRecognizeTextRequest, VNRequest, VNRequestTextRecognitionLevel,
    };
    if bytes.len() as u64 > MAX_INPUT {
        return Err("ocr_invalid_image".into());
    }
    // Decode before entering native code, with the same limits as portable OCR.
    // Flatten alpha onto white: Vision explicitly discards the alpha channel.
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| "ocr_invalid_image")?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().map_err(|_| "ocr_invalid_image")?;
    let (width, height) = (image.width(), image.height());
    if u64::from(width) * u64::from(height) > 25_000_000 {
        return Err("ocr_invalid_image".into());
    }
    let mut rgb = image.to_rgb8();
    for (out, pixel) in rgb.pixels_mut().zip(image.to_rgba8().pixels()) {
        for channel in 0..3 {
            out[channel] = ((u32::from(pixel[channel]) * u32::from(pixel[3])
                + 255 * (255 - u32::from(pixel[3])))
                / 255) as u8;
        }
    }
    let mut png = Cursor::new(Vec::new());
    rgb.write_to(&mut png, image::ImageFormat::Png)
        .map_err(|_| "ocr_invalid_image")?;
    objc2::rc::autoreleasepool(|_| {
        let request = VNRecognizeTextRequest::new();
        // File OCR is bounded and infrequent. Avoid compiling a fresh GPU
        // pipeline in every short-lived worker, especially on a cold install.
        #[allow(deprecated)]
        unsafe {
            request.setUsesCPUOnly(true);
        }
        request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
        request.setUsesLanguageCorrection(true);
        // Only query languages on this request's actual system revision.
        let supported = unsafe { request.supportedRecognitionLanguagesAndReturnError() }
            .map_err(|_| "ocr_no_engine")?;
        let desired: Vec<&str> = match language.unwrap_or("auto") {
            "en" | "eng" | "en-US" => vec!["en-US"],
            "zh" | "zh-CN" | "chi_sim" | "zh-Hans" => vec!["zh-Hans", "en-US"],
            "zh-TW" | "zh-HK" | "chi_tra" | "zh-Hant" => vec!["zh-Hant", "en-US"],
            "auto" | "" => vec!["zh-Hans", "zh-Hant", "en-US"],
            other => vec![other],
        };
        let languages: Vec<_> = desired
            .iter()
            .filter(|s| supported.iter().any(|v| v.to_string() == **s))
            .map(|s| NSString::from_str(s))
            .collect();
        if languages.is_empty() {
            return Err("ocr_language_unavailable".into());
        }
        request.setRecognitionLanguages(&NSArray::from_retained_slice(&languages));
        let handler = VNImageRequestHandler::initWithData_options(
            VNImageRequestHandler::alloc(),
            &NSData::with_bytes(png.get_ref()),
            &NSDictionary::new(),
        );
        handler
            .performRequests_error(&NSArray::from_slice(&[&*request as &VNRequest]))
            .map_err(|_| "ocr_failed")?;
        let observations = request.results().ok_or("ocr_failed")?;
        if observations.len() > 65536 {
            return Err("ocr_failed".into());
        }
        let mut items = Vec::with_capacity(observations.len());
        let mut size = 0usize;
        for observation in observations.iter() {
            if let Some(candidate) = observation.topCandidates(1).firstObject() {
                let text = candidate.string().to_string();
                size += text.len();
                if size as u64 > MAX_OUTPUT / 2 {
                    return Err("ocr_failed".into());
                }
                // Vision's lower-left normalized box becomes top-left pixels,
                // matching the shared XY-cut column/line reconstruction.
                let rect = unsafe { observation.boundingBox() };
                items.push(crate::ocr::LayoutItem {
                    text,
                    x_left: rect.origin.x * f64::from(width),
                    y_top: (1.0 - rect.origin.y - rect.size.height) * f64::from(height),
                    width: rect.size.width * f64::from(width),
                    height: rect.size.height * f64::from(height),
                });
            }
        }
        Ok(crate::ocr::xy_cut_lines(&items).join("\n"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packaged_and_cli_executable_names_dispatch_workers_without_accepting_a_test_harness() {
        for name in ["readmd", "ReadMD"] {
            assert!(is_worker_executable(std::path::Path::new(name)));
        }
        assert!(!is_worker_executable(std::path::Path::new(
            "readmd_kernel-1234"
        )));
    }
    #[test]
    fn actual_vision_recognizes_owned_probe_and_rejects_corruption() {
        let text = recognize(
            include_bytes!("../../../assets/engines/ocr-probe.png"),
            Some("en"),
        )
        .unwrap();
        assert!(
            text.contains("READMD") && text.contains("12345"),
            "{text:?}"
        );
        assert_eq!(
            recognize(b"not an image", None).unwrap_err(),
            "ocr_invalid_image"
        );
    }
    #[test]
    fn cancellation_precedes_worker_or_scratch_creation() {
        crate::speech::with_cancel(std::sync::Arc::new(|| true), || {
            assert_eq!(image(b"not an image", None).unwrap_err(), "cancelled");
        });
        assert_eq!(check(Instant::now()).unwrap_err(), "ocr_timeout");
    }
}
