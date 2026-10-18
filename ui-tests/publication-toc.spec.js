// UI-layer regression. This uses the real test kernel with formal CSP, but
// does not certify Word layout, EPUB readers or paginated export output.
const {test,expect}=require('@playwright/test');
const {waitForPage}=require('./page-condition.cjs');
const fs=require('node:fs'),path=require('node:path');
test.use({bypassCSP:false,...(process.env.READMD_UI_BROWSER_CHANNEL?{channel:process.env.READMD_UI_BROWSER_CHANNEL}:{})});
const SOURCE='# Research outline\n\nEvidence from an original source.\n\n## Method\n\n### Comparison\n\n#### Detailed method\n\nThis fourth-level section stays in the document.\n';
async function customize(page){
  const details=page.locator('#export-box .export-customize');
  if(!await details.evaluate(e=>e.open))await details.locator(':scope > summary').click();
}
async function groupFor(page,key){
  const group=page.locator('#export-opts .exp-sec').filter({has:page.locator('[data-k="'+key+'"]')});
  if(!await group.evaluate(e=>e.classList.contains('open')))await group.locator('.exp-sec-head').click();
  return group;
}
async function start(page,locale='zh-CN'){
  await page.goto('/');await waitForPage(page,()=>window.__readmdAppReady);
  const policy=await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content');
  const scriptPolicy=policy.split(';').find(s=>s.trim().startsWith('script-src'));
  expect(scriptPolicy).toContain("'self'");expect(scriptPolicy).not.toContain("'unsafe-inline'");expect(scriptPolicy).not.toContain("'unsafe-eval'");
  await page.evaluate(async({source,locale})=>{await i18n.setLanguage(locale,false);await renderVirtual('clipboard','Research-outline.md','',source,[]);await openExportModal();},{source:SOURCE,locale});
  await page.locator('#export-tab-docx').click();await customize(page);
  const preset=page.locator('#exp-preset'),presetDetails=preset.locator('xpath=ancestor::details[1]');
  if(!await presetDetails.evaluate(e=>e.open))await presetDetails.locator(':scope > summary').click();
  await preset.selectOption('__default__');
  return groupFor(page,'toc.enabled');
}
async function enable(page){
  const checkbox=page.locator('[data-k="toc.enabled"]');
  if(!await checkbox.isChecked())await checkbox.check();
  await expect(page.locator('[data-k="toc.depth"]')).toBeVisible();
  await expect(page.locator('[data-k="toc.leader"]')).toBeVisible();
}

test('TOC toggles preserve open groups, focus and explicit overrides under formal CSP',async({page})=>{
  const group=await start(page),checkbox=page.locator('[data-k="toc.enabled"]');
  await expect(page.locator('[data-k="toc.depth"]')).toHaveCount(0);
  await expect(page.locator('[data-k="toc.leader"]')).toHaveCount(0);
  const before=await page.locator('#export-opts .exp-sec').evaluateAll(nodes=>nodes.map(n=>[n.dataset.section,n.classList.contains('open')]));
  await checkbox.focus();await page.keyboard.press('Space');
  await expect(checkbox).toBeFocused();await expect(group).toHaveClass(/open/);
  expect(await page.locator('#export-opts .exp-sec').evaluateAll(nodes=>nodes.map(n=>[n.dataset.section,n.classList.contains('open')]))).toEqual(before);
  const depth=page.locator('[data-k="toc.depth"]'),leader=page.locator('[data-k="toc.leader"]');
  await expect(depth).toHaveValue('3');await expect(depth).toHaveAttribute('min','1');await expect(depth).toHaveAttribute('max','6');
  await expect(leader).toHaveValue('dot');
  expect(await page.evaluate(()=>collectExportOptions().toc)).toEqual({enabled:true});
  await depth.fill('5');await leader.selectOption('hyphen');
  expect(await page.evaluate(()=>collectExportOptions().toc)).toMatchObject({enabled:true,depth:5,leader:'hyphen'});
  await checkbox.uncheck();await expect(depth).toHaveCount(0);await expect(leader).toHaveCount(0);await expect(group).toHaveClass(/open/);
  await checkbox.check();await expect(depth).toHaveValue('5');await expect(leader).toHaveValue('hyphen');
  expect(await page.evaluate(()=>currentExportContent())).toBe(SOURCE);
});

test('TOC controls reflect real format capabilities without discarding shared options',async({page})=>{
  await start(page);await enable(page);
  await page.locator('[data-k="toc.depth"]').fill('4');await page.locator('[data-k="toc.leader"]').selectOption('none');
  await page.locator('#export-tab-epub').click();await groupFor(page,'toc.depth');
  await expect(page.locator('[data-k="toc.depth"]')).toHaveValue('4');
  await expect(page.locator('[data-k="toc.enabled"]')).toHaveCount(0);await expect(page.locator('[data-k="toc.leader"]')).toHaveCount(0);
  await page.locator('#export-tab-pdf').click();
  for(const engine of ['rust','xelatex','pdflatex']){
    await page.evaluate(engine=>{state.export.options.pdf={engine};renderExportSections(true);},engine);
    await expect(page.locator('[data-k="toc.depth"]')).toHaveCount(0);await expect(page.locator('[data-k="toc.leader"]')).toHaveCount(0);
  }
  await page.evaluate(()=>{state.export.options.pdf={engine:'typst'};renderExportSections(true);});await groupFor(page,'toc.depth');
  await expect(page.locator('[data-k="toc.depth"]')).toHaveValue('4');await expect(page.locator('[data-k="toc.leader"]')).toHaveValue('none');
  await page.locator('#export-tab-docx').click();await groupFor(page,'toc.depth');
  expect(await page.evaluate(()=>collectExportOptions().toc)).toMatchObject({enabled:true,depth:4,leader:'none'});
});

