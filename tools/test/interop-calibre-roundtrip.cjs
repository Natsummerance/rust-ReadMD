'use strict';
// A fixed synthetic book exercises the external console conversion engine only.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const { ROOT, inspectPe, consoleEnvironment } = require('./interop-diagnostics.cjs');
const { crc32 } = require('../../ui-tests/zip-fixtures.cjs');
const zipParts = require('../../ui-tests/zip-parts.cjs');
const sha256 = value => crypto.createHash('sha256').update(value).digest('hex');
const OUTPUT = path.join(ROOT, '.cache/capability-upgrade/longtail-20261019/interop');

function storedZip(entries) {
  const locals = [], directory = [];
  let offset = 0;
  for (const [name, value] of entries) {
    const filename = Buffer.from(name), bytes = Buffer.from(value), crc = crc32(bytes);
    const local = Buffer.alloc(30); local.writeUInt32LE(0x04034b50); local.writeUInt16LE(20, 4); local.writeUInt16LE(0x800, 6); local.writeUInt16LE(33, 12);
    local.writeUInt32LE(crc, 14); local.writeUInt32LE(bytes.length, 18); local.writeUInt32LE(bytes.length, 22); local.writeUInt16LE(filename.length, 26);
    const central = Buffer.alloc(46); central.writeUInt32LE(0x02014b50); central.writeUInt16LE(20, 4); central.writeUInt16LE(20, 6); central.writeUInt16LE(0x800, 8); central.writeUInt16LE(33, 14);
    central.writeUInt32LE(crc, 16); central.writeUInt32LE(bytes.length, 20); central.writeUInt32LE(bytes.length, 24); central.writeUInt16LE(filename.length, 28); central.writeUInt32LE(offset, 42);
    locals.push(local, filename, bytes); directory.push(central, filename); offset += local.length + filename.length + bytes.length;
  }
  const central = Buffer.concat(directory), end = Buffer.alloc(22); end.writeUInt32LE(0x06054b50); end.writeUInt16LE(entries.length, 8); end.writeUInt16LE(entries.length, 10); end.writeUInt32LE(central.length, 12); end.writeUInt32LE(offset, 16);
  return Buffer.concat([...locals, central, end]);
}

function createEpub() {
  const page = body => '<?xml version="1.0" encoding="UTF-8"?><html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops" lang="en"><head><title>Synthetic console interoperability</title></head><body>' + body + '</body></html>';
  return storedZip([
    ['mimetype', 'application/epub+zip'],
    ['META-INF/container.xml', '<?xml version="1.0"?><container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="EPUB/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>'],
    ['EPUB/book.opf', '<?xml version="1.0" encoding="UTF-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="book-id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="book-id">urn:uuid:ea5a8bce-5000-4000-8000-000000000019</dc:identifier><dc:title>Synthetic console interoperability</dc:title><dc:language>en</dc:language><meta property="dcterms:modified">2026-10-19T00:00:00Z</meta></metadata><manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="one" href="one.xhtml" media-type="application/xhtml+xml" properties="mathml"/><item id="two" href="two.xhtml" media-type="application/xhtml+xml"/><item id="notes" href="notes.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="one"/><itemref idref="two"/><itemref idref="notes"/></spine></package>'],
    ['EPUB/nav.xhtml', page('<nav epub:type="toc" id="toc"><h1>Contents</h1><ol><li><a href="one.xhtml#first">SYNTHETIC-FIRST</a></li><li><a href="two.xhtml#second">SYNTHETIC-SECOND</a></li></ol></nav>')],
    ['EPUB/one.xhtml', page('<h1 id="first">SYNTHETIC-FIRST</h1><p>Owned original material: 中文 😀.</p><p><a href="two.xhtml#second">Go to second</a>; <a id="note-ref" epub:type="noteref" role="doc-noteref" href="notes.xhtml#note-one">Read the note</a>.</p><p>Inline <math xmlns="http://www.w3.org/1998/Math/MathML"><mi>x</mi><mo>+</mo><mn>1</mn></math>.</p><math xmlns="http://www.w3.org/1998/Math/MathML" display="block"><mfrac><mi>α</mi><mi>β</mi></mfrac></math>')],
    ['EPUB/two.xhtml', page('<h1 id="second">SYNTHETIC-SECOND</h1><p><a href="one.xhtml#first">Go to first</a>.</p><table><tr><th>Key</th><th>Value</th></tr><tr><td>Synthetic A</td><td>3</td></tr><tr><td>Synthetic B</td><td>8</td></tr></table>')],
    ['EPUB/notes.xhtml', page('<h1>Notes</h1><aside id="note-one" epub:type="footnote" role="doc-footnote"><p>SYNTHETIC-NOTE Original synthetic note.</p><p><a href="one.xhtml#first">Back to first</a></p></aside>')]
  ]);
}

