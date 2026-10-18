param()
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
    @{ id = 'libreoffice'; name = 'LibreOffice Writer'; install = 'LibreOffice'; executables = @('soffice.exe'); processes = @('soffice', 'soffice.bin'); relative = @('LibreOffice\program\soffice.exe') },
    @{ id = 'thorium'; name = 'Thorium Reader'; install = 'Thorium'; executables = @('Thorium.exe'); processes = @('Thorium'); relative = @('Programs\Thorium\Thorium.exe', 'Thorium\Thorium.exe') },
    @{ id = 'calibre'; name = 'Calibre Ebook Viewer'; install = 'Calibre'; executables = @('ebook-viewer.exe'); processes = @('ebook-viewer'); relative = @('Calibre2\ebook-viewer.exe', 'Calibre\ebook-viewer.exe') }
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
    $found = [Collections.Generic.List[object]]::new()
    foreach ($candidate in ($candidates | Select-Object -Unique)) {
        try {
            if (Test-Path -LiteralPath $candidate -PathType Leaf) {
                $file = Get-Item -LiteralPath $candidate
                $found.Add([pscustomobject]@{ fileName = $file.Name; version = $file.VersionInfo.FileVersion; bytes = $file.Length })
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
    detectionScope = @('HKLM/HKCU uninstall registrations', '32/64-bit App Paths', 'known executable locations', 'process names; no window titles or documents')
    applications = @($applications); errors = @($errors.ToArray())
} | ConvertTo-Json -Depth 9 -Compress
