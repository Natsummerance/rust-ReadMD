//! AppKit open/save panels. HTTP workers enqueue an inert request; only the
//! native event-loop thread constructs or touches Cocoa objects. A successful
//! picker is still subject to the existing backend grant/revision/write checks.
use crate::native_dialogs::{DialogOutcome, DialogShape, Request};
use objc2::{rc::Retained, MainThreadMarker, Message};
use objc2_app_kit::{NSOpenPanel, NSSavePanel};
use objc2_foundation::{NSArray, NSString, NSURL};
use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex, OnceLock,
    },
};

type Wake = Arc<dyn Fn() + Send + Sync>;
static WAKE: OnceLock<Wake> = OnceLock::new();
static QUEUE: Mutex<Option<(Request, mpsc::SyncSender<DialogOutcome>)>> = Mutex::new(None);
static BUSY: AtomicBool = AtomicBool::new(false);
thread_local! { static ACTIVE: RefCell<Option<Active>> = const { RefCell::new(None) }; }
thread_local! { static PARENT: RefCell<Option<Retained<objc2_app_kit::NSWindow>>> = const { RefCell::new(None) }; }
enum Panel {
    Open(Retained<NSOpenPanel>),
    Save(Retained<NSSavePanel>),
}
impl Panel {
    fn base(&self) -> &NSSavePanel {
        match self {
            Self::Open(p) => p,
            Self::Save(p) => p,
        }
    }
    fn outcome(&self, response: isize) -> DialogOutcome {
        if response == 0 {
            return DialogOutcome::Canceled;
        }
        if response != 1 {
            return DialogOutcome::Unavailable;
        }
        let paths: Vec<_> = match self {
            Self::Open(panel) => panel
                .URLs()
                .iter()
                .filter(|u| u.isFileURL())
                .filter_map(|u| u.path().map(|p| p.to_string()))
                .collect(),
            Self::Save(panel) => panel
                .URL()
                .filter(|u| u.isFileURL())
                .and_then(|u| u.path().map(|p| p.to_string()))
                .into_iter()
                .collect(),
        };
        if paths.is_empty() {
            DialogOutcome::Unavailable
        } else {
            DialogOutcome::Picked(paths)
        }
    }
}
struct Active {
    panel: Panel,
    reply: mpsc::SyncSender<DialogOutcome>,
}

pub fn set_wake(wake: impl Fn() + Send + Sync + 'static) {
    let _ = WAKE.set(Arc::new(wake));
}

pub fn set_parent(window: &tao::window::Window) {
    use tao::platform::macos::WindowExtMacOS;
    if MainThreadMarker::new().is_none() {
        return;
    }
    // Tao owns this actual NSWindow. Retain it only on the GUI thread for
    // native sheets; no borrowed Cocoa pointer reaches an HTTP worker.
    let parent = unsafe { Retained::retain(window.ns_window().cast::<objc2_app_kit::NSWindow>()) };
    PARENT.with(|p| *p.borrow_mut() = parent);
}

pub(crate) fn dispatch(request: &Request) -> Option<DialogOutcome> {
    let wake = WAKE.get()?;
    if MainThreadMarker::new().is_some()
        || BUSY
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
    {
        return Some(DialogOutcome::Unavailable);
    }
    let (reply, receive) = mpsc::sync_channel(1);
    *QUEUE.lock().unwrap_or_else(|e| e.into_inner()) = Some((request.clone(), reply));
    wake();
    Some(receive.recv().unwrap_or(DialogOutcome::Unavailable))
}

/// Called by the native event loop. Presentation is asynchronous, keeping the
/// reader responsive and making Cancel/selection real AppKit outcomes.
pub fn drain() {
    let Some(_) = MainThreadMarker::new() else {
        return;
    };
    let Some((request, reply)) = QUEUE.lock().unwrap_or_else(|e| e.into_inner()).take() else {
        return;
    };
    // Cocoa can notify application/window delegates synchronously. Present on
    // the main operation queue after Tao releases its event-handler lock.
    let operation = block2::RcBlock::new(move || present(&request, reply.clone()));
    // Request and SyncSender are owned Send + Sync; native objects are created
    // only inside the operation on the system main queue.
    unsafe {
        objc2_foundation::NSOperationQueue::mainQueue().addOperationWithBlock(&operation);
    }
}