const INSPECT_ARCHIVE = String.raw`
import json, posixpath, sys, urllib.parse, zipfile
from html.parser import HTMLParser
import xml.etree.ElementTree as ET
class Parser(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True); self.nodes=[]; self.stack=[]
    def handle_starttag(self, tag, attrs):
        node={'tag':tag.split(':')[-1], 'attrs':dict(attrs), 'parts':[]}; self.nodes.append(node)
        if self.stack: self.stack[-1]['parts'].append(node)
        if tag not in ('meta','link','br','hr','img','input','source','wbr'): self.stack.append(node)
    def handle_startendtag(self, tag, attrs):
        self.handle_starttag(tag,attrs)
        if self.stack and self.stack[-1]['tag']==tag.split(':')[-1]: self.stack.pop()
    def handle_endtag(self, tag):
        for i in range(len(self.stack)-1,-1,-1):
            if self.stack[i]['tag']==tag.split(':')[-1]: del self.stack[i:]; break
    def handle_data(self, data):
        if self.stack: self.stack[-1]['parts'].append(data)
def text(node): return ''.join(text(p) if isinstance(p,dict) else p for p in node['parts'])
def norm(value): return ' '.join(value.split())
with zipfile.ZipFile(sys.argv[1]) as archive:
    pages={}; package_version=None
    for name in archive.namelist():
        if name.lower().endswith(('.html','.xhtml','.htm')):
            parser=Parser(); parser.feed(archive.read(name).decode('utf-8-sig')); pages[name]=parser.nodes
        if name.lower().endswith('.opf'): package_version=ET.fromstring(archive.read(name)).get('version')
    nodes=[(name,node) for name,rows in pages.items() for node in rows]
    content=' '.join(text(node) for name,node in nodes)
    expected={'Go to second':'SYNTHETIC-SECOND','Go to first':'SYNTHETIC-FIRST','Read the note':'SYNTHETIC-NOTE','Back to first':'SYNTHETIC-FIRST'}
    links=[]; network_refs=[]
    for name,node in nodes:
        href=node['attrs'].get('href')
        if not href: continue
        url=urllib.parse.urlsplit(href)
        if url.scheme or url.netloc: network_refs.append(href); continue
        label=norm(text(node))
        if node['tag']!='a' or label not in expected: continue
        member=posixpath.normpath(posixpath.join(posixpath.dirname(name),urllib.parse.unquote(url.path))) if url.path else name
        fragment=urllib.parse.unquote(url.fragment); rows=pages.get(member,[])
        at=next((i for i,row in enumerate(rows) if row['attrs'].get('id')==fragment or row['attrs'].get('name')==fragment),None) if fragment else 0
        context=norm(' '.join(text(row) for row in rows[at:at+12]))[:1024] if at is not None else ''
        links.append({'label':label,'targetMember':member,'fragmentPresent':bool(fragment),'resolved':at is not None and bool(rows),'expectedContextFound':expected[label] in context})
    math=[node for name,node in nodes if node['tag']=='math']
    report={'pageCount':len(pages),'packageVersion':package_version,'markers':{s:s in content for s in ('SYNTHETIC-FIRST','SYNTHETIC-SECOND','SYNTHETIC-NOTE','中文','😀')},
        'links':links,'networkReferenceCount':len(network_refs),'mathCount':len(math),'displayMathCount':sum(node['attrs'].get('display')=='block' for node in math),
        'fractionCount':sum(node['tag']=='mfrac' for name,node in nodes),'mathTokens':sorted(text(node) for name,node in nodes if node['tag'] in ('mi','mn','mo')),
        'noterefCount':sum('noteref' in node['attrs'].get('epub:type','').split() or node['attrs'].get('role')=='doc-noteref' for name,node in nodes),
        'footnoteCount':sum('footnote' in node['attrs'].get('epub:type','').split() or node['attrs'].get('role')=='doc-footnote' for name,node in nodes)}
    print(json.dumps(report,ensure_ascii=True))
`;

