'use strict';
// QA/content preparation only. Credentials stay in an owned temporary profile
// and are removed after the service stops; no provider secrets enter reports.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),net=require('node:net'),assert=require('node:assert/strict');
const {chromium}=require('playwright');
const {startNative}=require('../ui-tests/native-session.cjs');
const root=path.resolve(__dirname,'..'),hash=x=>crypto.createHash('sha256').update(x).digest('hex');
const data=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/publication-error-translations-'));
const profile=path.join(data,'profile');fs.mkdirSync(profile);
const report={complete:false,productRuntimeTest:false,humanLinguisticReview:false,groups:[],automaticRetries:0};
const text={
 'error.publication_layout_geometry':{en:'The content does not fit the requested page or paragraph layout. Adjust margins, columns or line spacing.',
  'zh-CN':'内容无法放入所设的页面或段落布局。请调整页边距、分栏或行距。',
  'zh-HK':'內容無法放入設定的頁面或段落版面。請調整頁邊距、分欄或行距。',
  'zh-TW':'內容無法放入設定的頁面或段落版面。請調整頁邊距、分欄或行距。'},
 'error.publication_line_spacing_unsupported':{en:'This output or preview does not support the requested line spacing. Choose a supported output or remove this line-spacing setting.',
  'zh-CN':'此输出或预览暂不支持所请求的行距。请选择支持的输出方式，或移除该行距设置。',
  'zh-HK':'此輸出或預覽暫不支援所要求的行距。請選擇支援的輸出方式，或移除此行距設定。',
  'zh-TW':'此輸出或預覽暫不支援要求的行距。請選擇支援的輸出方式，或移除此行距設定。'}
};
let native,stage='preparation';const copies=[];
async function freePort(){const s=net.createServer();await new Promise((r,j)=>{s.once('error',j);s.listen(0,'127.0.0.1',r);});const port=s.address().port;await new Promise(r=>s.close(r));return port;}
(async()=>{try{
 assert(process.env.READMD_BIN&&process.env.READMD_CANDIDATE_SHA256);
 const bytes=fs.readFileSync(process.env.READMD_BIN);assert.equal(hash(bytes),process.env.READMD_CANDIDATE_SHA256);
 report.serviceSha256=hash(bytes);const executable=path.join(profile,'readmd-translation.exe');
 fs.writeFileSync(executable,bytes,{flag:'wx'});copies.push(executable);process.env.READMD_BIN=executable;
 const source=path.join(process.env.APPDATA,'ReadMD'),configFile=path.join(source,'ai.json');
 const original=fs.readFileSync(configFile),config=JSON.parse(original);
 const relay=config.providers.find(p=>/antigravity/i.test(p.name||''));assert(relay,'Configured Antigravity is required');
 config.current.provider_id=relay.id;
 const localConfig=path.join(profile,'ai.json');fs.writeFileSync(localConfig,JSON.stringify(config),{flag:'wx'});copies.push(localConfig);
 for(const name of ['credentials.vault','encryption.key']){const input=path.join(source,name);if(fs.existsSync(input)){const out=path.join(profile,name);fs.copyFileSync(input,out,fs.constants.COPYFILE_EXCL);copies.push(out);}}
 stage='startup';native=await startNative(root,profile,chromium,await freePort());
 const api=(route,body)=>native.page.evaluate(async({route,body})=>{const r=await apiFetch(route,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body),signal:AbortSignal.timeout(180000)});return{status:r.status,data:await r.json()};},{route,body});
 stage='actual model discovery';const discovery=await api('/api/ai/models',{provider:relay.id});assert.equal(discovery.status,200);assert.equal(discovery.data.ok,true);
 const available=(discovery.data.models||[]).map(v=>typeof v==='string'?v:v.id).filter(Boolean);
 const model=available.find(s=>/gemini.*flash.*lite/i.test(s))||available.find(s=>/gemini.*flash/i.test(s));assert(model,'No available fast Gemini model');
 report.model=model;report.actualDiscovery={modelCount:available.length,selectedListed:true};
 const locales=JSON.parse(fs.readFileSync(path.join(root,'assets/i18n/meta.json'))),keys=Object.keys(text);
 const missing=Object.keys(locales).filter(loc=>!text[keys[0]][loc]);
 const jobs=[];for(let i=0;i<missing.length;i+=6)jobs.push(missing.slice(i,i+6));
 stage='translation';let cursor=0,failures=0;
 async function worker(){while(cursor<jobs.length&&failures<3){const id=cursor++,languages=jobs[id],record={id,locales:languages,passed:false};
  try{
   const prompt='Translate these two ReadMD document-export UI error messages into exactly the languages listed. Return ONLY valid JSON mapping each language code to an object with exactly the original two keys. Translate all prose naturally; no English fallback. "Line spacing" means the vertical distance between lines of text; "columns" means page columns. Languages: '+JSON.stringify(Object.fromEntries(languages.map(l=>[l,locales[l].native])))+'\nMessages: '+JSON.stringify(Object.fromEntries(keys.map(k=>[k,text[k].en])));
   const result=await api('/api/ai/chat',{provider:relay.id,model,stream:false,messages:[{role:'user',content:prompt}],session:'publication-error-translation-'+path.basename(data)+'-'+id,max_tokens:8192});
   record.status=result.status;assert.equal(result.status,200);
   const raw=result.data.content;assert.equal(typeof raw,'string');fs.writeFileSync(path.join(data,'group-'+id+'.response.txt'),raw,{flag:'wx'});record.responseSha256=hash(raw);
   const value=JSON.parse(raw.trim().replace(/^```(?:json)?\s*/i,'').replace(/\s*```$/,''));
   assert.deepEqual(Object.keys(value).sort(),[...languages].sort());
   for(const loc of languages){assert.deepEqual(Object.keys(value[loc]).sort(),[...keys].sort());for(const key of keys){const translated=value[loc][key];assert(typeof translated==='string'&&translated.trim()&&translated!==text[key].en);text[key][loc]=translated;}}
   record.passed=true;
  }catch(error){record.errorType=error.name;failures++;process.exitCode=1;}
  report.groups.push(record);fs.writeFileSync(path.join(data,'group-'+id+'.json'),JSON.stringify(record,null,2),{flag:'wx'});
  console.log(JSON.stringify({group:id,locales:languages,passed:record.passed}));
 }}
 await Promise.all([worker(),worker()]);
 assert.equal(hash(fs.readFileSync(configFile)),hash(original),'Installed user configuration changed');
 report.missing=Object.fromEntries(keys.map(k=>[k,Object.keys(locales).filter(l=>!text[k][l])]));
 report.complete=Object.values(report.missing).every(v=>v.length===0);
 fs.writeFileSync(path.join(data,'translations.json'),JSON.stringify(text,null,2)+'\n',{flag:'wx'});
 assert(report.complete,'Translation coverage incomplete');
}catch(error){report.failedStage=stage;report.errorType=error.name;process.exitCode=1;}
finally{
 if(native){await native.browser.close().catch(()=>{});const child=native.server.child;if(child.exitCode===null){const stopped=new Promise(r=>child.once('exit',r));child.kill();await Promise.race([stopped,new Promise(r=>setTimeout(r,3000))]);}report.hostStopped=child.exitCode!==null||child.signalCode!==null;}
 report.cleanupFailures=[];for(const file of copies.reverse()){try{fs.unlinkSync(file);}catch{report.cleanupFailures.push(path.basename(file));}}
 if(report.cleanupFailures.length){report.complete=false;process.exitCode=1;}
 report.recordedAt=new Date().toISOString();fs.writeFileSync(path.join(data,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx'});
 console.log(JSON.stringify({complete:report.complete,report:path.relative(root,path.join(data,'report.json')),failedStage:report.failedStage}));
}})();
