//! Cross-process read/use versus install/remove exclusion for managed packs.
//! The empty lock files are stable names, never PID files and never unlinked.
//! Closing the non-inherited handle (including process termination) releases
//! exclusion. No new dependency or MSRV increase is needed.
use crate::{ApiError,ApiResult};
use std::{fs,path::{Path,PathBuf},sync::atomic::{AtomicUsize,Ordering}};

#[derive(Clone,Copy)]
pub(crate) enum Mode { Shared, Exclusive }
pub(crate) struct Guard { _file:fs::File }
// Fixed names coordinate all current clients of one profile. Do not make the
// number a caller argument: clients using different limits would overcommit.
const WORKER_SLOTS:usize=2;
// One fixed protocol for desktop, MCP, browser uploads and installer CLI.
// Round each declaration up to 64 MiB: at most four active transfers and
// 2 GiB reserved compressed input per profile, including their installation.
// This bounds active input, not expanded engines or retained recovery data.
const TRANSFER_SLOTS:usize=4;
const TRANSFER_BYTES_SLOTS:usize=32;
const TRANSFER_QUANTUM:u64=crate::capability_archive::MAX_DOWNLOAD/TRANSFER_BYTES_SLOTS as u64;
pub(crate) struct TransferReservation { _count:Guard,_bytes:Vec<Guard> }
pub(crate) fn reserve_transfer(data:&Path,total:u64)->ApiResult<TransferReservation> {
    if total==0||total>crate::capability_archive::MAX_DOWNLOAD { return Err(invalid()); }
    let busy=||ApiError::new(409,"capability_install_busy");
    // Serialize only allocation, never network, hashing or installation. This
    // avoids two partially acquired byte allocations starving each other.
    let _gate=try_key(data,"transfers",Key::TransferGate,Mode::Exclusive)?.ok_or_else(busy)?;
    let mut count=None;
    for index in 0..TRANSFER_SLOTS {
        if let Some(guard)=try_key(data,"transfers",Key::TransferCount(index),Mode::Exclusive)? {
            count=Some(guard);break;
        }
    }
    let count=count.ok_or_else(busy)?;
    let required=total.div_ceil(TRANSFER_QUANTUM) as usize;
    let mut bytes=Vec::with_capacity(required);
    for index in 0..TRANSFER_BYTES_SLOTS {
        if let Some(guard)=try_key(data,"transfers",Key::TransferBytes(index),Mode::Exclusive)? {
            bytes.push(guard);
            if bytes.len()==required { return Ok(TransferReservation{_count:count,_bytes:bytes}); }
        }
    }
    // Drop every partial allocation, including the count, on all failures.
    Err(busy())
}
pub(crate) struct LocalWorker { active:&'static AtomicUsize }
impl LocalWorker {
    pub(crate) fn acquire(id:&str,active:&'static AtomicUsize)->ApiResult<Self> {
        let code=worker_busy(id)?;
        active.fetch_update(Ordering::AcqRel,Ordering::Acquire,|n|if n<WORKER_SLOTS{Some(n+1)}else{None})
            .map_err(|_|ApiError::new(409,code))?;
        Ok(Self{active})
    }
}
impl Drop for LocalWorker { fn drop(&mut self) { self.active.fetch_sub(1,Ordering::AcqRel); } }
pub(crate) struct WorkerReservation { _local:LocalWorker,_process:Guard }
fn worker_busy(id:&str)->ApiResult<&'static str> {
    match id { "latex"=>Ok("latex_busy"),"typst"=>Ok("typst_busy"),"ocr"=>Ok("ocr_busy"),_=>Err(invalid()) }
}
pub(crate) fn reserve_worker(data:&Path,id:&str,active:&'static AtomicUsize)->ApiResult<WorkerReservation> {
    let code=worker_busy(id)?;
    let local=LocalWorker::acquire(id,active)?;
    let process=try_worker_slot(data,id)?.ok_or_else(||ApiError::new(409,code))?;
    Ok(WorkerReservation{_local:local,_process:process})
}
#[derive(Clone,Copy)]
enum Key { Resource, Worker(usize), TransferGate, TransferCount(usize), TransferBytes(usize) }
fn invalid()->ApiError { ApiError::new(422,"capability_pack_invalid") }
fn real_directory(path:&Path)->ApiResult<()> {
    let metadata=fs::symlink_metadata(path).map_err(|_|invalid())?;
    if !metadata.is_dir()||metadata.file_type().is_symlink() { return Err(invalid()); }
    #[cfg(windows)] {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes()&0x400!=0 { return Err(invalid()); }
    }
    Ok(())
}
fn location(data:&Path,id:&str,key:Key)->ApiResult<(PathBuf,PathBuf)> {
    // Callers additionally resolve a fixed capability Spec. Keep this module
    // incapable of naming arbitrary paths even when invoked incorrectly.
    let name=match key {
        Key::Resource if matches!(id,"pdfium"|"typst"|"ocr"|"fonts"|"latex")=>format!("{id}.lock"),
        Key::Worker(index) if index<WORKER_SLOTS&&matches!(id,"latex"|"typst"|"ocr")=>format!("{id}.worker-{index}.lock"),
        Key::TransferGate if id=="transfers"=>"transfer-v1.gate.lock".into(),
        Key::TransferCount(index) if id=="transfers"&&index<TRANSFER_SLOTS=>format!("transfer-v1.count-{index}.lock"),
        Key::TransferBytes(index) if id=="transfers"&&index<TRANSFER_BYTES_SLOTS=>format!("transfer-v1.bytes-{index}.lock"),
        _=>return Err(invalid()),
    };
    let base=data.canonicalize().map_err(|_|invalid())?;
    real_directory(&base)?;
    let mut parent=base.clone();
    for name in ["capabilities",crate::capability_store::PROTOCOL,".locks"] {
        parent.push(name);
        match fs::create_dir(&parent) {
            Ok(())=>(),
            Err(e) if e.kind()==std::io::ErrorKind::AlreadyExists=>(),
            Err(_)=>return Err(invalid()),
        }
        real_directory(&parent)?;
    }
    if !parent.canonicalize().map_err(|_|invalid())?.starts_with(&base) { return Err(invalid()); }
    Ok((parent.join(name),parent))
}
fn check_file(file:&fs::File)->ApiResult<()> {
    let metadata=file.metadata().map_err(|_|invalid())?;
    if !metadata.is_file()||metadata.len()!=0 { return Err(invalid()); }
    #[cfg(windows)] {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes()&0x400!=0 { return Err(invalid()); }
    }
    Ok(())
}

