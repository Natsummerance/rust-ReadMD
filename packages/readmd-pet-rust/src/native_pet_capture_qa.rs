//! Captures only the marker-gated debug companion. No screen-wide pixels,
//! arbitrary output paths, system clipboard or inference services are used.
// WRY 0.57's macOS drop collector reads the legacy filenames pasteboard type.
#![allow(deprecated)]
use objc2::{
    define_class, msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject, Bool},
    DefinedClass, MainThreadMarker, MainThreadOnly,
};
use objc2_app_kit::{NSFilenamesPboardType, NSPasteboard};
use objc2_foundation::{
    NSArray, NSData, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};
use std::{fs, io::Write, path::Path};
use wry::{WebView, WebViewExtMacOS};

pub(super) fn frame(webview: &WebView, profile: &Path, sequence: u64) {
    let folder = profile.join("native-pet-frames");
    match fs::symlink_metadata(&folder) {
        Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if fs::create_dir(&folder).is_err() {
                return;
            }
        }
        _ => return,
    }
    let destination = folder.join(format!("{sequence:05}.png"));
    if destination.exists() {
        return;
    }
    unsafe {
        let configuration: Retained<AnyObject> =
            msg_send![AnyClass::get(c"WKSnapshotConfiguration").unwrap(), new];
        let bounds = webview.webview().bounds();
        let size = NSSize::new(
            (bounds.size.width * 3.0).round(),
            (bounds.size.height * 3.0).round(),
        );
        let width: Retained<AnyObject> =
            msg_send![AnyClass::get(c"NSNumber").unwrap(), numberWithDouble:size.width];
        let _: () = msg_send![&configuration, setSnapshotWidth:&*width];
        let _: () = msg_send![&configuration, setAfterScreenUpdates:false];
        let completion = block2::RcBlock::new(
            move |snapshot: *mut AnyObject, error: *mut AnyObject| {
                let save = || -> Option<()> {
                    if snapshot.is_null() || !error.is_null() {
                        return None;
                    }
                    let rep = AnyClass::get(c"NSBitmapImageRep")?;
                    let allocated: objc2::rc::Allocated<AnyObject> = msg_send![rep, alloc];
                    let space = NSString::from_str("NSDeviceRGBColorSpace");
                    let rep: Retained<AnyObject> = msg_send![allocated,initWithBitmapDataPlanes:std::ptr::null_mut::<*mut u8>(),pixelsWide:size.width as isize,pixelsHigh:size.height as isize,bitsPerSample:8_isize,samplesPerPixel:4_isize,hasAlpha:true,isPlanar:false,colorSpaceName:&*space,bytesPerRow:0_isize,bitsPerPixel:0_isize];
                    let graphics = AnyClass::get(c"NSGraphicsContext")?;
                    let context: Option<Retained<AnyObject>> =
                        msg_send![graphics,graphicsContextWithBitmapImageRep:&*rep];
                    let context = context?;
                    let _: () = msg_send![graphics, saveGraphicsState];
                    let _: () = msg_send![graphics,setCurrentContext:&*context];
                    let rect = NSRect::new(NSPoint::new(0., 0.), size);
                    let zero = NSRect::new(NSPoint::new(0., 0.), NSSize::new(0., 0.));
                    let _: () = msg_send![snapshot,drawInRect:rect,fromRect:zero,operation:2_usize,fraction:1.0_f64];
                    let _: () = msg_send![graphics, restoreGraphicsState];
                    let properties: Retained<AnyObject> =
                        msg_send![AnyClass::get(c"NSDictionary")?, dictionary];
                    let data: Option<Retained<NSData>> =
                        msg_send![&*rep, representationUsingType:4_usize, properties:&*properties];
                    let mut output = fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&destination)
                        .ok()?;
                    output.write_all(&data?.to_vec()).ok()?;
                    Some(())
                };
                let _ = save();
            },
        );
        let _: () = msg_send![&webview.webview(), takeSnapshotWithConfiguration:&*configuration, completionHandler:&*completion];
    }
}

struct DragIvars {
    pasteboard: Retained<NSPasteboard>,
    point: NSPoint,
}
define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = DragIvars]
    struct ReadMDOwnedPetDrag;
    unsafe impl NSObjectProtocol for ReadMDOwnedPetDrag {}
    impl ReadMDOwnedPetDrag {
        #[unsafe(method(draggingPasteboard))]
        fn pasteboard(&self) -> *const NSPasteboard { Retained::as_ptr(&self.ivars().pasteboard) }
        #[unsafe(method(draggingLocation))]
        fn location(&self) -> NSPoint { self.ivars().point }
    }
);

pub(super) fn drop_owned_file(webview: &WebView, profile: &Path, perform: bool) -> bool {
    let run = || -> Option<bool> {
        let main = MainThreadMarker::new()?;
        // The path is installed by this test's controller, never supplied by
        // IPC. Restrict the input to the same dedicated QA profile tree.
        let file = fs::read_to_string(profile.join(".readmd-pet-native-drop-owned")).ok()?;
        let file = Path::new(file.trim());
        let meta = fs::symlink_metadata(file).ok()?;
        if !meta.is_file()
            || meta.file_type().is_symlink()
            || !file
                .canonicalize()
                .ok()?
                .starts_with(profile.parent()?.canonicalize().ok()?)
        {
            return None;
        }
        let board = NSPasteboard::pasteboardWithUniqueName();
        let name = NSString::from_str(file.to_str()?);
        let paths = NSArray::arrayWithObject(&*name);
        let view = webview.webview();
        unsafe {
            let types = NSArray::arrayWithObject(NSFilenamesPboardType);
            board.declareTypes_owner(&types, None);
            if !board.setPropertyList_forType(&paths, NSFilenamesPboardType) {
                return None;
            }
            let drag = ReadMDOwnedPetDrag::alloc(main).set_ivars(DragIvars {
                pasteboard: board.clone(),
                point: NSPoint::new(view.bounds().size.width / 2.0, 50.0),
            });
            let drag: Retained<ReadMDOwnedPetDrag> = msg_send![super(drag), init];
            let entered: usize = msg_send![&view, draggingEntered:&*drag];
            let accepted: Bool = if perform {
                msg_send![&view, performDragOperation:&*drag]
            } else {
                Bool::new(entered != 0)
            };
            board.clearContents();
            Some(accepted.as_bool())
        }
    };
    run().unwrap_or(false)
}
