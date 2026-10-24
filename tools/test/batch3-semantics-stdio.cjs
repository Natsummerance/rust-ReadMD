'use strict';
// Bounded, synthetic M10/M11 acceptance over the candidate's real stdio host.
// --plan never starts a process or writes evidence. Historical runs are immutable.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {spawn,spawnSync}=require('node:child_process');
const {StringDecoder}=require('node:string_decoder');
const {isDeepStrictEqual}=require('node:util');
const {createTransportGuard,parseMcpLine,finishTransportObservation,payload}=require('./managed-tex-process-cold.cjs');
const zipParts=require('../../ui-tests/zip-parts.cjs');
const ROOT=path.resolve(__dirname,'../..');
const BASE=path.join(ROOT,'.cache/capability-upgrade/batch3-20261020/math');
const BUDGET=180000,MAX_WIRE=64*1024*1024;
const hash=value=>crypto.createHash('sha256').update(value).digest('hex');
const fileHash=file=>hash(fs.readFileSync(file));
const local=node=>node.name.split(':').at(-1);
const children=node=>(node.content||[]).filter(part=>part.type==='element').map(part=>part.value);
const flatten=node=>[node,...children(node).flatMap(flatten)];
const text=node=>(node.content||[]).map(part=>part.type==='text'?part.value:part.type==='element'?text(part.value):'').join('');
const child=(node,name)=>children(node).find(node=>local(node)===name);
const attr=(node,name)=>Object.entries(node?.attributes||{}).find(([key])=>key.split(':').at(-1)===name)?.[1];
const property=(node,container,key)=>attr(child(child(node,container)||{content:[]},key),'val');
const matrices=node=>flatten(node).filter(node=>local(node)==='m');
const functions=node=>flatten(node).filter(node=>local(node)==='func');
const TABLE='<mtable columnalign="center left" columnspacing="1.25pt" rowspacing="12pt" align="baseline -1"><mtr columnalign="left right"><mtd columnalign="center"><mi>a</mi></mtd><mtd><mi>wide</mi></mtd><mtd><mi>c</mi></mtd></mtr><mtr columnalign="center left"><mtd><mi>long</mi></mtd><mtd columnalign="right"><mi>e</mi></mtd><mtd columnalign="right"><mi>f</mi></mtd></mtr></mtable>';
const CANONICAL='<math><mtable columnalign="center right right" columnspacing="1.25pt" rowspacing="12pt" align="baseline -1"><mtr><mtd><mi>a</mi></mtd><mtd><mi>wide</mi></mtd><mtd><mi>c</mi></mtd></mtr><mtr><mtd><mi>long</mi></mtd><mtd><mi>e</mi></mtd><mtd><mi>f</mi></mtd></mtr></mtable></math>';
const UNICODE='𝛼+sin\u2061(x+1)+sin\u2061(x+1)';
const UNICODE_CHANGED='𝛼+sin\u2061(x+1)+sin\u2061(y+2)';
const positiveTables=[
  {name:'cascade',source:'<math>'+TABLE+'</math>',alignments:[['center','right','right']],rows:[[3,3]],spacing:true},
  {name:'empty-ragged',source:'<math><mtable><mtr columnalign="left right"><mtd><mi>a</mi></mtd><mtd><mi>b</mi></mtd><mtd><mi>c</mi></mtd></mtr><mtr columnalign="center"/><mtr columnalign="left"><mtd><mi>d</mi></mtd></mtr></mtable></math>',alignments:[['left','right','right']],rows:[[3,0,1]],reparse:false},
  {name:'nested-style',source:'<math><mstyle columnalign="left"><mtable columnalign="center"><mtr columnalign="center"><mtd columnalign="right"><mtable><mtr><mtd><mi>a</mi></mtd></mtr></mtable></mtd></mtr></mtable></mstyle><mtable><mtr><mtd><mi>b</mi></mtd></mtr></mtable></math>',alignments:[['right'],['left'],['center']],rows:[[1],[1],[1]]},
  {name:'uniform-minima',source:'<math><mtable columnalign="left" columnspacing="1.5em" rowspacing="12pt"><mtr columnalign="right center"><mtd style="box-sizing:content-box;min-width:36pt;padding-left:0pt;padding-right:0.75em;text-align:right"><mi>a</mi></mtd><mtd style="box-sizing:content-box;min-width:36pt;padding-left:0.75em;padding-right:0pt;text-align:center"><mi>b</mi></mtd></mtr><mtr columnalign="left center"><mtd columnalign="right" style="box-sizing:content-box;min-width:36pt;padding-left:0pt;padding-right:0.75em;text-align:right"><mi>c</mi></mtd><mtd style="box-sizing:content-box;min-width:36pt;padding-left:0.75em;padding-right:0pt;text-align:center"><mi>d</mi></mtd></mtr></mtable></math>',alignments:[['right','center']],rows:[[2,2]],minimum:true},
  {name:'unit-spans',source:'<math><mtable><mtr columnalign="left"><mtd rowspan="1" columnspan="1"><mi>x</mi></mtd></mtr></mtable></math>',alignments:[['left']],rows:[[1]]}
];
const negativeTables=[
  {name:'conflict',source:'<math><mtable columnalign="center left"><mtr columnalign="left right"><mtd><mi>a</mi></mtd><mtd><mi>b</mi></mtd></mtr><mtr columnalign="right"><mtd><mi>c</mi></mtd><mtd><mi>d</mi></mtd></mtr></mtable></math>',diagnostic:'mathml_table_alignment_projection',alignments:[['center','right']]},
  ...['','left right','decimalpoint','LEFT'].map((value,index)=>({name:'invalid-cell-'+index,source:`<math><mtable columnalign="left"><mtr columnalign="right"><mtd columnalign="${value}"><mi>x</mi></mtd></mtr></mtable></math>`,diagnostic:'mathml_table_alignment_projection',alignments:[['right']]})),
  ...['','left decimalpoint','RIGHT'].map((value,index)=>({name:'invalid-row-'+index,source:`<math><mtable columnalign="left"><mtr columnalign="${value}"><mtd><mi>x</mi></mtd></mtr></mtable></math>`,diagnostic:'mathml_table_alignment_projection',alignments:[['left']]})),
  ...['columnspan="2"','rowspan="2"','columnspan="0"'].map((attribute,index)=>({name:'unsupported-span-'+index,source:`<math><mtable columnalign="center"><mtr columnalign="left"><mtd ${attribute}><mi>x</mi></mtd></mtr></mtable></math>`,diagnostic:'mathml_table_alignment_projection',alignments:[['center']]})),
  {name:'labeled-row-boundary',source:'<math><mtable><mlabeledtr><mtd><mtext>(1)</mtext></mtd><mtd><mi>x</mi></mtd></mlabeledtr></mtable></math>',diagnostic:'mathml_labeled_row'},
  {name:'rowalign-boundary',source:'<math><mtable rowalign="top"><mtr><mtd><mi>x</mi></mtd></mtr></mtable></math>',diagnostic:'mathml_table_spacing_projection'},
  {name:'tensor-projection-boundary',source:'<math><mmultiscripts><mi>T</mi><mi>i</mi><mi>j</mi><mi>k</mi><mi>l</mi></mmultiscripts></math>',diagnostic:'mathml_multiscript_projection'},
  {name:'content-mathml-boundary',source:'<math><apply><plus/><ci>x</ci><cn>1</cn></apply></math>',diagnostic:'mathml_unsupported_apply'}
];
const invalidFunctions=['⁡x','\\funcapply x','f⁡','f⁡()','f⁡〖〗','()⁡x','f⁡)','f⁡+x','f⁡⁡x','a+⁡x'];
const projectedMathmlFunctions=[
  '<math><mrow><mi>f</mi><mo>⁡</mo><mi>x</mi><mi>y</mi></mrow></math>',
  '<math><mrow><mi>f</mi><mo>⁡</mo><mi>g</mi><mo>⁡</mo><mi>x</mi></mrow></math>',
  '<math><mrow><mi>f</mi><mo lspace="1em">⁡</mo><mi>x</mi></mrow></math>',
  '<math><mi>f</mi><mo>⁡</mo><mi>x</mi></math>'
];
const invalidMathmlFunctions=[
  '<math><mrow><mi>f</mi><mo>⁡</mo></mrow></math>',
  '<math><mrow><mi>f</mi><mo>⁡</mo><mrow/></mrow></math>',
  '<math><mo>⁡</mo></math>'
];
const QA_FILES=['tools/test/batch3-semantics-stdio.cjs','tools/test/batch3-semantics-stdio.test.cjs','tools/test/managed-tex-process-cold.cjs','ui-tests/zip-parts.cjs','ui-tests/zip-fixtures.cjs','tools/test/inspect-mcp-publication-closure.py'];
const PRODUCT_FILES=['rust/readmd-kernel/src/math_formats.rs','rust/readmd-kernel/src/unicode_math.rs','rust/readmd-kernel/src/math_document.rs','rust/readmd-kernel/src/upgrade_api.rs','rust/readmd-kernel/src/mcp.rs','rust/readmd-kernel/src/omml.rs'];

