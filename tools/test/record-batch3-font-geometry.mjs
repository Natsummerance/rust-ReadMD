// Freeze one tested candidate, reusing immutable older source archives.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const sha = b => crypto.createHash('sha256').update(b).digest('hex');
const relative = f => path.relative(root, f).replaceAll('\\', '/');
function local(name) {
  const file = path.resolve(root, name), rel = path.relative(root, file);
  if (!rel || rel.startsWith('..') || path.isAbsolute(rel)) throw Error('Workspace path required');
  return file;
}
const bytes = n => fs.readFileSync(local(n));
const json = n => JSON.parse(bytes(n));
const reports = process.argv.slice(2).map(file => ({file, report: json(file)}));
if (reports.length < 4) throw Error('Supply the final geometry, two compatibility and MCP reports');
const dest = 'docs/evidence/capability-upgrade/2026-10-13/font-geometry';
const output = 'docs/evidence/capability-upgrade/2026-10-13/batch3-font-geometry-manifest.json';
if (fs.existsSync(local(dest)) || fs.existsSync(local(output))) throw Error('Frozen evidence must not already exist');
const binary = '.cache/community-upgrade/build/debug/readmd.exe';
const binaryHash = sha(bytes(binary)), bootHash = sha(bytes('assets/readmd.boot.js'));
const fullLog = '.cache/capability-upgrade/batch3-font-geometry-final-full.log';
const buildLog = '.cache/capability-upgrade/batch3-font-geometry-final-build.log';
const full = bytes(fullLog).toString().match(/test result: ok\. (\d+) passed; 0 failed; (\d+) ignored;[^\n]*finished in ([\d.]+)s/);
if (!full || +full[1] < 2404 || +full[2] !== 3) throw Error('Full Rust baseline is missing');
if (!bytes(buildLog).toString().includes('Finished')) throw Error('Offline native build is missing');
for (const {file, report} of reports) {
  if (!relative(local(file)).startsWith('.cache/capability-upgrade/') || !report.passed || report.binarySha256 !== binaryHash)
    throw Error('Native result/candidate mismatch: ' + file);
  if (report.transport !== 'stdio' && report.bootSha256 !== bootHash) throw Error('Native boot mismatch: ' + file);
}
const current = reports.find(({report:r}) => r.oracleSha256);
const oracleName = 'rust/readmd-kernel/tests/fixtures/math-font-geometry-stix.json';
if (!current || current.report.oracleSha256 !== sha(bytes(oracleName)) || current.report.checks.length < 9)
  throw Error('Nine final font-geometry workflows are required');
const gateFile = '.cache/capability-upgrade/batch3-font-geometry-final-gates.json';
const gates = json(gateFile);
if (gates.length < 8 || gates.some(g => g.exitCode !== 0)) throw Error('Repository gates must pass');
const visualFile = '.cache/capability-upgrade/batch3-font-geometry-visual-review.json';
const visual = json(visualFile);
if (!visual.passed || visual.binarySha256 !== binaryHash || visual.images.length < 3)
  throw Error('Actual image review for this candidate is required');

const files = [];
function copy(source, target) {
  const data = bytes(source), out = local(dest + '/' + target);
  fs.mkdirSync(path.dirname(out), {recursive:true}); fs.writeFileSync(out, data, {flag:'wx'});
  const e = {path:relative(out), sha256:sha(data), bytes:data.length}; files.push(e); return e;
}
function object(value, target) {
  const data = Buffer.from(JSON.stringify(value, null, 2) + '\n'), out = local(dest + '/' + target);
  fs.mkdirSync(path.dirname(out), {recursive:true}); fs.writeFileSync(out, data, {flag:'wx'});
  const e = {path:relative(out), sha256:sha(data), bytes:data.length}; files.push(e); return e;
}
const previousName = 'docs/evidence/capability-upgrade/2026-10-13/batch3-parallel-semantics-manifest.json';
const previous = json(previousName), baseline = json(previous.sourceManifest.path);
if (sha(bytes(previous.sourceManifest.path)) !== previous.sourceManifest.sha256) throw Error('Previous source manifest mismatch');
const sources = baseline.sources.map(old => {
  const hash = sha(bytes(old.path));
  if (hash === old.sha256) {
    if (sha(bytes(old.archivedAt)) !== hash) throw Error('Immutable historical source changed: ' + old.path);
    return old;
  }
  const e = copy(old.path, 'source/' + old.path); return {path:old.path, sha256:hash, archivedAt:e.path};
});
const qaNames = [
  'rust/readmd-kernel/src/math_layout_font_geometry_tests.rs', oracleName,
  'tools/test/generate-math-font-geometry-oracle.py', 'ui-tests/math-font-geometry-native.cjs',
  'ui-tests/math-font-geometry-pdf.cjs', 'ui-tests/math-kern-native.cjs', 'ui-tests/math-small-fractions-native.cjs',
  'tools/test/math-source-operators-mcp.cjs', 'ui-tests/native-session.cjs', 'ui-tests/zip-parts.cjs',
  'ui-tests/helpers/word-fixtures.cjs', 'tools/check-epub.mjs', 'tools/no-python.allow.json',
  'docs/design/v005-batch3-font-geometry-spec.md', 'tools/test/record-batch3-font-geometry.mjs',
];
const qaSources = qaNames.map(n => {const e=copy(n,'source/'+n); return {path:n,sha256:e.sha256,archivedAt:e.path};});
for (const {report:r} of reports) if (r.scriptSha256 && !qaSources.some(s=>s.sha256===r.scriptSha256)) throw Error('Final QA script changed');
if (current.report.pdfInspectorSha256 !== sha(bytes('ui-tests/math-font-geometry-pdf.cjs'))) throw Error('Final PDF inspector changed');
const sourceManifest = object({recordedAt:new Date().toISOString(),sources,qaSources},'source-manifest.json');
for (const n of fs.readdirSync(local('.cache/capability-upgrade')).filter(n=>/^batch3-font-geometry-.*\.(?:log|json)$/.test(n)))
  copy('.cache/capability-upgrade/'+n,'verification/'+n);
