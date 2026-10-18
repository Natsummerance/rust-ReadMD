// Opt-in real relay comparison. Isolated settings and synthetic prose only.
// Never prints configuration, credentials, request headers or provider bodies.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const {startUiServer}=require('./ui-server.cjs');
const output=path.resolve(process.env.READMD_AI_SPEED_ROOT||'.cache/v005-completion-speed/real');
fs.mkdirSync(output,{recursive:true});
const source=process.env.READMD_AI_SOURCE_CONFIG,raw=fs.readFileSync(source,'utf8'),config=JSON.parse(raw);
const digest=text=>crypto.createHash('sha256').update(text).digest('hex');
const report={syntheticOnly:true,credentialsLogged:false,originalConfigHash:digest(raw),cases:[],models:[],errors:[]};
const samples=[{name:'Chinese research',prefix:'# 证据与推理\n\n好的研究从观察开始。为了避免主观偏见，我们应该',suffix:'\n\n## 结论\n观点需要接受证据检验。'},
 {name:'English writing',prefix:'# Research notes\n\nA convincing explanation begins with evidence. To test a claim, we should ',suffix:'\n\n## Next steps\nRecord the result and unresolved questions.'}];

async function run(binary,label,port){
 const data=path.join(output,label);fs.mkdirSync(data,{recursive:true});fs.writeFileSync(path.join(data,'ai.json'),JSON.stringify(config));
 process.env.READMD_BIN=path.resolve(binary);process.env.READMD_DATA_DIR=data;let server;
 try{
  server=await startUiServer(port,{stdio:'ignore'});const base='http://127.0.0.1:'+port;
  const html=await(await fetch(base)).text(),token=html.match(/name="readmd-app-token"\s+content="([^"]+)"/)?.[1];assert.ok(token);
  async function api(route,body,stream=false){
   const started=performance.now(),response=await fetch(base+route,{method:'POST',headers:{'Content-Type':'application/json','X-ReadMD-App-Token':token,Accept:stream?'text/event-stream':'application/json'},body:JSON.stringify(body),signal:AbortSignal.timeout(40000)});
   let result,firstSuggestionMs=null,frames=0;const attemptedModels=[];const streaming=(response.headers.get('content-type')||'').includes('text/event-stream');
   if(streaming){
    const reader=response.body.getReader(),decoder=new TextDecoder();let buffer='';
    for(;;){const {value,done}=await reader.read();buffer+=decoder.decode(value||new Uint8Array(),{stream:!done});let end;
     while((end=buffer.indexOf('\n'))>=0){const line=buffer.slice(0,end).trim();buffer=buffer.slice(end+1);if(!line.startsWith('data:'))continue;
      const frame=JSON.parse(line.slice(5));if(frame.model&&!attemptedModels.includes(frame.model))attemptedModels.push(frame.model);
      if(frame.type==='suggestion'&&frame.completion){frames++;firstSuggestionMs??=performance.now()-started;}
      if(frame.type==='done'||frame.type==='error')result=frame;
     }if(done)break;
    }
   }else result=await response.json();
   return {status:response.status,result,elapsedMs:Math.round(performance.now()-started),firstSuggestionMs:firstSuggestionMs===null?null:Math.round(firstSuggestionMs),frames,streaming,attemptedModels};
  }
  if(label==='candidate'){
   const discovered=await api('/api/ai/models',{});assert.equal(discovered.status,200);report.models=discovered.result.models;
  }
  const current=config.current.model;
  const modes=label==='baseline'?[['same-model baseline',current]]:process.env.READMD_AI_AUTO_ONLY==='1'?[['automatic',undefined]]:[['automatic',undefined],['same-model optimized',current],['fixed fast model',report.models.find(m=>m==='claude-sonnet-4-6')||report.models.find(m=>/claude.*sonnet/i.test(m))]];
  let cacheCase;
  for(const [mode,model]of modes){
   if(mode==='fixed fast model'&&!model)continue;
   for(const sample of samples){
    const body={prefix:sample.prefix,suffix:sample.suffix,...(model?{model}:{})};
    try{
     const value=await api('/api/ai/complete',body,label==='candidate'),text=value.result?.completion||'';
     const valid=value.status===200&&value.result?.ok===true&&text.trim().length>=4&&!/<think>|```|retired|no longer available/i.test(text)&&!text.includes(sample.suffix.trim());
     report.cases.push({build:label,mode,sample:sample.name,model:value.result?.model||model,passed:valid,elapsedMs:value.elapsedMs,firstSuggestionMs:value.firstSuggestionMs,streaming:value.streaming,frames:value.frames,attemptedModels:value.attemptedModels,cached:!!value.result?.cached,completion:valid?text:undefined,failure:valid?undefined:'provider-or-completion-failed',errorCode:valid?undefined:(/^[a-z0-9_]{1,64}$/.test(value.result?.error_code||'')?value.result.error_code:'provider-error')});
     if(valid&&label==='candidate')cacheCase={...body,model:value.result.model};
    }catch(error){report.cases.push({build:label,mode,sample:sample.name,model,passed:false,failure:error.name==='TimeoutError'?'deadline-exceeded':'request-failed'});}
    console.log(JSON.stringify({...report.cases.at(-1),completion:undefined}));
   }
  }
  if(cacheCase){const replay=await api('/api/ai/complete',cacheCase,true);report.cache={passed:replay.result?.cached===true,elapsedMs:replay.elapsedMs,model:replay.result?.model};assert.equal(report.cache.passed,true);}
 }finally{if(server)await server.stop();fs.rmSync(path.join(data,'ai.json'),{force:true});}
}
(async()=>{
 try{
  if(process.env.READMD_AI_BASELINE_REPORT){const previous=JSON.parse(fs.readFileSync(process.env.READMD_AI_BASELINE_REPORT,'utf8'));assert.equal(previous.originalConfigHash,report.originalConfigHash);report.cases.push(...previous.cases.filter(c=>c.build==='baseline'));report.baselineReused=true;}
  else await run(process.env.READMD_AI_BASELINE_BIN,'baseline',28871);
  await run(process.env.READMD_AI_CANDIDATE_BIN,'candidate',28872);
  report.originalConfigUnchanged=digest(fs.readFileSync(source,'utf8'))===report.originalConfigHash;
  assert.equal(report.originalConfigUnchanged,true);
  report.passed=report.cases.filter(c=>c.build==='candidate').every(c=>c.passed)&&report.cache?.passed===true;
  if(!report.passed)process.exitCode=1;
 }catch(error){report.passed=false;report.errors.push(error.name+': benchmark-incomplete');process.exitCode=1;}
 finally{fs.writeFileSync(path.join(output,'report.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,cases:report.cases.length,cache:report.cache,originalConfigUnchanged:report.originalConfigUnchanged}));}
})();
