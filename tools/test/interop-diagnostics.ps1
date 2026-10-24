param([Parameter(Mandatory=$true)]$Inventory, [int]$BudgetMs = 15000)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$clock = [Diagnostics.Stopwatch]::StartNew()
$errors = [Collections.Generic.List[object]]::new()
$com = [Collections.Generic.List[object]]::new()
$office = [Collections.Generic.List[object]]::new()
$script:budgetExceeded = $false
function Assert-Budget {
    if ($clock.ElapsedMilliseconds -ge $BudgetMs) { $script:budgetExceeded = $true; throw 'INTEROP_DIAGNOSTIC_BUDGET' }
}
function Read-Key($Base, [string]$Name) {
    Assert-Budget
    return $Base.OpenSubKey($Name, $false)
}
function Executable-FromCommand([string]$Value) {
    if (-not $Value) { return $null }
    $expanded = [Environment]::ExpandEnvironmentVariables($Value.Trim())
    if ($expanded -match '^"([^"]+\.(?:exe|com))"') { return $Matches[1] }
    if ($expanded -match '^(.+?\.(?:exe|com))(?=\s|,|$)') { return $Matches[1] }
    return $null
}
function Read-ComRegistration($Base, [string]$Scope, [string]$Prefix, [string]$ProgId) {
    $curVerKey = Read-Key $Base "$Prefix$ProgId\CurVer"
    $curVer = $null
    if ($curVerKey) { try { $curVer = [string]$curVerKey.GetValue('') } finally { $curVerKey.Dispose() } }
    if ($curVer -and $curVer -notmatch '^[A-Za-z0-9_.-]{1,128}$') { $curVer = '[invalid-progid]' }
    $key = Read-Key $Base "$Prefix$ProgId\CLSID"
    $clsid = $null
    if ($key) { try { $clsid = [string]$key.GetValue('') } finally { $key.Dispose() } }
    if (-not $clsid -or $clsid -notmatch '^\{[0-9A-Fa-f-]{36}\}$') {
        return [pscustomobject]@{ scope = $Scope; progId = $ProgId; clsidRegistered = $false; localServerRegistered = $false; curVer = $curVer }
    }
    $server = Read-Key $Base "${Prefix}CLSID\$clsid\LocalServer32"
    $serverCommand = $null
    if ($server) { try { $serverCommand = [string]$server.GetValue('') } finally { $server.Dispose() } }
    $treatAs = Read-Key $Base "${Prefix}CLSID\$clsid\TreatAs"
    $hasTreatAs = [bool]$treatAs
    if ($treatAs) { $treatAs.Dispose() }
    $inproc = Read-Key $Base "${Prefix}CLSID\$clsid\InprocServer32"
    $hasInprocKey = [bool]$inproc
    $hasInproc = [bool]($inproc -and [string]$inproc.GetValue(''))
    if ($inproc) { $inproc.Dispose() }
    $exe = Executable-FromCommand $serverCommand
    return [pscustomobject]@{
        scope = $Scope; progId = $ProgId; clsid = $clsid; clsidRegistered = $true; localServerRegistered = [bool]$serverCommand
        serverExecutableName = $(if ($exe) { [IO.Path]::GetFileName($exe) } else { $null })
        serverExecutableExists = [bool]($exe -and [IO.File]::Exists($exe))
        hasAutomationSwitch = [bool]($serverCommand -match '(?i)/(?:automation|embedding)\b'); treatAsRegistered = $hasTreatAs; inprocServerKeyPresent = $hasInprocKey; inprocServerRegistered = $hasInproc; curVer = $curVer
    }
}
try {
    foreach ($viewName in @('Registry64', 'Registry32')) {
        foreach ($hiveName in @('LocalMachine', 'CurrentUser', 'ClassesRoot')) {
            Assert-Budget
            $base = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::$hiveName, [Microsoft.Win32.RegistryView]::$viewName)
            $scope = "$hiveName/$viewName"
            try {
                $prefix = $(if ($hiveName -eq 'ClassesRoot') { '' } else { 'SOFTWARE\Classes\' })
                foreach ($progId in @('Word.Application', 'Word.Application.16')) { $com.Add((Read-ComRegistration $base $scope $prefix $progId)) }
                if ($hiveName -eq 'ClassesRoot') { continue }
                $categories = @(
                    @{ version = 'not-versioned'; suffix = 'Word\Addins'; key = 'SOFTWARE\Microsoft\Office\Word\Addins' }
                )
                foreach ($officeVersion in @('16.0', '15.0')) {
                    foreach ($suffix in @('Word\Resiliency\DisabledItems', 'Word\Resiliency\CrashingAddinList', 'Common\General', 'FirstRun')) {
                        $categories += @{ version = $officeVersion; suffix = $suffix; key = "SOFTWARE\Microsoft\Office\$officeVersion\$suffix" }
                    }
                }
                foreach ($category in $categories) {
                    $key = Read-Key $base $category.key
                    if (-not $key) {
                        $office.Add([pscustomobject]@{ scope = $scope; officeVersion = $category.version; category = $category.suffix; present = $false })
                        continue
                    }
                    try {
                        $entry = [ordered]@{ scope = $scope; officeVersion = $category.version; category = $category.suffix; present = $true; valueCount = $key.ValueCount; subkeyCount = $key.SubKeyCount }
                        if ($category.suffix -eq 'Word\Addins') {
                            $entry.addins = @($key.GetSubKeyNames() | Select-Object -First 128 | ForEach-Object {
                                $addin = $key.OpenSubKey($_, $false)
                                try {
                                    $loadBehavior = $addin.GetValue('LoadBehavior')
                                    [pscustomobject]@{ registered = $true; loadBehavior = $(if ($loadBehavior -is [int] -or $loadBehavior -is [long]) { $loadBehavior } else { $null }) }
                                } finally { if ($addin) { $addin.Dispose() } }
                            })
                        }
                        # Retain only numeric flags and counts, never account/document values or add-in paths/names.
                        $flags = [ordered]@{}
                        foreach ($name in @('ShownFirstRunOptin', 'DisableBootToOfficeStart', 'BootedRTM', 'FirstRun')) {
                            $value = $key.GetValue($name)
                            if ($value -is [int] -or $value -is [long]) { $flags[$name] = $value }
                        }
                        if ($flags.Count) { $entry.numericFlags = $flags }
                        $office.Add([pscustomobject]$entry)
                    } finally { $key.Dispose() }
                }
            } finally { $base.Dispose() }
        }
    }
} catch { $errors.Add([pscustomobject]@{ operation = 'bounded-com-configuration'; type = $_.Exception.GetType().Name }) }
foreach ($application in $Inventory.applications) { $application | Add-Member -NotePropertyName nativeAcceptance -NotePropertyValue 'not_run' -Force }
$runtime = [Diagnostics.Process]::GetCurrentProcess()
[pscustomobject]@{
    schemaVersion = 1; mode = 'read-only-static-diagnostics'; recordedAt = [DateTime]::UtcNow.ToString('o')
    workerArchitectureBits = [IntPtr]::Size * 8; runtimeExecutableName = [IO.Path]::GetFileName($runtime.MainModule.FileName); runtimeExecutableVersion = $runtime.MainModule.FileVersionInfo.FileVersion
    runtimeIs64BitProcess = [Environment]::Is64BitProcess; runtimeIs64BitOperatingSystem = [Environment]::Is64BitOperatingSystem
    elapsedMs = $clock.ElapsedMilliseconds; budgetMs = $BudgetMs; budgetExceeded = $script:budgetExceeded
    detectionScope = @($Inventory.detectionScope); officeComRegistration = @($com.ToArray()); officeConfiguration = @($office.ToArray())
    applications = @($Inventory.applications); errors = @($Inventory.errors) + @($errors.ToArray())
    limits = @('Bounded registrations, PATH and known locations only; no whole-disk search.', 'Registry flags and registrations cannot prove a first-run dialog, add-in failure or activation cause.', 'No COM activation, windows, documents, user profiles or command lines are inspected.', 'ClassesRoot records are the current merged registration view; no server or TreatAs target is activated.')
} | ConvertTo-Json -Depth 12 -Compress
