'use strict';
// Real stdio MCP integration, synthetic local inputs, no network or API mocks.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const crypto=require('node:crypto'),{spawn}=require('node:child_process');
assert(process.env.READMD_BIN,'READMD_BIN must identify the unified prebuilt candidate');
const root=path.resolve(__dirname,'../..');
const data=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/publication-locale-mcp-'));
const docs=path.join(data,'documents');fs.mkdirSync(docs);
const source='---\ntitle: Evidence and interpretation\nlanguage: de_DE\nbibliography: references.json\ncsl: dates.csl\n---\n# Evidence\n\nSource [@study].\n\nPreserve the author’s original declaration.\n';
const files={
  'Research.md':source,
  'references.json':JSON.stringify([{id:'study',type:'book',title:'Evidence',author:[{family:'Researcher',given:'Ada'}],issued:{'date-parts':[[2026,1,13]]}}]),
  'dates.csl':'<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="in-text"><info><title>Locale test</title><id>urn:readmd:test:mcp-locale</id><updated>2026-10-13T00:00:00Z</updated></info><citation><layout><date variable="issued"><date-part name="month" form="long"/></date></layout></citation></style>',
};
for(const[name,value]of Object.entries(files))fs.writeFileSync(path.join(docs,name),value);
const hash=file=>crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const initial=Object.fromEntries(Object.keys(files).map(name=>[name,hash(path.join(docs,name))]));
const report={passed:false,checks:[],synthetic:true,transport:'stdio',platform:process.platform,binarySha256:hash(process.env.READMD_BIN),scriptSha256:hash(__filename),inputs:initial};
const child=spawn(process.env.READMD_BIN,['--mcp','--data-dir',data,'--workspace',docs,'--assets',path.join(root,'assets')],{windowsHide:true,stdio:['pipe','pipe','ignore']});
let next=1,buffer='',stage='startup';const pending=new Map();
function failPending(reason){for(const call of pending.values()){clearTimeout(call.timer);call.reject(Error(reason));}pending.clear();}
child.stdout.setEncoding('utf8');child.stdout.on('data',chunk=>{
  buffer+=chunk;if(Buffer.byteLength(buffer)>2*1024*1024){failPending('MCP output budget exceeded');child.kill();return;}
  for(;;){const at=buffer.indexOf('\n');if(at<0)break;const line=buffer.slice(0,at);buffer=buffer.slice(at+1);if(!line.trim())continue;
    let message;try{message=JSON.parse(line);}catch{failPending('MCP emitted invalid JSON');child.kill();return;}
    const call=pending.get(message.id);if(!call)continue;clearTimeout(call.timer);pending.delete(message.id);
    message.error?call.reject(Error('MCP protocol error')):call.resolve(message.result);
  }
});
child.on('error',()=>failPending('MCP process launch failed'));
child.stdin.on('error',()=>failPending('MCP input stream closed'));
child.on('exit',()=>failPending('MCP process exited before replying'));
function request(method,params){return new Promise((resolve,reject)=>{
  const id=next++,timer=setTimeout(()=>{pending.delete(id);reject(Error('MCP request timed out: '+method));},30000);
  pending.set(id,{resolve,reject,timer});child.stdin.write(JSON.stringify({jsonrpc:'2.0',id,method,params})+'\n',error=>{if(error){clearTimeout(timer);pending.delete(id);reject(Error('MCP request write failed'));}});
});}
async function call(options){
  const result=await request('tools/call',{name:'readmd_academic_document',arguments:{content:source,baseDir:docs,...(options===undefined?{}:{options})}});
  assert.notEqual(result.isError,true,'MCP tool returned an error');
  return result.structuredContent||JSON.parse(result.content.find(item=>item.type==='text').text);
}
function unchanged(){
  assert.deepEqual(fs.readdirSync(docs).sort(),Object.keys(files).sort(),'A read-only request created a source-side file');
  for(const[name,value]of Object.entries(files)){
    assert.equal(fs.readFileSync(path.join(docs,name),'utf8'),value,'Input content changed: '+name);
    assert.equal(hash(path.join(docs,name)),initial[name],'Input byte hash changed: '+name);
  }
}
async function stop(){
  failPending('MCP session closing');child.stdin.end();
  if(child.exitCode!==null||child.signalCode!==null){report.processStopped=true;return;}
  const exited=new Promise(resolve=>child.once('exit',resolve));
  await Promise.race([exited,new Promise(resolve=>setTimeout(resolve,1500))]);
  if(child.exitCode===null&&child.signalCode===null){child.kill();await Promise.race([exited,new Promise(resolve=>setTimeout(resolve,3000))]);}
  report.processStopped=child.exitCode!==null||child.signalCode!==null;
}
(async()=>{try{
  stage='discovery';await request('initialize',{protocolVersion:'2024-11-05',capabilities:{},clientInfo:{name:'readmd-publication-locale-qa',version:'1'}});
  child.stdin.write(JSON.stringify({jsonrpc:'2.0',method:'notifications/initialized'})+'\n');
  const listed=await request('tools/list',{}),tool=listed.tools.find(t=>t.name==='readmd_academic_document');
  assert(tool,'Shared publication tool not listed');assert.equal(tool.annotations.readOnlyHint,true);assert.equal(tool.inputSchema.properties.options.type,'object');
  assert(tool.inputSchema.required.includes('content')&&tool.inputSchema.required.includes('baseDir'));
  report.checks.push('Real MCP initialization and tools/list expose optional publication options and the read-only shared document service.');
  for(const[options,locale,month,label]of [[undefined,'de-DE','Januar','source'],[{meta:{language:'fr_CA'}},'fr-CA','janvier','override'],[{language:'ar',meta:{language:'invalid--tag'}},'de-DE','Januar','invalid-override']]){
    stage=label;const result=await call(options);assert.equal(result.ok,true);assert.equal(result.active,true);assert.equal(result.locale,locale);
    assert(result.content.includes(month),'Actual CSL month does not match the effective language');assert(result.content.includes('language: de_DE'),'Publication view rewrote source metadata');
    assert.deepEqual(result.diagnostics,[]);assert.equal(result.source_citations.length,1);const cite=result.source_citations[0];
    assert.equal(source.slice(cite.from,cite.to),'[@study]');assert.equal(cite.source,'[@study]');unchanged();
    report.checks.push(label+': real CSL returns '+month+' with '+locale+', retaining original source mapping and input bytes.');
  }
  stage='immutability';unchanged();
  report.checks.push('Repeated read-only MCP requests leave Markdown, bibliography and CSL byte-identical and create no side files in the input folder.');
  report.passed=true;
}catch(error){report.failedStage=stage;report.error=String(error.stack||error.message).replaceAll(root,'<workspace>').replaceAll(data,'<fixture>').slice(0,2200);process.exitCode=1;}
finally{await stop();if(report.processStopped===false){report.passed=false;report.shutdownError='Owned MCP process did not stop within the shutdown budget';process.exitCode=1;}report.recordedAt=new Date().toISOString();report.scope='Limited shared publication-language behavior over real stdio MCP. This does not certify all CSL styles, complex-script layout, VS Code integration, native Word or independent EPUB reading applications.';fs.writeFileSync(path.join(data,'report.json'),JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,report:path.relative(root,path.join(data,'report.json')),failedStage:report.failedStage}));}
})();
