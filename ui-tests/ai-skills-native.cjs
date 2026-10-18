const fs=require('fs'),path=require('path'),assert=require('assert/strict'),{chromium}=require('playwright');
const {startUiServer}=require('./ui-server.cjs');
const out=path.resolve(process.env.READMD_AI_AUDIT_OUT||'.cache/ai-skills-native'),data=path.join(out,'data');fs.mkdirSync(data,{recursive:true});
if(!process.env.READMD_AI_SOURCE_CONFIG)throw Error('Set READMD_AI_SOURCE_CONFIG to a saved ReadMD configuration; never put credentials on the command line.');
const config=JSON.parse(fs.readFileSync(process.env.READMD_AI_SOURCE_CONFIG,'utf8'));fs.writeFileSync(path.join(data,'ai.json'),JSON.stringify(config));process.env.READMD_DATA_DIR=data;process.env.READMD_WORKSPACE=data;
(async()=>{let native,browser;const report={passed:false,realProvider:true,skills:[],failures:[]};
 try{
  const port=Number(process.env.READMD_AI_AUDIT_PORT||28847);native=await startUiServer(port,{stdio:'ignore'});browser=await chromium.launch({headless:true,channel:'msedge'});const page=await browser.newPage();await page.goto('http://127.0.0.1:'+port+'/');await page.waitForFunction(()=>window.__readmdAppReady);
  const api=(body,route='/api/ai/chat')=>page.evaluate(async({body,route})=>{const r=await apiFetch(route,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body),signal:AbortSignal.timeout(65000)});return{status:r.status,body:await r.json()};},{body,route});
  const discovery=await api({},'/api/ai/models');assert.equal(discovery.status,200);
  console.log(JSON.stringify({actualAdvertisedModels:discovery.body.models}));
  const model=process.env.READMD_AI_TEST_MODEL||discovery.body.models.find(m=>/gemini-3\.8-flash-(?:low|medium)$/.test(m))||config.current.model;report.model=model;
  const document='# 证据与推理\n\n研究从观察开始。证据支持判断，反例帮助修正结论。观察值为 3、5、8。\n\n- [x] 记录观察\n- [ ] 下周对比两种解释\n\nfunction mean(xs) { return xs.reduce((a,b)=>a+b,0); }\n';
  const requests={
   'readmd-ask':'用一句话解释证据和观点的区别。','readmd-summary':'用一句话总结。','readmd-outline':'只列三个小标题。','readmd-polish':'润色第一段，不改变意思。','readmd-translate':'Translate into English.','readmd-quick-read':'给出三个阅读要点。','readmd-code-analysis':'简要分析 mean 是否实现平均值。','readmd-code-review':'指出 mean 最严重的一个问题。','readmd-code-to-doc':'用三句话说明 mean 的输入输出。','readmd-continue':'只续写一句话。','readmd-export-style':'只给一个最小、清爽、可读的排版样式，尽量简短。','readmd-format-fix':'仅修复明显的 Markdown 排版。','readmd-humanizer':'改写第一段，简洁自然。','readmd-localization':'将第一段本地化为英文。','readmd-proofread':'仅指出一处最明显的文字问题，没有则说明。','readmd-skill-creator':'创建一个最小的“用一句话总结”Skill。','readmd-style-custom':'只定义正文的一个 font-size 样式，其他内容省略。','readmd-todo':'提取未完成事项。','readmd-weekly':'用三句话总结本周进展与下周计划。'};
  const selected=new Set((process.env.READMD_AI_SKILLS||'').split(',').filter(Boolean));
  for(const[id,request]of Object.entries(requests)){
   if(selected.size&&!selected.has(id))continue;
   const began=Date.now();try{
    const result=await api({model,stream:false,skill_id:id,skill_variables:{document,selection:'',request,language:'zh-CN',context:'',output_format:'Markdown'},messages:[{role:'user',content:request+'\n\n'+document}],session:'synthetic-skill-'+id});
    assert.equal(result.status,200,id+': '+result.body.error_code);assert.ok(result.body.content?.trim());assert.ok(!/(?:model|gemini|claude).*(?:retired|no longer available)|please switch to gemini|模型.*下线/i.test(result.body.content));
    const item={id,elapsedMs:Date.now()-began,characters:result.body.content.length,response:result.body.content,ok:true};report.skills.push(item);console.log(JSON.stringify({id,elapsedMs:item.elapsedMs,characters:item.characters,ok:true}));
   }catch(e){const item={id,elapsedMs:Date.now()-began,error:e.message.slice(0,160),ok:false};report.skills.push(item);report.failures.push(item);console.log(JSON.stringify(item));}
   fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2));
  }
  if(process.env.READMD_AI_SKILL_LIFECYCLE==='1'){
    const generated=await api({action:'generate',model,request:'创建一个用一句中文总结文档的 Skill。请保留 document 模板变量，不要写死示例文字。',document,language:'zh-CN'},'/api/skills');
    assert.equal(generated.status,200);const draft=generated.body.draft;assert.equal(draft.metadata.enabled,false);assert.equal(draft.metadata.scripts_allowed,false);
    const content='---\nname: '+draft.id+'\ndescription: '+draft.description.replace(/[\r\n]/g,' ')+'\n---\n\n'+draft.instructions;
    assert.ok(content.includes('{{document}}'),'Generated Skill must be reusable rather than contain a fixed sample');
    const body={id:draft.id,content,metadata:draft.metadata};
    const denied=await api({...body,action:'publish',confirm:true},'/api/skills');assert.equal(denied.status,400);
    const evaluated=await api({...body,action:'evaluate'},'/api/skills');assert.equal(evaluated.status,200);assert.ok(evaluated.body.evaluation_token);
    const changed=await api({...body,content:content+'\nChanged',action:'publish',confirm:true,evaluation_token:evaluated.body.evaluation_token},'/api/skills');assert.equal(changed.status,400);
    const published=await api({...body,action:'publish',confirm:true,evaluation_token:evaluated.body.evaluation_token},'/api/skills');assert.equal(published.status,200);
    const run=await api({model,stream:false,skill_id:draft.id,skill_variables:{document,request:'仅返回一句摘要。',language:'zh-CN'},messages:[{role:'user',content:document}]});assert.equal(run.status,200);assert.ok(run.body.content?.trim());
    const exported=await api({action:'export',id:draft.id},'/api/skills');assert.equal(exported.status,200);assert.equal(exported.body.content,content);
    const disabled=await api({action:'disable',id:draft.id},'/api/skills');assert.equal(disabled.status,200);
    const deleted=await api({action:'delete',id:draft.id},'/api/ai/prompts');assert.equal(deleted.status,200);
    report.lifecycle={passed:true,generatedDisabled:true,rawPublishRejected:true,changedDraftRejected:true,evaluatedPublished:true,actualGeneratedSkillExecuted:true,exportMatches:true,disabledAndRemoved:true};
  }
  report.passed=(report.skills.length>0||report.lifecycle?.passed)&&report.failures.length===0;if(!report.passed)process.exitCode=1;
 }catch(e){report.failure=e.message;process.exitCode=1;}
 finally{fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2));if(browser)await browser.close();if(native)await native.stop();fs.rmSync(path.join(data,'ai.json'),{force:true});}
})();
