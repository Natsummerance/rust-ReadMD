//! The main Cocoa window owns these controls; the web header only reserves
//! their measured bounds. All calls stay on Tao's main event-loop thread.
use objc2_app_kit::{NSWindow, NSWindowButton, NSWindowStyleMask, NSWindowTitleVisibility};
use serde_json::{json, Value};
use tao::{dpi::LogicalPosition, platform::macos::WindowExtMacOS, window::{Theme, Window}};

const HEADER: f64 = 48.;
const LEFT: f64 = 14.;
const GAP: f64 = 14.;
const BUTTONS: [NSWindowButton; 3] = [NSWindowButton::CloseButton, NSWindowButton::MiniaturizeButton, NSWindowButton::ZoomButton];

fn native(window: &Window) -> &NSWindow {
    // Tao created and retains this NSWindow. This module is only called from
    // run_window, on that same main thread, before the Window is dropped.
    unsafe { &*window.ns_window().cast::<NSWindow>() }
}

pub fn configure(window: &Window) {
    if let Some(button) = native(window).standardWindowButton(NSWindowButton::CloseButton) {
        let frame = button.frame();
        // Tao's inset y sets the native container height, not the button's
        // top. Derive it from AppKit's actual button size and local origin.
        let inset_y = HEADER / 2. + frame.origin.y - frame.size.height / 2.;
        window.set_traffic_light_inset(LogicalPosition::new(LEFT, inset_y));
    }
}

pub struct Chrome { visible: bool, edited: bool, theme: Option<Theme> }
impl Default for Chrome {
    fn default() -> Self { Self { visible: true, edited: false, theme: None } }
}
impl Chrome {
    pub fn apply(&mut self, window: &Window, options: &Value) {
        self.visible = options["visible"].as_bool().unwrap_or(true);
        let edited = options["edited"].as_bool().unwrap_or(false);
        if edited != self.edited {
            window.set_is_document_edited(edited); self.edited = edited;
        }
        let theme = match options["theme"].as_str() { Some("dark") => Some(Theme::Dark), Some("light") => Some(Theme::Light), _ => None };
        if theme != self.theme { window.set_theme(theme); self.theme = theme; }
        self.refresh(window);
    }
    pub fn refresh(&self, window: &Window) {
        // Fullscreen's titlebar panel and its reveal belong to AppKit. An
        // ordinary Zen window hides traffic lights with its web chrome.
        let hidden = !self.visible && window.fullscreen().is_none();
        for kind in BUTTONS { if let Some(button) = native(window).standardWindowButton(kind) { button.setHidden(hidden); } }
    }
}

pub fn snapshot(window: &Window) -> Value {
    let ns = native(window);
    let Some(content) = ns.contentView() else { return Value::Null; };
    let height = content.bounds().size.height;
    let mut buttons = Vec::new();
    let mut right: f64 = 0.;
    for (name, kind) in ["close", "minimize", "fullscreen"].into_iter().zip(BUTTONS) {
        if let Some(button) = ns.standardWindowButton(kind) {
            let bounds = button.convertRect_toView(button.bounds(), Some(&content));
            let top = if content.isFlipped() { bounds.origin.y } else { height - bounds.origin.y - bounds.size.height };
            right = right.max(bounds.origin.x + bounds.size.width);
            buttons.push(json!({"kind":name,"x":bounds.origin.x,"y":top,"width":bounds.size.width,"height":bounds.size.height,"hidden":button.isHidden(),"enabled":button.isEnabled()}));
        }
    }
    json!({"platform":"macos","systemControls":true,"headerHeight":HEADER,
        "leadingInset":(right+GAP).ceil().clamp(76.,128.),"buttons":buttons,
        "documentEdited":window.is_document_edited(),"windowNumber":ns.windowNumber(),
        "fullSizeContent":ns.styleMask().contains(NSWindowStyleMask::FullSizeContentView),
        "nativeFullscreen":ns.styleMask().contains(NSWindowStyleMask::FullScreen),
        "titleHidden":ns.titleVisibility()==NSWindowTitleVisibility::Hidden,
        "transparentTitlebar":ns.titlebarAppearsTransparent(),
        "contentWidth":content.bounds().size.width,"contentHeight":height,"scale":window.scale_factor()})
}

// This invokes the actual AppKit target/action, not a CSS or bridge imitation.
// It is compiled out of release and requires an explicitly owned native QA run.
#[cfg(all(feature="native-qa",debug_assertions))]
pub fn perform_button(window:&Window,name:&str)->bool {
    let kind=match name {"close"=>NSWindowButton::CloseButton,"minimize"=>NSWindowButton::MiniaturizeButton,"fullscreen"=>NSWindowButton::ZoomButton,_=>return false};
    let ns=native(window);
    // The modern green control is ZoomButton in both states. Apple's obsolete
    // FullScreenButton is always nil; do not substitute another control.
    let Some(button)=ns.standardWindowButton(kind) else {return false;};
    // AppKit's fullscreen titlebar can be offscreen while the native button's
    // action remains valid. Wait for its enabled state in the driver.
    if button.isHidden()&&window.fullscreen().is_none() || !button.isEnabled() {return false;}
    // The target/action is installed by AppKit on this standard NSWindow
    // button. It is retained, enabled and invoked on the owning main thread.
    unsafe { button.performClick(None); } true
}
