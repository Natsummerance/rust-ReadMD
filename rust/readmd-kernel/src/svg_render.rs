//! Locked offline SVG worker shared by Word and PDF. The original stays intact.
use quick_xml::{events::Event, Reader, XmlVersion};
use sha2::{Digest,Sha256};
use std::{collections::VecDeque,fs,path::PathBuf,process::Stdio,sync::{Mutex,OnceLock},time::{Duration,Instant}};
type Result<T> = std::result::Result<T, &'static str>;
fn hash(bytes:&[u8])->String {format!("{:x}",Sha256::digest(bytes))}
fn css_safe(value:&str)->bool {
    let lower=value.to_ascii_lowercase();
    static URL:OnceLock<regex::Regex>=OnceLock::new();
    !lower.contains('@')&&!lower.contains('\\')&&URL.get_or_init(||regex::Regex::new(r#"url\s*\(\s*['"]?([^)'"\s]*)"#).unwrap())
        .captures_iter(&lower).all(|c|c[1].starts_with('#'))
}
pub(crate) fn validate(bytes:&[u8])->Result<()> {
    if bytes.len()>4*1024*1024{return Err("svg_budget");}
    let source=std::str::from_utf8(bytes).map_err(|_|"svg_invalid")?;
    let mut reader=Reader::from_str(source);reader.config_mut().check_end_names=true;
    let mut roots=0;let mut depth=0usize;let mut nodes=0;let mut style_depth=None;
    loop {match reader.read_event().map_err(|_|"svg_invalid")? {
        event @ (Event::Start(_)|Event::Empty(_))=>{
            let empty=matches!(&event,Event::Empty(_));
            let e=match event{Event::Start(e)|Event::Empty(e)=>e,_=>unreachable!()};
            nodes+=1;if nodes>50000||depth>128{return Err("svg_budget");}
            let local=e.local_name();let name=local.as_ref();
            if depth==0{roots+=1;if roots!=1||name!="svg"{return Err("svg_invalid");}}
            if matches!(name,"script"|"foreignObject"|"animate"|"animateTransform"|"set"){return Err("svg_unsafe");}
            if name=="style"&&!empty{style_depth=Some(depth+1);}
            for attr in e.attributes(){let attr=attr.map_err(|_|"svg_invalid")?;let local=attr.key.local_name();let key=local.as_ref();
                let value=attr.normalized_value(XmlVersion::Implicit1_0).map_err(|_|"svg_invalid")?;
                if key=="base"||key.to_ascii_lowercase().starts_with("on"){return Err("svg_unsafe");}
                if matches!(key,"style"|"fill"|"stroke"|"filter"|"clip-path"|"mask"|"marker"|"marker-start"|"marker-mid"|"marker-end"|"cursor")&&!css_safe(&value){return Err("svg_unsafe");}
                if key=="href"&&!value.starts_with('#')&&!["data:image/png;base64,","data:image/jpeg;base64,","data:image/gif;base64,","data:image/webp;base64,"].iter().any(|p|value.starts_with(p)){return Err("svg_unsafe");}
            }
            if !empty{depth+=1;}
        }
        Event::End(_)=>{if style_depth==Some(depth){style_depth=None;}depth=depth.checked_sub(1).ok_or("svg_invalid")?;}
        Event::DocType(_)|Event::PI(_)=>return Err("svg_unsafe"),
        Event::Text(text)=>{if style_depth.is_some()&&!css_safe(&text.xml_content(XmlVersion::Implicit1_0)){return Err("svg_unsafe");}}
        Event::CData(text)=>{if style_depth.is_some()&&!css_safe(&text.xml_content(XmlVersion::Implicit1_0)){return Err("svg_unsafe");}}
        Event::GeneralRef(text)=>{if style_depth.is_some()||quick_xml::escape::unescape(&format!("&{};",text.as_ref())).is_err(){return Err("svg_unsafe");}}
        Event::Eof=>break,
        _=>(),
    }}
    if roots!=1||depth!=0{return Err("svg_invalid");}Ok(())
}
fn engine()->Result<(PathBuf,String)> {
    let platform=if cfg!(all(windows,target_arch="x86_64")){"windows-x64"}else if cfg!(all(target_os="macos",target_arch="aarch64")){"macos-arm64"}else if cfg!(target_os="macos"){"macos-x64"}else if cfg!(target_arch="aarch64"){"linux-arm64"}else{"linux-x64"};
    let root=crate::paths::assets_dir().join("engines/resvg").join(platform);
    let lock:serde_json::Value=serde_json::from_slice(&fs::read(root.join("lock.json")).map_err(|_|"svg_engine_unavailable")?).map_err(|_|"svg_engine_unavailable")?;
    let file=root.join(if cfg!(windows){"resvg.exe"}else{"resvg"});
    let bytes=fs::read(&file).map_err(|_|"svg_engine_unavailable")?;let digest=hash(&bytes);
    if lock["version"]!="0.48.1"||lock["sha256"].as_str()!=Some(&digest)||lock["size"].as_u64()!=Some(bytes.len() as u64){return Err("svg_engine_unavailable");}
    Ok((file,digest))
}
pub fn rasterize(bytes:&[u8])->Result<Vec<u8>> {
    validate(bytes)?;let(program,engine)=engine()?;
    let fonts=crate::publication_fonts::current();
    if fonts.is_none()&&crate::publication_fonts::needs_cjk(std::str::from_utf8(bytes).map_err(|_|"svg_invalid")?){return Err("publication_fonts_unavailable");}
    let identity=font_identity(fonts.as_deref());
    let key=format!("{engine}:{identity}:{}",hash(bytes));
    static CACHE:OnceLock<Mutex<VecDeque<(String,Vec<u8>)>>>=OnceLock::new();
    // Serialize workers and bound cached decoded resources. No persistent images.
    let mut cache=CACHE.get_or_init(||Mutex::new(VecDeque::new())).lock().unwrap_or_else(|e|e.into_inner());
    if let Some((_,result))=cache.iter().find(|(id,_)|id==&key){return Ok(result.clone());}
    let dir=tempfile::Builder::new().prefix("readmd-svg-").tempdir().map_err(|_|"svg_render_failed")?;
    let source=dir.path().join("input.svg");let target=dir.path().join("output.png");fs::write(&source,bytes).map_err(|_|"svg_render_failed")?;
    // Host fonts are neither versioned nor part of the export snapshot.
    // Pass only reviewed core faces and the leased publication resource set.
    let mut command=crate::silent_command(program);
    command.args(["--width","2400","--height","2400","--skip-system-fonts","--font-family","DejaVu Sans",
        "--serif-family","DejaVu Sans","--sans-serif-family","DejaVu Sans","--monospace-family","DejaVu Sans",
        "--cursive-family","DejaVu Sans","--fantasy-family","DejaVu Sans","--resources-dir"]).arg(dir.path());
    for (name,data) in [("text.ttf",crate::publication_fonts::CORE_TEXT_FALLBACK),("math.ttf",crate::publication_fonts::MATH),("thai.otf",crate::publication_fonts::CORE_THAI_FALLBACK)]{
        let file=dir.path().join(name);fs::write(&file,data).map_err(|_|"svg_render_failed")?;command.arg("--use-font-file").arg(file);
    }
    if let Some(fonts)=fonts{
        for file in [&fonts.sans,&fonts.serif]{
            command.arg("--use-font-file").arg(file);
            let bold=file.with_file_name(file.file_name().and_then(|s|s.to_str()).unwrap_or("").replace("-Regular.","-Bold."));
            if bold!=*file&&bold.is_file(){command.arg("--use-font-file").arg(bold);}
        }
    }
    let mut child=command.arg(&source).arg(&target)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|_|"svg_render_failed")?;
    let deadline=Instant::now()+Duration::from_secs(15);
    loop {match child.try_wait(){Ok(Some(status))=>{if !status.success(){return Err("svg_render_failed");}break;},Ok(None)if Instant::now()<deadline=>std::thread::sleep(Duration::from_millis(15)),_=>{let _=child.kill();let _=child.wait();return Err("svg_render_failed");}}}
    if fs::metadata(&target).map_err(|_|"svg_render_failed")?.len()>24*1024*1024{return Err("svg_budget");}
    let result=fs::read(target).map_err(|_|"svg_render_failed")?;
    if result.len()<24||&result[..8]!=b"\x89PNG\r\n\x1a\n"{return Err("svg_render_failed");}
    let w=u32::from_be_bytes(result[16..20].try_into().unwrap());let h=u32::from_be_bytes(result[20..24].try_into().unwrap());
    if w==0||h==0||w>2400||h>2400{return Err("svg_budget");}
    while (cache.len()>=32||cache.iter().map(|(_,v)|v.len()).sum::<usize>()+result.len()>16*1024*1024)&&!cache.is_empty(){cache.pop_front();}
    if result.len()<=16*1024*1024{cache.push_back((key,result.clone()));}Ok(result)
}
fn font_identity(fonts:Option<&crate::publication_fonts::Resources>)->String{
    let mut digest=Sha256::new();
    for bytes in [crate::publication_fonts::CORE_TEXT_FALLBACK,crate::publication_fonts::MATH,crate::publication_fonts::CORE_THAI_FALLBACK]{digest.update((bytes.len()as u64).to_le_bytes());digest.update(bytes);}
    if let Some(fonts)=fonts{digest.update(fonts.fingerprint.as_bytes());}
    format!("{:x}",digest.finalize())
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn svg_is_self_contained_bounded_and_cannot_read_external_resources(){
        assert!(validate(br##"<svg xmlns="http://www.w3.org/2000/svg"><style>.a{fill:url(#gradient)}</style><rect fill="url(#gradient)"/></svg>"##).is_ok());
        assert!(validate(br#"<svg><text aria-label="a@b.test">a@b.test, \alpha, url(file:///example)</text></svg>"#).is_ok());
        for source in [r#"<svg><image href="file:///private"/></svg>"#,r#"<svg><image href="https://example.test/private"/></svg>"#,r#"<svg><style>@import 'file:///private';</style></svg>"#,r#"<svg><rect style="fill:url(file:///private)"/></svg>"#,r#"<svg><image href="data:image/svg+xml;base64,PHN2Zz4="/></svg>"#,r#"<!DOCTYPE svg><svg/>"#,r#"<svg onload="alert(1)"/>"#,r#"<svg><foreignObject/></svg>"#,r#"<svg/><svg/>"#] {assert!(validate(source.as_bytes()).is_err(),"{source}");}
    }
    #[test]fn renderer_uses_real_core_font_outlines_without_host_fonts(){
        let png=rasterize(br#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="40"><text x="5" y="30" font-size="26" font-family="sans-serif">ReadMD 123</text></svg>"#).unwrap();
        let pixels=image::load_from_memory(&png).unwrap().to_rgba8();assert!(pixels.pixels().filter(|pixel|pixel[3]>128&&pixel[0]<128).count()>100);
        let fake=crate::publication_fonts::Resources{sans:PathBuf::from("unused-sans"),serif:PathBuf::from("unused-serif"),fingerprint:"different-managed-fonts".into()};
        assert_ne!(font_identity(None),font_identity(Some(&fake)));
    }
}
