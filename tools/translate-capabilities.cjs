'use strict';
// Explicitly configured provider only. Credentials never appear in stdout or
// translation artifacts. Existing native translations are preserved.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {chromium}=require('playwright'),{startNative}=require('../ui-tests/native-session.cjs');
const root=path.resolve(__dirname,'..'),specFile=path.resolve(root,process.env.READMD_TRANSLATION_SPEC||'tools/capability-translations.json'),base=path.join(root,'.cache/capability-upgrade/translations',process.env.READMD_TRANSLATION_SPEC?path.basename(specFile,'.json'):'');fs.mkdirSync(base,{recursive:true});
const spec=JSON.parse(fs.readFileSync(specFile)),meta=JSON.parse(fs.readFileSync(path.join(root,'assets/i18n/meta.json'))),english=JSON.parse(fs.readFileSync(path.join(root,'assets/i18n/en.json')));
const refresh=new Set((process.env.READMD_TRANSLATION_REFRESH||'').split(',').filter(Boolean));
if(!process.env.READMD_TRANSLATION_SPEC)for(const [key,value]of Object.entries(english))if(key.startsWith('formula.'))spec[key]||={en:value};
for(const locale of Object.keys(meta)){const existing=JSON.parse(fs.readFileSync(path.join(root,'assets/i18n',locale+'.json')));for(const[key,texts]of Object.entries(spec)){if(!refresh.has(key)&&(locale==='en'||existing[key]&&existing[key]!==texts.en))texts[locale]??=existing[key];}}
const output=path.join(base,'translated.json');if(fs.existsSync(output)){const saved=JSON.parse(fs.readFileSync(output));for(const[k,v]of Object.entries(saved))if(spec[k]&&!refresh.has(k))Object.assign(spec[k],v);}
const data=fs.mkdtempSync(path.join(base,'native-'));const sourceConfig=JSON.parse(fs.readFileSync(process.env.READMD_AI_SOURCE_CONFIG));
// Keep this long-running service isolated from the executable being rebuilt.
// Windows otherwise locks the linker's output for the entire translation run.
const serviceBinary=path.join(data,'readmd-translation-service.exe');fs.copyFileSync(process.env.READMD_BIN,serviceBinary);process.env.READMD_BIN=serviceBinary;
const relay=sourceConfig.providers?.find(p=>/antigravity/i.test(p.name||p.label||''));if(relay)sourceConfig.current.provider_id=relay.id;
fs.writeFileSync(path.join(data,'ai.json'),JSON.stringify(sourceConfig));
let native,phase='startup';
(async()=>{
  try{
    native=await startNative(root,data,chromium,28853);const page=native.page;
    const api=async(route,body)=>page.evaluate(async({route,body})=>{const r=await apiFetch(route,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body),signal:AbortSignal.timeout(90000)});return {status:r.status,data:await r.json()};},{route,body});
    phase='model discovery';const models=await api('/api/ai/models',{});assert.equal(models.status,200,'Provider model discovery failed');
    let model;
    const modelCache=path.join(base,'last-working-model.json');let preferred=sourceConfig.current?.model;try{preferred=JSON.parse(fs.readFileSync(modelCache)).model||preferred;}catch{}
    const candidates=models.data.models.filter(m=>/gemini.*flash/i.test(m)).sort((a,b)=>a===preferred?-1:b===preferred?1:b.localeCompare(a,undefined,{numeric:true}));
    for(const candidate of candidates){console.log('Checking an available Flash model with a real request');const probe=await api('/api/ai/chat',{model:candidate,stream:false,messages:[{role:'user',content:'Reply exactly READMD_OK.'}],session:'readmd-live-model-check'});if(probe.status===200&&probe.data.content?.trim()==='READMD_OK'){model=candidate;break;}}
    assert.ok(model,'No Flash model passed an actual request');
    fs.writeFileSync(modelCache,JSON.stringify({model,checkedAt:new Date().toISOString()}));
    const config=JSON.parse(fs.readFileSync(path.join(data,'ai.json')));config.current.model=model;fs.writeFileSync(path.join(data,'ai.json'),JSON.stringify(config));
    const locales=Object.keys(meta).filter(l=>l!=='en'&&Object.values(spec).some(texts=>!texts[l]));
    const batchSize=Math.max(1,Math.min(8,Number(process.env.READMD_TRANSLATION_BATCH)||1));
    for(let offset=0;offset<locales.length;offset+=batchSize){
      const requested=locales.slice(offset,offset+batchSize);
      for(let round=0;round<4;round++){
      const batch=requested.filter(locale=>Object.values(spec).some(texts=>!texts[locale])),missing={};if(!batch.length)break;
      phase='translate '+batch.join(',');
      for(const locale of batch)missing[locale]=Object.fromEntries(Object.entries(spec).filter(([k,texts])=>!texts[locale]).map(([k,texts])=>[k,texts.en]));
      const prompt='Translate the values of each locale object into its target language: '+batch.map(l=>l+' = '+meta[l].native).join('; ')+'. These are concise native UI labels in a Markdown document editor. Translate all ordinary English; preserve LaTeX, XML technical names and all {placeholder} names exactly. Do not add explanations. Return ONLY one JSON object with precisely the same locale objects and keys. Source:\n'+JSON.stringify(missing);
      let result;try{result=await api('/api/ai/chat',{model,stream:false,messages:[{role:'user',content:prompt}],session:'readmd-ui-translations-'+offset+'-'+round,max_tokens:16384});}catch(_){console.log(batch.join(',')+': request interrupted; validated entries retained');continue;}
      if([429,502,503,504].includes(result.status)){console.log(batch.join(',')+': transient provider response; requesting remaining entries again');continue;}
      assert.equal(result.status,200,'Translation request failed');
      const raw=result.data.content?.trim().replace(/^```(?:json)?\s*/,'').replace(/\s*```$/,'');fs.writeFileSync(path.join(base,'last-response.txt'),raw||'');let translated;try{translated=JSON.parse(raw);}catch(error){if(error instanceof SyntaxError){console.log(batch.join(',')+': malformed JSON, requesting remaining entries again');continue;}throw error;}
      assert.deepEqual(Object.keys(translated).sort(),[...batch].sort(),'locale coverage');
      for(const locale of batch){assert.ok(translated[locale]&&Object.keys(translated[locale]).length,'Translation made no progress');
        const seen=new Set();for(const[rawKey,value]of Object.entries(translated[locale])){const key=rawKey.trim();assert.ok(Object.hasOwn(missing[locale],key),'Unexpected translation key');assert.ok(!seen.has(key),'Duplicate translation key');seen.add(key);assert.equal(typeof value,'string');assert.ok(value.trim());assert.deepEqual((value.match(/\{[a-zA-Z][\w]*\}/g)||[]).sort(),(missing[locale][key].match(/\{[a-zA-Z][\w]*\}/g)||[]).sort(),locale+' placeholders: '+key);spec[key][locale]=value;}
      }
      fs.writeFileSync(output,JSON.stringify(spec,null,2));console.log(batch.join(',')+': validated '+batch.reduce((count,l)=>count+Object.keys(translated[l]).length,0)+' entries');
      }
      assert.ok(requested.every(locale=>Object.values(spec).every(texts=>texts[locale])),'Incomplete translation coverage');
    }
    fs.writeFileSync(specFile,JSON.stringify(spec,null,2)+'\n');console.log('All locale entries generated; linguistic and layout review remains required.');
  }finally{if(native){await native.browser.close().catch(()=>{});const child=native.server.child;if(child.exitCode===null){const closed=new Promise(resolve=>child.once('exit',resolve));child.kill();await Promise.race([closed,new Promise(resolve=>setTimeout(resolve,2000))]);}}fs.rmSync(path.join(data,'ai.json'),{force:true});try{fs.rmSync(serviceBinary,{force:true});}catch{console.log('Translation service executable is still locked; retained in the isolated cache.');}}
})().catch(e=>{console.error('Translation stopped at '+phase+' ('+e.name+'); validated results retained. No credentials logged.');process.exitCode=1;});
