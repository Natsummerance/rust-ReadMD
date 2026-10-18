//! OS-enforced TeX isolation and shared managed-worker lifecycle.
//! AppContainer execution fails closed outside Windows. The generic worker
//! preserves portable execution, with owner-crash job containment on Windows;
//! that lifecycle guarantee is not a filesystem or network sandbox.
#[cfg(windows)]
mod platform {
    use std::{
        ffi::{OsStr, OsString},
        fs::File,
        io,
        mem::{size_of, zeroed},
        os::windows::{ffi::OsStrExt, io::AsRawHandle},
        path::{Path, PathBuf},
        ptr,
        sync::Mutex,
        time::Duration,
    };
    use windows::{
        core::{BOOL, PCWSTR, PWSTR},
        Win32::{
            Foundation::*,
            Security::{Authorization::*, Isolation::*, *},
            Storage::FileSystem::*,
            System::{JobObjects::*, Threading::*, IO::DeviceIoControl},
        },
    };
    static RESOURCE_ACL: Mutex<()> = Mutex::new(());
    fn wide(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain(Some(0)).collect()
    }
    fn win<T>(value: windows::core::Result<T>) -> io::Result<T> {
        value.map_err(|e| io::Error::from_raw_os_error(e.code().0 & 0xffff))
    }
    fn stage<T>(value: io::Result<T>, name: &str) -> io::Result<T> {
        value.map_err(|e| io::Error::new(e.kind(), format!("{name}: {e}")))
    }
    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
    struct Local(*mut core::ffi::c_void);
    impl Drop for Local {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    LocalFree(Some(HLOCAL(self.0)));
                }
            }
        }
    }
    struct Sid(PSID);
    impl Drop for Sid {
        fn drop(&mut self) {
            unsafe {
                FreeSid(self.0);
            }
        }
    }
    struct Profile {
        name: Vec<u16>,
        sid: Sid,
    }
    // CreateProcess cannot use a >MAX_PATH cwd even with a verbatim path.
    // A task-local junction gives legacy readers a short cwd without copying
    // models or creating logon-session DOS mappings. Teardown is leaf-only.
    struct WorkingDirectory {
        parent: PathBuf,
        link: PathBuf,
    }
    impl WorkingDirectory {
        fn create(target: &Path, scratch: &Path) -> io::Result<Self> {
            let parent = scratch.join(format!("current-dir-{}", uuid::Uuid::new_v4()));
            Self::create_at(target, parent)
        }
        fn create_at(target: &Path, parent: PathBuf) -> io::Result<Self> {
            let link = parent.join("resources");
            if link.as_os_str().encode_wide().count() >= 248 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Task scratch requires a short native path",
                ));
            }
            crate::task_workspace::create_dir(&parent)?;
            let owned = Self { parent, link };
            std::fs::create_dir(&owned.link)?;
            let canonical = target.as_os_str().encode_wide().collect::<Vec<_>>();
            if !canonical.starts_with(&[92, 92, 63, 92]) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Canonical resource directory required",
                ));
            }
            let mut substitute = vec![92u16, 63, 63, 92];
            substitute.extend_from_slice(&canonical[4..]);
            let name_bytes = substitute
                .len()
                .checked_mul(2)
                .filter(|n| *n <= 16364)
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "Resource view path limit")
                })?;
            // MOUNT_POINT reparse data: substitute name, NUL, empty print name.
            let data_length = 8 + name_bytes + 4;
            let mut data = Vec::with_capacity(8 + data_length);
            data.extend_from_slice(&0xa0000003u32.to_le_bytes());
            data.extend_from_slice(&(data_length as u16).to_le_bytes());
            data.extend_from_slice(&0u16.to_le_bytes());
            data.extend_from_slice(&0u16.to_le_bytes());
            data.extend_from_slice(&(name_bytes as u16).to_le_bytes());
            data.extend_from_slice(&((name_bytes + 2) as u16).to_le_bytes());
            data.extend_from_slice(&0u16.to_le_bytes());
            for word in substitute {
                data.extend_from_slice(&word.to_le_bytes());
            }
            data.extend_from_slice(&[0, 0, 0, 0]);
            let name = wide(owned.link.as_os_str());
            unsafe {
                let handle = Handle(win(CreateFileW(
                    PCWSTR(name.as_ptr()),
                    0x40000000,
                    FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                    None,
                    OPEN_EXISTING,
                    FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                    None,
                ))?);
                let mut returned = 0;
                win(DeviceIoControl(
                    handle.0,
                    0x000900a4,
                    Some(data.as_ptr().cast()),
                    data.len() as u32,
                    None,
                    0,
                    Some(&mut returned),
                    None,
                ))?;
            }
            crate::task_workspace::track_link(&owned.link, target)?;
            Ok(owned)
        }
    }
    impl Drop for WorkingDirectory {
        fn drop(&mut self) {
            // RemoveDirectory removes the junction only. Never enumerate or
            // recursively delete its target, including on launch failure.
            let _ = std::fs::remove_dir(&self.link);
            let _ = std::fs::remove_dir(&self.parent);
        }
    }
    impl Profile {
        fn create(task: Option<&crate::task_workspace::Workspace>) -> io::Result<Self> {
            let label = task.map_or_else(
                || format!("ReadMD.Worker.{}", uuid::Uuid::new_v4()),
                |task| format!("ReadMD.Worker.{}.profile", task.id()),
            );
            let name = wide(OsStr::new(&label));
            let sid = unsafe {
                stage(
                    win(CreateAppContainerProfile(
                        PCWSTR(name.as_ptr()),
                        PCWSTR(name.as_ptr()),
                        windows::core::w!("ReadMD isolated document worker"),
                        None,
                    )),
                    "container profile",
                )?
            };
            let owned = Self {
                name,
                sid: Sid(sid),
            };
            if let Some(task) = task {
                task.register_native_resource(crate::task_workspace::NativeResource::AppContainer {
                    name: label,
                })?;
            }
            Ok(owned)
        }
    }
    impl Drop for Profile {
        fn drop(&mut self) {
            unsafe {
                let _ = DeleteAppContainerProfile(PCWSTR(self.name.as_ptr()));
            }
        }
    }
    // Legacy native libraries enumerate every parent while expanding their own
    // executable path. A task-local DOS-device name gives them a short view of
    // exactly the two granted roots, without granting access to private parents.
    // This is a unique non-drive-letter name in the current logon session, with
    // no Explorer broadcast. Remove only this exact definition on final drop.
    struct View {
        name: Vec<u16>,
        target: Vec<u16>,
        path: PathBuf,
    }
    impl View {
        fn new(root: &Path, task: Option<(&crate::task_workspace::Workspace, &str)>) -> io::Result<Self> {
            let label = task.map_or_else(
                || format!("ReadMD.Worker.{}", uuid::Uuid::new_v4()),
                |(task, role)| format!("ReadMD.Worker.{}.{role}", task.id()),
            );
            let canonical = root.as_os_str().encode_wide().collect::<Vec<_>>();
            if !canonical.starts_with(&[92, 92, 63, 92]) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Canonical worker root required",
                ));
            }
            let mut target = vec![92, 63, 63, 92];
            target.extend_from_slice(&canonical[4..]);
            target.push(0);
            let path = PathBuf::from(format!("\\\\.\\{label}\\"));
            let name = wide(OsStr::new(&label));
            unsafe {
                win(DefineDosDeviceW(
                    DDD_RAW_TARGET_PATH | DDD_NO_BROADCAST_SYSTEM,
                    PCWSTR(name.as_ptr()),
                    PCWSTR(target.as_ptr()),
                ))?;
            }
            let owned = Self { name, target, path };
            if let Some((task, _)) = task {
                task.register_native_resource(crate::task_workspace::NativeResource::DosDevice {
                    name: label,
                    target: owned.target[..owned.target.len()-1].to_vec(),
                })?;
            }
            Ok(owned)
        }
    }
    impl Drop for View {
        fn drop(&mut self) {
            unsafe {
                let _ = DefineDosDeviceW(
                    DDD_RAW_TARGET_PATH
                        | DDD_NO_BROADCAST_SYSTEM
                        | DDD_REMOVE_DEFINITION
                        | DDD_EXACT_MATCH_ON_REMOVE,
                    PCWSTR(self.name.as_ptr()),
                    PCWSTR(self.target.as_ptr()),
                );
            }
        }
    }
    struct Binding {
        runtime: View,
        workspace: View,
        profile: Profile,
        resource_directory: Option<WorkingDirectory>,
        // Drop the persisted task last, after ordinary resource teardown. The
        // worker's Arc keeps it alive; its OS lease is inherited at creation.
        task: Option<crate::task_workspace::Workspace>,
    }
    pub(super) fn recover_native_resource(resource: &crate::task_workspace::NativeResource) -> io::Result<bool> {
        use crate::task_workspace::NativeResource;
        match resource {
            NativeResource::AppContainer { name } => {
                let name = wide(OsStr::new(name));
                // The API is idempotent for an absent per-user profile.
                unsafe { win(DeleteAppContainerProfile(PCWSTR(name.as_ptr())))?; }
            }
            NativeResource::DosDevice { name, target } => {
                let name = wide(OsStr::new(name));
                let mut values = vec![0u16; 65_536];
                let count = unsafe { QueryDosDeviceW(PCWSTR(name.as_ptr()), Some(&mut values)) };
                if count == 0 {
                    let error = io::Error::last_os_error();
                    if error.kind() == io::ErrorKind::NotFound { return Ok(true); }
                    return Err(error);
                }
                // Query may contain stacked definitions. Remove only our exact
                // target; leave every replacement/other owner's definition.
                if values[..count as usize].split(|v| *v == 0).any(|v| v == target) {
                    let mut target = target.clone();
                    target.push(0);
                    unsafe {
                        win(DefineDosDeviceW(
                            DDD_RAW_TARGET_PATH | DDD_NO_BROADCAST_SYSTEM
                                | DDD_REMOVE_DEFINITION | DDD_EXACT_MATCH_ON_REMOVE,
                            PCWSTR(name.as_ptr()), PCWSTR(target.as_ptr()),
                        ))?;
                    }
                }
            }
        }
        Ok(true)
    }
    fn grant(directory: &Path, sid: PSID, rights: u32) -> io::Result<()> {
        let name = wide(directory.as_os_str());
        let mut old = ptr::null_mut();
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        unsafe {
            GetNamedSecurityInfoW(
                PCWSTR(name.as_ptr()),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                None,
                None,
                Some(&mut old),
                None,
                &mut descriptor,
            )
            .ok()?;
            let _descriptor = Local(descriptor.0);
            let mut new = ptr::null_mut();
            let access = EXPLICIT_ACCESS_W {
                grfAccessPermissions: rights,
                grfAccessMode: GRANT_ACCESS,
                grfInheritance: OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE,
                Trustee: TRUSTEE_W {
                    TrusteeForm: TRUSTEE_IS_SID,
                    TrusteeType: TRUSTEE_IS_UNKNOWN,
                    ptstrName: PWSTR(sid.0.cast()),
                    ..Default::default()
                },
            };
            SetEntriesInAclW(Some(&[access]), Some(old), &mut new).ok()?;
            let _acl = Local(new.cast());
            SetNamedSecurityInfoW(
                PCWSTR(name.as_ptr()),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                None,
                None,
                Some(new),
                None,
            )
            .ok()?;
        }
        Ok(())
    }
    fn low_integrity(directory: &Path) -> io::Result<()> {
        let name = wide(directory.as_os_str());
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        unsafe {
            win(ConvertStringSecurityDescriptorToSecurityDescriptorW(
                windows::core::w!("S:(ML;OICI;NW;;;LW)"),
                1,
                &mut descriptor,
                None,
            ))?;
            let _descriptor = Local(descriptor.0);
            let mut acl = ptr::null_mut();
            let (mut present, mut defaulted) = (BOOL(0), BOOL(0));
            win(GetSecurityDescriptorSacl(
                descriptor,
                &mut present,
                &mut acl,
                &mut defaulted,
            ))?;
            SetNamedSecurityInfoW(
                PCWSTR(name.as_ptr()),
                SE_FILE_OBJECT,
                LABEL_SECURITY_INFORMATION,
                None,
                None,
                None,
                Some(acl),
            )
            .ok()?;
        }
        Ok(())
    }
    fn resource_read_only(directory: &Path) -> io::Result<()> {
        let _lock = RESOURCE_ACL.lock().unwrap_or_else(|e| e.into_inner());
        let mut sid = PSID::default();
        unsafe {
            win(ConvertStringSidToSidW(
                windows::core::w!("S-1-15-2-1"),
                &mut sid,
            ))?;
        }
        let _sid = Local(sid.0);
        let rights = (FILE_GENERIC_READ | FILE_GENERIC_EXECUTE).0;
        // SetNamedSecurityInfo propagates an inheritable ACL through the whole
        // distribution. Do that once, never for every compiler pass/request.
        let name = wide(directory.as_os_str());
        let mut acl = ptr::null_mut();
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        unsafe {
            GetNamedSecurityInfoW(
                PCWSTR(name.as_ptr()),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                None,
                None,
                Some(&mut acl),
                None,
                &mut descriptor,
            )
            .ok()?;
            let _descriptor = Local(descriptor.0);
            if !acl.is_null() {
                for index in 0..(*acl).AceCount {
                    let mut entry = ptr::null_mut();
                    win(GetAce(acl, index as u32, &mut entry))?;
                    let header = &*entry.cast::<ACE_HEADER>();
                    if header.AceType == 0
                        && header.AceSize as usize >= size_of::<ACCESS_ALLOWED_ACE>()
                    {
                        let allowed = &*entry.cast::<ACCESS_ALLOWED_ACE>();
                        let entry_sid = PSID((&allowed.SidStart as *const u32).cast_mut().cast());
                        if EqualSid(entry_sid, sid).is_ok()
                            && allowed.Mask == rights
                            && header.AceFlags & 3 == 3
                        {
                            return Ok(());
                        }
                    }
                }
            }
        }
        grant(directory, sid, rights)
    }
    fn owned_directory(path: &Path) -> io::Result<PathBuf> {
        use std::os::windows::fs::MetadataExt;
        let metadata = std::fs::symlink_metadata(path)?;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 || !metadata.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Worker directory must be a real owned directory",
            ));
        }
        std::fs::canonicalize(path)
    }
    /// Set inheritable resource permissions before unpacking. Setting the
    /// same ACL after 35k files exist recursively rewrites their permissions
    /// and can block compilation/cancellation for minutes on Windows.
    pub fn prepare_empty_runtime(path: &Path) -> io::Result<()> {
        let path = owned_directory(path)?;
        if std::fs::read_dir(&path)?.next().is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Expected empty owned runtime",
            ));
        }
        resource_read_only(&path)
    }
    pub struct Sandbox {
        runtime: PathBuf,
        workspace: PathBuf,
        binding: std::sync::Arc<Binding>,
    }
    impl Sandbox {
        /// Callers supply only their managed pack and freshly owned task root.
        /// Runtime public resources become AppContainer-readable, never writable;
        /// task data is granted only to this unique per-task identity.
        pub fn new(runtime: &Path, workspace: &Path) -> io::Result<Self> {
            Self::new_inner(runtime, workspace, None)
        }
        pub(crate) fn new_for_task(runtime: &Path, workspace: &Path, task: crate::task_workspace::Workspace) -> io::Result<Self> {
            Self::new_inner(runtime, workspace, Some(task))
        }
        fn new_inner(runtime: &Path, workspace: &Path, task: Option<crate::task_workspace::Workspace>) -> io::Result<Self> {
            let runtime = owned_directory(runtime)?;
            let workspace = owned_directory(workspace)?;
            if runtime == workspace
                || workspace.starts_with(&runtime)
                || runtime.starts_with(&workspace)
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Runtime and task roots must be separate",
                ));
            }
            log::debug!(target:"readmd::tex", "container profile begins");
            let profile = Profile::create(task.as_ref())?;
            log::debug!(target:"readmd::tex", "resource ACL begins");
            stage(resource_read_only(&runtime), "runtime ACL")?;
            log::debug!(target:"readmd::tex", "task ACL begins");
            stage(
                grant(&workspace, profile.sid.0, FILE_ALL_ACCESS.0),
                "task ACL",
            )?;
            stage(low_integrity(&workspace), "task integrity")?;
            // TeX/fontconfig require an ordinary filesystem path for fonts;
            // canonical install/transaction paths can exceed their MAX_PATH
            // support. A recorded task-local junction gives them a short view
            // without copying the runtime or granting its private ancestors.
            let resource_directory = task.as_ref().map(|task| {
                grant(task.path(), profile.sid.0, (FILE_GENERIC_READ | FILE_GENERIC_EXECUTE).0)?;
                task.scope(|| WorkingDirectory::create_at(&runtime, task.path().join("tex-view")))
            }).transpose()?;
            log::debug!(target:"readmd::tex", "device views begin");
            let binding = std::sync::Arc::new(Binding {
                runtime: stage(View::new(&runtime, task.as_ref().map(|t| (t, "runtime"))), "runtime view")?,
                workspace: stage(View::new(&workspace, task.as_ref().map(|t| (t, "workspace"))), "task view")?,
                profile,
                resource_directory,
                task,
            });
            Ok(Self {
                runtime,
                workspace,
                binding,
            })
        }
        pub fn runtime_view(&self) -> &Path {
            &self.binding.runtime.path
        }
        pub fn workspace_view(&self) -> &Path {
            &self.binding.workspace.path
        }
        pub(crate) fn resource_view(&self) -> &Path {
            self.binding.resource_directory.as_ref()
                .map_or(self.runtime.as_path(), |view| dunce::simplified(&view.link))
        }
        /// Only the two Windows bootstrap locations are inherited. In particular,
        /// credentials, proxy settings, PATH and user TeX configuration are not.
        pub fn environment(&self) -> io::Result<Vec<(OsString, OsString)>> {
            let mut values = Vec::new();
            for key in ["SystemRoot", "LOCALAPPDATA"] {
                let value = std::env::var_os(key)
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::NotFound,
                            "Windows worker bootstrap environment is incomplete",
                        )
                    })?;
                values.push((key.into(), value));
            }
            for key in ["TEMP", "TMP", "HOME", "USERPROFILE"] {
                values.push((
                    key.into(),
                    dunce::simplified(&self.workspace).as_os_str().into(),
                ));
            }
            Ok(values)
        }
        pub fn spawn(
            &self,
            program: &Path,
            args: &[OsString],
            environment: &[(OsString, OsString)],
            capture: &File,
        ) -> io::Result<Worker> {
            let program = std::fs::canonicalize(program)?;
            if !program.starts_with(&self.runtime) || !program.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Worker program must belong to the managed runtime",
                ));
            }
            let launch = self
                .runtime_view()
                .join(program.strip_prefix(&self.runtime).map_err(|_| {
                    io::Error::new(io::ErrorKind::PermissionDenied, "Worker program boundary")
                })?);
            launch_worker(
                &launch,
                args,
                environment,
                &self.workspace_view(),
                capture,
                capture,
                None,
                Some(self.binding.clone()),
                self.binding.task.as_ref().map(|task| task.lease()),
            )
        }
    }
    pub(super) fn spawn_managed(
        command: &std::process::Command,
        directory: &Path,
        output: &File,
        errors: &File,
    ) -> io::Result<Worker> {
        spawn_managed_input(command, directory, output, errors, None)
    }
    pub(super) fn spawn_managed_input(
        command: &std::process::Command,
        directory: &Path,
        output: &File,
        output_errors: &File,
        input: Option<&File>,
    ) -> io::Result<Worker> {
        let requested = Path::new(command.get_program());
        if !requested.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Managed program requires an absolute path",
            ));
        }
        let program = std::fs::canonicalize(requested)?;
        let directory = std::fs::canonicalize(directory)?;
        if !program.is_file() || !directory.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid managed worker paths",
            ));
        }
        let environment = managed_environment(command, &program, &directory)?;
        // Explicit input permits a Unicode resource cwd; scratch/cache paths
        // remain in the task directory and no ambient stdin is inherited.
        let working = if input.is_some() {
            match command.get_current_dir() {
                Some(path) if path.is_absolute() => std::fs::canonicalize(path)?,
                Some(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Absolute resource directory required",
                    ))
                }
                None => directory.clone(),
            }
        } else {
            directory.clone()
        };
        if !working.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid resource directory",
            ));
        }
        let view = if input.is_some() && working.as_os_str().encode_wide().count() >= 248 {
            Some(WorkingDirectory::create(&working, &directory)?)
        } else {
            None
        };
        let cwd = view
            .as_ref()
            .map_or(working.as_path(), |view| view.link.as_path());
        let args = command
            .get_args()
            .map(OsStr::to_os_string)
            .collect::<Vec<_>>();
        let lease = crate::task_workspace::current_lease(&directory)?;
        let mut worker = launch_worker(
            &program,
            &args,
            &environment,
            cwd,
            output,
            output_errors,
            input,
            None,
            lease.as_ref(),
        )?;
        worker.working = view;
        Ok(worker)
    }
    fn managed_environment(
        command: &std::process::Command,
        program: &Path,
        directory: &Path,
    ) -> io::Result<Vec<(OsString, OsString)>> {
        let mut values = std::collections::BTreeMap::<String, OsString>::new();
        for key in ["SYSTEMROOT", "WINDIR"] {
            let value = std::env::var_os(key)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        "Windows worker bootstrap environment is incomplete",
                    )
                })?;
            values.insert(key.into(), value);
        }
        values.insert(
            "PATH".into(),
            program
                .parent()
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "Invalid worker program parent")
                })?
                .as_os_str()
                .into(),
        );
        for key in [
            "TEMP",
            "TMP",
            "HOME",
            "USERPROFILE",
            "APPDATA",
            "LOCALAPPDATA",
        ] {
            values.insert(key.into(), directory.as_os_str().into());
        }
        // Inherit no ambient secrets, user caches or unreviewed tool search
        // paths. Only explicit engine options may add to this bootstrap.
        for (key, value) in command.get_envs() {
            let key = key.to_string_lossy().to_uppercase();
            if matches!(
                key.as_str(),
                "SYSTEMROOT"
                    | "WINDIR"
                    | "PATH"
                    | "TEMP"
                    | "TMP"
                    | "HOME"
                    | "USERPROFILE"
                    | "APPDATA"
                    | "LOCALAPPDATA"
            ) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Managed worker bootstrap cannot be overridden",
                ));
            }
            match value {
                Some(value) => {
                    values.insert(key, value.into());
                }
                None => {
                    values.remove(&key);
                }
            }
        }
        Ok(values
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect())
    }
    // JOB_LIST binds the worker during CreateProcess, including the suspended
    // interval. Assigning after creation leaves an orphan if the owner dies
    // between the two calls. The job is unnamed and never inherited.
    fn launch_worker(
        program: &Path,
        args: &[OsString],
        environment: &[(OsString, OsString)],
        directory: &Path,
        output: &File,
        errors: &File,
        input: Option<&File>,
        binding: Option<std::sync::Arc<Binding>>,
        lease: Option<&File>,
    ) -> io::Result<Worker> {
        let app = wide(program.as_os_str());
        let cwd = wide(directory.as_os_str());
        let mut command = command_line(program.as_os_str(), args)?;
        let environment = environment_block(environment)?;
        unsafe {
            let mut size = 0;
            let count = if binding.is_some() { 3 } else { 2 };
            let _ = InitializeProcThreadAttributeList(None, count, None, &mut size);
            let mut storage = vec![0usize; size.div_ceil(size_of::<usize>())];
            let list = LPPROC_THREAD_ATTRIBUTE_LIST(storage.as_mut_ptr().cast());
            win(InitializeProcThreadAttributeList(
                Some(list),
                count,
                None,
                &mut size,
            ))?;
            struct Attributes(LPPROC_THREAD_ATTRIBUTE_LIST);
            impl Drop for Attributes {
                fn drop(&mut self) {
                    unsafe {
                        DeleteProcThreadAttributeList(self.0);
                    }
                }
            }
            let _attributes = Attributes(list);
            let capabilities = SECURITY_CAPABILITIES {
                AppContainerSid: binding
                    .as_ref()
                    .map(|b| b.profile.sid.0)
                    .unwrap_or_default(),
                ..Default::default()
            };
            if binding.is_some() {
                win(UpdateProcThreadAttribute(
                    list,
                    0,
                    PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
                    Some((&capabilities as *const SECURITY_CAPABILITIES).cast()),
                    size_of::<SECURITY_CAPABILITIES>(),
                    None,
                    None,
                ))?;
            }
            let mut stdout = HANDLE::default();
            win(DuplicateHandle(
                GetCurrentProcess(),
                HANDLE(output.as_raw_handle()),
                GetCurrentProcess(),
                &mut stdout,
                0,
                true,
                DUPLICATE_SAME_ACCESS,
            ))?;
            let stdout = Handle(stdout);
            let mut stderr = HANDLE::default();
            win(DuplicateHandle(
                GetCurrentProcess(),
                HANDLE(errors.as_raw_handle()),
                GetCurrentProcess(),
                &mut stderr,
                0,
                true,
                DUPLICATE_SAME_ACCESS,
            ))?;
            let stderr = Handle(stderr);
            let null;
            let input = match input {
                Some(file) => file,
                None => {
                    null = File::open("NUL")?;
                    &null
                }
            };
            let mut stdin = HANDLE::default();
            win(DuplicateHandle(
                GetCurrentProcess(),
                HANDLE(input.as_raw_handle()),
                GetCurrentProcess(),
                &mut stdin,
                0,
                true,
                DUPLICATE_SAME_ACCESS,
            ))?;
            let stdin = Handle(stdin);
            let lease = match lease {
                Some(file) => {
                    let mut handle = HANDLE::default();
                    win(DuplicateHandle(GetCurrentProcess(), HANDLE(file.as_raw_handle()),
                        GetCurrentProcess(), &mut handle, 0, true, DUPLICATE_SAME_ACCESS))?;
                    Some(Handle(handle))
                },
                None => None,
            };
            let mut handles = vec![stdout.0, stderr.0, stdin.0];
            if let Some(lease) = &lease { handles.push(lease.0); }
            win(UpdateProcThreadAttribute(
                list,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                Some(handles.as_ptr().cast()),
                size_of::<HANDLE>() * handles.len(),
                None,
                None,
            ))?;
            let job = Handle(win(CreateJobObjectW(None, PCWSTR::null()))?);
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
                | JOB_OBJECT_LIMIT_PROCESS_MEMORY
                | JOB_OBJECT_LIMIT_JOB_MEMORY
                | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
                | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION;
            limits.BasicLimitInformation.ActiveProcessLimit = 8;
            limits.ProcessMemoryLimit = if binding.is_some() { 512 } else { 1024 } * 1024 * 1024;
            limits.JobMemoryLimit = if binding.is_some() { 1024 } else { 2048 } * 1024 * 1024;
            win(SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ))?;
            let jobs = [job.0];
            win(UpdateProcThreadAttribute(
                list,
                0,
                PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
                Some(jobs.as_ptr().cast()),
                size_of::<HANDLE>() * jobs.len(),
                None,
                None,
            ))?;
            let mut startup = STARTUPINFOEXW::default();
            startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
            startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
            startup.StartupInfo.hStdInput = stdin.0;
            startup.StartupInfo.hStdOutput = stdout.0;
            startup.StartupInfo.hStdError = stderr.0;
            startup.lpAttributeList = list;
            let mut process: PROCESS_INFORMATION = zeroed();
            stage(
                win(CreateProcessW(
                    PCWSTR(app.as_ptr()),
                    Some(PWSTR(command.as_mut_ptr())),
                    None,
                    None,
                    true,
                    EXTENDED_STARTUPINFO_PRESENT
                        | CREATE_UNICODE_ENVIRONMENT
                        | CREATE_SUSPENDED
                        | CREATE_NO_WINDOW,
                    Some(environment.as_ptr().cast()),
                    PCWSTR(cwd.as_ptr()),
                    &startup.StartupInfo,
                    &mut process,
                )),
                "isolated process",
            )?;
            let process_handle = Handle(process.hProcess);
            let thread = Handle(process.hThread);
            if ResumeThread(thread.0) == u32::MAX {
                let error = io::Error::last_os_error();
                let _ = TerminateJobObject(job.0, 1);
                return Err(error);
            }
            Ok(Worker {
                process: process_handle,
                job,
                id: process.dwProcessId,
                _binding: binding,
                stop_deadline: None,
                stopped: false,
                working: None,
            })
        }
    }
    pub struct Worker {
        process: Handle,
        job: Handle,
        id: u32,
        _binding: Option<std::sync::Arc<Binding>>,
        stop_deadline: Option<std::time::Instant>,
        stopped: bool,
        working: Option<WorkingDirectory>,
    }
    impl Worker {
        pub fn id(&self) -> u32 {
            self.id
        }
        pub fn try_wait(&mut self) -> io::Result<Option<u32>> {
            unsafe {
                match WaitForSingleObject(self.process.0, 0) {
                    WAIT_TIMEOUT => Ok(None),
                    WAIT_OBJECT_0 => {
                        let mut code = 0;
                        win(GetExitCodeProcess(self.process.0, &mut code))?;
                        Ok(Some(code))
                    }
                    _ => Err(io::Error::last_os_error()),
                }
            }
        }
        pub fn stop(&mut self) -> io::Result<()> {
            if self.stopped {
                return Ok(());
            }
            // Explicit cancellation followed by RAII cleanup shares one
            // deadline; Drop must not restart another two-second wait.
            let deadline = *self
                .stop_deadline
                .get_or_insert_with(|| std::time::Instant::now() + Duration::from_secs(2));
            unsafe {
                win(TerminateJobObject(self.job.0, 1))?;
            }
            loop {
                let mut accounting = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
                unsafe {
                    win(QueryInformationJobObject(
                        Some(self.job.0),
                        JobObjectBasicAccountingInformation,
                        (&mut accounting as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                        size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                        None,
                    ))?;
                }
                // Job accounting and the process object's final termination
                // signal are separate observations. Cancellation must not
                // report success while the root still owns files/leases.
                let root_exited = self.try_wait()?.is_some();
                if accounting.ActiveProcesses == 0 && root_exited {
                    self.stopped = true;
                    self.working.take();
                    return Ok(());
                }
                if std::time::Instant::now() >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "Worker tree stop deadline",
                    ));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        pub fn wait(&mut self, timeout: Duration) -> io::Result<Option<u32>> {
            unsafe {
                let ms = timeout.as_millis().min(u32::MAX as u128) as u32;
                if WaitForSingleObject(self.process.0, ms) == WAIT_TIMEOUT {
                    return Ok(None);
                }
            }
            self.try_wait()
        }
    }
    impl Drop for Worker {
        fn drop(&mut self) {
            let _ = self.stop();
        }
    }
    fn command_line(program: &OsStr, args: &[OsString]) -> io::Result<Vec<u16>> {
        let mut result = Vec::new();
        for value in std::iter::once(program).chain(args.iter().map(OsString::as_os_str)) {
            if !result.is_empty() {
                result.push(b' ' as u16);
            }
            result.push(b'"' as u16);
            let mut slashes = 0;
            for c in value.encode_wide() {
                if c == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "NUL in worker argument",
                    ));
                }
                if c == b'\\' as u16 {
                    slashes += 1;
                    continue;
                }
                if c == b'"' as u16 {
                    result.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2 + 1));
                } else {
                    result.extend(std::iter::repeat_n(b'\\' as u16, slashes));
                }
                slashes = 0;
                result.push(c);
            }
            result.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
            result.push(b'"' as u16);
        }
        result.push(0);
        if result.len() > 32767 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Worker command length",
            ));
        }
        Ok(result)
    }
    fn environment_block(values: &[(OsString, OsString)]) -> io::Result<Vec<u16>> {
        let mut values = values.to_vec();
        values.sort_by_key(|(k, _)| k.to_string_lossy().to_uppercase());
        let mut result = Vec::new();
        let mut previous = None;
        for (k, v) in values {
            let key = k.to_string_lossy().to_uppercase();
            if key.is_empty() || key.contains('=') || previous.as_ref() == Some(&key) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Invalid worker environment",
                ));
            }
            previous = Some(key);
            let pair = k
                .encode_wide()
                .chain(Some(b'=' as u16))
                .chain(v.encode_wide());
            for c in pair {
                if c == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "NUL in worker environment",
                    ));
                }
                result.push(c);
            }
            result.push(0);
        }
        if result.is_empty() {
            result.push(0);
        }
        result.push(0);
        Ok(result)
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn arguments_preserve_quotes_and_trailing_backslashes() {
            let line = command_line(
                OsStr::new("engine.exe"),
                &["a b".into(), "x\"y".into(), "C:\\work\\".into()],
            )
            .unwrap();
            assert_eq!(
                String::from_utf16(&line[..line.len() - 1]).unwrap(),
                "\"engine.exe\" \"a b\" \"x\\\"y\" \"C:\\work\\\\\""
            );
            assert!(command_line(OsStr::new("a\0b"), &[]).is_err());
        }
        #[test]
        fn environment_is_explicit_sorted_and_rejects_duplicate_keys() {
            assert!(environment_block(&[
                ("PATH".into(), "one".into()),
                ("path".into(), "two".into())
            ])
            .is_err());
            assert!(environment_block(&[("BAD=KEY".into(), "x".into())]).is_err());
            let env =
                environment_block(&[("Z".into(), "last".into()), ("A".into(), "first".into())])
                    .unwrap();
            assert_eq!(String::from_utf16(&env).unwrap(), "A=first\0Z=last\0\0");
        }
        #[test]
        fn managed_environment_uses_owned_roots_and_explicit_options() {
            let program = Path::new("C:\\ReadMD\\bin\\engine.exe");
            let directory = Path::new("C:\\ReadMD\\task");
            let mut command = std::process::Command::new(program);
            command
                .env("OMP_THREAD_LIMIT", "1")
                .env_remove("TESSDATA_PREFIX");
            let values = managed_environment(&command, program, directory).unwrap();
            let keys = values
                .iter()
                .map(|(k, _)| k.to_str().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                keys,
                [
                    "APPDATA",
                    "HOME",
                    "LOCALAPPDATA",
                    "OMP_THREAD_LIMIT",
                    "PATH",
                    "SYSTEMROOT",
                    "TEMP",
                    "TMP",
                    "USERPROFILE",
                    "WINDIR"
                ]
            );
            for key in [
                "TEMP",
                "TMP",
                "HOME",
                "USERPROFILE",
                "APPDATA",
                "LOCALAPPDATA",
            ] {
                assert_eq!(
                    values.iter().find(|(k, _)| k == key).unwrap().1.as_os_str(),
                    directory.as_os_str()
                );
            }
            assert_eq!(
                values
                    .iter()
                    .find(|(k, _)| k == "PATH")
                    .unwrap()
                    .1
                    .as_os_str(),
                program.parent().unwrap().as_os_str()
            );
            command.env("temp", "outside-owned-task");
            assert_eq!(
                managed_environment(&command, program, directory)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidInput
            );
            assert!(environment_block(&[("BAD\0KEY".into(), "x".into())]).is_err());
        }
        #[test]
        fn managed_launch_rejects_search_path_programs_without_creating_a_process() {
            let root = tempfile::tempdir().unwrap();
            let output = File::create(root.path().join("stdout.log")).unwrap();
            let command = std::process::Command::new("unreviewed-engine.exe");
            assert_eq!(
                spawn_managed(&command, root.path(), &output, &output)
                    .err()
                    .unwrap()
                    .kind(),
                io::ErrorKind::InvalidInput
            );
            assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
            assert_eq!(output.metadata().unwrap().len(), 0);
        }
        #[test]
        fn managed_explicit_input_uses_resource_cwd_and_owned_scratch() -> io::Result<()> {
            use std::io::Read;
            let payload = b"owned\0binary\r\ninput\xff";
            if std::env::var_os("READMD_MANAGED_INPUT_CHILD").is_some() {
                let mut input = Vec::new();
                std::io::stdin().read_to_end(&mut input)?;
                assert_eq!(input, payload);
                assert_eq!(std::fs::read("model.bin")?, b"owned resource");
                assert_eq!(
                    std::env::var_os("TEMP"),
                    std::env::var_os("READMD_EXPECTED_SCRATCH")
                );
                println!("explicit-input-and-resource-cwd-checked");
                return Ok(());
            }
            let root = tempfile::tempdir()?;
            let scratch = root.path().join("scratch");
            std::fs::create_dir(&scratch)?;
            let segment = "资源".repeat(35);
            let models = root
                .path()
                .join(&segment)
                .join(&segment)
                .join(&segment)
                .join(&segment);
            assert!(models.as_os_str().encode_wide().count() > 260);
            std::fs::create_dir_all(&models)?;
            std::fs::write(models.join("model.bin"), b"owned resource")?;
            std::fs::write(scratch.join("input.bin"), payload)?;
            let input = File::open(scratch.join("input.bin"))?;
            let output = File::create(scratch.join("stdout.log"))?;
            let errors = File::create(scratch.join("stderr.log"))?;
            let mut command = std::process::Command::new(std::env::current_exe()?);
            command.args(["--exact", "worker_sandbox::platform::tests::managed_explicit_input_uses_resource_cwd_and_owned_scratch", "--nocapture"])
                .current_dir(models.canonicalize()?)
                .env("READMD_MANAGED_INPUT_CHILD", "1")
                .env("READMD_EXPECTED_SCRATCH", scratch.canonicalize()?);
            let mut worker = super::super::spawn_managed_with_input(
                &mut command,
                &scratch,
                &input,
                &output,
                &errors,
            )?;
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            loop {
                if let Some(code) = worker.try_wait()? {
                    assert_eq!(
                        code,
                        0,
                        "{}",
                        std::fs::read_to_string(scratch.join("stderr.log"))?
                    );
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "Explicit input child did not exit"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            worker.stop()?;
            assert_eq!(std::fs::read_dir(&scratch)?.count(), 3);
            assert!(std::fs::read_to_string(scratch.join("stdout.log"))?
                .contains("explicit-input-and-resource-cwd-checked"));
            command.current_dir("relative-resources");
            assert_eq!(
                super::super::spawn_managed_with_input(
                    &mut command,
                    &scratch,
                    &input,
                    &output,
                    &errors
                )
                .err()
                .unwrap()
                .kind(),
                io::ErrorKind::InvalidInput
            );
            assert_eq!(std::fs::read_dir(&models)?.count(), 1);
            command
                .current_dir(models.canonicalize()?)
                .arg("invalid\0argument");
            assert_eq!(
                super::super::spawn_managed_with_input(
                    &mut command,
                    &scratch,
                    &input,
                    &output,
                    &errors
                )
                .err()
                .unwrap()
                .kind(),
                io::ErrorKind::InvalidInput
            );
            assert_eq!(std::fs::read_dir(&scratch)?.count(), 3);
            assert_eq!(std::fs::read(models.join("model.bin"))?, b"owned resource");
            Ok(())
        }
        #[test]
        fn resource_working_directory_removes_only_its_link_and_keeps_other_views() -> io::Result<()>
        {
            let root = tempfile::tempdir()?;
            let target = root.path().join("资源".repeat(80)).join("资源".repeat(35));
            std::fs::create_dir_all(&target)?;
            std::fs::write(target.join("original.bin"), b"unchanged model")?;
            let first = WorkingDirectory::create(&target.canonicalize()?, root.path())?;
            let second = WorkingDirectory::create(&target.canonicalize()?, root.path())?;
            assert_eq!(
                std::fs::read(first.link.join("original.bin"))?,
                b"unchanged model"
            );
            let parent = first.parent.clone();
            std::fs::write(parent.join("keep-user.txt"), b"unknown neighbor")?;
            drop(first);
            assert_eq!(
                std::fs::read(parent.join("keep-user.txt"))?,
                b"unknown neighbor"
            );
            assert_eq!(std::fs::read_dir(&parent)?.count(), 1);
            assert_eq!(
                std::fs::read(second.link.join("original.bin"))?,
                b"unchanged model"
            );
            let other_parent = second.parent.clone();
            drop(second);
            assert!(!other_parent.exists());
            assert_eq!(
                std::fs::read(target.join("original.bin"))?,
                b"unchanged model"
            );
            Ok(())
        }
        #[test]
        fn managed_worker_owner_exit_and_drop_stop_only_owned_tree() -> io::Result<()> {
            use std::{
                io::Write,
                process::{Child, Command, Stdio},
                time::Instant,
            };
            const TEST:&str="worker_sandbox::platform::tests::managed_worker_owner_exit_and_drop_stop_only_owned_tree";
            struct Owner(Child);
            impl Drop for Owner {
                fn drop(&mut self) {
                    let _ = self.0.kill();
                    let _ = self.0.wait();
                }
            }
            struct Observed(Handle);
            impl Observed {
                fn open(path: &Path) -> io::Result<Self> {
                    let pid = std::fs::read_to_string(path)?.parse::<u32>().map_err(|_| {
                        io::Error::new(io::ErrorKind::InvalidData, "Invalid synthetic PID")
                    })?;
                    Ok(Self(Handle(unsafe {
                        win(OpenProcess(PROCESS_ACCESS_RIGHTS(0x0010_0000), false, pid))?
                    })))
                }
                fn stopped(&self, timeout: Duration) -> bool {
                    unsafe {
                        WaitForSingleObject(
                            self.0 .0,
                            timeout.as_millis().min(u32::MAX as u128) as u32,
                        ) == WAIT_OBJECT_0
                    }
                }
            }
            fn command(role: &str, root: &Path) -> io::Result<Command> {
                let mut command = crate::silent_command(std::env::current_exe()?);
                command
                    .args(["--exact", TEST, "--test-threads=1", "--nocapture"])
                    .env("READMD_MANAGED_TEST_ROLE", role)
                    .env("READMD_MANAGED_TEST_ROOT", root);
                Ok(command)
            }
            fn wait_ready(root: &Path, mut alive: impl FnMut() -> bool) {
                let deadline = Instant::now() + Duration::from_secs(10);
                while !root.join("engine.ready").is_file() || !root.join("leaf.ready").is_file() {
                    assert!(alive(), "Owned helper exited before ready");
                    assert!(Instant::now() < deadline, "Owned helper ready deadline");
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            if let Ok(role) = std::env::var("READMD_MANAGED_TEST_ROLE") {
                let root = PathBuf::from(std::env::var_os("READMD_MANAGED_TEST_ROOT").ok_or_else(
                    || io::Error::new(io::ErrorKind::InvalidInput, "Missing synthetic task root"),
                )?);
                match role.as_str() {
                    "leaf" => {
                        std::fs::write(root.join("leaf.pid"), std::process::id().to_string())?;
                        std::fs::write(root.join("leaf.ready"), b"ready")?;
                        std::thread::sleep(Duration::from_secs(30));
                    }
                    "engine" | "root-exits" => {
                        assert!(
                            std::env::var_os("READMD_MANAGED_SECRET").is_none(),
                            "Ambient environment leaked"
                        );
                        assert_eq!(
                            std::env::var_os("TEMP").unwrap(),
                            std::fs::canonicalize(&root)?.as_os_str()
                        );
                        let mut leaf = Owner(
                            command("leaf", &root)?
                                .stdout(Stdio::null())
                                .stderr(Stdio::null())
                                .spawn()?,
                        );
                        std::fs::write(root.join("engine.pid"), std::process::id().to_string())?;
                        let deadline = Instant::now() + Duration::from_secs(10);
                        while !root.join("leaf.ready").is_file() {
                            assert!(
                                leaf.0.try_wait()?.is_none(),
                                "Owned descendant exited before ready"
                            );
                            assert!(Instant::now() < deadline, "Owned descendant ready deadline");
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        println!("synthetic-stdout-marker");
                        std::io::stdout().flush()?;
                        eprintln!("synthetic-stderr-marker");
                        std::io::stderr().flush()?;
                        std::fs::write(root.join("engine.ready"), b"ready")?;
                        if role == "root-exits" {
                            std::mem::forget(leaf);
                        } else {
                            std::thread::sleep(Duration::from_secs(30));
                        }
                    }
                    "owner" => {
                        let output = File::create(root.join("stdout.log"))?;
                        let errors = File::create(root.join("stderr.log"))?;
                        let mut engine = super::spawn_managed(
                            &command("engine", &root)?,
                            &root,
                            &output,
                            &errors,
                        )?;
                        let deadline = Instant::now() + Duration::from_secs(30);
                        while Instant::now() < deadline {
                            assert!(engine.try_wait()?.is_none(), "Managed engine exited early");
                            std::thread::sleep(Duration::from_millis(10));
                        }
                    }
                    _ => panic!("Invalid synthetic helper role"),
                }
                return Ok(());
            }
            let root = tempfile::tempdir()?;
            fn owner(root: &Path) -> io::Result<Owner> {
                std::fs::create_dir(root)?;
                Ok(Owner(
                    command("owner", root)?
                        .env("READMD_MANAGED_SECRET", "synthetic-only")
                        .stdout(Stdio::null())
                        .stderr(Stdio::inherit())
                        .spawn()?,
                ))
            }
            let first_root = root.path().join("first");
            let second_root = root.path().join("second");
            let mut first = owner(&first_root)?;
            let mut second = owner(&second_root)?;
            wait_ready(&first_root, || first.0.try_wait().unwrap().is_none());
            wait_ready(&second_root, || second.0.try_wait().unwrap().is_none());
            let first_engine = Observed::open(&first_root.join("engine.pid"))?;
            let first_leaf = Observed::open(&first_root.join("leaf.pid"))?;
            let second_engine = Observed::open(&second_root.join("engine.pid"))?;
            let second_leaf = Observed::open(&second_root.join("leaf.pid"))?;
            let start = Instant::now();
            first.0.kill()?;
            assert!(!first.0.wait()?.success());
            assert!(first_engine.stopped(Duration::from_secs(2).saturating_sub(start.elapsed())));
            assert!(first_leaf.stopped(Duration::from_secs(2).saturating_sub(start.elapsed())));
            assert!(start.elapsed() < Duration::from_secs(2));
            assert!(!second_engine.stopped(Duration::ZERO));
            assert!(!second_leaf.stopped(Duration::ZERO));
            let direct = root.path().join("drop");
            std::fs::create_dir(&direct)?;
            let output = File::create(direct.join("stdout.log"))?;
            let errors = File::create(direct.join("stderr.log"))?;
            let mut engine =
                super::spawn_managed(&command("engine", &direct)?, &direct, &output, &errors)?;
            wait_ready(&direct, || engine.try_wait().unwrap().is_none());
            let observed_engine = Observed::open(&direct.join("engine.pid"))?;
            let observed_leaf = Observed::open(&direct.join("leaf.pid"))?;
            let start = Instant::now();
            drop(engine);
            assert!(observed_engine.stopped(Duration::from_secs(2).saturating_sub(start.elapsed())));
            assert!(observed_leaf.stopped(Duration::from_secs(2).saturating_sub(start.elapsed())));
            assert!(start.elapsed() < Duration::from_secs(2));
            assert!(!second_engine.stopped(Duration::ZERO));
            let out = std::fs::read_to_string(direct.join("stdout.log"))?;
            let err = std::fs::read_to_string(direct.join("stderr.log"))?;
            assert!(
                out.contains("synthetic-stdout-marker") && !out.contains("synthetic-stderr-marker")
            );
            assert!(
                err.contains("synthetic-stderr-marker") && !err.contains("synthetic-stdout-marker")
            );
            // A successful root exit does not authorize an escaped descendant.
            let exited = root.path().join("root-exits");
            std::fs::create_dir(&exited)?;
            let output = File::create(exited.join("stdout.log"))?;
            let errors = File::create(exited.join("stderr.log"))?;
            let mut engine =
                super::spawn_managed(&command("root-exits", &exited)?, &exited, &output, &errors)?;
            wait_ready(&exited, || true);
            let leaf = Observed::open(&exited.join("leaf.pid"))?;
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if let Some(status) = engine.try_wait()? {
                    assert_eq!(status, 0);
                    break;
                }
                assert!(Instant::now() < deadline, "Root exit deadline");
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(!leaf.stopped(Duration::ZERO));
            let start = Instant::now();
            engine.stop()?;
            assert!(leaf.stopped(Duration::from_secs(2).saturating_sub(start.elapsed())));
            assert!(start.elapsed() < Duration::from_secs(2));
            let start = Instant::now();
            drop(second);
            assert!(second_engine.stopped(Duration::from_secs(2).saturating_sub(start.elapsed())));
            assert!(second_leaf.stopped(Duration::from_secs(2).saturating_sub(start.elapsed())));
            Ok(())
        }
        #[test]
        fn native_resource_child() -> io::Result<()> {
            let Some(base) = std::env::var_os("READMD_NATIVE_RESOURCE_CHILD") else { return Ok(()); };
            let base = PathBuf::from(base);
            let data = PathBuf::from(std::env::var_os("READMD_NATIVE_RESOURCE_PROFILE").ok_or_else(|| io::Error::other("Shared task profile required"))?);
            let task = crate::task_workspace::Workspace::create(&data, "latex-sandbox")?;
            let id = task.id();
            let sandbox = Sandbox::new_for_task(&base.join("runtime"), &base.join("work"), task)?;
            let receipt = serde_json::json!({
                "id": id,
                "profile": String::from_utf16(&sandbox.binding.profile.name[..sandbox.binding.profile.name.len()-1]).unwrap(),
                "runtime": String::from_utf16(&sandbox.binding.runtime.name[..sandbox.binding.runtime.name.len()-1]).unwrap(),
                "workspace": String::from_utf16(&sandbox.binding.workspace.name[..sandbox.binding.workspace.name.len()-1]).unwrap(),
                "runtimeTarget": sandbox.binding.runtime.target[..sandbox.binding.runtime.target.len()-1],
            });
            std::fs::write(base.join("ready.json"), serde_json::to_vec(&receipt)?)?;
            loop { std::thread::sleep(Duration::from_millis(20)); }
        }
        fn device_targets(name: &str) -> io::Result<Vec<Vec<u16>>> {
            let name = wide(OsStr::new(name));
            let mut buffer = vec![0u16; 65_536];
            let count = unsafe { QueryDosDeviceW(PCWSTR(name.as_ptr()), Some(&mut buffer)) };
            if count == 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::NotFound { return Ok(Vec::new()); }
                return Err(error);
            }
            Ok(buffer[..count as usize].split(|v| *v == 0).filter(|v| !v.is_empty()).map(|v| v.to_vec()).collect())
        }
        fn assert_profile_present(name: &str) -> io::Result<()> {
            let name = wide(OsStr::new(name));
            match unsafe { CreateAppContainerProfile(PCWSTR(name.as_ptr()), PCWSTR(name.as_ptr()), PCWSTR(name.as_ptr()), None) } {
                Err(error) if error.code().0 & 0xffff == 183 => Ok(()),
                Err(error) => Err(io::Error::from_raw_os_error(error.code().0 & 0xffff)),
                Ok(sid) => {
                    // A failed assertion must not leave the verification profile.
                    let _sid = Sid(sid);
                    unsafe { let _ = DeleteAppContainerProfile(PCWSTR(name.as_ptr())); }
                    Err(io::Error::other("Expected persisted AppContainer profile"))
                }
            }
        }
        fn assert_profile_absent(name: &str) -> io::Result<()> {
            let name = wide(OsStr::new(name));
            // Native creation must succeed after deletion, then tear down this
            // newly owned verification profile. No registry layout assumptions.
            let sid = unsafe { win(CreateAppContainerProfile(PCWSTR(name.as_ptr()), PCWSTR(name.as_ptr()), PCWSTR(name.as_ptr()), None))? };
            let _sid = Sid(sid);
            unsafe { win(DeleteAppContainerProfile(PCWSTR(name.as_ptr()))) }
        }
        #[test]
        fn native_resource_inspector_child() -> io::Result<()> {
            let Some(receipt) = std::env::var_os("READMD_NATIVE_RESOURCE_INSPECT") else { return Ok(()); };
            let receipt: serde_json::Value = serde_json::from_slice(&std::fs::read(receipt)?)?;
            let absent = std::env::var_os("READMD_NATIVE_RESOURCE_EXPECT").is_some_and(|v| v == "absent");
            let name = receipt["profile"].as_str().ok_or_else(|| io::Error::other("Profile receipt required"))?;
            if absent { assert_profile_absent(name)?; } else { assert_profile_present(name)?; }
            for field in ["runtime", "workspace"] {
                let name = receipt[field].as_str().ok_or_else(|| io::Error::other("Device receipt required"))?;
                let targets = device_targets(name)?;
                assert_eq!(targets.is_empty(), absent, "Native device {field}");
            }
            println!("NATIVE_RESOURCES_CHECKED {}", if absent { "absent" } else { "present" });
            Ok(())
        }
        struct ResourceChild(std::process::Child);
        impl Drop for ResourceChild {
            fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); }
        }
        fn launch_resource_child(base: &Path, shared_data: &Path) -> io::Result<(ResourceChild, serde_json::Value)> {
            std::fs::create_dir(base)?;
            std::fs::create_dir(base.join("runtime"))?;
            std::fs::create_dir(base.join("work"))?;
            let errors = File::create(base.join("child.log"))?;
            let mut child = ResourceChild(std::process::Command::new(std::env::current_exe()?)
                .args(["--exact", "worker_sandbox::platform::tests::native_resource_child", "--nocapture"])
                .env("READMD_NATIVE_RESOURCE_CHILD", base)
                .env("READMD_NATIVE_RESOURCE_PROFILE", shared_data)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::from(errors.try_clone()?))
                .stderr(std::process::Stdio::from(errors)).spawn()?);
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            loop {
                if let Ok(bytes) = std::fs::read(base.join("ready.json")) {
                    if let Ok(receipt) = serde_json::from_slice(&bytes) { return Ok((child, receipt)); }
                }
                if child.0.try_wait()?.is_some() || std::time::Instant::now() >= deadline {
                    return Err(io::Error::other(format!("Native resource child failed: {}", std::fs::read_to_string(base.join("child.log"))?)));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        #[test]
        fn native_resources_recover_after_owner_crash_and_preserve_other_owners() -> io::Result<()> {
            let root = tempfile::tempdir()?;
            let data = root.path().join("profile");
            std::fs::create_dir(&data)?;
            struct RecoverAfter(PathBuf);
            impl Drop for RecoverAfter { fn drop(&mut self) { let _ = crate::task_workspace::recover(&self.0, 128); } }
            let _recovery = RecoverAfter(data.clone());
            let (mut first, a) = launch_resource_child(&root.path().join("first"), &data)?;
            let (mut second, b) = launch_resource_child(&root.path().join("second"), &data)?;
            let unknown = View::new(&root.path().canonicalize()?, None)?;
            let unknown_name = String::from_utf16(&unknown.name[..unknown.name.len()-1]).unwrap();
            assert_profile_present(a["profile"].as_str().unwrap())?;
            assert_profile_present(b["profile"].as_str().unwrap())?;
            assert_eq!(device_targets(a["runtime"].as_str().unwrap())?.len(), 1);
            assert_eq!(crate::task_workspace::recover(&data, 128)?.running, 2);
            first.0.kill()?; first.0.wait()?;
            // Forced process termination bypasses all Rust destructors.
            assert_profile_present(a["profile"].as_str().unwrap())?;
            assert_eq!(device_targets(a["workspace"].as_str().unwrap())?.len(), 1);
            let report = crate::task_workspace::recover(&data, 128)?;
            assert_eq!(report.removed, 1, "{report:?}");
            assert_eq!(report.running, 1, "{report:?}");
            assert!(device_targets(a["runtime"].as_str().unwrap())?.is_empty());
            assert!(device_targets(a["workspace"].as_str().unwrap())?.is_empty());
            assert_profile_absent(a["profile"].as_str().unwrap())?;
            assert_profile_present(b["profile"].as_str().unwrap())?;
            assert_eq!(device_targets(b["runtime"].as_str().unwrap())?.len(), 1);
            assert_eq!(device_targets(&unknown_name)?.len(), 1);
            second.0.kill()?; second.0.wait()?;
            assert_eq!(crate::task_workspace::recover(&data, 128)?.removed, 1);
            assert_profile_absent(b["profile"].as_str().unwrap())?;
            assert!(device_targets(b["runtime"].as_str().unwrap())?.is_empty());
            assert_eq!(device_targets(&unknown_name)?.len(), 1);
            Ok(())
        }
        #[test]
        fn native_resource_recovery_removes_only_the_exact_device_definition() -> io::Result<()> {
            let root = tempfile::tempdir()?;
            let data = root.path().join("profile"); std::fs::create_dir(&data)?;
            let task = crate::task_workspace::Workspace::create(&data, "latex-sandbox")?;
            let first = root.path().join("first"); let second = root.path().join("second");
            std::fs::create_dir(&first)?; std::fs::create_dir(&second)?;
            let owned = View::new(&first.canonicalize()?, Some((&task, "runtime")))?;
            let label = String::from_utf16(&owned.name[..owned.name.len()-1]).unwrap();
            let mut target = vec![92,63,63,92];
            target.extend(second.canonicalize()?.as_os_str().encode_wide().skip(4)); target.push(0);
            // This independently created definition is deliberately stacked
            // over the known task definition; an exact removal must preserve it.
            unsafe { win(DefineDosDeviceW(DDD_RAW_TARGET_PATH | DDD_NO_BROADCAST_SYSTEM,
                PCWSTR(owned.name.as_ptr()), PCWSTR(target.as_ptr())))?; }
            let other = View { name: owned.name.clone(), target, path: owned.path.clone() };
            let resource = crate::task_workspace::NativeResource::DosDevice {
                name: label.clone(), target: owned.target[..owned.target.len()-1].to_vec(),
            };
            assert_eq!(device_targets(&label)?.len(), 2);
            recover_native_resource(&resource)?;
            assert_eq!(device_targets(&label)?, vec![other.target[..other.target.len()-1].to_vec()]);
            recover_native_resource(&resource)?;
            assert_eq!(device_targets(&label)?.len(), 1);
            drop(owned); drop(other); drop(task);
            assert!(device_targets(&label)?.is_empty());
            Ok(())
        }
        #[test]
        fn runtime_permissions_are_prepared_only_for_an_empty_root() {
            let root = tempfile::tempdir().unwrap();
            prepare_empty_runtime(root.path()).unwrap();
            std::fs::write(root.path().join("keep.txt"), b"synthetic").unwrap();
            assert!(prepare_empty_runtime(root.path()).is_err());
            assert_eq!(
                std::fs::read(root.path().join("keep.txt")).unwrap(),
                b"synthetic"
            );
        }
    }
}
#[cfg(windows)]
pub use platform::{prepare_empty_runtime, Sandbox, Worker};

pub(crate) fn recover_native_resource(resource: &crate::task_workspace::NativeResource) -> std::io::Result<bool> {
    #[cfg(windows)]
    { platform::recover_native_resource(resource) }
    #[cfg(not(windows))]
    { let _ = resource; Ok(false) }
}

/// An owned engine process. Windows uses creation-time job membership and
/// kills the whole owned tree on Drop/owner exit. Other platforms retain the
/// existing child lifecycle; native crash-containment remains unverified.
pub struct ManagedWorker {
    #[cfg(windows)]
    inner: Worker,
    #[cfg(not(windows))]
    inner: std::process::Child,
}
pub fn spawn_managed(
    command: &mut std::process::Command,
    directory: &std::path::Path,
    output: &std::fs::File,
    errors: &std::fs::File,
) -> std::io::Result<ManagedWorker> {
    #[cfg(windows)]
    let inner = platform::spawn_managed(command, directory, output, errors)?;
    #[cfg(not(windows))]
    let inner = command
        .current_dir(directory)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(output.try_clone()?))
        .stderr(std::process::Stdio::from(errors.try_clone()?))
        .spawn()?;
    Ok(ManagedWorker { inner })
}
/// Explicitly opened bounded input; resource cwd and task scratch stay separate.
pub fn spawn_managed_with_input(
    command: &mut std::process::Command,
    scratch: &std::path::Path,
    input: &std::fs::File,
    output: &std::fs::File,
    errors: &std::fs::File,
) -> std::io::Result<ManagedWorker> {
    #[cfg(windows)]
    let inner = platform::spawn_managed_input(command, scratch, output, errors, Some(input))?;
    #[cfg(not(windows))]
    let inner = {
        if command.get_current_dir().is_none() {
            command.current_dir(scratch);
        }
        command
            .stdin(std::process::Stdio::from(input.try_clone()?))
            .stdout(std::process::Stdio::from(output.try_clone()?))
            .stderr(std::process::Stdio::from(errors.try_clone()?))
            .spawn()?
    };
    Ok(ManagedWorker { inner })
}
impl ManagedWorker {
    pub fn id(&self) -> u32 {
        self.inner.id()
    }
    pub fn try_wait(&mut self) -> std::io::Result<Option<u32>> {
        #[cfg(windows)]
        {
            self.inner.try_wait()
        }
        #[cfg(not(windows))]
        {
            self.inner
                .try_wait()
                .map(|status| status.map(|s| s.code().unwrap_or(1) as u32))
        }
    }
    pub fn stop(&mut self) -> std::io::Result<()> {
        #[cfg(windows)]
        {
            self.inner.stop()
        }
        #[cfg(not(windows))]
        {
            let _ = self.inner.kill();
            self.inner.wait().map(|_| ())
        }
    }
}
impl Drop for ManagedWorker {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
