const {waitForPage}=require('./page-condition.cjs');
const {test,expect}=require('@playwright/test');
async function editor(page,enabled=true) {
  await page.goto('/'); await waitForPage(page, ()=>window.__readmdAppReady);
  await page.evaluate(async enabled=>{
    await i18n.setLanguage('zh-CN',false);
    await renderVirtual('clipboard','续写演示.md','', '# 研究札记\n\n我们先观察证据，再',[]);
    await toggleEdit(); setPvLayout('none'); setEditorPref('aiCompletion',enabled);
    cmView.dispatch({selection:{anchor:cmView.state.doc.length}}); cmView.focus();
  },enabled);
}
test('real editor ghost is ephemeral, Tab inserts once and undo restores the exact draft',async({page})=>{
  let count=0;
  await page.route('**/api/ai/complete',async route=>{count++;await route.fulfill({json:{ok:true,completion:'提出可检验的解释。'}});});
  await editor(page);
  const before=await page.evaluate(()=>getEditContent());
  await page.keyboard.press('Control+Alt+Space');
  await expect(page.locator('.readmd-ai-ghost')).toHaveText('提出可检验的解释。');
  expect(await page.evaluate(()=>getEditContent())).toBe(before);
  await page.keyboard.press('Tab');
  expect(await page.evaluate(()=>getEditContent())).toBe(before+'提出可检验的解释。');
  await page.keyboard.press('Control+z');
  expect(await page.evaluate(()=>getEditContent())).toBe(before);
  const cachedAt=Date.now();
  await page.keyboard.press('Control+Alt+Space');
  await expect(page.locator('.readmd-ai-ghost')).toBeVisible();
  expect(Date.now()-cachedAt).toBeLessThan(300);
  expect(count).toBe(1); // The identical request is served from the bounded cache.
  await page.keyboard.press('Escape');
  await expect(page.locator('.readmd-ai-ghost')).toHaveCount(0);
  expect(await page.evaluate(()=>getEditContent())).toBe(before);
});

test('completion model stays in advanced settings, persists independently and fits compact windows',async({page},testInfo)=>{
  const provider={id:'custom:fast-test',name:'本地测试连接',custom:true,category:'local',base_url:'http://127.0.0.1:1/v1',
    models:['chat-model','fast-writing-model'],autocomplete:{mode:'auto'}};
  let config={ok:true,custom:[provider],presets:[],current:{provider_id:provider.id,model:'chat-model'}};
  let saved;
  await page.route('**/api/ai/config',async route=>{
    if(route.request().method()==='POST'){
      saved=route.request().postDataJSON();config={...config,custom:saved.providers,current:saved.current};
    }
    await route.fulfill({json:config});
  });
  await page.goto('/');await waitForPage(page, ()=>window.__readmdAppReady);
  await page.evaluate(async()=>{await i18n.setLanguage('zh-CN',false);await loadAiConfig();openAiModal('ai-settings-modal',document.querySelector('#ai-settings-open'));});
  const advanced=page.locator('#ai-settings-box .conn-advanced');
  await expect(advanced).not.toHaveAttribute('open','');
  await advanced.locator('summary').first().click();
  const model=page.locator('#ai-autocomplete-model');
  await expect(model).toHaveValue('auto');await model.selectOption('model:fast-writing-model');
  await page.locator('#ai-save-key').click();
  expect(saved.providers[0].autocomplete).toEqual({mode:'model',model:'fast-writing-model'});
  expect(saved.current.model).toBe('chat-model');
  await page.evaluate(()=>openAiModal('ai-settings-modal',document.querySelector('#ai-settings-open')));
  await expect(model).toHaveValue('model:fast-writing-model');
  await model.selectOption('auto');
  for(const [width,height]of [[1160,820],[1024,680]]){
    await page.setViewportSize({width,height});
    for(const language of ['zh-CN','en','ja']){
      await page.evaluate(language=>i18n.setLanguage(language,false),language);
      await expect(model.locator('option[value="auto"]')).toHaveText({'zh-CN':'自动选择 · 优先响应速度',en:'Automatic · prefer speed',ja:'自動選択 · 応答速度を優先'}[language]);
      await model.scrollIntoViewIfNeeded();
      expect(await page.locator('#ai-settings-box').evaluate(el=>el.scrollWidth<=el.clientWidth+1)).toBe(true);
      const box=await page.locator('#ai-save-key').boundingBox();expect(box.y+box.height).toBeLessThanOrEqual(height);
      await page.screenshot({path:testInfo.outputPath('completion-model-'+width+'-'+language+'.png')});
    }
  }
});
test('typing through a suggestion retains the suffix without another AI request',async({page})=>{
  let count=0;
  await page.route('**/api/ai/complete',route=>{count++;return route.fulfill({json:{ok:true,completion:'test the evidence.'}});});
  await editor(page);await page.keyboard.press('Control+Alt+Space');
  await expect(page.locator('.readmd-ai-ghost')).toHaveText('test the evidence.');
  await page.keyboard.type('test ',{delay:50});
  await expect(page.locator('.readmd-ai-ghost')).toHaveText('the evidence.');
  await page.waitForTimeout(1200);expect(count).toBe(1);
  const before=await page.evaluate(()=>getEditContent());
  await page.keyboard.press('Tab');expect(await page.evaluate(()=>getEditContent())).toBe(before+'the evidence.');
  await page.keyboard.press('Control+z');expect(await page.evaluate(()=>getEditContent())).toBe(before);
});

