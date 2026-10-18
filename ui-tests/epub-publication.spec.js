const {test,expect}=require('@playwright/test');
const {waitForPage}=require('./page-condition.cjs');
const fs=require('node:fs'),path=require('node:path'),os=require('node:os');
const zipParts=require('./zip-parts.cjs'),fixture=require('./epub-fixtures.cjs');
test.use({bypassCSP:false});let folder,file;
test.beforeEach(async({page})=>{folder=fs.mkdtempSync(path.join(os.tmpdir(),'readmd-epub-'));file=fixture.write(folder);await page.goto('/');await waitForPage(page,()=>window.__readmdAppReady);await page.evaluate(async file=>{await i18n.setLanguage('en',false);await loadFile(file);await openExportModal();},file);await page.locator('#export-tab-epub').click();await page.locator('#export-box .export-customize > summary').click();});
test.afterEach(()=>{expect(path.dirname(path.resolve(folder))).toBe(path.resolve(os.tmpdir()));expect(path.basename(folder).startsWith('readmd-epub-')).toBe(true);fs.rmSync(folder,{recursive:true,force:true});});
async function output(page,name='workshop.epub',extra={}){return page.evaluate(async({out,extra})=>{const r=await apiFetch('/api/export',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({format:'epub',content:currentExportContent(),baseDir:state.dir,out_path:out,options:{...collectExportOptions(),...extra}})});return r.json();},{out:path.join(folder,name),extra});}
test('EPUB cover controls create a real packaged vector cover with metadata and no side files',async({page})=>{
  const enabled=page.locator('[data-k="cover.enabled"]');await page.locator('#export-opts .exp-sec').filter({has:page.locator('[data-k="cover.enabled"]')}).locator('.exp-sec-head').click();await enabled.check();
  await page.locator('[data-k="cover.subtitle"]').fill('A careful reading workshop');expect((await output(page)).ok).toBe(true);
  const parts=zipParts(fs.readFileSync(path.join(folder,'workshop.epub')));expect(parts.get('OEBPS/content.opf')).toContain('properties="cover-image"');expect(parts.get('OEBPS/images/cover.svg')).toContain('2400 3200');expect(parts.get('OEBPS/cover.xhtml')).toContain('images/cover.svg');
  expect(parts.get('OEBPS/content.opf')).toContain('<dc:title>从观察到判断</dc:title>');expect(parts.get('OEBPS/chapter_1.xhtml')).toContain('id="fnref-1-2"');expect(parts.get('OEBPS/chapter_1.xhtml')).toContain('href="#fnref-1-2"');
  expect(fs.readdirSync(folder).sort()).toEqual(['Reading-workshop.md','edition.svg','references.bib','workshop.epub'].sort());expect(fs.readFileSync(file,'utf8')).toBe(fixture.source);
});
test('custom cover shares a packaged image and preserves an existing export when the source is unreadable',async({page})=>{
  await page.locator('[data-k="epub.cover"]').fill('edition.svg');await page.locator('[data-k="epub.title"]').fill('A reviewed edition');expect((await output(page)).ok).toBe(true);
  const saved=fs.readFileSync(path.join(folder,'workshop.epub')),parts=zipParts(saved);expect([...parts.keys()].filter(n=>n.startsWith('OEBPS/images/'))).toEqual(['OEBPS/images/cover.svg']);expect(parts.get('OEBPS/content.opf')).toContain('<dc:title>A reviewed edition</dc:title>');
  await page.locator('[data-k="epub.cover"]').fill('missing.png');expect((await output(page)).ok).toBe(false);expect(fs.readFileSync(path.join(folder,'workshop.epub'))).toEqual(saved);
});
test('single-document EPUB and browser-selected self-contained SVG use the same export path',async({page})=>{
  const picker=page.waitForEvent('filechooser');await page.locator('.exp-asset button').click();await(await picker).setFiles(path.join(folder,'edition.svg'));await expect(page.locator('[data-k="epub.cover"]')).toHaveValue(/^data:image\/svg\+xml;base64,/);
  await page.locator('[data-k="epub.splitLevel"]').selectOption('none');expect((await output(page)).ok).toBe(true);
  const parts=zipParts(fs.readFileSync(path.join(folder,'workshop.epub')));expect([...parts.keys()].filter(n=>/chapter_\d+\.xhtml$/.test(n))).toHaveLength(1);expect(parts.get('OEBPS/chapter_1.xhtml')).toContain('检验与复核');expect(parts.get('OEBPS/cover.xhtml')).toContain('images/cover.svg');
});

test('real EPUB preview keeps chapter and footnote navigation inside its sandbox',async({page})=>{
  await expect(page.locator('#export-preview-mini-content iframe')).toHaveCount(1);
  await page.locator('#export-preview-card').click();
  const frame=page.frameLocator('#export-preview-full-page iframe');
  await expect(frame.locator('article')).toHaveCount(3);
  await expect(page.locator('#export-preview-prev-btn')).toBeHidden();await expect(page.locator('#export-preview-next-btn')).toBeHidden();await expect(page.locator('#export-preview-pages-meta')).toBeEmpty();
  await frame.locator('nav a').last().click();
  const heading=frame.locator('[id="epub-chapter_2-h-检验与复核"]');
  await expect(heading).toBeInViewport();
  const note=frame.locator('a[role="doc-noteref"]').first();await note.click();
  const id=(await note.getAttribute('href')).slice(1),target=frame.locator('[id="'+id+'"]');
  await expect(target).toBeInViewport();await target.locator('a[role="doc-backlink"]').first().click();await expect(note).toBeInViewport();
  expect(await frame.locator('body').evaluate(()=>location.href)).toBe('about:srcdoc');
  await expect(frame.locator('math')).toHaveCount(1);
});

test('an invalid formula gives a localized EPUB preview error and preserves the source',async({page})=>{
  const invalid='# Draft\n\n```readmd-math v1\nnot_a_structure(x)\n```\n';
  await page.evaluate(async invalid=>{closeExportModal();await renderVirtual('clipboard','Draft.md','',invalid,[]);await openExportModal();},invalid);
  await page.locator('#export-tab-epub').click();const preview=page.locator('#export-preview-mini-content');
  await expect(preview).toContainText('Could not generate preview');await expect(preview).not.toContainText('math_unknown_macro');await expect(preview.locator('iframe')).toHaveCount(0);
  expect(await page.evaluate(()=>currentExportContent())).toBe(invalid);
});
