param([Parameter(Mandatory=$true)][int]$ProcessId, [Parameter(Mandatory=$true)][string]$StartTicks)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$process = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
if (-not $process) {
    @{ state = 'already_exited' } | ConvertTo-Json -Compress
    exit 0
}
if ($process.ProcessName -ne 'WINWORD' -or $process.StartTime.ToUniversalTime().Ticks.ToString() -cne $StartTicks) {
    @{ state = 'ownership_mismatch'; action = 'No process terminated.' } | ConvertTo-Json -Compress
    exit 0
}
Stop-Process -Id $ProcessId -ErrorAction Stop
@{ state = 'owned_process_stopped'; processId = $ProcessId } | ConvertTo-Json -Compress