enum SlotProbe { Missing, Busy, Available(fs::File) }
/// Advisory fast rejection only. Never creates a profile, lock or worker slot
/// and never replaces the authoritative reservation under the resource guard.
/// A free observation may race; reserve_worker still enforces the actual limit.
pub(crate) fn check_worker_capacity(data:&Path,id:&str)->ApiResult<()> {
    let busy=worker_busy(id)?;
    match fs::symlink_metadata(data) {
        Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(()),
        Err(_)=>return Err(invalid()),
        Ok(_)=>(),
    }
    let base=data.canonicalize().map_err(|_|invalid())?;
    // Match location(): an explicitly selected profile alias resolves once;
    // redirects inside that canonical profile remain forbidden.
    real_directory(&base)?;
    let mut parent=base.clone();
    for name in ["capabilities",crate::capability_store::PROTOCOL,".locks"] {
        parent.push(name);
        match fs::symlink_metadata(&parent) {
            Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(()),
            Err(_)=>return Err(invalid()),
            Ok(_)=>real_directory(&parent)?,
        }
    }
    if parent.canonicalize().map_err(|_|invalid())?!=parent { return Err(invalid()); }
    for index in 0..WORKER_SLOTS {
        let path=parent.join(format!("{id}.worker-{index}.lock"));
        match probe_existing_slot(&path)? {
            SlotProbe::Missing=>return Ok(()),
            SlotProbe::Busy=>(),
            SlotProbe::Available(file)=>{
                check_file(&file)?;
                real_directory(&parent)?;
                real_directory(parent.parent().ok_or_else(invalid)?)?;
                if parent.canonicalize().map_err(|_|invalid())?!=parent { return Err(invalid()); }
                return Ok(());
            }
        }
    }
    Err(ApiError::new(409,busy))
}

