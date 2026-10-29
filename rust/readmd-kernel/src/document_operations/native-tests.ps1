# Standalone Windows verification using existing dependency rlibs. No Cargo,
# dependency installation, boot packaging, configuration copies or cleanup.
[CmdletBinding()]
param(
    [string]$DependencyDirectory = '',
    [string]$EvidenceDirectory = '',
    [string]$Sha2Rlib = '',
    [string]$WindowsRlib = ''
)
$ErrorActionPreference = 'Stop'
$taskProjectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../../..'))
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    throw 'This standalone runner is for Windows. Unix requires its own native execution.'
}
if (-not $DependencyDirectory) { $DependencyDirectory = Join-Path $taskProjectRoot '.cache/community-upgrade/build/debug/deps' }
if (-not $EvidenceDirectory) { $EvidenceDirectory = Join-Path $taskProjectRoot '.cache/v006-document-operations-native' }
$DependencyDirectory = (Resolve-Path -LiteralPath $DependencyDirectory).Path
$EvidenceDirectory = [IO.Path]::GetFullPath($EvidenceDirectory)
if (-not $Sha2Rlib) {
    $Sha2Rlib = (Get-ChildItem -LiteralPath $DependencyDirectory -Filter 'libsha2-*.rlib' -File | Sort-Object LastWriteTime -Descending | Select-Object -First 1).FullName
}
if (-not $WindowsRlib) {
    $WindowsRlib = (Get-ChildItem -LiteralPath $DependencyDirectory -Filter 'libwindows-*.rlib' -File | Sort-Object LastWriteTime -Descending | Select-Object -First 1).FullName
}
if (-not $Sha2Rlib -or -not $WindowsRlib) { throw 'Existing sha2/windows dependency rlibs are required. This runner never builds or installs them.' }
$Sha2Rlib = (Resolve-Path -LiteralPath $Sha2Rlib).Path
$WindowsRlib = (Resolve-Path -LiteralPath $WindowsRlib).Path
$taskRustc = (Get-Command rustc -ErrorAction Stop).Source
$taskRun = Join-Path $EvidenceDirectory ('run-' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfff') + '-' + [Guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory -Path $taskRun | Out-Null
$taskSource = Join-Path $taskProjectRoot 'rust/readmd-kernel/src/document_operations.rs'
$taskBinary = Join-Path $taskRun 'document-operations-tests.exe'
$taskCompileLog = Join-Path $taskRun 'compile.log'
$taskTestLog = Join-Path $taskRun 'native-tests.log'
$taskArguments = @('--edition=2021', '--crate-name', 'document_operations', '--test', $taskSource, '-C', 'debuginfo=0', '-L', ('dependency=' + $DependencyDirectory), '--extern', ('sha2=' + $Sha2Rlib), '--extern', ('windows=' + $WindowsRlib), '-o', $taskBinary)
$taskPreviousPolicy = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
& $taskRustc @taskArguments *> $taskCompileLog
$taskCompileExit = $LASTEXITCODE
$ErrorActionPreference = $taskPreviousPolicy
$taskReport = [ordered]@{
    schema = 1
    scope = 'Standalone Windows native module tests; not full kernel or HTTP/frontend integration'
    compiler = (& $taskRustc --version | Out-String).Trim()
    dependencies = @{ sha2 = $Sha2Rlib; windows = $WindowsRlib }
    compileExit = $taskCompileExit
    testExit = $null
    sourceHashes = [ordered]@{}
    fixturesPreserved = $true
    compileLog = $taskCompileLog
    testLog = $taskTestLog
}
foreach ($taskFile in @('document_operations.rs', 'document_operations/windows.rs', 'document_operations/unix.rs', 'document_operations/unsupported.rs', 'document_operations/tests.rs')) {
    $taskReport.sourceHashes[$taskFile] = (Get-FileHash -LiteralPath (Join-Path $taskProjectRoot ('rust/readmd-kernel/src/' + $taskFile)) -Algorithm SHA256).Hash.ToLowerInvariant()
}
if ($taskCompileExit -eq 0) {
    $taskPreviousTestRoot = [Environment]::GetEnvironmentVariable('READMD_DOCUMENT_OPERATIONS_TEST_ROOT', 'Process')
    try {
        [Environment]::SetEnvironmentVariable('READMD_DOCUMENT_OPERATIONS_TEST_ROOT', (Join-Path $taskRun 'fixtures'), 'Process')
        $ErrorActionPreference = 'Continue'
        & $taskBinary --nocapture --test-threads=1 *> $taskTestLog
        $taskReport.testExit = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $taskPreviousPolicy
        [Environment]::SetEnvironmentVariable('READMD_DOCUMENT_OPERATIONS_TEST_ROOT', $taskPreviousTestRoot, 'Process')
    }
    $taskReport.binarySha256 = (Get-FileHash -LiteralPath $taskBinary -Algorithm SHA256).Hash.ToLowerInvariant()
}
$taskReportPath = Join-Path $taskRun 'report.json'
$taskReport | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $taskReportPath -Encoding UTF8
Get-Content -LiteralPath $taskCompileLog
if (Test-Path -LiteralPath $taskTestLog) { Get-Content -LiteralPath $taskTestLog }
Write-Output ('Report: ' + $taskReportPath)
if ($taskCompileExit -ne 0 -or $taskReport.testExit -ne 0) { throw 'Standalone native verification failed; evidence and fixtures have been retained.' }
