// Freeze only this increment. Reuse hash-checked historical sources instead of
// copying old evidence, browser profiles or capability engines again.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const relative=file=>path.relative(root,file).replaceAll('\\','/');
function local(name){const file=path.resolve(root,name),r=path.relative(root,file);if(!r||r.startsWith('..')||path.isAbsolute(r))throw Error('Path must stay within workspace');return file;}
const args=process.argv.slice(2),destination='docs/evidence/capability-upgrade/2026-10-13/matrix-semantics';
const manifestPath='docs/evidence/capability-upgrade/2026-10-13/batch3-matrix-semantics-manifest.json';
if(args.length<2||fs.existsSync(local(manifestPath)))throw Error('Supply final and regression native report paths; never overwrite a frozen manifest.');
const binary='.cache/community-upgrade/build/debug/readmd.exe',boot='assets/readmd.boot.js',binaryHash=sha(fs.readFileSync(local(binary))),bootHash=sha(fs.readFileSync(local(boot)));
const reports=args.map(name=>{const file=local(name);if(!relative(file).startsWith('.cache/capability-upgrade/'))throw Error('Owned native report required');const report=JSON.parse(fs.readFileSync(file,'utf8'));if(!report.passed||report.binarySha256!==binaryHash||(report.transport!=='stdio'&&report.bootSha256!==bootHash))throw Error('Failed or mismatched native build: '+name);return{file,report};});
const fullLog='.cache/capability-upgrade/batch3-matrix-final-full.log',buildLog='.cache/capability-upgrade/batch3-matrix-final-build.log';
if(!fs.readFileSync(local(fullLog),'utf8').includes('2372 passed; 0 failed; 3 ignored'))throw Error('Final full Rust baseline is required');
if(!fs.readFileSync(local(buildLog),'utf8').includes('Finished'))throw Error('Final offline build is required');
const files=[];
function copy(source,target){const bytes=fs.readFileSync(local(source)),out=local(`${destination}/${target}`);fs.mkdirSync(path.dirname(out),{recursive:true});fs.writeFileSync(out,bytes);const entry={path:relative(out),sha256:sha(bytes),bytes:bytes.length};files.push(entry);return entry;}
const previousPath='docs/evidence/capability-upgrade/2026-10-12/batch3-publication-language-manifest.json',previous=JSON.parse(fs.readFileSync(local(previousPath),'utf8'));
const baseline=JSON.parse(fs.readFileSync(local(previous.sourceManifest.path),'utf8'));
const sources=baseline.sources.map(item=>{
 const current=sha(fs.readFileSync(local(item.path)));
 if(current===item.sha256){if(sha(fs.readFileSync(local(item.archivedAt)))!==current)throw Error('Historical source hash mismatch');return item;}
 const archived=copy(item.path,'source/'+item.path);return{path:item.path,sha256:current,archivedAt:archived.path};
});
const qaNames=['ui-tests/math-matrix-semantics-native.cjs','ui-tests/math-row-limits-native.cjs','ui-tests/math-kern-native.cjs','ui-tests/html-math-native.cjs','ui-tests/mathml-epub-native.cjs','tools/test/math-source-operators-mcp.cjs','ui-tests/native-session.cjs','ui-tests/zip-parts.cjs','rust/readmd-kernel/tests/fixtures/math-kern-stix.json','tools/test/record-batch3-matrix.mjs','docs/design/v005-batch3-execution-spec.md'];
const priorQa=(baseline.qaSources||[]).map(item=>({...item,archivedAt:item.archivedAt||previous.files.find(v=>v.path.endsWith('/source/'+item.path)&&v.sha256===item.sha256)?.path})).filter(item=>item.archivedAt&&!qaNames.includes(item.path)&&fs.existsSync(local(item.path))&&sha(fs.readFileSync(local(item.path)))===item.sha256);
for(const item of priorQa)if(sha(fs.readFileSync(local(item.archivedAt)))!==item.sha256)throw Error('Historical QA source hash mismatch');
const qaSources=[...priorQa,...qaNames.map(name=>{const entry=copy(name,'source/'+name);return{path:name,sha256:entry.sha256,archivedAt:entry.path};})];
for(const {report}of reports)if(report.scriptSha256&&!qaSources.some(s=>s.sha256===report.scriptSha256))throw Error('Native QA script does not match captured sources');
const sourceManifest=copyBytes({recordedAt:new Date().toISOString(),sources,qaSources},'source-manifest.json');
function copyBytes(value,target){const out=local(`${destination}/${target}`),bytes=Buffer.from(JSON.stringify(value,null,2)+'\n');fs.mkdirSync(path.dirname(out),{recursive:true});fs.writeFileSync(out,bytes);const entry={path:relative(out),sha256:sha(bytes),bytes:bytes.length};files.push(entry);return entry;}
for(const name of fs.readdirSync(local('.cache/capability-upgrade')).filter(n=>/^batch3-matrix-.*\.log$/.test(n)))copy('.cache/capability-upgrade/'+name,'verification/'+name);
for(const [index,{file}]of reports.entries()){
 const directory=path.dirname(file),prefix=`native/${index}-${path.basename(directory)}`;
 for(const name of fs.readdirSync(directory)){
  if(name==='settings.json'||!(/\.(?:json|xml|xhtml|png|bmp|log)$/.test(name)))continue;
  const source=path.join(directory,name);if(fs.statSync(source).isFile())copy(relative(source),`${prefix}/${name}`);
 }
 const documents=path.join(directory,'documents');if(fs.existsSync(documents))for(const name of fs.readdirSync(documents)){const source=path.join(documents,name);if(fs.statSync(source).isFile()&&/\.(md|pdf|epub|docx|html)$/.test(name))copy(relative(source),`${prefix}/documents/${name}`);}
}
// Keep diagnostic reports from failed owned runs; they are never accepted as
// final native evidence. No recursive browser/profile copying.
for(const name of fs.readdirSync(local('.cache/capability-upgrade')).filter(n=>n.startsWith('math-matrix-semantics-native-'))){
 const reportFile=local(`.cache/capability-upgrade/${name}/report.json`);if(!fs.existsSync(reportFile))continue;
 const report=JSON.parse(fs.readFileSync(reportFile,'utf8'));if(report.passed)continue;
 copy(relative(reportFile),`diagnostics/${name}/report.json`);
}
const reportEntry=({file,report})=>({report:files.find(v=>v.path.includes(path.basename(path.dirname(file)))&&v.path.endsWith('/report.json')).path,checks:report.checks.length});
const gates=JSON.parse(fs.readFileSync(local('.cache/capability-upgrade/batch3-matrix-gates.log'),'utf8'));
if(gates.length!==8||gates.some(v=>v.exitCode!==0))throw Error('Repository gate failure');
const manifest={schema:1,recordedAt:new Date().toISOString(),status:'passed-limited-windows-matrix-semantics-increment',scope:'M03 physical cSp minimum, independent column gap, plcHide empty-cell ink/identity; bounded generated MathML reimport, native property and empty-slot edits, undo, save, restart and actual exports. Full Office/reader/B3 certification remains pending.',previous:{path:previousPath,sha256:sha(fs.readFileSync(local(previousPath)))},binary:{path:binary,sha256:binaryHash,bytes:fs.statSync(local(binary)).size},bootSha256:bootHash,sourceManifest,library:{passed:2372,failed:0,ignored:3,newTests:7},native:reports.filter(v=>v.report.transport!=='stdio').map(reportEntry),mcp:reports.filter(v=>v.report.transport==='stdio').map(reportEntry),repositoryGates:{passed:true,count:gates.length},visualInspection:{passed:true,scope:'Manually viewed current-build 1160x820 light and 1024x680 dark native screenshots and actual PDFium page bitmap. Formula ink, empty-slot strokes, column separation and content remain legible/contained. QA screenshots are not publicity recordings.'},compatibilityGaps:['Empty cSp vs fixed schema required val remains M15 pending adjudication.','Nonuniform MathML cell minima or mixed placeholder visibility report projection.','Independent Word application and EPUB reader remain unverified.'],actualWordApplication:'unverified',independentEpubReader:'unverified',fullOmmlCertification:false,fullRequirementCertification:false,releaseAccepted:false,files};
fs.writeFileSync(local(manifestPath),JSON.stringify(manifest,null,2)+'\n');
console.log(JSON.stringify({manifest:manifestPath,sha256:sha(fs.readFileSync(local(manifestPath))),files:files.length,bytes:files.reduce((n,v)=>n+v.bytes,0),nativeChecks:reports.reduce((n,v)=>n+v.report.checks.length,0)}));
