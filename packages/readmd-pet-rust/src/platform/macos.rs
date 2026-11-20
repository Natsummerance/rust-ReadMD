use super::{InteractionRegionSnapshot, PlatformBackend, Window};
use crate::error::HostResult;
use crate::protocol::SnapshotBounds;
use objc2_app_kit::{
    NSEvent, NSEventModifierFlags, NSEventType, NSFloatingWindowLevel, NSWindow,
    NSWindowCollectionBehavior,
};
use objc2_foundation::NSProcessInfo;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// The real NSPanel and its WRY surface remain on the AppKit event-loop thread.
#[derive(Default)]
pub struct MacOsBackend {
    click_through: bool,
    focusable: bool,
    regions: InteractionRegionSnapshot,
    applied: Option<SnapshotBounds>,
    last_screen_check: Option<Instant>,
}

static DRAGGING: AtomicBool = AtomicBool::new(false);
static DRAG_COMPLETED: AtomicBool = AtomicBool::new(false);
pub fn native_drag_active() -> bool {
    DRAGGING.load(Ordering::Acquire)
}
pub fn take_completed_drag() -> bool {
    DRAG_COMPLETED.swap(false, Ordering::AcqRel)
}
pub(crate) fn set_native_drag_active(active: bool) {
    DRAGGING.store(active, Ordering::Release);
}
pub(crate) fn complete_native_drag() {
    DRAGGING.store(false, Ordering::Release);
    DRAG_COMPLETED.store(true, Ordering::Release);
}

impl PlatformBackend for MacOsBackend {
    fn name(&self) -> &'static str {
        "macos-nspanel"
    }
    fn init(&mut self, window: &Window) -> HostResult<()> {
        window.set_always_on_top(true);
        configure_native_panel(window)?;
        Ok(())
    }
    fn set_bounds(&mut self, window: &Window, bounds: SnapshotBounds) -> HostResult<()> {
        self.applied = Some(window.set_bounds(bounds.clamp_host()));
        Ok(())
    }
    fn applied_bounds(&self) -> Option<SnapshotBounds> {
        self.applied
    }
    fn refresh_bounds(&mut self, window: &Window) -> HostResult<Option<SnapshotBounds>> {
        if native_drag_active()
            || self.last_screen_check.is_some_and(|last| last.elapsed() < Duration::from_secs(1))
        {
            return Ok(None);
        }
        self.last_screen_check = Some(Instant::now());
        let recovered = window.recover_offscreen_bounds();
        if let Some(bounds) = recovered {
            self.applied = Some(bounds);
        }
        Ok(recovered)
    }
    fn set_visible(&mut self, window: &Window, visible: bool) -> HostResult<()> {
        window.set_visible(visible);
        Ok(())
    }
    fn set_opacity(&mut self, _window: &Window, _opacity: f64) -> HostResult<()> {
        let raw = _window.ns_window();
        if raw.is_null() {
            return Err(crate::error::HostError::Backend(
                "macos_nswindow_unavailable".into(),
            ));
        }
        // AppKit applies alpha to the native surface, keeping the WebView's
        // transparent pixels transparent while preserving hit testing.
        unsafe {
            (&*(raw as *const NSWindow)).setAlphaValue(_opacity.clamp(0.0, 1.0));
        }
        Ok(())
    }
    fn set_click_through(&mut self, window: &Window, click_through: bool) -> HostResult<()> {
        self.click_through = click_through;
        let raw = window.ns_window();
        if raw.is_null() {
            return Err(crate::error::HostError::Backend(
                "macos_nswindow_unavailable".into(),
            ));
        }
        unsafe {
            (&*(raw as *const NSWindow)).setIgnoresMouseEvents(click_through);
        }
        Ok(())
    }
    fn set_focusable(&mut self, window: &Window, focusable: bool) -> HostResult<()> {
        self.focusable = focusable;
        window.set_focusable(focusable);
        Ok(())
    }
    fn set_always_on_top(&mut self, window: &Window, enabled: bool) -> HostResult<()> {
        window.set_always_on_top(enabled);
        Ok(())
    }
    fn update_interaction_regions(
        &mut self,
        _window: &Window,
        regions: &InteractionRegionSnapshot,
    ) -> HostResult<()> {
        self.regions = regions.clone();
        Ok(())
    }
    fn pointer_origin(&self, window: &Window) -> Option<(f64, f64)> {
        let bounds = window.bounds();
        Some((bounds.x, bounds.y))
    }
    fn drag_window(&self, window: &Window) -> HostResult<()> {
        let raw = window.ns_window();
        if raw.is_null() || native_drag_active() || NSEvent::pressedMouseButtons() & 1 == 0 {
            return Ok(());
        }
        let native = unsafe { &*(raw as *const NSWindow) };
        // The global watcher delivers through Tao's user-event queue, where
        // NSApp.currentEvent is no longer the original mouse-down. Give the
        // real AppKit drag loop an owned event at the current native pointer.
        let location = native.convertPointFromScreen(NSEvent::mouseLocation());
        let event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            NSEventType::LeftMouseDown, location, NSEventModifierFlags::empty(),
            NSProcessInfo::processInfo().systemUptime(), native.windowNumber(), None, 0, 1, 1.0)
            .ok_or_else(|| crate::error::HostError::Backend("macos_drag_event_failed".into()))?;
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                DRAGGING.store(false, Ordering::Release);
            }
        }
        DRAGGING.store(true, Ordering::Release);
        let _reset = Reset;
        native.performWindowDragWithEvent(&event);
        DRAG_COMPLETED.store(true, Ordering::Release);
        Ok(())
    }
}

/// Configure the actual owned NSPanel on its main event-loop thread. Its
/// non-activating style is established by construction and never toggled.
fn configure_native_panel(window: &Window) -> HostResult<()> {
    let raw = window.ns_window();
    if raw.is_null() {
        return Err(crate::error::HostError::Backend(
            "macos_nswindow_unavailable".into(),
        ));
    }
    unsafe {
        let native = &*(raw as *const NSWindow);
        native.setLevel(NSFloatingWindowLevel);
        native.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        native.setHidesOnDeactivate(false);
        native.setHasShadow(false);
        native.setOpaque(false);
        native.setMovable(true);
        native.setMovableByWindowBackground(false);
        native.setIgnoresMouseEvents(true);
    }
    Ok(())
}
