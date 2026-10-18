'use strict';
// Optional acceptance probe of an installed Word, not a product dependency.
// Only opens the explicitly provided owned QA document and writes new outputs.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),{spawnSync}=require('node:child_process');
const input=path.resolve(process.argv[2]||''),output=path.resolve(process.argv[3]||'');
if(!process.argv[2]||!process.argv[3]||!fs.statSync(input).isFile()||!fs.statSync(output).isDirectory())throw Error('Owned input and output directory are required');
const report={productRequiresOffice:false,application:'Microsoft Word',passed:false,inputSha256:crypto.createHash('sha256').update(fs.readFileSync(input)).digest('hex')};
const script=String.raw`
$ErrorActionPreference='Stop'
if(Get-Process WINWORD -ErrorAction SilentlyContinue){throw 'WORD_ACTIVE_INSTANCE'}
$word=$null
try {
 $word=New-Object -ComObject Word.Application
 $word.Visible=$false; $word.DisplayAlerts=0; $word.AutomationSecurity=3
 $document=$word.Documents.Open($env:READMD_WORD_INPUT,$false,$false)
 if($document.Sections.Count -ne 2){throw 'WORD_SECTIONS_MISMATCH'}
 if($document.Sections.Item(1).PageSetup.TextColumns.Count -ne 1){throw 'WORD_COVER_COLUMNS_MISMATCH'}
 if($document.Sections.Item(2).PageSetup.TextColumns.Count -ne 2){throw 'WORD_BODY_COLUMNS_MISMATCH'}
 $document.Fields.Update() | Out-Null
 $document.Repaginate()
 $pages=$document.ComputeStatistics(2)
 $document.SaveAs2((Join-Path $env:READMD_WORD_OUTPUT 'Word-roundtrip.docx'),16)
 $document.ExportAsFixedFormat((Join-Path $env:READMD_WORD_OUTPUT 'Word-layout.pdf'),17)
 $document.Close(0)
 $again=$word.Documents.Open((Join-Path $env:READMD_WORD_OUTPUT 'Word-roundtrip.docx'),$false,$true)
 if($again.Sections.Item(2).PageSetup.TextColumns.Count -ne 2){throw 'WORD_REOPEN_COLUMNS_MISMATCH'}
 $again.Close(0)
 [pscustomobject]@{passed=$true;pages=$pages;version=$word.Version} | ConvertTo-Json -Compress
} finally {if($word){$word.Quit();[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($word)}}
`;
const started=Date.now(),result=spawnSync('powershell.exe',['-NoProfile','-NonInteractive','-EncodedCommand',Buffer.from(script,'utf16le').toString('base64')],{
 windowsHide:true,encoding:'utf8',timeout:45000,env:{...process.env,READMD_WORD_INPUT:input,READMD_WORD_OUTPUT:output},maxBuffer:1024*1024,
});
report.elapsedMs=Date.now()-started;
if(result.status===0){Object.assign(report,JSON.parse(result.stdout.trim()));}
else {report.outcome='unverified';report.failure=result.error?.code==='ETIMEDOUT'?'Installed Word automation did not start or complete within 45 seconds':/WORD_ACTIVE_INSTANCE/.test(result.stderr)?'An existing user Word instance was preserved; probe did not run':/WORD_\w+_MISMATCH/.exec(result.stderr)?.[0]||'Installed Word COM activation or document processing failed';report.exitCode=result.status;const code=/0x[0-9A-Fa-f]{8}/.exec(result.stderr);if(code)report.code=code[0];}
report.recordedAt=new Date().toISOString();fs.writeFileSync(path.join(output,'word-probe.json'),JSON.stringify(report,null,2));console.log(JSON.stringify(report));if(!report.passed)process.exitCode=2;
