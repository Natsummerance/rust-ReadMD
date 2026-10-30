// Real executable/stdio integration. All files and AI requests are synthetic.
import fs from 'node:fs';
import path from 'node:path';
import http from 'node:http';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { once } from 'node:events';
import { createHash } from 'node:crypto';
const binary = process.env.READMD_BIN, root = process.env.READMD_MCP_TEST_ROOT;
assert.ok(binary && root && path.isAbsolute(root), 'Set READMD_BIN and an isolated READMD_MCP_TEST_ROOT');
const data = path.join(root, 'data'); fs.mkdirSync(data, {recursive:true});
const digest = value => createHash('sha256').update(value).digest('hex');
const binarySha256=digest(fs.readFileSync(binary));
if(process.env.READMD_EXPECTED_BINARY_SHA256)assert.equal(binarySha256,process.env.READMD_EXPECTED_BINARY_SHA256.toLowerCase());
let pdfRuntime=null;
if(process.env.READMD_PDF_WORKER||process.env.READMD_PDFIUM_LIB){
  assert.ok(process.env.READMD_PDF_WORKER&&process.env.READMD_PDFIUM_LIB,'Supply both explicit real PDFium runtime files');
  const platform=process.platform==='win32'?'windows':process.platform;
  const lockFile=path.join(process.env.READMD_ASSETS_DIR||path.resolve('assets'),'capabilities',`pdfium-7881-${platform}-${process.arch}.lock.json`);
  const lock=JSON.parse(fs.readFileSync(lockFile,'utf8'));pdfRuntime={source:'explicit-lock-verified-read-only-files',lockSha256:digest(fs.readFileSync(lockFile)),files:[]};
  for(const file of [process.env.READMD_PDF_WORKER,process.env.READMD_PDFIUM_LIB]){
    assert.ok(path.isAbsolute(file));const row=lock.files.find(row=>path.basename(row.path)===path.basename(file));assert.ok(row,'Explicit runtime is not in the platform lock');
    const bytes=fs.readFileSync(file);assert.equal(bytes.length,row.size);assert.equal(digest(bytes),row.sha256);pdfRuntime.files.push({path:file,sha256:row.sha256,bytes:row.size});
  }
}
const wireIn=fs.openSync(path.join(root,'stdio.stdin.jsonl'),'wx'),wireOut=fs.openSync(path.join(root,'stdio.stdout.jsonl'),'wx'),wireErr=fs.openSync(path.join(root,'stdio.stderr.log'),'wx');
const EXPECTED_TOOLS=[
  'readmd_publication_metadata','readmd_publication_profile','readmd_publication_blocks','readmd_publication_numbering','readmd_publication_inline_text',
  'readmd_openapi_document','readmd_pdf_document','readmd_academic_document','readmd_content_transaction','readmd_math',
  'readmd_fix_markdown','readmd_convert_to_markdown','readmd_web_to_markdown','readmd_ocr_to_markdown','readmd_export_document',
  'readmd_export_presets','readmd_latex_to_md','readmd_md_to_latex','readmd_parse_bibtex','readmd_latex_to_omml',
  'readmd_ai_assistant','readmd_ai_providers','readmd_ai_models','readmd_ai_complete','readmd_ai_chat','readmd_render_diagram',
  'readmd_process_imports','readmd_generate_toc','readmd_export_presentation','readmd_export_epub','readmd_run_code_chunk','readmd_pdf_audit',
  'readmd_analyze_document','readmd_search_workspace','readmd_suggest_links','readmd_resolve_link','readmd_read_document','readmd_edit_document',
  'readmd_document_history','readmd_pdf_rollback',
  'readmd_ai_task','readmd_export_snapshot','readmd_review_ai','readmd_review_document'
];
function assertSchema(schema,at){
  if(typeof schema==='boolean')return;
  assert.ok(schema&&typeof schema==='object'&&!Array.isArray(schema),at+' must be a JSON Schema');
  if(schema.type!==undefined){const types=Array.isArray(schema.type)?schema.type:[schema.type];assert.ok(types.length>0);assert.equal(new Set(types).size,types.length);for(const type of types)assert.ok(['object','array','string','number','integer','boolean','null'].includes(type),at+'.type');}
  if(schema.properties!==undefined){assert.ok(schema.properties&&typeof schema.properties==='object'&&!Array.isArray(schema.properties));for(const [key,value]of Object.entries(schema.properties))assertSchema(value,at+'.properties.'+key);}
  // A composed schema may require a property declared by an enclosing branch.
  if(schema.required!==undefined){assert.ok(Array.isArray(schema.required));assert.equal(new Set(schema.required).size,schema.required.length);for(const key of schema.required)assert.equal(typeof key,'string');}
  if(schema.additionalProperties!==undefined)assertSchema(schema.additionalProperties,at+'.additionalProperties');
  if(schema.items!==undefined){if(Array.isArray(schema.items))schema.items.forEach((item,index)=>assertSchema(item,at+'.items.'+index));else assertSchema(schema.items,at+'.items');}
  for(const key of ['oneOf','anyOf','allOf'])if(schema[key]!==undefined){assert.ok(Array.isArray(schema[key])&&schema[key].length>0);schema[key].forEach((item,index)=>assertSchema(item,at+'.'+key+'.'+index));}
  if(schema.enum!==undefined){assert.ok(Array.isArray(schema.enum)&&schema.enum.length>0);assert.equal(new Set(schema.enum.map(value=>JSON.stringify(value))).size,schema.enum.length,at+'.enum duplicates');}
  for(const key of ['minLength','maxLength','minItems','maxItems'])if(schema[key]!==undefined)assert.ok(Number.isSafeInteger(schema[key])&&schema[key]>=0,at+'.'+key);
  for(const key of ['minimum','maximum','exclusiveMinimum','exclusiveMaximum','multipleOf'])if(schema[key]!==undefined)assert.ok(Number.isFinite(schema[key])&&(key!=='multipleOf'||schema[key]>0),at+'.'+key);
  for(const [minimum,maximum]of [['minLength','maxLength'],['minItems','maxItems'],['minimum','maximum']])if(schema[minimum]!==undefined&&schema[maximum]!==undefined)assert.ok(schema[minimum]<=schema[maximum],at+' bounds');
  if(schema.uniqueItems!==undefined)assert.equal(typeof schema.uniqueItems,'boolean');
}
let slow = false, started;
const server = http.createServer((req, res) => {
  req.resume();
  if (req.url.endsWith('/models')) { res.writeHead(200, {'Content-Type':'application/json'}); res.end(JSON.stringify({data:[{id:'fixture-model'}]})); }
  else if (req.url.endsWith('/chat/completions')) {
    started?.();
    setTimeout(() => { res.writeHead(200, {'Content-Type':'application/json'}); res.end(JSON.stringify({choices:[{message:{content:'# Synthetic AI result'}}],usage:{total_tokens:12}})); }, slow ? 300 : 0);
  } else { res.writeHead(404); res.end(); }
});
server.listen(0, '127.0.0.1'); await once(server, 'listening');
fs.writeFileSync(path.join(data,'ai.json'), JSON.stringify({schema_version:3,providers:[{id:'custom:fixture',name:'Fixture',category:'local',format:'openai',mode:'chat',base_url:`http://127.0.0.1:${server.address().port}/v1`,models:[]}],current:{provider:'custom:fixture'}}));
const proc = spawn(binary, ['--mcp'], {windowsHide:true,env:{...process.env,READMD_DATA_DIR:data,READMD_WORKSPACE:root}});
const pending = new Map(), responses = [];
const exited=once(proc,'exit');
let id = 0;
proc.stdout.on('data',chunk=>fs.writeSync(wireOut,chunk));
createInterface({input:proc.stdout}).on('line', line => {
  const message = JSON.parse(line); responses.push(message);
  if (!Object.hasOwn(message, 'id')) return;
  const waiter = pending.get(message.id); if (!waiter) return;
  pending.delete(message.id); clearTimeout(waiter.timer); waiter.resolve(message);
});
proc.stderr.on('data',chunk=>fs.writeSync(wireErr,chunk));
proc.on('error', error => { for(const waiter of pending.values()) waiter.reject(error); });
const send = value => {const line=JSON.stringify(value)+'\n';fs.writeSync(wireIn,line);proc.stdin.write(line);};
const call = (method,params={}) => new Promise((resolve,reject) => {
  const requestId=++id;
  const timer=setTimeout(()=>{pending.delete(requestId);reject(Error('MCP response timeout: '+method));},30000);
  pending.set(requestId,{resolve,reject,timer}); send({jsonrpc:'2.0',id:requestId,method,params});
});
const tool = (name,args) => {report.lastTool=name;return call('tools/call',{name,arguments:args});};
const unwrap = message => { assert.ok(!message.error,JSON.stringify(message.error)); assert.ok(!message.result.isError,JSON.stringify(message.result.structuredContent)); return message.result.structuredContent || message.result.content[0].text; };
const expectToolError = (message,code) => {assert.ok(!message.error,JSON.stringify(message.error));assert.equal(message.result.isError,true);const value=message.result.structuredContent;assert.equal(value.ok,false);assert.equal(value.error_code,code,JSON.stringify(value));return value;};
const report = {ok:false,checks:[],binarySha256,pid:proc.pid,syntheticAi:{loopbackProtocolOnly:true,productionModelQualityCertified:false},nativePdf:{state:'not-probed'},tex:{state:'not-probed'}};
try {
  const meta={'io.modelcontextprotocol/protocolVersion':'2026-07-28','io.modelcontextprotocol/clientCapabilities':{}};
  const discovery=await call('server/discover',{_meta:meta}); assert.equal(discovery.result.resultType,'complete'); report.checks.push('modern discovery');
  const hello=await call('initialize',{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'readmd-fixture',version:'1'}}); assert.equal(hello.result.protocolVersion,'2025-11-25');
  send({jsonrpc:'2.0',method:'notifications/initialized'}); report.checks.push('legacy handshake');
  const catalog=(await call('tools/list')).result.tools;
  assert.equal(new Set(EXPECTED_TOOLS).size,EXPECTED_TOOLS.length,'Expected catalog itself must be unique');
  assert.equal(new Set(catalog.map(item=>item.name)).size,catalog.length,'Real catalog contains duplicate names');
  assert.deepEqual(catalog.map(item=>item.name).sort(),[...EXPECTED_TOOLS].sort(),'Complete explicit Rust tool catalog');
  for(const item of catalog){assert.match(item.name,/^readmd_[a-z0-9_]+$/);assert.equal(typeof item.description,'string');assert.ok(item.description.length>0);assert.equal(item.inputSchema.type,'object');assertSchema(item.inputSchema,item.name);for(const key of ['readOnlyHint','destructiveHint','idempotentHint','openWorldHint'])assert.equal(typeof item.annotations[key],'boolean',item.name+'.'+key);}
  report.tools=catalog.length;report.toolNames=[...EXPECTED_TOOLS].sort();report.checks.push('complete unique tool names and legal recursive JSON Schemas');
  const pdfTool=catalog.find(item=>item.name==='readmd_pdf_document');assert.ok(pdfTool);assert.ok(pdfTool.inputSchema.required.includes('confirm'));assert.ok(pdfTool.inputSchema.properties.action.enum.includes('resume'));
  assert.equal((await tool('readmd_pdf_document',{action:'probe'})).result.structuredContent.error_code,'confirmation_required');
  const probe=await tool('readmd_pdf_document',{action:'probe',confirm:true});
  if(probe.result.isError){const error=expectToolError(probe,'pdf_engine_unavailable');assert.equal(error.session,undefined);report.nativePdf={state:'unavailable',errorCode:error.error_code,annotationSaveReopen:'unverified-capability-unavailable',runtime:pdfRuntime};}
  else{const ready=unwrap(probe);assert.equal(ready.ok,true);assert.equal(ready.engine,'pdfium');assert.equal(ready.version,'7881');report.nativePdf={state:'ready',engine:ready.engine,version:ready.version,annotationSaveReopen:'pending',runtime:pdfRuntime||{source:'managed-profile',profile:data}};}
  report.checks.push('actual PDFium readiness or exact typed unavailable, with explicit confirmation');
  const math=unwrap(await tool('readmd_math',{operation:'parse',source:'\\frac{a}{b}',dialect:'latex'}));assert.ok(math);
  const content='hello',transaction={base_revision:createHash('sha256').update(content).digest('hex'),origin:'mcp',edits:[{from:0,to:5,insert:'world'}]};
  const tx=unwrap(await tool('readmd_content_transaction',{content,transaction}));assert.equal(tx.result.content,'world');
  assert.equal((await tool('readmd_content_transaction',{content,transaction:[]})).result.structuredContent.error_code,'invalid_arguments');report.checks.push('structured math and revision-safe transaction parameters accepted; arrays rejected for object schema');
  const publicationTools=['readmd_publication_metadata','readmd_publication_profile','readmd_publication_blocks','readmd_publication_numbering','readmd_publication_inline_text'];
  for(const name of publicationTools){
    const entry=catalog.find(item=>item.name===name);assert.equal(entry.annotations.readOnlyHint,true);assert.equal(entry.annotations.destructiveHint,false);assert.equal(entry.annotations.openWorldHint,false);
    expectToolError(await tool(name,{action:name==='readmd_publication_blocks'?'list':'get',content:false}),'invalid_arguments');
    expectToolError(await tool(name,{action:name==='readmd_publication_blocks'?'list':'get',content:'Body',unknown:true}),'invalid_arguments');
  }
  const publicationSource='\uFEFF---\r\ntitle: Original title\r\ncustom: keep\r\n---\r\n\r\n# First {#first}\r\n\r\n## Second {#second}\r\n\r\n😀 selected text stays.\r\n';
  const publicationFile=path.join(root,'publication-source.md');fs.writeFileSync(publicationFile,publicationSource);
  const presetFile=path.join(data,'export_presets.json'),presetsBefore=fs.existsSync(presetFile)?fs.readFileSync(presetFile):null;
  const restoreInMemory=async(changed,inverse)=>{
    const restored=unwrap(await tool('readmd_content_transaction',{content:changed.content,transaction:{base_revision:changed.revision,origin:'editor',edits:inverse}}));assert.equal(restored.result.content,publicationSource);
  };
  const metadata=unwrap(await tool('readmd_publication_metadata',{action:'get',content:publicationSource}));assert.equal(metadata.revision,digest(publicationSource));assert.equal(metadata.sourceValues.title,'Original title');
  const metadataRequest={action:'patch',content:publicationSource,base_revision:metadata.revision,patch:{title:'Revised title'}};
  const metadataChanged=unwrap(await tool('readmd_publication_metadata',metadataRequest));assert.equal(metadataChanged.changed,true);assert.equal(metadataChanged.sourceValues.title,'Revised title');assert.ok(metadataChanged.content.includes('custom: keep\r\n'));assert.ok(metadataChanged.content.endsWith('😀 selected text stays.\r\n'));
  await restoreInMemory(metadataChanged,metadataChanged.inverse);
  expectToolError(await tool('readmd_publication_metadata',{...metadataRequest,content:metadataChanged.content}),'transaction_conflict');report.checks.push('publication metadata actual read, reversible patch and stale rejection');
  const profile=unwrap(await tool('readmd_publication_profile',{action:'get',content:publicationSource}));assert.deepEqual(profile.sourceValues,{});
  const profileRequest={action:'patch',content:publicationSource,base_revision:profile.revision,patch:{footnotes:{start:4}}};
  const profileChanged=unwrap(await tool('readmd_publication_profile',profileRequest));assert.equal(profileChanged.changed,true);assert.equal(profileChanged.sourceValues.footnotes.start,4);
  await restoreInMemory(profileChanged,profileChanged.inverse);
  expectToolError(await tool('readmd_publication_profile',{...profileRequest,content:profileChanged.content}),'transaction_conflict');report.checks.push('publication profile actual read, reversible source patch and stale rejection');
  const blocks=unwrap(await tool('readmd_publication_blocks',{action:'list',content:publicationSource}));assert.equal(blocks.document.revision,digest(publicationSource));
  const paragraph=blocks.document.nodes.find(node=>node.kind==='paragraph'&&publicationSource.slice(node.from,node.to).includes('😀'));assert.ok(paragraph,'Actual UTF-16 paragraph range');
  const blockRequest={action:'apply',content:publicationSource,base_revision:blocks.document.revision,id:paragraph.id,patch:{text:{size:14},paragraph:{align:'center'}}};
  const blockChanged=unwrap(await tool('readmd_publication_blocks',blockRequest));assert.equal(blockChanged.result.changed,true);assert.ok(blockChanged.result.content.includes('readmd:layout v1'));
  await restoreInMemory(blockChanged.result,blockChanged.result.inverse);
  expectToolError(await tool('readmd_publication_blocks',{...blockRequest,content:blockChanged.result.content}),'transaction_conflict');report.checks.push('publication blocks actual UTF-16 list, reversible local style and stale rejection');
  const numbering=unwrap(await tool('readmd_publication_numbering',{action:'get',content:publicationSource}));assert.equal(numbering.config,null);
  const numberingRequest={action:'apply',content:publicationSource,base_revision:numbering.revision,config:{headings:{levels:[{format:'upperRoman'},{format:'lowerLetter'}],separator:'.',suffix:'space'}}};
  const numberingChanged=unwrap(await tool('readmd_publication_numbering',numberingRequest));assert.equal(numberingChanged.result.changed,true);assert.equal(numberingChanged.plan.headings[0].id,'first');assert.equal(numberingChanged.plan.headings[1].numbering.text,'I.a');
  await restoreInMemory(numberingChanged.result,numberingChanged.result.inverse);
  expectToolError(await tool('readmd_publication_numbering',{...numberingRequest,content:numberingChanged.result.content}),'transaction_conflict');report.checks.push('publication numbering actual get, stable identities, inverse and stale rejection');
  const inline=unwrap(await tool('readmd_publication_inline_text',{action:'get',content:publicationSource}));assert.equal(inline.revision,digest(publicationSource));
  const selection=publicationSource.indexOf('selected'),inlineRequest={action:'apply',content:publicationSource,base_revision:inline.revision,from:selection,to:selection+'selected'.length,patch:{size:30,sizeCs:10}};
  const inlineChanged=unwrap(await tool('readmd_publication_inline_text',inlineRequest));assert.equal(inlineChanged.result.changed,true);assert.ok(inlineChanged.result.content.includes('data-size-pt="30"'));assert.ok(inlineChanged.result.content.includes('data-size-cs-pt="10"'));assert.ok(inlineChanged.result.content.endsWith(' text stays.\r\n'));
  await restoreInMemory(inlineChanged.result,inlineChanged.result.inverse);
  expectToolError(await tool('readmd_publication_inline_text',{...inlineRequest,content:inlineChanged.result.content}),'transaction_conflict');report.checks.push('publication inline text actual UTF-16 edit, independent sizes, inverse and stale rejection');
  assert.equal(fs.readFileSync(publicationFile,'utf8'),publicationSource,'Publication planners changed the source file');
  assert.deepEqual(fs.existsSync(presetFile)?fs.readFileSync(presetFile):null,presetsBefore,'Publication planners changed user export presets');
  const specification='openapi: 3.1.1\ninfo: {title: Research API, version: "1"}\npaths:\n  /notes:\n    get:\n      responses:\n        200: {description: Works}\n';
  const apiDoc=unwrap(await tool('readmd_openapi_document',{source:specification,source_name:'research.yaml',content:'',base_revision:createHash('sha256').update('').digest('hex'),locale:'zh-CN'}));assert.equal(apiDoc.changes[0].kind,'added');assert.ok(apiDoc.result.content.includes('响应'));
  const apiUpdated=unwrap(await tool('readmd_openapi_document',{source:specification,source_name:'research.yaml',content:apiDoc.result.content,base_revision:apiDoc.result.revision,locale:'zh-CN'}));assert.equal(apiUpdated.result.changed,false);assert.equal(apiUpdated.source_map[0].pointer,'/paths/~1notes/get');report.checks.push('shared offline OpenAPI YAML generation, source map and idempotent update over stdio');
  for (const name of ['readmd_analyze_document','readmd_search_workspace','readmd_read_document','readmd_edit_document','readmd_document_history']) {
    assert.ok(catalog.some(item=>item.name===name),name);
  }
  const source=path.join(root,'reading.md'), linked=path.join(root,'Plato.md');
  fs.writeFileSync(linked,'---\ntags:\n  - philosophy\n---\n# Plato\n\nA careful argument.\n');
  fs.writeFileSync(source,'# Reading\n\n[[Plato]]\n\n[Broken](#missing)\n');
  const suggestions=unwrap(await tool('readmd_suggest_links',{workspace_root:root,file_path:source,kind:'wiki',query:'Pla'}));
  assert.equal(suggestions.items[0].insert,'Plato');
  const resolved=unwrap(await tool('readmd_resolve_link',{workspace_root:root,file_path:source,target:'Plato#plato'}));
  assert.equal(path.basename(resolved.path),'Plato.md'); assert.equal(resolved.heading.position.line,5);
  const escaped=await tool('readmd_resolve_link',{workspace_root:root,file_path:source,target:'../../outside'});
  assert.equal(escaped.result.isError,true);
  for(const name of ['readmd_suggest_links','readmd_resolve_link'])assert.equal(catalog.find(t=>t.name===name).annotations.readOnlyHint,true);
  report.checks.push('shared local suggestions and verified heading resolution');
  const analyzed=unwrap(await tool('readmd_analyze_document',{content:fs.readFileSync(source,'utf8'),file_path:source,workspace_root:root}));
  assert.equal(path.basename(analyzed.links[0].resolved_path),'Plato.md');
  assert.ok(analyzed.diagnostics.some(item=>item.code==='missing_anchor'));
  report.checks.push('AST wiki targets and missing anchors');
  const search=unwrap(await tool('readmd_search_workspace',{workspace_root:root,query:'tag:philosophy "careful argument"'}));
  assert.equal(search.results[0].relative_path,'Plato.md');
  report.checks.push('fresh workspace phrase and YAML tag search');
  const original=fs.readFileSync(linked,'utf8'), read=unwrap(await tool('readmd_read_document',{file_path:linked}));
  const edit={file_path:linked,expected_revision:read.revision,old_text:'careful',new_text:'clear'};
  assert.equal(unwrap(await tool('readmd_edit_document',edit)).dry_run,true);
  assert.equal(fs.readFileSync(linked,'utf8'),original);
  assert.equal((await tool('readmd_edit_document',{...edit,dry_run:false})).result.structuredContent.error_code,'confirmation_required');
  const committed=unwrap(await tool('readmd_edit_document',{...edit,dry_run:false,confirm:true}));
  assert.match(fs.readFileSync(linked,'utf8'),/clear argument/);
  assert.equal((await tool('readmd_edit_document',edit)).result.structuredContent.error_code,'document_conflict');
  report.checks.push('revision-protected preview and confirmed commit');
  fs.unlinkSync(linked);
  const history=unwrap(await tool('readmd_document_history',{file_path:linked,operation:'read',checkpoint_id:committed.checkpoint_id}));
  assert.equal(history.content,original);
  report.checks.push('recovery after original file deletion');
  const presets=unwrap(await tool('readmd_export_presets',{})); assert.ok(presets.presets.classic); report.checks.push('real export preset catalog');
  const models=unwrap(await tool('readmd_ai_models',{provider:'custom:fixture',confirm:true})); assert.ok(models.models.includes('fixture-model')); report.checks.push('model discovery over loopback HTTP');
  const aiArgs={provider:'custom:fixture',model:'fixture-model',skill_id:'readmd-summary',markdown_content:'# Synthetic source'};
  assert.equal(unwrap(await tool('readmd_ai_chat',aiArgs)).content,'# Synthetic AI result'); report.checks.push('structured AI result');
  const completion=unwrap(await tool('readmd_ai_complete',{provider:'custom:fixture',model:'fixture-model',prefix:'# Research\n\nWe examine ',suffix:'\n\n## Conclusion'}));
  assert.equal(completion.completion,'# Synthetic AI result');
  assert.match(completion.context_revision,/^[a-f0-9]{64}$/);
  assert.equal(catalog.find(t=>t.name==='readmd_ai_complete').annotations.readOnlyHint,true);
  report.checks.push('actual shared Rust inline-completion endpoint over stdio');
  const diagram=unwrap(await tool('readmd_render_diagram',{engine:'wsd',code:'A -> B: hello',confirm:true})); assert.match(diagram.svg,/<svg/); report.checks.push('native diagram rendering');
  const output=path.join(root,'export.html'); fs.writeFileSync(output,'KEEP');
  const args={markdown_content:'# Export acceptance',output_path:output,output_format:'html',confirm:true};
  assert.equal((await tool('readmd_export_document',args)).result.structuredContent.error_code,'output_exists'); assert.equal(fs.readFileSync(output,'utf8'),'KEEP');
  unwrap(await tool('readmd_export_document',{...args,overwrite:true,style_preset:'classic'})); assert.match(fs.readFileSync(output,'utf8'),/Export acceptance/); report.checks.push('protected and atomic HTML replacement');
  const texPdf=path.join(root,'unavailable-tex.pdf'),oldTex=Buffer.from('OWNED OLD PDF TARGET\n');fs.writeFileSync(texPdf,oldTex);
  const texFailure=expectToolError(await tool('readmd_export_document',{markdown_content:'# Native TeX request\n\nA vector equation: $x^2 + y^2 = z^2$.',output_path:texPdf,output_format:'pdf',options:{pdf:{engine:'xelatex'}},overwrite:true,confirm:true}),'latex_engine_unavailable');
  assert.deepEqual(fs.readFileSync(texPdf),oldTex,'Unavailable TeX engine changed the old target');report.tex={state:'unavailable',errorCode:texFailure.error_code,oldTargetPreserved:true};report.checks.push('real typed TeX capability failure preserves the old PDF target');
  const academicPdf=path.join(root,'academic.pdf');
  unwrap(await tool('readmd_export_document',{markdown_content:'# Native core PDF output\n\nA vector equation: $x^2 + y^2 = z^2$.',output_path:academicPdf,output_format:'pdf',options:{pdf:{engine:'rust'}},confirm:true}));
  assert.equal(fs.readFileSync(academicPdf).subarray(0,5).toString(),'%PDF-');report.checks.push('actual built-in core PDF export without an optional TeX engine');
  if(report.nativePdf.state==='ready'){
    let pdf=unwrap(await tool('readmd_pdf_document',{action:'open',path:academicPdf,confirm:true}));
    pdf=unwrap(await tool('readmd_pdf_document',{action:'edit',session:pdf.session,revision:pdf.revision,operation:{kind:'note',page:0,x:40,y:700,text:'MCP reviewed'},confirm:true}));
    unwrap(await tool('readmd_pdf_document',{action:'save',session:pdf.session,revision:pdf.revision,confirm:true}));
    unwrap(await tool('readmd_pdf_document',{action:'close',session:pdf.session,confirm:true}));
    pdf=unwrap(await tool('readmd_pdf_document',{action:'open',path:academicPdf,confirm:true}));assert.ok(pdf.annotations.some(a=>a.text==='MCP reviewed'));
    unwrap(await tool('readmd_pdf_document',{action:'close',session:pdf.session,confirm:true}));report.nativePdf.annotationSaveReopen='passed';report.checks.push('shared core PDF draft, native annotation save and reopen over actual stdio');
  }else{
    const before=fs.readFileSync(academicPdf),unavailable=expectToolError(await tool('readmd_pdf_document',{action:'open',path:academicPdf,confirm:true}),'pdf_engine_unavailable');
    assert.equal(unavailable.session,undefined);assert.deepEqual(fs.readFileSync(academicPdf),before,'Unavailable PDFium open changed the real core PDF');report.nativePdf.noSession=true;report.nativePdf.oldPdfPreserved=true;report.checks.push('absent platform PDFium is explicitly unavailable, creates no session and preserves the real core PDF; annotation save/reopen remains unverified');
  }
  const pixel='iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg==';
  fs.writeFileSync(path.join(root,'fixture.png'),Buffer.from(pixel,'base64'));
  for (const theme of ['black','night']) {
    const slides=path.join(root,`slides-${theme}.html`);
    unwrap(await tool('readmd_export_presentation',{markdown_content:'# Exported offline\n\n![Local](fixture.png)\n\n$x^2$\n\n```mermaid\ngraph LR\n A-->B\n```\n\n---\n\n# Second slide',output_path:slides,base_dir:root,theme,overwrite:true,confirm:true}));
    assert.ok(fs.readFileSync(slides,'utf8').includes('data:image/png;base64,'+pixel));
  }
  report.checks.push('standalone presentations embed source-directory images');
  const burst=await Promise.all(Array.from({length:128},()=>tool('readmd_fix_markdown',{content:('# Heading\n\nText | value\n').repeat(400)})));
  const busy=burst.filter(response=>response.error?.code===-32001).length;
  for(const response of burst) if(response.error) assert.equal(response.error.code,-32001); else assert.equal(response.result.structuredContent.ok,true);
  assert.ok(burst.length-busy>0); report.stress={requests:128,completed:128-busy,busyRejected:busy}; report.checks.push('bounded concurrent load');
  slow=true;
  const reached=new Promise(resolve=>started=resolve), cancelledId=++id;
  send({jsonrpc:'2.0',id:cancelledId,method:'tools/call',params:{name:'readmd_ai_chat',arguments:aiArgs}}); await reached;
  send({jsonrpc:'2.0',id:cancelledId,method:'tools/call',params:{name:'readmd_fix_markdown',arguments:{content:'duplicate'}}});
  send({jsonrpc:'2.0',method:'notifications/cancelled',params:{requestId:cancelledId}});
  await new Promise(resolve=>setTimeout(resolve,500));
  const cancelled=responses.filter(response=>response.id===cancelledId);
  assert.equal(cancelled.length,1); assert.equal(cancelled[0].error.code,-32600); assert.ok((await call('ping')).result); report.checks.push('duplicate ID rejected; cancellation suppresses late result; connection survives');
  const oversized='x'.repeat(32*1024*1024+1)+'\n';fs.writeSync(wireIn,oversized);proc.stdin.write(oversized); await call('ping');
  assert.ok(responses.some(response=>response.id===null && response.error?.code===-32600)); report.checks.push('oversized input drains and recovers');
  report.ok=true;
} catch(error) {
  report.firstFailure={lastTool:report.lastTool,lastCompletedCheck:report.checks.at(-1)||null,message:error.message,stack:error.stack};
  process.exitCode=1;
} finally {
  for(const waiter of pending.values()) clearTimeout(waiter.timer);
  proc.stdin.end();let cleanupTimer;const exit=await Promise.race([exited,new Promise(resolve=>{cleanupTimer=setTimeout(()=>{report.forcedStop=true;proc.kill();resolve(null);},5000);})]);clearTimeout(cleanupTimer);report.hostExit=exit;if(!exit||exit[0]!==0)report.ok=false;
  server.close(); server.closeAllConnections();
  for(const fd of [wireIn,wireOut,wireErr])fs.closeSync(fd);
  if(pdfRuntime)for(const file of pdfRuntime.files)if(digest(fs.readFileSync(file.path))!==file.sha256){report.ok=false;report.pdfRuntimeChanged=true;}
  report.finalBinarySha256=digest(fs.readFileSync(binary));if(report.finalBinarySha256!==binarySha256){report.ok=false;report.binaryChanged=true;}
  fs.writeFileSync(path.join(root,'stdio-result.json'),JSON.stringify(report,null,2));console.log(JSON.stringify(report));if(!report.ok)process.exitCode=1;
}
