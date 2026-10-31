//! Debug-only, marker-gated QA of this process's own Cocoa window. Commands
//! cannot name another application, script, file, URL or native window.
use super::{
    sync_input_watcher, HostConfig, HostState, InputWatcher, PlatformBackend, WebViewHost,
};
use crate::platform::Window;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType, NSWindow};
use objc2_foundation::{NSPoint, NSProcessInfo};
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
#[path = "native_pet_capture_qa.rs"]
mod capture;

pub(super) struct Driver {
    directory: PathBuf,
    sequence: u64,
    gesture: Option<Gesture>,
}
struct Gesture {
    sequence: u64,
    click: bool,
    context_menu: bool,
    start: NSPoint,
    stage: u8,
    next: Instant,
}
impl Driver {
    pub(super) fn from_config(config: &HostConfig) -> Option<Self> {
        if std::env::var("READMD_PET_NATIVE_QA").as_deref() != Ok("1") {
            return None;
        }
        if fs::read_to_string(config.data_dir.join(".readmd-pet-native-qa-owned"))
            .ok()?
            .trim()
            != "readmd-synthetic-pet-native-qa-v1"
        {
            return None;
        }
        Some(Self {
            directory: config.data_dir.clone(),
            sequence: 0,
            gesture: None,
        })
    }
    fn write(&self, value: Value) {
        if let Ok(bytes) = serde_json::to_vec_pretty(&value) {
            let temp = self.directory.join("native-pet-qa-result.tmp");
            if fs::write(&temp, bytes).is_ok() {
                let _ = fs::rename(temp, self.directory.join("native-pet-qa-result.json"));
            }
        }
    }
    fn status(sequence: u64, native: &NSWindow, state: &HostState) -> Value {
        let frame = native.frame();
        let hit = state
            .pet_rects
            .iter()
            .max_by(|a, b| (a.width * a.height).total_cmp(&(b.width * b.height)))
            .map(|r| json!({"x":r.x+r.width/2.0,"y":r.y+r.height/2.0}));
        json!({"sequence":sequence,"ok":true,"window_number":native.windowNumber(),
            "visible":native.isVisible(),"opaque":native.isOpaque(),"shadow":native.hasShadow(),
            "ignores_mouse":native.ignoresMouseEvents(),"level":native.level(),"alpha":native.alphaValue(),
            "nonactivating":native.styleMask().contains(objc2_app_kit::NSWindowStyleMask::NonactivatingPanel),
            "frame":{"x":frame.origin.x,"y":frame.origin.y,"width":frame.size.width,"height":frame.size.height},
            "bounds":{"x":state.bounds.x,"y":state.bounds.y,"width":state.bounds.width,"height":state.bounds.height},
            "renderer_ready":state.renderer_ready,"interaction_generation":state.interaction_generation,
            "regions":state.interaction_rects.len(),"pet_regions":state.pet_rects.len(),"locked":state.lock_position,
            "hit_point":hit,"pointer":pointer(),
            "input_permissions":crate::input::native_permissions()})
    }
    pub(super) fn tick(
        &mut self,
        window: &Window,
        backend: &mut dyn PlatformBackend,
        state: &mut HostState,
        watcher: &InputWatcher,
        _webview: &WebViewHost,
    ) {
        let raw = window.ns_window();
        if raw.is_null() {
            return;
        }
        let native = unsafe { &*(raw as *const NSWindow) };
        if let Some(gesture) = self.gesture.as_mut() {
            if Instant::now() < gesture.next {
                return;
            }
            if gesture.stage >= 4 {
                let sequence = gesture.sequence;
                self.gesture = None;
                self.write(Self::status(sequence, native, state));
                return;
            }
            let (kind, dx, dy) = match gesture.stage {
                0 if gesture.context_menu => (NSEventType::RightMouseDown, 0.0, 0.0),
                _ if gesture.context_menu => (NSEventType::RightMouseUp, 0.0, 0.0),
                0 => (NSEventType::LeftMouseDown, 0.0, 0.0),
                1 if !gesture.click => (NSEventType::LeftMouseDragged, 24.0, -18.0),
                2 if !gesture.click => (NSEventType::LeftMouseDragged, 48.0, -36.0),
                _ => (
                    NSEventType::LeftMouseUp,
                    if gesture.click { 0.0 } else { 48.0 },
                    if gesture.click { 0.0 } else { -36.0 },
                ),
            };
            let local = native
                .convertPointFromScreen(NSPoint::new(gesture.start.x + dx, gesture.start.y + dy));
            let event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
                kind, local, NSEventModifierFlags::empty(), NSProcessInfo::processInfo().systemUptime(), native.windowNumber(), None,
                i64::from(gesture.stage) as isize, 1, if kind == NSEventType::LeftMouseUp {0.0} else {1.0});
            if let (Some(main), Some(event)) = (MainThreadMarker::new(), event) {
                NSApplication::sharedApplication(main).postEvent_atStart(&event, false);
            }
            gesture.stage = if gesture.click && gesture.stage == 0 {
                3
            } else {
                gesture.stage + 1
            };
            gesture.next =
                Instant::now() + Duration::from_millis(if gesture.stage == 4 { 180 } else { 60 });
            return;
        }
        let command = self.directory.join("native-pet-qa-command.json");
        if fs::metadata(&command).map_or(true, |m| m.len() > 4096) {
            return;
        }
        let Ok(bytes) = fs::read(command) else {
            return;
        };
        let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
            return;
        };
        if value["pid"].as_u64() != Some(u64::from(std::process::id())) {
            return;
        }
        let Some(sequence) = value["sequence"].as_u64().filter(|s| *s > self.sequence) else {
            return;
        };
        if !state.renderer_ready || state.interaction_generation == 0 {
            return;
        }
        self.sequence = sequence;
        match value["action"].as_str().unwrap_or("") {
            "status" => self.write(Self::status(sequence, native, state)),
            "capture" => {
                capture::frame(&_webview.view, &self.directory, sequence);
                self.write(Self::status(sequence, native, state));
            }
            "drop-owned-file" | "hover-owned-file" => {
                let accepted = capture::drop_owned_file(
                    &_webview.view,
                    &self.directory,
                    value["action"] == "drop-owned-file",
                );
                let mut result = Self::status(sequence, native, state);
                result["owned_drop_accepted"] = json!(accepted);
                self.write(result);
            }
            "click" | "drag" | "context-menu" => {
                let rect = state
                    .pet_rects
                    .iter()
                    .max_by(|a, b| (a.width * a.height).total_cmp(&(b.width * b.height)));
                if let Some(rect) = rect {
                    let point = NSPoint::new(
                        rect.x + rect.width / 2.0,
                        native.frame().size.height - rect.y - rect.height / 2.0,
                    );
                    self.gesture = Some(Gesture {
                        sequence,
                        click: value["action"] != "drag",
                        context_menu: value["action"] == "context-menu",
                        start: native.convertPointToScreen(point),
                        stage: 0,
                        next: Instant::now(),
                    });
                } else {
                    self.write(
                        json!({"sequence":sequence,"ok":false,"code":"native_pet_region_missing"}),
                    );
                }
            }
            "refresh_geometry" => {
                sync_input_watcher(watcher, backend, window, state);
                self.write(Self::status(sequence, native, state));
            }
            "diagnose_native_style" => {
                let _ = backend.init(window);
                self.write(Self::status(sequence, native, state));
            }
            _ => self.write(
                json!({"sequence":sequence,"ok":false,"code":"native_pet_qa_action_invalid"}),
            ),
        }
    }
}

#[repr(C)]
struct Point {
    x: f64,
    y: f64,
}
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventCreate(source: *const std::ffi::c_void) -> *const std::ffi::c_void;
    fn CGEventGetLocation(event: *const std::ffi::c_void) -> Point;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: *const std::ffi::c_void);
}
fn pointer() -> Option<Value> {
    unsafe {
        let event = CGEventCreate(std::ptr::null());
        if event.is_null() {
            return None;
        }
        let point = CGEventGetLocation(event);
        CFRelease(event);
        Some(json!({"x":point.x,"y":point.y}))
    }
}
