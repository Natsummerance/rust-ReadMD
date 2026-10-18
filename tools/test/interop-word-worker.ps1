param([Parameter(Mandatory=$true)][string]$Workspace)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$root = [IO.Path]::GetFullPath($Workspace)
if (-not (Test-Path -LiteralPath (Join-Path $root 'interop-owner.json') -PathType Leaf)) { throw 'INTEROP_OWNED_WORKSPACE_REQUIRED' }
$inputFile = Join-Path $root 'source.docx'
$resultFile = Join-Path $root 'word-result.json'
$ownerFile = Join-Path $root 'word-owner.json'
$started = [DateTime]::UtcNow
$word = $null
$ownsWord = $false
$script:interopStage = 'preflight'
$result = [ordered]@{ passed = $false; acceptance = 'unverified'; stage = $script:interopStage; checks = @() }
function Write-JsonFile([string]$Destination, $Value) {
    [IO.File]::WriteAllText($Destination, ($Value | ConvertTo-Json -Depth 10), [Text.UTF8Encoding]::new($false))
}
function Set-Stage([string]$Value) {
    $script:interopStage = $Value
    Write-JsonFile $resultFile @{ passed = $false; acceptance = 'unverified'; stage = $Value }
}
function Snapshot-Document($Document) {
    $columns = @()
    for ($index = 1; $index -le $Document.Sections.Count; $index++) { $columns += $Document.Sections.Item($index).PageSetup.TextColumns.Count }
    return [pscustomobject]@{ math = $Document.OMaths.Count; sections = $Document.Sections.Count; columns = $columns; fields = $Document.Fields.Count; footnotes = $Document.Footnotes.Count; endnotes = $Document.Endnotes.Count }
}
try {
    if (@(Get-Process | Where-Object { $_.ProcessName -eq 'WINWORD' }).Count) { throw 'WORD_USER_INSTANCE_PRESERVED' }
    if (-not (Test-Path -LiteralPath $inputFile -PathType Leaf)) { throw 'WORD_FIXTURE_REQUIRED' }
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class ReadMdInteropWindow {
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
}
'@
    Set-Stage 'com-activation'
    Write-JsonFile $resultFile @{ passed = $false; acceptance = 'unverified'; stage = $script:interopStage }
    $word = New-Object -ComObject Word.Application
    [uint32]$wordProcessId = 0
    [void][ReadMdInteropWindow]::GetWindowThreadProcessId([IntPtr]$word.Hwnd, [ref]$wordProcessId)
    $process = Get-Process -Id $wordProcessId
    if ($process.ProcessName -ne 'WINWORD' -or $process.StartTime.ToUniversalTime() -lt $started) { throw 'WORD_OWNERSHIP_UNPROVEN' }
    if ($word.Documents.Count -ne 0) { throw 'WORD_INSTANCE_HAS_DOCUMENTS' }
    $ownsWord = $true
    Write-JsonFile $ownerFile @{ processId = $wordProcessId; processName = 'WINWORD'; startTicks = $process.StartTime.ToUniversalTime().Ticks.ToString(); workerStartedAt = $started.ToString('o') }
    $word.Visible = $false
    $word.DisplayAlerts = 0
    $word.AutomationSecurity = 3
    Set-Stage 'open'
    # Do not add fixtures to the user's Recent list or request repair mode.
    $document = $word.Documents.Open($inputFile, $false, $false, $false)
    $before = Snapshot-Document $document
    if ($before.math -lt 1) { throw 'WORD_EDITABLE_MATH_REQUIRED' }
    Set-Stage 'edit-equation'
    $equation = $document.OMaths.Item(1)
    $edited = $false
    for ($index = 1; $index -le $equation.Range.Characters.Count; $index++) {
        $character = $equation.Range.Characters.Item($index)
        if ($character.Text -cmatch '^[A-Za-z]$') {
            $oldCharacter = [string]$character.Text
            $replacement = if ($oldCharacter -ceq 'q') { 'r' } else { 'q' }
            $character.Text = $replacement
            $edited = $true
            break
        }
    }
    if (-not $edited) { throw 'WORD_FIXTURE_NEEDS_ASCII_MATH_IDENTIFIER' }
    $document.OMaths.Item(1).BuildUp()
    $expectedEquation = [string]$document.OMaths.Item(1).Range.Text
    if (-not $expectedEquation.Contains($replacement)) { throw 'WORD_EDIT_NOT_VISIBLE_IN_MATH' }
    Set-Stage 'update-fields'
    $fieldFailure = $document.Fields.Update()
    if ($fieldFailure -ne 0) { throw 'WORD_FIELD_UPDATE_FAILED' }
    $document.Repaginate()
    $pages = $document.ComputeStatistics(2)
    Set-Stage 'save'
    $roundtrip = Join-Path $root 'Word-roundtrip.docx'
    $document.SaveAs2($roundtrip, 16)
    $document.ExportAsFixedFormat((Join-Path $root 'Word-layout.pdf'), 17)
    $document.Close(0)
    Set-Stage 'reopen'
    $again = $word.Documents.Open($roundtrip, $false, $true, $false)
    $after = Snapshot-Document $again
    if (($before | ConvertTo-Json -Compress) -cne ($after | ConvertTo-Json -Compress)) { throw 'WORD_REOPEN_STRUCTURE_MISMATCH' }
    if ([string]$again.OMaths.Item(1).Range.Text -cne $expectedEquation) { throw 'WORD_REOPEN_EDIT_MISMATCH' }
    $again.Close(0)
    $result = [ordered]@{
        passed = $true; acceptance = 'partial_native_roundtrip'; stage = 'complete'; version = $word.Version; pages = $pages
        checks = @('existing OMML is editable', 'one math identifier changed in Word', 'fields updated without reported failure', 'saved and reopened in Word', 'edited equation persisted', 'math/section/column/field/note counts retained', 'Word paginated and exported PDF')
        before = $before; after = $after
        remaining = @('all 19 objects and properties', 'visual and repair-dialog inspection', 'ReadMD return import', 'Word tracked changes', 'WPS/LibreOffice and independent ebook readers')
    }
} catch {
    $code = [regex]::Match($_.Exception.Message, '(?:WORD|INTEROP)_[A-Z_]+').Value
    $result = [ordered]@{ passed = $false; acceptance = 'unverified'; stage = $script:interopStage; failure = $(if ($code) { $code } else { 'WORD_AUTOMATION_ERROR' }); hresult = ('0x{0:X8}' -f $_.Exception.HResult) }
} finally {
    if ($word -and $ownsWord) {
        try { $word.Quit(0) } catch { $result.cleanup = 'OWNED_WORD_QUIT_FAILED' }
    }
    if ($word) { try { [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($word) } catch { } }
    $result.recordedAt = [DateTime]::UtcNow.ToString('o')
    Write-JsonFile $resultFile $result
}
if (-not $result.passed) { exit 2 }
