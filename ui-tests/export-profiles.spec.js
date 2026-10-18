const {test,expect}=require('@playwright/test');
const {waitForPage}=require('./page-condition.cjs');
const fs=require('node:fs'),path=require('node:path'),os=require('node:os'),zipParts=require('./zip-parts.cjs'),fixture=require('./epub-fixtures.cjs');
test.use({bypassCSP:false});let folder;
test.beforeEach(async({page})=>{folder=fs.mkdtempSync(path.join(os.tmpdir(),'readmd-profiles-'));const file=fixture.write(folder);await page.goto('/');await waitForPage(page,()=>window.__readmdAppReady);await page.evaluate(async file=>{await i18n.setLanguage('en',false);await loadFile(file);await openExportModal();},file);await page.locator('#export-tab-pdf').click();});
test.afterEach(()=>{expect(path.dirname(path.resolve(folder))).toBe(path.resolve(os.tmpdir()));expect(path.basename(folder).startsWith('readmd-profiles-')).toBe(true);fs.rmSync(folder,{recursive:true,force:true});});
async function select(page,name){const details=page.locator('#exp-preset').locator('xpath=ancestor::details[1]');if(await details.count()&&!await details.evaluate(e=>e.open))await details.locator(':scope > summary').click();await page.locator('#exp-preset').selectOption(name);}
test('book, academic and common profiles share a compact format-aware picker',async({page})=>{
  await expect(page.locator('#exp-preset optgroup[label="Academic"] option')).toHaveCount(5);await expect(page.locator('#exp-preset optgroup[label="Books"] option')).toHaveCount(5);await expect(page.locator('#exp-preset optgroup[label="General purpose"] option')).toHaveCount(3);
  expect(await page.locator('#exp-preset-cards button').count()).toBeLessThanOrEqual(5);
  await select(page,'academic_thesis');expect(await page.evaluate(()=>collectExportOptions().typography.font)).toBe('SimSun');
  await page.locator('#export-tab-epub').click();await expect(page.locator('#exp-preset optgroup[label="Academic"]')).toHaveCount(0);await expect(page.locator('#exp-preset')).toHaveValue('__custom__');
  await select(page,'book_novel');await expect(page.locator('#exp-preset-cards [data-preset="book_novel"]')).toHaveAttribute('aria-pressed','true');
  await page.locator('#export-tab-docx').click();await expect(page.locator('#exp-preset')).toHaveValue('book_novel');expect(await page.evaluate(()=>collectExportOptions().page.size)).toBe('A5');
});
test('novel parameters render in the real EPUB and remain editable after choosing a preset',async({page})=>{
  test.setTimeout(120000); // A real 38 MiB offline import includes integrity and engine probes.
  await page.locator('#export-tab-epub').click();await select(page,'book_novel');
  await expect(page.locator('#export-preview-mini-content iframe')).toHaveCount(1);await page.locator('#export-preview-card').click();const frame=page.frameLocator('#export-preview-full-page iframe');
  await expect(frame.locator('section[epub\\:type="cover"] img')).toHaveCount(1);
  const first=frame.locator('#epub-chapter_1- > p').first();
  const actual=await first.evaluate(e=>({indent:parseFloat(getComputedStyle(e).textIndent),spacing:parseFloat(getComputedStyle(e).marginBottom),align:getComputedStyle(e).textAlign,font:getComputedStyle(e).fontFamily}));
  expect(actual.indent).toBeCloseTo(7.8*96/25.4,1);expect(actual.spacing).toBe(0);expect(actual.align).toBe('justify');expect(actual.font).toContain('SimSun');
  await page.locator('#export-preview-close').click();await page.locator('#export-box .export-customize > summary').click();
  const group=page.locator('#export-opts .exp-sec').filter({has:page.locator('[data-k="typography.firstLineIndent"]')});await group.locator('.exp-sec-head').click();await page.locator('[data-k="typography.firstLineIndent"]').fill('4');await page.locator('[data-k="typography.spacing"]').fill('3');
  const out=path.join(folder,'Novel.epub');const result=await page.evaluate(async({out,base})=>{return(await apiFetch('/api/export',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({format:'epub',content:currentExportContent(),baseDir:base,out_path:out,options:collectExportOptions()})})).json();},{out,base:folder});expect(result.ok).toBe(true);
  const css=zipParts(fs.readFileSync(out)).get('OEBPS/style.css');expect(css).toContain('text-indent:4mm');expect(css).toContain('margin:0 0 3pt');
  await page.locator('#export-tab-html').click();const htmlOut=path.join(folder,'Novel.html');
  const htmlResult=await page.evaluate(async({out,base})=>(await apiFetch('/api/export',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({format:'html',content:currentExportContent(),baseDir:base,out_path:out,options:collectExportOptions()})})).json(),{out:htmlOut,base:folder});
  expect(htmlResult.ok).toBe(true);const html=fs.readFileSync(htmlOut,'utf8');expect(html).toContain('text-indent:4mm');expect(html).toContain('SimSun');expect(html).toContain('检验与复核');expect(fs.readFileSync(path.join(folder,'Reading-workshop.md'),'utf8')).toBe(fixture.source);
  const isPdf=r=>r.url().endsWith('/api/export/preview')&&r.request().postDataJSON()?.format==='pdf';
  const pdfReady=page.waitForResponse(isPdf);await page.locator('#export-tab-pdf').click();await page.locator('#export-preview-card').click();let pdf=await(await pdfReady).json();
  if(!pdf.ok){
    // Use the same informed installer as a user, with a real locked offline
    // pack. Keep all PDF output assertions; absence is not a test skip.
    expect(pdf.error_code).toBe('publication_fonts_unavailable');
    const wrapper=page.locator('#export-preview-modal .export-preview-paper-wrapper');
    await expect(wrapper).toHaveAttribute('aria-busy','false');
    await expect(wrapper.locator('.task-loading')).toHaveCount(0);
    await wrapper.getByRole('button',{name:'Install and continue',exact:true}).click();
    await expect(page.locator('.capability-install-summary')).toContainText('38.1 MiB');
    expect(process.env.READMD_FONTS_PACK,'Set READMD_FONTS_PACK to the verified publication font pack').toBeTruthy();
    const resumed=page.waitForResponse(isPdf,{timeout:90000}),chooser=page.waitForEvent('filechooser');
    await page.locator('#capability-install-modal').getByRole('button',{name:'Import offline pack',exact:true}).click();
    await(await chooser).setFiles(process.env.READMD_FONTS_PACK);
    await expect(page.locator('#capability-install-modal')).toHaveCount(0,{timeout:90000});
    pdf=await(await resumed).json();
  }
  expect(pdf.ok).toBe(true);expect(pdf.pages).toBeGreaterThan(1);await expect(page.locator('#export-preview-next-btn')).toBeVisible();await expect(page.locator('#export-preview-next-btn')).toBeEnabled();
});
test('all 46 locale files translate the new profile names and categories',async({page})=>{
  const specs=JSON.parse(fs.readFileSync(path.join(__dirname,'../tools/export-profile-translations.json'))),locales=Object.keys(JSON.parse(fs.readFileSync(path.join(__dirname,'../assets/i18n/meta.json'))));
  for(const locale of locales){const text=JSON.parse(fs.readFileSync(path.join(__dirname,'../assets/i18n',locale+'.json')));for(const key of Object.keys(specs)){expect(typeof specs[key][locale],locale+' '+key).toBe('string');expect(text[key]).toBe(specs[key][locale]);}}
  await page.evaluate(()=>i18n.setLanguage('ar',false));await page.evaluate(()=>renderExportPresetSelect());await expect(page.locator('#exp-preset optgroup').first()).not.toHaveAttribute('label','General purpose');
  await page.evaluate(()=>renderExportSections());const labels=await page.evaluate(()=>['alignLeft','alignCenter','alignRight','alignJustify'].map(k=>i18n.t('export.'+k)));expect(await page.locator('[data-k="headings.h1.align"] option').allTextContents()).toEqual(labels);
});

