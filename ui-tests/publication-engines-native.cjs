'use strict';
// Root's unique candidate, real production CSP/services, existing offline assets.
const fs = require('node:fs'), path = require('node:path'), net = require('node:net'), assert = require('node:assert/strict');
const {spawnSync} = require('node:child_process');
const {createHarness, fileHash, sha256} = require('../tools/test/publication-batch3-harness.cjs');
const {paperMetadata, sourceFor, references, richStyle} = require('../tools/test/publication-batch3-fixtures.cjs');
const {inspectArtifact, checkMetadataArtifact, checkRichArtifact, visibleTokens} = require('../tools/test/publication-batch3-checks.cjs');
const {runtimeMaterialization, verifyRuntimeMaterialization} = require('../tools/test/publication-runtime-reuse.cjs');
const repository = path.resolve(__dirname, '..');
const receiptFile = path.resolve(process.env.READMD_TEX_QA_FIXTURE || path.join(repository, '.cache/capability-upgrade/tex-managed-fixture-07Z3UL'), 'receipt.json');
const receipt = JSON.parse(fs.readFileSync(receiptFile, 'utf8'));
assert(receipt.qaOnly && receipt.redistributionApproved === false && !receipt.published);
const runtimeSource = path.resolve(process.env.READMD_TEX_RUNTIME_SOURCE || path.join(repository, '.cache/capability-upgrade/tex-inputs/publication-runtime'));
const materializationPlanFile = path.join(repository, '.cache/capability-upgrade/publication-context-inputs/engines-fresh-runtime-construction-plan.safe.json');
const materializationPlanSha256 = 'aaac9a3f6d2b6156e8844b99484770eea5f4df743a79d0df146c01ee139a869f';
const originalAclAuditSha256 = '1ee11114b6f1bd4ceeab36de04c9c563eba8d76a7f73115cdb4e51e9f939d8dc';
const originalEngineReport = '.cache/capability-upgrade/publication-engines-native-1QHUGo/report.json';
const originalEngineReportSha256 = '2bba05ed029dc2d50e3c1378fae4f574c50fea9071f7cbd64dcffa9c52964088';
const reusableRuntimeProfile = '.cache/capability-upgrade/publication-engines-native-hZuzm8';
const reusableRuntimeReportSha256 = 'ac1c2017bb7de863bf0fb165897d61a8d4199849adc76bccad9ff8093d5cf250';
const engineReuseProfile = '.cache/capability-upgrade/publication-engines-native-n2e7Ug';
const engineReuseReportSha256 = 'dd4fb8c4420c92a29a141ba3e7847b056f9021c8ee8f51b820dc8198742c1052';
const engineReuseCatalogSha256 = 'b4048c82dc3e5b933ba93912f462bea652f27ab4a543ee05ba4de94f7d6b3dfb';
const engineReuseBootSha256 = '6ac2524a326be04b8280aed674b8ebf1cf9cdae53cc921c00975133fbf14185e';
function checkNativeTexMetadata(text, tokens, assetsDirectory) {
  const fontSha256 = '7da195a74c55bef988d0d48f9508bd5d849425c1770dba5d7bfc6ce9ed848954';
  const licenseSha256 = '7a083b136e64d064794c3419751e5c7dd10d2f64c108fe5ba161eae5e5958a93';
  let supplementalCharacters = 0;
  for (const token of tokens) {
    assert.equal(typeof token, 'string', 'Native metadata token must be explicit original text');
    // Preserve the original BMP/raw-TeX assertion. Only actual SMP scalars use
    // the writer's exact controlled fallback command; no macro stripping.
    const expected = [...token].map(character => {
      if (character.codePointAt(0) <= 0xFFFF) return character;
      supplementalCharacters++; return '\\readmdunicode{' + character + '}';
    }).join('');
    assert(text.includes(expected), 'Standalone TeX dropped shared metadata: ' + token);
  }
  if (!supplementalCharacters) return {tokens: tokens.length, supplementalCharacters};
  assert(text.includes('\\DeclareRobustCommand{\\readmdunicode}[1]{{\\normalfont\\readmdcorefallback #1}}'), 'SMP text has no controlled fallback command');
  const fontName = 'readmd-core-' + fontSha256 + '.ttf', licenseName = fontName.slice(0, -4) + '.LICENSE.txt';
  const resources = [[fontName, fontSha256, 757076], [licenseName, licenseSha256, 8816]].map(([name, expectedSha256, bytes]) => {
    const file = path.join(assetsDirectory, name), stat = fs.lstatSync(file);
    assert(stat.isFile() && !stat.isSymbolicLink(), 'Native core font/license is not an actual plain file');
    assert.equal(stat.size, bytes, 'Native core font/license byte count changed');
    const digest = fileHash(file); assert.equal(digest, expectedSha256, 'Native core font/license differs from its independent source lock');
    return {file: name, sha256: digest, bytes};
  });
  return {tokens: tokens.length, supplementalCharacters, controlledMacro: 'readmdunicode', resources};
}
function readSmallcapsProof(h, file, family, fontAudit, proofFile) {
  const stdoutFile = proofFile + '.stdout.log', stderrFile = proofFile + '.stderr.log', commandFile = proofFile + '.command.json';
  for (const target of [proofFile, stdoutFile, stderrFile, commandFile]) assert(!fs.existsSync(target), 'Preserve first small-caps command evidence: ' + target);
  const args = [path.join(h.root, 'tools/test/publication-smallcaps-proof.py'), 'prove', file,
    '--family', family, '--font-audit', fontAudit, '--report', proofFile];
  // Match the harness command budget and retain raw output even on failure.
  // The written proof is the protocol; third-party stdout is only a log.
  const result = spawnSync('python', args, {encoding: 'utf8', windowsHide: true, timeout: 90000, maxBuffer: 32 * 1024 * 1024});
  fs.writeFileSync(stdoutFile, result.stdout || '', {flag: 'wx'});
  fs.writeFileSync(stderrFile, result.stderr || '', {flag: 'wx'});
  const command = {status: result.status, signal: result.signal, error: result.error ? String(result.error) : null,
    stdoutLog: path.basename(stdoutFile), stdoutSha256: fileHash(stdoutFile),
    stderrLog: path.basename(stderrFile), stderrSha256: fileHash(stderrFile)};
  fs.writeFileSync(commandFile, JSON.stringify(command, null, 2) + '\n', {flag: 'wx'});
  assert.equal(result.status, 0, `python failed: ${String(result.error || result.stderr).slice(0, 1200)}`);
  return {proof: JSON.parse(fs.readFileSync(proofFile, 'utf8')), command: {...command,
    file: path.basename(commandFile), sha256: fileHash(commandFile)}};
}
// Read real file identities and binary SDs in one Win32 batch per phase. Cache
// decoding by SD hash; never translate account names or emit raw SD/personal SID.
const aclReaderScript = String.raw`param([Parameter(Mandatory=$true)][string]$Plan,[Parameter(Mandatory=$true)][string]$SourceRoot,[string]$TargetRoot='')
$ErrorActionPreference='Stop'
Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Security.AccessControl;
using System.Security.Cryptography;
public static class ReadMDQaWin32Metadata {
  [StructLayout(LayoutKind.Sequential)] private struct FileInfo {
    public uint Attributes, CreationLow, CreationHigh, AccessLow, AccessHigh, WriteLow, WriteHigh;
    public uint Volume, SizeHigh, SizeLow, Links, IndexHigh, IndexLow;
  }
  public class Descriptor {public string hash; public bool isProtected; public long allow, deny;}
  public class FileRecord {public string sdSha256, identity; public bool @protected; public long allAppAllow, allAppDeny, bytes; public uint links;}
  public class Row {public string path; public FileRecord source, target;}
  private static readonly Dictionary<string, Descriptor> Cache = new Dictionary<string, Descriptor>();
  public static int UniqueDescriptors {get {return Cache.Count;}}
  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] private static extern IntPtr CreateFile(string path,uint access,uint share,IntPtr security,uint creation,uint flags,IntPtr template);
  [DllImport("kernel32.dll", SetLastError=true)] private static extern bool GetFileInformationByHandle(IntPtr handle,out FileInfo info);
  [DllImport("kernel32.dll")] private static extern bool CloseHandle(IntPtr handle);
  [DllImport("kernel32.dll")] private static extern IntPtr LocalFree(IntPtr value);
  [DllImport("advapi32.dll")] private static extern uint GetSecurityInfo(IntPtr handle,int type,uint requested,out IntPtr owner,out IntPtr group,out IntPtr dacl,out IntPtr sacl,out IntPtr descriptor);
  [DllImport("advapi32.dll")] private static extern uint GetSecurityDescriptorLength(IntPtr descriptor);
  private static FileRecord Read(string path) {
    // READ_CONTROL | FILE_READ_ATTRIBUTES, OPEN_EXISTING, OPEN_REPARSE_POINT.
    IntPtr handle=CreateFile(path,0x20080,7,IntPtr.Zero,3,0x200000,IntPtr.Zero);
    if(handle==new IntPtr(-1))throw new Win32Exception(Marshal.GetLastWin32Error());
    try {
      FileInfo info;if(!GetFileInformationByHandle(handle,out info))throw new Win32Exception(Marshal.GetLastWin32Error());
      if((info.Attributes&0x400)!=0)throw new InvalidDataException("A runtime resource is a reparse point");
      IntPtr owner,group,dacl,sacl,sd;
      uint error=GetSecurityInfo(handle,1,7,out owner,out group,out dacl,out sacl,out sd);
      if(error!=0)throw new Win32Exception((int)error);
      try {
        byte[] binary=new byte[GetSecurityDescriptorLength(sd)];Marshal.Copy(sd,binary,0,binary.Length);
        string hash;using(SHA256 hasher=SHA256.Create()){hash=BitConverter.ToString(hasher.ComputeHash(binary)).Replace("-","").ToLowerInvariant();}
        Descriptor parsed;
        if(!Cache.TryGetValue(hash,out parsed)) {
          RawSecurityDescriptor raw=new RawSecurityDescriptor(binary,0);
          parsed=new Descriptor {hash=hash,isProtected=(raw.ControlFlags&ControlFlags.DiscretionaryAclProtected)!=0};
          foreach(GenericAce generic in raw.DiscretionaryAcl) {
            CommonAce ace=generic as CommonAce;if(ace==null)throw new InvalidDataException("Unsupported ACL entry type");
            if(ace.SecurityIdentifier.Value!="S-1-15-2-1"||(ace.AceFlags&AceFlags.InheritOnly)!=0)continue;
            long mask=unchecked((uint)ace.AccessMask);
            if(ace.AceQualifier==AceQualifier.AccessAllowed)parsed.allow|=mask;
            else if(ace.AceQualifier==AceQualifier.AccessDenied)parsed.deny|=mask;
            else throw new InvalidDataException("Unsupported runtime SID ACL qualifier");
          }
          Cache.Add(hash,parsed);
        }
        return new FileRecord {sdSha256=hash,identity=info.Volume.ToString("x8")+info.IndexHigh.ToString("x8")+info.IndexLow.ToString("x8"),
          @protected=parsed.isProtected,allAppAllow=parsed.allow,allAppDeny=parsed.deny,links=info.Links,bytes=((long)info.SizeHigh<<32)|info.SizeLow};
      } finally {LocalFree(sd);}
    } finally {CloseHandle(handle);}
  }
  public static Row[] Scan(string[] paths,string sourceRoot,string targetRoot) {
    List<Row> rows=new List<Row>(paths.Length);
    foreach(string relative in paths) {
      if(Path.IsPathRooted(relative)||Array.IndexOf(relative.Split('/','\\'),"..")>=0)throw new InvalidDataException("Invalid locked path");
      string source=Path.Combine(sourceRoot,relative),target=String.IsNullOrEmpty(targetRoot)?null:Path.Combine(targetRoot,relative);
      rows.Add(new Row {path=relative,source=Read(source),target=target!=null&&File.Exists(target)?Read(target):null});
    }
    return rows.ToArray();
  }
}
'@
$Specification=Get-Content -LiteralPath $Plan -Raw | ConvertFrom-Json
$Paths=[string[]]@($Specification.files | ForEach-Object {$_.path})
$Rows=[ReadMDQaWin32Metadata]::Scan($Paths,$SourceRoot,$TargetRoot)
ConvertTo-Json -InputObject ([ordered]@{rows=$Rows;uniqueDescriptors=[ReadMDQaWin32Metadata]::UniqueDescriptors}) -Depth 6 -Compress
`;
const body = '# Engine evidence\n\nRead [see @study, p. 42 and *passim*].\n\nNarrative @study [p. 43] supports the claim.\n\nAuthor :cite[author-only cf. @study3].\n\nManual note[^original].\n\n[^original]: Original author note with [@study2, chap. 7].\n';
// The fixture's source language remains independent of every UI locale switch.
const paper = sourceFor(paperMetadata, body).replace('csl: apa', 'csl: rich.csl');
const bib = '@book{study,title={Synthetic Evidence},author={Example, Ada},year={2020},publisher={Example Press},url={https://example.invalid/study}}\n' +
  '@book{study2,title={Second Evidence},author={Example, Ada},year={2020},publisher={Example Press}}\n' +
  '@book{study3,title={Third Evidence},author={Other, Bea},year={2021},publisher={Example Press}}\n';
