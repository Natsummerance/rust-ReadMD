param(
    [Parameter(Mandatory=$true)][int]$NativeProcessId,
    [Parameter(Mandatory=$true)][string]$Destination,
    [Parameter(Mandatory=$true)][string]$OwnedDocuments
)
$ErrorActionPreference='Stop'
$resolvedDestination=[IO.Path]::GetFullPath($Destination)
$resolvedDocuments=[IO.Path]::GetFullPath($OwnedDocuments).TrimEnd('\')+'\'
if(-not $resolvedDestination.StartsWith($resolvedDocuments,[StringComparison]::OrdinalIgnoreCase)) { throw 'Save destination is outside owned QA documents' }
if(Test-Path -LiteralPath $resolvedDestination) { throw 'QA save destination already exists' }
if(-not (Get-Process -Id $NativeProcessId -ErrorAction Stop)) { throw 'Owned native process is missing' }
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$processCondition=New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ProcessIdProperty,$NativeProcessId)
$classCondition=New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ClassNameProperty,'#32770')
$dialogCondition=New-Object System.Windows.Automation.AndCondition($processCondition,$classCondition)
function Find-OwnedDialog([System.Windows.Automation.Condition]$ownerCondition,[System.Windows.Automation.Condition]$ownedDialogCondition) {
    # UIA can nest a modal IFileDialog under its disabled owner, rather than
    # exposing it as a desktop child. Search only this process's owned trees.
    $ownedRoots=[System.Windows.Automation.AutomationElement]::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children,$ownerCondition)
    foreach($ownedRoot in $ownedRoots) {
        if($ownedRoot.Current.ClassName -eq '#32770') { return $ownedRoot }
        $nested=$ownedRoot.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$ownedDialogCondition)
        if($nested) { return $nested }
    }
    return $null
}
$deadline=[DateTime]::UtcNow.AddSeconds(20)
$dialog=$null
while([DateTime]::UtcNow -lt $deadline) {
    $dialog=Find-OwnedDialog $processCondition $dialogCondition
    if(-not $dialog) {
        # ReadMD can fall back to a direct, owned WinForms helper when COM is unavailable.
        $fallbacks=Get-CimInstance Win32_Process -Filter ("ParentProcessId="+$NativeProcessId) | Where-Object {$_.Name -eq 'powershell.exe'}
        foreach($fallback in $fallbacks) {
            $fallbackCondition=New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ProcessIdProperty,([int]$fallback.ProcessId))
            $ownedFallback=New-Object System.Windows.Automation.AndCondition($fallbackCondition,$classCondition)
            $dialog=Find-OwnedDialog $fallbackCondition $ownedFallback
            if($dialog) { break }
        }
    }
    if($dialog) { break }
    Start-Sleep -Milliseconds 100
}
if(-not $dialog) {
    $ownedWindows=[System.Windows.Automation.AutomationElement]::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children,$processCondition)
    $classes=@($ownedWindows | ForEach-Object {$_.Current.ClassName}) -join ','
    throw ('Owned native Save As dialog did not appear; owned window classes: '+$classes)
}
# The Windows IFileDialog filename edit and Save button have stable automation IDs.
$fileCondition=New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::AutomationIdProperty,'1001')
$fileEdit=$dialog.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$fileCondition)
if(-not $fileEdit) { throw 'Owned Save As filename field was not found' }
$valuePattern=$fileEdit.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
$valuePattern.SetValue($resolvedDestination)
$saveCondition=New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::AutomationIdProperty,'1')
$saveButton=$dialog.FindFirst([System.Windows.Automation.TreeScope]::Children,$saveCondition)
if(-not $saveButton) { $saveButton=$dialog.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$saveCondition) }
if(-not $saveButton) { throw 'Owned Save As button was not found' }
$invokePattern=$saveButton.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
$invokePattern.Invoke()
Write-Output 'Owned native Save As dialog accepted.'
