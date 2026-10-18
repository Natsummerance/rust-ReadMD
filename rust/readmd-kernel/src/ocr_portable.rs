//! Owned, version-locked OCR. Rust decodes images; the isolated, static engine
//! reads only a local PNM and locked local models. No system program discovery.
use std::{cell::RefCell,fs,io::{Cursor,Read},path::Path,sync::atomic::{AtomicUsize,Ordering},time::{Duration,Instant}};
thread_local!{static PATHS:RefCell<Option<crate::paths::AppPaths>>=const{RefCell::new(None)};}
static ACTIVE:AtomicUsize=AtomicUsize::new(0);
pub(crate) fn busy()->bool{ACTIVE.load(Ordering::Acquire)>0}
pub(crate) fn with_paths<T>(paths:&crate::paths::AppPaths,work:impl FnOnce()->T)->T{
    struct Restore(Option<crate::paths::AppPaths>);impl Drop for Restore{fn drop(&mut self){PATHS.with(|p|*p.borrow_mut()=self.0.take());}}
    let _restore=Restore(PATHS.with(|p|p.replace(Some(paths.clone()))));work()
}
pub(crate) fn paths()->crate::paths::AppPaths{
    PATHS.with(|p|p.borrow().clone()).unwrap_or_else(||{let data=crate::paths::data_dir();crate::paths::AppPaths::with_dirs(&data,&data.join("workspace"),&crate::paths::assets_dir())})
}
pub(crate) fn available()->bool{let p=paths();crate::capability_packs::ocr_engine(&p.data_dir,&p.assets_dir).is_ok()}
fn check(deadline:Instant)->Result<(),&'static str>{
    if crate::speech::cancelled(){Err("cancelled")}else if Instant::now()>=deadline{Err("ocr_timeout")}else{Ok(())}
}
#[cfg(test)]
fn pnm(bytes:&[u8])->Result<Vec<u8>,String>{
    pnm_at(bytes,Instant::now()+Duration::from_secs(90))
}
fn pnm_at(bytes:&[u8],deadline:Instant)->Result<Vec<u8>,String>{
    check(deadline)?;
    if bytes.len()>64*1024*1024{return Err("ocr_invalid_image".into());}
    let mut reader=image::ImageReader::new(Cursor::new(bytes)).with_guessed_format().map_err(|_|"ocr_invalid_image")?;
    let mut limits=image::Limits::default();limits.max_image_width=Some(8192);limits.max_image_height=Some(8192);limits.max_alloc=Some(128*1024*1024);reader.limits(limits);
    let image=reader.decode().map_err(|_|"ocr_invalid_image")?;
    check(deadline)?;
    if u64::from(image.width())*u64::from(image.height())>25_000_000{return Err("ocr_invalid_image".into());}
    let mut out=format!("P5\n{} {}\n255\n",image.width(),image.height()).into_bytes();
    let rgba=image.to_rgba8();out.reserve(rgba.len()/4);
    for (index,pixel) in rgba.pixels().enumerate(){if index%8192==0{check(deadline)?;}let lum=(u32::from(pixel[0])*299+u32::from(pixel[1])*587+u32::from(pixel[2])*114)/1000;let a=u32::from(pixel[3]);out.push(((lum*a+255*(255-a))/255)as u8);}
    Ok(out)
}
fn recognize_at(program:&Path,models:&Path,bytes:&[u8],language:Option<&str>,deadline:Instant,dir:&Path)->Result<String,String>{
    check(deadline)?;
    let lang=match language.unwrap_or(""){"en"|"eng"=>"eng","zh-CN"|"chi_sim"=>"chi_sim+eng","zh-TW"|"zh-HK"|"chi_tra"=>"chi_tra+eng",_=>"chi_sim+chi_tra+eng"};
    let bytes=pnm_at(bytes,deadline)?;
    let input=dir.join("input.pgm");crate::task_workspace::write(&input,bytes).map_err(|_|"ocr_failed")?;
    let input=fs::File::open(&input).map_err(|_|"ocr_failed")?;
    let output=tempfile::NamedTempFile::new_in(dir).map_err(|_|"ocr_failed")?;let log=tempfile::NamedTempFile::new_in(dir).map_err(|_|"ocr_failed")?;
    for file in [&output,&log]{crate::task_workspace::track_created(file.path(),file.as_file()).map_err(|_|"ocr_failed")?;}
    check(deadline)?;
    let mut command=crate::silent_command(program);
    // Tesseract appends '/' to model paths, breaking Windows verbatim paths.
    // Relative models and inherited PNM also avoid narrow-argv Unicode loss.
    command.current_dir(models).args(["stdin","stdout","--tessdata-dir",".","-l",lang,"--oem","1","--psm","3","-c","user_defined_dpi=300"])
        .env_remove("TESSDATA_PREFIX").env("OMP_THREAD_LIMIT","1");
    let mut child=crate::worker_sandbox::spawn_managed_with_input(&mut command,dir,&input,output.as_file(),log.as_file()).map_err(|_|"ocr_failed")?;
    let stop=|child:&mut crate::worker_sandbox::ManagedWorker|{let _=child.stop();};
    loop{
        if let Err(error)=check(deadline){stop(&mut child);return Err(error.into());}
        if [output.as_file(),log.as_file()].iter().any(|f|f.metadata().map_or(true,|m|m.len()>4*1024*1024)){stop(&mut child);return Err("ocr_failed".into());}
        match child.try_wait(){Ok(Some(status))=>{if status!=0{return Err("ocr_failed".into());}break;},Ok(None)=>std::thread::sleep(Duration::from_millis(20)),Err(_)=>{stop(&mut child);return Err("ocr_failed".into());}}
    }
    child.stop().map_err(|_|"ocr_failed")?;
    let mut text=String::new();output.reopen().map_err(|_|"ocr_failed")?.take(4*1024*1024).read_to_string(&mut text).map_err(|_|"ocr_failed")?;
    check(deadline)?;
    Ok(text.trim().to_string())
}
pub(crate) fn image(bytes:&[u8],language:Option<&str>)->Result<String,String>{
    let p=paths();let deadline=Instant::now()+Duration::from_secs(90);
    let checked=||check(deadline).map_err(|code|crate::ApiError::new(422,code));
    checked().map_err(|e|e.code)?;
    let capacity=crate::capability_process::check_worker_capacity(&p.data_dir,"ocr");
    checked().map_err(|e|e.code)?;
    capacity.map_err(|e| {
        if e.code=="ocr_busy" {e.code} else {"ocr_no_engine".into()}
    })?;
    let (engine,_slot)=crate::capability_packs::lease_reserved(&p.data_dir,&p.assets_dir,"ocr",&checked,||crate::capability_process::reserve_worker(&p.data_dir,"ocr",&ACTIVE))
        .map_err(|e|match e.code.as_str(){"cancelled"|"ocr_timeout"|"ocr_busy"=>e.code,_=>"ocr_no_engine".into()})?;
    let program=engine.directory.join(if cfg!(windows){"bin/tesseract.exe"}else{"bin/tesseract"});let models=engine.directory.join("tessdata");
    let work=crate::task_workspace::Workspace::create(&p.data_dir,"ocr-recognize").map_err(|_|"ocr_failed")?;
    work.scope(||recognize_at(&program,&models,bytes,language,deadline,work.path()))
}
pub(crate) fn probe_directory(dir:&Path,data:&Path)->Result<(),String>{
    let deadline=Instant::now()+Duration::from_secs(12);check(deadline)?;
    let _slot=crate::capability_process::LocalWorker::acquire("ocr",&ACTIVE).map_err(|e|e.code)?;
    probe_at(dir,deadline,data)
}
pub(crate) fn probe_managed()->Result<(),String>{
    let p=paths();let deadline=Instant::now()+Duration::from_secs(12);
    let checked=||check(deadline).map_err(|code|crate::ApiError::new(422,code));
    checked().map_err(|e|e.code)?;
    let capacity=crate::capability_process::check_worker_capacity(&p.data_dir,"ocr");
    checked().map_err(|e|e.code)?;
    capacity.map_err(|e| {
        if e.code=="ocr_busy" {e.code} else {"ocr_no_engine".into()}
    })?;
    let (engine,_slot)=crate::capability_packs::lease_reserved(&p.data_dir,&p.assets_dir,"ocr",&checked,||crate::capability_process::reserve_worker(&p.data_dir,"ocr",&ACTIVE))
        .map_err(|e|match e.code.as_str(){"cancelled"|"ocr_timeout"|"ocr_busy"=>e.code,_=>"ocr_no_engine".into()})?;
    probe_at(&engine.directory,deadline,&p.data_dir)
}
fn probe_at(dir:&Path,deadline:Instant,data:&Path)->Result<(),String>{
    check(deadline)?;
    let temp=crate::task_workspace::Workspace::create(data,"ocr-probe").map_err(|_|"ocr_failed")?;
    temp.scope(||{
    let name=if cfg!(windows){"tesseract.exe"}else{"tesseract"};let program=temp.path().join(name);
    crate::task_workspace::copy(&dir.join("bin").join(name),&program).map_err(|_|"ocr_failed")?;
    let text=recognize_at(&program,&dir.join("tessdata"),include_bytes!("../../../assets/engines/ocr-probe.png"),None,deadline,temp.path())?;
    if text.contains("READMD")&&text.contains("12345"){Ok(())}else{Err("ocr_probe_failed".into())}
    })
}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn worker_launch_failure_is_not_reported_as_a_missing_capability(){
        let root=tempfile::tempdir().unwrap();
        let result=recognize_at(&root.path().join("unavailable-reviewed-program"),root.path(),include_bytes!("../../../assets/engines/ocr-probe.png"),None,Instant::now()+Duration::from_secs(12),root.path());
        assert_eq!(result.unwrap_err(),"ocr_failed");
    }
    #[test]fn expired_or_cancelled_decode_never_becomes_invalid_image(){
        assert_eq!(pnm_at(b"not an image",Instant::now()).unwrap_err(),"ocr_timeout");
        crate::speech::with_cancel(std::sync::Arc::new(||true),||{
            assert_eq!(pnm_at(b"not an image",Instant::now()).unwrap_err(),"cancelled");
        });
    }
    #[test]fn global_busy_precedes_runtime_validation_and_image_decoding(){
        let data=tempfile::tempdir().unwrap();
        let assets=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let scoped=crate::paths::AppPaths::with_dirs(data.path(),&data.path().join("workspace"),&assets);
        let first=crate::capability_process::try_worker_slot(data.path(),"ocr").unwrap().unwrap();
        let second=crate::capability_process::try_worker_slot(data.path(),"ocr").unwrap().unwrap();
        with_paths(&scoped,||assert_eq!(image(b"not an image",None).unwrap_err(),"ocr_busy"));
        assert!(!data.path().join("capabilities/managed-v2/ocr").exists());assert!(!data.path().join("workspace").exists());
        crate::speech::with_cancel(std::sync::Arc::new(||true),||{
            with_paths(&scoped,||assert_eq!(image(b"not an image",None).unwrap_err(),"cancelled"));
        });drop((first,second));
    }
    #[test]fn transparent_pixels_are_white_and_malformed_images_are_rejected(){
        let mut rgba=image::RgbaImage::new(2,1);rgba.put_pixel(0,0,image::Rgba([0,0,0,0]));rgba.put_pixel(1,0,image::Rgba([0,0,0,255]));
        let mut bytes=Cursor::new(Vec::new());rgba.write_to(&mut bytes,image::ImageFormat::Png).unwrap();let decoded=pnm(bytes.get_ref()).unwrap();assert!(decoded.ends_with(&[255,0]));assert!(pnm(b"not an image").is_err());
    }
    #[test]fn nested_profile_scope_restores_after_panics(){
        let original=paths();let mut scoped=original.clone();scoped.data_dir=std::path::PathBuf::from("owned-profile");
        let _=std::panic::catch_unwind(||with_paths(&scoped,||{assert_eq!(paths().data_dir,scoped.data_dir);panic!("test");}));assert_eq!(paths().data_dir,original.data_dir);
    }
}
