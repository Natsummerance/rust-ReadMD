# Ephemeral CI only: all volume bytes belong to this fresh workspace VHD.
# No user disks, VM installation, models, credentials or default associations.
$ErrorActionPreference = 'Stop'
if ($env:CI -ne 'true' -or $env:RUNNER_OS -ne 'Windows' -or !$env:GITHUB_WORKSPACE) { throw 'Dedicated Windows CI required' }
$repo = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
if ($repo -ne (Resolve-Path $env:GITHUB_WORKSPACE).Path) { throw 'Unexpected CI workspace' }
$out = Join-Path $repo '.cache/p1-platform-space'
$prepared = Get-Content -LiteralPath (Join-Path $out 'prepared.json') -Raw | ConvertFrom-Json
$binary = Join-Path $repo $prepared.binary
$pack = Join-Path $repo $prepared.pack
$assets = Join-Path $repo $prepared.assets
$image = Join-Path $out 'owned-256m.vhd'
$tmp = Join-Path $out 'tmp'
$diskpart = Join-Path $env:SystemRoot 'System32/diskpart.exe'
$fsutil = Join-Path $env:SystemRoot 'System32/fsutil.exe'
if ((Test-Path -LiteralPath $image) -or (Test-Path -LiteralPath $tmp) -or (Test-Path -LiteralPath (Join-Path $out 'native-space.json'))) { throw 'Do not reuse a volume attempt' }
New-Item -ItemType Directory -Path $tmp | Out-Null
$created = $false
$attached = $false
$quotaUser = $null
$quotaIdentity = $null
$quotaPassword = $null
$quotaProfile = Join-Path $out 'quota-profile'
if (Test-Path -LiteralPath $quotaProfile) { throw 'Do not reuse a quota profile' }
$phase = 'preparation'
function Disk-Part([string]$label, [string[]]$commands) {
    $script = Join-Path $out ($label + '.diskpart')
    [IO.File]::WriteAllText($script, (($commands + 'exit') -join "`r`n") + "`r`n")
    $text = (& $diskpart /s $script 2>&1 | Out-String)
    [IO.File]::WriteAllText((Join-Path $out ($label + '.log')), $text)
    if ($LASTEXITCODE -ne 0) { throw 'Owned VHD command failed' }
    return $text
}
function Quota([string[]]$arguments) {
    $text = (& $fsutil @arguments 2>&1 | Out-String)
    if ($LASTEXITCODE -ne 0) { [IO.File]::WriteAllText((Join-Path $out 'quota-command-failure.log'), $text); throw 'Owned volume quota setup failed' }
}
function Consumer([string]$label, [string]$volumeRoot, [string]$report, [hashtable]$extra, [bool]$standardUser = $false) {
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $binary
    $info.WorkingDirectory = $repo
    $info.UseShellExecute = $false
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    if ($standardUser) {
        # A runner administrator's default file owner is the Administrators
        # group. Its apparent free space cannot certify an individual quota.
        # Use a fresh unprivileged CI identity, with an in-memory password.
        $info.UserName = $quotaUser.Name
        $info.Domain = $env:COMPUTERNAME
        $info.Password = $quotaPassword
        $info.LoadUserProfile = $false
        $info.Environment['USERPROFILE'] = $quotaProfile
        $info.Environment['HOME'] = $quotaProfile
        $info.Environment['LOCALAPPDATA'] = Join-Path $quotaProfile 'AppData/Local'
        $info.Environment['APPDATA'] = Join-Path $quotaProfile 'AppData/Roaming'
    }
    foreach ($key in @($info.Environment.Keys)) { if ($key -match 'TOKEN|SECRET|CREDENTIAL|API_KEY') { $info.Environment.Remove($key) | Out-Null } }
    foreach ($name in @('READMD_SPACE_REOPEN_ONLY', 'READMD_SPACE_QUOTA_ONLY', 'READMD_SPACE_QUOTA_REOPEN_ONLY')) { $info.Environment.Remove($name) | Out-Null }
    $values = @{ TEMP=$tmp; TMP=$tmp; SQLITE_TMPDIR=$tmp; READMD_SPACE_OWNED_VOLUME=$volumeRoot; READMD_SPACE_INSTALL_ASSETS=$assets; READMD_SPACE_INSTALL_PACK=$pack; READMD_SPACE_REPORT_PATH=$report }
    foreach ($entry in $values.GetEnumerator()) { $info.Environment[$entry.Key] = [string]$entry.Value }
    foreach ($entry in $extra.GetEnumerator()) { $info.Environment[$entry.Key] = [string]$entry.Value }
    foreach ($argument in @('--exact', 'capability_packs::space_volume_tests::native_small_volume_preflight_enospc_cancel_and_reopen', '--ignored', '--nocapture', '--test-threads=1')) { $info.ArgumentList.Add($argument) }
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $info
    if (!$process.Start()) { throw 'Owned consumer failed to start' }
    try {
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        if (!$process.WaitForExit(120000)) { $process.Kill($true); $process.WaitForExit(); throw 'Owned consumer deadline' }
        $text = $stdout.GetAwaiter().GetResult() + $stderr.GetAwaiter().GetResult()
        if ($text.Length -gt 1048576) { throw 'Owned consumer output budget' }
        [IO.File]::WriteAllText((Join-Path $out ($label + '.log')), $text)
        if ($process.ExitCode -ne 0) { throw 'Owned native consumer failed; retain the first log' }
    } finally { if (!$process.HasExited) { $process.Kill($true); $process.WaitForExit() }; $process.Dispose() }
}
try {
    $phase = 'create_owned_vhd'
    $text = Disk-Part 'create' @("create vdisk file=`"$image`" maximum=256 type=expandable")
    if ($text -notmatch 'successfully created the virtual disk file') { throw 'No positive fresh-VHD ownership receipt' }
    $item = Get-Item -LiteralPath $image
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'VHD cannot be redirected' }
    $created = $true
    $imageBirth = $item.CreationTimeUtc
    $phase = 'attach_owned_vhd'
    Disk-Part 'attach' @("select vdisk file=`"$image`"", 'attach vdisk') | Out-Null
    $diskImage = Get-DiskImage -ImagePath $image
    if (!$diskImage.Attached) { throw 'Owned VHD was not attached' }
    $attached = $true
    $disk = $diskImage | Get-Disk
    if ($disk.Size -gt 268435456 -or $disk.PartitionStyle -ne 'RAW') { throw 'Refuse non-fresh or oversized virtual disk' }
    $drive = $null
    foreach ($code in 82..90) { $letter = [char]$code; if (!(Test-Path ($letter + ':\')) -and !(Get-Volume -DriveLetter $letter -ErrorAction SilentlyContinue)) { $drive = $letter; break } }
    if (!$drive) { throw 'No free CI volume letter' }
    Initialize-Disk -Number $disk.Number -PartitionStyle MBR | Out-Null
    $partition = New-Partition -DiskNumber $disk.Number -UseMaximumSize -DriveLetter $drive
    $partition | Format-Volume -FileSystem NTFS -NewFileSystemLabel 'ReadMD-P1-Owned' -AllocationUnitSize 4096 -Confirm:$false -Force | Out-Null
    $volume = $drive + ':\'
    if ((Get-Volume -DriveLetter $drive).FileSystemLabel -ne 'ReadMD-P1-Owned') { throw 'Owned volume identity mismatch' }
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent().Name
    $quotaRoot = Join-Path $volume 'quota-fixture'
    $mainRoot = Join-Path $volume 'enospc-fixture'
    foreach ($root in @($quotaRoot, $mainRoot)) { New-Item -ItemType Directory -Path $root | Out-Null; [IO.File]::WriteAllText((Join-Path $root '.readmd-owned-small-volume-v1'), "ReadMD P0 capacity evidence only`n", [Text.UTF8Encoding]::new($false)) }
    $phase = 'fresh_standard_quota_caller'
    $name = 'RdP1Quota' + [Guid]::NewGuid().ToString('N').Substring(0, 8)
    if (Get-LocalUser -Name $name -ErrorAction SilentlyContinue) { throw 'Do not reuse an existing CI identity' }
    $quotaPassword = ConvertTo-SecureString ('Rd1!' + [Guid]::NewGuid().ToString('N') + [Guid]::NewGuid().ToString('N')) -AsPlainText -Force
    $quotaUser = New-LocalUser -Name $name -Password $quotaPassword -Description 'Ephemeral ReadMD owned-volume acceptance only' -AccountNeverExpires
    $quotaIdentity = $env:COMPUTERNAME + '\' + $name
    Add-LocalGroupMember -SID 'S-1-5-32-545' -Member $quotaIdentity
    New-Item -ItemType Directory -Path $quotaProfile | Out-Null
    # Only the disposable public CI checkout and this owned volume are granted
    # access. No account password or credential is saved in scripts or logs.
    foreach ($entry in @(@($repo, 'RX'), @($out, 'M'), @($quotaRoot, 'M'))) {
        & (Join-Path $env:SystemRoot 'System32/icacls.exe') $entry[0] /grant ($quotaIdentity + ':(OI)(CI)' + $entry[1]) /q | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'Owned fixture ACL setup failed' }
    }
    $phase = 'real_caller_hard_quota'
    Quota @('quota', 'enforce', $volume)
    Quota @('quota', 'modify', $volume, '67108864', '67108864', $quotaIdentity)
    $quotaReport = Join-Path $out 'native-quota.json'
    Consumer 'quota-first-process' $quotaRoot $quotaReport @{ READMD_SPACE_QUOTA_ONLY='1' } $true
    Consumer 'quota-reopen-process' $quotaRoot $quotaReport @{ READMD_SPACE_QUOTA_REOPEN_ONLY='1' } $true
    $quotaResult = Get-Content -LiteralPath $quotaReport -Raw | ConvertFrom-Json
    if (!$quotaResult.passed -or !$quotaResult.real_ntfs_hard_quota -or !$quotaResult.new_process_reopened -or !$quotaResult.undo_redo_and_document_save_after_quota_refusal) { throw 'Missing actual quota proof' }
    $phase = 'native_enospc_and_recovery'
    Quota @('quota', 'modify', $volume, '268435456', '268435456', $identity)
    $report = Join-Path $out 'native-space.json'
    Consumer 'first-process' $mainRoot $report @{}
    Consumer 'reopen-process' $mainRoot $report @{ READMD_SPACE_REOPEN_ONLY='1' }
    $result = Get-Content -LiteralPath $report -Raw | ConvertFrom-Json
    if (!$result.passed -or !$result.native_enospc -or !$result.new_process_reopened -or !$result.pending_review_retained) { throw 'Missing actual native space proof' }
    $result | Add-Member -NotePropertyName sourceCommit -NotePropertyValue $prepared.sourceCommit
    $result | Add-Member -NotePropertyName kernel_test_sha256 -NotePropertyValue $prepared.kernelTestSha256
    $result | Add-Member -NotePropertyName offline_pack_sha256 -NotePropertyValue $prepared.offlinePackSha256
    [IO.File]::WriteAllText($report, ($result | ConvertTo-Json -Depth 12) + "`n")
} catch {
    [IO.File]::WriteAllText((Join-Path $out 'first-failure.json'), (@{passed=$false; phase=$phase; modelsRequested=$false; sourceCommit=$prepared.sourceCommit} | ConvertTo-Json) + "`n")
    throw
} finally {
    if ($attached) { Dismount-DiskImage -ImagePath $image; if ((Get-DiskImage -ImagePath $image).Attached) { throw 'Preserve still-attached owned VHD' }; $attached=$false }
    if ($created) { $item=Get-Item -LiteralPath $image; if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $item.CreationTimeUtc -ne $imageBirth) { throw 'Preserve changed VHD identity' }; Remove-Item -LiteralPath $image }
    if (Test-Path -LiteralPath $tmp) { Remove-Item -LiteralPath $tmp -Recurse }
    if ($quotaUser) {
        $currentUser = Get-LocalUser -Name $quotaUser.Name -ErrorAction SilentlyContinue
        if ($currentUser -and $currentUser.SID -ne $quotaUser.SID) { throw 'Preserve changed CI user identity' }
        if ($currentUser) { Remove-LocalUser -SID $quotaUser.SID }
    }
    if (Test-Path -LiteralPath $quotaProfile) { Remove-Item -LiteralPath $quotaProfile -Recurse }
    if ($quotaPassword) { $quotaPassword.Dispose() }
}
Write-Output '{"passed":true,"nativeWindowsEnospc":true,"realCallerQuota":true,"newProcessReopened":true,"ownedImageRemoved":true,"modelsRequested":false}'
