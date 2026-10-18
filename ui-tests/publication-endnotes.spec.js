const {test,expect}=require('@playwright/test');
const {waitForPage}=require('./page-condition.cjs');
test.use({bypassCSP:false});
async function start(page){
  await page.goto('/');await waitForPage(page,()=>window.__readmdAppReady);
  await page.evaluate(async()=>{await i18n.setLanguage('zh-CN',false);await renderVirtual('clipboard','Study.md','','# Study\n\nClaim[^n].\n\n[^n]: Evidence.\n',[]);await openExportModal();});
  await page.locator('#export-tab-epub').click();
  const customize=page.locator('#export-box .export-customize');if(!await customize.evaluate(e=>e.open))await customize.locator('summary').click();
  const group=page.locator('.exp-sec').filter({has:page.locator('[data-k="footnotes.position"]')});if(!await group.evaluate(e=>e.classList.contains('open')))await group.locator('.exp-sec-head').click();
  return group;
}
for(const[width,height]of [[1024,680],[1160,820]])test(`endnote options remain open and keyboard accessible at ${width}×${height}`,async({page})=>{
  await page.setViewportSize({width,height});const group=await start(page),position=page.locator('[data-k="footnotes.position"]');
  await expect(position).toHaveValue('page');await expect(page.locator('[data-k="footnotes.title"]')).toHaveCount(0);
  await position.focus();await position.selectOption('document');await expect(group).toHaveClass(/open/);await expect(position).toBeFocused();
  const title=page.locator('[data-k="footnotes.title"]'),localized=await page.evaluate(()=>i18n.t('export.noteEndnotes'));
  await expect(title).toHaveValue(localized);expect(await page.evaluate(()=>collectExportOptions().footnotes.title)).toBe(localized);
  await title.fill('研究注释');await page.locator('[data-k="footnotes.numberFormat"]').selectOption('lowerRoman');
  expect(await page.evaluate(()=>collectExportOptions().footnotes)).toMatchObject({position:'document',title:'研究注释',numberFormat:'lowerRoman'});
  expect(await page.locator('#export-box').evaluate(e=>e.scrollWidth-e.clientWidth)).toBeLessThanOrEqual(1);
  await position.selectOption('page');await expect(page.locator('[data-k="footnotes.title"]')).toHaveCount(0);await expect(group).toHaveClass(/open/);
});
test('legacy defaults stay absent; unsupported PDF engines hide endnote controls',async({page})=>{
  await start(page);expect(await page.evaluate(()=>collectExportOptions().footnotes?.position)).toBeUndefined();
  await page.locator('#export-tab-docx').click();await expect(page.locator('[data-k="footnotes.position"]')).toHaveCount(1);await expect(page.locator('[data-k="footnotes.title"]')).toHaveCount(0);
  await page.locator('#export-tab-pdf').click();await page.evaluate(()=>{state.export.options.pdf={engine:'rust'};renderExportSections();});await expect(page.locator('[data-k="footnotes.position"]')).toHaveCount(0);
  await page.evaluate(()=>{state.export.options.pdf={engine:'typst'};renderExportSections();});await expect(page.locator('[data-k="footnotes.position"]')).toHaveCount(1);
});
test('all locale files contain the new note controls and actionable nested-note error',async({page})=>{
  const fs=require('node:fs'),path=require('node:path'),translations=require('../tools/publication-endnote-translations.json');
  const locales=Object.keys(require('../assets/i18n/meta.json'));expect(locales.length).toBe(46);
  for(const locale of locales){const values=JSON.parse(fs.readFileSync(path.join(__dirname,'../assets/i18n',locale+'.json')));for(const[key,strings]of Object.entries(translations)){expect(values[key],locale+' '+key).toBe(strings[locale]);expect(strings[locale]?.trim()).toBeTruthy();}}
  await start(page);await page.evaluate(async()=>{await i18n.setLanguage('ar',false);renderExportSections();});
  expect(await page.locator('[data-k="footnotes.position"] option').allTextContents()).toEqual(await page.evaluate(()=>[i18n.t('export.noteFootnotes'),i18n.t('export.noteEndnotes')]));
});