const nativeBody = 'Normal [see @study, chap. 4 with *passim*].\n\nNarrative @study [p. 8].\n\nAuthor :cite[author-only cf. @study3].\n';
const nativeSource = sourceFor(paperMetadata, nativeBody).replace('references.json', 'references.bib');
const unsupportedNativeCases = [
  ['biblatex', nativeSource.replace(nativeBody, '[-@study, p. 8]\n')],
  ['biblatex', nativeSource.replace(nativeBody, '[see @study; -@study2]\n')],
  ['natbib', nativeSource.replace(nativeBody, '[see @study, p. 8; cf. @study2, chap. 4]\n')],
  ['bibtex', nativeSource],
  ['biblatex', nativeSource.replace('references.bib', 'references.json')],
  ['biblatex', nativeSource.replace(nativeBody, ':cite[author-only cf. @study; see @study2]\n')],
  ['biblatex', nativeSource.replace(nativeBody, ':cite[author-only @study after; @study2 elsewhere]\n')],
  ['biblatex', nativeSource.replace(nativeBody, ':cite[author-only *cf.* @study; **cf.** @study2]\n')],
  ['biblatex', nativeSource.replace(nativeBody, ':cite[author-only @study *after*; @study2 **after**]\n')],
];
const lightSource = sourceFor(paperMetadata, 'Light citation [@study].\n').replace('csl: apa', 'csl: light.csl');
const lightStyle = '<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="in-text"><info><title>ReadMD original Light unsupported-output evidence</title><id>urn:readmd:qa:light-weight</id></info><citation><layout><text variable="title" font-weight="light"/></layout></citation></style>';
const richWithLocator = richStyle.replace('<text variable="URL"/></group></layout></citation>', '<text variable="locator"/><text variable="URL"/></group></layout></citation>');
const baseOptions = {typography: {font: 'Arial', size: 11}, cover: {enabled: false}, toc: {enabled: false}, footer: {pageNumbers: false}, frontMatter: {mode: 'paper'}, tex: {docClass: 'article', useCtex: true}};
const capsFaces = [['R', 'book', ''], ['B', 'chapter', ' font-weight="bold"'], ['I', 'report', ' font-style="italic"'], ['BI', 'article', ' font-weight="bold" font-style="italic"']];
const capsReferences = capsFaces.flatMap(([face, type]) => ['NORMAL', 'SMALL', 'UPPER'].map(mode => ({
  id: face + '-' + mode, type, title: mode === 'UPPER' ? 'ABCXYZ' : 'Abcxyz', genre: face + '_' + mode,
  ...(mode === 'SMALL' ? {abstract: 'Select the explicit small-caps formatting branch.'} : {}),
})));
const capsStyle = '<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="in-text"><info><title>ReadMD original physical small-caps QA</title><id>urn:readmd:qa:physical-smallcaps</id><updated>2026-10-14T00:00:00Z</updated></info><citation><layout><choose>' +
  capsFaces.map(([face, type, format], index) => '<' + (index ? 'else-if' : 'if') + ' type="' + type + '"><group' + format + ' delimiter=" "><text variable="genre"/><choose><if variable="abstract"><text variable="title" font-variant="small-caps"/></if><else><text variable="title"/></else></choose></group></' + (index ? 'else-if' : 'if') + '>').join('') +
  '</choose></layout></citation></style>';
