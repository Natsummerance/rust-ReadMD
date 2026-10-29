# Document operation primitives

This module is an implementation unit for the rename safety protocol. Its
presence is **not** evidence that the server, review service, UI, or every write
entry point has joined that protocol. Do not expose rename success until their
authorization, durable intent, recovery, and integration tests pass.

## Public contract

```rust
pub fn acquire(data_dir: &Path, wait: Duration) -> Result<Guard>;
pub fn observe(path: &Path, maximum_bytes: u64) -> Result<Observation>;
pub fn observe_identity(path: &Path) -> Result<IdentityObservation>;

pub enum Observation { Missing, Regular(ObservedFile), Unsupported(UnsupportedReason) }
pub enum IdentityObservation { Missing, Regular(FileIdentity), Unsupported(UnsupportedReason) }
pub struct FileFingerprint { pub identity: FileIdentity, pub revision: String }

impl ObservedFile {
    pub fn fingerprint(&self) -> &FileFingerprint;
    pub fn bytes(&self) -> &[u8];
}
impl FileIdentity {
    pub fn as_str(&self) -> &str;
    pub fn parse(value: &str) -> Result<Self>;
}
impl Guard {
    pub fn verify_profile(&self, data_dir: &Path) -> Result<()>;
    pub fn revalidate(&self) -> Result<()>;
    pub fn verify_existing(&self, path: &Path, expected: &FileFingerprint,
                           maximum_bytes: u64) -> Result<ObservedFile>;
    pub fn verify_existing_identity(&self, path: &Path, expected: &FileIdentity)
        -> Result<FileIdentity>;
    pub fn verify_missing(&self, path: &Path) -> Result<()>;
    pub fn move_same_directory_noreplace(&self, source: &ObservedFile,
                                       target_name: &OsStr) -> Result<MoveReceipt>;
}
pub struct MoveReceipt { pub fingerprint: FileFingerprint }
```

The existing data directory is canonicalized to a shared profile identity;
trusted aliases to that profile use one fixed `document-operations.lock`.
The lock is empty, never truncated or deleted, and uses no PID or stale timeout
reclamation. Wait requests are capped at three seconds. Invalid lock objects
fail without repairing or deleting them. Lock order is `SAVE_LOCK`, profile
gate, authorization/review transaction. Callers must not reacquire the gate or
hold it during UI/network/conversion work. `verify_profile` checks that an
existing guard belongs to the intended data directory.

Document observations use absolute paths, retain and recheck every parent, and
refuse leaf or parent symlink/reparse redirection. Only an absent leaf under an
existing verified parent is `Missing`; an unreadable/missing parent does not
become create permission. `verify_missing` is a preflight, not an atomic
authorization to publish a new file.

Strict rename observations retain the actual file handle and exact raw bytes,
require one hard link and strong identity, and cap capture at the lower of the
caller budget and 32 MiB. `revision` is lowercase SHA-256 of those raw bytes,
including BOM and original line endings. Metadata and the named identity are
checked again after reading. No body is decoded, normalized or saved.

Identity-only observation reads no body, has no size cap, and permits regular
hard-linked files. Ordinary save authorization still has to bind the exact
opened path, this identity, and the caller's decoded-content revision; the gate
does not grant access to another hard-link name. Missing or changed objects
cannot be recreated using an old existing-file authorization. Filesystems
without strong identity return `Unsupported`: callers must explicitly define
ordinary open/save compatibility rather than fabricating identity or silently
claiming safe rename. Strong identity is subject to the filesystem's identity
and creation-time persistence/recycling guarantees.

Serialize identity only using `as_str`/`parse`. The versioned format is
`readmd-file-v1:<windows|unix>:<volume/dev,16 lowercase hex>:<file-ID,32 lowercase hex>:<birth,32 lowercase hex>`.
Windows uses the complete 128-bit file ID and volume serial from `FileIdInfo`
and creation FILETIME ticks. Unix uses device/inode plus creation nanoseconds
since Unix epoch; absent creation-time support is refused. Identity contains
no path and is not an access credential. It has no serde dependency.

## Platform and failure boundaries

Windows pins ordinary ancestors using actual read access without delete
sharing. The gate has read/write access and zero sharing; an attributes-only
handle verifies its stable name. Moves use one `MoveFileExW` call with
`WRITE_THROUGH` and without `REPLACE_EXISTING` or `COPY_ALLOWED`. Case-only
aliases that already resolve to an existing target are refused. There is no
temporary-name trick or overwrite/copy/delete fallback. This does not promise
power-loss durability beyond the documented native flag.

Linux/macOS use retained directory descriptors, descriptor-relative no-follow
opens, `flock`, and respectively `renameat2(RENAME_NOREPLACE)` /
`renameatx_np(RENAME_EXCL)`, followed by directory `sync_all`. Unavailable
no-replace or identity support fails closed. Other platforms are unsupported.
The current evidence is Windows native only; Unix code has not been executed
on Linux or macOS by this slice.

`Error.kind` and `Error.move_state` are public. `Error.code()` returns stable
`document_operation_` codes: `busy`, `lock`, `path`, `unsupported`, `missing`,
`identity_changed`, `revision_changed`, `target_exists`, or `storage`. Genuine
I/O errors retain their cause; observations classify unsafe/unsupported objects
without treating them as missing. `MoveState` is `NotAttempted`, `Unknown`
(native error), or `Moved` (native success then verification/flush failure).
For `Unknown`/`Moved`, preserve the durable rename intent and inspect both ends.
The module never guesses ownership, overwrites a target, or rolls back by
deleting/replacing another object.

The gate coordinates only participating new processes using the same profile.
External programs, old kernels, and another profile remain outside it. An
identity/hash precheck is **not an atomic inode/content compare-and-swap**.
A nonparticipating writer can replace the source between the final check and
the native move; the module detects an unexpected moved identity afterward,
returns `Moved`, and preserves both original and replacement objects. It cannot
promise that such an external source is never moved. Recovery must not commit
review metadata based only on matching content hashes.

## Standalone native verification

From the repository root on Windows, with existing compatible dependency rlibs:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File rust/readmd-kernel/src/document_operations/native-tests.ps1
```

`-DependencyDirectory`, `-Sha2Rlib`, `-WindowsRlib`, and `-EvidenceDirectory`
can select existing artifacts explicitly. The runner invokes `rustc --test`,
never Cargo, installs nothing, and retains a unique report, binary, logs, and
all synthetic fixtures. It touches no user documents/configuration. The 40 MiB
fixture is made using `set_len`; physical allocation depends on the filesystem.

The tests include actual two-process gate contention, the three-second cap,
normal and forced owned-child exit, stale authority, source replacement,
hard-link and 40 MiB identity-only compatibility, raw-byte preservation,
junction refusal, and deterministic late-target/native-move boundaries. A
single ignored test is the child helper invoked by the process tests, not a
skipped product scenario. The tests are not HTTP or native-window UI evidence.

Initial Windows evidence is retained under
`.cache/v006-document-operations-native/first-test-run.log`: 20 passed, one
fixture failed because its replacement body exceeded the original capture
budget and correctly produced `Unsupported` before the expected identity
mismatch. The fixture was changed to an equal-length body. The next direct run
is `final-test-run.log`: 22 passed, one child helper ignored. Preserve both.
