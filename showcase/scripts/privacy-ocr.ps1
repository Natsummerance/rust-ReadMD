param([Parameter(Mandatory=$true)][string]$InputDirectory,[Parameter(Mandatory=$true)][string]$OutputFile,[int]$Shard=0,[int]$Shards=1)
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Runtime.WindowsRuntime
$null=[Windows.Storage.StorageFile,Windows.Storage,ContentType=WindowsRuntime]
$null=[Windows.Graphics.Imaging.BitmapDecoder,Windows.Graphics.Imaging,ContentType=WindowsRuntime]
$null=[Windows.Media.Ocr.OcrEngine,Windows.Foundation,ContentType=WindowsRuntime]
$asTask=[System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object { $_.Name -eq 'AsTask' -and $_.IsGenericMethod -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1' } | Select-Object -First 1
function Await-Operation($operation,[Type]$type){$task=$asTask.MakeGenericMethod($type).Invoke($null,@($operation));$task.Wait();return $task.Result}
$engine=[Windows.Media.Ocr.OcrEngine]::TryCreateFromUserProfileLanguages()
if(!$engine){throw 'A local Windows OCR language pack is required'}
$suspicious='(?i)natsumer|\.codex|worktrees|\.runtime|appdata|roaming|[zct]:[\\/]|users[\\/]|antigravity-agent'
$total=0;$index=0;$findings=New-Object System.Collections.Generic.List[object]
foreach($file in Get-ChildItem -LiteralPath $InputDirectory -Filter '*.png' -File){
 $selected=($index % $Shards) -eq $Shard;$index++;if(!$selected){continue}
 $storage=Await-Operation ([Windows.Storage.StorageFile]::GetFileFromPathAsync($file.FullName)) ([Windows.Storage.StorageFile])
 $stream=Await-Operation ($storage.OpenReadAsync()) ([Windows.Storage.Streams.IRandomAccessStreamWithContentType])
 try{
  $decoder=Await-Operation ([Windows.Graphics.Imaging.BitmapDecoder]::CreateAsync($stream)) ([Windows.Graphics.Imaging.BitmapDecoder])
  $bitmap=Await-Operation ($decoder.GetSoftwareBitmapAsync()) ([Windows.Graphics.Imaging.SoftwareBitmap])
  try{$result=Await-Operation ($engine.RecognizeAsync($bitmap)) ([Windows.Media.Ocr.OcrResult])}finally{$bitmap.Dispose()}
  foreach($line in $result.Lines){if($line.Text -match $suspicious){
   $rects=@($line.Words | ForEach-Object { @{x=$_.BoundingRect.X;y=$_.BoundingRect.Y;width=$_.BoundingRect.Width;height=$_.BoundingRect.Height} })
   # Do not persist recognized private text; only frame IDs and redaction coordinates.
   $findings.Add(@{frame=$file.Name;rectangles=$rects})
  }}
 }finally{$stream.Dispose()}
 $total++
 if($total % 100 -eq 0){
  @{frames=$total;findings=$findings.ToArray();engine='Windows local OCR';text_persisted=$false} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $OutputFile -Encoding UTF8
  Write-Output "Inspected $total frames; $($findings.Count) private-path matches"
 }
}
@{frames=$total;findings=$findings.ToArray();engine='Windows local OCR';text_persisted=$false} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $OutputFile -Encoding UTF8
Write-Output "Inspected $total frames; $($findings.Count) private-path matches"
