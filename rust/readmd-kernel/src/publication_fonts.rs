//! Publication resources belong to an application profile, never the host's
//! font installation. A scope pins its private files for the whole operation.
use std::{cell::RefCell,path::{Path,PathBuf},sync::{Arc,atomic::{AtomicUsize,Ordering}}};
pub const MATH:&[u8]=include_bytes!("../../../assets/fonts/STIXTwoMath-Regular.ttf");
pub(crate) const CORE_TEXT_FALLBACK:&[u8]=include_bytes!("../../../assets/fonts/DejaVuSans.ttf");
pub(crate) const CORE_TEXT_FALLBACK_LICENSE:&[u8]=include_bytes!("../../../assets/fonts/DejaVuSans.LICENSE.txt");
/// Keep ordinary typefaces unchanged. Supplemental-plane text can use the
/// same reviewed outline font in the native renderer and managed engines.
pub(crate) fn core_text_fallback_character(character:char)->bool {
    character as u32>0xffff && ttf_parser::Face::parse(CORE_TEXT_FALLBACK,0).ok()
        .and_then(|face|face.glyph_index(character)).is_some_and(|glyph|glyph.0!=0)
}
pub(crate) fn needs_core_text_fallback(text:&str)->bool {
    if !text.chars().any(|character|character as u32>0xffff){return false;}
    let Ok(face)=ttf_parser::Face::parse(CORE_TEXT_FALLBACK,0)else{return false;};
    text.chars().any(|character|character as u32>0xffff&&face.glyph_index(character).is_some_and(|glyph|glyph.0!=0))
}
pub const VERSION:&str="2026.10.10";
static ACTIVE:AtomicUsize=AtomicUsize::new(0);
#[derive(Debug,Clone)]
pub struct Resources{pub sans:PathBuf,pub serif:PathBuf,pub fingerprint:String}
thread_local!{static CURRENT:RefCell<Option<Arc<Resources>>>=const{RefCell::new(None)};}
pub struct Scope{previous:Option<Arc<Resources>>,_lease:Option<crate::capability_packs::ResourceLease>,_thread_bound:std::marker::PhantomData<std::rc::Rc<()>>}
impl Drop for Scope{fn drop(&mut self){CURRENT.with(|c|*c.borrow_mut()=self.previous.take());ACTIVE.fetch_sub(1,Ordering::AcqRel);}}
pub fn busy()->bool{ACTIVE.load(Ordering::Acquire)>0}
pub fn current()->Option<Arc<Resources>>{CURRENT.with(|c|c.borrow().clone())}
pub fn scope(data:&Path,assets:&Path)->Scope{
    let resources=crate::capability_packs::leased_font_resources(data,assets).ok();
    enter(resources)
}
pub(crate) fn scope_checked(data:&Path,assets:&Path,check:&(dyn Fn()->crate::ApiResult<()>+Sync))->crate::ApiResult<Scope>{
    let resources=match crate::capability_packs::leased_font_resources_checked(data,assets,check){
        Ok(resources)=>Some(resources),
        Err(error) if error.code=="capability_pack_invalid"=>None,
        Err(error)=>return Err(error),
    };
    check()?;
    Ok(enter(resources))
}
fn enter(resources:Option<(Resources,crate::capability_packs::ResourceLease)>)->Scope{
    ACTIVE.fetch_add(1,Ordering::AcqRel);
    let (fonts,lease)=match resources{Some((resources,lease))=>(Some(Arc::new(resources)),Some(lease)),None=>(None,None)};
    Scope{previous:CURRENT.with(|c|c.replace(fonts)),_lease:lease,_thread_bound:std::marker::PhantomData}
}
pub fn needs_cjk(text:&str)->bool{text.chars().any(|c|matches!(c as u32,0x2e80..=0x9fff|0xac00..=0xd7af|0xf900..=0xfaff|0x20000..=0x323af))}
pub fn require_cjk(text:&str)->Result<(),String>{if crate::speech::cancelled(){return Err("cancelled".into());}if needs_cjk(text)&&current().is_none(){Err("publication_fonts_unavailable".into())}else{Ok(())}}
pub fn probe_directory(dir:&Path)->Result<(),String>{
    for name in ["ReadMDSansSC-Regular.ttf","ReadMDSansSC-Bold.ttf","ReadMDSerifSC-Regular.ttf","ReadMDSerifSC-Bold.ttf"]{
        let bytes=std::fs::read(dir.join("fonts").join(name)).map_err(|_|"capability_pack_invalid")?;
        let face=ttf_parser::Face::parse(&bytes,0).map_err(|_|"capability_pack_invalid")?;
        if matches!(face.permissions(),Some(ttf_parser::Permissions::Restricted))||!face.is_subsetting_allowed(){return Err("capability_pack_invalid".into());}
        for c in "研究论文书籍α∑".chars(){let glyph=face.glyph_index(c).filter(|g|g.0!=0).ok_or("capability_pack_invalid")?;if face.glyph_hor_advance(glyph).is_none()||face.glyph_bounding_box(glyph).is_none(){return Err("capability_pack_invalid".into());}}
    }Ok(())
}
/// Office family requests remain unchanged in DOCX. Managed PDF workers map
/// platform-specific names to bundled faces with an explicit loss diagnostic.
pub fn family(name:&str)->&str{match name{
    "MicrosoftYaHei"|"Microsoft YaHei"|"SimHei"|"DengXian"|"Arial"|"Helvetica"|"Calibri"|"Noto Sans SC"|"ReadMD Sans SC"=>"ReadMD Sans SC",
    "SimSun"|"KaiTi"|"Times New Roman"|"Georgia"|"Cambria"|"Noto Serif SC"|"ReadMD Serif SC"=>"ReadMD Serif SC",
    "Cambria Math"|"STIX Two Math"=>"STIX Two Math",
    "Consolas"|"Courier New"|"monospace"=>"DejaVu Sans Mono",
    other=>other,
}}
/// MathML uses only faces that can be embedded, rather than requesting an
/// arbitrary host family. The original Office request remains in the DSL.
pub(crate) fn math_text_family(requested:Option<&str>,character:char)->Option<&str>{
    let requested=match requested {Some(name)=>family(name),None if needs_cjk(&character.to_string())=>"ReadMD Sans SC",None=>return None};
    Some(match requested {
        "ReadMD Sans SC"|"ReadMD Serif SC" if current().is_some()=>requested,
        _=>"STIX Two Math",
    })
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn shared_text_fallback_requires_a_real_supplemental_outline(){
        assert!(needs_core_text_fallback("中文 😀"));
        assert!(core_text_fallback_character('😀'));
        assert!(!needs_core_text_fallback("Latin α 中文"));
        assert!(!needs_core_text_fallback("\u{10ffff}"));
        assert!(!core_text_fallback_character('\u{10ffff}'));
        assert!(std::str::from_utf8(CORE_TEXT_FALLBACK_LICENSE).unwrap().contains("Permission is hereby granted"));
        let face=ttf_parser::Face::parse(CORE_TEXT_FALLBACK,0).unwrap();
        let glyph=face.glyph_index('😀').unwrap();assert!(face.glyph_bounding_box(glyph).is_some());
        assert_ne!(face.permissions(),Some(ttf_parser::Permissions::Restricted));
    }
    #[test]fn font_wait_deadline_preserves_thread_context_and_creates_no_files(){
        let data=tempfile::tempdir().unwrap();let assets=tempfile::tempdir().unwrap();
        let previous=current().map(|resources|resources.fingerprint.clone());
        let error=scope_checked(data.path(),assets.path(),&||Err(crate::ApiError::new(422,"typst_timeout"))).err().unwrap();
        assert_eq!(error.code,"typst_timeout");
        assert_eq!(current().map(|resources|resources.fingerprint.clone()),previous);
        assert_eq!(std::fs::read_dir(data.path()).unwrap().count(),0);
    }
    #[test]fn core_math_is_licensed_embeddable_and_has_real_outlines(){let face=ttf_parser::Face::parse(MATH,0).unwrap();assert!(face.tables().math.is_some());assert_ne!(face.permissions(),Some(ttf_parser::Permissions::Restricted));for c in "α∑∫√ℝ𝑥𝛼".chars(){let id=face.glyph_index(c).unwrap();assert!(id.0>0);assert!(face.glyph_bounding_box(id).is_some());}}
    #[test]fn cjk_detection_and_family_mapping_are_not_host_specific(){assert!(needs_cjk("研究𝑥"));assert!(!needs_cjk("α + x = 2"));assert_eq!(family("SimSun"),"ReadMD Serif SC");assert_eq!(family("Consolas"),"DejaVu Sans Mono");}
    #[test]fn missing_fonts_are_localized_at_both_export_boundaries(){let body=crate::latex_compile::error_body("publication_fonts_unavailable");assert_eq!(body["error_code"],"publication_fonts_unavailable");assert!(body.get("error").is_none());crate::speech::with_cancel(Arc::new(||true),||assert_eq!(require_cjk("研究").unwrap_err(),"cancelled"));}
}