#[cfg(windows)]
fn probe_existing_slot(path:&Path)->ApiResult<SlotProbe> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
    match fs::OpenOptions::new().read(true).write(true).share_mode(0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0).open(path) {
        Ok(file)=>Ok(SlotProbe::Available(file)),
        Err(e) if e.kind()==std::io::ErrorKind::NotFound=>Ok(SlotProbe::Missing),
        Err(e) if e.raw_os_error()==Some(32)=>Ok(SlotProbe::Busy),
        Err(_)=>Err(invalid()),
    }
}
#[cfg(any(target_os="linux",target_os="macos"))]
fn probe_existing_slot(path:&Path)->ApiResult<SlotProbe> {
    use std::os::{fd::AsRawFd,unix::fs::OpenOptionsExt};
    extern "C" { fn flock(fd:std::os::raw::c_int,operation:std::os::raw::c_int)->std::os::raw::c_int; }
    #[cfg(target_os="linux")] const NOFOLLOW:i32=0x20000;
    #[cfg(target_os="macos")] const NOFOLLOW:i32=0x100;
    let file=match fs::OpenOptions::new().read(true).write(true).custom_flags(NOFOLLOW).open(path) {
        Ok(file)=>file,
        Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(SlotProbe::Missing),
        Err(_)=>return Err(invalid()),
    };
    check_file(&file)?;
    loop {
        if unsafe { flock(file.as_raw_fd(),2|4) }==0 { return Ok(SlotProbe::Available(file)); }
        let error=std::io::Error::last_os_error();
        if error.kind()==std::io::ErrorKind::Interrupted { continue; }
        if error.kind()==std::io::ErrorKind::WouldBlock { return Ok(SlotProbe::Busy); }
        return Err(invalid());
    }
}
#[cfg(not(any(windows,target_os="linux",target_os="macos")))]
fn probe_existing_slot(_:&Path)->ApiResult<SlotProbe> { Err(invalid()) }

/// None means actual contention, not a corrupt path or failed permissions.
pub(crate) fn try_acquire(data:&Path,id:&str,mode:Mode)->ApiResult<Option<Guard>> {
    try_key(data,id,Key::Resource,mode)
}

/// Reserve one of the profile's two engine slots without waiting or hashing.
/// The caller must already hold the shared resource lease (or its install
/// guard for a probe). A corrupt slot is an error, not an extra available slot.
pub(crate) fn try_worker_slot(data:&Path,id:&str)->ApiResult<Option<Guard>> {
    for index in 0..WORKER_SLOTS {
        if let Some(guard)=try_key(data,id,Key::Worker(index),Mode::Exclusive)? {
            return Ok(Some(guard));
        }
    }
    Ok(None)
}

fn try_key(data:&Path,id:&str,key:Key,mode:Mode)->ApiResult<Option<Guard>> {
    let (path,parent)=location(data,id,key)?;
    let Some(file)=open(&path,mode)? else { return Ok(None); };
    check_file(&file)?;
    real_directory(&parent)?;
    real_directory(parent.parent().ok_or_else(invalid)?)?;
    // Check that an intermediate directory was not redirected while opening.
    if parent.canonicalize().map_err(|_|invalid())?!=parent { return Err(invalid()); }
    Ok(Some(Guard{_file:file}))
}

#[cfg(windows)]
fn open(path:&Path,mode:Mode)->ApiResult<Option<fs::File>> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows::Win32::Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT,FILE_SHARE_READ};
    let mut options=fs::OpenOptions::new();
    options.read(true).custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0);
    match mode {
        Mode::Shared=>{options.share_mode(FILE_SHARE_READ.0);},
        Mode::Exclusive=>{options.write(true).create(true).truncate(false).share_mode(0);},
    }
    match options.open(path) {
        Ok(file)=>Ok(Some(file)),
        Err(e) if e.raw_os_error()==Some(32)=>Ok(None), // ERROR_SHARING_VIOLATION
        Err(e) if matches!(mode,Mode::Shared)&&e.kind()==std::io::ErrorKind::NotFound=>{
            // A read-only open cannot create a file. Create once without
            // truncation, close it, then reopen with the shared contract.
            let created=fs::OpenOptions::new().read(true).write(true).create_new(true)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0).share_mode(0).open(path);
            match created {
                Ok(file)=>{check_file(&file)?;drop(file);},
                Err(e) if e.kind()==std::io::ErrorKind::AlreadyExists=>(),
                Err(e) if e.raw_os_error()==Some(32)=>return Ok(None),
                Err(_)=>return Err(invalid()),
            }
            match options.open(path) {
                Ok(file)=>Ok(Some(file)),
                Err(e) if e.raw_os_error()==Some(32)=>Ok(None),
                Err(_)=>Err(invalid()),
            }
        },
        Err(_)=>Err(invalid()),
    }
}

