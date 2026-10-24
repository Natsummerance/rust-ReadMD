'use strict';
const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const {EventEmitter}=require('node:events'),{PassThrough,Writable}=require('node:stream');
const qa=require('./batch3-semantics-stdio.cjs');
const element=(name,content=[],attributes={})=>({id:name,name:'m:'+name,attributes,content:content.map(value=>typeof value==='string'?{type:'text',value}:{type:'element',value})});
const property=(name,value)=>element(name,[],{'m:val':value});
function matrix(alignments,rows=[alignments.length]){return element('m',[element('mPr',[element('mcs',alignments.map(alignment=>element('mc',[element('mcPr',[property('count','1'),property('mcJc',alignment)])])))]),...rows.map(count=>element('mr',Array.from({length:count},()=>element('e',[element('r',[element('t',['x'])])]))))]);}
function value(matrices){return {document:{diagnostics:[],root:element('oMath',matrices)},preview:'<math/>',vector_preview:'<svg/>'};}
function ownedFolder(label){fs.mkdirSync(qa.BASE,{recursive:true});return fs.mkdtempSync(path.join(qa.BASE,'selftest-'+label+'-'));}
function fakeSpawn(settings={}){
  const child=new EventEmitter();child.pid=909090;child.stdout=new PassThrough();child.stderr=new PassThrough();child.killCount=0;child.sent=[];
  child.stdin=new Writable({write(chunk,encoding,done){const message=JSON.parse(chunk.toString());child.sent.push(message);done();if(message.id!==undefined)queueMicrotask(()=>child.stdout.write(JSON.stringify({jsonrpc:'2.0',id:message.id,result:settings.respond?settings.respond(message):{reply:message.method}})+'\n'));},final(done){done();if(settings.hang)return;setImmediate(()=>{settings.beforeExit?.(child);child.emit('exit',settings.code??0,settings.signal??null);settings.afterExit?.(child);child.stdout.end();child.stderr.end();child.emit('close',settings.code??0,settings.signal??null);});}});
  child.kill=()=>{child.killCount++;setImmediate(()=>{child.emit('exit',null,'SIGTERM');child.stdout.end();child.stderr.end();child.emit('close',null,'SIGTERM');});return true;};
  return {child,spawnImpl:(binary,args,options)=>{child.invocation={binary,args,options};return child;}};
}
async function observation(settings={}){
  const out=ownedFolder('transport'),fake=fakeSpawn(settings);const client=new qa.StdioClient({binary:'readmd.exe',profile:path.join(out,'profile'),documents:path.join(out,'documents'),assets:'assets',out,spawnImpl:fake.spawnImpl,closeTimeout:settings.hang?1:1000,killTimeout:1000});
  const reply=await client.request('ping');assert.deepEqual(reply,{reply:'ping'});const report={passed:true};await client.stop(report);return {out,client,child:fake.child,report};
}

