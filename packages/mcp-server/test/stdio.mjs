// Real executable/stdio integration. All files and AI requests are synthetic.
import fs from 'node:fs';
import path from 'node:path';
import http from 'node:http';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { once } from 'node:events';
const binary = process.env.READMD_BIN, root = process.env.READMD_MCP_TEST_ROOT;
assert.ok(binary && root && path.isAbsolute(root), 'Set READMD_BIN and an isolated READMD_MCP_TEST_ROOT');
const data = path.join(root, 'data'); fs.mkdirSync(data, {recursive:true});
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
const proc = spawn(binary, ['--mcp'], {windowsHide:true,env:{...process.env,READMD_DATA_DIR:data}});
const pending = new Map(), responses = [];
let id = 0;
createInterface({input:proc.stdout}).on('line', line => {
  const message = JSON.parse(line); responses.push(message);
  if (!Object.hasOwn(message, 'id')) return;
  const waiter = pending.get(message.id); if (!waiter) return;
  pending.delete(message.id); clearTimeout(waiter.timer); waiter.resolve(message);
});
proc.stderr.resume();
proc.on('error', error => { for(const waiter of pending.values()) waiter.reject(error); });
const send = value => proc.stdin.write(JSON.stringify(value)+'\n');
const call = (method,params={}) => new Promise((resolve,reject) => {
  const requestId=++id;
  const timer=setTimeout(()=>{pending.delete(requestId);reject(Error('MCP response timeout: '+method));},30000);
  pending.set(requestId,{resolve,reject,timer}); send({jsonrpc:'2.0',id:requestId,method,params});
});
const tool = (name,args) => call('tools/call',{name,arguments:args});
const unwrap = message => { assert.ok(!message.error,JSON.stringify(message.error)); assert.ok(!message.result.isError,JSON.stringify(message.result.structuredContent)); return message.result.structuredContent || message.result.content[0].text; };
const report = {ok:false,checks:[]};
try {
  const meta={'io.modelcontextprotocol/protocolVersion':'2026-07-28','io.modelcontextprotocol/clientCapabilities':{}};
  const discovery=await call('server/discover',{_meta:meta}); assert.equal(discovery.result.resultType,'complete'); report.checks.push('modern discovery');
  const hello=await call('initialize',{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'readmd-fixture',version:'1'}}); assert.equal(hello.result.protocolVersion,'2025-11-25');
  send({jsonrpc:'2.0',method:'notifications/initialized'}); report.checks.push('legacy handshake');
  const catalog=(await call('tools/list')).result.tools; assert.equal(catalog.length,22); report.tools=catalog.length;
  const presets=unwrap(await tool('readmd_export_presets',{})); assert.ok(presets.presets.classic); report.checks.push('real export preset catalog');
  const models=unwrap(await tool('readmd_ai_models',{provider:'custom:fixture',confirm:true})); assert.ok(models.models.includes('fixture-model')); report.checks.push('model discovery over loopback HTTP');
  const aiArgs={provider:'custom:fixture',model:'fixture-model',skill_id:'readmd-summary',markdown_content:'# Synthetic source'};
  assert.equal(unwrap(await tool('readmd_ai_chat',aiArgs)).content,'# Synthetic AI result'); report.checks.push('structured AI result');
  const diagram=unwrap(await tool('readmd_render_diagram',{engine:'wsd',code:'A -> B: hello',confirm:true})); assert.match(diagram.svg,/<svg/); report.checks.push('native diagram rendering');
  const output=path.join(root,'export.html'); fs.writeFileSync(output,'KEEP');
  const args={markdown_content:'# Export acceptance',output_path:output,output_format:'html',confirm:true};
  assert.equal((await tool('readmd_export_document',args)).result.structuredContent.error_code,'output_exists'); assert.equal(fs.readFileSync(output,'utf8'),'KEEP');
  unwrap(await tool('readmd_export_document',{...args,overwrite:true,style_preset:'classic'})); assert.match(fs.readFileSync(output,'utf8'),/Export acceptance/); report.checks.push('protected and atomic HTML replacement');
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
  proc.stdin.write('x'.repeat(32*1024*1024+1)+'\n'); await call('ping');
  assert.ok(responses.some(response=>response.id===null && response.error?.code===-32600)); report.checks.push('oversized input drains and recovers');
  report.ok=true; fs.writeFileSync(path.join(root,'stdio-result.json'),JSON.stringify(report,null,2)); console.log(JSON.stringify(report));
} finally {
  for(const waiter of pending.values()) clearTimeout(waiter.timer);
  proc.stdin.end(); await Promise.race([once(proc,'exit'),new Promise(resolve=>setTimeout(()=>{proc.kill();resolve();},2000))]);
  server.close(); server.closeAllConnections();
}