test('TOC user preset sends real options and reloads without changing the document',async({page})=>{
  await start(page);await enable(page);
  await page.locator('[data-k="toc.depth"]').fill('6');await page.locator('[data-k="toc.leader"]').selectOption('underscore');
  await page.locator('#exp-save-preset').click();await page.locator('#exp-save-input').fill('TOC UI six levels');
  const saved=page.waitForResponse(r=>r.url().endsWith('/api/export/presets')&&r.request().method()==='POST');
  await page.locator('#exp-save-ok').click();const response=await saved;
  expect(response.ok()).toBe(true);expect((await response.json()).ok).toBe(true);
  const payload=response.request().postDataJSON();
  expect(payload.last.options.toc).toMatchObject({enabled:true,depth:6,leader:'underscore'});
  expect(payload.custom['TOC UI six levels'].toc).toEqual(payload.last.options.toc);
  await expect(page.locator('#exp-save-name')).toBeHidden();
  await page.reload();await waitForPage(page,()=>window.__readmdAppReady);
  await page.evaluate(async source=>{await renderVirtual('clipboard','Research-outline.md','',source,[]);await openExportModal();},SOURCE);
  await customize(page);await expect(page.locator('#exp-preset')).toHaveValue('TOC UI six levels');
  await expect(page.locator('#export-tab-docx')).toHaveAttribute('aria-selected','true');await groupFor(page,'toc.depth');
  await expect(page.locator('[data-k="toc.depth"]')).toHaveValue('6');await expect(page.locator('[data-k="toc.leader"]')).toHaveValue('underscore');
  expect(await page.evaluate(()=>currentExportContent())).toBe(SOURCE);
});

for(const [width,height]of [[1024,680],[1160,820]])test('TOC long labels and controls do not overlap at '+width+'×'+height,async({page})=>{
  await page.setViewportSize({width,height});await start(page,'de');await enable(page);
  const controls=page.locator('[data-k="toc.depth"], [data-k="toc.leader"]');
  await controls.first().scrollIntoViewIfNeeded();await controls.last().scrollIntoViewIfNeeded();
  const actual=await controls.evaluateAll(nodes=>nodes.map(n=>{
    const box=n.getBoundingClientRect(),label=n.closest('.exp-field').querySelector('label').getBoundingClientRect();
    return {left:box.left,right:box.right,top:box.top,bottom:box.bottom,width:box.width,labelBottom:label.bottom,labelWidth:label.width};
  }));
  for(const box of actual){expect(box.width).toBeGreaterThan(44);expect(box.labelWidth).toBeGreaterThan(0);expect(box.labelBottom).toBeLessThanOrEqual(box.top+1);expect(box.left).toBeGreaterThanOrEqual(0);expect(box.right).toBeLessThanOrEqual(width);}
  const [a,b]=actual;expect(Math.min(a.right,b.right)-Math.max(a.left,b.left)<=1||Math.min(a.bottom,b.bottom)-Math.max(a.top,b.top)<=1).toBe(true);
  expect(await page.locator('#export-box').evaluate(e=>e.scrollWidth-e.clientWidth)).toBeLessThanOrEqual(1);
  await controls.first().focus();await page.keyboard.press('Tab');await expect(controls.last()).toBeFocused();
});

test('all 46 locales include both TOC labels and Arabic controls use localized accessible names',async({page})=>{
  const read=file=>JSON.parse(fs.readFileSync(path.join(__dirname,'..',file),'utf8'));
  const spec=read('tools/i18n-batch3-toc.json'),locales=Object.keys(read('assets/i18n/meta.json'));expect(locales).toHaveLength(46);
  for(const locale of locales){const text=read('assets/i18n/'+locale+'.json');for(const[key,values]of Object.entries(spec)){expect(values[locale]?.trim(),locale+' '+key).toBeTruthy();expect(text[key]).toBe(values[locale]);}}
  await start(page,'ar');await enable(page);
  await expect(page.locator('[data-k="toc.depth"]')).toHaveAccessibleName(spec['export.tocDepth'].ar);
  await expect(page.locator('[data-k="toc.leader"]')).toHaveAccessibleName(spec['export.tocLeader'].ar);
  const none=await page.evaluate(()=>i18n.t('presentation.transitionNone'));
  await expect(page.locator('[data-k="toc.leader"] option[value="none"]')).toHaveText(none);
});
