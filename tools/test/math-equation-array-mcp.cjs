'use strict';
// Actual stdio integration. Synthetic inputs only; no network or API mocks.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {spawn}=require('node:child_process');
const root=path.resolve(__dirname,'../..'),sha=b=>crypto.createHash('sha256').update(b).digest('hex');
assert(process.env.READMD_BIN&&process.env.READMD_CANDIDATE_SHA256,'Explicit candidate required');
assert.equal(sha(fs.readFileSync(process.env.READMD_BIN)),process.env.READMD_CANDIDATE_SHA256);
const data=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/math-equation-array-mcp-'));
const docs=path.join(data,'documents');fs.mkdirSync(docs);
const source='<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:eqArr><m:eqArrPr><m:maxDist m:val="1"/><m:objDist m:val="1"/></m:eqArrPr><m:e><m:r><m:rPr><m:sty m:val="p"/></m:rPr><m:t>a&amp;b&amp;x&amp;y</m:t></m:r></m:e><m:e><m:r><m:rPr><m:sty m:val="p"/></m:rPr><m:t>x&amp;y&amp;b&amp;a</m:t></m:r></m:e></m:eqArr></m:oMath>';
const file=path.join(docs,'original.omml');fs.writeFileSync(file,source,{flag:'wx'});
const report={passed:false,synthetic:true,transport:'stdio',checks:[],requests:[],candidateSha256:process.env.READMD_CANDIDATE_SHA256,scriptSha256:sha(fs.readFileSync(__filename)),inputSha256:sha(Buffer.from(source)),automaticRetries:0};
const child=spawn(process.env.READMD_BIN,['--mcp','--data-dir',data,'--workspace',docs,'--assets',path.join(root,'assets')],{windowsHide:true,stdio:['pipe','pipe','ignore']});
let buffer='',next=1,stage='initialization';const pending=new Map();
function failPending(reason){for(const call of pending.values()){clearTimeout(call.timer);call.reject(Error(reason));}pending.clear();}
child.stdout.setEncoding('utf8');child.stdout.on('data',chunk=>{
  buffer+=chunk;if(Buffer.byteLength(buffer)>4*1024*1024){failPending('MCP response budget exceeded');child.kill();return;}
  for(;;){const end=buffer.indexOf('\n');if(end<0)break;const line=buffer.slice(0,end);buffer=buffer.slice(end+1);if(!line.trim())continue;
    let message;try{message=JSON.parse(line);}catch{failPending('Invalid stdio JSON');child.kill();return;}
    const call=pending.get(message.id);if(!call)continue;clearTimeout(call.timer);pending.delete(message.id);
    message.error?call.reject(Error('MCP protocol error')):call.resolve(message.result);
  }
});
child.on('error',()=>failPending('Owned MCP launch failed'));child.on('exit',()=>failPending('Owned MCP exited'));child.stdin.on('error',()=>failPending('Owned MCP input closed'));
function request(method,params){return new Promise((resolve,reject)=>{const id=next++,timer=setTimeout(()=>{pending.delete(id);reject(Error('MCP timeout '+method));},30000);pending.set(id,{resolve,reject,timer});child.stdin.write(JSON.stringify({jsonrpc:'2.0',id,method,params})+'\n');});}
async function math(args){const raw=await request('tools/call',{name:'readmd_math',arguments:args});const value=raw.structuredContent||JSON.parse(raw.content.find(x=>x.type==='text').text);report.requests.push({operation:args.operation,fontSize:args.font_size,layoutWidth:args.layout_width,ok:value.ok,errorCode:value.error_code,responseSha256:sha(Buffer.from(JSON.stringify(raw)))});return value;}
const near=(a,b)=>assert(Number.isFinite(a)&&Math.abs(a-b)<.00003,`${a} != ${b}`);
function all(n){return[n,...n.content.filter(c=>c.type==='element').flatMap(c=>all(c.value))];}
async function stop(){failPending('Closing test');child.stdin.end();const exited=new Promise(resolve=>child.once('exit',resolve));if(child.exitCode===null&&child.signalCode===null)await Promise.race([exited,new Promise(resolve=>setTimeout(resolve,1500))]);if(child.exitCode===null&&child.signalCode===null){report.forcedStop=true;child.kill();await Promise.race([exited,new Promise(resolve=>setTimeout(resolve,3000))]);}report.processStopped=child.exitCode!==null||child.signalCode!==null;}
(async()=>{try{
  await request('initialize',{protocolVersion:'2024-11-05',capabilities:{},clientInfo:{name:'readmd-equation-array-qa',version:'1'}});child.stdin.write(JSON.stringify({jsonrpc:'2.0',method:'notifications/initialized'})+'\n');
  stage='advertised shared layout parameters';const listing=await request('tools/list',{}),tool=listing.tools.find(t=>t.name==='readmd_math');assert(tool?.annotations.readOnlyHint);
  const props=tool.inputSchema.properties;
  assert.deepEqual([props.font_size?.type,props.font_size?.minimum,props.font_size?.maximum],['number',4,512]);
  assert.deepEqual([props.layout_width?.type,props.layout_width?.minimum,props.layout_width?.maximum],['number',1,20000]);
  report.checks.push('Real discovery advertises bounded physical point parameters.');
  stage='actual layout and source';let original;
  for(const size of [12,24]){
    const parsed=await math({operation:'parse',source,dialect:'omml',display:true,font_size:size,layout_width:10});assert.equal(parsed.ok,true);assert.equal(parsed.document.source,source);assert.equal(parsed.layout_diagnostics.length,1);
    const overflow=parsed.layout_diagnostics[0];assert.equal(overflow.code,'math_equation_array_overflow');near(overflow.required_points,24.084*size/12);near(overflow.available_points,10);assert(parsed.vector_preview.includes('<svg'));if(size===12)original=parsed;
  }
  report.checks.push('Real native shared layout reports independent 12/24pt widths without rewriting source.');
  stage='patch, stale and invalid parameters';const node=all(original.document.root).find(n=>n.name.split(':').at(-1)==='eqArr');assert(node);
  const args={operation:'patch',source,dialect:'omml',display:true,font_size:12,layout_width:96,expected_revision:original.document.revision,patch:{operation:'set_property',node_id:node.id,container:'eqArrPr',property:'objDist',value:'false'}};
  const patched=await math(args);assert.equal(patched.ok,true);assert.notEqual(patched.document.source,source);assert.deepEqual(patched.layout_diagnostics,[]);
  const stale=await math({...args,source:patched.document.source});assert.equal(stale.ok,false);assert.equal(stale.error_code,'transaction_conflict');
  for(const extra of [{font_size:0},{font_size:513},{font_size:'12'},{layout_width:0},{layout_width:20001},{layout_width:'96'},{layout_width:null}]){const failed=await math({operation:'parse',source,dialect:'omml',...extra});assert.equal(failed.ok,false);assert.equal(failed.error_code,'invalid_arguments');}
  report.checks.push('Real MCP revision guard and strict types/ranges reject stale or invalid operations.');
  stage='serialization and immutability';const output=await math({operation:'serialize',source:patched.document.source,dialect:'omml',target:'mathml',display:true});assert.equal(output.ok,true);assert(output.output.includes('<mtable'));assert.equal(fs.readFileSync(file,'utf8'),source);assert.deepEqual(fs.readdirSync(docs),['original.omml']);
  report.checks.push('Same service serializes the edited equation and leaves input directory untouched.');report.passed=true;
}catch(error){report.failedStage=stage;report.error=String(error.stack||error).replaceAll(root,'<workspace>').slice(0,2200);process.exitCode=1;}
finally{await stop();if(!report.processStopped){report.passed=false;process.exitCode=1;}report.recordedAt=new Date().toISOString();report.scope='Limited real MCP equation-array parameters, layout, revision and serialization; no VS Code host, Word, native file saving or full math certification.';const target=path.join(data,'report.json');fs.writeFileSync(target,JSON.stringify(report,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,report:path.relative(root,target),sha256:sha(fs.readFileSync(target)),failedStage:report.failedStage}));}
})();
