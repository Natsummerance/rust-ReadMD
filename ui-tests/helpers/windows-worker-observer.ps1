param([Parameter(Mandatory=$true)][string]$QaRoot,[Parameter(Mandatory=$true)][uint32]$NodeProcessId,[Parameter(Mandatory=$false)][string]$QaRootPrefix)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
[Console]::InputEncoding = [System.Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
Add-Type -Path (Join-Path $PSScriptRoot 'windows-worker-observer.cs')
$base = Join-Path (Split-Path (Split-Path $PSScriptRoot -Parent) -Parent) '.cache/capability-upgrade'
if ($QaRootPrefix) { $observer = [ReadMDWorkerObserver]::new($QaRoot,$NodeProcessId,$base,$QaRootPrefix) }
else { $observer = [ReadMDWorkerObserver]::new($QaRoot,$NodeProcessId,$base) }
try {
    while ($null -ne ($line = [Console]::ReadLine())) {
        if ($line.Length -gt 4194304) { throw 'Observer input exceeds protocol budget' }
        $request = ConvertFrom-Json -InputObject $line
        try {
            $arguments = $request.params
            $result = switch ($request.action) {
                'ready' { $observer.Ready([string]$arguments.echo) }
                'arm' { $ownerParentPid = 0; if ($null -ne $arguments -and $arguments.PSObject.Properties['ownerParentPid']) { $ownerParentPid = [uint32]$arguments.ownerParentPid }; $observer.Arm([uint32]$arguments.ownerPid,[string]$arguments.program,[string]$arguments.ownerProgram,[string]$arguments.kind,[string]$arguments.sha256,$ownerParentPid) }
                'observe' { $observer.Observe([string]$arguments.key) }
                'alive' { $observer.IsAlive([string]$arguments.key) }
                'freeSlots' { $observer.FreeSlots([string]$arguments.key) }
                'jobAccounting' { $observer.JobAccounting([string]$arguments.key) }
                'lease' { $observer.Lease([string]$arguments.key,[string]$arguments.resource) }
                'resume' { $observer.Resume([string]$arguments.key) }
                'stopped' { $timeout = 2000; if ($arguments.PSObject.Properties['timeoutMs']) { $timeout = [int]$arguments.timeoutMs }; $observer.Stopped([string]$arguments.key,$timeout) }
                default { throw 'Unknown observer action' }
            }
            $response = @{id=$request.id;result=$result}
        } catch { $response = @{id=$request.id;error=$_.Exception.ToString()} }
        [Console]::WriteLine((ConvertTo-Json -InputObject $response -Depth 12 -Compress))
    }
} finally { $observer.Dispose() }