function inspectArchive(directory, name, timeoutMs = 10000) {
  const file = path.join(directory, name), bytes = fs.readFileSync(file), parts = zipParts(bytes);
  const result = spawnSync(process.env.READMD_TEST_PYTHON || 'python.exe', ['-I', '-X', 'utf8', '-c', INSPECT_ARCHIVE, name], { cwd: directory, windowsHide: true, encoding: 'utf8', timeout: Math.max(1, Math.min(10000, timeoutMs)), maxBuffer: 1024 * 1024 });
  if (result.status !== 0) throw Error('INDEPENDENT_ARCHIVE_INSPECTION_FAILED');
  return { file: name, bytes: bytes.length, sha256: sha256(bytes), zipMemberCount: parts.size, ...JSON.parse(result.stdout) };
}

function compareFeatures(source, output) {
  const checks = [
    { id: 'synthetic-text', passed: Object.keys(source.markers).every(marker => output.markers[marker] === true) },
    { id: 'crosslinks-and-note-return', passed: ['Go to second', 'Go to first', 'Read the note', 'Back to first'].every(label => output.links.some(link => link.label === label && link.resolved && link.expectedContextFound)) },
    { id: 'mathml-objects', passed: output.mathCount >= source.mathCount && output.displayMathCount >= source.displayMathCount && output.fractionCount >= source.fractionCount && JSON.stringify(output.mathTokens) === JSON.stringify(source.mathTokens) },
    { id: 'epub-note-roles', passed: output.noterefCount >= source.noterefCount && output.footnoteCount >= source.footnoteCount },
    { id: 'no-network-content', passed: output.networkReferenceCount === 0 },
    { id: 'epub3-output', passed: output.packageVersion === '3.0' }
  ];
  return { passed: checks.every(check => check.passed), checks };
}

const OWNED_CONSOLE_WRAPPER = String.raw`
$ErrorActionPreference='Stop'
[Console]::InputEncoding=[Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false)
$clock=[Diagnostics.Stopwatch]::StartNew()
Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
public static class ReadMdOwnedConsoleJob {
 [StructLayout(LayoutKind.Sequential)] struct Basic { public long PerProcess, PerJob; public uint Flags; public UIntPtr Min, Max; public uint Active; public UIntPtr Affinity; public uint Priority, Scheduling; }
 [StructLayout(LayoutKind.Sequential)] struct IO { public ulong A,B,C,D,E,F; }
 [StructLayout(LayoutKind.Sequential)] struct Limits { public Basic Basic; public IO IO; public UIntPtr ProcessMemory,JobMemory,PeakProcess,PeakJob; }
 [DllImport("kernel32.dll",SetLastError=true,CharSet=CharSet.Unicode)] static extern IntPtr CreateJobObjectW(IntPtr attributes,string name);
 [DllImport("kernel32.dll",SetLastError=true)] static extern bool SetInformationJobObject(IntPtr job,int kind,ref Limits limits,uint size);
 [DllImport("kernel32.dll",SetLastError=true)] static extern bool AssignProcessToJobObject(IntPtr job,IntPtr process);
 [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
 [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
 public static IntPtr Own() { IntPtr job=CreateJobObjectW(IntPtr.Zero,null); if(job==IntPtr.Zero) throw new Win32Exception(); Limits limits=new Limits(); limits.Basic.Flags=0x2000; if(!SetInformationJobObject(job,9,ref limits,(uint)Marshal.SizeOf(typeof(Limits))) || !AssignProcessToJobObject(job,GetCurrentProcess())) { int error=Marshal.GetLastWin32Error(); CloseHandle(job); throw new Win32Exception(error); } return job; }
}
'@
try {
 $config=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('__CONFIG__')) | ConvertFrom-Json
 $ownedJob=[ReadMdOwnedConsoleJob]::Own()
 foreach($argument in $config.arguments) { if($argument -notmatch '^[A-Za-z0-9_.-]+$') { throw 'CLI_ARGUMENT_REFUSED' } }
 $process=[Diagnostics.Process]::new(); $process.StartInfo=[Diagnostics.ProcessStartInfo]::new()
 $process.StartInfo.FileName=$config.binary; $process.StartInfo.Arguments=$config.arguments -join ' '; $process.StartInfo.WorkingDirectory=$config.directory
 $process.StartInfo.UseShellExecute=$false; $process.StartInfo.CreateNoWindow=$true; $process.StartInfo.RedirectStandardOutput=$true; $process.StartInfo.RedirectStandardError=$true
 [void]$process.Start(); $stdout=$process.StandardOutput.ReadToEndAsync(); $stderr=$process.StandardError.ReadToEndAsync()
 $remaining=[Math]::Max(1,[int]$config.timeoutMs-[int]$clock.ElapsedMilliseconds)
 if(-not $process.WaitForExit($remaining)) { @{ state='deadline_exceeded'; jobOwned=$true; elapsedMs=$clock.ElapsedMilliseconds } | ConvertTo-Json -Compress; exit 2 }
 $remaining=[Math]::Max(1,[int]$config.timeoutMs-[int]$clock.ElapsedMilliseconds)
 if(-not [Threading.Tasks.Task]::WaitAll([Threading.Tasks.Task[]]@($stdout,$stderr),$remaining)) { @{ state='output_deadline_exceeded'; jobOwned=$true; elapsedMs=$clock.ElapsedMilliseconds } | ConvertTo-Json -Compress; exit 2 }
 @{ state='completed'; jobOwned=$true; exitCode=$process.ExitCode; elapsedMs=$clock.ElapsedMilliseconds; stdout=$stdout.Result; stderr=$stderr.Result } | ConvertTo-Json -Depth 4 -Compress
} catch { @{ state='owned_cli_unverified'; failure='CLI_JOB_OR_LAUNCH_FAILED'; elapsedMs=$clock.ElapsedMilliseconds } | ConvertTo-Json -Compress; exit 2 }
# The job handle is deliberately retained until this helper exits. Windows closes
# it on normal exit or forced timeout, terminating only this owned process tree.
`;

