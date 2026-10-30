//! Application-level macOS Quit is distinct from closing a window. Add the
//! official optional delegate callback without replacing Tao's delegate or
//! overriding an existing method. All Quit sources use the shared recovery UI.
use objc2::{
    runtime::{AnyObject, Imp, NSObjectProtocol, Sel},
    sel, MainThreadMarker,
};
#[cfg(all(feature = "native-qa", debug_assertions))]
use objc2::runtime::Bool;
use objc2_app_kit::{NSApplication, NSApplicationTerminateReply};
use std::sync::{Arc, OnceLock};

static QUIT: OnceLock<Arc<dyn Fn() + Send + Sync>> = OnceLock::new();
unsafe extern "C-unwind" fn should_terminate(
    _delegate: *mut AnyObject,
    _selector: Sel,
    _application: *mut AnyObject,
) -> NSApplicationTerminateReply {
    if let Some(quit) = QUIT.get() {
        quit();
    }
    // ReadMDRecovery owns unsaved-body, review and running-task decisions. Its
    // confirmed IPC exits the event loop normally; Cocoa must not pre-empt it.
    NSApplicationTerminateReply::TerminateCancel
}

pub fn install(quit: impl Fn() + Send + Sync + 'static) -> Result<(), &'static str> {
    if QUIT.get().is_some() {
        return Ok(());
    }
    let main = MainThreadMarker::new().ok_or("macos_application_main_thread_required")?;
    let app = NSApplication::sharedApplication(main);
    let delegate = app.delegate().ok_or("macos_application_delegate_missing")?;
    let selector = sel!(applicationShouldTerminate:);
    if delegate.respondsToSelector(selector) {
        return Err("macos_quit_guard_conflict");
    }
    let _ = QUIT.set(Arc::new(quit));
    // NSUInteger return, followed by self, _cmd and the application object.
    // These are 64-bit on both supported Mac architectures. This is an added
    // optional protocol method on our own process's delegate, never swizzling.
    let imp: Imp = unsafe {
        std::mem::transmute(
            should_terminate
                as unsafe extern "C-unwind" fn(
                    *mut AnyObject,
                    Sel,
                    *mut AnyObject,
                ) -> NSApplicationTerminateReply,
        )
    };
    let delegate_object: &AnyObject = (*delegate).as_ref();
    let added = unsafe {
        objc2::ffi::class_addMethod(
            delegate_object.class() as *const _ as *mut _,
            selector,
            imp,
            b"Q@:@\0".as_ptr().cast(),
        )
    };
    if !added.as_bool() {
        return Err("macos_quit_guard_install_failed");
    }
    Ok(())
}

#[cfg(all(feature = "native-qa", debug_assertions))]
pub fn qa_dispatch(action: &str, path: Option<&std::path::Path>, finished: impl Fn(bool) + Send + Sync + 'static) {
    if !matches!(
        action,
        "terminate" | "quit_key" | "save_key" | "undo_menu" | "redo_menu" | "reopen" | "open_file"
    ) {
        finished(false); return;
    }
    if action == "open_file" && !path.is_some_and(|p| p.is_file()) {
        finished(false); return;
    }
    let action = action.to_string();
    let path = path.map(std::path::Path::to_path_buf);
    // Tao's delegate enters its event-handler mutex. Dispatch like a real OS
    // callback, after the current user event returns, never re-enter that lock.
    let operation = block2::RcBlock::new(move || {
        finished(qa_dispatch_now(&action, path.as_deref()));
    });
    // Captures only owned Send + Sync data; AppKit is touched on mainQueue.
    unsafe {
        objc2_foundation::NSOperationQueue::mainQueue().addOperationWithBlock(&operation);
    }
}

#[cfg(all(feature = "native-qa", debug_assertions))]
fn qa_dispatch_now(action: &str, path: Option<&std::path::Path>) -> bool {
    let Some(main) = MainThreadMarker::new() else {
        return false;
    };
    let app = NSApplication::sharedApplication(main);
    match action {
        "undo_menu" | "redo_menu" => {
            let Some(menu)=app.mainMenu().and_then(|m|m.itemAtIndex(2)).and_then(|i|i.submenu()) else {return false;};
            menu.performActionForItemAtIndex(if action=="redo_menu" {1} else {0});
            true
        }
        "save_key" => {
            use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};
            let Some(menu) = app.mainMenu() else {
                return false;
            };
            let (key,code)=("s",1);
            let string = objc2_foundation::NSString::from_str(key);
            let flags=NSEventModifierFlags::Command;
            let Some(event)=NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
                NSEventType::KeyDown,objc2_foundation::NSPoint::new(0.0,0.0),flags,
                objc2_foundation::NSProcessInfo::processInfo().systemUptime(),0,None,&string,&string,false,code) else { return false; };
            menu.performKeyEquivalent(&event)
        }
        "terminate" => {
            app.terminate(None);
            true
        }
        "quit_key" => {
            use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};
            let Some(menu) = app.mainMenu() else {
                return false;
            };
            let string = objc2_foundation::NSString::from_str("q");
            let Some(event) = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
                NSEventType::KeyDown, objc2_foundation::NSPoint::new(0.0,0.0), NSEventModifierFlags::Command,
                objc2_foundation::NSProcessInfo::processInfo().systemUptime(), 0, None, &string, &string, false, 12) else { return false; };
            menu.performKeyEquivalent(&event)
        }
        "reopen" => {
            let Some(delegate) = app.delegate() else {
                return false;
            };
            if !delegate.respondsToSelector(sel!(applicationShouldHandleReopen:hasVisibleWindows:))
            {
                return false;
            }
            let _: Bool = unsafe {
                objc2::msg_send![&*delegate, applicationShouldHandleReopen:&*app, hasVisibleWindows:Bool::NO]
            };
            true
        }
        "open_file" => {
            use objc2_foundation::{NSArray, NSString, NSURL};
            let Some(path) = path.filter(|p| p.is_file()) else {
                return false;
            };
            let Some(delegate) = app.delegate() else {
                return false;
            };
            if !delegate.respondsToSelector(sel!(application:openURLs:)) {
                return false;
            }
            let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
            let urls = NSArray::from_retained_slice(&[url]);
            let _: () = unsafe { objc2::msg_send![&*delegate, application:&*app, openURLs:&*urls] };
            true
        }
        _ => false,
    }
}
