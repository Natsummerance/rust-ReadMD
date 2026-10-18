//! Self-host the same durable service as the desktop LAN host.
use std::{path::PathBuf,sync::Arc};
use readmd_kernel::{collaboration::Service,collaboration_host::Host};
fn run()->Result<(),String>{
    let mut args=std::env::args().skip(1);let mut data=PathBuf::from("readmd-collaboration-data");let mut assets=PathBuf::from("assets");let mut address="127.0.0.1:27941".to_string();let mut origin=String::new();let mut backup=None;let mut restore=None;
    while let Some(arg)=args.next(){match arg.as_str(){
        "--data-dir"=>data=PathBuf::from(args.next().ok_or("Missing data directory")?),
        "--assets"=>assets=PathBuf::from(args.next().ok_or("Missing assets directory")?),
        "--listen"=>address=args.next().ok_or("Missing listen address")?,
        "--public-origin"=>origin=args.next().ok_or("Missing public HTTPS origin")?,
        "--backup"=>backup=Some(PathBuf::from(args.next().ok_or("Missing backup path")?)),
        "--restore"=>restore=Some(PathBuf::from(args.next().ok_or("Missing backup path")?)),
        "--help"=>{println!("readmd-collaboration [--data-dir DIR] [--assets DIR] [--listen IP:PORT] [--public-origin https://HOST]\nreadmd-collaboration --data-dir DIR --backup NEW.sqlite\nreadmd-collaboration --data-dir NEW_DIR --restore BACKUP.sqlite\nSet READMD_COLLAB_ADMIN_TOKEN to a random secret (at least 32 characters). Bind loopback behind an HTTPS reverse proxy for public deployments. Backups and restores never overwrite an existing file.");return Ok(())},
        _=>return Err("Unknown argument".into()),
    }}
    if backup.is_some()&&restore.is_some(){return Err("Choose backup or restore".into());}
    if let Some(source)=restore{std::fs::create_dir_all(&data).map_err(|_|"Cannot create data directory")?;Service::restore_backup(&source,&data.join("collaboration.sqlite")).map_err(|e|e.code)?;println!("Collaboration backup restored into a new profile");return Ok(());}
    if let Some(destination)=backup{let db=data.join("collaboration.sqlite");if !db.is_file(){return Err("Collaboration database not found".into());}Service::open(&db).map_err(|e|e.code)?.backup(&destination).map_err(|e|e.code)?;println!("Collaboration backup created");return Ok(());}
    let socket:std::net::SocketAddr=address.parse().map_err(|_|"Listen address must be IP:PORT")?;
    if !socket.ip().is_loopback()&&!origin.starts_with("https://"){return Err("A public listener requires --public-origin https://HOST and an HTTPS reverse proxy".into());}
    let admin=std::env::var("READMD_COLLAB_ADMIN_TOKEN").map_err(|_|"READMD_COLLAB_ADMIN_TOKEN is required")?;
    if admin.len()<32{return Err("Admin secret must have at least 32 characters".into());}
    std::fs::create_dir_all(&data).map_err(|_|"Cannot create data directory")?;
    let service=Arc::new(Service::open(&data.join("collaboration.sqlite")).map_err(|e|e.code)?);
    let host=Host::start(service,assets,&address,origin,Some(admin)).map_err(|e|e.code)?;
    println!("ReadMD collaboration service listening on port {}",host.port);
    loop{std::thread::park();}
}
fn main(){if let Err(error)=run(){eprintln!("{error}");std::process::exit(1);}}
