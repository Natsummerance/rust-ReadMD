const {test,expect}=require('@playwright/test');
const {waitForPage}=require('./page-condition.cjs');
test.use({bypassCSP:false});
for(const [width,height]of [[360,740],[1024,680],[1160,820],[1440,900]])test(`title fits its text beside the logo and bounds long names at ${width}×${height}`,async({page},info)=>{
  await page.setViewportSize({width,height});await page.goto('/');await waitForPage(page,()=>window.__readmdAppReady);await page.evaluate(async()=>{await window.__readmdSessionReady;await goHome();});
  const title=page.locator('#window-title');await expect(title).toHaveText('ReadMD');
  const home=await page.locator('#window-drag-region').boundingBox();expect(home.width).toBeLessThan(80);
  const initial=await page.locator('#titlebar-actions').boundingBox();expect(initial.x-home.x-home.width).toBeLessThanOrEqual(8);
  const name='研究记录与证据分析_'.repeat(12)+'修订稿.md';await page.evaluate(name=>renderVirtual('clipboard',name,'','# Evidence\n\nDocument title layout.',[]),name);
  await expect(title).toHaveText(name);await expect(title).toHaveAttribute('title',name);
  const geometry=await page.evaluate(()=>{
    const logo=$('titlebar-home').getBoundingClientRect(),title=$('window-drag-region').getBoundingClientRect(),actions=$('titlebar-actions').getBoundingClientRect(),text=$('window-title');
    return{logoRight:logo.right,titleX:title.x,titleRight:title.right,titleWidth:title.width,actionsX:actions.x,textWidth:text.clientWidth,fullWidth:text.scrollWidth,overflow:getComputedStyle(text).textOverflow,barOverflow:$('app-titlebar').scrollWidth-$('app-titlebar').clientWidth};
  });
  expect(geometry.titleX).toBeGreaterThanOrEqual(geometry.logoRight);expect(geometry.titleX-geometry.logoRight).toBeLessThanOrEqual(8);expect(geometry.titleRight).toBeLessThanOrEqual(geometry.actionsX);expect(geometry.actionsX-geometry.titleRight).toBeLessThanOrEqual(8);expect(geometry.titleWidth).toBeLessThanOrEqual(240);expect(geometry.actionsX).toBeGreaterThan(initial.x);expect(geometry.overflow).toBe('ellipsis');expect(geometry.fullWidth).toBeGreaterThan(geometry.textWidth);expect(geometry.barOverflow).toBeLessThanOrEqual(1);
  await page.locator('#toast').waitFor({state:'hidden'});await page.screenshot({path:info.outputPath('titlebar.png')});
  await page.locator('#titlebar-home').click();await expect(title).toHaveText('ReadMD');
  expect((await page.locator('#titlebar-actions').boundingBox()).x).toBeCloseTo(initial.x,0);
});

for(const [width,height]of [[1024,680],[1160,820]])test(`reading settings follow edit and remain usable at ${width}×${height}`,async({page},info)=>{
  await page.setViewportSize({width,height});await page.goto('/');await waitForPage(page,()=>window.__readmdAppReady);
  await page.evaluate(async()=>{await window.__readmdSessionReady;await goHome();});
  await expect(page.locator('#rd-prefs-btn')).toBeHidden();
  await page.evaluate(async()=>{await window.__readmdSessionReady;await i18n.setLanguage('zh-CN',false);await renderVirtual('convert','阅读设置.md','','# 阅读设置\n\n检查字体、布局和专注阅读。',[]);});
  const button=page.locator('#rd-prefs-btn');await expect(button).toBeVisible();expect(await page.locator('#btn-edit').evaluate(e=>e.nextElementSibling.id)).toBe('rd-tools');await expect(page.locator('#app-titlebar #rd-tools')).toHaveCount(0);
  const before=await page.evaluate(()=>state.fontSize);await button.click();await expect(page.locator('#rd-prefs')).toBeVisible();
  const panel=await page.locator('#rd-prefs').boundingBox();expect(panel.x).toBeGreaterThanOrEqual(0);expect(panel.x+panel.width).toBeLessThanOrEqual(width);expect(panel.y+panel.height).toBeLessThanOrEqual(height);
  await page.locator('.rd-size-btn--inc').click();expect(await page.evaluate(()=>state.fontSize)).toBeGreaterThan(before);await page.keyboard.press('Escape');await expect(page.locator('#rd-prefs')).toBeHidden();await expect(button).toBeFocused();
  await page.mouse.move(4,220);
  const styles=await page.evaluate(()=>['btn-search','btn-print','btn-edit','rd-prefs-btn'].map(id=>{const el=$(id),s=getComputedStyle(el),r=el.getBoundingClientRect();return{id,border:s.borderColor,background:s.backgroundColor,shadow:s.boxShadow,width:r.width,height:r.height};}));
  for(const style of styles){expect(style.border).toBe(styles[0].border);expect(style.background).toBe(styles[0].background);expect(style.shadow).toBe(styles[0].shadow);expect(style.width).toBe(44);expect(style.height).toBe(44);}
  await page.locator('#btn-edit').click();await expect(button).toBeHidden();await expect(page.locator('#edit-bar')).toBeVisible();
  await page.locator('#edit-cancel').click();await expect(button).toBeVisible();
  await page.locator('#toast').waitFor({state:'hidden'});await page.screenshot({path:info.outputPath('reading-settings.png')});
  await button.click();await expect(page.locator('#rd-prefs')).toBeVisible();await page.locator('#titlebar-home').click();
  await expect(button).toBeHidden();await expect(page.locator('#rd-prefs')).toBeHidden();await expect(button).toHaveAttribute('aria-expanded','false');
  await page.locator('.tab-item').first().click();await expect(button).toBeVisible();
  await page.locator('.tab-close').first().click();await expect(button).toBeHidden();await expect(page.locator('#close-confirm-modal')).toBeHidden();
});