test('saving completion preferences preserves the chat model when discovery is empty or omits it',async({page})=>{
  const provider={id:'custom:empty-discovery',name:'本地测试连接',custom:true,category:'local',base_url:'http://127.0.0.1:1/v1',models:[],autocomplete:{mode:'auto'}};
  let config={ok:true,custom:[provider],presets:[],current:{provider_id:provider.id,model:'saved-chat-model'}};
  let saved;
  await page.route('**/api/ai/config',async route=>{
    if(route.request().method()==='POST'){
      saved=route.request().postDataJSON();config={...config,custom:saved.providers,current:saved.current};
    }
    await route.fulfill({json:config});
  });
  await page.goto('/');await waitForPage(page, ()=>window.__readmdAppReady);
  await page.evaluate(async()=>{await i18n.setLanguage('zh-CN',false);await loadAiConfig();openAiModal('ai-settings-modal',document.querySelector('#ai-settings-open'));});
  await expect(page.locator('#ai-model')).toHaveValue('saved-chat-model');
  await expect(page.locator('#ai-model')).toBeEnabled();
  await page.locator('#ai-settings-box .conn-advanced summary').first().click();
  await page.locator('#ai-autocomplete-model').selectOption('current');
  await page.locator('#ai-save-key').click();
  expect(saved.current.model).toBe('saved-chat-model');
  expect(saved.providers[0].autocomplete).toEqual({mode:'current'});
  // A click dispatches the async save; wait for its catalog refresh before
  // simulating another discovery, just as the temporarily locked UI requires.
  await expect(page.locator('#ai-save-key')).toBeEnabled();
  await page.evaluate(()=>fillAiModels(['different-available-model'],'saved-chat-model'));
  await expect(page.locator('#ai-model')).toHaveValue('saved-chat-model');
  await expect(page.locator('#ai-model option[value="different-available-model"]')).toHaveCount(1);
});

test('a slow catalog refresh keeps saved model choices stable and overlapping saves coalesce',async({page})=>{
  const provider={id:'custom:slow-refresh',name:'测试连接',custom:true,category:'local',models:['chat','fast'],autocomplete:{mode:'auto'}};
  let config={ok:true,custom:[provider],presets:[],current:{provider_id:provider.id,model:'chat'}};
  let posts=0,refreshStarted=false,releaseRefresh;
  const refreshGate=new Promise(resolve=>{releaseRefresh=resolve;});
  await page.route('**/api/ai/config',async route=>{
    if(route.request().method()==='POST'){
      posts++;const saved=route.request().postDataJSON();
      config={...config,custom:saved.providers,current:saved.current};
    }else if(posts){refreshStarted=true;await refreshGate;}
    await route.fulfill({json:config});
  });
  await page.goto('/');await waitForPage(page, ()=>window.__readmdAppReady);
  await page.evaluate(async()=>{await loadAiConfig();openAiModal('ai-settings-modal',document.querySelector('#ai-settings-open'));});
  await page.locator('#ai-settings-box .conn-advanced summary').first().click();
  await page.locator('#ai-autocomplete-model').selectOption('model:fast');
  try{
    await page.evaluate(()=>{window.__pendingSave=Promise.all([saveAiSelection(true),saveAiSelection(true)]);});
    await expect.poll(()=>refreshStarted).toBe(true);
    expect(posts).toBe(1);
    await expect(page.locator('#ai-save-key')).toBeDisabled();
    await expect(page.locator('#ai-model')).toBeDisabled();
    await expect(page.locator('#ai-autocomplete-model')).toBeDisabled();
    expect(config.current.model).toBe('chat');
    expect(config.custom[0].autocomplete).toEqual({mode:'model',model:'fast'});
  }finally{releaseRefresh();}
  expect(await page.evaluate(()=>window.__pendingSave)).toEqual([true,true]);
  await expect(page.locator('#ai-autocomplete-model')).toBeEnabled();
  await expect(page.locator('#ai-autocomplete-model')).toHaveValue('model:fast');
  await expect(page.locator('#ai-model')).toHaveValue('chat');
});

