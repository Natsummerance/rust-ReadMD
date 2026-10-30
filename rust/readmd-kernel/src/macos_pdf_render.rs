//! Bounded, in-memory PDF page rasterization with the system Quartz renderer.
//! No helper process, filesystem input/output, or optional engine installation.
//! Drawing transform: https://developer.apple.com/documentation/coregraphics/cgpdfpage/getdrawingtransform(_:rect:rotate:preserveaspectratio:)
use std::ffi::c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct Point { x: f64, y: f64 }
#[repr(C)]
#[derive(Clone, Copy)]
struct Size { width: f64, height: f64 }
#[repr(C)]
#[derive(Clone, Copy)]
struct Rect { origin: Point, size: Size }
#[repr(C)]
struct Transform { a: f64, b: f64, c: f64, d: f64, tx: f64, ty: f64 }
type Ref = *mut c_void;

#[link(name="CoreGraphics", kind="framework")]
unsafe extern "C" {
    fn CGDataProviderCreateWithData(info: Ref, data: *const c_void, size: usize,
        release: Option<unsafe extern "C" fn(Ref, *const c_void, usize)>) -> Ref;
    fn CGDataProviderRelease(value: Ref);
    fn CGPDFDocumentCreateWithProvider(provider: Ref) -> Ref;
    fn CGPDFDocumentRelease(value: Ref);
    fn CGPDFDocumentGetNumberOfPages(document: Ref) -> usize;
    fn CGPDFDocumentGetPage(document: Ref, page: usize) -> Ref;
    fn CGPDFDocumentIsUnlocked(document: Ref) -> bool;
    fn CGPDFPageGetBoxRect(page: Ref, box_kind: i32) -> Rect;
    fn CGPDFPageGetRotationAngle(page: Ref) -> i32;
    fn CGPDFPageGetDrawingTransform(page: Ref, box_kind: i32, rect: Rect,
        rotate: i32, preserve_aspect_ratio: bool) -> Transform;
    fn CGColorSpaceCreateDeviceRGB() -> Ref;
    fn CGColorSpaceRelease(value: Ref);
    fn CGBitmapContextCreate(data: Ref, width: usize, height: usize,
        bits: usize, bytes_per_row: usize, color: Ref, info: u32) -> Ref;
    fn CGContextRelease(value: Ref);
    fn CGContextSetRGBFillColor(context: Ref, r: f64, g: f64, b: f64, a: f64);
    fn CGContextFillRect(context: Ref, rect: Rect);
    fn CGContextConcatCTM(context: Ref, transform: Transform);
    fn CGContextDrawPDFPage(context: Ref, page: Ref);
    fn CGContextFlush(context: Ref);
}

struct Owned(Ref, unsafe extern "C" fn(Ref));
impl Owned {
    fn new(value: Ref, release: unsafe extern "C" fn(Ref)) -> Result<Self, String> {
        if value.is_null() { Err("pdf_renderer_failed".into()) } else { Ok(Self(value, release)) }
    }
}
impl Drop for Owned {
    fn drop(&mut self) { unsafe { (self.1)(self.0) } }
}
fn check_cancel() -> Result<(), String> {
    if crate::speech::cancelled() { Err("cancelled".into()) } else { Ok(()) }
}

