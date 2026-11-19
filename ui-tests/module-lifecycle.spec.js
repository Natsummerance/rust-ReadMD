const {test,expect}=require('@playwright/test');
const {waitForPage}=require('./page-condition.cjs');
const json=(route,body,status=200)=>route.fulfill({status,contentType:'application/json',body:JSON.stringify(body)});
test('failed engine initialization is reported immediately and a new attempt starts a real retry',async({page})=>{
  await page.goto('/');await waitForPage(page,()=>window.__readmdAppReady);
  let starts=0,polls=0;
  await page.route('**/api/modules/load',route=>{
    expect(route.request().postDataJSON()).toEqual({name:'convert',retry:true});starts++;
    return starts===1?json(route,{ok:false,status:'error',error_code:'module_unavailable'},503):json(route,{ok:true,status:'ready'});
  });
  await page.route('**/api/modules',route=>{polls++;return json(route,{modules:{convert:'ready'}});});
  expect(await page.evaluate(()=>ensureModule('convert',1500))).toBe(false);expect(polls).toBe(0);
  await expect(page.locator('#toast')).toContainText(await page.evaluate(()=>i18n.t('toast.moduleLoadFail',{name:i18n.t('menu.convert')})));
  expect(await page.evaluate(()=>ensureModule('convert',1500))).toBe(true);expect(starts).toBe(2);expect(polls).toBe(1);
});
for(const status of ['unavailable','disabled'])test(`a ${status} module stops polling and remains retryable`,async({page})=>{
  await page.goto('/');await waitForPage(page,()=>window.__readmdAppReady);
  let starts=0,polls=0;
  await page.route('**/api/modules/load',route=>{starts++;return json(route,{status:'loading'},202);});
  await page.route('**/api/modules',route=>{polls++;return json(route,{modules:{convert:starts===1?status:'ready'}});});
  expect(await page.evaluate(()=>ensureModule('convert',1500))).toBe(false);expect(polls).toBe(1);
  expect(await page.evaluate(()=>ensureModule('convert',1500))).toBe(true);expect(starts).toBe(2);expect(polls).toBe(2);
});
test('module details are available from the plugin entry and follow the language', async ({ page }) => {
  await page.goto('/');
  await waitForPage(page, () => window.__readmdAppReady);
  await page.evaluate(async () => {
    await i18n.setLanguage('zh-CN', false);
    state.modules = { convert:'ready', ocr:'loading', transcribe:'unavailable', pdf_editor:'error' };
    updateModuleUi();
  });
  await page.locator('#tab-settings').click();
  const entry = page.locator('#btn-plugin-menu');
  await expect(entry).toBeVisible();
  await expect(entry).toHaveAccessibleName(await page.evaluate(() => i18n.t('menu.plugins')));
  const chinese = await entry.getAttribute('aria-description');
  expect(chinese).toContain('语音识别');
  expect(chinese).toContain('编辑 PDF');
  expect(chinese).not.toMatch(/pdf_editor|transcribe/);
  await page.evaluate(() => i18n.setLanguage('en', false));
  const english = await entry.getAttribute('aria-description');
  expect(english).not.toBe(chinese);
  expect(english).not.toMatch(/pdf_editor|transcribe|系统引擎/);
  await expect(entry).toHaveAttribute('title', english);
  await expect(page.locator('#status-mods')).toHaveCount(0);
});
