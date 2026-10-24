'use strict';
// Real executable/stdio only. Synthetic documents; no provider, engine, HTTP,
// process or protocol substitutes. Every invocation keeps a fresh evidence tree.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const zlib = require('node:zlib');
const {spawn, spawnSync} = require('node:child_process');
const {StringDecoder} = require('node:string_decoder');
const {SECTION_OPTIONS, declarations, directive} = require('./publication-structure-fixtures.cjs');

const root = path.resolve(__dirname, '../..');
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
const fileHash = file => sha(fs.readFileSync(file));
const delay = ms => new Promise(resolve => { const timer=setTimeout(resolve, ms);timer.unref(); });
const flatten = node => [node, ...node.content.filter(part => part.type === 'element').flatMap(part => flatten(part.value))];
const local = node => node.name.split(':').at(-1);
const text = node => node.content.map(part => part.type === 'text' ? part.value : part.type === 'element' ? text(part.value) : '').join('');
const runDsl = 'office.r(office.rPr(office.sty(val: "p")), word.rPr(word.color(val: "336699"), word.sz(val: "24")), office.t("𝑎 +b"))';
const formula = '// author 😀 — preserve repeated nested fractions and exact trivia\r\nrow( '+Array(3).fill('frac( '+runDsl+' , root(3, β) )').join(',  ')+' ) // end author\r\n';
const fence = value => '```readmd-math v1\n'+value.replaceAll('\r\n', '\n')+'```\n\n';
const repeatedParagraph = 'MCPDUPLICATE Identical paragraph for exact local formatting.';
const markers = ['MCPFRONT', 'MCPMAIN', 'MCPFULL', 'MCPRESUME', 'MCPWIDE', 'MCPODD', 'MCPEVEN'];
const bodies = [
  '# Synthetic front\n\nMCPFRONT Roman preface; [go to main](#readmd-section-main).\n\n😀 Prefix proves UTF-16 offsets.\n\n'+fence(formula)+fence(formula)+fence(formula),
  '# Synthetic main\n\nMCPMAIN Two-column text and a retained note.[^qa]\n\n'+Array.from({length:45}, (_,i) => 'MCPROW'+String(i).padStart(2, '0')+' Local publication proof.\n\n').join(''),
  'MCPFULL Full width continuous text.\n\n'+repeatedParagraph+'\n\n'+repeatedParagraph+'\n\n',
  'MCPRESUME Two-column continuous text.\n\n> A quoted paragraph retains its structure.\n\n- A first list item\n- A second list item\n\n',
  'MCPWIDE Landscape table.\n\n| Wide A | Wide B | Wide C | Wide D |\n|---|---|---|---|\n| Alpha proof | Beta proof | Gamma proof | Delta proof |\n\n',
  '# Synthetic odd appendix\n\nMCPODD Physical odd page, displayed letter A.\n\n',
  '# Synthetic even appendix\n\nMCPEVEN Physical even page, displayed number 7; explicit blank header.\n\n[^qa]: MCPNOTE A retained synthetic footnote.\n',
];
const original = '\uFEFF'+('---\ntitle: Synthetic MCP closure\nlanguage: en-US\nunknown-extension: "Keep: exact, bytes" # protected\n---\n\n'+declarations.map((section,i) => directive(section)+bodies[i]).join('')).replaceAll('\r\n', '\n').replaceAll('\n', '\r\n');
const exactFormula = formula.replaceAll('\r\n', '\n').replaceAll('\n', '\r\n');
const options = {...structuredClone(SECTION_OPTIONS), page:{...SECTION_OPTIONS.page, mirrorMargins:true}, typography:{...SECTION_OPTIONS.typography, widowControl:true, keepLines:false}, footnotes:{size:8,lineHeight:1.2,numberFormat:'lowerLetter'}};

if (process.argv.includes('--plan')) {
  console.log(JSON.stringify({transport:'real-stdio', synthetic:true, sections:declarations.length,
    inputSha256:sha(original), inputBytes:Buffer.byteLength(original), formulaSha256:sha(exactFormula),
    requiredEnvironment:['READMD_BIN','READMD_CANDIDATE_SHA256','READMD_BOOT_SHA256','READMD_MCP_CANDIDATE','READMD_TEST_PYTHON'],
    optionalEnvironment:['READMD_MCP_TEST_ROOT','READMD_MCP_ENABLE_XELATEX','READMD_ENGINE_REUSE_PROFILE'],
    noProcessesStarted:true, fullI10Certified:false}, null, 2));
  return;
}

const base = path.resolve(process.env.READMD_MCP_TEST_ROOT || path.join(root, '.cache/capability-upgrade/longtail-step12-20261018/mcp'));
assert(path.isAbsolute(base) && path.relative(root, base).startsWith('.cache'+path.sep), 'MCP evidence must stay under the workspace .cache directory');
fs.mkdirSync(base, {recursive:true});
const evidence = fs.mkdtempSync(path.join(base, 'stdio-'));
for (const name of ['profile','documents','outputs','wire','inspection']) fs.mkdirSync(path.join(evidence, name));
let profile = path.join(evidence, 'profile');
const docs = path.join(evidence, 'documents'), outputs = path.join(evidence, 'outputs');
let assets = path.resolve(process.env.READMD_ASSETS_DIR || path.join(root, 'assets'));
const binary = process.env.READMD_BIN, python = process.env.READMD_TEST_PYTHON;
const inspector = path.join(root, 'tools/test/inspect-mcp-publication-closure.py');
const report = {passed:false, fullI10Certified:false, scope:'bounded-I10-publication-math-stdio', synthetic:true,
  transport:'real-stdio', automaticRetries:0, startedAt:new Date().toISOString(), checks:[], outputs:[], sessions:[],
  limitations:['This bounded real stdio chain does not certify all publication parameters or arbitrary mathematical structures.',
    'DOCX package inspection and application readback do not certify Word/WPS/LibreOffice rendering or editable application round trips.',
    'VS Code Extension Host, independent EPUB readers, eight-hour stress and non-Windows devices are separate acceptance work.',
    'Cancellation covers a native DOCX export before publication; TeX companion-directory cancellation and all managed-worker fault matrices are separate.']};