function redactLog(value, directory, binary) {
  let text = String(value || '');
  const replacements = [[directory, '<owned-run>'], [binary, '<calibre-console>'], [ROOT, '<workspace>'], [process.env.USERPROFILE, '<user-profile>']].filter(([from]) => from).sort((a, b) => b[0].length - a[0].length);
  for (const [from, to] of replacements) text = text.split(from).join(to).split(from.replaceAll('\\', '/')).join(to);
  return text.replace(/[A-Za-z]:[\\/][^\r\n"<>]*/g, '<absolute-path>').replace(/((?:api[_-]?key|secret|token)\s*[:=]\s*)[^\s,;]+/gi, '$1<redacted>');
}

function runOwnedConsole(binary, args, directory, name, timeoutMs) {
  if (timeoutMs < 1000) return { stage: name, state: 'total_deadline_exhausted', launched: false };
  const config = Buffer.from(JSON.stringify({ binary, arguments: args, directory, timeoutMs: Math.max(1, timeoutMs - 500) }), 'utf8').toString('base64');
  const script = OWNED_CONSOLE_WRAPPER.replace('__CONFIG__', config);
  const bootstrap = '[Console]::InputEncoding=[Text.UTF8Encoding]::new($false); $source=[Console]::In.ReadToEnd(); & ([ScriptBlock]::Create($source))';
  const started = Date.now();
  const result = spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-EncodedCommand', Buffer.from(bootstrap, 'utf16le').toString('base64')], {
    input: script, cwd: directory, windowsHide: true, encoding: 'utf8', timeout: timeoutMs, maxBuffer: 2 * 1024 * 1024, env: consoleEnvironment(path.join(directory, 'isolation'))
  });
  let record;
  try { record = JSON.parse(result.stdout.trim().replace(/^\uFEFF/, '')); } catch { record = { state: 'wrapper_result_incomplete' }; }
  for (const stream of ['stdout', 'stderr']) {
    fs.writeFileSync(path.join(directory, name + '.' + stream + '.redacted.log'), redactLog(record[stream], directory, binary), { flag: 'wx' });
    delete record[stream];
  }
  return { stage: name, ...record, outerElapsedMs: Date.now() - started, wrapperExitCode: result.status, outerTimedOut: result.error?.code === 'ETIMEDOUT', outerErrorCode: result.error?.code || null };
}