function argumentsFor(argv,env=process.env){
  const options={candidate:env.READMD_MCP_CANDIDATE,binary:env.READMD_BIN,assets:env.READMD_ASSETS_DIR,python:env.READMD_TEST_PYTHON,exports:false,plan:false};
  for(let index=0;index<argv.length;index++){
    const argument=argv[index];
    if(argument==='--plan'){options.plan=true;continue;}
    if(argument==='--exports'){options.exports=true;continue;}
    if(argument==='--help')return {help:true};
    assert(['--candidate','--binary','--assets','--python','--output'].includes(argument),'Unknown argument: '+argument);
    assert(index+1<argv.length,'Missing argument: '+argument);options[argument.slice(2)]=argv[++index];
  }
  for(const key of ['candidate','binary','assets','python','output'])if(options[key])options[key]=path.resolve(options[key]);
  if(!options.plan){assert(options.candidate,'Required --candidate or READMD_MCP_CANDIDATE');if(options.exports)assert(options.python,'--exports requires an existing QA-only --python or READMD_TEST_PYTHON');}
  if(options.output)inside(BASE,options.output);
  return options;
}
function inside(base,file){const relative=path.relative(base,path.resolve(file));assert(relative&&!relative.startsWith('..'+path.sep)&&relative!=='..'&&!path.isAbsolute(relative),'Path escaped owned evidence scope: '+file);return path.resolve(file);}
function canonical(node){
  const attrs=Object.entries(node.attributes||{}).filter(([key])=>key!=='xmlns'&&!key.startsWith('xmlns:')&&key!=='readmdId').map(([key,value])=>[key.split(':').at(-1),value]).sort(([a],[b])=>a.localeCompare(b));
  return {name:local(node),attributes:attrs,content:(node.content||[]).map(part=>part.type==='element'?canonical(part.value):part)};
}
function columnAlignments(matrix){
  const columns=Math.max(0,...children(matrix).filter(node=>local(node)==='mr').map(row=>children(row).filter(node=>local(node)==='e').length));
  const result=[];const groups=child(child(matrix,'mPr')||{content:[]},'mcs');
  for(const group of children(groups||{content:[]}))if(local(group)==='mc'){
    const count=Number(property(group,'mcPr','count')??1),alignment=property(group,'mcPr','mcJc')??'center';
    assert(Number.isSafeInteger(count)&&count>0&&count<=64,'Invalid actual column count');assert(['left','center','right'].includes(alignment));
    result.push(...Array(count).fill(alignment));
  }
  assert(result.length<=columns,'Extra matrix column groups');return result.concat(Array(columns-result.length).fill('center'));
}
function assertTables(value,fixture){
  assert.deepEqual(value.document.diagnostics,[],'Standard input gained a diagnostic');const actual=matrices(value.document.root);
  assert.deepEqual(actual.map(columnAlignments),fixture.alignments,'Actual editable column alignments differ');
  if(fixture.rows)assert.deepEqual(actual.map(matrix=>children(matrix).filter(node=>local(node)==='mr').map(row=>children(row).filter(node=>local(node)==='e').length)),fixture.rows,'Empty/ragged matrix rows changed');
  if(fixture.spacing)for(const matrix of actual){assert.equal(property(matrix,'mPr','baseJc'),'bot');assert.equal(property(matrix,'mPr','cGpRule'),'3');assert.equal(property(matrix,'mPr','cGp'),'25');assert.equal(property(matrix,'mPr','rSpRule'),'3');assert.equal(property(matrix,'mPr','rSp'),'12');}
  if(fixture.minimum)for(const matrix of actual){assert.equal(property(matrix,'mPr','cSp'),'720');assert.equal(property(matrix,'mPr','cGpRule'),'1');assert.equal(property(matrix,'mPr','rSp'),'12');}
  assert(typeof value.preview==='string'&&value.preview.includes('<math'),'Missing real MathML preview');assert(typeof value.vector_preview==='string'&&value.vector_preview.includes('<svg'),'Missing real vector preview');
  return actual;
}
function assertFunctions(value,args=['x+1','y+2']){
  assert.deepEqual(value.document.diagnostics,[]);const actual=functions(value.document.root);assert.equal(actual.length,2,'Native function objects lost');
  assert.deepEqual(actual.map(node=>text(child(node,'fName'))),['sin','sin']);assert.deepEqual(actual.map(node=>text(child(node,'e'))),args);
  for(const node of actual){
    assert.deepEqual(children(node).filter(node=>!local(node).endsWith('Pr')).map(local),['fName','e']);
    const delimiters=flatten(child(node,'e')).filter(node=>local(node)==='d');assert.equal(delimiters.length,1,'Visible argument delimiter object lost');
    assert.equal(property(delimiters[0],'dPr','begChr')??'(','(');assert.equal(property(delimiters[0],'dPr','endChr')??')',')');assert.equal(children(delimiters[0]).filter(node=>local(node)==='e').length,1);
  }
  return actual;
}
function assertScriptedFunction(value){
  assert.deepEqual(value.document.diagnostics,[]);const actual=functions(value.document.root);assert.equal(actual.length,1);const name=child(actual[0],'fName'),scripts=flatten(name).filter(node=>local(node)==='sSup');assert.equal(scripts.length,1);assert.equal(text(child(scripts[0],'e')),'sin');assert.equal(text(child(scripts[0],'sup')),'2');assert.equal(text(child(actual[0],'e')),'x');
}
function errorPayload(result,code){
  const value=result.structuredContent??JSON.parse((result.content||[]).find(item=>item.type==='text').text);
  assert.equal(result.isError,true,'Expected tool error '+code);assert.equal(value.ok,false);assert.equal(value.error_code,code,JSON.stringify(value));return value;
}
function exportRequestFor(format,parameters){
  assert(['docx','epub','pdf'].includes(format),'Unsupported bounded export format: '+format);
  assert(parameters&&typeof parameters==='object'&&!Array.isArray(parameters),'Export parameters required');
  assert(!Object.hasOwn(parameters,'output_format'),'The export request builder owns output_format');
  const args={...parameters};if(format!=='epub')args.output_format=format;
  return {name:format==='epub'?'readmd_export_epub':'readmd_export_document',arguments:args};
}
function assertToolCallSchema(call,catalog){
  const tool=catalog.find(tool=>tool.name===call.name);assert(tool,'Missing real tool '+call.name);
  function matches(value,schema){
    if(Object.hasOwn(schema,'const')&&!isDeepStrictEqual(value,schema.const))return false;
    if(schema.enum&&!schema.enum.some(item=>isDeepStrictEqual(value,item)))return false;
    if(schema.anyOf&&!schema.anyOf.some(branch=>matches(value,branch)))return false;
    if(schema.allOf&&!schema.allOf.every(branch=>matches(value,branch)))return false;
    if(schema.oneOf&&schema.oneOf.filter(branch=>matches(value,branch)).length!==1)return false;
    if(Array.isArray(schema.type))return schema.type.some(type=>matches(value,{...schema,type}));
    switch(schema.type){
      case 'object':{
        if(!value||typeof value!=='object'||Array.isArray(value))return false;
        if((schema.required||[]).some(key=>!Object.hasOwn(value,key)))return false;
        return Object.entries(value).every(([key,item])=>Object.hasOwn(schema.properties||{},key)?matches(item,schema.properties[key]):schema.additionalProperties===false?false:typeof schema.additionalProperties==='object'?matches(item,schema.additionalProperties):true);
      }
      case 'array':return Array.isArray(value)&&(schema.minItems===undefined||value.length>=schema.minItems)&&(schema.maxItems===undefined||value.length<=schema.maxItems)&&(!schema.items||value.every(item=>matches(item,schema.items)))&&(!schema.uniqueItems||value.every((item,index)=>!value.slice(0,index).some(previous=>isDeepStrictEqual(item,previous))));
      case 'string':return typeof value==='string'&&(schema.minLength===undefined||[...value].length>=schema.minLength)&&(schema.maxLength===undefined||[...value].length<=schema.maxLength);
      case 'integer':if(!Number.isInteger(value))return false;
      // An integer must also meet the finite numeric bounds below.
      case 'number':return typeof value==='number'&&Number.isFinite(value)&&(schema.minimum===undefined||value>=schema.minimum)&&(schema.maximum===undefined||value<=schema.maximum);
      case 'boolean':return typeof value==='boolean';
      case 'null':return value===null;
      case undefined:return true;
      default:return false;
    }
  }
  assert(matches(call.arguments,tool.inputSchema),'Generated arguments violate the real '+call.name+' inputSchema');return call;
}
function systemEnvironment(profile,documents,env=process.env){
  const result={...env};for(const key of Object.keys(result))if(['PATH','READMD_PRINT_TOKEN','READMD_DATA_DIR','READMD_WORKSPACE'].includes(key.toUpperCase()))delete result[key];
  result.PATH=path.win32.join(env.SystemRoot||env.WINDIR||'C:\\Windows','System32');result.READMD_DATA_DIR=profile;result.READMD_WORKSPACE=documents;return result;
}
async function waitBounded(promise,milliseconds){let timer;try{return await Promise.race([promise,new Promise(resolve=>{timer=setTimeout(()=>resolve(false),milliseconds);})]);}finally{clearTimeout(timer);}}
class StdioClient{
  constructor(options){
    this.options=options;this.pending=new Map();this.next=0;this.buffer='';this.decoder=new StringDecoder('utf8');this.bytes=0;this.closed=false;this.stopped=false;this.logsClosed=false;
    this.info={transport:options.spawnImpl?'simulated-stdio-selftest':'real-stdio',system32OnlyPath:true,forcedStop:false,windowsHide:true,requests:0,responses:0,notifications:0};
    this.fds={};for(const name of ['stdin.jsonl','stdout.jsonl','stderr.log','events.jsonl'])this.fds[name]=fs.openSync(path.join(options.out,name),'wx');
    this.transport=createTransportGuard(error=>{for(const pending of this.pending.values()){clearTimeout(pending.timer);pending.reject(error);}this.pending.clear();});
    const spawnImpl=options.spawnImpl||spawn,environment=systemEnvironment(options.profile,options.documents,options.env),command=['--mcp','--data-dir',options.profile,'--workspace',options.documents,'--assets',options.assets];
    this.info.command=command;this.info.environment={PATH:environment.PATH,READMD_DATA_DIR:environment.READMD_DATA_DIR,READMD_WORKSPACE:environment.READMD_WORKSPACE};
    try{this.child=spawnImpl(options.binary,command,{windowsHide:true,stdio:['pipe','pipe','pipe'],env:environment});}catch(error){for(const fd of Object.values(this.fds))fs.closeSync(fd);this.logsClosed=true;throw error;}
    this.info.pid=this.child.pid;
    this.closeObserved=new Promise(resolve=>this.child.once('close',()=>{
      this.buffer+=this.decoder.end();if(this.buffer.trim())this.transport.fail(Error('MCP stdout ended with an unterminated JSON line'),'incomplete_json');
      this.closed=true;this.event('close',this.info.hostExit||{});resolve(true);
    }));
    this.child.on('error',error=>this.transport.fail(error,'process_error'));
    this.child.on('exit',(code,signal)=>{this.stopped=true;this.info.hostExit={code,signal};this.event('exit',{code,signal});this.transport.exit(code,signal);});
    this.child.stdin.on('error',error=>this.transport.fail(error,'stdin_error'));
    this.child.stdout.on('error',error=>this.transport.fail(error,'stdout_error'));
    this.child.stderr.on('error',error=>this.transport.fail(error,'stderr_error'));
    this.child.stdout.on('data',chunk=>{
      try{fs.writeSync(this.fds['stdout.jsonl'],chunk);this.bytes+=chunk.length;if(this.bytes>MAX_WIRE){this.transport.fail(Error('MCP stdout budget exceeded'),'stdout_budget');this.forceStop();return;}
        this.buffer+=this.decoder.write(chunk);for(;;){const index=this.buffer.indexOf('\n');if(index<0)break;const line=this.buffer.slice(0,index);this.buffer=this.buffer.slice(index+1);if(line.trim())this.receive(line);}
      }catch(error){this.transport.fail(error,'stdout_log_error');this.forceStop();}
    });
    this.child.stderr.on('data',chunk=>{try{fs.writeSync(this.fds['stderr.log'],chunk);}catch(error){this.transport.fail(error,'stderr_log_error');this.forceStop();}});
  }
  event(direction,value){fs.writeSync(this.fds['events.jsonl'],JSON.stringify({at:new Date().toISOString(),direction,value})+'\n');}
  receive(line){
    const message=parseMcpLine(line,this.transport);if(!message)return;
    this.event('receive',message);
    if(!Object.hasOwn(message,'id')){if(message.method==='notifications/progress'){this.info.notifications++;return;}this.transport.fail(Error('Unexpected MCP notification'),'uncorrelated_response');return;}
    const pending=this.pending.get(message.id);if(!pending){this.transport.fail(Error('Uncorrelated or duplicate MCP response id '+message.id),'uncorrelated_response');return;}
    this.pending.delete(message.id);clearTimeout(pending.timer);this.info.responses++;
    if(Object.hasOwn(message,'error')===Object.hasOwn(message,'result')){this.transport.fail(Error('MCP response must contain exactly one result or error'),'invalid_envelope');pending.reject(Error('Invalid MCP response'));return;}
    message.error?pending.reject(Error('JSON-RPC error: '+JSON.stringify(message.error))):pending.resolve(message.result);
  }
  send(message){
    if(this.transport.fatal)throw Error(this.transport.fatal.message);assert(!this.closed,'MCP stdio already closed');
    const line=JSON.stringify(message)+'\n';fs.writeSync(this.fds['stdin.jsonl'],line);this.event('send',message);
    this.child.stdin.write(line,error=>{if(error)this.transport.fail(error,'stdin_error');});
  }
  request(method,params={},timeout=30000){
    if(this.transport.fatal)return Promise.reject(Error(this.transport.fatal.message));
    return new Promise((resolve,reject)=>{const id=++this.next;const timer=setTimeout(()=>this.transport.fail(Error('MCP request deadline: '+method),'request_deadline'),timeout);this.pending.set(id,{resolve,reject,timer});this.info.requests++;
      try{this.send({jsonrpc:'2.0',id,method,params});}catch(error){this.transport.fail(error,'stdin_error');}
    });
  }
  tool(name,args,timeout){return this.request('tools/call',{name,arguments:args},timeout);}
  async initialize(exportsEnabled){
    const hello=await this.request('initialize',{protocolVersion:'2024-11-05',capabilities:{},clientInfo:{name:'readmd-batch3-semantics-stdio',version:'1'}});assert.equal(hello.protocolVersion,'2024-11-05');
    this.send({jsonrpc:'2.0',method:'notifications/initialized'});const list=await this.request('tools/list');assert(Array.isArray(list.tools));this.tools=list.tools;
    for(const name of exportsEnabled?['readmd_math','readmd_export_document','readmd_export_epub','readmd_convert_to_markdown']:['readmd_math'])assert(list.tools.some(tool=>tool.name===name),'Missing real tool '+name);
  }
  forceStop(){if(!this.closed){this.info.forcedStop=true;this.child.kill();}}
  async stop(report){
    if(this.stopPromise)return this.stopPromise;
    this.stopPromise=(async()=>{
      this.transport.beginClose();this.info.pendingAtClose=this.pending.size;if(this.pending.size)this.transport.fail(Error('MCP closed with pending calls'),'pending_at_close');
      if(!this.closed){try{this.child.stdin.end();}catch(error){this.transport.fail(error,'stdin_error');}}
      if(!this.closed&&!await waitBounded(this.closeObserved,this.options.closeTimeout??15000)){this.transport.fail(Error('MCP did not close after stdin EOF'),'close_timeout');this.forceStop();await waitBounded(this.closeObserved,this.options.killTimeout??5000);}
      finishTransportObservation(report,this.transport,this.stopped,this.closed);report.session=this.info;
      const closeLogs=()=>{if(!this.logsClosed){for(const fd of Object.values(this.fds))fs.closeSync(fd);this.logsClosed=true;}};
      if(this.closed)closeLogs();else this.child.once('close',closeLogs);
      report.logDescriptorsClosed=this.logsClosed;if(this.info.forcedStop){report.passed=false;report.error??='Owned MCP host required forced termination';}
    })();return this.stopPromise;
  }
}