for (const n of fs.readdirSync(local('.cache/capability-upgrade')).filter(n=>/^math-font-geometry-(?:run|attempts)-.*\.(?:log|json)$/.test(n)))
  copy('.cache/capability-upgrade/'+n,'diagnostics/'+n);
const entries = [];
function archiveDir(directory,prefix) {
  for (const n of fs.readdirSync(directory)) {
    const f=path.join(directory,n);
    if (fs.statSync(f).isFile() && /\.(?:json|xml|xhtml|svg|png|bmp|log|md|pdf|epub|docx)$/.test(n) && n!=='settings.json') copy(relative(f),prefix+'/'+n);
  }
}
for (const [index,{file,report:r}] of reports.entries()) {
  const dir=path.dirname(local(file)),prefix='native/'+index+'-'+path.basename(dir);
  archiveDir(dir,prefix); if(fs.existsSync(path.join(dir,'documents')))archiveDir(path.join(dir,'documents'),prefix+'/documents');
  entries.push({report:dest+'/'+prefix+'/report.json',checks:r.checks.length,transport:r.transport||'native'});
}
for (const n of fs.readdirSync(local('.cache/capability-upgrade')).filter(n=>n.startsWith('math-font-geometry-native-'))) {
  const reportName='.cache/capability-upgrade/'+n+'/report.json';
  if(fs.existsSync(local(reportName))&&!json(reportName).passed){
    const dir=path.dirname(local(reportName)); archiveDir(dir,'diagnostics/'+n);
    if(fs.existsSync(path.join(dir,'documents')))archiveDir(path.join(dir,'documents'),'diagnostics/'+n+'/documents');
  }
}
const manifest = {
  schema:1,recordedAt:new Date().toISOString(),status:'passed-limited-windows-font-geometry-increment',
  scope:'M01 font-driven stack/skew fractions; M02 real root degree ink/kerns, separate bars, accent attachments and bounded horizontal stretch; phantom composition; source/font-frozen PDF heading/table math re-layout and explicit code paint; no complete Office or platform certification.',
  previous:{path:previousName,sha256:sha(bytes(previousName))},
  binary:{path:binary,sha256:binaryHash,bytes:bytes(binary).length},bootSha256:bootHash,sourceManifest,
  oracle:{path:oracleName,sha256:sha(bytes(oracleName)),fontSha256:json(oracleName).fontSha256,constants:56,glyphs:Object.keys(json(oracleName).glyphs).length},
  library:{passed:+full[1],failed:0,ignored:+full[2],seconds:+full[3],added:+full[1]-2388},
  native:entries.filter(e=>e.transport!=='stdio'),mcp:entries.filter(e=>e.transport==='stdio'),
  repositoryGates:{passed:true,count:gates.length},visualInspection:visual,
  limitations:['No assembly is supplied for STIX wide hats: the final outline stretches only horizontally beyond the largest variant.','Beyond-largest skew slash has no font assembly: explicit uniform enlargement preserves slash coverage but can thicken strokes.','Current body font range is 8–20pt; real 24pt PDF agreement uses the supported heading style. Full publication ranges remain P04.','Mixed per-formula Office document defaults report a conflict in DOCX; source preservation is not certification of Word-local visual equivalence.','Word editing/layout, independent EPUB readers and full OMML/MathML/UnicodeMath shaping remain unverified.','Remaining publication, delivery, pressure and platform acceptance is retained.'],
  actualWordApplication:'unverified',independentEpubReader:'unverified',fullOmmlCertification:false,fullRequirementCertification:false,releaseAccepted:false,files,
};
fs.writeFileSync(local(output),JSON.stringify(manifest,null,2)+'\n',{flag:'wx'});
console.log(JSON.stringify({manifest:output,sha256:sha(bytes(output)),files:files.length,bytes:files.reduce((n,e)=>n+e.bytes,0)}));
