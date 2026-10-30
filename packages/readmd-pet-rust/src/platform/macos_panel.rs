//! A real NSPanel owns the companion surface. A Tao NSWindow does not acquire
//! NSPanel's non-activation behavior by changing its style mask after creation.
//! The existing Tao event loop, WRY renderer and shared host state are reused.
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
    pub(crate) fn set_bounds(&self, bounds: SnapshotBounds) {
        let height = self.primary_height();
        let frame = NSRect::new(
            NSPoint::new(bounds.x, height - bounds.y - bounds.height),
            NSSize::new(bounds.width, bounds.height),
        );
        self.panel.setFrame_display(frame, true);
    }
    fn primary_height(&self) -> f64 {
        NSScreen::screens(self.panel.mtm())
            .firstObject()
            .map_or(0.0, |screen| screen.frame().size.height)
    }
    pub(crate) fn scale_factor(&self) -> f64 {
        self.panel.backingScaleFactor()
    }
    pub(crate) fn outer_position(&self) -> Result<PhysicalPosition<i32>, &'static str> {
        let frame = self.panel.frame();
        let scale = self.scale_factor();
        Ok(PhysicalPosition::new(
            (frame.origin.x * scale).round() as i32,
            ((self.primary_height() - frame.origin.y - frame.size.height) * scale).round() as i32,
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
