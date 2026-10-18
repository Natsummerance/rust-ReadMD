const {waitForPage}=require('./page-condition.cjs');
const {test,expect}=require('@playwright/test');
const http=require('node:http');

async function streamingEditor(page,{error=false}={}){
  let requests=0,closed=0,advance=()=>{throw Error('stream is not ready');};
  const server=http.createServer((req,res)=>{
    requests++;req.resume();
    res.writeHead(200,{'Content-Type':'text/event-stream'});
    const delta=text=>res.write('data: '+JSON.stringify({choices:[{delta:{content:text}}]})+'\n\n');
    let stage=0;
    advance=()=>{
      if(res.destroyed)return;
      if(error){res.destroy();return;}
      if(++stage===1)delta('再比较不同解释，');
      else if(stage===2)delta('主动寻找能检验结论的反例。');
      else res.end('data: [DONE]\n\n');
    };
    // Keep each upstream boundary open until the browser has observed it.
    // Fixed 500 ms windows are unreliable under concurrent stress/builds.
    const timers=[setTimeout(()=>delta('核对原始证据，'),100)];
    res.on('close',()=>{closed++;timers.forEach(clearTimeout);});
  });
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  await page.goto('/');await waitForPage(page, ()=>window.__readmdAppReady);
  await page.evaluate(async port=>{
    await apiFetch('/api/ai/config',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({
      providers:[{id:'custom:stream-fixture',name:'流式测试',base_url:'http://127.0.0.1:'+port+'/v1',category:'local',models:['stream-model'],autocomplete:{mode:'current'}}],
      current:{provider_id:'custom:stream-fixture',model:'stream-model'}})});
    await loadAiConfig();await i18n.setLanguage('zh-CN',false);
    await renderVirtual('clipboard','流式演示.md','','# 研究札记\n\n为了避免主观偏见，我们应该',[]);
    await toggleEdit();setPvLayout('none');setEditorPref('aiCompletion',true);
    cmView.dispatch({selection:{anchor:cmView.state.doc.length}});cmView.focus();
  },server.address().port);
  return {advance:()=>advance(),requests:()=>requests,closed:()=>closed,stop:()=>new Promise(resolve=>{server.closeAllConnections();server.close(resolve);})};
}

test('actual HTTP streaming renders incremental ghost snapshots without editing the draft',async({page})=>{
  const fixture=await streamingEditor(page);
  try{
    const before=await page.evaluate(()=>getEditContent());
    await page.keyboard.press('Control+Alt+Space');
    await expect(page.locator('.readmd-ai-ghost')).toHaveText('核对原始证据，');
    await expect(page.locator('.readmd-ai-ghost')).toHaveAttribute('data-streaming','true');
    expect(await page.evaluate(()=>getEditContent())).toBe(before);
    fixture.advance();
    await expect(page.locator('.readmd-ai-ghost')).toHaveText('核对原始证据，再比较不同解释，');
    fixture.advance();
    await expect(page.locator('.readmd-ai-ghost')).toHaveText('核对原始证据，再比较不同解释，主动寻找能检验结论的反例。');
    fixture.advance();
    await expect(page.locator('.readmd-ai-ghost')).not.toHaveAttribute('data-streaming','true');
    await page.keyboard.press('Tab');await page.keyboard.press('Control+z');
    expect(await page.evaluate(()=>getEditContent())).toBe(before);expect(fixture.requests()).toBe(1);
  }finally{await fixture.stop();}
});

test('Tab accepts the currently visible partial candidate and cancels later chunks',async({page})=>{
  const fixture=await streamingEditor(page);
  try{
    const before=await page.evaluate(()=>getEditContent());await page.keyboard.press('Control+Alt+Space');
    await expect(page.locator('.readmd-ai-ghost')).toHaveText('核对原始证据，');
    await page.keyboard.press('Tab');fixture.advance();fixture.advance();fixture.advance();await page.waitForTimeout(500);
    expect(await page.evaluate(()=>getEditContent())).toBe(before+'核对原始证据，');
    await expect(page.locator('.readmd-ai-ghost')).toHaveCount(0);
    await page.keyboard.press('Control+z');expect(await page.evaluate(()=>getEditContent())).toBe(before);
  }finally{await fixture.stop();}
});

test('a disconnected upstream clears provisional ghost text and preserves the draft',async({page})=>{
  const fixture=await streamingEditor(page,{error:true});
  try{
    const before=await page.evaluate(()=>getEditContent());await page.keyboard.press('Control+Alt+Space');
    await expect(page.locator('.readmd-ai-ghost')).toHaveText('核对原始证据，');
    fixture.advance();
    await expect(page.locator('#status-ai-completion')).toContainText('补全暂不可用');
    await expect(page.locator('.readmd-ai-ghost')).toHaveCount(0);
    expect(await page.evaluate(()=>getEditContent())).toBe(before);
  }finally{await fixture.stop();}
});