test('a customized book profile persists across a real page reload without touching the manuscript',async({page})=>{
  await page.locator('#export-tab-epub').click();await select(page,'book_nonfiction');await page.locator('#exp-save-preset').click();
  await page.locator('#exp-save-input').fill('My book reading layout');await page.locator('#exp-save-ok').click();await expect(page.locator('#exp-save-name')).toBeHidden();
  await page.reload();await waitForPage(page,()=>window.__readmdAppReady);await page.evaluate(async file=>{await loadFile(file);await openExportModal();},path.join(folder,'Reading-workshop.md'));
  await expect(page.locator('#exp-preset')).toHaveValue('My book reading layout');await expect(page.locator('#export-tab-epub')).toHaveAttribute('aria-selected','true');
  expect(await page.evaluate(()=>collectExportOptions().epub.fontSize)).toBe(14);expect(fs.readFileSync(path.join(folder,'Reading-workshop.md'),'utf8')).toBe(fixture.source);
});

for(const [width,height] of [[1024,680],[1160,820]])test(`book profiles stay readable at ${width}×${height}`,async({page},info)=>{
  await page.setViewportSize({width,height});await page.evaluate(()=>i18n.setLanguage('de',false));
  await page.locator('#export-tab-epub').click();await select(page,'book_technical');
  await expect(page.locator('#exp-preset')).toHaveValue('book_technical');
  const layout=await page.locator('#export-box').evaluate(box=>({horizontal:box.scrollWidth-box.clientWidth,cards:[...box.querySelectorAll('.exp-preset-card')].map(card=>({width:card.clientWidth,overflow:card.scrollWidth-card.clientWidth,name:card.querySelector('.exp-preset-name').textContent}))}));
  expect(layout.horizontal).toBeLessThanOrEqual(1);expect(layout.cards.length).toBeLessThanOrEqual(5);expect(layout.cards.every(card=>card.width>=44&&card.overflow<=1&&card.name.length>0)).toBe(true);
  await waitForPage(page,()=>document.querySelector('#export-preview-mini-content iframe')?.contentDocument?.querySelector('article'));
  await page.locator('#toast').waitFor({state:'hidden'});
  await page.screenshot({path:info.outputPath('book-presets.png')});
});