function bindingFor(options){
  const candidate=JSON.parse(fs.readFileSync(options.candidate,'utf8'));const binary=options.binary||candidate.binary,assets=options.assets||path.dirname(candidate.boot),boot=path.join(assets,'readmd.boot.js');
  const sourceFreeze=inside(ROOT,path.resolve(ROOT,candidate.sourceFreeze));assert.equal(fileHash(sourceFreeze),candidate.sourceFreezeSha256,'Source-freeze manifest changed');
  const freeze=JSON.parse(fs.readFileSync(sourceFreeze,'utf8'));const boundFiles={};
  for(const name of [...QA_FILES,...PRODUCT_FILES]){
    const row=freeze.sources.find(row=>row.path===name);assert(row,'Candidate did not freeze QA/product source '+name);const file=inside(ROOT,path.join(ROOT,name)),copy=inside(ROOT,path.join(ROOT,row.copy));
    assert.equal(fileHash(file),row.sha256,'Working source differs from candidate '+name);assert.equal(fileHash(copy),row.sha256,'Frozen source differs '+name);boundFiles[name]={file,copy,sha256:row.sha256};
  }
  if(process.env.READMD_CANDIDATE_SHA256)assert.equal(candidate.binarySha256,process.env.READMD_CANDIDATE_SHA256);
  if(process.env.READMD_BOOT_SHA256)assert.equal(candidate.bootSha256,process.env.READMD_BOOT_SHA256);
  const binding={candidate:options.candidate,candidateSha256:fileHash(options.candidate),binary,assets,boot,binarySha256:candidate.binarySha256,bootSha256:candidate.bootSha256,sourceFreeze,sourceFreezeSha256:candidate.sourceFreezeSha256,boundFiles};
  verifyBinding(binding);return binding;
}
function verifyBinding(binding){
  for(const [file,digest]of [[binding.candidate,binding.candidateSha256],[binding.binary,binding.binarySha256],[binding.boot,binding.bootSha256],[binding.sourceFreeze,binding.sourceFreezeSha256]])assert.equal(fileHash(file),digest,'Candidate identity changed: '+file);
  for(const [name,row]of Object.entries(binding.boundFiles)){assert.equal(fileHash(row.file),row.sha256,'QA/product source changed: '+name);assert.equal(fileHash(row.copy),row.sha256,'Frozen source changed: '+name);}
}
function planFor(options){return {schemaVersion:1,scope:'M10 explicit UnicodeMath function application and M11 representable table column alignment',transport:'real-stdio',synthetic:true,noProcessesStarted:true,noFilesWritten:true,
  candidateRequiredForExecution:true,binaryGuiStarted:false,system32OnlyPath:true,ownedProfileAndDocuments:true,managedEnginesCopied:false,
  positiveTables:positiveTables.map(({name,source,alignments,reparse})=>({name,sourceSha256:hash(source),alignments,threeTargetReparse:reparse!==false})),negativeTables:negativeTables.map(({name,source,diagnostic})=>({name,sourceSha256:hash(source),diagnostic,serializeMustFail:'math_invalid_source'})),
  repeatedMatrices:{sourceSha256:hash('<math>'+TABLE+TABLE+'</math>'),selectedMatrix:2,selectedColumn:2,change:'right to left',staleMustFail:'transaction_conflict'},
  repeatedFunctions:{source:UNICODE,sourceSha256:hash(UNICODE),selectedFunction:2,slot:'e',replacement:'y+2',expected:UNICODE_CHANGED,staleMustFail:'transaction_conflict'},
  invalidFunctionCount:invalidFunctions.length,invalidFunctionError:'math_missing_argument',projectedMathmlFunctionCount:projectedMathmlFunctions.length,invalidMathmlFunctionCount:invalidMathmlFunctions.length,serializationTargets:['omml','mathml','dsl'],
  exports:{enabled:options.exports,formats:['docx','epub','pdf'],tools:{docx:'readmd_export_document',epub:'readmd_export_epub',pdf:'readmd_export_document'},actualToolSchemaPreflight:true,pdfEngine:'rust',checks:['Actual OPC matrix mcJc sequence and native function count','Actual EPUB MathML table attributes and U+2061 markers','Actual PDF independently read by existing QA-only PyMuPDF inspector; vector drawings, no image fallback, marker text','Real MCP conversion readback of each actual file'],qaOnlyPythonRequired:options.exports,externalApplicationRoundtripCertified:false},
  qaSha256:Object.fromEntries(QA_FILES.filter(name=>fs.existsSync(path.join(ROOT,name))).map(name=>[name,fileHash(path.join(ROOT,name))])),
  limitations:['M10 native MathML function recovery is limited to explicit grouped name/U+2061/argument rows; juxtaposition and ambiguous ungrouped application are outside this acceptance.','Ragged and empty rows retain the existing editable shape; three-target reparse of that existing irregular shape is not certified here.','Conflicting columns, invalid overrides, spans, labeled rows, vertical alignment, complex tensors and Content MathML remain diagnosed projections.','No GUI, Word/independent-reader round trip, complete M10/M11 acceptance or third-batch completion is certified.']};}

