'use strict';
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto'),{spawn}=require('node:child_process');
const root=process.cwd(),input=process.argv[2];assert(input&&input.startsWith('.cache/capability-upgrade/math-controls-native-'));assert(process.env.READMD_BIN);
const file=path.join(input,'migrated.md'),original=fs.readFileSync(file,'utf8'),sources=[...original.matchAll(/```readmd-math v1\r?\n([\s\S]*?)\r?\n```/g)].map(m=>m[1]);assert.equal(sources.length,5);
const data=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/math-controls-mcp-')),sha=f=>crypto.createHash('sha256').update(fs.readFileSync(f)).digest('hex'),report={passed:false,checks:[],synthetic:true,transport:'stdio',inputSha256:sha(file),binarySha256:sha(process.env.READMD_BIN),scriptSha256:sha(__filename)};
const child=spawn(process.env.READMD_BIN,['--mcp','--data-dir',data,'--workspace',data,'--assets',path.join(root,'assets')],{windowsHide:true,stdio:['pipe','pipe','ignore']});let next=1,buffer='';const pending=new Map();
child.stdout.setEncoding('utf8');child.stdout.on('data',chunk=>{buffer+=chunk;if(buffer.length>2*1024*1024){for(const p of pending.values())p.reject(Error('Output budget'));child.kill();return;}for(;;){const at=buffer.indexOf('\n');if(at<0)break;const line=buffer.slice(0,at);buffer=buffer.slice(at+1);if(!line.trim())continue;const message=JSON.parse(line),p=pending.get(message.id);if(p){clearTimeout(p.timer);pending.delete(message.id);message.error?p.reject(Error('MCP protocol error')):p.resolve(message.result);}}});
child.on('error',()=>{for(const p of pending.values())p.reject(Error('MCP launch failed'));});
function request(method,params){return new Promise((resolve,reject)=>{const id=next++,timer=setTimeout(()=>{pending.delete(id);reject(Error('MCP timeout: '+method));},15000);pending.set(id,{resolve,reject,timer});child.stdin.write(JSON.stringify({jsonrpc:'2.0',id,method,params})+'\n');});}
async function call(args){const result=await request('tools/call',{name:'readmd_math',arguments:args});return result.structuredContent||JSON.parse(result.content[0].text);}
function paint(svg){return[...svg.matchAll(/<(?:path|rect)\b[^>]*>/g)].map(m=>m[0]).filter(s=>!s.includes('fill="transparent"'));}
(async()=>{try{
 await request('initialize',{protocolVersion:'2024-11-05',capabilities:{},clientInfo:{name:'readmd-control-qa',version:'1'}});child.stdin.write(JSON.stringify({jsonrpc:'2.0',method:'notifications/initialized'})+'\n');const tools=await request('tools/list',{});assert(tools.tools.some(t=>t.name==='readmd_math'&&t.annotations.readOnlyHint));
 const results=[];for(const [i,mode] of [[0,'Blue'],[2,'Red'],[3,'Auto']]){
  const parsed=await call({operation:'parse',source:sources[i],dialect:'dsl',display:true});assert.equal(parsed.ok,true);const items=paint(parsed.vector_preview),rects=items.filter(s=>s.startsWith('<rect'));assert.equal(rects.length,2);
  if(mode==='Blue')assert(rects.every(s=>s.includes('fill="#336699"')));if(mode==='Red')assert(rects.every(s=>s.includes('fill="#CC3366"')));if(mode==='Auto')assert(rects.every(s=>!s.includes('fill=')));
  assert(items.some(s=>s.startsWith('<path')&&s.includes('fill="#336699"')));if(mode==='Red')assert(items.filter(s=>s.startsWith('<path')&&s.includes('fill="#CC3366"')).length>=2);results.push({i,mode,parsed,items});
  const color=mode==='Red'?'#CC3366':mode==='Auto'?'CanvasText':'#336699',points=mode==='Blue'?16:24;
  fs.writeFileSync(path.join(data,mode+'.mathml'),parsed.preview);
  assert(new RegExp(`<mstyle mathcolor="${color}" mathsize="${points}pt">(?:<mrow id="[^"]*">)?<mfrac>`).test(parsed.preview));
  assert(parsed.preview.includes('mathcolor="#336699" mathsize="16pt"'));
 }
 report.checks.push('Real stdio discovers the shared read-only math service and independently renders inherited blue, explicit red, and automatic control paint; operands stay blue.');
 report.checks.push('The same MCP service also projects independent control and operand colors/physical point sizes into MathML, including automatic theme color.');
 for(const {i,mode,items} of results){for(const target of ['dsl','omml']){const serialized=await call({operation:'serialize',source:sources[i],dialect:'dsl',display:true,target});assert.equal(serialized.ok,true);const reopened=await call({operation:'parse',source:serialized.output,dialect:target,display:true});assert.equal(reopened.ok,true);assert.deepEqual(paint(reopened.vector_preview),items,mode+' '+target);}}
 report.checks.push('Actual readable DSL and lossless internal OMML serialize/reopen preserve every painted path, rule, physical coordinate and independent control color.');
 assert.equal(fs.readFileSync(file,'utf8'),original);assert.equal(sha(file),report.inputSha256);assert(!fs.readdirSync(data).some(n=>n.endsWith('.md')));report.checks.push('The source document is never modified or copied by the MCP operations.');report.passed=true;
}catch(e){report.error=String(e.stack||e.message).replaceAll(root,'<workspace>').slice(0,1800);process.exitCode=1;}
finally{for(const p of pending.values())clearTimeout(p.timer);child.stdin.end();if(child.exitCode===null)child.kill();report.recordedAt=new Date().toISOString();report.scope='Actual stdio independent control/run format in vector and MathML, and exact vector DSL/OMML round-trip; UI editing is independently native-tested. Full Office font/theme semantics, Word application, VS Code and complete B3 remain unverified.';fs.writeFileSync(path.join(data,'report.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,report:path.relative(root,path.join(data,'report.json'))}));}
})();
