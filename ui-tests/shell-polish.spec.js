const {test,expect}=require('@playwright/test');
test.beforeEach(async({page})=>{
 await page.goto('/');await page.evaluate(()=>window.__readmdSessionReady);
 await page.evaluate(async()=>{await i18n.setLanguage('zh-CN',false);await renderVirtual('convert','阅读记录.md','','# 阅读记录\n\n核对证据后，再写下判断。',[]);});
});
test('brand returns home without closing pages and respects unsaved-change cancellation',async({page})=>{
 await page.locator('#titlebar-home').click();
 expect(await page.evaluate(()=>({mode:state.mode,pages:state.tabs.length}))).toEqual({mode:'welcome',pages:1});
 await page.locator('.tab-item').click();await page.evaluate(async()=>{await toggleEdit();cmView.dispatch({changes:{from:cmView.state.doc.length,insert:'\n尚未保存的结论。'}});});
 await page.locator('#titlebar-home').focus();await page.keyboard.press('Enter');
 await expect(page.locator('#close-confirm-modal')).toBeVisible();await page.locator('#close-confirm-cancel').click();
 expect(await page.evaluate(()=>getEditContent())).toContain('尚未保存');
 expect(await page.evaluate(()=>state.mode)).toBe('virtual');
});
test('command shortcut is only a hover hint and keyboard invocation still opens the palette',async({page})=>{
 await expect(page.locator('#btn-palette')).not.toContainText('Ctrl');
 await expect(page.locator('#btn-palette')).toHaveAttribute('title',/Ctrl\+K/);
 await page.keyboard.press('Control+k');await expect(page.locator('#command-palette-modal')).toBeVisible();
});
test('borderless composer remains usable with long input in both compact windows and all themes',async({page})=>{
 await page.evaluate(()=>toggleAiPanel());
 for(const size of [{width:1024,height:680},{width:1160,height:820}])for(const theme of ['light','dark','sepia']){
  await page.setViewportSize(size);await page.evaluate(theme=>{state.theme=theme;applySettings();},theme);
  await page.locator('#ai-prompt').fill('比较证据与解释。\n'.repeat(40));await page.locator('#ai-prompt').focus();
  await page.keyboard.press('Shift+Enter');expect(await page.locator('#ai-prompt').inputValue()).toContain('\n');
  const geometry=await page.evaluate(()=>{
   const wrapper=document.querySelector('.ai-composer-wrapper').getBoundingClientRect(),card=document.querySelector('.composer-input-card');
   const style=getComputedStyle(card),rect=card.getBoundingClientRect(),send=$('ai-run').getBoundingClientRect();
   return{border:style.borderWidth,radius:style.borderRadius,shadow:style.boxShadow,width:rect.width,wrapper:wrapper.width,send:{x:send.x,y:send.y,right:send.right,bottom:send.bottom},viewport:{width:innerWidth,height:innerHeight}};
  });
  expect(geometry.border).toBe('0px');expect(geometry.radius).toBe('0px');expect(geometry.shadow).toBe('none');expect(geometry.width).toBeCloseTo(geometry.wrapper,0);
  expect(geometry.send.x).toBeGreaterThanOrEqual(0);expect(geometry.send.y).toBeGreaterThanOrEqual(0);
  expect(geometry.send.right).toBeLessThanOrEqual(size.width);expect(geometry.send.bottom).toBeLessThanOrEqual(size.height);
 }
});