const capsSource = '---\ntitle: Physical small-caps evidence\nbibliography: smallcaps-references.json\ncsl: smallcaps.csl\nlanguage: en-US\n---\n\n' + capsReferences.map(item => '[@' + item.id + ']').join('\n\n') + '\n';
function hardlinkTree(source, target) {
  fs.mkdirSync(target, {recursive: true});
  for (const entry of fs.readdirSync(source, {withFileTypes: true})) {
    const from = path.join(source, entry.name), to = path.join(target, entry.name);
    assert(!entry.isSymbolicLink(), 'QA assets/runtime must be real reviewed files');
    if (entry.isDirectory()) hardlinkTree(from, to);
    else if (entry.isFile()) fs.linkSync(from, to);
  }
}
function resolveEngineReuseProfile(value) {
  if (value === undefined) return undefined;
  assert(typeof value === 'string' && path.isAbsolute(value), 'READMD_ENGINE_REUSE_PROFILE must be the explicit absolute reviewed owned profile');
  const profile = path.resolve(value), expected = path.join(repository, engineReuseProfile);
  assert.equal(profile.toLowerCase(), expected.toLowerCase(), 'Only the fixed reviewed n2e7Ug engine profile may be reused');
  for (let file = profile; ; file = path.dirname(file)) {
    const stat = fs.lstatSync(file);
    assert(stat.isDirectory() && !stat.isSymbolicLink(), 'A reused profile ancestor is not an ordinary directory');
    if (path.dirname(file) === file) break;
  }
  return profile;
}
function verifyEngineProfileReuse(h, value) {
  const profile = resolveEngineReuseProfile(value);
  assert(profile, 'An explicit reuse profile is required');
  const reportFile = path.join(profile, 'report.json');
  const quotePowerShell = value => "'" + value.replaceAll("'", "''") + "'";
  const paths = [profile, reportFile, path.join(profile, 'assets'), path.join(profile, 'assets/capabilities'),
    path.join(profile, 'documents'), path.join(profile, 'outputs'),
    path.join(profile, 'assets/readmd.boot.js'), path.join(profile, 'assets/capabilities/catalog.json'),
    path.join(profile, 'assets/capabilities/latex-2026.10-r1-windows-x64.lock.json'),
    ...['latex/2026.10-r1', 'typst/0.15.1', 'fonts/2026.10.10', 'pdfium/7881'].map(name => path.join(profile, 'capabilities/managed-v2', name))];
  for (const file of [...paths]) for (let parent = path.dirname(file); !paths.includes(parent); parent = path.dirname(parent)) {
    paths.push(parent); if (path.dirname(parent) === parent) break;
  }
  const isolation = JSON.parse(h.command(path.join(process.env.SystemRoot || process.env.WINDIR, 'System32/WindowsPowerShell/v1.0/powershell.exe'),
    ['-NoProfile', '-NonInteractive', '-Command', `$ErrorActionPreference='Stop'\n$Profile=${quotePowerShell(profile)}\n$Targets=@(${paths.map(quotePowerShell).join(',')})\nforeach($Target in $Targets){$Item=Get-Item -LiteralPath $Target;if(($Item.Attributes -band [IO.FileAttributes]::ReparsePoint)-ne 0){throw 'Owned profile reparse point rejected'}}\n$Owned=@(Get-CimInstance Win32_Process -Filter "Name='readmd.exe' OR Name='msedgewebview2.exe' OR Name='xelatex.exe' OR Name='pdflatex.exe' OR Name='dvipdfmx.exe'"|Where-Object {$_.CommandLine -and $_.CommandLine.IndexOf($Profile,[StringComparison]::OrdinalIgnoreCase)-ge 0}|ForEach-Object {[pscustomobject]@{pid=$_.ProcessId;name=$_.Name}})\n[pscustomobject]@{checkedPaths=$Targets.Count;reparsePoints=0;activeOwnedProcesses=$Owned}|ConvertTo-Json -Depth 4 -Compress`]));
  assert.equal(isolation.reparsePoints, 0); assert.deepEqual(isolation.activeOwnedProcesses, [], 'The reviewed native profile already has an active owned process');
  assert.equal(fileHash(reportFile), engineReuseReportSha256, 'The retained resource-proof report changed');
  const previous = JSON.parse(fs.readFileSync(reportFile, 'utf8'));
  assert.equal(previous.checks[0]?.passed, true); assert.equal(previous.checks[1]?.passed, true);
  assert.equal(previous.checks[0].name, 'current candidate assets overlay and immutable existing managed TeX reuse');
  assert.equal(previous.checks[1].name, 'real Windows production CSP, System32-only child PATH and locked engine/font/PDFium readiness');
  assert.equal(previous.coldReadiness.performanceAccepted, false);
  for (const id of ['typst', 'latex', 'fonts', 'pdfium']) {
    const status = previous.capabilityStatus.find(item => item.id === id);
    assert(status?.response.installed && status.response.ready, 'The retained profile did not prove readiness for ' + id);
  }
  const tex = previous.tex;
  assert.equal(tex.qaOnly, true); assert.equal(tex.redistributionApproved, false);
  assert.equal(tex.receiptSha256, fileHash(receiptFile)); assert.equal(tex.lockedArchiveSha256, receipt.archiveSha256);
  assert.equal(tex.safeConstructionPlanSha256, materializationPlanSha256); assert.equal(tex.originalAclAuditSha256, originalAclAuditSha256);
  assert.equal(tex.lockedFiles, 34822); assert.equal(tex.actualSourceAndTargetIdentitiesChecked, 34822);
  assert.equal(tex.linkedFiles + tex.copiedFiles, tex.lockedFiles); assert.equal(tex.linkedBytes + tex.copiedBytes, receipt.installedBytes);
  assert.equal(tex.sourceSecurityDescriptorsUnchanged, true); assert.equal(tex.sourceRuntimeContentUnchanged, true);
  assert.equal(tex.sourceHardlinkCountsVerified, true);
  const preserved = new Map();
  const record = (file, expected) => {
    const stat = fs.lstatSync(file); assert(stat.isFile() && !stat.isSymbolicLink(), 'A retained proof is not an ordinary file');
    const digest = fileHash(file); if (expected) assert.equal(digest, expected, 'Retained proof content changed');
    preserved.set(file, digest); return digest;
  };
  record(reportFile, engineReuseReportSha256);
  record(path.join(profile, 'assets/capabilities/catalog.json'), engineReuseCatalogSha256);
  const lockFile = path.join(profile, 'assets/capabilities/latex-2026.10-r1-windows-x64.lock.json');
  record(lockFile, tex.lockSha256);
  const lock = JSON.parse(fs.readFileSync(lockFile, 'utf8'));
  assert.equal(lock.files.length, 34822); assert.equal(new Set(lock.files.map(item => item.path)).size, 34822);
  assert.equal(lock.files.reduce((sum, item) => sum + item.size, 0), receipt.installedBytes);
  for (const proof of [tex.resourceAclBefore, tex.resourceAclAfter]) {
    assert(proof.file && path.basename(proof.file) === proof.file); record(path.join(profile, proof.file), proof.sha256);
  }
  record(path.join(profile, 'assets/readmd.boot.js'), engineReuseBootSha256);
  assert.equal(fs.statSync(path.join(profile, 'assets/readmd.boot.js')).nlink, 1, 'Old evidence boot must be detached before a new bundle');
  for (const [name, digest] of Object.entries(previous.inputs)) {
    assert.equal(path.basename(name), name); record(path.join(profile, 'documents', name), digest);
  }
  for (const output of previous.outputs) {
    assert.equal(path.basename(output.file), output.file); record(path.join(profile, 'outputs', output.file), output.sha256);
  }
  for (const file of fs.readdirSync(profile).filter(name => name === 'native.log' || name === 'failure.png' || /\.(?:inspection|diagnostic)\.json$/.test(name))) record(path.join(profile, file));
  const assertPreserved = () => {
    for (const [file, digest] of preserved) assert.equal(fileHash(file), digest, 'Reused profile evidence changed: ' + path.relative(profile, file));
    h.report.engineProfileReuse.oldEvidenceBytesPreservedAtCompletion = true;
  };
  h.report.engineProfileReuse = {enabled: true, profile: engineReuseProfile, priorReportSha256: engineReuseReportSha256,
    priorCandidateSha256: previous.candidateSha256, priorSuitePassed: previous.passed, priorFailureRetained: previous.failedStage,
    nativeDataFresh: false, freshQaDocumentsAndOutputs: true, qaRuntimeClonePerformed: false, qaRuntimeFilesRescanned: 0,
    previousIdentityAndAclProofFiles: 34822, priorReadinessIsNotCurrentReadiness: true, actualProductReadinessReexecuted: false,
    coldPerformanceAccepted: false, mutableProductState: ['owned readmd.db and SQLite sidecars', 'owned settings, WebView/application caches and task/lease state'],
    oldEvidenceFiles: preserved.size, oldEvidenceBytesPreservedAtPreflight: true, oldEvidenceBytesPreservedAtCompletion: false, isolation};
  h.report.tex = {qaOnly: true, redistributionApproved: false, receiptSha256: tex.receiptSha256, lockSha256: tex.lockSha256,
    lockedArchiveSha256: tex.lockedArchiveSha256, lockedFiles: tex.lockedFiles, reusedInstallation: true,
    currentRunNewHardlinks: 0, currentRunCopiedFiles: 0, currentRunCopiedBytes: 0, priorConstruction: tex};
  h.report.limitations.push('Native data and already installed immutable runtimes are explicitly reused. DB/settings/WebView state may change; prior report, source documents, outputs, assets, lock and ACL proofs must retain their original bytes. This is not a new-profile cold-performance measurement.');
  return {profile, previous, assertPreserved};
}
function prepareEngineProfileReuse(h, profile, currentAssets) {
  const evidence = verifyEngineProfileReuse(h, profile), assets = path.join(h.data, 'assets');
  const currentCatalogFile = path.join(currentAssets, 'capabilities/catalog.json'), currentCatalogHash = fileHash(currentCatalogFile);
  hardlinkTree(currentAssets, assets);
  for (const name of ['catalog.json', 'latex-2026.10-r1-windows-x64.lock.json']) {
    const target = path.join(assets, 'capabilities', name);
    if (fs.existsSync(target)) fs.unlinkSync(target);
    fs.writeFileSync(target, fs.readFileSync(path.join(evidence.profile, 'assets/capabilities', name)), {flag: 'wx'});
  }
  assert.equal(fileHash(currentCatalogFile), currentCatalogHash);
  assert.equal(fileHash(path.join(assets, 'capabilities/catalog.json')), engineReuseCatalogSha256);
  assert.equal(fileHash(path.join(assets, 'capabilities/latex-2026.10-r1-windows-x64.lock.json')), evidence.previous.tex.lockSha256);
  assert.equal(fileHash(path.join(assets, 'readmd.boot.js')), fileHash(path.join(currentAssets, 'readmd.boot.js')));
  if (process.env.READMD_BOOT_SHA256) assert.equal(fileHash(path.join(assets, 'readmd.boot.js')), process.env.READMD_BOOT_SHA256);
  for (const [id, variable, reported] of [['fonts', 'READMD_FONTS_PACK', 'fontsPackSha256'], ['pdfium', 'READMD_PDFIUM_PACK', 'pdfiumPackSha256'], ['typst', 'READMD_TYPST_PACK', 'typstPackSha256']]) {
    const archive = process.env[variable] || (id === 'pdfium' ? process.env.READMD_PDF_PACK : id === 'typst' ? path.join(repository, '.cache/capability-upgrade/locked-packs/readmd-typst-0.15.1-windows-x64.readmd-pack') : undefined);
    assert(archive && fs.existsSync(archive), variable + ' must identify the existing locked pack even for explicit profile reuse');
    assert.equal(fileHash(archive), evidence.previous[reported], 'A reused installed capability differs from the supplied locked pack');
    h.report[reported] = evidence.previous[reported];
  }
  process.env.READMD_ASSETS_DIR = assets; process.env.READMD_NATIVE_LOG_FILE = path.join(h.data, 'native.log');
  h.report.engineProfileReuse.currentAssetsBootSha256 = fileHash(path.join(assets, 'readmd.boot.js'));
  h.report.engineProfileReuse.assetsOverlay = path.relative(h.root, assets).replaceAll(path.sep, '/');
  h.report.engineProfileReuse.catalogSha256 = engineReuseCatalogSha256;
  h.report.engineProfileReuse.texLockSha256 = evidence.previous.tex.lockSha256;
  return {...evidence, assets};
}
async function ownedPort() {
  const server = net.createServer();
  await new Promise((resolve, reject) => {server.once('error', reject); server.listen(0, '127.0.0.1', resolve);});
  const port = server.address().port; await new Promise(resolve => server.close(resolve)); return port;
}
async function run() {
  const reuseProfile = resolveEngineReuseProfile(process.env.READMD_ENGINE_REUSE_PROFILE);
  const h = createHarness('publication-engines-native', await ownedPort(), __filename, reuseProfile);
  const scriptSnapshot = path.join(h.data, 'publication-engines-native.cjs');
  fs.writeFileSync(scriptSnapshot, fs.readFileSync(__filename), {flag: 'wx'});
  assert.equal(fileHash(scriptSnapshot), h.report.scriptSha256);
  h.report.qaSourceSnapshot = {file: path.basename(scriptSnapshot), sha256: fileHash(scriptSnapshot)};
  const step = (label, work) => {console.log(JSON.stringify({stage: label})); return h.step(label, work);};
  const assets = path.join(h.data, 'assets'), currentAssets = process.env.READMD_ASSETS_DIR || path.join(h.root, 'assets');
  const outputs = [], sourceFile = path.join(h.docs, 'Engine-evidence.md');
  h.report.outputs = outputs;
  for (const [name, value] of Object.entries({'Engine-evidence.md': paper, 'Native-context.md': nativeSource, 'references.json': JSON.stringify(references), 'references.bib': bib, 'rich.csl': richWithLocator,
    'Smallcaps-evidence.md': capsSource, 'smallcaps-references.json': JSON.stringify(capsReferences), 'smallcaps.csl': capsStyle,
    'Light-evidence.md': lightSource, 'light.csl': lightStyle,
    ...Object.fromEntries(unsupportedNativeCases.map(([, content], index) => ['Unsupported-context-' + index + '.md', content]))})) fs.writeFileSync(path.join(h.docs, name), value);
  const initial = h.snapshotInputs();
  h.report.inputs = initial;
  h.report.scope = 'Real Typst and managed XeLaTeX PDF, standalone TeX context, current P01 panel locales. TeX input is existing unpublished local QA only; no runtime redistribution or release-pack acceptance is asserted.';
  h.report.limitations.push('The PDF baseline is its real /Prev revision, not a separately captured engine process output. Visual locale smoke is not a human linguistic review.');
  h.report.qaDependencySha256['tools/test/publication-engine-revision.py'] = fileHash(path.join(h.root, 'tools/test/publication-engine-revision.py'));
  h.report.qaDependencySha256['tools/test/publication-smallcaps-proof.py'] = fileHash(path.join(h.root, 'tools/test/publication-smallcaps-proof.py'));
  h.report.qaDependencySha256['tools/test/publication-runtime-reuse.cjs'] = fileHash(path.join(h.root, 'tools/test/publication-runtime-reuse.cjs'));
  h.report.coldReadiness = {diagnosticDeadlineMs: 600000, originalStatusDeadlineMs: 120000, performanceAccepted: false,
    measurement: reuseProfile ? 'Explicit reused native data; not a new-profile cold measurement' : 'Fresh native data with complete QA runtime construction',
    originalFailure: '.cache/capability-upgrade/publication-engines-native-BcD2ef/report.json',
    limitation: 'The retained original 120s status failure and 34822-file/736MB runtime cold-hash performance remain unresolved. A bounded cold integrity attestation only permits actual output diagnosis; it is not a cold-performance pass.'};
  h.report.qaFixtureContractCorrection = {originalFailure: originalEngineReport, originalReportSha256: originalEngineReportSha256,
    originalScriptSha256: 'e01b2490a4e594d8015f1f6538f162be701731ae07cb79a5efa537f64f781ce6',
    materializationPlan: path.relative(h.root, materializationPlanFile), materializationPlanSha256, originalAclAuditSha256,
    reason: 'Read the actual source permissions for all 34,822 locked resources. Reuse resources already granting AppContainer RX as immutable hardlinks; create missing-RX resources as new owned byte copies only when the fixed 13,369-file safe plan permits it. Historical protected sources and the repaired hZuzm8 runtime share the same content lock.',
    productChanged: false, sourceAclChanged: false, sourceContentChanged: false,
    originalProbeAndExportBudgetsUnchanged: true, coldPerformanceAccepted: false};
  const exportFile = async (name, format, content, options) => {
    const file = path.join(h.outputs, name + '.' + format);
    const result = await h.api('/api/export', {format, content, baseDir: h.docs, out_path: file, suggestedName: name, options});
    assert.equal(result.status, 200, JSON.stringify(result.data)); assert.equal(result.data.ok, true, JSON.stringify(result.data));
    outputs.push({file: path.basename(file), sha256: fileHash(file), bytes: fs.statSync(file).size}); return file;
  };
  let page, reuseEvidence;
  await h.finish(async () => {
    try {
    await step('current candidate assets overlay and immutable existing managed TeX reuse', async () => {
      if (reuseProfile) {
        reuseEvidence = prepareEngineProfileReuse(h, reuseProfile, currentAssets);
        h.report.qaFixtureContractCorrection.currentRun = 'Explicit fixed-profile reuse; no QA runtime clone or per-file rescan';
        return {reusedProfile: engineReuseProfile, priorReportSha256: engineReuseReportSha256, currentRuntimeFilesCopied: 0, productReadinessStillRequired: true};
      }
      hardlinkTree(currentAssets, assets);
      assert.equal(fileHash(path.join(assets, 'readmd.boot.js')), fileHash(path.join(currentAssets, 'readmd.boot.js')));
      if (process.env.READMD_BOOT_SHA256) assert.equal(fileHash(path.join(assets, 'readmd.boot.js')), process.env.READMD_BOOT_SHA256);
      const catalogFile = path.join(assets, 'capabilities/catalog.json'), originalCatalogHash = fileHash(path.join(currentAssets, 'capabilities/catalog.json'));
      const catalog = JSON.parse(fs.readFileSync(catalogFile, 'utf8'));
      const oldCatalog = JSON.parse(fs.readFileSync(path.join(receipt.assets, 'capabilities/catalog.json'), 'utf8'));
      const texEntry = oldCatalog.packs.find(item => item.id === 'latex'); assert(texEntry?.qaOnly);
      assert.equal(texEntry.sha256, receipt.archiveSha256);
      catalog.packs = [...catalog.packs.filter(item => item.id !== 'latex'), texEntry];
      // Unlink this owned hardlink before changing the overlay's catalog.
      fs.unlinkSync(catalogFile); fs.writeFileSync(catalogFile, JSON.stringify(catalog, null, 2));
      assert.equal(fileHash(path.join(currentAssets, 'capabilities/catalog.json')), originalCatalogHash);
      const lockName = `latex-${texEntry.version}-${texEntry.platform}.lock.json`;
      const lockSource = path.join(receipt.assets, 'capabilities', lockName), lockTarget = path.join(assets, 'capabilities', lockName);
      if (fs.existsSync(lockTarget)) fs.unlinkSync(lockTarget);
      fs.writeFileSync(lockTarget, fs.readFileSync(lockSource), {flag: 'wx'});
      const lock = JSON.parse(fs.readFileSync(lockTarget, 'utf8'));
      assert.equal(fileHash(materializationPlanFile), materializationPlanSha256);
      assert.equal(fileHash(path.join(h.root, originalEngineReport)), originalEngineReportSha256);
      const plan = JSON.parse(fs.readFileSync(materializationPlanFile, 'utf8').replace(/^\uFEFF/, ''));
      h.report.qaDependencySha256[path.relative(h.root, materializationPlanFile).replaceAll(path.sep, '/')] = materializationPlanSha256;
      assert.equal(fileHash(lockTarget), plan.lockSha256); assert.equal(plan.schemaVersion, 1); assert.equal(plan.originalAuditSha256, originalAclAuditSha256);
      assert.equal(plan.qaOnly, true); assert.equal(plan.redistributionApproved, false); assert.equal(lock.files.length, 34822);
      assert.equal(plan.freshProfileCopyUnion.files, 13369); assert.equal(plan.freshProfileCopyUnion.bytes, 200049317);
      assert.equal(plan.requiredRuntimeSid, 'S-1-15-2-1'); assert.equal(plan.requiredRuntimeMask, '0x1200a9');
      const locked = new Map(lock.files.map(entry => [entry.path, entry])); assert.equal(locked.size, lock.files.length);
      const sourceRelative = path.relative(h.root, runtimeSource); assert(sourceRelative && !path.isAbsolute(sourceRelative) && !sourceRelative.split(path.sep).includes('..'));
      h.report.sourceRuntime = {path: sourceRelative.replaceAll(path.sep, '/'), selection: 'Explicit source; actual complete-lock Win32 permission/identity inspection determines hardlink or approved byte-copy'};
      if (runtimeSource === path.join(h.root, reusableRuntimeProfile, 'capabilities', 'managed-v2', 'latex', texEntry.version)) {
        const sourceReportFile = path.join(h.root, reusableRuntimeProfile, 'report.json');
        assert.equal(fileHash(sourceReportFile), reusableRuntimeReportSha256);
        const sourceReport = JSON.parse(fs.readFileSync(sourceReportFile, 'utf8'));
        assert(sourceReport.capabilityStatus.some(item => item.id === 'latex' && item.response.ready && item.response.installed));
        h.report.sourceRuntime.provenance = {report: reusableRuntimeProfile + '/report.json', sha256: reusableRuntimeReportSha256,
          candidateSha256: sourceReport.candidateSha256, nativeSuitePassed: false, scope: 'Runtime construction/readiness proof only; retained Typst /Prev and emoji failures remain product failures.'};
      }
      const ownedResources = new Map();
      for (const candidate of plan.freshProfileCopyUnion.sourceLockedFiles) {
        assert.deepEqual(Object.keys(candidate).sort(), ['path', 'sha256', 'size']);
        assert(candidate.path && !path.isAbsolute(candidate.path) && !candidate.path.split(/[\\/]/).includes('..'));
        const entry = locked.get(candidate.path); assert(entry, 'Owned runtime resource is absent from the fixed lock');
        assert.equal(candidate.size, entry.size); assert.equal(candidate.sha256, entry.sha256, 'The complete copy plan changed a locked resource');
        assert(!ownedResources.has(candidate.path)); ownedResources.set(candidate.path, candidate);
      }
      assert.equal(ownedResources.size, 13369); assert.equal([...ownedResources.values()].reduce((sum, candidate) => sum + candidate.size, 0), 200049317);
      const runtimeTarget = path.join(h.data, 'capabilities', 'managed-v2', 'latex', texEntry.version);
      fs.mkdirSync(runtimeTarget, {recursive: true});
      // Match worker_sandbox::prepare_empty_runtime: grant inheritable RX on
      // this new EMPTY owned directory before adding any immutable hardlinks.
      // A late inheritable ACL update would rewrite the source files' shared
      // security descriptors and can take minutes for a 35k-file runtime.
      const relative = path.relative(h.data, runtimeTarget);
      assert(relative && !path.isAbsolute(relative) && !relative.split(path.sep).includes('..'));
      assert(!fs.lstatSync(runtimeTarget).isSymbolicLink() && fs.readdirSync(runtimeTarget).length === 0);
      assert.equal(process.platform, 'win32');
      h.command(path.join(process.env.SystemRoot || process.env.WINDIR, 'System32', 'icacls.exe'),
        [runtimeTarget, '/grant', '*S-1-15-2-1:(OI)(CI)(RX)']);
      const aclFile = path.join(h.data, 'runtime-resource-acl-reader.ps1');
      fs.writeFileSync(aclFile, aclReaderScript, {flag: 'wx'});
      const quotePowerShell = value => "'" + value.replaceAll("'", "''") + "'";
      // The host permits direct commands but requires signed script files.
      // Keep the reader as evidence and execute the same read-only command;
      // neither system execution policy nor application CSP is changed.
      const aclCommand = '& {\n' + aclReaderScript + '\n} -Plan ' + quotePowerShell(lockTarget) + ' -SourceRoot ' + quotePowerShell(runtimeSource) + ' -TargetRoot ' + quotePowerShell(runtimeTarget);
      const readAcls = () => JSON.parse(h.command(path.join(process.env.SystemRoot || process.env.WINDIR, 'System32/WindowsPowerShell/v1.0/powershell.exe'),
        ['-NoProfile', '-NonInteractive', '-Command', aclCommand]));
      const beforeScan = readAcls(), beforeAcls = beforeScan.rows; assert.equal(beforeAcls.length, lock.files.length);
      const beforeByPath = new Map(), methods = new Map(), sourceAddedLinks = new Map();
      let expectedCopiedFiles = 0, expectedCopiedBytes = 0;
      for (const [index, item] of beforeAcls.entries()) {
        assert.equal(item.path, lock.files[index].path); assert.equal(item.target, null); assert.equal(item.source.bytes, lock.files[index].size);
        assert(!beforeByPath.has(item.path)); beforeByPath.set(item.path, item);
        const method = runtimeMaterialization(item.source, ownedResources.has(item.path)); methods.set(item.path, method);
        if (method === 'byte-copy') {expectedCopiedFiles++; expectedCopiedBytes += item.source.bytes;}
        else sourceAddedLinks.set(item.source.identity, (sourceAddedLinks.get(item.source.identity) || 0) + 1);
      }
      assert(expectedCopiedFiles <= 13369 && expectedCopiedBytes <= 200049317);
      fs.writeFileSync(path.join(h.data, 'runtime-resource-acl-before.json'), JSON.stringify(beforeAcls, null, 2) + '\n', {flag: 'wx'});
      let linkedBytes = 0, linkedFiles = 0, copiedBytes = 0, copiedFiles = 0;
      for (const entry of lock.files) {
        assert(!path.isAbsolute(entry.path) && !entry.path.split('/').includes('..'));
        const from = path.join(runtimeSource, entry.path), to = path.join(runtimeTarget, entry.path);
        const stat = fs.lstatSync(from); assert(stat.isFile() && !stat.isSymbolicLink()); assert.equal(stat.size, entry.size);
        fs.mkdirSync(path.dirname(to), {recursive: true});
        if (methods.get(entry.path) === 'byte-copy') {
          const bytes = fs.readFileSync(from); assert.equal(bytes.length, entry.size); assert.equal(sha256(bytes), entry.sha256);
          // A fresh file inherits its reviewed parent RX ACL. CopyFile would
          // carry the old protected descriptor, and chmod/Set-Acl on a hardlink
          // would mutate the original source file's shared security descriptor.
          fs.writeFileSync(to, bytes, {flag: 'wx'});
          assert.equal(fileHash(to), entry.sha256);
          copiedBytes += stat.size; copiedFiles++;
        } else {fs.linkSync(from, to); linkedBytes += stat.size; linkedFiles++;}
      }
      assert.equal(copiedFiles, expectedCopiedFiles); assert.equal(copiedBytes, expectedCopiedBytes);
      assert.equal(linkedFiles + copiedFiles, lock.files.length); assert.equal(linkedBytes + copiedBytes, receipt.installedBytes);
      const afterScan = readAcls(), afterAcls = afterScan.rows; assert.equal(afterAcls.length, beforeAcls.length);
      for (const [index, item] of afterAcls.entries()) {
        assert.equal(item.path, beforeAcls[index].path);
        verifyRuntimeMaterialization(beforeByPath.get(item.path).source, item, methods.get(item.path), sourceAddedLinks.get(item.source.identity) || 0);
        assert.equal(fileHash(path.join(runtimeSource, item.path)), locked.get(item.path).sha256, 'Source content changed');
        assert.equal(item.source.bytes, locked.get(item.path).size); assert.equal(item.target.bytes, locked.get(item.path).size);
      }
      fs.writeFileSync(path.join(h.data, 'runtime-resource-acl-after.json'), JSON.stringify(afterAcls, null, 2) + '\n', {flag: 'wx'});
      assert.equal(fileHash(path.join(h.root, originalEngineReport)), originalEngineReportSha256);
      assert.equal(fileHash(materializationPlanFile), materializationPlanSha256); assert.equal(fileHash(lockSource), plan.lockSha256);
      process.env.READMD_ASSETS_DIR = assets;
      process.env.READMD_NATIVE_LOG_FILE = path.join(h.data, 'native.log');
      h.report.tex = {qaOnly: true, redistributionApproved: false, receiptSha256: fileHash(receiptFile), lockSha256: fileHash(lockTarget), lockedArchiveSha256: receipt.archiveSha256,
        lockedFiles: lock.files.length, linkedFiles, linkedBytes, copiedFiles, copiedBytes, newRuntimeBytesCopied: copiedBytes, otherFilesCopied: 0,
        emptyOwnedRuntimePreparedBeforeHardlinks: true, resourceAclReaderSha256: fileHash(aclFile), safeConstructionPlanSha256: materializationPlanSha256, originalAclAuditSha256, aclReaderExecution: 'One Win32 batch process per phase; direct read-only command, system execution policy unchanged', sourceSecurityDescriptorsUnchanged: true, sourceRuntimeContentUnchanged: true,
        actualSourceAndTargetIdentitiesChecked: lock.files.length, sourceHardlinkCountsVerified: true,
        plannedCopiedFiles: expectedCopiedFiles, plannedCopiedBytes: expectedCopiedBytes,
        parsedSourceDescriptors: beforeScan.uniqueDescriptors, parsedAfterDescriptors: afterScan.uniqueDescriptors,
        resourceAclBefore: {file: 'runtime-resource-acl-before.json', sha256: fileHash(path.join(h.data, 'runtime-resource-acl-before.json'))},
        resourceAclAfter: {file: 'runtime-resource-acl-after.json', sha256: fileHash(path.join(h.data, 'runtime-resource-acl-after.json'))}};
    });
    await step('real Windows production CSP, System32-only child PATH and locked engine/font/PDFium readiness', async () => {
      const oldPath = process.env.PATH;
      try {process.env.PATH = path.join(process.env.WINDIR, 'System32'); page = await h.start();} finally {process.env.PATH = oldPath;}
      h.report.system32OnlyChildPath = true;
      const csp = await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content');
      const scripts = csp.split(';').find(value => value.trim().startsWith('script-src'));
      assert(scripts?.includes("'self'") && !scripts.includes("'unsafe-inline'") && !scripts.includes("'unsafe-eval'"));
      if (!reuseProfile) {
        await h.installPacks();
        const typst = process.env.READMD_TYPST_PACK || path.join(h.root, '.cache/capability-upgrade/locked-packs/readmd-typst-0.15.1-windows-x64.readmd-pack');
        const installed = await h.api('/api/capabilities/packs', {action: 'install', id: 'typst', path: typst}); assert.equal(installed.data.ok, true, JSON.stringify(installed.data));
        h.report.typstPackSha256 = fileHash(typst);
      }
    });
    h.report.capabilityStatus = [];
    for (const id of ['typst', 'latex', 'fonts', 'pdfium']) {
      await step((reuseProfile ? 'reused-profile' : 'cold') + ' integrity attestation ' + id + ': bounded 600s output diagnosis, original 120s performance unresolved', async () => {
        const entry = {id, startedAt: new Date().toISOString()}, started = Date.now();
        h.report.capabilityStatus.push(entry); console.log(JSON.stringify({statusStarted: id, at: entry.startedAt}));
        const status = await page.evaluate(async ({id, budget}) => {
          const response = await apiFetch('/api/capabilities/packs', {method: 'POST', headers: {'Content-Type': 'application/json'},
            body: JSON.stringify({action: 'status', id}), signal: AbortSignal.timeout(budget)});
          return {status: response.status, data: await response.json()};
        }, {id, budget: h.report.coldReadiness.diagnosticDeadlineMs});
        Object.assign(entry, {elapsedMs: Date.now() - started, response: status.data}); console.log(JSON.stringify({statusFinished: id, elapsedMs: entry.elapsedMs, response: status.data}));
        entry.exceedsOriginalDeadline = entry.elapsedMs > h.report.coldReadiness.originalStatusDeadlineMs;
        assert.equal(status.status, 200);
        assert.equal(status.data.installed, true, JSON.stringify(status.data)); assert.equal(status.data.ready, true, JSON.stringify(status.data));
      });
    }
    if (reuseEvidence) h.report.engineProfileReuse.actualProductReadinessReexecuted = true;
    for (const engine of ['typst', 'xelatex']) {
      await step(engine + ': same service preview/export bytes, full metadata and physical rich citation glyphs/annotations', async () => {
        const options = {...baseOptions, pdf: {engine}}, name = 'Paper-' + engine;
        const preview = await h.api('/api/export/preview', {format: 'pdf', content: paper, baseDir: h.docs, suggestedName: name, options});
        assert.equal(preview.status, 200, JSON.stringify(preview.data)); assert.equal(preview.data.ok, true, JSON.stringify(preview.data));
        assert.equal(preview.data.previewEngine, engine); assert(preview.data.pages > 0 && preview.data.pdf);
        const file = await exportFile(name, 'pdf', paper, options);
        assert.deepEqual(fs.readFileSync(file), Buffer.from(preview.data.pdf, 'base64'), 'Final export changed the selected preview artifact');
        const baseline = path.join(h.outputs, name + '-before-metadata.pdf');
        const revision = JSON.parse(h.command('python', [path.join(h.root, 'tools/test/publication-engine-revision.py'), file, baseline]));
        const inspected = inspectArtifact(h, file, {baseline, glyphs: true, render: path.join(h.data, name + '-pages')});
        checkMetadataArtifact(inspected, paperMetadata, 'paper'); checkRichArtifact(inspected);
        assert(inspected.text.includes('😀') && inspected.pypdfText.includes('😀'), engine + ': original emoji disappeared from visible front matter in an independent parser');
        const emojiGlyphs = inspected.pages.flatMap((page, pageIndex) => (page.glyphTraces || []).flatMap(trace => trace.chars
          .filter(character => character.unicode === 0x1F600).map(character => ({pageIndex, font: trace.font, size: trace.size, ...character}))));
        assert(emojiGlyphs.length && emojiGlyphs.every(character => character.glyphId > 0 && character.bbox[2] > character.bbox[0] && character.bbox[3] > character.bbox[1]),
          engine + ': original U+1F600 has no actual non-.notdef PDF glyph');
        for (const property of ['samePageObjects', 'sameNonMetadataObjects', 'samePagePixels']) assert.equal(inspected.comparison[property], true, engine + ': metadata changed ' + property);
        assert(inspected.pageObjects.some(item => JSON.stringify(item.annotations).includes('https://example.invalid/study')), engine + ': CSL link has no actual PDF annotation');
        assert(inspected.text.includes('42') && inspected.text.includes('43') && inspected.text.includes('passim'), 'Affix or locator disappeared in engine output');
        assert(!inspected.text.includes(':cite[') && !inspected.text.includes('data-slot='));
        h.report[engine] = {revision, comparison: inspected.comparison, pages: inspected.pages.length, previewSha256: fileHash(file),
          originalEmojiGlyphs: emojiGlyphs, originalEmojiInBothIndependentParsers: true};
        h.unchanged(initial);
      });
      if (engine === 'typst') await step('typst: unknown U+10FFFF rejects atomically for existing and absent PDF destinations', async () => {
        const original = path.join(h.outputs, 'Paper-typst.pdf'), originalDigest = fileHash(original);
        const existing = path.join(h.outputs, 'Guard-U10FFFF-existing-typst.pdf'), absent = path.join(h.outputs, 'Guard-U10FFFF-absent-typst.pdf');
        fs.writeFileSync(existing, fs.readFileSync(original), {flag: 'wx'}); assert.equal(fileHash(existing), originalDigest); assert.equal(fs.existsSync(absent), false);
        const unknown = '\u{10ffff}'; assert.equal([...unknown].length, 1); assert.equal(unknown.codePointAt(0), 0x10ffff);
        const content = paper + '\nUnknown SMP glyph: ' + unknown + '.\n';
        const localization = await page.evaluate(() => ({language: i18n.currentLang, message: i18n.dict['error.typst_invalid_content']}));
        assert(typeof localization.message === 'string' && localization.message.trim() && localization.message !== 'error.typst_invalid_content', 'Typed Typst rejection lacks the served locale message');
        h.report.typstUnavailableGlyphRejections = [];
        for (const destination of [existing, absent]) {
          const request = {format: 'pdf', content, baseDir: h.docs, out_path: destination, suggestedName: 'Unknown-SMP', options: {...baseOptions, pdf: {engine: 'typst'}}};
          const response = await h.api('/api/export', request);
          const evidence = {codepoint: 'U+10FFFF', requestContentSha256: sha256(content), destination: path.basename(destination),
            response, localization, expectedErrorCode: 'typst_invalid_content', originalPaperSha256: originalDigest,
            existingTargetSha256: fileHash(existing), absentTargetCreated: fs.existsSync(absent),
            absentTargetSha256: fs.existsSync(absent) ? fileHash(absent) : null};
          h.report.typstUnavailableGlyphRejections.push(evidence);
          fs.writeFileSync(path.join(h.data, 'typst-unavailable-glyph-rejections.json'), JSON.stringify(h.report.typstUnavailableGlyphRejections, null, 2) + '\n');
          // The established export route carries typed operation errors in an
          // HTTP 200 envelope. A successful operation or .notdef PDF must fail.
          assert.equal(response.status, 200); assert.equal(response.data.ok, false, JSON.stringify(response.data));
          assert.equal(response.data.error_code, 'typst_invalid_content', JSON.stringify(response.data));
          assert.equal(evidence.existingTargetSha256, originalDigest); assert.equal(evidence.absentTargetCreated, false);
          assert.equal(fileHash(original), originalDigest); h.unchanged(initial);
        }
      });
      await step(engine + ': original Unicode and four-face physical small-caps glyph/font/point-size proof', async () => {
        const family = engine === 'typst' ? 'Libertinus Serif' : 'TeX Gyre Termes';
        let content = capsSource;
        // Global font choices intentionally have a smaller whitelist. Exercise
        // the real local block-font API so controls use the same physical face.
        for (let index = 0; index < capsReferences.length; index++) {
          const model = await h.api('/api/publication/blocks', {content});
          assert.equal(model.status, 200, JSON.stringify(model.data)); assert.equal(model.data.ok, true);
          const paragraphs = model.data.document.nodes.filter(node => node.editable && node.kind === 'paragraph');
          assert.equal(paragraphs.length, capsReferences.length);
          const applied = await h.api('/api/publication/blocks', {action: 'apply', content, base_revision: model.data.document.revision,
            id: paragraphs[index].id, patch: {text: {font: family}}});
          assert.equal(applied.status, 200, JSON.stringify(applied.data)); assert.equal(applied.data.ok, true);
          assert.equal(applied.data.result.changed, true);
          content = applied.data.result.content;
          assert.equal(typeof content, 'string');
        }
        const name = 'Smallcaps-' + engine, options = {...baseOptions, pdf: {engine}, frontMatter: {mode: 'none'}};
        const file = await exportFile(name, 'pdf', content, options);
        const baseline = path.join(h.outputs, name + '-before-metadata.pdf');
        const revision = JSON.parse(h.command('python', [path.join(h.root, 'tools/test/publication-engine-revision.py'), file, baseline]));
        const inspected = inspectArtifact(h, file, {baseline, glyphs: true, render: path.join(h.data, name + '-pages')});
        for (const property of ['samePageObjects', 'sameNonMetadataObjects', 'samePagePixels']) assert.equal(inspected.comparison[property], true);
        const fontAudit = engine === 'typst' ? (process.env.READMD_TYPST_FONT_AUDIT || path.join(h.root, '.cache/private/publication-context-inputs/typst-builtin-fonts-20261014/audit.json')) :
          (process.env.READMD_TEX_FONT_AUDIT || path.join(h.root, '.cache/capability-upgrade/publication-context-inputs/engines-smallcaps-locked-font-audit.json'));
        const proofFile = path.join(h.data, name + '-glyph-proof.json');
        const {proof, command} = readSmallcapsProof(h, file, family, fontAudit, proofFile);
        assert.equal(proof.passed, true); assert.equal(proof.faces, 4); assert.equal(proof.glyphsVerified, 72);
        (h.report.smallcaps ||= {})[engine] = {proof: path.basename(proofFile), proofSha256: fileHash(proofFile), fontAuditSha256: fileHash(fontAudit),
          command, revision, comparison: inspected.comparison, localFontEdits: capsReferences.length, originalInputsUnchanged: true};
        h.unchanged(initial);
      });
    }
    await step('standalone BibLaTeX/natbib native parameters retain modes, formatted affixes and locators', async () => {
      const expected = {
        biblatex: ['\\autocite[{see}][{chapter 4 with \\textit{passim}}]{study}', '\\textcite[{}][{8}]{study}', '\\citeauthor[{cf.}][{}]{study3}' ],
        natbib: ['\\citep[{see}][{chapter 4 with \\textit{passim}}]{study}', '\\citet[{}][{8}]{study}', '\\citeauthor[{cf.}][{}]{study3}'],
      };
      for (const engine of ['biblatex', 'natbib']) {
        const name = 'Native-' + engine;
        const content = engine === 'biblatex' ? nativeSource + '\nMultiple [see @study, p. 9; cf. @study2, chap. 3].\n' : nativeSource;
        const file = await exportFile(name, 'tex', content, {...baseOptions, tex: {...baseOptions.tex, bibEngine: engine, renderBibliography: true}});
        const text = fs.readFileSync(file, 'utf8');
        for (const token of expected[engine]) assert(text.includes(token), engine + ' missing native context: ' + token);
        if (engine === 'biblatex') assert(text.includes('\\autocites[{see}][{9}]{study}[{cf.}][{chapter 3}]{study2}'), 'Per-item BibLaTeX parameters were flattened into one cluster affix');
        (h.report.nativeMetadataText ||= {})[engine] = checkNativeTexMetadata(text, visibleTokens(paperMetadata, 'paper'), path.join(h.outputs, name + '.assets'));
        assert(text.includes('study3') && text.includes('Nguyen, Linh') && text.includes('林墨'));
        assert(!text.includes('readmd-csl') && !text.includes(':cite['));
        const copied = path.join(h.outputs, name + '.assets', 'references1.bib'); assert.equal(fileHash(copied), fileHash(path.join(h.docs, 'references.bib')));
        h.unchanged(initial);
      }
      const name = 'Native-bibtex', content = nativeSource.replace(nativeBody, 'Normal [@study, p. 8 suffix].\n');
      const file = await exportFile(name, 'tex', content, {...baseOptions, tex: {...baseOptions.tex, bibEngine: 'bibtex', renderBibliography: true}});
      assert(fs.readFileSync(file, 'utf8').includes('\\cite[{8 suffix}]{study}'), 'Plain BibTeX needs one supported postnote argument');
      assert.equal(fileHash(path.join(h.outputs, name + '.assets/references1.bib')), fileHash(path.join(h.docs, 'references.bib')));
      h.unchanged(initial);
    });
    await step('unsupported standalone native contexts fail explicitly without changing old outputs or assets', async () => {
      h.report.unsupportedNativeContexts = [];
      for (const [index, [engine, content]] of unsupportedNativeCases.entries()) {
        assert.equal(sha256(content), initial['Unsupported-context-' + index + '.md']);
        const target = path.join(h.outputs, 'Unsupported-' + index + '.tex'), old = Buffer.from('Synthetic existing TeX result\n'); fs.writeFileSync(target, old);
        const assetDir = path.join(h.outputs, 'Unsupported-' + index + '.assets'); fs.mkdirSync(assetDir); fs.writeFileSync(path.join(assetDir, 'keep.txt'), old);
        const result = await h.api('/api/export', {format: 'tex', content, baseDir: h.docs, out_path: target, suggestedName: 'Unsupported-' + index, options: {...baseOptions, tex: {...baseOptions.tex, bibEngine: engine, renderBibliography: true}}});
        h.report.unsupportedNativeContexts.push({index, engine, input: 'Unsupported-context-' + index + '.md', inputSha256: sha256(content), response: result.data,
          oldTargetSha256: sha256(old), retainedTargetSha256: fileHash(target), assetFiles: fs.readdirSync(assetDir), foreignAssetSha256: fileHash(path.join(assetDir, 'keep.txt'))});
        assert.equal(result.data.ok, false, JSON.stringify(result.data)); assert.equal(result.data.error_code, 'academic_tex_context');
        assert.deepEqual(fs.readFileSync(target), old); assert.deepEqual(fs.readdirSync(assetDir), ['keep.txt']); assert.deepEqual(fs.readFileSync(path.join(assetDir, 'keep.txt')), old);
      }
      h.unchanged(initial);
    });
    await step('Light output limitations reject DOCX/static TeX/Typst atomically; core PDF keeps an explicit loss warning', async () => {
      assert.equal(sha256(lightSource), initial['Light-evidence.md']);
      h.report.lightLimitations = {input: 'Light-evidence.md', inputSha256: sha256(lightSource), styleSha256: initial['light.csl'], lightFaceCertified: false, rejections: []};
      for (const [id, format, extra] of [['docx', 'docx', {}], ['static-tex', 'tex', {tex: {...baseOptions.tex, renderBibliography: false}}], ['typst', 'pdf', {pdf: {engine: 'typst'}}]]) {
        const name = 'Light-unsupported-' + id, target = path.join(h.outputs, name + '.' + format), old = Buffer.from('Synthetic existing Light output\n');
        const assetDir = path.join(h.outputs, name + '.assets'); fs.writeFileSync(target, old); fs.mkdirSync(assetDir); fs.writeFileSync(path.join(assetDir, 'keep.txt'), old);
        const result = await h.api('/api/export', {format, content: lightSource, baseDir: h.docs, out_path: target, suggestedName: name, options: {...baseOptions, ...extra}});
        h.report.lightLimitations.rejections.push({id, format, response: result.data, previousSha256: sha256(old), currentSha256: fileHash(target)});
        assert.equal(result.status, 200); assert.equal(result.data.ok, false, JSON.stringify(result.data));
        assert.equal(result.data.error_code, 'academic_rich_unsupported');
        assert.deepEqual(fs.readFileSync(target), old); assert.deepEqual(fs.readdirSync(assetDir), ['keep.txt']); assert.deepEqual(fs.readFileSync(path.join(assetDir, 'keep.txt')), old);
        h.unchanged(initial);
      }
      const file = path.join(h.outputs, 'Light-core.pdf');
      const result = await h.api('/api/export', {format: 'pdf', content: lightSource, baseDir: h.docs, out_path: file, suggestedName: 'Light-core', options: baseOptions});
      assert.equal(result.status, 200); assert.equal(result.data.ok, true, JSON.stringify(result.data));
      assert(Array.isArray(result.data.warns) && result.data.warns.some(code => code === 'publication_pdf_light_unsupported' || code.startsWith('publication_pdf_light_unsupported:')), 'Core PDF must report its Light-weight rendering limit');
      const inspected = inspectArtifact(h, file, {glyphs: true});
      assert(inspected.text.includes(references[0].title) && inspected.pypdfText.includes(references[0].title), 'Core PDF warning must retain citation text');
      outputs.push({file: path.basename(file), sha256: fileHash(file), bytes: fs.statSync(file).size});
      h.report.lightLimitations.corePdf = {ok: true, warnings: result.data.warns, file: path.basename(file), sha256: fileHash(file), visibleTextRetained: true, lightFaceCertified: false};
      h.unchanged(initial);
    });
    await step('all 46 real locale dictionaries and source-panel labels retain original source bytes', async () => {
      await page.evaluate(async file => {await loadFile(file); await toggleEdit(); setEditorPref('aiCompletion', false); setPvLayout('none');}, sourceFile);
      const languages = await page.evaluate(() => Object.keys(i18n.meta)); assert.equal(languages.length, 46);
      const english = JSON.parse(fs.readFileSync(path.join(assets, 'i18n/en.json'), 'utf8'));
      const keys = Object.keys(english).filter(key => key.startsWith('pm.')); assert(keys.length >= 60);
      const evidence = [];
      for (const language of languages) {
        await page.evaluate(async language => {await i18n.setLanguage(language, false); openFrontmatterModal();}, language);
        await h.until(() => page.locator('#publication-metadata-editor [data-pm-field="authors"] [data-pm-row]').count().then(count => count === 2));
        const labels = await page.evaluate(({language, keys}) => {
          const host = document.getElementById('publication-metadata-editor');
          return {current: i18n.currentLang, dir: document.documentElement.dir, expectedDir: i18n.meta[language].dir,
            missing: keys.filter(key => typeof i18n.dict[key] !== 'string' || !i18n.dict[key].trim()),
            authors: host.querySelector('[data-pm-field="authors"] > summary').textContent, expectedAuthors: i18n.dict['pm.authors'],
            hint: host.querySelector('.exp-note').textContent, expectedHint: i18n.dict['pm.sourceHint']};
        }, {language, keys});
        assert.equal(labels.current, language); assert.equal(labels.dir, labels.expectedDir); assert.deepEqual(labels.missing, []);
        assert.equal(labels.authors, labels.expectedAuthors); assert.equal(labels.hint, labels.expectedHint);
        await page.locator('#fm-modal-cancel').click(); assert.equal(await page.evaluate(() => getEditContent()), paper);
        evidence.push({language, direction: labels.dir, keys: keys.length});
      }
      h.report.locales = evidence; h.unchanged(initial);
    });
    await step('RTL and long-text metadata controls fit both reference sizes without source writes', async () => {
      for (const [width, height, language, theme] of [[1024, 680, 'ar', 'light'], [1160, 820, 'de', 'dark'], [1024, 680, 'he', 'dark'], [1160, 820, 'ug', 'light']]) {
        await page.setViewportSize({width, height});
        await h.until(() => page.evaluate(() => !document.documentElement.classList.contains('rm-theme-vt')));
        for (let count = 0; count < 4 && await page.locator('body').getAttribute('data-theme') !== theme; count++) {
          const beforeChoice = await page.locator('#btn-theme').getAttribute('data-theme-icon');
          assert(['auto', 'light', 'dark', 'sepia'].includes(beforeChoice));
          await page.locator('#btn-theme').click();
          await h.until(() => page.evaluate(before => document.getElementById('btn-theme').dataset.themeIcon !== before &&
            !document.documentElement.classList.contains('rm-theme-vt'), beforeChoice));
          const afterChoice = await page.locator('#btn-theme').getAttribute('data-theme-icon');
          assert.equal(afterChoice, ['auto', 'light', 'dark', 'sepia'][(['auto', 'light', 'dark', 'sepia'].indexOf(beforeChoice) + 1) % 4]);
        }
        assert.equal(await page.locator('body').getAttribute('data-theme'), theme);
        await page.evaluate(async language => {await i18n.setLanguage(language, false); openFrontmatterModal();}, language);
        await h.until(() => page.locator('#publication-metadata-editor [data-pm-field="title"] input').count());
        await page.locator('#publication-metadata-editor [data-pm-field="title"] input').fill(paperMetadata.title.repeat(8));
        const overflow = await page.evaluate(() => ({document: document.documentElement.scrollWidth > innerWidth + 1,
          panel: [...document.querySelectorAll('#frontmatter-modal .modal-scroll, #publication-metadata-editor')].some(node => node.scrollWidth > node.clientWidth + 2)}));
        assert.deepEqual(overflow, {document: false, panel: false});
        for (const selector of ['#fm-modal-cancel', '#fm-modal-insert']) {const box = await page.locator(selector).boundingBox(); assert(box && box.x >= -1 && box.x + box.width <= width + 1 && box.y >= -1 && box.y + box.height <= height + 1);}
        await page.screenshot({path: path.join(h.data, `metadata-${language}-${width}-${theme}.png`)});
        await page.locator('#fm-modal-cancel').click(); assert.equal(await page.evaluate(() => getEditContent()), paper);
        await page.evaluate(() => openExportModal()); await page.locator('#export-tab-docx').click();
        const customize = page.locator('#export-box .export-customize');
        if (!await customize.evaluate(element => element.open)) await customize.locator(':scope > summary').click();
        const host = page.locator('#publication-metadata-export'); await h.until(() => host.count().then(count => count === 1));
        const section = page.locator('#export-opts .exp-sec').filter({has: host});
        if (await section.count() && !await section.evaluate(element => element.classList.contains('open'))) await section.locator('.exp-sec-head').click();
        const inherited = host.locator('[data-pm-field="authors"] [data-pm-state]');
        assert.equal(await inherited.locator('option[value="inherit"]').textContent(), await page.evaluate(() => i18n.dict['pm.inherit']));
        assert.equal(await inherited.locator('option[value="clear"]').textContent(), await page.evaluate(() => i18n.dict['pm.clear']));
        assert.equal(await host.evaluate(element => element.scrollWidth > element.clientWidth + 2), false);
        const run = await page.locator('#export-run').boundingBox(); assert(run && run.y >= -1 && run.y + run.height <= height + 1);
        await page.screenshot({path: path.join(h.data, `metadata-export-${language}-${width}-${theme}.png`)});
        await page.evaluate(() => closeExportModal()); assert.equal(await page.evaluate(() => getEditContent()), paper);
      }
      h.unchanged(initial);
    });
    h.report.outputs = outputs;
    } finally { if (reuseEvidence) reuseEvidence.assertPreserved(); }
  });
}
module.exports = {resolveEngineReuseProfile, verifyEngineProfileReuse, prepareEngineProfileReuse, readSmallcapsProof, checkNativeTexMetadata};
if (require.main === module) run().catch(error => {console.error(error.stack || error.message); process.exitCode = 1;});
