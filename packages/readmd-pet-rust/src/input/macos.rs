//! Quartz pointer/button state, without an event tap or a permission prompt.
//! Keyboard state is queried only when macOS has already granted listen access;
//! no text, characters, focused window title or clipboard is read here.
use super::{
    pressed::{smoothing_alpha, PressedInput},
    probe_cursor, HitTestTarget, InputEvent,
};
use std::{
    ffi::c_void,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

#[repr(C)]
#[derive(Clone, Copy)]
struct Point {
    x: f64,
    y: f64,
}
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventCreate(source: *const c_void) -> *const c_void;
    fn CGEventGetLocation(event: *const c_void) -> Point;
    fn CGEventSourceButtonState(state: i32, button: u32) -> bool;
    fn CGEventSourceKeyState(state: i32, key: u16) -> bool;
    fn CGPreflightListenEventAccess() -> bool;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: *const c_void);
}

pub(super) fn permissions() -> serde_json::Value {
    serde_json::json!({"keyboard_listen_authorized": unsafe { CGPreflightListenEventAccess() },
        "pointer_permission_required":false,"clipboard_read":"on_user_command"})
}

// USB usage identity, matching the renderer and Windows adapter. macOS virtual
// key codes are physical positions, independent of input method and text.
const KEY_USAGES: &[(u16, u16)] = &[
    (0, 4),
    (1, 22),
    (2, 7),
    (3, 9),
    (4, 11),
    (5, 10),
    (6, 29),
    (7, 27),
    (8, 6),
    (9, 25),
    (11, 5),
    (12, 20),
    (13, 26),
    (14, 8),
    (15, 21),
    (16, 28),
    (17, 23),
    (18, 30),
    (19, 31),
    (20, 32),
    (21, 33),
    (22, 35),
    (23, 34),
    (24, 46),
    (25, 38),
    (26, 36),
    (27, 45),
    (28, 37),
    (29, 39),
    (30, 48),
    (31, 18),
    (32, 24),
    (33, 47),
    (34, 12),
    (35, 19),
    (36, 40),
    (37, 15),
    (38, 13),
    (39, 52),
    (40, 14),
    (41, 51),
    (42, 49),
    (43, 54),
    (44, 56),
    (45, 17),
    (46, 16),
    (47, 55),
    (48, 43),
    (49, 44),
    (50, 53),
    (51, 42),
    (53, 41),
    (54, 231),
    (55, 227),
    (56, 225),
    (57, 57),
    (58, 226),
    (59, 224),
    (60, 229),
    (61, 230),
    (62, 228),
    (65, 99),
    (67, 85),
    (69, 87),
    (71, 83),
    (75, 84),
    (76, 88),
    (78, 86),
    (81, 103),
    (82, 98),
    (83, 89),
    (84, 90),
    (85, 91),
    (86, 92),
    (87, 93),
    (88, 94),
    (89, 95),
    (91, 96),
    (92, 97),
    (96, 62),
    (97, 63),
    (98, 64),
    (99, 60),
    (100, 65),
    (101, 66),
    (103, 68),
    (109, 67),
    (111, 69),
    (115, 74),
    (116, 75),
    (117, 76),
    (118, 61),
    (119, 77),
    (120, 59),
    (121, 78),
    (122, 58),
    (123, 80),
    (124, 79),
    (125, 81),
    (126, 82),
];