fn present(request: &Request, reply: mpsc::SyncSender<DialogOutcome>) {
    let completed = Arc::new(AtomicBool::new(false));
    let outcome = objc2::exception::catch(std::panic::AssertUnwindSafe(|| {
        present_inner(request, reply.clone(), completed.clone())
    }));
    if outcome.is_err() && !completed.swap(true, Ordering::AcqRel) {
        let active = ACTIVE.with(|a| a.borrow_mut().take());
        if let Some(active) = active {
            let _ = objc2::exception::catch(std::panic::AssertUnwindSafe(|| {
                active.panel.base().orderOut(None)
            }));
        }
        BUSY.store(false, Ordering::Release);
        let _ = reply.try_send(DialogOutcome::Unavailable);
        eprintln!("macos_file_dialog_unavailable");
    }
}

fn present_inner(
    request: &Request,
    reply: mpsc::SyncSender<DialogOutcome>,
    completed: Arc<AtomicBool>,
) {
    let Some(main) = MainThreadMarker::new() else {
        BUSY.store(false, Ordering::Release);
        let _ = reply.send(DialogOutcome::Unavailable);
        return;
    };
    let panel = match request.shape {
        DialogShape::ChooseFile
        | DialogShape::ChooseAnyFile
        | DialogShape::ChooseManyFiles
        | DialogShape::ChooseFolder => {
            let panel = NSOpenPanel::openPanel(main);
            panel.setCanChooseDirectories(request.shape == DialogShape::ChooseFolder);
            panel.setCanChooseFiles(request.shape != DialogShape::ChooseFolder);
            panel.setAllowsMultipleSelection(request.shape == DialogShape::ChooseManyFiles);
            Panel::Open(panel)
        }
        _ => Panel::Save(NSSavePanel::savePanel(main)),
    };
    let base = panel.base();
    base.setCanCreateDirectories(true);
    base.setTitle(Some(&NSString::from_str(request.shape.title())));
    if let Some(dir) = &request.initial_dir {
        base.setDirectoryURL(Some(&NSURL::fileURLWithPath_isDirectory(
            &NSString::from_str(dir),
            true,
        )));
    }
    if let Some(name) = request.effective_name() {
        base.setNameFieldStringValue(&NSString::from_str(&name));
    }
    if let Some((_, globs)) = crate::native_dialogs::filter_globs(request) {
        let types: Vec<_> = globs
            .iter()
            .filter_map(|g| g.strip_prefix("*."))
            .filter(|g| !g.contains('*'))
            .map(NSString::from_str)
            .collect();
        if !types.is_empty() {
            #[allow(deprecated)]
            base.setAllowedFileTypes(Some(&NSArray::from_retained_slice(&types)));
            base.setAllowsOtherFileTypes(true);
        }
    }
    let base = base.retain();
    ACTIVE.with(|active| *active.borrow_mut() = Some(Active { panel, reply }));
    let completion = block2::RcBlock::new(move |response: isize| {
        if completed.swap(true, Ordering::AcqRel) {
            return;
        }
        let active = ACTIVE.with(|a| a.borrow_mut().take());
        BUSY.store(false, Ordering::Release);
        if let Some(active) = active {
            let _ = active.reply.send(active.panel.outcome(response));
        }
    });
    let parent = PARENT.with(|p| p.borrow().clone());
    if let Some(parent) = parent {
        base.beginSheetModalForWindow_completionHandler(&parent, &completion);
    } else {
        base.beginWithCompletionHandler(&completion);
    }
}

#[cfg(all(feature = "native-qa", debug_assertions))]
pub fn qa_schedule(
    choice: String,
    path: Option<std::path::PathBuf>,
    finished: impl Fn(serde_json::Value) + Send + Sync + 'static,
) {
    let operation = block2::RcBlock::new(move || finished(qa_respond(&choice, path.as_deref())));
    unsafe {
        objc2_foundation::NSOperationQueue::mainQueue().addOperationWithBlock(&operation);
    }
}