test('a wrapped ghost near the bottom remains visible without entering the saved draft',async({page})=>{
  const suggestion='\n第一步，检查原始证据。\n第二步，比较不同解释。\n第三步，主动寻找反例。';
  await page.route('**/api/ai/complete',route=>route.fulfill({json:{ok:true,completion:suggestion}}));
  await editor(page);
  const before=await page.evaluate(()=>{
    const text=Array.from({length:45},(_,i)=>'研究记录 '+i+'：证据需要核对。').join('\n')+'\n因此，我们应该';
    cmView.dispatch({changes:{from:0,to:cmView.state.doc.length,insert:text},selection:{anchor:text.length},scrollIntoView:true});
    cmView.focus();return text;
  });
  await page.keyboard.press('Control+Alt+Space');
  await expect(page.locator('.readmd-ai-ghost')).toHaveText(suggestion);
  await expect.poll(()=>page.evaluate(()=>{
    const ghost=document.querySelector('.readmd-ai-ghost').getBoundingClientRect();
    const viewport=cmView.scrollDOM.getBoundingClientRect();
    return ghost.top>=viewport.top-1 && ghost.bottom<=viewport.bottom+1;
  })).toBe(true);
  expect(await page.evaluate(()=>getEditContent())).toBe(before);
});

test('cursor completion remains inspectable and localized without becoming a chat template',async({page})=>{
  await editor(page,false);
  const result=await page.evaluate(async()=>{
    await loadAiPrompts();
    const skill=state.ai.templates.find(t=>t.skill_id==='readmd-autocomplete');
    renderSkillOverview(skill);
    return {present:!!skill,name:getLocalizedSkillName(skill),description:document.querySelector('#tpl-skill-overview p').textContent,
      chat:[...document.querySelector('#ai-template').options].map(o=>o.value)};
  });
  expect(result.present).toBe(true);
  expect(result.name).toBe('AI 自动补全');
  expect(result.description).toBe('在光标处提供简短续写；Tab 接受，Esc 忽略。');
  expect(result.chat).not.toContain('readmd-autocomplete');
});

test('disabled means no request; typing debounces; selections and IME cancel late suggestions',async({page})=>{
  let calls=0;
  await page.route('**/api/ai/complete',async route=>{
    calls++; await new Promise(r=>setTimeout(r,350));
    await route.fulfill({json:{ok:true,completion:'这是候选。'}}).catch(()=>{});
  });
  await editor(page,false); await page.keyboard.type(' test',{delay:30}); await page.waitForTimeout(800);
  expect(calls).toBe(0);
  await page.evaluate(()=>setEditorPref('aiCompletion',true));
  await page.keyboard.type(' typing',{delay:35}); await page.waitForTimeout(680); expect(calls).toBe(1);
  await page.keyboard.press('ArrowLeft'); await page.waitForTimeout(500);
  await expect(page.locator('.readmd-ai-ghost')).toHaveCount(0);
  await page.evaluate(()=>cmView.dispatch({selection:{anchor:0,head:5}}));
  await page.keyboard.press('Control+Alt+Space'); await page.waitForTimeout(700); expect(calls).toBe(1);
  await page.evaluate(()=>{cmView.dispatch({selection:{anchor:cmView.state.doc.length}});cmView.focus();});
  await page.keyboard.press('Control+Alt+Space'); await page.waitForTimeout(120);
  await page.locator('.cm-content').dispatchEvent('compositionstart');
  await page.waitForTimeout(500); await expect(page.locator('.readmd-ai-ghost')).toHaveCount(0);
});
test('table Tab and slash menu retain priority; failure leaves editable content intact',async({page})=>{
  await page.route('**/api/ai/complete',route=>route.fulfill({status:429,json:{ok:false,error_code:'ai_completion_busy'}}));
  await editor(page); const before=await page.evaluate(()=>getEditContent());
  await page.keyboard.press('Control+Alt+Space');
  await expect(page.locator('#status-ai-completion')).toContainText('补全暂不可用');
  expect(await page.evaluate(()=>getEditContent())).toBe(before);
  await page.evaluate(()=>{setEditorPref('aiCompletion',false);cmView.dispatch({changes:{from:0,to:cmView.state.doc.length,insert:'| A | B |\n| --- | --- |\n| 一 | 二 |'},selection:{anchor:29}});cmView.focus();});
  await page.keyboard.press('Tab');
  expect(await page.evaluate(()=>getEditContent())).toContain('|');
  expect(await page.evaluate(()=>editorPrefs.aiCompletion)).toBe(false);
});