function main(args = process.argv.slice(2)) {
  if (args.length === 1 && args[0] === '--plan') {
    console.log(JSON.stringify({ mode: 'calibre-console-plan', syntheticInputSha256: sha256(createEpub()), stages: ['EPUB3 to HTMLZ', 'HTMLZ to EPUB3'], graphicalApplicationsLaunched: false, acceptance: 'not_run' }));
    return;
  }
  if (args.length || process.platform !== 'win32') throw Error('CALIBRE_ROUNDTRIP_ARGUMENTS_OR_PLATFORM');
  const binary = process.env.READMD_CALIBRE_CONVERT_BIN;
  if (!binary || path.basename(binary).toLowerCase() !== 'ebook-convert.exe') throw Error('EXPLICIT_CALIBRE_CONSOLE_REQUIRED');
  if (fs.statSync(binary).size > 2 * 1024 * 1024) throw Error('CALIBRE_CONSOLE_LAUNCHER_SIZE_OUTSIDE_SCOPE');
  const bytes = fs.readFileSync(binary), identity = { ...inspectPe(bytes), sha256: sha256(bytes), bytes: bytes.length };
  if (!identity.verified || !identity.console) throw Error('CALIBRE_CONSOLE_IDENTITY_UNVERIFIED');
  fs.mkdirSync(OUTPUT, { recursive: true });
  const directory = fs.mkdtempSync(path.join(OUTPUT, 'calibre-roundtrip-'));
  const source = createEpub(); fs.writeFileSync(path.join(directory, 'source.epub'), source, { flag: 'wx' });
  const report = { schemaVersion: 1, recordedAt: new Date().toISOString(), mode: 'external-console-engine-roundtrip', passed: false, acceptance: 'unverified', productExportUsed: false, synthetic: true,
    identity, scripts: [{ file: 'interop-calibre-roundtrip.cjs', sha256: sha256(fs.readFileSync(__filename)) }], stages: [], inspections: [],
    safety: { graphicalApplicationsLaunched: false, officeActivationAttempted: false, existingProcessesReusedOrStopped: false, isolatedChildEnvironment: true, ownedWindowsJobRequired: true, networkFeaturesRequested: false },
    limitations: ['Calibre console conversion is supplementary evidence; it does not certify I06/I07 reader navigation, rendering, settings or restart.', 'The fixed synthetic EPUB was independently assembled, not exported by a ReadMD candidate.', 'Only synthetic text, local links/notes, MathML and EPUB note roles are compared; visual fidelity is not inspected.'] };
  const started = Date.now(), deadline = started + 90000;
  try {
    report.inspections.push(inspectArchive(directory, 'source.epub', deadline - Date.now()));
    const stages = [
      { name: 'epub-to-htmlz', args: ['source.epub', 'intermediate.htmlz'], output: 'intermediate.htmlz' },
      { name: 'htmlz-to-epub', args: ['intermediate.htmlz', 'roundtrip.epub', '--epub-version', '3'], output: 'roundtrip.epub' }
    ];
    for (const stage of stages) {
      const execution = runOwnedConsole(binary, stage.args, directory, stage.name, Math.max(0, deadline - Date.now())); report.stages.push(execution);
      if (execution.state !== 'completed' || execution.exitCode !== 0) throw Error('CALIBRE_CONVERSION_UNVERIFIED');
      report.inspections.push(inspectArchive(directory, stage.output, deadline - Date.now()));
    }
    report.comparison = compareFeatures(report.inspections[0], report.inspections.at(-1));
    report.passed = report.comparison.passed;
    report.acceptance = report.passed ? 'scoped_console_roundtrip' : 'console_roundtrip_with_observed_losses';
    if (!report.passed) report.failure = 'CALIBRE_FEATURE_RETENTION_FAILED';
  } catch (error) { report.failure = /^[A-Z_]+$/.test(error.message) ? error.message : 'CALIBRE_ROUNDTRIP_UNVERIFIED'; }
  report.totalElapsedMs = Date.now() - started;
  report.totalDeadlineMs = 90000;
  report.sourceUnchanged = sha256(fs.readFileSync(path.join(directory, 'source.epub'))) === sha256(source);
  fs.writeFileSync(path.join(directory, 'report.json'), JSON.stringify(report, null, 2) + '\n', { flag: 'wx' });
  console.log(JSON.stringify({ report: path.relative(ROOT, path.join(directory, 'report.json')), passed: report.passed, acceptance: report.acceptance, failure: report.failure, checks: report.comparison?.checks }));
  if (!report.passed) process.exitCode = 2;
  return { directory, report };
}

module.exports = { createEpub, inspectArchive, compareFeatures, redactLog, runOwnedConsole, main };
if (require.main === module) { try { main(); } catch { console.error('CALIBRE_ROUNDTRIP_PREFLIGHT_FAILED'); process.exitCode = 1; } }
