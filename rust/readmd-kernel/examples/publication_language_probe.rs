//! QA-only direct PDF catalog inspection. Not a product or release binary.
fn main()->Result<(),Box<dyn std::error::Error>>{
    let file=std::env::args_os().nth(1).ok_or("owned PDF path required")?;
    let document=lopdf::Document::load(file)?;
    let raw=document.catalog()?.get(b"Lang")?.as_str()?;
    println!("{}",serde_json::json!({"language":String::from_utf8_lossy(raw),"pages":document.get_pages().len(),"parser":"lopdf 0.42.0; standalone QA probe"}));
    Ok(())
}