#[cfg(any(target_os="linux",target_os="macos"))]
fn open(path:&Path,mode:Mode)->ApiResult<Option<fs::File>> {
    use std::os::{fd::AsRawFd,unix::fs::OpenOptionsExt};
    extern "C" { fn flock(fd:std::os::raw::c_int,operation:std::os::raw::c_int)->std::os::raw::c_int; }
    #[cfg(target_os="linux")] const NOFOLLOW:i32=0x20000;
    #[cfg(target_os="macos")] const NOFOLLOW:i32=0x100;
    let file=fs::OpenOptions::new().read(true).write(true).create(true).truncate(false)
        .custom_flags(NOFOLLOW).mode(0o600).open(path).map_err(|_|invalid())?;
    let operation=match mode { Mode::Shared=>1,Mode::Exclusive=>2 }|4; // LOCK_NB
    loop {
        if unsafe{flock(file.as_raw_fd(),operation)}==0 { return Ok(Some(file)); }
        let error=std::io::Error::last_os_error();
        if error.kind()==std::io::ErrorKind::Interrupted { continue; }
        if error.kind()==std::io::ErrorKind::WouldBlock { return Ok(None); }
        return Err(invalid());
    }
}
#[cfg(not(any(windows,target_os="linux",target_os="macos")))]
fn open(_:&Path,_:Mode)->ApiResult<Option<fs::File>> { Err(invalid()) }

