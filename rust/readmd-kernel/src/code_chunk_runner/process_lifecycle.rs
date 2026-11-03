//! Request-owned process lifetime. No ambient process scans or model control.
use super::*;
use std::io;

pub(super) struct OwnedChild {
    #[cfg(windows)]
    inner: crate::worker_sandbox::ManagedWorker,
    #[cfg(not(windows))]
    inner: Child,
    pub stdout: Option<Box<dyn Read + Send>>,
    pub stderr: Option<Box<dyn Read + Send>>,
    stopped: bool,
    stop_deadline: Option<Instant>,
}
impl OwnedChild {
    pub fn spawn(mut cmd: Command, cwd: Option<&Path>) -> io::Result<Self> {
        #[cfg(windows)]
        {
            use std::{fs::File, os::windows::io::OwnedHandle};
            let (stdout, output) = std::io::pipe()?;
            let (stderr, errors) = std::io::pipe()?;
            let output = File::from(OwnedHandle::from(output));
            let errors = File::from(OwnedHandle::from(errors));
            let program = Path::new(cmd.get_program());
            if !program.is_absolute() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "Absolute code runtime required",
                ));
            }
            let directory = cwd
                .map(Path::to_path_buf)
                .unwrap_or(std::env::current_dir()?);
            let inner =
                crate::worker_sandbox::spawn_managed(&mut cmd, &directory, &output, &errors)?;
            Ok(Self {
                inner,
                stdout: Some(Box::new(stdout)),
                stderr: Some(Box::new(stderr)),
                stopped: false,
                stop_deadline: None,
            })
        }
        #[cfg(not(windows))]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
            let mut inner = cmd.spawn()?;
            let stdout = inner
                .stdout
                .take()
                .map(|p| Box::new(p) as Box<dyn Read + Send>);
            let stderr = inner
                .stderr
                .take()
                .map(|p| Box::new(p) as Box<dyn Read + Send>);
            Ok(Self {
                inner,
                stdout,
                stderr,
                stopped: false,
                stop_deadline: None,
            })
        }
    }
    pub fn try_wait(&mut self) -> io::Result<Option<i64>> {
        #[cfg(windows)]
        {
            self.inner.try_wait().map(|v| v.map(i64::from))
        }
        #[cfg(not(windows))]
        {
            // Keep the exited root as a waitable child until its owned group
            // is signalled. Reaping it first could allow PID/group-ID reuse.
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            loop {
                let result = unsafe {
                    libc::waitid(
                        libc::P_PID,
                        self.inner.id(),
                        &mut info,
                        libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                    )
                };
                if result == 0 {
                    break;
                }
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::Interrupted {
                    return Err(error);
                }
            }
            if unsafe { info.si_pid() } == 0 {
                return Ok(None);
            }
            Ok(Some(if info.si_code == libc::CLD_EXITED {
                (unsafe { info.si_status() }) as i64
            } else {
                -1
            }))
        }
    }
    pub fn stop(&mut self) -> io::Result<()> {
        if self.stopped {
            return Ok(());
        }
        #[cfg(windows)]
        {
            self.inner.stop()?;
        }
        #[cfg(not(windows))]
        {
            let deadline = *self
                .stop_deadline
                .get_or_insert_with(|| Instant::now() + Duration::from_secs(2));
            let pid = i32::try_from(self.inner.id())
                .map_err(|_| io::Error::other("Invalid owned child ID"))?;
            if pid <= 0 {
                return Err(io::Error::other("Invalid owned process group"));
            }
            if unsafe { libc::kill(-pid, libc::SIGKILL) } != 0 {
                let error = io::Error::last_os_error();
                #[cfg(target_os = "macos")]
                let exited_only = error.raw_os_error() == Some(libc::EPERM)
                    && self.try_wait()?.is_some()
                    && mac_group_exited_only(pid)?;
                #[cfg(not(target_os = "macos"))]
                let exited_only = false;
                if error.raw_os_error() != Some(libc::ESRCH) && !exited_only {
                    return Err(error);
                }
            }
            loop {
                if self.inner.try_wait()?.is_some() {
                    break;
                }
                if Instant::now() >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "Owned process stop pending",
                    ));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        self.stopped = true;
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn mac_group_exited_only(root: i32) -> io::Result<bool> {
    // Darwin filters zombies out of killpg's signal targets and can return
    // EPERM for a group containing only our exited root. Confirm that precise
    // state; a live/unreadable member must still leave stop pending.
    let mut pids = [0i32; 4096];
    // Unlike proc_listpids, this wrapper returns a PID count, not bytes.
    let count = unsafe {
        libc::proc_listpgrppids(
            root,
            pids.as_mut_ptr().cast(),
            std::mem::size_of_val(&pids) as i32,
        )
    };
    if count <= 0 || count as usize >= pids.len() || !pids[..count as usize].contains(&root) {
        return Err(io::Error::other("Owned group observation failed"));
    }
    for pid in pids
        .iter()
        .take(count as usize)
        .copied()
        .filter(|p| *p > 0 && *p != root)
    {
        let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of_val(&info) as i32;
        let read = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTBSDINFO,
                0,
                (&mut info as *mut libc::proc_bsdinfo).cast(),
                size,
            )
        };
        if read != size || info.pbi_pgid != root as u32 || info.pbi_status != libc::SZOMB {
            return Ok(false);
        }
    }
    Ok(true)
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn exited_root_with_a_live_descendant_is_not_an_exited_only_group() {
        let mut cmd = command(&std::env::current_exe().unwrap().into_os_string());
        cmd.args([
            "--exact",
            "code_chunk_runner::lifecycle_tests::owned_descendant_root",
            "--ignored",
            "--nocapture",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        let mut child = OwnedChild::spawn(cmd, None).unwrap();
        let end = Instant::now() + Duration::from_secs(1);
        while child.try_wait().unwrap().is_none() {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            !mac_group_exited_only(child.inner.id() as i32).unwrap(),
            "the actual live descendant must be observed"
        );
        child.stop().unwrap();
    }
}
