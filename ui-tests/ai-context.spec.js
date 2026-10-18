const {test,expect}=require('@playwright/test');
const http=require('node:http');
const {waitForPage}=require('./page-condition.cjs');
test.use({bypassCSP:false});
test.beforeEach(async({page})=>{
  await page.goto('/'); await waitForPage(page,()=>window.__readmdAppReady);
  await page.evaluate(()=>Promise.all([window.__readmdSessionReady,window.__readmdPreferencesReady]));
});
async function document(page,source,editing=false){
  await page.evaluate(async({source,editing})=>{
    await renderVirtual('clipboard','Context.md','',source,[]);
    if(editing){await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('none');}
  },{source,editing});
}
async function relay(page){
  const requests=[];
  const server=http.createServer((req,res)=>{
    let raw='';req.on('data',chunk=>raw+=chunk);req.on('end',()=>{
      requests.push(JSON.parse(raw));res.writeHead(200,{'Content-Type':'application/json'});
      res.end(JSON.stringify({choices:[{message:{content:'Revised evidence.'}}]}));
    });
  });
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  await page.evaluate(async port=>{
    const result=await apiFetch('/api/ai/config',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({
      providers:[{id:'custom:context-test',name:'Context test',category:'local',base_url:'http://127.0.0.1:'+port+'/v1',mode:'chat',format:'openai',models:['context-test']}],
      current:{provider_id:'custom:context-test',model:'context-test'}})});
    if(!result.ok)throw Error('Local provider configuration failed');
    await loadAiConfig();$('ai-stream').checked=false;
    $('ai-panel').classList.remove('hidden');
  },server.address().port);
  return {requests,async close(){server.closeAllConnections();await new Promise(resolve=>server.close(resolve));}};
}
test('editor selection is automatic, survives composer focus and follow-up, and uses exact duplicate offsets',async({page})=>{
  const source='Same evidence.\n\nSame evidence.';await document(page,source,true);
  const local=await relay(page);
  try{
    await page.evaluate(()=>{cmView.dispatch({selection:{anchor:16,head:30}});cmView.focus();});
    await page.locator('#ai-prompt').fill('Polish only this passage.');
    await expect(page.locator('#ai-selection')).toHaveCount(0);
    await page.evaluate(()=>runAi('ask'));
    expect(local.requests).toHaveLength(1);
    expect(await page.evaluate(()=>state.ai.lastSelection)).toMatchObject({text:'Same evidence.',from:16,to:30,exact:true});
    await page.evaluate(()=>{cmView.dispatch({selection:{anchor:0}});});
    await page.locator('#ai-prompt').fill('Make it shorter.');await page.evaluate(()=>runAi('ask'));
    expect(local.requests).toHaveLength(2);
    expect(await page.evaluate(()=>state.ai.lastSelection.from)).toBe(16);
    await page.evaluate(()=>applyAi('Revised evidence.',state.ai.lastSelection));
    expect(await page.evaluate(()=>getEditContent())).toBe('Same evidence.\n\nRevised evidence.');
    await page.evaluate(()=>cmUndo());expect(await page.evaluate(()=>getEditContent())).toBe(source);
  }finally{await local.close();}
});
test('reader selection maps the second repeated word through its block and survives focus',async({page})=>{
  const source='Evidence evidence.\n\nEvidence evidence.';await document(page,source);
  await page.evaluate(()=>{
    const text=$('content').querySelectorAll('p')[1].firstChild,range=document.createRange();
    range.setStart(text,9);range.setEnd(text,17);getSelection().removeAllRanges();getSelection().addRange(range);
    toggleAiPanel();
  });
  await page.locator('#ai-prompt').fill('Explain this word.');
  expect(await page.evaluate(()=>getAiTargetText())).toMatchObject({text:'evidence',isSelection:true,from:29,to:37});
  await expect(page.locator('#ai-context-file')).toContainText(await page.evaluate(()=>i18n.t('ai.scopeSelection')));
});
test('an AI answer selection never becomes document context and changing documents resets the old target',async({page})=>{
  await document(page,'# Whole document\n\nOriginal evidence.');
  await page.evaluate(()=>{
    $('ai-output').textContent='Answer outside the document';
    const range=document.createRange();range.selectNodeContents($('ai-output'));getSelection().removeAllRanges();getSelection().addRange(range);
    captureAiSelection();
  });
  expect(await page.evaluate(()=>getAiTargetText())).toMatchObject({text:'# Whole document\n\nOriginal evidence.',isSelection:false});
  await document(page,'# Another document\n\nDifferent evidence.');
  expect(await page.evaluate(()=>getAiTargetText().text)).toContain('Different evidence.');
});
test('free conversation works without a document',async({page})=>{
  await page.evaluate(()=>goHome({discardConfirmed:true}));const local=await relay(page);
  try{
    await page.locator('#ai-prompt').fill('Explain what a premise is.');await page.evaluate(()=>runAi('ask'));
    expect(local.requests).toHaveLength(1);
    expect(JSON.stringify(local.requests[0].messages)).toContain('Explain what a premise is.');
    expect(await page.evaluate(()=>state.ai.raw)).toBe('Revised evidence.');
  }finally{await local.close();}
});
test('waiting for configuration fixes the original target and prevents duplicate requests',async({page})=>{
  await document(page,'Original task target.');const local=await relay(page);
  try{
    await page.locator('#ai-prompt').fill('Summarize.');
    const tab=await page.evaluate(()=>{
      const ensure=ensureAiConfigured,gate=new Promise(resolve=>window.__releaseAiConfig=resolve);
      ensureAiConfigured=async()=>{await gate;return ensure();};
      window.__contextRequest=runAi('ask');runAi('ask');return state.activeTabId;
    });
    await document(page,'Later unrelated document.');
    await page.evaluate(async()=>{window.__releaseAiConfig();await window.__contextRequest;});
    expect(local.requests).toHaveLength(1);
    const messages=JSON.stringify(local.requests[0].messages);
    expect(messages).toContain('Original task target.');expect(messages).not.toContain('Later unrelated document.');
    expect(await page.evaluate(()=>state.ai.lastSelection.tabId)).toBe(tab);
  }finally{await local.close();}
});
test('rich reader projections without exact offsets cannot replace a coincidental source string',async({page})=>{
  const source='**Evidence** matters.\n\nEvidence.';await document(page,source);
  await page.evaluate(()=>{
    const text=$('content').querySelector('strong').firstChild,range=document.createRange();range.selectNodeContents(text);
    getSelection().removeAllRanges();getSelection().addRange(range);captureAiSelection();
  });
  expect(await page.evaluate(()=>getAiTargetText())).toMatchObject({text:'Evidence',from:-1,to:-1});
  await page.evaluate(async()=>{await toggleEdit();await applyAi('Changed',{...getAiTargetText(),exact:false,name:'Context.md',tabId:state.activeTabId});});
  expect(await page.evaluate(source=>state.tabs.some(t=>(t.content||t.original)===source),source)).toBe(true);
});

test('fallback editor uses exact duplicate offsets and refuses ambiguous selections',async({page})=>{
  const source='Same evidence.\n\nSame evidence.';await document(page,source,true);
  const replaced=await page.evaluate(async source=>{
    // Exercise the existing fallback editor independently of CodeMirror.
    cmView=null;window.ReadMDCodeMirror=null;$('edit-area').value=source;
    await applyAi('Revised evidence.',{text:'Same evidence.',from:16,to:30,exact:true,tabId:state.activeTabId});
    return $('edit-area').value;
  },source);
  expect(replaced).toBe('Same evidence.\n\nRevised evidence.');
  const target=await page.evaluate(async source=>{
    $('edit-area').value=source;const tabId=state.activeTabId;
    await applyAi('Ambiguous change',{text:'Same evidence.',from:-1,to:-1,name:'Context.md',tabId});
    return {tabId,active:state.activeTabId,original:state.tabs.find(t=>t.id===tabId).content};
  },source);
  expect(target.active).not.toBe(target.tabId);expect(target.original).toBe(source);
});