#[cfg(test)]
mod tests {
    use super::*;
    use std::{process::{Child,Command,Stdio},time::{Duration,Instant}};
    #[test]
    fn legacy_locks_and_budget_records_cannot_block_or_mutate_the_new_protocol() {
        let root=tempfile::tempdir().unwrap();
        let old=root.path().join("capabilities/.locks");
        fs::create_dir_all(&old).unwrap();
        let old_resource=old.join("fonts.lock");
        let legacy_guard=open(&old_resource,Mode::Exclusive).unwrap().unwrap();
        let count=old.join("transfer-v1.count-0.lock");
        fs::write(&count,b"preserve unknown legacy record").unwrap();
        let current=try_acquire(root.path(),"fonts",Mode::Exclusive).unwrap().unwrap();
        assert!(try_acquire(root.path(),"fonts",Mode::Shared).unwrap().is_none());
        let transfer=reserve_transfer(root.path(),crate::capability_archive::MAX_DOWNLOAD).unwrap();
        assert_eq!(fs::read(&count).unwrap(),b"preserve unknown legacy record");
        assert!(root.path().join("capabilities/managed-v2/.locks/fonts.lock").is_file());
        drop((current,transfer,legacy_guard));
        assert!(try_acquire(root.path(),"fonts",Mode::Shared).unwrap().is_some());
    }
    struct OwnedChild(Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) { let _=self.0.kill();let _=self.0.wait(); }
    }
    #[test]
    fn transfer_allocations_bound_count_and_bytes_and_release_partial_failures() {
        let root=tempfile::tempdir().unwrap();
        for size in [0,crate::capability_archive::MAX_DOWNLOAD+1] {
            assert_eq!(reserve_transfer(root.path(),size).err().unwrap().code,"capability_pack_invalid");
        }
        assert!(!root.path().join("capabilities").exists());
        let all=reserve_transfer(root.path(),crate::capability_archive::MAX_DOWNLOAD).unwrap();
        assert_eq!(reserve_transfer(root.path(),1).err().unwrap().code,"capability_install_busy");
        // Engine execution and another explicit profile have independent limits.
        let engine=try_worker_slot(root.path(),"latex").unwrap().unwrap();drop(engine);
        let other=tempfile::tempdir().unwrap();
        drop(reserve_transfer(other.path(),1).unwrap());
        drop(all);
        let almost=reserve_transfer(root.path(),crate::capability_archive::MAX_DOWNLOAD-TRANSFER_QUANTUM+1).unwrap();
        assert_eq!(reserve_transfer(root.path(),1).err().unwrap().code,"capability_install_busy");
        drop(almost);
        let mut small=Vec::new();
        for _ in 0..TRANSFER_SLOTS { small.push(reserve_transfer(root.path(),1).unwrap()); }
        assert_eq!(reserve_transfer(root.path(),1).err().unwrap().code,"capability_install_busy");
        small.pop();small.push(reserve_transfer(root.path(),1).unwrap());drop(small);
        drop(reserve_transfer(root.path(),crate::capability_archive::MAX_DOWNLOAD).unwrap());
    }
    #[test]
    fn transfer_allocations_preserve_corrupt_slots_and_reject_invalid_keys() {
        let root=tempfile::tempdir().unwrap();
        for key in [Key::TransferCount(TRANSFER_SLOTS),Key::TransferBytes(TRANSFER_BYTES_SLOTS)] {
            assert!(try_key(root.path(),"transfers",key,Mode::Exclusive).is_err());
        }
        assert!(try_acquire(root.path(),"transfers",Mode::Exclusive).is_err());
        assert!(!root.path().join("capabilities").exists());
        drop(reserve_transfer(root.path(),1).unwrap());
        let bytes=root.path().join("capabilities/managed-v2/.locks/transfer-v1.bytes-0.lock");
        fs::write(&bytes,b"Unrecognized data must survive").unwrap();
        assert_eq!(reserve_transfer(root.path(),1).err().unwrap().code,"capability_pack_invalid");
        assert_eq!(fs::read(&bytes).unwrap(),b"Unrecognized data must survive");
        // Only this fixture repairs its corrupted empty slot. The product never
        // truncates a lock to make capacity appear available.
        fs::write(&bytes,b"").unwrap();
        drop(reserve_transfer(root.path(),crate::capability_archive::MAX_DOWNLOAD).unwrap());
        let gate=root.path().join("capabilities/managed-v2/.locks/transfer-v1.gate.lock");
        fs::write(&gate,b"Unknown gate").unwrap();
        assert_eq!(reserve_transfer(root.path(),1).err().unwrap().code,"capability_pack_invalid");
        assert_eq!(fs::read(gate).unwrap(),b"Unknown gate");
    }
    #[test]
    fn cross_process_transfer_count_bytes_exit_and_handle_inheritance() {
        if let Some(root)=std::env::var_os("READMD_TRANSFER_SLOT_ROOT") {
            let root=PathBuf::from(root);
            let name=std::env::var("READMD_TRANSFER_SLOT_NAME").unwrap();
            assert!(matches!(name.as_str(),"first"|"second"|"idle"));
            let _reservation=if name=="idle" { None } else {
                Some(reserve_transfer(&root,crate::capability_archive::MAX_DOWNLOAD/2).unwrap())
            };
            fs::write(root.join(format!("{name}.ready")),b"ready").unwrap();
            let until=Instant::now()+Duration::from_secs(20);
            while !root.join(format!("{name}.stop")).is_file() {
                assert!(Instant::now()<until,"Parent did not release transfer child");
                std::thread::sleep(Duration::from_millis(10));
            }
            return;
        }
        fn holder(root:&Path,name:&str)->OwnedChild {
            let mut child=OwnedChild(Command::new(std::env::current_exe().unwrap())
                .args(["--exact","capability_process::tests::cross_process_transfer_count_bytes_exit_and_handle_inheritance","--test-threads=1"])
                .env("READMD_TRANSFER_SLOT_ROOT",root).env("READMD_TRANSFER_SLOT_NAME",name)
                .stdout(Stdio::null()).stderr(Stdio::inherit()).spawn().unwrap());
            let until=Instant::now()+Duration::from_secs(20);
            while !root.join(format!("{name}.ready")).is_file() {
                if let Some(status)=child.0.try_wait().unwrap() { panic!("Transfer child exited: {status}"); }
                assert!(Instant::now()<until,"Transfer child not ready");
                std::thread::sleep(Duration::from_millis(10));
            }
            child
        }
        let root=tempfile::tempdir().unwrap();
        let mut first=holder(root.path(),"first");let mut second=holder(root.path(),"second");
        assert_eq!(reserve_transfer(root.path(),1).err().unwrap().code,"capability_install_busy");
        first.0.kill().unwrap();assert!(!first.0.wait().unwrap().success());
        let replacement=reserve_transfer(root.path(),crate::capability_archive::MAX_DOWNLOAD/2).unwrap();
        assert_eq!(reserve_transfer(root.path(),1).err().unwrap().code,"capability_install_busy");
        let mut idle=holder(root.path(),"idle");drop(replacement);
        let replacement=reserve_transfer(root.path(),crate::capability_archive::MAX_DOWNLOAD/2).unwrap();
        assert_eq!(reserve_transfer(root.path(),1).err().unwrap().code,"capability_install_busy");
        drop(replacement);
        fs::write(root.path().join("idle.stop"),b"stop").unwrap();assert!(idle.0.wait().unwrap().success());
        fs::write(root.path().join("second.stop"),b"stop").unwrap();assert!(second.0.wait().unwrap().success());
        drop(reserve_transfer(root.path(),crate::capability_archive::MAX_DOWNLOAD).unwrap());
        for entry in fs::read_dir(root.path().join("capabilities/managed-v2/.locks")).unwrap() {
            assert_eq!(entry.unwrap().metadata().unwrap().len(),0);
        }
    }
    fn child(root:&Path,mode:&str,name:&str)->OwnedChild {
        let mut child=OwnedChild(Command::new(std::env::current_exe().unwrap())
            .args(["--exact","capability_process::tests::cross_process_readers_writers_exit_and_handle_inheritance","--test-threads=1"])
            .env("READMD_CAP_LOCK_TEST_ROOT",root).env("READMD_CAP_LOCK_TEST_MODE",mode)
            .env("READMD_CAP_LOCK_TEST_NAME",name).stdout(Stdio::null()).stderr(Stdio::inherit()).spawn().unwrap());
        let until=Instant::now()+Duration::from_secs(20);
        while !root.join(format!("{name}.ready")).is_file() {
            if let Some(status)=child.0.try_wait().unwrap() { panic!("Lock child exited before ready: {status}"); }
            assert!(Instant::now()<until,"Lock child did not become ready");
            std::thread::sleep(Duration::from_millis(10));
        }
        child
    }
    #[test]
    fn capacity_check_is_non_creating_and_rejects_existing_full_or_corrupt_slots() {
        let root=tempfile::tempdir().unwrap();
        for id in ["latex","typst","ocr"] { check_worker_capacity(root.path(),id).unwrap(); }
        check_worker_capacity(&root.path().join("missing-profile"),"latex").unwrap();
        assert_eq!(fs::read_dir(root.path()).unwrap().count(),0);
        let first=try_worker_slot(root.path(),"latex").unwrap().unwrap();
        check_worker_capacity(root.path(),"latex").unwrap();
        assert!(!root.path().join("capabilities/managed-v2/.locks/latex.worker-1.lock").exists());
        let second=try_worker_slot(root.path(),"latex").unwrap().unwrap();
        assert_eq!(check_worker_capacity(root.path(),"latex").unwrap_err().code,"latex_busy");
        check_worker_capacity(root.path(),"ocr").unwrap();
        drop((first,second));
        let slot=root.path().join("capabilities/managed-v2/.locks/latex.worker-0.lock");
        fs::write(&slot,b"Unknown slot contents").unwrap();
        assert_eq!(check_worker_capacity(root.path(),"latex").unwrap_err().code,"capability_pack_invalid");
        assert_eq!(fs::read(&slot).unwrap(),b"Unknown slot contents");
        assert!(check_worker_capacity(root.path(),"../outside").is_err());
    }
    #[test]
    fn full_external_slots_reject_managed_tex_before_loading_a_manifest_or_creating_task_files() {
        let root=tempfile::tempdir().unwrap();
        let first=try_worker_slot(root.path(),"latex").unwrap().unwrap();
        let second=try_worker_slot(root.path(),"latex").unwrap().unwrap();
        let task=root.path().join("must-remain-absent");
        let assets=root.path().join("missing-assets");
        let result=crate::latex_compile::compile_managed("valid input",&task,"pdflatex",Duration::from_secs(30),root.path(),&assets);
        assert_eq!(result.unwrap_err(),"latex_busy");
        assert!(!task.exists()&&!assets.exists());
        crate::speech::with_cancel(std::sync::Arc::new(||true),|| {
            assert_eq!(crate::latex_compile::compile_managed("valid input",&task,"pdflatex",Duration::from_secs(30),root.path(),&assets).unwrap_err(),"cancelled");
        });
        drop((first,second));
        assert_eq!(crate::latex_compile::compile_managed("valid input",&task,"pdflatex",Duration::from_secs(30),root.path(),&assets).unwrap_err(),"latex_engine_unavailable");
        assert!(!task.exists()&&!assets.exists());
    }
    #[test]
    fn cross_process_readers_writers_exit_and_handle_inheritance() {
        if let Some(root)=std::env::var_os("READMD_CAP_LOCK_TEST_ROOT") {
            let root=PathBuf::from(root);
            let name=std::env::var("READMD_CAP_LOCK_TEST_NAME").unwrap();
            assert!(matches!(name.as_str(),"reader"|"writer"|"idle"));
            let mode=std::env::var("READMD_CAP_LOCK_TEST_MODE").unwrap();
            let _guard=match mode.as_str() {
                "shared"=>Some(try_acquire(&root,"fonts",Mode::Shared).unwrap().unwrap()),
                "exclusive"=>Some(try_acquire(&root,"fonts",Mode::Exclusive).unwrap().unwrap()),
                "idle"=>None,
                _=>panic!("Invalid test mode"),
            };
            fs::write(root.join(format!("{name}.ready")),b"ready").unwrap();
            let until=Instant::now()+Duration::from_secs(20);
            while !root.join(format!("{name}.stop")).is_file() {
                assert!(Instant::now()<until,"Parent did not release child");
                std::thread::sleep(Duration::from_millis(10));
            }
            return;
        }
        let root=tempfile::tempdir().unwrap();
        let shared=try_acquire(root.path(),"fonts",Mode::Shared).unwrap().unwrap();
        let mut reader=child(root.path(),"shared","reader");
        assert!(try_acquire(root.path(),"fonts",Mode::Exclusive).unwrap().is_none());
        let second=try_acquire(root.path(),"fonts",Mode::Shared).unwrap().unwrap();drop(second);drop(shared);
        assert!(try_acquire(root.path(),"fonts",Mode::Exclusive).unwrap().is_none());
        reader.0.kill().unwrap();assert!(!reader.0.wait().unwrap().success());drop(reader);
        let exclusive=try_acquire(root.path(),"fonts",Mode::Exclusive).unwrap().unwrap();drop(exclusive);
        let mut writer=child(root.path(),"exclusive","writer");
        assert!(try_acquire(root.path(),"fonts",Mode::Shared).unwrap().is_none());
        assert!(try_acquire(root.path(),"fonts",Mode::Exclusive).unwrap().is_none());
        fs::write(root.path().join("writer.stop"),b"stop").unwrap();assert!(writer.0.wait().unwrap().success());drop(writer);
        let shared=try_acquire(root.path(),"fonts",Mode::Shared).unwrap().unwrap();
        let mut idle=child(root.path(),"idle","idle");drop(shared);
        // A still-running child must not inherit the parent's pack lease.
        let exclusive=try_acquire(root.path(),"fonts",Mode::Exclusive).unwrap().unwrap();drop(exclusive);
        fs::write(root.path().join("idle.stop"),b"stop").unwrap();assert!(idle.0.wait().unwrap().success());
        let other=try_acquire(root.path(),"ocr",Mode::Exclusive).unwrap().unwrap();
        let fonts=try_acquire(root.path(),"fonts",Mode::Shared).unwrap().unwrap();drop((fonts,other));
    }
    #[test]
    fn failed_global_reservations_release_the_local_counter() {
        static ACTIVE:AtomicUsize=AtomicUsize::new(0);
        let root=tempfile::tempdir().unwrap();
        let first=try_worker_slot(root.path(),"typst").unwrap().unwrap();
        let second=try_worker_slot(root.path(),"typst").unwrap().unwrap();
        let busy=reserve_worker(root.path(),"typst",&ACTIVE).err().unwrap();
        assert_eq!(busy.code,"typst_busy");assert_eq!(ACTIVE.load(Ordering::Acquire),0);
        drop((first,second));
        let reserved=reserve_worker(root.path(),"typst",&ACTIVE).unwrap();
        assert_eq!(ACTIVE.load(Ordering::Acquire),1);drop(reserved);
        assert_eq!(ACTIVE.load(Ordering::Acquire),0);
        let slot=root.path().join("capabilities/managed-v2/.locks/typst.worker-0.lock");
        fs::write(&slot,b"Unknown content").unwrap();
        assert_eq!(reserve_worker(root.path(),"typst",&ACTIVE).err().unwrap().code,"capability_pack_invalid");
        assert_eq!(ACTIVE.load(Ordering::Acquire),0);
        assert_eq!(fs::read(slot).unwrap(),b"Unknown content");
    }
    #[test]
    fn cross_process_worker_budget_releases_only_owner_slots() {
        if let Some(root)=std::env::var_os("READMD_WORKER_SLOT_ROOT") {
            let root=PathBuf::from(root);
            let name=std::env::var("READMD_WORKER_SLOT_NAME").unwrap();
            assert!(matches!(name.as_str(),"first"|"second"|"idle"));
            let _resource=if name=="idle" { None } else {
                Some(try_acquire(&root,"latex",Mode::Shared).unwrap().unwrap())
            };
            let _slot=if name=="idle" { None } else {
                Some(try_worker_slot(&root,"latex").unwrap().unwrap())
            };
            fs::write(root.join(format!("{name}.ready")),b"ready").unwrap();
            let until=Instant::now()+Duration::from_secs(20);
            while !root.join(format!("{name}.stop")).is_file() {
                assert!(Instant::now()<until,"Parent did not release slot child");
                std::thread::sleep(Duration::from_millis(10));
            }
            return;
        }
        fn worker(root:&Path,name:&str)->OwnedChild {
            let mut child=OwnedChild(Command::new(std::env::current_exe().unwrap())
                .args(["--exact","capability_process::tests::cross_process_worker_budget_releases_only_owner_slots","--test-threads=1"])
                .env("READMD_WORKER_SLOT_ROOT",root).env("READMD_WORKER_SLOT_NAME",name)
                .stdout(Stdio::null()).stderr(Stdio::inherit()).spawn().unwrap());
            let until=Instant::now()+Duration::from_secs(20);
            while !root.join(format!("{name}.ready")).is_file() {
                if let Some(status)=child.0.try_wait().unwrap() { panic!("Slot child exited: {status}"); }
                assert!(Instant::now()<until,"Slot child did not become ready");
                std::thread::sleep(Duration::from_millis(10));
            }
            child
        }
        let root=tempfile::tempdir().unwrap();
        let mut first=worker(root.path(),"first");
        let mut second=worker(root.path(),"second");
        assert_eq!(check_worker_capacity(root.path(),"latex").unwrap_err().code,"latex_busy");
        assert!(try_worker_slot(root.path(),"latex").unwrap().is_none());
        assert!(try_acquire(root.path(),"latex",Mode::Exclusive).unwrap().is_none());
        let other_engine=try_worker_slot(root.path(),"ocr").unwrap().unwrap();
        let other_profile=tempfile::tempdir().unwrap();
        let other_slot=try_worker_slot(other_profile.path(),"latex").unwrap().unwrap();
        drop((other_engine,other_slot));
        first.0.kill().unwrap();assert!(!first.0.wait().unwrap().success());drop(first);
        check_worker_capacity(root.path(),"latex").unwrap();
        let replacement=try_worker_slot(root.path(),"latex").unwrap().unwrap();
        assert!(try_worker_slot(root.path(),"latex").unwrap().is_none());
        // A child started while the parent owns a slot must not inherit it.
        let mut idle=worker(root.path(),"idle");drop(replacement);
        let replacement=try_worker_slot(root.path(),"latex").unwrap().unwrap();
        assert!(try_worker_slot(root.path(),"latex").unwrap().is_none());
        drop(replacement);
        fs::write(root.path().join("idle.stop"),b"stop").unwrap();assert!(idle.0.wait().unwrap().success());
        fs::write(root.path().join("second.stop"),b"stop").unwrap();assert!(second.0.wait().unwrap().success());drop(second);
        let first=try_worker_slot(root.path(),"latex").unwrap().unwrap();
        let second=try_worker_slot(root.path(),"latex").unwrap().unwrap();
        assert!(try_worker_slot(root.path(),"latex").unwrap().is_none());drop((first,second));
        for index in 0..WORKER_SLOTS {
            assert_eq!(fs::metadata(root.path().join(format!("capabilities/managed-v2/.locks/latex.worker-{index}.lock"))).unwrap().len(),0);
        }
        assert!(try_acquire(root.path(),"latex",Mode::Exclusive).unwrap().is_some());
    }
    #[test]
    fn worker_slots_fail_closed_for_unknown_engines_and_corrupt_files() {
        let root=tempfile::tempdir().unwrap();
        for id in ["fonts","pdfium","../outside","latex.worker-2"] {
            assert!(try_worker_slot(root.path(),id).is_err());
        }
        assert_eq!(fs::read_dir(root.path()).unwrap().count(),0);
        fs::create_dir_all(root.path().join("capabilities/managed-v2/.locks")).unwrap();
        let file=root.path().join("capabilities/managed-v2/.locks/latex.worker-0.lock");
        fs::write(&file,b"Preserve unknown user bytes").unwrap();
        assert!(try_worker_slot(root.path(),"latex").is_err());
        assert_eq!(fs::read(&file).unwrap(),b"Preserve unknown user bytes");
        assert!(!root.path().join("capabilities/managed-v2/.locks/latex.worker-1.lock").exists());
    }
    #[test]
    fn invalid_names_and_nonempty_or_redirected_paths_are_not_overwritten() {
        let root=tempfile::tempdir().unwrap();
        assert!(try_acquire(root.path(),"../outside",Mode::Exclusive).is_err());
        assert!(!root.path().join("capabilities").exists());
        fs::create_dir_all(root.path().join("capabilities/managed-v2/.locks")).unwrap();
        let file=root.path().join("capabilities/managed-v2/.locks/fonts.lock");fs::write(&file,b"Synthetic user data").unwrap();
        assert!(try_acquire(root.path(),"fonts",Mode::Exclusive).is_err());
        assert_eq!(fs::read(&file).unwrap(),b"Synthetic user data");
        fs::remove_file(&file).unwrap();fs::create_dir(&file).unwrap();
        assert!(try_acquire(root.path(),"fonts",Mode::Shared).is_err());
        #[cfg(unix)] {
            fs::remove_dir(&file).unwrap();
            let outside=root.path().join("outside");fs::write(&outside,b"Synthetic outside").unwrap();
            std::os::unix::fs::symlink(&outside,&file).unwrap();
            assert!(try_acquire(root.path(),"fonts",Mode::Shared).is_err());
            assert_eq!(fs::read(outside).unwrap(),b"Synthetic outside");
        }
    }
}