pub(super) fn run<F>(target: Arc<Mutex<HitTestTarget>>, running: Arc<AtomicBool>, emit: F)
where
    F: Fn(InputEvent) + Send + Sync + 'static,
{
    let mut pressed = PressedInput::new();
    let mut hovered = false;
    let mut active = false;
    let mut previous_left = false;
    let mut previous_point: Option<Point> = None;
    let mut gaze: Option<(f64, f64)> = None;
    let mut keyboard_allowed = unsafe { CGPreflightListenEventAccess() };
    let mut permission_at = Instant::now();
    let mut last = Instant::now();
    let mut hover_at = Instant::now() - Duration::from_secs(1);
    let mut faulted = false;
    while running.load(Ordering::Relaxed) {
        let at = Instant::now();
        let event = unsafe { CGEventCreate(std::ptr::null()) };
        if event.is_null() {
            if !faulted {
                emit(InputEvent::Fault("macos_pointer_unavailable"));
                faulted = true;
            }
            std::thread::sleep(Duration::from_millis(16));
            continue;
        }
        let point = unsafe {
            let p = CGEventGetLocation(event);
            CFRelease(event);
            p
        };
        if permission_at.elapsed() >= Duration::from_secs(2) {
            keyboard_allowed = unsafe { CGPreflightListenEventAccess() };
            permission_at = at;
        }
        let mut left = false;
        for button in 0..5 {
            let down = unsafe { CGEventSourceButtonState(0, button) };
            pressed.mouse(button as usize, down);
            if button == 0 {
                left = down;
            }
        }
        for &(key, usage) in KEY_USAGES {
            let down = keyboard_allowed && unsafe { CGEventSourceKeyState(0, key) };
            pressed.key(usage, i32::from(key), down);
        }
        let target = target.lock().map(|t| t.clone()).unwrap_or_default();
        // Runtime supplies the NSWindow frame in Quartz points on Mac; unlike
        // Windows, Quartz pointer coordinates are not backing-store pixels.
        let probe = probe_cursor(
            &target,
            (target.window_x, target.window_y),
            (point.x, point.y),
            left && !previous_left,
        );
        let dragging = crate::platform::native_drag_active();
        if !dragging
            && (probe.hovering != hovered || hover_at.elapsed() >= Duration::from_millis(100))
        {
            hovered = probe.hovering;
            hover_at = at;
            emit(InputEvent::Hover(hovered));
        }
        // Clicks and drag gestures use AppKit's own-window local event stream,
        // rather than inferring a gesture over a potentially occluded window.
        let mut snapshot = pressed.snapshot();
        let moved =
            previous_point.is_some_and(|p| (p.x - point.x).abs() + (p.y - point.y).abs() > 0.5);
        snapshot.active |= moved;
        snapshot.pointer_x = point.x;
        snapshot.pointer_y = point.y;
        let (gx, gy) = gaze.unwrap_or((probe.rel_x, probe.rel_y));
        let alpha = smoothing_alpha(at.duration_since(last));
        let smooth = (
            gx + (probe.rel_x - gx) * alpha,
            gy + (probe.rel_y - gy) * alpha,
        );
        gaze = Some(smooth);
        snapshot.mouse_x = smooth.0;
        snapshot.mouse_y = smooth.1;
        snapshot.pet_clicked = probe.head_clicked;
        if snapshot.active != active {
            active = snapshot.active;
            emit(InputEvent::Activity(active));
        }
        emit(InputEvent::Bongo(snapshot));
        previous_left = left;
        previous_point = Some(point);
        last = at;
        std::thread::sleep(Duration::from_millis(16).saturating_sub(at.elapsed()));
    }
    pressed.reset();
    emit(InputEvent::Hover(false));
    emit(InputEvent::Activity(false));
    emit(InputEvent::Bongo(pressed.snapshot()));
}

/// A local monitor observes only events already delivered to this pet window.
/// It preserves WebView delivery and never requests global event-tap access.
pub(crate) struct LocalMonitor(objc2::rc::Retained<objc2::runtime::AnyObject>);
impl Drop for LocalMonitor {
    fn drop(&mut self) {
        unsafe {
            objc2_app_kit::NSEvent::removeMonitor(&self.0);
        }
    }
}
impl LocalMonitor {
    pub(crate) fn install(
        window: &crate::platform::Window,
        target: Arc<Mutex<HitTestTarget>>,
        emit: impl Fn(InputEvent) + 'static,
    ) -> Option<Self> {
        use objc2::{rc::Retained, MainThreadMarker};
        use objc2_app_kit::{NSEvent, NSEventMask, NSEventType, NSWindow};
        use std::{cell::RefCell, ptr::NonNull};
        let _main = MainThreadMarker::new()?;
        let native = unsafe { Retained::retain(window.ns_window().cast::<NSWindow>()) }?;
        // Native frame uses AppKit points, bottom-left origin. Keeping the
        // initial frame and pointer avoids accumulating rounding during a drag.
        let drag: RefCell<Option<(objc2_foundation::NSPoint, objc2_foundation::NSPoint, bool)>> =
            RefCell::new(None);
        let handler = block2::RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            let event_ref = unsafe { event.as_ref() };
            if event_ref.windowNumber() != native.windowNumber() {
                return event.as_ptr();
            }
            let t = target.lock().map(|v| v.clone()).unwrap_or_default();
            let local = event_ref.locationInWindow();
            let point = native.convertPointToScreen(local);
            match event_ref.r#type() {
                NSEventType::LeftMouseDown => {
                    let mut logical = t.clone();
                    logical.scale_factor = 1.0;
                    let probe = probe_cursor(
                        &logical,
                        (0.0, 0.0),
                        (local.x, native.frame().size.height - local.y),
                        true,
                    );
                    *drag.borrow_mut() = probe
                        .head_clicked
                        .then(|| (point, native.frame().origin, false));
                }
                NSEventType::LeftMouseDragged => {
                    if let Some((start, origin, moved)) = drag.borrow_mut().as_mut() {
                        let (dx, dy) = (point.x - start.x, point.y - start.y);
                        if dx.abs() + dy.abs() >= 4.0 {
                            *moved = true;
                        }
                        if !t.lock_position && *moved {
                            crate::platform::set_native_drag_active(true);
                            native.setFrameOrigin(objc2_foundation::NSPoint::new(
                                origin.x + dx,
                                origin.y + dy,
                            ));
                        }
                    }
                }
                NSEventType::LeftMouseUp => {
                    if let Some((_, _, moved)) = drag.borrow_mut().take() {
                        if moved {
                            if crate::platform::native_drag_active() {
                                crate::platform::complete_native_drag();
                            }
                        } else if t.visible {
                            emit(InputEvent::PetClick);
                        }
                    }
                }
                _ => (),
            }
            event.as_ptr()
        });
        let mask =
            NSEventMask::LeftMouseDown | NSEventMask::LeftMouseDragged | NSEventMask::LeftMouseUp;
        unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &handler).map(Self) }
    }
}