test('plan is read-only and fixes real stdio, candidate binding, positive and negative semantics',()=>{
  const options=qa.argumentsFor(['--plan'],{}),plan=qa.planFor(options);assert.equal(plan.noProcessesStarted,true);assert.equal(plan.noFilesWritten,true);assert.equal(plan.positiveTables.length,5);assert(plan.negativeTables.length>=14);assert.equal(plan.repeatedMatrices.selectedMatrix,2);assert.equal(plan.repeatedFunctions.expected,qa.UNICODE_CHANGED);assert.equal(plan.invalidFunctionError,'math_missing_argument');assert.deepEqual(plan.serializationTargets,['omml','mathml','dsl']);assert.equal(plan.exports.enabled,false);assert.equal(qa.argumentsFor(['--plan','--exports'],{}).exports,true);
});
test('execution requires explicit candidate and optional existing PDF QA runtime',()=>{
  assert.throws(()=>qa.argumentsFor([],{}),/candidate/);assert.throws(()=>qa.argumentsFor(['--candidate','candidate.json','--exports'],{}),/QA-only/);assert.throws(()=>qa.argumentsFor(['--plan','--other'],{}),/Unknown/);assert.throws(()=>qa.argumentsFor(['--plan','--candidate'],{}),/Missing/);assert.equal(qa.argumentsFor(['--help'],{}).help,true);
  assert.throws(()=>qa.argumentsFor(['--plan','--output',path.join(qa.ROOT,'outside')],{}),/escaped/);assert.throws(()=>qa.inside(qa.BASE,path.join(qa.BASE,'..','root','baseline14-H5Gneq')),/escaped/);
});
test('actual export request builder preserves PDF/DOCX tools and uses the EPUB-specific contract',()=>{
  // Relevant published schema fields from mcp.rs. Real execution also checks
  // the complete inputSchema returned by that candidate's tools/list.
  const commonProperties={markdown_content:{type:'string'},output_path:{type:'string'},base_dir:{type:'string'},title:{type:'string'},options:{type:'object'},confirm:{type:'boolean',const:true}};
  const catalog=[{name:'readmd_export_document',inputSchema:{type:'object',properties:{...commonProperties,output_format:{type:'string',enum:['pdf','docx','html','tex']},style_preset:{type:'string'},overwrite:{type:'boolean'}},required:['markdown_content','output_path','output_format','confirm'],additionalProperties:false}},{name:'readmd_export_epub',inputSchema:{type:'object',properties:{...commonProperties,author:{type:'string'},language:{type:'string'},overwrite:{type:'boolean'}},required:['markdown_content','output_path','confirm'],additionalProperties:false}}];
  const parameters={markdown_content:'# Bounded formula',output_path:'Output',base_dir:'documents',title:'M10/M11',confirm:true,options:{pdf:{engine:'rust'},cover:{enabled:false}}};
  for(const format of ['docx','pdf','epub']){
    const request=qa.exportRequestFor(format,{...parameters,output_path:'Math.'+format});qa.assertToolCallSchema(request,catalog);assert.equal(request.name,format==='epub'?'readmd_export_epub':'readmd_export_document');assert.equal(Object.hasOwn(request.arguments,'output_format'),format!=='epub');assert.equal(request.arguments.output_format,format==='epub'?undefined:format);
  }
  assert(!Object.hasOwn(parameters,'output_format'),'Builder mutated caller parameters');assert.throws(()=>qa.exportRequestFor('epub',{...parameters,output_format:'epub'}),/builder owns/);assert.throws(()=>qa.exportRequestFor('html',parameters),/bounded export format/);
  const epub=qa.exportRequestFor('epub',parameters);assert.throws(()=>qa.assertToolCallSchema({...epub,arguments:{...epub.arguments,output_format:'epub'}},catalog),/inputSchema/);assert.throws(()=>qa.assertToolCallSchema({name:'readmd_export_document',arguments:{...parameters,output_format:'epub'}},catalog),/inputSchema/);assert.throws(()=>qa.assertToolCallSchema({...epub,arguments:{...epub.arguments,confirm:false}},catalog),/inputSchema/);assert.throws(()=>qa.assertToolCallSchema({...epub,arguments:{...epub.arguments,unexpected:true}},catalog),/inputSchema/);assert.throws(()=>qa.assertToolCallSchema(epub,[]),/Missing real tool/);
});
test('export initialization requires the EPUB tool from the actual discovery response',async()=>{
  for(const epubPresent of [false,true]){
    const out=ownedFolder('catalog'),names=['readmd_math','readmd_export_document','readmd_convert_to_markdown',...(epubPresent?['readmd_export_epub']:[])],fake=fakeSpawn({respond:message=>message.method==='initialize'?{protocolVersion:'2024-11-05'}:{tools:names.map(name=>({name,inputSchema:{type:'object'}}))}});
    const client=new qa.StdioClient({binary:'readmd.exe',profile:path.join(out,'profile'),documents:path.join(out,'documents'),assets:'assets',out,spawnImpl:fake.spawnImpl});
    if(epubPresent){await client.initialize(true);assert(client.tools.some(tool=>tool.name==='readmd_export_epub'));}else await assert.rejects(client.initialize(true),/Missing real tool readmd_export_epub/);
    const report={passed:true};await client.stop(report);assert.equal(fake.child.killCount,0);assert.equal(report.transportFatal,null);
  }
});
test('candidate, boot, manifest, QA and frozen source hashes stay bound throughout observation',()=>{
  const out=ownedFolder('binding'),digest=file=>crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');const names=['candidate','binary','boot','sourceFreeze','qa','copy'];
  for(const name of names)fs.writeFileSync(path.join(out,name),name,{flag:'wx'});
  const binding=Object.fromEntries(names.slice(0,4).map(name=>[name,path.join(out,name)]));for(const name of names.slice(0,4))binding[name+'Sha256']=digest(binding[name]);binding.boundFiles={'synthetic-qa':{file:path.join(out,'qa'),copy:path.join(out,'copy'),sha256:digest(path.join(out,'qa'))}};fs.writeFileSync(path.join(out,'copy'),'qa');
  qa.verifyBinding(binding);for(const name of names){const file=path.join(out,name),before=fs.readFileSync(file);fs.writeFileSync(file,'changed');assert.throws(()=>qa.verifyBinding(binding));fs.writeFileSync(file,before);qa.verifyBinding(binding);}
});
test('owned child has only System32 PATH, explicit profile and hidden process',async()=>{
  const environment=qa.systemEnvironment('own-profile','own-documents',{Path:'bad',PATH:'also-bad',SystemRoot:'C:\\Windows',READMD_PRINT_TOKEN:'remove',READMD_DATA_DIR:'other',READMD_WORKSPACE:'other',KEEP:'yes'});assert.equal(environment.PATH,'C:\\Windows\\System32');assert.equal(environment.Path,undefined);assert.equal(environment.READMD_PRINT_TOKEN,undefined);assert.equal(environment.READMD_DATA_DIR,'own-profile');assert.equal(environment.READMD_WORKSPACE,'own-documents');assert.equal(environment.KEEP,'yes');
  const {child,report}=await observation();assert.equal(child.invocation.options.windowsHide,true);assert.deepEqual(child.invocation.options.stdio,['pipe','pipe','pipe']);assert.equal(report.passed,true);assert.equal(child.killCount,0);
});
test('matrix inspection expands count groups and protects empty/ragged coordinates',()=>{
  const m=matrix(['left','right'],[2,0,1]);assert.deepEqual(qa.columnAlignments(m),['left','right']);qa.assertTables(value([m]),{alignments:[['left','right']],rows:[[2,0,1]]});
  assert.throws(()=>qa.assertTables(value([m]),{alignments:[['right','right']],rows:[[2,0,1]]}),/alignments/);assert.throws(()=>qa.assertTables(value([m]),{alignments:[['left','right']],rows:[[2,1,1]]}),/rows/);
  const group=element('mc',[element('mcPr',[property('count','2'),property('mcJc','right')])]),repeated=matrix(['center','center','center']);repeated.content[0].value.content[0].value.content=[{type:'element',value:group}];assert.deepEqual(qa.columnAlignments(repeated),['right','right','center']);
});
test('unknown and native structures cannot be counted as successful standard semantics',()=>{
  const actual=value([matrix(['left'])]);actual.document.diagnostics=[{code:'mathml_table_alignment_projection'}];assert.throws(()=>qa.assertTables(actual,{alignments:[['left']]}));
  assert.throws(()=>qa.errorPayload({isError:false,structuredContent:{ok:false,error_code:'transaction_conflict'}},'transaction_conflict'));assert.throws(()=>qa.errorPayload({isError:true,structuredContent:{ok:false,error_code:'other'}},'transaction_conflict'));assert.equal(qa.errorPayload({isError:true,content:[{type:'text',text:'{"ok":false,"error_code":"transaction_conflict"}'}]},'transaction_conflict').ok,false);
  const fn=argument=>element('func',[element('fName',[element('r',[element('t',['sin'])])]),element('e',[element('d',[element('dPr',[property('begChr','('),property('endChr',')')]),element('e',[element('r',[element('t',[argument])])])])])]);qa.assertFunctions(value([fn('x+1'),fn('y+2')]));assert.throws(()=>qa.assertFunctions(value([fn('x+1'),fn('x+1')])));assert.throws(()=>qa.assertFunctions(value([element('r',[element('t',['sin⁡x+1sin⁡y+2'])])])));
  const lost=fn('x+1');lost.content[1].value.content=[{type:'element',value:element('r',[element('t',['x+1'])])}];assert.throws(()=>qa.assertFunctions(value([lost,fn('y+2')])),/delimiter/);
});
test('native DOCX column checks inspect actual matrix tags and ignore adjacent math properties',()=>{
  const xml='<m:m><m:mPr><m:mcs><m:mc><m:mcPr><m:mcJc m:val="left"/></m:mcPr></m:mc></m:mcs></m:mPr><m:mr/></m:m><m:func/><m:m><m:mPr><m:mcs><m:mc><m:mcPr><m:mcJc m:val="right"/></m:mcPr></m:mc></m:mcs></m:mPr><m:mr/></m:m>';
  assert.deepEqual(qa.matrixXmlAlignments(xml),[['left'],['right']]);assert.notDeepEqual(qa.matrixXmlAlignments(xml.replace('m:val="right"','m:val="center"')),[['left'],['right']]);
});
test('normal stdin EOF waits for close and retains stdout/stderr arriving after exit',async()=>{
  const {client,child,report,out}=await observation({afterExit(child){child.stdout.write('{"jsonrpc":"2.0","method":"notifications/progress","params":{"progress":1}}\n');child.stderr.write('late stderr retained\n');}});
  assert.equal(report.passed,true);assert.equal(child.killCount,0);assert.equal(client.closed,true);assert.equal(client.logsClosed,true);assert.equal(report.logDescriptorsClosed,true);assert.equal(report.transportFatal,null);assert.equal(report.session.notifications,1);assert.equal(fs.readFileSync(path.join(out,'stderr.log'),'utf8'),'late stderr retained\n');assert(fs.readFileSync(path.join(out,'stdout.jsonl'),'utf8').includes('notifications/progress'));assert(fs.readFileSync(path.join(out,'events.jsonl'),'utf8').includes('"direction":"close"'));
});
test('bad JSON after the last resolved reply cannot pass with an empty pending map',async()=>{
  for(const trailing of ['late non-JSON\n','null\n','{"jsonrpc":"1.0"}\n']){
    const {report,client,child,out}=await observation({afterExit(child){child.stdout.write(trailing);}});assert.equal(client.pending.size,0);assert.equal(report.passed,false);assert(report.transportFatal);assert.equal(child.killCount,0);assert(fs.readFileSync(path.join(out,'stdout.jsonl'),'utf8').endsWith(trailing));
  }
});
test('abnormal final exit and unterminated final output remain failures after requested EOF',async()=>{
  for(const settings of [{code:7},{signal:'SIGTERM'},{afterExit(child){child.stdout.write('{"jsonrpc":"2.0"');}}]){
    const {report,child}=await observation(settings);assert.equal(report.passed,false);assert(report.transportFatal);assert.equal(report.hostStdioClosed,true);assert.equal(child.killCount,0);
  }
});
test('duplicate final response is rejected even when its original request completed',async()=>{
  const {report}=await observation({afterExit(child){child.stdout.write('{"jsonrpc":"2.0","id":1,"result":{"reply":"ping"}}\n');}});assert.equal(report.passed,false);assert.equal(report.transportFatal.kind,'uncorrelated_response');
});
test('forced cleanup is always a failed observation rather than normal host shutdown',async()=>{
  const {report,child}=await observation({hang:true});assert.equal(report.passed,false);assert.equal(report.session.forcedStop,true);assert.equal(child.killCount,1);assert.equal(report.transportFatal.kind,'close_timeout');assert.equal(report.logDescriptorsClosed,true);
});
