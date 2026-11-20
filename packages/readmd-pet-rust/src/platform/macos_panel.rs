//! A real NSPanel owns the companion surface. A Tao NSWindow does not acquire
//! NSPanel's non-activation behavior by changing its style mask after creation.
//! The existing Tao event loop, WRY renderer and shared host state are reused.
use super::macos_placement::{recover_offscreen, ScreenArea};
use crate::{
    error::{HostError, HostResult},
    protocol::SnapshotBounds,
};
use objc2::{define_class, msg_send, rc::Retained, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSFloatingWindowLevel, NSNormalWindowLevel, NSPanel, NSScreen,
    NSView, NSWindowStyleMask,
};
use objc2_foundation::{NSObjectProtocol, NSPoint, NSRect, NSSize};
use raw_window_handle::{
    AppKitDisplayHandle, AppKitWindowHandle, DisplayHandle, HandleError, HasDisplayHandle,
    HasWindowHandle, RawDisplayHandle, RawWindowHandle, WindowHandle,
};
use std::{cell::Cell, ffi::c_void, ptr::NonNull};
use tao::dpi::PhysicalPosition;

#[derive(Default)]
struct PanelIvars {
    focusable: Cell<bool>,
}
define_class!(
    #[unsafe(super = NSPanel)]
    #[thread_kind = MainThreadOnly]
    #[ivars = PanelIvars]
    struct ReadMDNativePetPanel;
    unsafe impl NSObjectProtocol for ReadMDNativePetPanel {}
    impl ReadMDNativePetPanel {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key_window(&self) -> bool { self.ivars().focusable.get() }
        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main_window(&self) -> bool { false }
    }
);

/// Owns both objects used by the raw AppKit window handle, on the GUI thread.
pub struct Window {
    panel: Retained<ReadMDNativePetPanel>,
    view: Retained<NSView>,
}

fn logical_bounds(frame: NSRect, top: f64) -> SnapshotBounds {
    SnapshotBounds {
        x: frame.origin.x,
        y: top - frame.origin.y - frame.size.height,
        width: frame.size.width,
        height: frame.size.height,
    }
}

fn native_frame(bounds: SnapshotBounds, top: f64) -> NSRect {
    NSRect::new(
        NSPoint::new(bounds.x, top - bounds.y - bounds.height),
        NSSize::new(bounds.width, bounds.height),
    )
}