/// Only the opt-in debug native QA host can reach this. It operates on the
/// real panel; the normal panel completion and backend validation still run.
#[cfg(all(feature = "native-qa", debug_assertions))]
pub fn qa_respond(choice: &str, path: Option<&std::path::Path>) -> serde_json::Value {
    match objc2::exception::catch(std::panic::AssertUnwindSafe(|| {
        qa_respond_inner(choice, path)
    })) {
        Ok(value) => value,
        Err(exception) => {
            let active = ACTIVE.with(|a| a.borrow_mut().take());
            BUSY.store(false, Ordering::Release);
            if let Some(active) = active {
                let _ = active.reply.send(DialogOutcome::Unavailable);
            }
            serde_json::json!({"available":false,"native_exception":true,
                "native_exception_description":exception.map(|e|format!("{e:?}")).unwrap_or_default()})
        }
    }
}

#[cfg(all(feature = "native-qa", debug_assertions))]
fn qa_respond_inner(choice: &str, path: Option<&std::path::Path>) -> serde_json::Value {
    let panel = ACTIVE.with(|a| a.borrow().as_ref().map(|a| a.panel.base().retain()));
    let Some(panel) = panel else {
        return serde_json::json!({"available":false});
    };
    let visible = panel.isVisible();
    let number = panel.windowNumber();
    let mut native_accept_succeeded = None;
    if choice == "activate" {
        if let Some(main) = MainThreadMarker::new() {
            #[allow(deprecated)]
            objc2_app_kit::NSApplication::sharedApplication(main).activateIgnoringOtherApps(true);
        }
    } else if choice == "cancel" {
        unsafe {
            panel.cancel(None);
        }
    } else if choice == "configure" {
        if let Some(path) = path {
            if let (Some(parent), Some(name)) =
                (path.parent(), path.file_name().and_then(|s| s.to_str()))
            {
                let same_directory = panel
                    .directoryURL()
                    .and_then(|u| u.path())
                    .is_some_and(|p| std::path::Path::new(&p.to_string()) == parent);
                if !same_directory {
                    panel.setDirectoryURL(Some(&NSURL::fileURLWithPath_isDirectory(
                        &NSString::from_str(&parent.to_string_lossy()),
                        true,
                    )));
                }
                panel.setNameFieldStringValue(&NSString::from_str(name));
            }
        }
    } else if choice == "accept" {
        // The system file-list view lives in PowerBox's process. This owned
        // AppKit driver cannot confirm its selection without WindowServer
        // input or a manual click. Refuse rather than fake a panel completion.
        native_accept_succeeded = Some(false);
    }
    let directory_matches = path.is_some_and(|path| {
        panel
            .directoryURL()
            .and_then(|u| u.path())
            .is_some_and(|s| {
                path.parent()
                    .is_some_and(|p| p == std::path::Path::new(&s.to_string()))
            })
    });
    let name_matches = path.is_some_and(|path| {
        path.file_name().and_then(|s| s.to_str())
            == Some(panel.nameFieldStringValue().to_string().as_str())
    });
    let (open, selected_matches) =
        ACTIVE.with(|active| match active.borrow().as_ref().map(|a| &a.panel) {
            Some(Panel::Open(open)) => (
                true,
                path.is_some_and(|wanted| {
                    open.URLs()
                        .iter()
                        .filter_map(|u| u.path())
                        .any(|p| std::path::Path::new(&p.to_string()) == wanted)
                }),
            ),
            _ => (false, false),
        });
    let configured = if open {
        selected_matches
    } else {
        directory_matches && name_matches
    };
    serde_json::json!({"available":true,"visible":visible,"window_number":number,"configured":configured,
        "open":open,"directory_matches":directory_matches,"name_matches":name_matches,"selected_matches":selected_matches,
        "key_window":panel.isKeyWindow(),"has_sheet":panel.attachedSheet().is_some(),
        "native_accept_succeeded":native_accept_succeeded,
        "native_confirmation_automation_supported":false})
}
