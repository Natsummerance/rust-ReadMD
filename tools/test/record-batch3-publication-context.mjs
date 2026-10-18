// Archive a tested P01/P07 candidate without copying runtimes, profiles or
// unknown-license upstream citation fixtures. The explicit plan is QA input.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {fileURLToPath} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function local(name) {
  if (typeof name !== 'string' || path.isAbsolute(name)) throw Error('Relative workspace path required');
  if(name.split(/[\\/]/).some(part=>part==='..'))throw Error('Evidence paths cannot contain parent traversal');
  const file = path.resolve(root, name), relative = path.relative(root, file);
  if (!relative || relative.startsWith('..') || path.isAbsolute(relative)) throw Error('Workspace path required');
  let current = root;
  for (const part of relative.split(path.sep)) {
    current = path.join(current, part);
    if (fs.existsSync(current) && fs.lstatSync(current).isSymbolicLink()) throw Error('Evidence cannot traverse links');
  }
  return file;
}
const read = name => fs.readFileSync(local(name));
const json = name => JSON.parse(read(name));
const checkOnly = process.argv.includes('--check');
const plan = json(process.argv[2]);
if (plan.schema !== 1 || !Array.isArray(plan.artifacts) || !Array.isArray(plan.sources) || !Array.isArray(plan.qaSources)) throw Error('Invalid archive plan');
if(plan.readyForArchiver!==true||plan.pending?.length)throw Error('Archive plan still has pending verification');
const target = plan.destination, output = plan.manifest;
if (!target.startsWith('docs/evidence/capability-upgrade/') || !output.startsWith('docs/evidence/capability-upgrade/')) throw Error('Evidence destination required');
if (fs.existsSync(local(target)) || fs.existsSync(local(output))) throw Error('Immutable evidence already exists');
const binary = '.cache/community-upgrade/build/debug/readmd.exe';
const candidateSha256 = sha(read(binary)), bootSha256 = sha(read('assets/readmd.boot.js'));
if(plan.candidateSha256!==candidateSha256||plan.bootSha256!==bootSha256)throw Error('Archive plan candidate changed');
for(const name of plan.sources){
  const frozen=plan.sourceSnapshot?.find(entry=>entry.path===name),data=read(name);
  if(!frozen||frozen.sha256!==sha(data)||frozen.bytes!==data.length)throw Error('Product source changed after plan freeze: '+name);
}
const native = Object.fromEntries(Object.entries(plan.reports).map(([kind, name]) => [kind, {path: name, report: json(name)}]));
const candidate=report=>report.candidateSha256||report.binarySha256;
for (const kind of ['metadata', 'citations', 'mcp', 'engines', 'layout']) {
  const item = native[kind];
  if (!item || !item.report.passed || candidate(item.report) !== candidateSha256 || item.report.noAutomaticRetries !== true) throw Error('Unverified candidate: ' + kind);
  if (kind !== 'mcp' && (item.report.nativeCsp !== true || item.report.bootSha256 !== bootSha256 || item.report.servedBootSha256 !== bootSha256)) throw Error('Production CSP/bundle mismatch: ' + kind);
  if (!item.report.checks.length || item.report.checks.some(check => !check.passed)) throw Error('Failed native workflow: ' + kind);
}
const layout = native.layout.report;
if(layout.composite||layout.compositeMandatoryGroups||layout.engineEvidence?.path!==native.engines.path||layout.engineEvidence.sha256!==sha(read(native.engines.path)))throw Error('Layout must bind the complete successful engine execution');
if(layout.cases?.length!==4||new Set(layout.cases.map(item=>item.id)).size!==4||layout.outputs?.length!==20||new Set(layout.outputs.map(item=>item.file)).size!==20||layout.outputs.filter(item=>item.file.endsWith('.pdf')).length!==12)throw Error('All four real layout cases and twenty distinct outputs required');
for(const id of ['L1-E0','L1-E2','L3-E0','L3-E2']){
  const item=layout.cases.find(item=>item.id===id);
  if(!item||item.outputs?.length!==5||item.outputs.some(output=>output.validated!==true))throw Error('Incomplete native layout case: '+id);
  const keys=item.outputs.map(output=>output.engine||output.format).sort();
  if(keys.join(',')!==['docx','epub','rust','typst','xelatex'].sort().join(','))throw Error('Five different formats/engines required: '+id);
  for(const output of item.outputs){
    const expected=id+'-'+(output.engine||output.format)+'.'+output.format;
    const flat=layout.outputs.find(entry=>entry.file===expected);
    if(output.file!==expected||!flat||flat.sha256!==output.sha256||flat.bytes!==output.bytes||output.response?.status!==200||output.response.data?.ok!==true)throw Error('Case/actual-file binding mismatch: '+id);
    if(output.engine&&(output.format!=='pdf'||output.comparison?.passed!==true||!output.geometry))throw Error('Missing physical layout comparison: '+id+'/'+output.engine);
    const inspected=path.posix.join(path.posix.dirname(native.layout.path),output.inspection||'');
    if(output.inspection!==expected+'.inspection.json'||sha(read(inspected))!==output.inspectionSha256||!plan.artifacts.some(entry=>entry.source===inspected))throw Error('Unbound physical layout inspection: '+expected);
  }
}
for(const output of layout.outputs){
  const file=path.posix.join(path.posix.dirname(native.layout.path),'outputs',output.file),data=read(file);
  if(output.validated!==true||sha(data)!==output.sha256||data.length!==output.bytes||!plan.artifacts.some(entry=>entry.source===file))throw Error('Unbound actual layout output: '+file);
}
const matrix = json(path.posix.join(path.posix.dirname(native.metadata.path), native.metadata.report.fieldMatrix.path));
function prior(file,digest,count){
  file=file.replaceAll('\\','/');const bytes=read(file),report=JSON.parse(bytes);
  if(sha(bytes)!==digest||candidate(report)!==candidateSha256||report.passed!==false||report.checks.slice(0,count).some(check=>!check.passed)||report.checks[count]?.passed!==false)throw Error('Original failed execution or successful prefix changed');
  return {path:file,report};
}
const metadataPrior=native.metadata.report.composite?prior(path.posix.join(path.posix.dirname(native.metadata.path),native.metadata.report.priorExecution.report),native.metadata.report.priorExecution.sha256,15):null;
const citationComposite=native.citations.report.compositeMandatoryGroups;
const citationPrior=citationComposite?prior(native.citations.report.compositeProvenance.originalReport,native.citations.report.compositeProvenance.originalReportSha256,citationComposite.inheritedPassed):null;
if(metadataPrior&&native.metadata.report.checks.slice(0,15).some((check,index)=>check.name!==metadataPrior.report.checks[index].name))throw Error('Metadata prefix must reference the actual successful checks');
if(citationPrior&&(![[10,4],[13,1]].some(([inherited,actual])=>citationComposite.inheritedPassed===inherited&&citationComposite.actualContinuationPassed===actual)||citationComposite.expectedTotal!==14||!citationComposite.passed||native.citations.report.checks.length!==citationComposite.actualContinuationPassed||native.citations.report.compositeProvenance.inheritedStages.length!==citationComposite.inheritedPassed||native.citations.report.compositeProvenance.inheritedStages.some((check,index)=>JSON.stringify(check)!==JSON.stringify(citationPrior.report.checks[index]))))throw Error('Citation composition must reference the actual successful prefix and only the previously unvalidated groups');
if(native.metadata.report.checks.length!==16||(citationComposite?citationComposite.expectedTotal:native.citations.report.checks.length)!==14||native.mcp.report.checks.length!==7)throw Error('All final workflow groups are required');
if (!matrix.passed || matrix.plannedCaseCount !== 35 || matrix.passedCaseCount !== 35 || matrix.counts.validated !== 111 || matrix.counts.failed || matrix.counts.notRun) throw Error('Complete metadata output matrix required');
if(Object.keys(matrix.baselines).length!==2||matrix.cases.length!==35)throw Error('Exactly two baselines and thirty-five field cases required');
if(sha(read(path.posix.join(path.posix.dirname(native.metadata.path),native.metadata.report.fieldMatrix.path)))!==native.metadata.report.fieldMatrix.sha256)throw Error('Metadata matrix changed');
let checkedMetadataOutputs=0;
const uniqueMetadataOutputs=new Set();
for(const item of [...Object.values(matrix.baselines),...matrix.cases]){
  if(item.passed===false)throw Error('Failed metadata case');
  if(Object.keys(item.outputs).sort().join(',')!=='docx,epub,pdf')throw Error('Each metadata case requires all three actual formats');
  for(const [format,artifact] of Object.entries(item.outputs)){
    if(!['docx','pdf','epub'].includes(format)||artifact.status!=='passed'||!artifact.validated||!artifact.inputsUnchanged)throw Error('Unvalidated metadata output');
    const file=path.posix.join(path.posix.dirname(native.metadata.path),'outputs',artifact.file),data=read(file);
    if(data.length!==artifact.bytes||sha(data)!==artifact.sha256||!plan.artifacts.some(entry=>entry.source===file))throw Error('Missing or changed actual metadata output: '+file);
    if(uniqueMetadataOutputs.has(file))throw Error('Duplicated metadata output: '+file);uniqueMetadataOutputs.add(file);
    checkedMetadataOutputs++;
  }
}
if(checkedMetadataOutputs!==111)throw Error('All 111 actual outputs must be archived');
const fullMetadataOutputs=native.metadata.report.outputs||metadataPrior?.report.outputs;
if(!Array.isArray(fullMetadataOutputs)||fullMetadataOutputs.length!==121||new Set(fullMetadataOutputs.map(item=>item.file)).size!==121)throw Error('All 121 native output bindings required');
for(const output of fullMetadataOutputs){
  const file=path.posix.join(path.posix.dirname(native.metadata.path),'outputs',output.file),data=read(file);
  if(sha(data)!==output.sha256||(output.bytes!==undefined&&data.length!==output.bytes)||!plan.artifacts.some(entry=>entry.source===file))throw Error('Unbound native metadata output: '+file);
}
const epubSummary=native.metadata.report.epubcheck;
const epubProofFile=path.posix.join(path.posix.dirname(native.metadata.path),Array.isArray(epubSummary)?'epubcheck.json':epubSummary.path),epubProof=json(epubProofFile);
if(!epubProof.passed||epubProof.files.length!==40||!epubProof.offline||!plan.artifacts.some(entry=>entry.source===epubProofFile))throw Error('Actual offline EPUBCheck results required');
if(new Set(epubProof.files.map(file=>file.name)).size!==40)throw Error('EPUB validator files must be distinct');
for(const file of epubProof.files){
  const actual=path.posix.join(path.posix.dirname(native.metadata.path),'outputs',file.name);
  if(!file.passed||file.exitCode!==0||Object.values(file.counts).some(count=>count!==0)||sha(read(actual))!==file.sha256||!plan.artifacts.some(entry=>entry.source===actual))throw Error('Actual EPUB validation/output mismatch: '+file.name);
}
if(Array.isArray(epubSummary)){
  if(epubSummary.length!==40||epubSummary.some(entry=>!entry.sha256||!epubProof.files.some(file=>file.name===entry.name&&file.sha256===entry.sha256)||Object.values(entry.counts).some(count=>count!==0)))throw Error('Full-run EPUBCheck summary does not match actual validator evidence');
}else if(sha(read(epubProofFile))!==epubSummary.sha256)throw Error('EPUBCheck evidence changed');
const epubProofBinding={path:epubProofFile,sha256:sha(read(epubProofFile)),files:40,offline:true};
const oracle = native.citations.report.oracle;
if (oracle?.mandatoryPassed !== true || oracle.mandatoryFixtures !== 20 || oracle.mandatoryFailures !== 0) throw Error('Independent citation expectations must pass');
const full = read(plan.libraryLog).toString().match(/test result: ok\. (\d+) passed; 0 failed; (\d+) ignored;[^\n]*finished in ([\d.]+)s/);
if (!full || +full[1] < 2471 || +full[2] !== 3) throw Error('Full offline Rust baseline missing');
if (!read(plan.buildLog).toString().includes('Finished')) throw Error('Offline native build missing');
const gates = json(plan.gates);
if (gates.length < 8 || gates.some(gate => gate.exitCode !== 0)) throw Error('Repository gates must pass');
for(const name of ['assets','wiring','styles','i18n','no-python','ledger','checker-tests','boot']){
  const matching=gates.filter(gate=>gate.name===name);
  if(matching.length!==1||!matching[0].sha256||sha(read(matching[0].log))!==matching[0].sha256)throw Error('Repository gate log must be unique and unchanged: '+name);
}
const visual = json(plan.visual);
if (!visual.passed || visual.candidateSha256 !== candidateSha256 || !visual.images?.length) throw Error('Actual visual review required');
for(const entry of visual.images)if(!entry.passed||sha(read(entry.path))!==entry.sha256)throw Error('Visual evidence changed');
const previous = json(plan.previous), oldSources = json(previous.sourceManifest.path);
if (sha(read(previous.sourceManifest.path)) !== previous.sourceManifest.sha256) throw Error('Historical source manifest changed');
const old = new Map([...oldSources.sources, ...(oldSources.qaSources || [])].map(entry => [entry.path, entry]));
const files = [], used = new Set(); let bytes = 0;
function charge(data){
  if(data.length>24*1024*1024||bytes+data.length>160*1024*1024)throw Error('Evidence size budget exceeded');
  bytes+=data.length;
}
function copy(source, destination) {
  const canonical=path.relative(root,local(source)).replaceAll(path.sep,'/');
  if (/^(?:\.cache\/private\/)|^\.cache\/capability-upgrade\/(?:tex-inputs|locked-packs)\/|(?:^|\/)(?:managed-v2|webview)(?:\/|$)|(?:^|\/)(?:settings\.json|Cookies|Local State|readmd\.db(?:-[\w]+)?|[^/]*\.readmd-pack)$/i.test(canonical)) throw Error('Private/runtime resource excluded: ' + source);
  const data = read(source);
  charge(data);
  const name = target + '/' + destination;
  if(path.relative(local(target),local(name)).startsWith('..'))throw Error('Archive destination escaped target');
  if (used.has(name)) throw Error('Duplicate archive destination: ' + name);
  used.add(name); if(!checkOnly){fs.mkdirSync(path.dirname(local(name)), {recursive: true}); fs.writeFileSync(local(name), data, {flag: 'wx'});}
  const entry = {path: name, sha256: sha(data), bytes: data.length}; files.push(entry); return entry;
}
function object(value, name) {
  const data = Buffer.from(JSON.stringify(value, null, 2) + '\n'), file = target + '/' + name;
  charge(data);
  if (used.has(file)) throw Error('Duplicate archive destination'); used.add(file);
  if(!checkOnly){fs.mkdirSync(path.dirname(local(file)), {recursive: true}); fs.writeFileSync(local(file), data, {flag: 'wx'});}
  const entry = {path: file, sha256: sha(data), bytes: data.length}; files.push(entry); return entry;
}
const productNames = [...new Set([...oldSources.sources.map(entry => entry.path), ...plan.sources])].sort();
const collected = new Map();
function source(name) {
  if (collected.has(name)) return collected.get(name);
  const digest = sha(read(name)), historical = old.get(name);
  if (historical?.sha256 === digest) {
    if (sha(read(historical.archivedAt)) !== digest) throw Error('Immutable source archive changed: ' + name);
    collected.set(name, historical);
    return historical;
  }
  const archived = copy(name, 'source/' + name), entry = {path: name, sha256: digest, archivedAt: archived.path};
  collected.set(name, entry); return entry;
}
const sources = productNames.map(source), qaSources = [...new Set(plan.qaSources)].sort().map(source);
const qaVersions=(plan.qaVersions||[]).map(version=>{
  if(!version.source.startsWith('.cache/capability-upgrade/'))throw Error('Owned original QA snapshot required');
  local(version.path);const digest=sha(read(version.source));
  if(digest!==version.sha256)throw Error('Original QA snapshot changed: '+version.path);
  const archived=copy(version.source,'source/qa-versions/'+digest+'/'+version.path);
  return {path:version.path,sha256:digest,archivedAt:archived.path};
});
const known=new Map();
for(const entry of [...sources,...qaSources,...qaVersions]){
  if(!known.has(entry.path))known.set(entry.path,new Set());known.get(entry.path).add(entry.sha256);
}
for (const {report} of [...Object.values(native),...[metadataPrior,citationPrior].filter(Boolean)]) {
  if (![...known.values()].some(versions=>versions.has(report.scriptSha256))) throw Error('Missing final QA script archive');
  for (const [name, digest] of Object.entries(report.qaDependencySha256 || {})) if (!known.get(name)?.has(digest)) throw Error('Stale or unarchived QA dependency: ' + name);
}
const sourceManifest = object({recordedAt: new Date().toISOString(), sources, qaSources, qaVersions}, 'source-manifest.json');
for (const artifact of plan.artifacts) {
  if (!artifact.source.startsWith('.cache/capability-upgrade/')) throw Error('Only explicit owned QA artifacts may be archived');
  const data=read(artifact.source);
  if(!artifact.sha256||sha(data)!==artifact.sha256||data.length!==artifact.bytes)throw Error('Artifact changed after plan freeze: '+artifact.source);
  copy(artifact.source, artifact.destination);
}
copy(process.argv[2], 'archive-plan.json');
const manifest = {
  schema: 1, recordedAt: new Date().toISOString(), status: 'passed-limited-windows-publication-metadata-citations',
  scope: plan.scope, previous: {path: plan.previous, sha256: sha(read(plan.previous))}, sourceManifest,
  binary: {path: binary, sha256: candidateSha256, bytes: read(binary).length}, bootSha256,
  library: {passed: +full[1], failed: 0, ignored: +full[2], seconds: +full[3]},
  native: Object.entries(native).map(([kind, {path: file, report}]) => {
    const archived = plan.artifacts.find(artifact => artifact.source === file);
    if (!archived) throw Error('Final native report must be archived: ' + kind);
    return {kind, report: target + '/' + archived.destination, sha256: sha(read(file)), checks: report.compositeMandatoryGroups?.expectedTotal||report.checks.length,
      actualFinalExecutionChecks:report.checks.filter(check=>check.execution!=='prior-successful-stage').length,
      composite:Boolean(report.composite||report.compositeMandatoryGroups),singleRunFullSuitePassed:report.singleRunFullSuitePassed??!(report.composite||report.compositeMandatoryGroups),
      transport: report.transport || 'native'};
  }),
  metadata: {cases: 35, baselines: 2, actualValidatedArtifacts: 111, formats: matrix.formats, epubcheck: epubProofBinding},
  citations: {mandatoryFixtures: oracle.mandatoryFixtures, mandatoryPassed: oracle.mandatoryFixtures-oracle.mandatoryFailures, mandatoryFailures: 0, optionalCompatibility: oracle.optionalCompatibility},
  citationLayout: {cases:4,actualOutputs:20,actualPdfs:12,engines:['rust','typst','xelatex'],tolerance:layout.tolerance,singleLineUnits:layout.singleLineUnits,engineEvidence:layout.engineEvidence},
  repositoryGates: {passed: true, count: gates.length}, visualInspection: visual,
  limitations: plan.limitations, actualWordApplication: 'unverified', independentEpubReader: 'unverified',
  fullRequirementCertification: false, fullBatch3Certification: false, releaseAccepted: false, files,
};
const manifestBytes=Buffer.from(JSON.stringify(manifest,null,2)+'\n');
if(manifestBytes.length>24*1024*1024||bytes+manifestBytes.length>160*1024*1024)throw Error('Final manifest exceeds evidence size budget');
if(!checkOnly){fs.mkdirSync(path.dirname(local(output)), {recursive: true}); fs.writeFileSync(local(output), manifestBytes, {flag: 'wx'});}
console.log(JSON.stringify({checkOnly,manifest: output, sha256: sha(manifestBytes), files: files.length, archivedBytes: bytes,manifestBytes:manifestBytes.length,totalArchivedBytes:bytes+manifestBytes.length,checkedMetadataOutputs}));