impl Window {
    pub(crate) fn new() -> HostResult<Self> {
        let main = MainThreadMarker::new()
            .ok_or_else(|| HostError::Backend("macos_main_thread_required".into()))?;
        let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(320.0, 420.0));
        let allocated = ReadMDNativePetPanel::alloc(main).set_ivars(PanelIvars::default());
        // NSWindow auto-release-on-close must be disabled when ARC/Rust owns
        // the native object rather than an NSWindowController.
        let panel: Retained<ReadMDNativePetPanel> = unsafe {
            msg_send![super(allocated),
            initWithContentRect:frame, styleMask:NSWindowStyleMask::NonactivatingPanel,
            backing:NSBackingStoreType::Buffered, defer:false]
        };
        unsafe {
            panel.setReleasedWhenClosed(false);
        }
        panel.setTitle(&objc2_foundation::NSString::from_str("ReadMD Desktop Pet"));
        panel.setOpaque(false);
        panel.setHasShadow(false);
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        panel.setHidesOnDeactivate(false);
        panel.setFloatingPanel(true);
        panel.setBecomesKeyOnlyIfNeeded(true);
        panel.setWorksWhenModal(true);
        let view = NSView::initWithFrame(NSView::alloc(main), frame);
        panel.setContentView(Some(&view));
        Ok(Self { panel, view })
    }
    pub(crate) fn ns_window(&self) -> *mut c_void {
        Retained::as_ptr(&self.panel) as *mut c_void
    }
    pub(crate) fn set_bounds(&self, bounds: SnapshotBounds) -> SnapshotBounds {
        if super::native_drag_active() {
            return self.bounds();
        }
        let (top, screens) = self.screen_layout();
        let placed = recover_offscreen(bounds, &screens, false);
        self.apply_bounds(placed, top)
    }
    fn apply_bounds(&self, bounds: SnapshotBounds, top: f64) -> SnapshotBounds {
        self.panel.setFrame_display(native_frame(bounds, top), true);
        self.bounds_with_top(top)
    }
    fn screen_layout(&self) -> (f64, Vec<ScreenArea>) {
        let screens = NSScreen::screens(self.panel.mtm());
        let top = screens.firstObject().map_or(0.0, |screen| {
            let frame = screen.frame();
            frame.origin.y + frame.size.height
        });
        let areas = screens
            .iter()
            .map(|screen| ScreenArea {
                frame: logical_bounds(screen.frame(), top),
                visible: logical_bounds(screen.visibleFrame(), top),
            })
            .collect();
        (top, areas)
    }
    fn bounds_with_top(&self, top: f64) -> SnapshotBounds {
        logical_bounds(self.panel.frame(), top)
    }
    pub(crate) fn bounds(&self) -> SnapshotBounds {
        let top = NSScreen::screens(self.panel.mtm())
            .firstObject()
            .map_or(0.0, |screen| {
                let frame = screen.frame();
                frame.origin.y + frame.size.height
            });
        self.bounds_with_top(top)
    }
    pub(crate) fn recover_offscreen_bounds(&self) -> Option<SnapshotBounds> {
        if super::native_drag_active() {
            return None;
        }
        let (top, screens) = self.screen_layout();
        let current = self.bounds_with_top(top);
        let placed = recover_offscreen(current, &screens, false);
        (placed != current).then(|| self.apply_bounds(placed, top))
    }
    pub(crate) fn scale_factor(&self) -> f64 {
        self.panel.backingScaleFactor()
    }
    pub(crate) fn outer_position(&self) -> Result<PhysicalPosition<i32>, &'static str> {
        let bounds = self.bounds();
        let scale = self.scale_factor();
        Ok(PhysicalPosition::new(
            (bounds.x * scale).round() as i32,
            (bounds.y * scale).round() as i32,
        ))
    }
    pub(crate) fn set_visible(&self, visible: bool) {
        if visible {
            self.panel.orderFrontRegardless();
        } else {
            self.panel.orderOut(None);
        }
    }
    pub(crate) fn set_always_on_top(&self, enabled: bool) {
        self.panel.setLevel(if enabled {
            NSFloatingWindowLevel
        } else {
            NSNormalWindowLevel
        });
    }
    pub(crate) fn set_focusable(&self, focusable: bool) {
        self.panel.ivars().focusable.set(focusable);
        if !focusable {
            self.panel.resignKeyWindow();
        }
    }
}
impl HasWindowHandle for Window {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let view = NonNull::new(Retained::as_ptr(&self.view) as *mut c_void)
            .ok_or(HandleError::Unavailable)?;
        // The NSPanel and NSView are retained by this owner for the entire
        // borrowed handle. Only the event-loop thread can own this value.
        Ok(unsafe {
            WindowHandle::borrow_raw(RawWindowHandle::AppKit(AppKitWindowHandle::new(view)))
        })
    }
}
impl HasDisplayHandle for Window {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(unsafe {
            DisplayHandle::borrow_raw(RawDisplayHandle::AppKit(AppKitDisplayHandle::new()))
        })
    }
}
impl Drop for Window {
    fn drop(&mut self) {
        self.panel.orderOut(None);
        self.panel.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appkit_above_and_negative_monitors_use_the_shared_top_left_origin() {
        let above = NSRect::new(NSPoint::new(0.0, 1920.0), NSSize::new(1920.0, 1080.0));
        assert_eq!(
            logical_bounds(above, 1920.0),
            SnapshotBounds {
                x: 0.0,
                y: -1080.0,
                width: 1920.0,
                height: 1080.0
            }
        );
        let left = NSRect::new(NSPoint::new(-1600.0, 920.0), NSSize::new(1600.0, 900.0));
        assert_eq!(
            logical_bounds(left, 1920.0),
            SnapshotBounds {
                x: -1600.0,
                y: 100.0,
                width: 1600.0,
                height: 900.0
            }
        );
    }

    #[test]
    fn appkit_window_round_trip_preserves_logical_points_and_fractional_positions() {
        for bounds in [
            SnapshotBounds {
                x: 880.0,
                y: 309.0,
                ..SnapshotBounds::default()
            },
            SnapshotBounds {
                x: -1550.5,
                y: -800.25,
                ..SnapshotBounds::default()
            },
        ] {
            assert_eq!(logical_bounds(native_frame(bounds, 1920.0), 1920.0), bounds);
        }
    }
}
