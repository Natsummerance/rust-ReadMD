//! Explicit debug-only WKWebView integration driver. No public route, relaxed
//! CSP, chooser substitute or token substitute. Never compiled into a release.
use serde_json::Value;
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

static OUTPUT: OnceLock<PathBuf> = OnceLock::new();
static GUARDED_QUIT: OnceLock<Mutex<QuitBarrier>> = OnceLock::new();
const QUIT_SCHEMA: &str = "readmd-native-qa-guarded-quit-v1";

#[derive(Debug, PartialEq, Eq)]
pub enum RecordOutcome { NotQa, Recorded, ReleaseHeldQuit }

struct QuitBarrier { phase: String, held: u8, finished: bool, blocked: bool }
impl QuitBarrier {
    fn new(phase: String) -> Self { Self { phase, held: 0, finished: false, blocked: false } }
    fn hold(&mut self) {
        self.held = self.held.saturating_add(1);
        if self.finished || self.held != 1 { self.blocked = true; }
    }
    fn terminal(&mut self, value: &Value, persisted: bool) -> bool {
        let release = !self.blocked && !self.finished && self.held == 1 && persisted
            && value["ok"].as_bool() == Some(true) && value["phase"].as_str() == Some(self.phase.as_str())
            && value["checks"].as_array().is_some_and(|checks| checks.iter().any(|v| v.as_str() == Some("guarded_quit_accepted")));
        self.finished = true;
        if !release { self.blocked = true; }
        release
    }
}
fn label(value: &Value) -> bool {
    value.as_str().is_some_and(|s| !s.is_empty() && s.len() < 128
        && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'))
}
fn load_quit_barrier(profile: &Path) -> io::Result<Option<QuitBarrier>> {
    let file = profile.join("native-qa-guarded-quit.json");
    let meta = match fs::symlink_metadata(&file) {
        Ok(meta) => meta,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "invalid owned QA guarded-quit opt-in");
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 1024 { return Err(invalid()); }
    let value: Value = serde_json::from_slice(&fs::read(file)?).map_err(|_| invalid())?;
    if value.as_object().is_none_or(|map| map.len() != 2) || value["schema"].as_str() != Some(QUIT_SCHEMA) || !label(&value["phase"]) { return Err(invalid()); }
    Ok(Some(QuitBarrier::new(value["phase"].as_str().unwrap().to_owned())))
}
// This module is excluded unless native-qa AND debug_assertions are enabled.
// A normal profile, including every old QA harness without explicit opt-in,
// still follows the original native quit path.
pub fn hold_quit() -> bool {
    if OUTPUT.get().is_none() { return false; }
    let Some(barrier) = GUARDED_QUIT.get() else { return false; };
    if let Ok(mut state) = barrier.lock() { state.hold(); }
    true // A poisoned or rejected barrier must never fall through to quit.
}
fn block(barrier: Option<&Mutex<QuitBarrier>>) {
    if let Some(barrier) = barrier { if let Ok(mut state) = barrier.lock() { state.blocked = true; } }
}
// Recording uses WebKit's actual high-resolution renderer and AppKit's own
// button drawing, independent of the user's current monitor resolution.
// Only an explicitly owned debug profile can request a frame. No output path
// comes from IPC, and release builds never contain this adapter.
#[cfg(target_os = "macos")]
pub fn capture_frame(reader: &wry::WebView, id: u64) {
    use objc2::{msg_send, rc::{Allocated, Retained}, runtime::{AnyClass, AnyObject}};
    use objc2_app_kit::NSWindowButton;
    use objc2_foundation::{NSData, NSPoint, NSRect, NSSize, NSString};
    use std::sync::atomic::{AtomicBool, Ordering};
    use wry::WebViewExtMacOS;
    static BUSY: AtomicBool = AtomicBool::new(false);
    let Some(profile) = OUTPUT.get().and_then(|p| p.parent()) else { return; };
    if BUSY.swap(true, Ordering::Acquire) { return; }
    let folder=profile.join("native-frames");
    let owned_folder=match fs::symlink_metadata(&folder) {
        Ok(meta)=>meta.is_dir()&&!meta.file_type().is_symlink(),
        Err(error) if error.kind()==std::io::ErrorKind::NotFound=>fs::create_dir(&folder).is_ok(),
        _=>false,
    };
    if !owned_folder { BUSY.store(false,Ordering::Release); return; }
    let destination=folder.join(format!("{id:05}.png"));
    if destination.exists() { BUSY.store(false,Ordering::Release); return; }
    unsafe {
        let config: Retained<AnyObject>=msg_send![AnyClass::get(c"WKSnapshotConfiguration").unwrap(), new];
        let width: Retained<AnyObject>=msg_send![AnyClass::get(c"NSNumber").unwrap(), numberWithDouble:3840.0_f64];
        let _:()=msg_send![&config,setSnapshotWidth:&*width];
        // Capture the current committed pixels. Waiting for a future screen
        // update can suspend a capture when another app occludes this window;
        // the driver separately verifies the actual content readiness.
        let _:()=msg_send![&config,setAfterScreenUpdates:false];
        let view=reader.webview();
        let bounds=view.bounds();
        let window=reader.ns_window();
        let controls=[NSWindowButton::CloseButton,NSWindowButton::MiniaturizeButton,NSWindowButton::ZoomButton].into_iter()
            .filter_map(|kind|window.standardWindowButton(kind))
            .filter(|b|!b.isHidden())
            .map(|button|{let rect=button.convertRect_toView(button.bounds(),Some(&view));(button,rect)}).collect::<Vec<_>>();
        let completion=block2::RcBlock::new(move |snapshot:*mut AnyObject,error:*mut AnyObject| {
            let write=||->Option<()> {
                if snapshot.is_null()||!error.is_null()||bounds.size.width<=0. {return None;}
                let scale=3840./bounds.size.width;
                let size=NSSize::new(3840.,(bounds.size.height*scale).round());
                // An explicit pixel bitmap avoids lockFocus's implicit 1x/2x
                // screen scale; the output stays 3840px on either monitor.
                let rep:Allocated<AnyObject>=msg_send![AnyClass::get(c"NSBitmapImageRep")?,alloc];
                let space=NSString::from_str("NSDeviceRGBColorSpace");
                let rep:Retained<AnyObject>=msg_send![rep,initWithBitmapDataPlanes:std::ptr::null_mut::<*mut u8>(),pixelsWide:3840_isize,pixelsHigh:size.height as isize,bitsPerSample:8_isize,samplesPerPixel:4_isize,hasAlpha:true,isPlanar:false,colorSpaceName:&*space,bytesPerRow:0_isize,bitsPerPixel:0_isize];
                let graphics=AnyClass::get(c"NSGraphicsContext")?;
                let context:Option<Retained<AnyObject>>=msg_send![graphics,graphicsContextWithBitmapImageRep:&*rep];
                let context=context?;
                let _:()=msg_send![graphics,saveGraphicsState];
                let _:()=msg_send![graphics,setCurrentContext:&*context];
                let rect=NSRect::new(NSPoint::new(0.,0.),size);
                let zero=NSRect::new(NSPoint::new(0.,0.),NSSize::new(0.,0.));
                let _:()=msg_send![snapshot,drawInRect:rect,fromRect:zero,operation:2_usize,fraction:1.0_f64];
                for (button,frame) in &controls {
                    let _:()=msg_send![graphics,saveGraphicsState];
                    let transform:Retained<AnyObject>=msg_send![AnyClass::get(c"NSAffineTransform")?,transform];
                    let _:()=msg_send![&transform,translateXBy:frame.origin.x*scale,yBy:size.height-(frame.origin.y+frame.size.height)*scale];
                    let _:()=msg_send![&transform,scaleBy:scale];
                    let _:()=msg_send![&transform,concat];
                    button.drawRect(button.bounds());
                    let _:()=msg_send![graphics,restoreGraphicsState];
                }
                let _:()=msg_send![graphics,restoreGraphicsState];
                let properties:Retained<AnyObject>=msg_send![AnyClass::get(c"NSDictionary")?,dictionary];
                let data:Option<Retained<NSData>>=msg_send![&*rep,representationUsingType:4_usize,properties:&*properties];
                let mut file=fs::OpenOptions::new().write(true).create_new(true).open(&destination).ok()?;
                file.write_all(&data?.to_vec()).ok()?;
                Some(())
            };
            let _=write();BUSY.store(false,Ordering::Release);
        });
        let _:()=msg_send![&view,takeSnapshotWithConfiguration:&*config,completionHandler:&*completion];
    }
}
pub struct Driver {
    script: Option<String>,
}
impl Driver {
    pub fn from_profile(profile: &Path) -> Option<Self> {
        if std::env::var("READMD_NATIVE_QA").as_deref() != Ok("1") {
            return None;
        }
        let load = || -> Option<String> {
            if fs::read_to_string(profile.join(".readmd-native-qa-owned"))
                .ok()?
                .trim()
                != "readmd-synthetic-native-qa-v1"
            {
                return None;
            }
            let script = fs::read_to_string(profile.join("native-qa.js")).ok()?;
            if script.len() > 64 * 1024 {
                return None;
            }
            Some(script)
        };
        let script = load().unwrap_or_else(|| {
            eprintln!("readmd: native QA requires an explicitly owned synthetic profile");
            std::process::exit(2)
        });
        let output = profile.join("native-qa-result.json");
        let _ = OUTPUT.set(output.clone());
        match load_quit_barrier(profile) {
            Ok(Some(barrier)) => {
                if OUTPUT.get() != Some(&output) || GUARDED_QUIT.set(Mutex::new(barrier)).is_err() {
                    eprintln!("readmd: invalid owned QA guarded-quit opt-in"); std::process::exit(2);
                }
            }
            Ok(None) => {}
            _ => { eprintln!("readmd: invalid owned QA guarded-quit opt-in"); std::process::exit(2); }
        }
        Some(Self {
            script: Some(script),
        })
    }
    pub fn tick(&mut self, reader: &wry::WebView, ready: bool) {
        if ready {
            if let Some(script) = self.script.take() {
                let _ = reader.evaluate_script(&script);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn payload(ok: Value, phase: &str) -> String {
        serde_json::json!({"readmd_native_qa_result":{"ok":ok,"phase":phase,"checks":["guarded_quit_accepted"]}}).to_string()
    }
    fn barrier() -> Mutex<QuitBarrier> { Mutex::new(QuitBarrier::new("owned_phase".into())) }
    fn send(output: &Path, barrier: Option<&Mutex<QuitBarrier>>, body: &str) -> RecordOutcome {
        record_to(output, barrier, body, |file| file.sync_all())
    }
    #[test]
    fn guarded_quit_requires_explicit_valid_profile_opt_in() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_quit_barrier(dir.path()).unwrap().is_none());
        let file = dir.path().join("native-qa-guarded-quit.json");
        for value in [serde_json::json!({"schema":"wrong","phase":"owned_phase"}),
            serde_json::json!({"schema":QUIT_SCHEMA,"phase":"UPPER"}),
            serde_json::json!({"schema":QUIT_SCHEMA,"phase":"owned_phase","extra":true})] {
            fs::write(&file, value.to_string()).unwrap(); assert!(load_quit_barrier(dir.path()).is_err());
        }
        fs::write(&file, serde_json::json!({"schema":QUIT_SCHEMA,"phase":"owned_phase"}).to_string()).unwrap();
        assert_eq!(load_quit_barrier(dir.path()).unwrap().unwrap().phase, "owned_phase");
    }
    #[cfg(unix)]
    #[test]
    fn guarded_quit_opt_in_rejects_symlinks() {
        let dir = tempfile::tempdir().unwrap(); let actual = dir.path().join("actual.json");
        fs::write(&actual, serde_json::json!({"schema":QUIT_SCHEMA,"phase":"owned_phase"}).to_string()).unwrap();
        std::os::unix::fs::symlink(actual, dir.path().join("native-qa-guarded-quit.json")).unwrap();
        assert!(load_quit_barrier(dir.path()).is_err());
    }
    #[test]
    fn one_held_quit_releases_once_only_after_fresh_write_and_real_sync() {
        let dir = tempfile::tempdir().unwrap(); let output = dir.path().join("result.json"); let barrier = barrier();
        barrier.lock().unwrap().hold(); let mut sync_calls = 0;
        let body = payload(Value::Bool(true), "owned_phase");
        let outcome = record_to(&output, Some(&barrier), &body, |file| { sync_calls += 1; file.sync_all() });
        assert_eq!(outcome, RecordOutcome::ReleaseHeldQuit); assert_eq!(sync_calls, 1);
        let saved = fs::read(&output).unwrap(); let value: Value = serde_json::from_slice(&saved).unwrap();
        assert_eq!(value["ok"], true); assert_eq!(value["phase"], "owned_phase");
        assert_eq!(send(&output, Some(&barrier), &body), RecordOutcome::Recorded);
        assert_eq!(fs::read(output).unwrap(), saved);
    }
    #[test]
    fn zero_or_duplicate_held_quit_never_releases() {
        for count in [0, 2, 3] {
            let dir = tempfile::tempdir().unwrap(); let barrier = barrier();
            for _ in 0..count { barrier.lock().unwrap().hold(); }
            assert_eq!(send(&dir.path().join("result.json"), Some(&barrier), &payload(Value::Bool(true), "owned_phase")), RecordOutcome::Recorded);
        }
    }
    #[test]
    fn rejected_non_boolean_or_wrong_phase_terminal_never_releases() {
        for (ok, phase) in [(Value::Bool(false), "owned_phase"), (Value::String("true".into()), "owned_phase"),
            (serde_json::json!(1), "owned_phase"), (Value::Null, "owned_phase"), (Value::Bool(true), "other_phase")] {
            let dir = tempfile::tempdir().unwrap(); let barrier = barrier(); barrier.lock().unwrap().hold();
            assert_eq!(send(&dir.path().join("result.json"), Some(&barrier), &payload(ok, phase)), RecordOutcome::Recorded);
            assert!(barrier.lock().unwrap().blocked);
        }
    }
    #[test]
    fn terminal_before_quit_cannot_arm_a_later_release() {
        let dir = tempfile::tempdir().unwrap(); let barrier = barrier(); let body = payload(Value::Bool(true), "owned_phase");
        assert_eq!(send(&dir.path().join("early.json"), Some(&barrier), &body), RecordOutcome::Recorded);
        barrier.lock().unwrap().hold();
        assert_eq!(send(&dir.path().join("late.json"), Some(&barrier), &body), RecordOutcome::Recorded);
    }
    #[test]
    fn progress_and_other_ipc_cannot_release_a_held_quit() {
        let dir = tempfile::tempdir().unwrap(); let output = dir.path().join("result.json"); let barrier = barrier(); barrier.lock().unwrap().hold();
        assert_eq!(send(&output, Some(&barrier), r#"{"readmd_window":"status"}"#), RecordOutcome::NotQa);
        assert_eq!(send(&output, Some(&barrier), r#"{"readmd_native_qa_progress":{"ok":true,"phase":"owned_phase","checks":[]}}"#), RecordOutcome::Recorded);
        assert!(!output.exists()); assert_eq!(barrier.lock().unwrap().held, 1);
        assert_eq!(send(&output, Some(&barrier), &payload(Value::Bool(true), "owned_phase")), RecordOutcome::ReleaseHeldQuit);
    }
    #[test]
    fn malformed_invalid_or_mixed_receipt_blocks_subsequent_success() {
        for body in ["{", r#"{"readmd_native_qa_result":{"ok":true,"phase":"owned_phase","checks":["UPPER"]}}"#,
            r#"{"readmd_native_qa_result":{"ok":true,"phase":"owned_phase","checks":[]}}"#,
            r#"{"readmd_native_qa_progress":{"ok":true,"phase":"owned_phase","checks":[]},"readmd_native_qa_result":{"ok":true,"phase":"owned_phase","checks":[]}}"#] {
            let dir = tempfile::tempdir().unwrap(); let barrier = barrier(); barrier.lock().unwrap().hold();
            assert_ne!(send(&dir.path().join("first.json"), Some(&barrier), body), RecordOutcome::ReleaseHeldQuit);
            assert_ne!(send(&dir.path().join("next.json"), Some(&barrier), &payload(Value::Bool(true), "owned_phase")), RecordOutcome::ReleaseHeldQuit);
        }
    }
    #[test]
    fn existing_terminal_or_open_failure_never_releases_or_overwrites() {
        let dir = tempfile::tempdir().unwrap(); let output = dir.path().join("result.json"); fs::write(&output, b"first failure").unwrap();
        let barrier = barrier(); barrier.lock().unwrap().hold();
        assert_eq!(send(&output, Some(&barrier), &payload(Value::Bool(true), "owned_phase")), RecordOutcome::Recorded);
        assert_eq!(fs::read(&output).unwrap(), b"first failure");
        let blocked = Mutex::new(QuitBarrier::new("owned_phase".into())); blocked.lock().unwrap().hold();
        assert_eq!(send(dir.path(), Some(&blocked), &payload(Value::Bool(true), "owned_phase")), RecordOutcome::Recorded);
    }
    #[test]
    fn sync_failure_cannot_turn_written_success_bytes_into_release() {
        let dir = tempfile::tempdir().unwrap(); let output = dir.path().join("result.json"); let barrier = barrier(); barrier.lock().unwrap().hold();
        let outcome = record_to(&output, Some(&barrier), &payload(Value::Bool(true), "owned_phase"), |_| Err(io::Error::other("controlled sync failure")));
        assert_eq!(outcome, RecordOutcome::Recorded); assert!(output.is_file()); assert!(barrier.lock().unwrap().blocked);
        assert_eq!(send(&dir.path().join("retry.json"), Some(&barrier), &payload(Value::Bool(true), "owned_phase")), RecordOutcome::Recorded);
    }
    #[test]
    fn old_qa_without_opt_in_records_without_authorizing_a_new_quit() {
        let dir = tempfile::tempdir().unwrap(); let output = dir.path().join("result.json");
        assert_eq!(send(&output, None, &payload(Value::Bool(true), "owned_phase")), RecordOutcome::Recorded);
        assert!(output.is_file());
    }
}
pub fn record(body: &str) -> RecordOutcome {
    let Some(output) = OUTPUT.get() else {
        return RecordOutcome::NotQa;
    };
    record_to(output, GUARDED_QUIT.get(), body, |file| file.sync_all())
}
fn record_to(output: &Path, barrier: Option<&Mutex<QuitBarrier>>, body: &str,
    sync: impl FnOnce(&fs::File) -> io::Result<()>) -> RecordOutcome {
    if body.len() > 16 * 1024 {
        block(barrier); return RecordOutcome::NotQa;
    }
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        block(barrier); return RecordOutcome::NotQa;
    };
    let progress = value.get("readmd_native_qa_progress");
    let Some(result) = progress.or_else(|| value.get("readmd_native_qa_result")) else {
        return RecordOutcome::NotQa;
    };
    if !result["ok"].is_boolean()
        || !label(&result["phase"])
        || !result["checks"]
            .as_array()
            .is_some_and(|items| items.len() <= 64 && items.iter().all(label))
    {
        block(barrier); return RecordOutcome::Recorded;
    }
    // Do not accept document text, URLs, paths, tokens or arbitrary diagnostics.
    let mut safe =
        serde_json::json!({"ok":result["ok"],"phase":result["phase"],"checks":result["checks"]});
    // Optional numeric own-window handle for isolated screenshot verification;
    // no document text, arbitrary diagnostics, endpoints or paths are accepted.
    if let Some(number) = result["window_number"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= u32::MAX as u64)
    {
        safe["window_number"] = serde_json::json!(number);
    }
    if progress.is_some() {
        if value.get("readmd_native_qa_result").is_some() { block(barrier); }
        let path = output.with_file_name("native-qa-progress.json");
        let temp = output.with_file_name("native-qa-progress.tmp");
        if let Ok(bytes) = serde_json::to_vec_pretty(&safe) {
            if fs::write(&temp, bytes).is_ok() {
                let _ = fs::rename(temp, path);
            }
        }
        return RecordOutcome::Recorded;
    }
    let persisted = (|| -> io::Result<()> {
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(output)?;
        let bytes = serde_json::to_vec_pretty(&safe).map_err(io::Error::other)?;
        file.write_all(&bytes)?; sync(&file)
    })().is_ok();
    if barrier.is_some_and(|barrier| barrier.lock().is_ok_and(|mut state| state.terminal(&safe, persisted))) {
        RecordOutcome::ReleaseHeldQuit
    } else { RecordOutcome::Recorded }
}
