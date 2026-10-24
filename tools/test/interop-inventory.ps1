param([switch]$IncludePrivatePaths)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$errors = [Collections.Generic.List[object]]::new()
$entries = [Collections.Generic.List[object]]::new()
$appPaths = [Collections.Generic.List[object]]::new()
function Read-RegistryChildren([string]$Root, [string]$Kind) {
    try {
        if (-not (Test-Path -LiteralPath $Root)) { return }
        foreach ($key in Get-ChildItem -LiteralPath $Root) {
            try {
                $value = Get-ItemProperty -LiteralPath $key.PSPath
                if ($Kind -eq 'uninstall') {
                    if ($value.DisplayName) {
                        $entries.Add([pscustomobject]@{ Name = [string]$value.DisplayName; Version = [string]$value.DisplayVersion; Location = [string]$value.InstallLocation })
                    }
                } else {
                    $appPaths.Add([pscustomobject]@{ Name = $key.PSChildName; Path = [string]$value.'(default)' })
                }
            } catch { $errors.Add([pscustomobject]@{ operation = 'registry-entry'; kind = $Kind; reason = $_.Exception.GetType().Name }) }
        }
    } catch { $errors.Add([pscustomobject]@{ operation = 'registry-root'; kind = $Kind; reason = $_.Exception.GetType().Name }) }
}
foreach ($root in @('HKLM:\SOFTWARE', 'HKLM:\SOFTWARE\WOW6432Node', 'HKCU:\SOFTWARE')) {
    Read-RegistryChildren "$root\Microsoft\Windows\CurrentVersion\Uninstall" 'uninstall'
    Read-RegistryChildren "$root\Microsoft\Windows\CurrentVersion\App Paths" 'app-path'
}
$definitions = @(
    @{ id = 'word'; name = 'Microsoft Word'; install = 'Microsoft (Office|365)|Office 16'; executables = @('WINWORD.EXE'); processes = @('WINWORD'); relative = @('Microsoft Office\root\Office16\WINWORD.EXE', 'Microsoft Office\Office16\WINWORD.EXE') },
    @{ id = 'wps'; name = 'WPS Writer'; install = '^WPS|Kingsoft'; executables = @('wps.exe'); processes = @('wps'); relative = @('Kingsoft\WPS Office\office6\wps.exe') },
    @{ id = 'libreoffice'; name = 'LibreOffice Writer'; install = 'LibreOffice'; executables = @('soffice.com', 'soffice.exe'); processes = @('soffice', 'soffice.bin'); relative = @('LibreOffice\program\soffice.com', 'LibreOffice\program\soffice.exe') },
    @{ id = 'thorium'; name = 'Thorium Reader'; install = 'Thorium'; executables = @('Thorium.exe'); processes = @('Thorium'); relative = @('Programs\Thorium\Thorium.exe', 'Thorium\Thorium.exe') },
    @{ id = 'calibre'; name = 'Calibre Ebook Viewer'; install = 'Calibre'; executables = @('ebook-viewer.exe', 'ebook-convert.exe', 'ebook-polish.exe'); processes = @('calibre', 'ebook-viewer'); relative = @('Calibre2\ebook-viewer.exe', 'Calibre\ebook-viewer.exe', 'Calibre2\ebook-convert.exe', 'Calibre2\ebook-polish.exe') },
    @{ id = 'vscode'; name = 'Visual Studio Code'; install = 'Microsoft Visual Studio Code'; executables = @('Code.exe'); processes = @('Code'); relative = @('Microsoft VS Code\Code.exe', 'Programs\Microsoft VS Code\Code.exe') }
)
$processes = @()
try { $processes = @(Get-Process) } catch { $errors.Add([pscustomobject]@{ operation = 'process-list'; kind = 'all'; reason = $_.Exception.GetType().Name }) }
$roots = @($env:ProgramFiles, [Environment]::GetEnvironmentVariable('ProgramFiles(x86)'), $env:LOCALAPPDATA) | Where-Object { $_ } | Select-Object -Unique
$applications = foreach ($definition in $definitions) {
    $registered = @($entries | Where-Object { $_.Name -match $definition.install })
    $running = @($processes | Where-Object { $definition.processes -contains $_.ProcessName })
    $candidates = [Collections.Generic.List[string]]::new()
    foreach ($entry in $appPaths) {
        if ($definition.executables -contains $entry.Name -and $entry.Path) { $candidates.Add($entry.Path.Trim('"')) }
    }
    foreach ($executable in $definition.executables) {
        $command = Get-Command $executable -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($command) { $candidates.Add($command.Source) }
    }
    # Explicit QA-selected CLI locations can point to already staged portable
    # applications. They never trigger downloads, directory scans or execution.
    if ($definition.id -eq 'calibre' -and $env:READMD_CALIBRE_CONVERT_BIN) { $candidates.Add($env:READMD_CALIBRE_CONVERT_BIN) }
    if ($definition.id -eq 'vscode' -and $env:READMD_VSCODE_BIN) { $candidates.Add($env:READMD_VSCODE_BIN) }
    foreach ($process in $running) { try { if ($process.Path) { $candidates.Add($process.Path) } } catch { } }
    foreach ($root in $roots) { foreach ($relative in $definition.relative) { $candidates.Add((Join-Path $root $relative)) } }
    foreach ($registration in $registered) {
        if ($registration.Location) {
            foreach ($executable in $definition.executables) {
                foreach ($relative in @($executable, "program\$executable", "office6\$executable", "root\Office16\$executable")) {
                    $candidates.Add((Join-Path $registration.Location $relative))
                }
            }
        }
    }
    if ($definition.id -eq 'calibre') {
        # Exact console siblings only, without scanning user folders or invoking the viewer.
        foreach ($candidate in @($candidates.ToArray())) {
            foreach ($file in @('ebook-convert.exe', 'ebook-polish.exe')) {
                $candidates.Add((Join-Path ([IO.Path]::GetDirectoryName($candidate)) $file))
            }
        }
    }
    $found = [Collections.Generic.List[object]]::new()
    foreach ($candidate in ($candidates | Select-Object -Unique)) {
        try {
            if (Test-Path -LiteralPath $candidate -PathType Leaf) {
                $file = Get-Item -LiteralPath $candidate
                $metadata = [ordered]@{ fileName = $file.Name; version = $file.VersionInfo.FileVersion; bytes = $file.Length }
                # The diagnostics orchestrator uses this only in memory and strips
                # underscore-prefixed fields before writing any public report.
                if ($IncludePrivatePaths) { $metadata._path = $file.FullName }
                $found.Add([pscustomobject]$metadata)
            }
        } catch { $errors.Add([pscustomobject]@{ operation = 'executable-read'; kind = $definition.id; reason = $_.Exception.GetType().Name }) }
    }
    [pscustomobject]@{
        id = $definition.id; name = $definition.name
        executables = @($found.ToArray())
        registrations = @($registered | ForEach-Object { [pscustomobject]@{ name = $_.Name; version = $_.Version } })
        running = @($running | ForEach-Object { [pscustomobject]@{ id = $_.Id; name = $_.ProcessName } })
    }
}
[pscustomobject]@{
    schemaVersion = 1
    recordedAt = [DateTime]::UtcNow.ToString('o')
    platform = 'win32'; readOnly = $true
    detectionScope = @('HKLM/HKCU uninstall registrations', '32/64-bit App Paths', 'known executable locations and PATH', 'exact Calibre console siblings', 'process names; no window titles or documents')
    applications = @($applications); errors = @($errors.ToArray())
} | ConvertTo-Json -Depth 9 -Compress