async function semantics(client,retain,check){
  const math=async(label,args,error)=>{retain('requests/'+label+'.json',args);const result=await client.tool('readmd_math',args);retain('responses/'+label+'.json',result);return error?errorPayload(result,error):payload(result);};
  const parse=async(label,source,dialect)=>math(label,{operation:'parse',source,dialect,display:true,font_size:12});
  async function roundtrip(label,source,dialect,verify){
    const outputs={};for(const target of ['omml','mathml','dsl']){const serialized=await math(label+'-'+target+'-serialize',{operation:'serialize',source,dialect,display:true,target,font_size:12});assert.equal(typeof serialized.output,'string');assert(serialized.output.length>0);assert.equal(serialized.extension_preservation,null,'Standard source was classified as unknown extension');
      retain('outputs/'+label+'.'+target,serialized.output);const reopened=await parse(label+'-'+target+'-reparse',serialized.output,target);verify(reopened,target);outputs[target]=serialized.output;}
    return outputs;
  }
  for(const fixture of positiveTables){
    retain('inputs/m11-'+fixture.name+'.mathml',fixture.source);const parsed=await parse('m11-'+fixture.name,fixture.source,'mathml');assertTables(parsed,fixture);
    if(fixture.reparse!==false)await roundtrip('m11-'+fixture.name,fixture.source,'mathml',value=>assertTables(value,fixture));
    check('M11 '+fixture.name,{alignments:fixture.alignments,threeTargetReparse:fixture.reparse!==false});
  }
  const canonicalTable=await parse('m11-canonical-explicit-columns',CANONICAL,'mathml'),cascade=await parse('m11-cascade-equivalence','<math>'+TABLE+'</math>','mathml');
  assert.deepEqual(canonical(cascade.document.root),canonical(canonicalTable.document.root));assert.equal(cascade.vector_preview,canonicalTable.vector_preview,'Effective cascade and explicit equivalent table render differently');check('M11 effective cascade equals explicit standard columns in the shared tree and vector preview');
  for(const fixture of negativeTables){
    retain('inputs/m11-'+fixture.name+'.mathml',fixture.source);const parsed=await parse('m11-'+fixture.name,fixture.source,'mathml');assert.equal(parsed.document.source,fixture.source,'Original unsupported source was not retained');assert(parsed.document.diagnostics.some(item=>item.code===fixture.diagnostic),'Missing projection diagnostic '+fixture.diagnostic);
    if(fixture.alignments)assert.deepEqual(matrices(parsed.document.root).map(columnAlignments),fixture.alignments,'Projection chose an arbitrary row');
    await math('m11-'+fixture.name+'-serialize-rejected',{operation:'serialize',source:fixture.source,dialect:'mathml',display:true,target:'omml'},'math_invalid_source');check('M11 rejected '+fixture.name,{diagnostic:fixture.diagnostic,editableEquivalentCertified:false});
  }
  const repeated='<math>'+TABLE+TABLE+'</math>';retain('inputs/m11-repeated.mathml',repeated);
  const repeatedDsl=await math('m11-repeated-dsl',{operation:'serialize',source:repeated,dialect:'mathml',display:true,target:'dsl'});const document=await parse('m11-repeated-dsl-parse',repeatedDsl.output,'dsl');
  const original=assertTables(document,{alignments:[['center','right','right'],['center','right','right']],rows:[[3,3],[3,3]],spacing:true});assert.notEqual(original[0].id,original[1].id);
  const sourceNodes=document.document.nodes.filter(node=>node.kind==='m');assert.equal(sourceNodes.length,2);const firstSource=repeatedDsl.output.slice(sourceNodes[0].from,sourceNodes[0].to);
  const operation={operation:'matrix_column_alignment',node_id:original[1].id,index:1,alignment:'left'};
  const changed=await math('m11-second-matrix-patch',{operation:'patch',source:repeatedDsl.output,dialect:'dsl',display:true,expected_revision:document.document.revision,patch:operation});
  const selected=assertTables(changed,{alignments:[['center','right','right'],['center','left','right']],rows:[[3,3],[3,3]],spacing:true});assert.deepEqual(canonical(original[0]),canonical(selected[0]),'First repeated matrix changed');assert.equal(text(original[1]),text(selected[1]),'Column alignment edit changed matrix text');
  const kept=changed.document.nodes.filter(node=>node.kind==='m')[0];assert.equal(changed.document.source.slice(kept.from,kept.to),firstSource,'First repeated matrix source changed');
  await math('m11-second-matrix-stale-rejected',{operation:'patch',source:changed.document.source,dialect:changed.document.dialect,display:true,expected_revision:document.document.revision,patch:operation},'transaction_conflict');
  const matrixOutputs=await roundtrip('m11-selected-matrix',changed.document.source,changed.document.dialect,value=>assertTables(value,{alignments:[['center','right','right'],['center','left','right']],rows:[[3,3],[3,3]],spacing:true}));
  check('M11 second repeated matrix edit preserves first source and tree, rejects stale revision, and reparses all three targets');

  retain('inputs/m10-repeated.unicodemath',UNICODE);const unicode=await parse('m10-repeated',UNICODE,'unicodemath');assertFunctions(unicode,['x+1','x+1']);
  const sourceFunctions=unicode.document.nodes.filter(node=>node.kind==='func');assert.equal(sourceFunctions.length,2);assert.notEqual(sourceFunctions[0].id,sourceFunctions[1].id);
  assert.deepEqual(sourceFunctions.map(({from,to})=>[from,to]),[[3,12],[13,22]]);assert.deepEqual(sourceFunctions.map(node=>node.slots.map(({role,from,to})=>[role,from,to])),[[['fName',3,6],['e',7,12]],[['fName',13,16],['e',17,22]]]);
  const functionOperation={operation:'replace_slot',node_id:sourceFunctions[1].id,role:'e',text:'y+2'};
  const functionChange=await math('m10-second-argument-patch',{operation:'patch',source:UNICODE,dialect:'unicodemath',display:true,expected_revision:unicode.document.revision,patch:functionOperation});assert.equal(functionChange.document.source,UNICODE_CHANGED);assert.equal(functionChange.document.dialect,'unicodemath');assertFunctions(functionChange);
  await math('m10-second-argument-stale-rejected',{operation:'patch',source:functionChange.document.source,dialect:'unicodemath',display:true,expected_revision:unicode.document.revision,patch:functionOperation},'transaction_conflict');
  const functionOutputs=await roundtrip('m10-selected-function',UNICODE_CHANGED,'unicodemath',value=>assertFunctions(value));
  check('M10 explicit repeated functions retain native fName/e and visible argument delimiters through all three targets; exact UTF-16 second-argument editing and stale rejection pass',{scopedMathmlNativeFunctionRoundtripCertified:true});
  for(const source of ['sin^2⁡x','sin⁡^2 x','sin\\funcapply^2 x']){
    const label='m10-script-'+hash(source).slice(0,8);retain('inputs/'+label+'.unicodemath',source);const parsed=await parse(label,source,'unicodemath');assertScriptedFunction(parsed);await roundtrip(label,source,'unicodemath',assertScriptedFunction);
  }
  for(const [source,replacement,expected,role]of [['sin⁡〖x+1〗','y+2','sin⁡〖y+2〗','e'],['sin⁡^2 x','cos','cos⁡ x','fName']]){
    const label='m10-group-'+role,parsed=await parse(label,source,'unicodemath'),node=parsed.document.nodes.find(node=>node.kind==='func');assert(node);const changed=await math(label+'-patch',{operation:'patch',source,dialect:'unicodemath',display:true,expected_revision:parsed.document.revision,patch:{operation:'replace_slot',node_id:node.id,role,text:replacement}});assert.equal(changed.document.source,expected);assert.deepEqual(changed.document.diagnostics,[]);if(role==='e')assert(!flatten(changed.document.root).some(node=>local(node)==='d'),'Hidden grouping acquired a visible delimiter');
  }
  for(const [index,source]of invalidFunctions.entries()){retain('inputs/m10-invalid-'+index+'.unicodemath',source);await math('m10-invalid-'+index,{operation:'parse',source,dialect:'unicodemath',display:true},'math_missing_argument');}
  const deepFunction='f⁡'.repeat(65)+'x';retain('inputs/m10-depth-budget.unicodemath',deepFunction);await math('m10-depth-budget',{operation:'parse',source:deepFunction,dialect:'unicodemath',display:true},'math_budget');
  for(const [index,source]of ['sin x','f\\⁡x','"sin⁡x"'].entries()){const parsed=await parse('m10-not-inferred-'+index,source,'unicodemath');assert.equal(functions(parsed.document.root).length,0,'Ordinary/literal source was reclassified as a function');}
  const unknownSource='sin\\funcapplyExtra x',unknown=await parse('m10-unknown-macro',unknownSource,'unicodemath');assert.equal(functions(unknown.document.root).length,0);assert.deepEqual(unknown.document.diagnostics.map(({code,from,to})=>[code,from,to]),[['math_unknown_macro',3,18]]);await math('m10-unknown-serialize-rejected',{operation:'serialize',source:unknownSource,dialect:'unicodemath',display:true,target:'omml'},'math_invalid_source');
  check('M10 scripted names, hidden argument grouping, exact invalid-input codes and literal/macro boundaries pass');
  const nested='<math><mrow><mi>f</mi><mo>⁡</mo><mrow><mi>g</mi><mo>⁡</mo><mi>x</mi></mrow></mrow></math>';
  const verifyNested=value=>{assert.deepEqual(value.document.diagnostics,[]);const actual=functions(value.document.root);assert.equal(actual.length,2);assert.deepEqual(actual.map(node=>text(child(node,'fName'))),['f','g']);assert.deepEqual(actual.map(node=>text(child(node,'e'))),['gx','x']);};
  verifyNested(await parse('m10-nested-mathml',nested,'mathml'));await roundtrip('m10-nested-mathml',nested,'mathml',verifyNested);
  for(const [index,source]of projectedMathmlFunctions.entries()){
    retain('inputs/m10-mathml-projection-'+index+'.mathml',source);const parsed=await parse('m10-mathml-projection-'+index,source,'mathml');assert.equal(parsed.document.source,source);assert(parsed.document.diagnostics.some(item=>item.code==='mathml_function_application_projection'));assert.equal(functions(parsed.document.root).length,0);assert(text(parsed.document.root).includes('⁡'),'Projection discarded the original application marker');await math('m10-mathml-projection-'+index+'-serialize-rejected',{operation:'serialize',source,dialect:'mathml',display:true,target:'omml'},'math_invalid_source');
  }
  for(const [index,source]of invalidMathmlFunctions.entries()){retain('inputs/m10-mathml-invalid-'+index+'.mathml',source);await math('m10-mathml-invalid-'+index,{operation:'parse',source,dialect:'mathml',display:true},'math_missing_argument');}
  check('Explicit nested MathML functions retain native roles in all three targets; ambiguous and incomplete application rows are rejected from editable-equivalence acceptance');
  return {matrixDsl:matrixOutputs.dsl,functionDsl:functionOutputs.dsl};
}

