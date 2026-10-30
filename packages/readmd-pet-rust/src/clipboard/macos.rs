//! Native, revision-consistent NSPasteboard capture. File URLs remain paths:
//! reading a clipboard item never authorizes opening its parent directory.
use super::{ClipboardCapture, MAX_PNG_PIXELS};
use crate::protocol::{MAX_CLIPBOARD_PATHS, MAX_CLIPBOARD_PATH_CHARS, MAX_CLIPBOARD_TEXT_BYTES};
use objc2_app_kit::{
    NSPasteboard, NSPasteboardTypeFileURL, NSPasteboardTypePNG, NSPasteboardTypeString,
    NSPasteboardTypeTIFF,
};
use std::ffi::c_void;

pub(super) fn read() -> ClipboardCapture {
    objc2::rc::autoreleasepool(|_| read_from(&NSPasteboard::generalPasteboard()))
}

fn read_from(board: &NSPasteboard) -> ClipboardCapture {
    let revision = board.changeCount();
    let mut out = ClipboardCapture::default();
    let Some(items) = board.pasteboardItems() else {
        return out;
    };
    // A provider can change the pasteboard during a lazy data request. Never
    // combine text, image and file identities from different revisions.
    for item in items.iter().take(MAX_CLIPBOARD_PATHS) {
        if let Some(data) = item.dataForType(unsafe { NSPasteboardTypeString }) {
            if data.len() <= MAX_CLIPBOARD_TEXT_BYTES.saturating_sub(out.text.len()) {
                let text = String::from_utf8_lossy(&data.to_vec()).into_owned();
                let separator = usize::from(!out.text.is_empty());
                if !text.is_empty()
                    && text
                        .len()
                        .saturating_add(out.text.len())
                        .saturating_add(separator)
                        <= MAX_CLIPBOARD_TEXT_BYTES
                {
                    if !out.text.is_empty() {
                        out.text.push('\n');
                    }
                    out.text.push_str(&text);
                }
            }
        }
        if let Some(data) = item.dataForType(unsafe { NSPasteboardTypeFileURL }) {
            if data.len() <= MAX_CLIPBOARD_PATH_CHARS * 4 {
                if let Ok(url) = url::Url::parse(&String::from_utf8_lossy(&data.to_vec())) {
                    if url.scheme() == "file"
                        && matches!(url.host_str(), None | Some("") | Some("localhost"))
                    {
                        if let Ok(path) = url.to_file_path() {
                            if let Some(path) = path
                                .to_str()
                                .filter(|p| p.chars().count() <= MAX_CLIPBOARD_PATH_CHARS)
                            {
                                out.paths.push(path.to_owned());
                            }
                        }
                    }
                }
            }
        }
        if out.image_png.is_empty() {
            for kind in [unsafe { NSPasteboardTypePNG }, unsafe {
                NSPasteboardTypeTIFF
            }] {
                if let Some(data) = item.dataForType(kind) {
                    if data.len() <= 24 * 1024 * 1024 {
                        if let Some(png) = decode_png(&data) {
                            out.image_png = super::image_png_or_empty(png);
                            if !out.image_png.is_empty() {
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
    if board.changeCount() == revision {
        out
    } else {
        ClipboardCapture::default()
    }
}

type Ref = *const c_void;
#[repr(C)]
struct Point {
    x: f64,
    y: f64,
}
#[repr(C)]
struct Size {
    width: f64,
    height: f64,
}
#[repr(C)]
struct Rect {
    origin: Point,
    size: Size,
}
#[link(name = "ImageIO", kind = "framework")]
unsafe extern "C" {
    fn CGImageSourceCreateWithData(data: Ref, options: Ref) -> Ref;
    fn CGImageSourceCopyPropertiesAtIndex(source: Ref, index: usize, options: Ref) -> Ref;
    fn CGImageSourceCreateImageAtIndex(source: Ref, index: usize, options: Ref) -> Ref;
    static kCGImagePropertyPixelWidth: Ref;
    static kCGImagePropertyPixelHeight: Ref;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: Ref);
    fn CFDictionaryGetValue(dictionary: Ref, key: Ref) -> Ref;
    fn CFGetTypeID(value: Ref) -> usize;
    fn CFNumberGetTypeID() -> usize;
    fn CFNumberGetValue(number: Ref, kind: isize, value: *mut i64) -> bool;
}
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGColorSpaceCreateDeviceRGB() -> Ref;
    fn CGBitmapContextCreate(
        data: *mut c_void,
        width: usize,
        height: usize,
        bits: usize,
        row: usize,
        space: Ref,
        info: u32,
    ) -> Ref;
    fn CGContextSetRGBFillColor(context: Ref, r: f64, g: f64, b: f64, a: f64);
    fn CGContextFillRect(context: Ref, rect: Rect);
    fn CGContextDrawImage(context: Ref, rect: Rect, image: Ref);
}
struct Owned(Ref);
impl Owned {
    fn new(value: Ref) -> Option<Self> {
        (!value.is_null()).then_some(Self(value))
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        unsafe {
            CFRelease(self.0);
        }
    }
}

fn decode_png(data: &objc2_foundation::NSData) -> Option<Vec<u8>> {
    // ImageIO metadata is inspected before allocating a decoded bitmap. PNG
    // and TIFF are normalized onto white, then use the existing bounded PNG
    // encoder shared with Windows. No shell, temporary file or global Python.
    unsafe {
        let source = Owned::new(CGImageSourceCreateWithData(
            data as *const _ as Ref,
            std::ptr::null(),
        ))?;
        let properties = Owned::new(CGImageSourceCopyPropertiesAtIndex(
            source.0,
            0,
            std::ptr::null(),
        ))?;
        let dimension = |key| {
            let number = CFDictionaryGetValue(properties.0, key);
            let mut value = 0i64;
            if number.is_null()
                || CFGetTypeID(number) != CFNumberGetTypeID()
                || !CFNumberGetValue(number, 4, &mut value)
                || value <= 0
            {
                None
            } else {
                Some(value as u64)
            }
        };
        let width = dimension(kCGImagePropertyPixelWidth)?;
        let height = dimension(kCGImagePropertyPixelHeight)?;
        if width.checked_mul(height)? > MAX_PNG_PIXELS {
            return None;
        }
        let image = Owned::new(CGImageSourceCreateImageAtIndex(
            source.0,
            0,
            std::ptr::null(),
        ))?;
        let color = Owned::new(CGColorSpaceCreateDeviceRGB())?;
        let mut rgba = vec![0u8; (width * height * 4) as usize];
        let context = Owned::new(CGBitmapContextCreate(
            rgba.as_mut_ptr().cast(),
            width as usize,
            height as usize,
            8,
            width as usize * 4,
            color.0,
            (4 << 12) | 1,
        ))?;
        let rect = || Rect {
            origin: Point { x: 0.0, y: 0.0 },
            size: Size {
                width: width as f64,
                height: height as f64,
            },
        };
        CGContextSetRGBFillColor(context.0, 1.0, 1.0, 1.0, 1.0);
        CGContextFillRect(context.0, rect());
        CGContextDrawImage(context.0, rect(), image.0);
        drop(context);
        Some(super::encode_png(width as u32, height as u32, &rgba))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::NSPasteboardItem;
    use objc2_foundation::{NSArray, NSData, NSString};

    #[test]
    fn native_pasteboard_reads_unicode_file_urls_and_real_png_without_mutating_it() {
        objc2::rc::autoreleasepool(|_| {
            let board = NSPasteboard::pasteboardWithUniqueName();
            let item = NSPasteboardItem::new();
            item.setString_forType(&NSString::from_str("ReadMD 中文 😀"), unsafe {
                NSPasteboardTypeString
            });
            item.setString_forType(
                &NSString::from_str("file:///tmp/ReadMD-%E4%B8%AD%E6%96%87.md"),
                unsafe { NSPasteboardTypeFileURL },
            );
            let png = super::super::encode_png(2, 1, &[255, 0, 0, 255, 0, 255, 0, 255]);
            item.setData_forType(&NSData::with_bytes(&png), unsafe { NSPasteboardTypePNG });
            assert!(board.writeObjects(&NSArray::from_slice(&[ProtocolObject::from_ref(&*item)])));
            let revision = board.changeCount();
            let captured = read_from(&board);
            assert_eq!(captured.text, "ReadMD 中文 😀");
            assert_eq!(captured.paths, vec!["/tmp/ReadMD-中文.md"]);
            assert!(captured.image_png.starts_with("iVBOR"));
            assert_eq!(board.changeCount(), revision);
            board.clearContents();
        });
    }
}
