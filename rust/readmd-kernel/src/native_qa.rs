//! Explicit debug-only WKWebView integration driver. No public route, relaxed
//! CSP, chooser substitute or token substitute. Never compiled into a release.
use serde_json::Value;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::OnceLock,
};

static OUTPUT: OnceLock<PathBuf> = OnceLock::new();
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
        let _ = OUTPUT.set(profile.join("native-qa-result.json"));
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
pub fn record(body: &str) -> bool {
    let Some(output) = OUTPUT.get() else {
        return false;
    };
    if body.len() > 16 * 1024 {
        return false;
    }
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return false;
    };
    let progress = value.get("readmd_native_qa_progress");
    let Some(result) = progress.or_else(|| value.get("readmd_native_qa_result")) else {
        return false;
    };
    let label = |value: &Value| {
        value.as_str().is_some_and(|s| {
            !s.is_empty()
                && s.len() < 128
                && s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        })
    };
    if !result["ok"].is_boolean()
        || !label(&result["phase"])
        || !result["checks"]
            .as_array()
            .is_some_and(|items| items.len() <= 64 && items.iter().all(label))
    {
        return true;
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
        let path = output.with_file_name("native-qa-progress.json");
        let temp = output.with_file_name("native-qa-progress.tmp");
        if let Ok(bytes) = serde_json::to_vec_pretty(&safe) {
            if fs::write(&temp, bytes).is_ok() {
                let _ = fs::rename(temp, path);
            }
        }
        return true;
    }
    if let Ok(mut file) = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
    {
        if let Ok(bytes) = serde_json::to_vec_pretty(&safe) {
            let _ = file.write_all(&bytes).and_then(|_| file.sync_all());
        }
    }
    true
}