pub(crate) fn render(bytes: &[u8], page_no: usize) -> Result<Vec<u8>, String> {
    check_cancel()?;
    if bytes.len() > 32 * 1024 * 1024 { return Err("pdf_page_too_large".into()); }
    if !bytes.starts_with(b"%PDF-") { return Err("pdf_renderer_failed".into()); }
    // Every borrowed input and bitmap buffer outlives all Quartz objects that
    // refer to it. Borrowed CGPDFPageRef is owned by `document`, not released.
    unsafe {
        let provider=Owned::new(CGDataProviderCreateWithData(std::ptr::null_mut(),
            bytes.as_ptr().cast(),bytes.len(),None),CGDataProviderRelease)?;
        let document=Owned::new(CGPDFDocumentCreateWithProvider(provider.0),CGPDFDocumentRelease)?;
        if !CGPDFDocumentIsUnlocked(document.0) { return Err("pdf_password_required".into()); }
        if page_no >= CGPDFDocumentGetNumberOfPages(document.0) { return Err("invalid_page_range".into()); }
        let page=CGPDFDocumentGetPage(document.0,page_no+1);
        if page.is_null() { return Err("pdf_renderer_failed".into()); }
        // CropBox, including the page's own rotation, matches what readers show.
        let crop=CGPDFPageGetBoxRect(page,1);
        let (mut w,mut h)=(crop.size.width,crop.size.height);
        if !w.is_finite() || !h.is_finite() || w<=0.0 || h<=0.0 || w.max(h)>144_000.0 {
            return Err("pdf_page_too_large".into());
        }
        if CGPDFPageGetRotationAngle(page).rem_euclid(180)==90 { std::mem::swap(&mut w,&mut h); }
        let scale=(150.0_f64/72.0).min(1600.0/w.max(h));
        let (width,height)=((w*scale).round().max(1.0) as usize,(h*scale).round().max(1.0) as usize);
        let mut rgba=vec![0u8;width*height*4];
        let color=Owned::new(CGColorSpaceCreateDeviceRGB(),CGColorSpaceRelease)?;
        // Explicit big-endian 32-bit + premultiplied-last = RGBA byte order on
        // both Apple Silicon and Intel. White background makes alpha opaque.
        let context=Owned::new(CGBitmapContextCreate(rgba.as_mut_ptr().cast(),width,height,
            8,width*4,color.0,(4<<12)|1),CGContextRelease)?;
        let target=Rect{origin:Point{x:0.0,y:0.0},size:Size{width:width as f64,height:height as f64}};
        CGContextSetRGBFillColor(context.0,1.0,1.0,1.0,1.0);
        CGContextFillRect(context.0,target);
        let mut transform=CGPDFPageGetDrawingTransform(page,1,target,0,true);
        // Quartz fits oversized pages down but leaves small pages at 1x. Fit
        // the transformed crop bounds to the requested raster size as well,
        // otherwise a 150-DPI bitmap can contain a small 72-DPI page in white.
        let corners=[(crop.origin.x,crop.origin.y),(crop.origin.x+crop.size.width,crop.origin.y),
            (crop.origin.x,crop.origin.y+crop.size.height),(crop.origin.x+crop.size.width,crop.origin.y+crop.size.height)];
        let points=corners.map(|(x,y)|(transform.a*x+transform.c*y+transform.tx,transform.b*x+transform.d*y+transform.ty));
        let (min_x,max_x)=points.iter().fold((f64::INFINITY,f64::NEG_INFINITY),|(lo,hi),p|(lo.min(p.0),hi.max(p.0)));
        let (min_y,max_y)=points.iter().fold((f64::INFINITY,f64::NEG_INFINITY),|(lo,hi),p|(lo.min(p.1),hi.max(p.1)));
        let gain=(width as f64/(max_x-min_x)).min(height as f64/(max_y-min_y));
        if !gain.is_finite() || gain<=0.0{return Err("pdf_renderer_failed".into());}
        transform.a*=gain;transform.b*=gain;transform.c*=gain;transform.d*=gain;
        transform.tx=(transform.tx-min_x)*gain+(width as f64-(max_x-min_x)*gain)/2.0;
        transform.ty=(transform.ty-min_y)*gain+(height as f64-(max_y-min_y)*gain)/2.0;
        CGContextConcatCTM(context.0,transform);
        check_cancel()?;
        CGContextDrawPDFPage(context.0,page);
        CGContextFlush(context.0);
        drop(context);
        check_cancel()?;
        let image=image::RgbaImage::from_raw(width as u32,height as u32,rgba).ok_or("pdf_renderer_failed")?;
        let mut png=std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image).write_to(&mut png,image::ImageFormat::Png)
            .map_err(|_|"pdf_renderer_failed")?;
        check_cancel()?;
        if png.get_ref().len()>8*1024*1024 { return Err("pdf_page_too_large".into()); }
        Ok(png.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Document, Object, Stream};
    fn fixture(rotation: i64) -> Vec<u8> {
        let mut doc=Document::with_version("1.5");
        let pages=doc.new_object_id();
        // Independent PDF operators: red at the lower-left, blue upper-right.
        let stream=doc.add_object(Stream::new(dictionary!{},b"1 0 0 rg 0 0 40 60 re f\n0 0 1 rg 60 140 40 60 re f".to_vec()));
        let page=doc.add_object(dictionary!{"Type"=>"Page","Parent"=>pages,"MediaBox"=>vec![0.into(),0.into(),100.into(),200.into()],"Resources"=>dictionary!{},"Contents"=>stream,"Rotate"=>rotation});
        doc.objects.insert(pages,Object::Dictionary(dictionary!{"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}));
        let root=doc.add_object(dictionary!{"Type"=>"Catalog","Pages"=>pages});doc.trailer.set("Root",root);
        let mut bytes=Vec::new();doc.save_to(&mut bytes).unwrap();bytes
    }
    #[test]
    fn quartz_renders_real_pixels_with_upright_orientation_and_rotation() {
        let image=image::load_from_memory(&render(&fixture(0),0).unwrap()).unwrap().to_rgba8();
        assert_eq!((image.width(),image.height()),(208,417));
        let red:Vec<_>=image.enumerate_pixels().filter(|(_,_,p)|p.0==[255,0,0,255]).map(|(x,y,_)|(x,y)).collect();
        assert!(red.len()>4000,"Quartz must draw PDF operators; red pixels: {}",red.len());
        assert_eq!(image.get_pixel(20,image.height()-20).0,[255,0,0,255],"PDF bottom-left must remain bottom-left in PNG");
        assert_eq!(image.get_pixel(image.width()-20,20).0,[0,0,255,255],"PDF top-right must remain top-right in PNG");
        for rotation in [90,270] {
            let turned=image::load_from_memory(&render(&fixture(rotation),0).unwrap()).unwrap();
            assert_eq!((turned.width(),turned.height()),(417,208));
            assert!(turned.to_rgba8().pixels().filter(|p|p.0==[255,0,0,255]).count()>4000);
        }
    }
    #[test]
    fn malformed_page_range_and_cancel_do_not_produce_a_preview() {
        assert!(render(b"not a PDF",0).is_err());
        assert_eq!(render(&fixture(0),1).unwrap_err(),"invalid_page_range");
        assert_eq!(render(&fixture(0),usize::MAX).unwrap_err(),"invalid_page_range");
        let result=crate::speech::with_cancel(std::sync::Arc::new(||true),||render(&fixture(0),0));
        assert_eq!(result.unwrap_err(),"cancelled");
    }
}