function matrixXmlAlignments(xml){
  return [...xml.matchAll(/<m:m(?:\s[^>]*)?>[\s\S]*?<\/m:m>/g)].map(([matrix])=>[...matrix.matchAll(/<m:mcJc\b[^>]*\bm:val="([^"]*)"[^>]*\/?\s*>/g)].map(match=>match[1]));
}
async function exportChecks(client,options,binding,evidence,standard,retain,check,report){
  report.exportVerification.executed=true;
  const markdown='# BATCH3MATH20261020\n\nBounded editable mathematics.\n\n```readmd-math v1\n'+standard.matrixDsl+'\n```\n\n```readmd-math v1\n'+standard.functionDsl+'\n```\n';retain('documents/Input.md',markdown);
  const settings={pdf:{engine:'rust'},cover:{enabled:false},toc:{enabled:false},frontMatter:{mode:'none'},footer:{pageNumbers:false},typography:{font:'Arial',size:12}};
  for(const format of ['docx','epub','pdf']){
    verifyBinding(binding);const file=path.join(evidence,'outputs','Math.'+format);const request=exportRequestFor(format,{markdown_content:markdown,output_path:file,base_dir:path.join(evidence,'documents'),confirm:true,title:'BATCH3MATH20261020',options:settings});retain('requests/export-'+format+'.json',request);assertToolCallSchema(request,client.tools);
    const exported=await client.tool(request.name,request.arguments,90000);retain('responses/export-'+format+'.json',exported);payload(exported);assert(fs.statSync(file).size>0);
    const actual={format,path:path.relative(evidence,file),bytes:fs.statSync(file).size,sha256:fileHash(file),inspectionPassed:false};report.outputs.push(actual);
    if(format==='docx'){
      const parts=zipParts(fs.readFileSync(file)),xml=parts.get('word/document.xml');assert(xml);retain('inspection/document.xml',xml);assert.deepEqual(matrixXmlAlignments(xml),[['center','right','right'],['center','left','right']]);assert.equal((xml.match(/<m:func(?:\s[^>]*)?>/g)||[]).length,2);actual.nativeMatrices=2;actual.nativeFunctions=2;
    }else if(format==='epub'){
      const parts=zipParts(fs.readFileSync(file));assert.equal(parts.get('mimetype'),'application/epub+zip');const content=[...parts].filter(([name])=>name.endsWith('.xhtml')).map(([,value])=>value).join('\n');retain('inspection/epub-xhtml.xml',content);
      const alignments=[...content.matchAll(/<mtable\b[^>]*\bcolumnalign="([^"]*)"/g)].map(match=>match[1]);assert.deepEqual(alignments,['center right right','center left right']);const applyMarkers=(content.match(/<mo(?:\s[^>]*)?>(?:&#x0*2061;|&#0*8289;|\u2061)<\/mo>/gi)||[]).length;assert.equal(applyMarkers,2);actual.mathmlMatrices=2;actual.functionApplicationMarkers=2;
    }else{
      const bytes=fs.readFileSync(file);assert(bytes.subarray(0,5).equals(Buffer.from('%PDF-')));assert(bytes.subarray(-1024).includes(Buffer.from('%%EOF')));
      const inspector=path.join(ROOT,'tools/test/inspect-mcp-publication-closure.py');const inspected=spawnSync(options.python,[inspector,file],{windowsHide:true,timeout:60000,maxBuffer:16*1024*1024,env:systemEnvironment(path.join(evidence,'profile'),path.join(evidence,'documents'))});
      retain('inspection/pdf.stdout.json',inspected.stdout||Buffer.alloc(0));retain('inspection/pdf.stderr.log',inspected.stderr||Buffer.alloc(0));assert(!inspected.error,inspected.error?.message);assert.equal(inspected.status,0,'Existing independent PDF reader failed');const value=JSON.parse(inspected.stdout);assert.equal(value.kind,'pdf');assert(value.pages.some(page=>page.text.includes('BATCH3MATH20261020')));assert(value.pages.some(page=>page.drawings>0),'Actual PDF lacks vector mathematics');assert(value.pages.every(page=>page.images===0),'Actual PDF fell back to an image');actual.independentReader=value.reader;actual.pages=value.pages.length;actual.vectorMathReadback=true;actual.imageFallback=false;
    }
    const converted=await client.tool('readmd_convert_to_markdown',{file_path:file},90000);retain('responses/readback-'+format+'.json',converted);assert.notEqual(converted.isError,true,JSON.stringify(converted.structuredContent));const value=converted.content.filter(item=>item.type==='text').map(item=>item.text).join('\n');assert(value.includes('BATCH3MATH20261020'),'Actual file marker missing from real MCP readback');retain('inspection/readback-'+format+'.md',value);actual.inspectionPassed=true;check('Actual '+format+' output inspected and read back',{applicationRoundtripCertified:false});
  }
  report.exportVerification.passed=true;
}
async function main(){
  const options=argumentsFor(process.argv.slice(2));if(options.help){console.log('node tools/test/batch3-semantics-stdio.cjs --plan [--exports]\nnode tools/test/batch3-semantics-stdio.cjs --candidate CANDIDATE_JSON [--binary EXE --assets ASSETS --output NEW_OWNED_DIRECTORY] [--exports --python EXISTING_QA_PYTHON]');return;}
  if(options.plan){console.log(JSON.stringify(planFor(options),null,2));return;}
  assert.equal(process.platform,'win32','This acceptance run describes the Windows stdio host');const binding=bindingFor(options);if(options.exports)assert(fs.statSync(options.python).isFile(),'Existing QA-only Python executable required');fs.mkdirSync(BASE,{recursive:true});
  const evidence=options.output?(fs.mkdirSync(options.output),options.output):fs.mkdtempSync(path.join(BASE,'stdio-'));for(const name of ['profile','documents','outputs','requests','responses','inputs','inspection','wire'])fs.mkdirSync(path.join(evidence,name));
  const retain=(name,value)=>{const file=inside(evidence,path.join(evidence,name));fs.writeFileSync(file,typeof value==='string'||Buffer.isBuffer(value)?value:JSON.stringify(value,null,2)+'\n',{flag:'wx'});return {path:name,sha256:fileHash(file),bytes:fs.statSync(file).size};};
  const report={schemaVersion:1,passed:false,scope:'bounded-M10-M11-real-stdio',synthetic:true,startedUtc:new Date().toISOString(),candidateBinding:binding,checks:[],outputs:[],exportVerification:{requested:options.exports,executed:false,passed:options.exports?false:null},pdfInspectorPython:options.exports?{path:options.python,sha256:fileHash(options.python),qaOnly:true}:null,fullM10Certified:false,fullM11Certified:false,fullI10Certified:false,fullBatch3Certified:false,guiStarted:false,managedEngineCopied:false,bootLoadedByStdio:false,limitations:planFor(options).limitations};
  let client,timer;const check=(name,details={})=>report.checks.push({name,...details});
  try{
    retain('plan.json',planFor(options));client=new StdioClient({binary:binding.binary,assets:binding.assets,profile:path.join(evidence,'profile'),documents:path.join(evidence,'documents'),out:path.join(evidence,'wire')});
    timer=setTimeout(()=>{client.transport.fail(Error('Entire bounded workflow exceeded '+BUDGET+'ms'),'workflow_deadline');client.forceStop();},BUDGET);
    await client.initialize(options.exports);const standard=await semantics(client,retain,check);if(options.exports)await exportChecks(client,options,binding,evidence,standard,retain,check,report);report.passed=true;
  }catch(error){report.error=String(error.stack||error);}
  finally{
    clearTimeout(timer);if(client)await client.stop(report);else report.processStarted=false;
    try{verifyBinding(binding);if(report.pdfInspectorPython)assert.equal(fileHash(options.python),report.pdfInspectorPython.sha256,'Independent PDF reader runtime changed');report.finalCandidateBindingVerified=true;}catch(error){report.passed=false;report.bindingError=String(error.message);}
    report.finishedUtc=new Date().toISOString();retain('report.json',report);
  }
  console.log(JSON.stringify({evidence:path.relative(ROOT,evidence),passed:report.passed,checks:report.checks.length,outputs:report.outputs.length,error:report.error,transportFatal:report.transportFatal}));if(!report.passed)process.exitCode=1;
}
if(require.main===module)main().catch(error=>{console.error(error.stack);process.exitCode=1;});
module.exports={argumentsFor,inside,canonical,columnAlignments,assertTables,assertFunctions,assertScriptedFunction,errorPayload,exportRequestFor,assertToolCallSchema,systemEnvironment,StdioClient,planFor,verifyBinding,matrixXmlAlignments,positiveTables,negativeTables,invalidFunctions,projectedMathmlFunctions,invalidMathmlFunctions,TABLE,UNICODE,UNICODE_CHANGED,BASE,ROOT};