let stage = 'candidate binding', client, manifest, freeze, binding, reuseEvidence;
let baselineTasks = new Set();
const write = (name, value) => fs.writeFileSync(path.join(evidence, name), typeof value === 'string' || Buffer.isBuffer(value) ? value : JSON.stringify(value, null, 2)+'\n', {flag:'wx'});
const retain = (name, value) => { write(name, value); return {path:name.replaceAll(path.sep,'/'),sha256:fileHash(path.join(evidence,name)),bytes:fs.statSync(path.join(evidence,name)).size}; };
function check(name, value={}) { report.checks.push({name, ...value}); }
function safeInsideWorkspace(file) {
  const relative = path.relative(root, path.resolve(file));
  assert(relative && !relative.startsWith('..') && !path.isAbsolute(relative), 'Source manifest path escaped workspace');
  return path.resolve(file);
}
function verifyBinding(final=false) {
  assert.equal(fileHash(binary), binding.binarySha256, 'Candidate EXE changed');
  assert.equal(fileHash(path.join(assets,'readmd.boot.js')), binding.bootSha256, 'Candidate boot changed');
  assert.equal(fileHash(binding.manifest), binding.manifestSha256, 'Candidate manifest changed');
  assert.equal(fileHash(binding.sourceFreeze), manifest.sourceFreezeSha256, 'Source freeze manifest changed');
  for (const source of freeze.sources) {
    const current = safeInsideWorkspace(path.join(root,source.path));
    assert.equal(fileHash(current), source.sha256, 'Current product source differs from candidate: '+source.path);
    if (!final && source.copy) assert.equal(fileHash(safeInsideWorkspace(path.join(root,source.copy))),source.sha256,'Frozen source bytes differ: '+source.path);
  }
  for (const [name,digest] of Object.entries(binding.qaSha256)) assert.equal(fileHash(path.join(root,name)),digest,'QA source changed during run: '+name);
}
function toolValue(message, expectedError) {
  assert(!message.error, 'Unexpected JSON-RPC error: '+JSON.stringify(message.error));
  const result = message.result;
  if (expectedError) {
    assert.equal(result.isError, true, 'Expected tool failure '+expectedError);
    assert.equal(result.structuredContent?.error_code, expectedError, JSON.stringify(result.structuredContent));
    return result.structuredContent;
  }
  assert.notEqual(result.isError, true, JSON.stringify(result.structuredContent));
  return result.structuredContent ?? result.content.find(item => item.type === 'text')?.text;
}
class StdioClient {
  constructor(number) {
    this.number=number; this.next=0; this.pending=new Map(); this.responses=[]; this.cancelled=new Set(); this.retired=new Set(); this.unexpected=[]; this.failure=null; this.buffer=''; this.decoder=new StringDecoder('utf8'); this.stderrDecoder=new StringDecoder('utf8'); this.stderr='';
    this.out=fs.openSync(path.join(evidence,'wire',`session-${number}.stdout.jsonl`),'wx');
    this.input=fs.openSync(path.join(evidence,'wire',`session-${number}.stdin.jsonl`),'wx');
    this.events=fs.openSync(path.join(evidence,'wire',`session-${number}.events.jsonl`),'wx');
    const env={...process.env,READMD_DATA_DIR:profile,READMD_WORKSPACE:docs};
    delete env.READMD_PRINT_TOKEN;
    if (process.platform==='win32') env.PATH=process.env.SystemRoot+'\\System32;'+process.env.SystemRoot;
    this.child=spawn(binary,['--mcp','--data-dir',profile,'--workspace',docs,'--assets',assets],{windowsHide:true,env,stdio:['pipe','pipe','pipe']});
    this.info={number,pid:this.child.pid,command:['--mcp','--data-dir','<owned-profile>','--workspace','<owned-documents>','--assets','<candidate-assets>'],pendingAtStop:null,forcedStop:false,system32OnlyPath:process.platform==='win32'};
    report.sessions.push(this.info);
    this.closed=new Promise(resolve=>this.child.once('close',resolve));
    this.exited=new Promise(resolve => this.child.once('exit',(code,signal)=>{this.info.exitCode=code;this.info.signal=signal;this.info.exitedAt=new Date().toISOString();if(this.pending.size)this.fail(Error('MCP exited with pending requests'));resolve();}));
    this.child.once('error',error=>this.fail(error));
    this.child.stdin.on('error',error=>this.fail(error));
    this.child.stdout.on('data',chunk=>{fs.writeSync(this.out,chunk);this.buffer+=this.decoder.write(chunk);if(Buffer.byteLength(this.buffer)>32*1024*1024)return this.fail(Error('MCP output budget exceeded'));for(;;){const at=this.buffer.indexOf('\n');if(at<0)break;const line=this.buffer.slice(0,at);this.buffer=this.buffer.slice(at+1);if(!line.trim())continue;let value;try{value=JSON.parse(line);}catch{return this.fail(Error('Non-JSON MCP stdout'));}this.responses.push(value);this.event('receive',value);if(!Object.hasOwn(value,'id')){if(value.method!=='notifications/progress')this.unexpected.push(value);continue;}const pending=this.pending.get(value.id);if(!pending){this.unexpected.push(value);continue;}this.pending.delete(value.id);clearTimeout(pending.timer);pending.resolve(value);}});
    this.child.stderr.on('data',chunk=>{this.stderr+=this.stderrDecoder.write(chunk);if(Buffer.byteLength(this.stderr)>4*1024*1024)this.fail(Error('MCP stderr budget exceeded'));});
  }
  event(direction,value) { fs.writeSync(this.events,JSON.stringify({at:new Date().toISOString(),direction,value})+'\n'); }
  fail(error) { this.failure??=error;for(const value of this.pending.values()){clearTimeout(value.timer);value.reject(error);}this.pending.clear(); }
  send(value) { if(this.failure)throw this.failure;const line=JSON.stringify(value)+'\n';fs.writeSync(this.input,line);this.event('send',value);this.child.stdin.write(line); }
  request(method,params={}, {id=`s${this.number}-${++this.next}`,timeout=120000}={}) {
    assert(!this.pending.has(id),'Client reused an in-flight ID');
    assert(!this.retired.has(id),'Client reused a completed ID');
    const result=new Promise((resolve,reject)=>{const timer=setTimeout(()=>{this.pending.delete(id);reject(Error('MCP total response deadline: '+method));},timeout);this.pending.set(id,{resolve,reject,timer});});
    this.send({jsonrpc:'2.0',id,method,params});
    return result.then(value=>{assert.equal(value.jsonrpc,'2.0');assert.equal(value.id,id);this.retired.add(id);return value;});
  }
  tool(name,args,control) { return this.request('tools/call',{name,arguments:args},control); }
  cancel(id) { const pending=this.pending.get(id);assert(pending,'Cannot cancel an unknown request');clearTimeout(pending.timer);this.pending.delete(id);this.cancelled.add(id);this.send({jsonrpc:'2.0',method:'notifications/cancelled',params:{requestId:id,reason:'Synthetic export acceptance cancellation'}});const error=Error('Client cancelled its waiter; the server must not send a result');error.name='AbortError';pending.reject(error); }
  async initialize() {
    const discovery=await this.request('server/discover',{_meta:{'io.modelcontextprotocol/protocolVersion':'2026-07-28','io.modelcontextprotocol/clientCapabilities':{}}});
    assert.equal(discovery.result.resultType,'complete');
    const hello=await this.request('initialize',{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'readmd-publication-math-closure',version:'1'}});
    assert.equal(hello.result.protocolVersion,'2025-11-25');
    this.send({jsonrpc:'2.0',method:'notifications/initialized'});
    const list=await this.request('tools/list');
    for(const name of ['readmd_publication_metadata','readmd_publication_blocks','readmd_math','readmd_content_transaction','readmd_export_document','readmd_read_document','readmd_edit_document','readmd_convert_to_markdown'])assert(list.result.tools.some(item=>item.name===name),'Missing real tool '+name);
    this.info.tools=list.result.tools.length;
  }
  async stop() {
    if(this.stopPromise)return this.stopPromise;
    this.stopPromise=this.finishStop();return this.stopPromise;
  }
  async finishStop() {
    this.info.pendingAtStop=this.pending.size;
    this.child.stdin.end();
    await Promise.race([this.closed,delay(15000)]);
    if(this.child.exitCode===null&&this.child.signalCode===null){this.info.forcedStop=true;this.child.kill();await Promise.race([this.closed,delay(5000)]);}
    this.stderr+=this.stderrDecoder.end();this.buffer+=this.decoder.end();
    for(const fd of [this.out,this.input,this.events])fs.closeSync(fd);
    // --mcp emits no app token. Fail closed if a future startup begins to do so:
    // never persist the credential-bearing stderr line in the evidence tree.
    const secret=/\b(?:authorization\s*:|bearer\s+[A-Za-z0-9._-]+|(?:api[_-]?key|app[_-]?token)\s*[=:]\s*\S+)/iu;
    const clean=this.stderr.split(/(?<=\n)/u).map(line=>secret.test(line)?'[credential-bearing stderr line omitted]\n':line).join('');
    retain(path.join('wire',`session-${this.number}.stderr.log`),clean);
    this.info.stderrRedacted=clean!==this.stderr;
    this.info.stdout=path.join('wire',`session-${this.number}.stdout.jsonl`).replaceAll(path.sep,'/');
    this.info.stderr=path.join('wire',`session-${this.number}.stderr.log`).replaceAll(path.sep,'/');
    this.info.responses=this.responses.length;this.info.unexpectedResponses=this.unexpected.length;
    this.info.stopped=this.child.exitCode!==null||this.child.signalCode!==null;
    assert(this.info.stopped,'Owned MCP process did not stop');
    assert(!this.info.forcedStop,'MCP needed forced termination');
    assert.equal(this.info.exitCode,0,'MCP did not exit cleanly');
    assert.equal(this.info.stderrRedacted,false,'Unexpected credential-like data on MCP stderr');
    assert.equal(this.buffer.trim(),'','Truncated MCP response');
    assert.deepEqual(this.unexpected,[],'Uncorrelated, duplicate or cancelled late MCP result');
    if(this.failure)throw this.failure;
  }
}
async function until(predicate, label, ms=120000) {
  const deadline=Date.now()+ms;
  for(;;){if(client?.failure)throw client.failure;const result=await predicate();if(result)return result;assert(Date.now()<deadline,'Total deadline: '+label);await delay(20);}
}
function taskRecords() {
  const directory=path.join(profile,'worker-tasks-v1');
  if(!fs.existsSync(directory))return [];
  const found=[];
  for(const entry of fs.readdirSync(directory,{withFileTypes:true}))if(entry.isDirectory()){
    const file=path.join(directory,entry.name,'record.jsonl');if(!fs.existsSync(file))continue;
    let lines;try{lines=fs.readFileSync(file,'utf8').trim().split('\n').filter(Boolean).map(line=>JSON.parse(line));}catch{continue;}
    const header=lines.find(value=>value.type==='Header')?.value;
    if(header)found.push({id:entry.name,purpose:header.purpose,root:header.root,record:file,lines});
  }
  return found;
}
function inspect(file,label) {
  const result=spawnSync(python,[inspector,file],{windowsHide:true,timeout:90000,maxBuffer:32*1024*1024});
  const stdout=retain(path.join('inspection',label+'.stdout.json'),result.stdout||Buffer.alloc(0));
  const stderr=retain(path.join('inspection',label+'.stderr.log'),result.stderr||Buffer.alloc(0));
  (report.independentCommands??=[]).push({label,status:result.status,signal:result.signal,error:result.error?.code??null,stdout,stderr});
  assert.equal(result.status,0,'Independent actual-output inspector failed: '+label);
  const value=JSON.parse(result.stdout.toString('utf8'));assert.equal(value.sha256,fileHash(file));return value;
}
function recordOutput(file,extra={}) { const item={path:path.relative(evidence,file).replaceAll(path.sep,'/'),sha256:fileHash(file),bytes:fs.statSync(file).size,...extra};report.outputs.push(item);return item; }
function compareImportedReadback(before,after) {
  const carrierPattern=/<!--readmd:math:v1:([A-Za-z0-9+/=]+)-->/g;
  const decode=value=>{
    const found=[...value.matchAll(carrierPattern)];
    assert.equal(found.length,3,'All three rich formula carriers must remain present');
    assert.equal([...value.matchAll(/<!--readmd:math:/g)].length,found.length,'Malformed or unexpected carrier');
    const payloads=found.map(match=>{
      const compressed=Buffer.from(match[1],'base64');assert.equal(compressed.toString('base64'),match[1]);
      const payload=JSON.parse(zlib.gunzipSync(compressed,{maxOutputLength:4*1024*1024}).toString('utf8'));
      assert.deepEqual(Object.keys(payload).sort(),['id','omml','schema','source_revision']);
      assert.equal(payload.schema,1);assert.equal(typeof payload.omml,'string');assert(payload.omml.length>0);
      assert.match(payload.id,/^eq-[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i);
      const prefix=value.slice(0,match.index);assert(prefix.endsWith('$$'),'Carrier must immediately follow its displayed formula');
      const opening=prefix.lastIndexOf('$$',prefix.length-3);assert(opening>=0);
      assert.equal(payload.source_revision,sha(prefix.slice(opening+2,-2)),'Carrier digest must match its exact visible formula');
      return payload;
    });
    assert.equal(new Set(payloads.map(payload=>payload.id)).size,payloads.length,'Each independently imported formula needs its own valid identity');
    return {visible:value.replace(carrierPattern,''),payloads};
  };
  const a=decode(before),b=decode(after);assert.equal(a.visible,b.visible,'All visible Markdown must remain byte-identical');
  const formulas=a.payloads.map((value,index)=>{
    const other=b.payloads[index];
    assert.deepEqual({schema:value.schema,source_revision:value.source_revision,omml:value.omml},
      {schema:other.schema,source_revision:other.source_revision,omml:other.omml},'Only a newly generated identity may differ; the complete lossless XML payload must be exact');
    assert(!a.payloads.some(before=>before.id===other.id),'A separate import allocates fresh formula identities');
    return {index,beforeId:value.id,afterId:other.id,sourceRevision:value.source_revision,
      ommlSha256:sha(value.omml),ommlBytes:Buffer.byteLength(value.omml),entireOmmlByteEqual:true};
  });
  return {allowedGeneratedDifference:['$.id'],strictEquality:['original DOCX bytes','saved source bytes','visible Markdown','schema','source_revision','complete OMML including unknown XML'],formulas,
    identityContract:{path:'rust/readmd-kernel/src/convert.rs',line:2420,sha256:fileHash(path.join(root,'rust/readmd-kernel/src/convert.rs')),
      behavior:'docx_math_metadata assigns format!("eq-{}", uuid::Uuid::new_v4()) on each independent import'},
    earlierCheckerFailure:{path:'.cache/capability-upgrade/longtail-step12-20261018/mcp/stdio-Durl0e/report.json',sha256:'cdf3db579fe686f70c7c5ff188ef83fabfdd453d949ecbc257aa3a2abad60c7b',
      diagnosis:'.cache/capability-upgrade/longtail-step12-20261018/mcp/stdio-Durl0e/restart-comparison-proof.json'}};
}
function assertDocx(value,changed=false) {
  assert.equal(value.kind,'docx');assert.equal(value.sections.length,7);
  const columns=[1,2,1,2,1,1,1],formats=['lowerRoman','decimal','decimal','decimal','decimal','upperLetter','decimal'];
  for(const [i,section] of value.sections.entries()) {
    assert(section.text.includes(markers[i]),'Section lost its original owned body');
    for(const [j,marker] of markers.entries())if(i!==j)assert(!section.text.includes(marker),'Section owns neighbouring body');
    assert.equal(section.type.val,declarations[i].break);assert.equal(Number(section.columns.num||1),columns[i]);
    assert.equal(section.numbering.fmt,formats[i]);assert.equal(section.parent,i===6?'body':'pPr');
  }
  assert(Number(value.sections[4].page.w)>Number(value.sections[4].page.h),'Landscape section dimensions');
  assert.equal(value.sections[0].numbering.start,'1');assert.equal(value.sections[1].numbering.start,'1');assert.equal(value.sections[5].numbering.start,'1');assert.equal(value.sections[6].numbering.start,'7');
  for(const i of [2,3,4])assert.equal(value.sections[i].numbering.start,undefined,'Continuous numbering must not restart');
  assert(value.sections[0].headers.some(part=>part.text==='FRONTHEADER'));
  assert(value.sections[1].headers.some(part=>part.text==='MAINHEADER'));
  assert.equal(value.sections[2].headers.length,0);assert.equal(value.sections[3].headers.length,0);assert.equal(value.sections[4].headers.length,0);
  assert(value.sections[6].headers.some(part=>part.text===''),'Explicit blank header missing');
  assert(value.tables.some(table=>table.includes('Alpha proof')&&table.includes('Delta proof')));
  assert(value.footnotes.includes('MCPNOTE'));assert(value.settings.includes('mirrorMargins'));
  assert.equal(value.math.length,3);assert.equal(value.fractions,9);assert.equal(value.radicals,9);
  assert.equal(value.math.filter(value=>value.includes('=')).length,changed?1:0);
  if(changed)assert.equal(value.math[1].split('=').length-1,1);
  const duplicates=value.paragraphs.filter(paragraph=>paragraph.text===repeatedParagraph);assert.equal(duplicates.length,2);
  if(changed){assert(duplicates[1].runs.some(run=>run.size.val==='30'&&run.color.val==='663399'));assert(!duplicates[0].runs.some(run=>run.size.val==='30'));}
}

(async()=>{
  try {
    assert(binary&&path.isAbsolute(binary),'READMD_BIN must be an explicit absolute candidate');
    assert(python&&path.isAbsolute(python),'READMD_TEST_PYTHON must identify the independent Python runtime');
    for(const key of ['READMD_CANDIDATE_SHA256','READMD_BOOT_SHA256'])assert.match(process.env[key]||'',/^[a-f0-9]{64}$/iu,key+' required');
    assert(process.env.READMD_MCP_CANDIDATE,'READMD_MCP_CANDIDATE must point at the final source-bound candidate manifest');
    const manifestPath=path.resolve(process.env.READMD_MCP_CANDIDATE);manifest=JSON.parse(fs.readFileSync(manifestPath));
    assert.equal(manifest.binarySha256,process.env.READMD_CANDIDATE_SHA256.toLowerCase());assert.equal(manifest.bootSha256,process.env.READMD_BOOT_SHA256.toLowerCase());
    assert(manifest.sourceFreeze&&manifest.sourceFreezeSha256,'Candidate manifest needs a source freeze');
    const freezePath=safeInsideWorkspace(path.join(root,manifest.sourceFreeze));freeze=JSON.parse(fs.readFileSync(freezePath));assert(freeze.sources?.length>0,'Empty candidate source freeze');
    binding={binarySha256:manifest.binarySha256,bootSha256:manifest.bootSha256,manifest:manifestPath,manifestSha256:fileHash(manifestPath),sourceFreeze:freezePath,sourceCount:freeze.sources.length,
      pythonSha256:fileHash(python),qaSha256:Object.fromEntries(['tools/test/mcp-publication-math-closure.cjs','tools/test/inspect-mcp-publication-closure.py','tools/test/publication-structure-fixtures.cjs','tools/test/publication-batch3-fixtures.cjs','ui-tests/publication-engines-native.cjs','tools/test/publication-runtime-reuse.cjs'].map(name=>[name,fileHash(path.join(root,name))]))};
    verifyBinding();report.binding=binding;retain('candidate.json',manifest);retain('source-freeze.json',freeze);
    stage='synthetic inputs';
    const input=path.join(docs,'Synthetic.md');retain(path.join('documents','Synthetic.md'),original);
    report.inputs={original:retain('input-original.md',original)};
    retain('materials.json',{sourceSha256:sha(original),formula:exactFormula,formulaSha256:sha(exactFormula),declarations,options,markers});
    if(process.env.READMD_ENGINE_REUSE_PROFILE){
      stage='reviewed immutable runtime reuse';
      const {resolveEngineReuseProfile,prepareEngineProfileReuse}=require('../../ui-tests/publication-engines-native.cjs');
      const reviewed=resolveEngineReuseProfile(process.env.READMD_ENGINE_REUSE_PROFILE);
      for(const [variable,name] of [['READMD_FONTS_PACK','readmd-fonts-2026.10.10-universal.readmd-pack'],['READMD_PDFIUM_PACK','readmd-pdfium-7881-windows-x64.readmd-pack'],['READMD_TYPST_PACK','readmd-typst-0.15.1-windows-x64.readmd-pack']])process.env[variable]??=path.join(root,'.cache/capability-upgrade/locked-packs',name);
      let commandNumber=0;
      const harness={root,data:evidence,report,command(executable,args){
        const result=spawnSync(executable,args,{windowsHide:true,timeout:60000,maxBuffer:16*1024*1024});
        const name='runtime-reuse-preflight-'+String(++commandNumber).padStart(2,'0');
        retain(name+'.stdout.log',result.stdout||Buffer.alloc(0));retain(name+'.stderr.log',result.stderr||Buffer.alloc(0));
        assert.equal(result.status,0,'Reviewed runtime reuse preflight failed');return result.stdout.toString('utf8');
      }};
      reuseEvidence=prepareEngineProfileReuse(harness,reviewed,assets);profile=reviewed;assets=reuseEvidence.assets;
      report.runtimeReuseCurrentReadiness='Actual export below must still pass; old readiness is only provenance.';
      check('Reviewed synthetic managed runtimes reused without new engine copies; prior files are preserved and no cold-performance claim is made.');
    }
    baselineTasks=new Set(taskRecords().map(task=>task.id));report.baselineTaskJournalCount=baselineTasks.size;
    client=new StdioClient(1);await client.initialize();check('Modern discovery and legacy initialization use the final real stdio process.');
    stage='publication metadata and UTF-16 layout';
    const read=toolValue(await client.tool('readmd_read_document',{file_path:input}));assert.equal(read.content,original.slice(1));assert.equal(read.encoding,'utf-8-sig');assert.equal(read.eol,'crlf');assert.equal(read.revision,sha(original));
    const meta=toolValue(await client.tool('readmd_publication_metadata',{content:original}));assert.equal(meta.metadata.title,'Synthetic MCP closure');
    const listed=toolValue(await client.tool('readmd_publication_blocks',{content:original}));assert.equal(listed.document.revision,sha(original));
    const duplicates=listed.document.nodes.filter(node=>node.kind==='paragraph'&&original.slice(node.from,node.to).trim()===repeatedParagraph);assert.equal(duplicates.length,2);
    const localPatch={text:{size:15,color:'#663399'},paragraph:{keepLines:true,before:4,after:5}};
    const styled=toolValue(await client.tool('readmd_publication_blocks',{action:'apply',content:original,base_revision:listed.document.revision,id:duplicates[1].id,patch:localPatch}));assert.equal(styled.result.changed,true);
    assert.equal(styled.transaction.edits.length,1);assert(styled.transaction.edits[0].from>duplicates[0].to);assert.equal(styled.transaction.edits[0].from,duplicates[1].from);
    const idempotent=toolValue(await client.tool('readmd_publication_blocks',{action:'apply',content:styled.result.content,base_revision:styled.result.revision,id:duplicates[1].id,patch:localPatch}));assert.equal(idempotent.result.changed,false);assert.equal(idempotent.result.content,styled.result.content);
    const reversed=toolValue(await client.tool('readmd_content_transaction',{content:styled.result.content,transaction:{base_revision:styled.result.revision,origin:'editor',edits:styled.result.inverse}}));assert.equal(reversed.result.content,original);
    toolValue(await client.tool('readmd_publication_blocks',{action:'apply',content:styled.result.content,base_revision:listed.document.revision,id:duplicates[1].id,patch:localPatch}),'transaction_conflict');
    check('Exact UTF-16 layout edits target only the second duplicate, preserve BOM/CRLF, reverse exactly, remain idempotent and reject stale revisions.');
    stage='precise nested mathematical character edit';
    const parses=await Promise.all([client.tool('readmd_math',{operation:'parse',source:exactFormula,dialect:'dsl',display:true}),client.request('ping'),client.tool('readmd_math',{operation:'parse',source:'root(3, β)',dialect:'dsl',display:true})]);
    const parsed=toolValue(parses[0]);assert.equal(parsed.document.revision,sha(exactFormula));assert.deepEqual(parses[1].result,{});assert.equal(toolValue(parses[2]).document.source,'root(3, β)');
    const texts=flatten(parsed.document.root).filter(node=>local(node)==='t'&&text(node)==='𝑎 +b');assert.equal(texts.length,3);
    const mathPatch={operation:'replace_text_range',node_id:texts[1].id,from:3,to:4,text:'='};
    const changed=toolValue(await client.tool('readmd_math',{operation:'patch',source:exactFormula,dialect:'dsl',display:true,expected_revision:parsed.document.revision,patch:mathPatch}));
    const at=exactFormula.indexOf('𝑎 +b',exactFormula.indexOf('𝑎 +b')+1)+3;
    assert.equal(changed.document.source,exactFormula.slice(0,at)+'='+exactFormula.slice(at+1));
    assert.deepEqual(flatten(changed.document.root).filter(node=>local(node)==='t'&&['𝑎 +b','𝑎 =b'].includes(text(node))).map(text),['𝑎 +b','𝑎 =b','𝑎 +b']);
    toolValue(await client.tool('readmd_math',{operation:'patch',source:changed.document.source,dialect:'dsl',display:true,expected_revision:parsed.document.revision,patch:mathPatch}),'transaction_conflict');
    toolValue(await client.tool('readmd_math',{operation:'patch',source:exactFormula,dialect:'dsl',display:true,expected_revision:parsed.document.revision,patch:{...mathPatch,from:1,to:2}}),'transaction_invalid_range');
    const repeated=toolValue(await client.tool('readmd_math',{operation:'patch',source:changed.document.source,dialect:'dsl',display:true,expected_revision:changed.document.revision,patch:mathPatch}));assert.equal(repeated.document.source,changed.document.source);assert.equal(repeated.document.revision,changed.document.revision);
    const bodyFrom=styled.result.content.indexOf(exactFormula,styled.result.content.indexOf(exactFormula)+1);assert(bodyFrom>=0);
    const transaction={base_revision:styled.result.revision,origin:'formula',edits:[{from:bodyFrom,to:bodyFrom+exactFormula.length,insert:changed.document.source}]};
    const integrated=toolValue(await client.tool('readmd_content_transaction',{content:styled.result.content,transaction}));
    assert.equal(integrated.result.content,styled.result.content.slice(0,bodyFrom)+changed.document.source+styled.result.content.slice(bodyFrom+exactFormula.length));
    const competing=toolValue(await client.tool('readmd_publication_metadata',{action:'patch',content:styled.result.content,base_revision:styled.result.revision,patch:{subtitle:'Intervening edit'}}));
    toolValue(await client.tool('readmd_content_transaction',{content:competing.content,transaction}),'transaction_conflict');
    const omml=toolValue(await client.tool('readmd_math',{operation:'serialize',source:changed.document.source,dialect:'dsl',display:true,target:'omml'}));
    const reopenedMath=toolValue(await client.tool('readmd_math',{operation:'parse',source:omml.output,dialect:'omml',display:true}));assert.equal(flatten(reopenedMath.document.root).filter(node=>local(node)==='f').length,3);assert.equal(flatten(reopenedMath.document.root).filter(node=>local(node)==='rad').length,3);
    check('Correlated concurrent calls and precise second-character editing preserve the other nested fractions; stale math/document transactions and surrogate-splitting edits are rejected.');
    stage='confirmed atomic source save and readback';
    const edited=integrated.result.content;assert(edited.startsWith('\uFEFF---\r\n'));assert.equal(edited.replaceAll('\r\n','').includes('\n'),false);
    const editArgs={file_path:input,expected_revision:read.revision,old_text:original.slice(1),new_text:edited.slice(1)};
    const preview=toolValue(await client.tool('readmd_edit_document',editArgs));assert.equal(preview.dry_run,true);assert.equal(fileHash(input),sha(original));
    toolValue(await client.tool('readmd_edit_document',{...editArgs,dry_run:false}),'confirmation_required');assert.equal(fileHash(input),sha(original));
    const committed=toolValue(await client.tool('readmd_edit_document',{...editArgs,dry_run:false,confirm:true}));assert.equal(fileHash(input),sha(edited));assert(committed.checkpoint_id);
    toolValue(await client.tool('readmd_edit_document',{...editArgs,dry_run:false,confirm:true}),'document_conflict');assert.equal(fileHash(input),sha(edited));
    const saved=toolValue(await client.tool('readmd_read_document',{file_path:input,end_line:1000}));assert.equal(saved.content,edited.slice(1));assert.equal(saved.encoding,'utf-8-sig');assert.equal(saved.revision,sha(edited));
    retain('saved-source.md',edited);check('Preview, explicit commit, stale retry protection and actual file readback preserve the complete rich source.');
    stage='rich DOCX export and independent readback';
    const docx=path.join(outputs,'Synthetic-publication.docx');
    const exportArgs={markdown_content:edited,output_path:docx,output_format:'docx',base_dir:docs,style_preset:'classic',options,title:'Synthetic MCP closure',confirm:true};
    toolValue(await client.tool('readmd_export_document',exportArgs));const actual=inspect(docx,'rich-docx');assertDocx(actual,true);recordOutput(docx,{sections:7,math:3});
    const converted=toolValue(await client.tool('readmd_convert_to_markdown',{file_path:docx}));assert.equal(typeof converted,'string');for(const marker of markers)assert(converted.includes(marker));assert(converted.includes('='));retain('docx-readback.md',converted);
    const protectedHash=fileHash(docx),protectedNames=fs.readdirSync(outputs).sort();toolValue(await client.tool('readmd_export_document',exportArgs),'output_exists');assert.equal(fileHash(docx),protectedHash);assert.deepEqual(fs.readdirSync(outputs).sort(),protectedNames);
    for(const [label,bad,code] of [['invalid-section',edited+'\r\n<!-- readmd-section:v1 {"id":"invalid","unknown":true} -->\r\n','publication_section_invalid'],['invalid-geometry',directive({id:'cramped',page:{size:'A5',columns:3,columnGap:30,gutter:40}})+'Body','publication_section_geometry']]){
      toolValue(await client.tool('readmd_export_document',{...exportArgs,markdown_content:bad,overwrite:true}),code);assert.equal(fileHash(docx),protectedHash);assert.deepEqual(fs.readdirSync(outputs).sort(),protectedNames);check('Rejected '+label+' preserves the previous actual DOCX.');
    }
    check('Actual seven-section DOCX has native breaks, columns, landscape dimensions, linked/blank headers, numbering, notes, table, local format and exactly one edited formula; real import reads it back.');
    stage='actual mathematical PDF output';
    const pdf=path.join(outputs,'Synthetic-math.pdf');const pdfSource='---\ntitle: Synthetic math readback\nlanguage: en-US\n---\n\n# MCP math publication\n\nMCPPDF Actual output readback.\n\n'+fence(changed.document.source);
    toolValue(await client.tool('readmd_export_document',{markdown_content:pdfSource,output_path:pdf,output_format:'pdf',base_dir:docs,options:{pdf:{engine:'rust'},cover:{enabled:false},toc:{enabled:false},frontMatter:{mode:'none'},footer:{pageNumbers:false},typography:{font:'Arial',size:12}},title:'Synthetic math',confirm:true}));
    const actualPdf=inspect(pdf,'math-pdf');assert.equal(actualPdf.kind,'pdf');assert.equal(actualPdf.pages.length,1);assert.equal(actualPdf.metadata.title,'Synthetic math readback');assert(actualPdf.pages[0].text.includes('MCPPDF'));assert(actualPdf.pages[0].drawings>0,'Actual PDF must include vector mathematics');assert.equal(actualPdf.pages[0].images,0);recordOutput(pdf,{pages:1,vectorDrawingReadback:true,metadataReadback:true});check('Actual Rust PDF reads back with declared metadata, expected text, vector math and no image fallback.');
    if(process.env.READMD_MCP_ENABLE_XELATEX==='1'){
      stage='managed complex-section PDF output';const managedPdf=path.join(outputs,'Synthetic-publication-xelatex.pdf');
      const began=Date.now();toolValue(await client.tool('readmd_export_document',{...exportArgs,output_path:managedPdf,output_format:'pdf',options:{...options,pdf:{engine:'xelatex'},tex:{docClass:'article',useCtex:false}}},{timeout:600000}));
      report.managedExport={engine:'xelatex',elapsedMs:Date.now()-began,coldPerformanceAccepted:false};
      const value=inspect(managedPdf,'rich-xelatex-pdf');const all=value.pages.map(page=>page.text).join('\n');for(const marker of markers)assert(all.includes(marker));assert(value.pages.some(page=>page.width>page.height));recordOutput(managedPdf,{pages:value.pages.length,managedEngine:'xelatex'});check('Genuine managed XeLaTeX renders all seven section bodies and the landscape page, independently read from PDF.');
    }
    stage='real export cancellation lifecycle';
    const cancelledTarget=path.join(outputs,'Cancelled-publication.docx');fs.copyFileSync(docx,cancelledTarget,fs.constants.COPYFILE_EXCL);const oldHash=fileHash(cancelledTarget);
    const heavy='# Synthetic cancellation\n\n'+Array.from({length:3000},(_,i)=>'MCPCANCEL'+String(i).padStart(4,'0')+' Genuine native export $x^2+y^2=z^2$.\n\n').join('');
    report.inputs.cancellation=retain('cancellation-input.md',heavy);const cancelId='cancel-export-real';
    const beforeCancellationTasks=new Set(taskRecords().map(task=>task.id));
    const running=client.tool('readmd_export_document',{...exportArgs,markdown_content:heavy,output_path:cancelledTarget,overwrite:true},{id:cancelId});running.catch(()=>{});
    const active=await until(()=>{assert(!client.responses.some(value=>value.id===cancelId),'Export finished before observed cancellation');return taskRecords().find(task=>task.purpose==='mcp-export'&&!beforeCancellationTasks.has(task.id));},'observe actual native export workspace');
    retain('cancellation-active-task.json',active);assert(fs.existsSync(active.root));
    const cancelledAt=Date.now();client.cancel(cancelId);await assert.rejects(running,{name:'AbortError'});
    await until(()=>!fs.existsSync(active.record)&&!fs.existsSync(active.root),'cancelled export ownership cleanup');await delay(50);
    assert.equal(fileHash(cancelledTarget),oldHash,'Cancelled export changed previous output');assert.equal(client.responses.filter(value=>value.id===cancelId).length,0,'Cancelled request returned a late result');
    await client.request('ping');
    // A fresh request may explicitly reuse an ID after cancellation has fully
    // drained. It must receive exactly its own result, never the old export.
    assert(!client.retired.has(cancelId),'A cancelled request has no completed response');const released=toolValue(await client.tool('readmd_fix_markdown',{content:'# Cancelled request ID released'},{id:cancelId}));assert.equal(released.ok,true);
    assert.equal(client.responses.filter(value=>value.id===cancelId).length,1);assert.equal(fileHash(cancelledTarget),oldHash);
    toolValue(await client.tool('readmd_export_document',{...exportArgs,output_path:cancelledTarget,overwrite:true}));assertDocx(inspect(cancelledTarget,'retry-docx'),true);recordOutput(cancelledTarget,{retryAfterCancellation:true});
    report.cancellation={requestId:cancelId,observedOwnedTask:active.id,oldOutputSha256:oldHash,oldOutputPreserved:true,lateResultSuppressed:true,requestIdReleased:true,elapsedMs:Date.now()-cancelledAt,ownedWorkspaceRemoved:true};
    check('A genuine active native export is cancelled; old output survives, late results are suppressed, owned work drains, its ID is released and an explicit retry produces a valid rich DOCX.');
    stage='clean process shutdown and actual restart';await client.stop();client=null;
    assert.equal(taskRecords().filter(task=>!baselineTasks.has(task.id)).length,0,'New owned task journals remain after clean MCP exit');
    client=new StdioClient(2);await client.initialize();const afterRestart=toolValue(await client.tool('readmd_read_document',{file_path:input,end_line:1000}));assert.equal(afterRestart.content,edited.slice(1));assert.equal(afterRestart.encoding,'utf-8-sig');assert.equal(afterRestart.revision,sha(edited));
    const afterImport=toolValue(await client.tool('readmd_convert_to_markdown',{file_path:docx}));
    retain('docx-readback-after-restart.md',afterImport);report.reimportReadback=compareImportedReadback(converted,afterImport);
    report.reimportReadback.proof=retain('docx-reimport-proof.json',report.reimportReadback);
    assert.equal(fileHash(docx),protectedHash,'The original DOCX bytes changed across restart/readback');
    const firstHash=fileHash(docx);toolValue(await client.tool('readmd_export_document',exportArgs),'output_exists');assert.equal(fileHash(docx),firstHash);assert.equal(fileHash(input),sha(edited));
    await client.stop();client=null;assert.equal(taskRecords().filter(task=>!baselineTasks.has(task.id)).length,0);assert.deepEqual(fs.readdirSync(docs),['Synthetic.md']);check('Two real MCP processes exit cleanly; restart reads exact saved source and DOCX bytes, identical visible Markdown and full math XML, validates fresh import identities and keeps overwrite protection.');
    stage='final binding';verifyBinding(true);assert.equal(fileHash(python),binding.pythonSha256);report.passed=true;
  } catch(error) {
    report.failedStage=stage;report.firstFailure=String(error.stack||error).replaceAll(root,'<workspace>');process.exitCode=1;
  } finally {
    if(client)try{await client.stop();}catch(error){report.passed=false;report.shutdownFailure=String(error.stack||error).replaceAll(root,'<workspace>');process.exitCode=1;}
    if(reuseEvidence)try{reuseEvidence.assertPreserved();}catch(error){report.passed=false;report.runtimePreservationFailure=String(error.stack||error).replaceAll(root,'<workspace>');process.exitCode=1;}
    report.remainingOwnedTasks=taskRecords().filter(task=>!baselineTasks.has(task.id)).map(task=>({id:task.id,purpose:task.purpose,root:task.root}));
    if(report.remainingOwnedTasks.length){report.passed=false;process.exitCode=1;}
    report.finishedAt=new Date().toISOString();
    report.logs=fs.readdirSync(path.join(evidence,'wire')).map(name=>({path:'wire/'+name,sha256:fileHash(path.join(evidence,'wire',name)),bytes:fs.statSync(path.join(evidence,'wire',name)).size}));
    write('report.json',report);
    console.log(JSON.stringify({passed:report.passed,fullI10Certified:false,checks:report.checks.length,outputs:report.outputs.length,failedStage:report.failedStage,report:path.relative(root,path.join(evidence,'report.json')).replaceAll(path.sep,'/'),sha256:fileHash(path.join(evidence,'report.json'))}));
  }
})();
