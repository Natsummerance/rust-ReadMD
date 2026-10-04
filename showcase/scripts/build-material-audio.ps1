$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Speech
$speaker=[System.Speech.Synthesis.SpeechSynthesizer]::new()
try {
 $speaker.Rate=-1
 $speaker.SetOutputToWaveFile([IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../materials/reading-discussion.wav')))
 $speaker.Speak('Evidence and reasoning. Observations are the starting point of inquiry. Record the premises, compare explanations, and test your conclusion with a counterexample.')
} finally {$speaker.Dispose()}
Write-Output 'Created original reading discussion audio'
