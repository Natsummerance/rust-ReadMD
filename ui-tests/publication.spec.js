const {waitForPage}=require('./page-condition.cjs');
const {test,expect}=require('@playwright/test');
const fs=require('node:fs'),os=require('node:os'),path=require('node:path');
test.use({bypassCSP:false});
test('real local bibliography is consistent in reading, preview, metadata edits and export',async({page})=>{
  const folder=fs.mkdtempSync(path.join(os.tmpdir(),'readmd-publication-'));
  const file=path.join(folder,'Research.md'),bib=path.join(folder,'references.bib'),original='---\ntitle: Evidence and reasoning\nbibliography: references.bib\ncsl: apa\nlang: en-US\n---\n# Evidence and reasoning\n\nCompare the original argument [@kant, p. 42].\n\nExample: `[@kant]`.\n\nA formula $x^2$.\n';
  fs.writeFileSync(file,original);fs.writeFileSync(bib,'@book{kant,title={Critique of Pure Reason},author={Kant, Immanuel},year={1781},publisher={Cambridge University Press}}');
  try{
    await page.goto('/');await waitForPage(page, ()=>window.__readmdAppReady);await page.evaluate(async file=>{await i18n.setLanguage('en',false);await loadFile(file);},file);
    await expect(page.locator('#content')).toContainText('Kant, 1781');await expect(page.locator('#content')).toContainText('42');await expect(page.locator('#content h2')).toHaveText('References');await expect(page.locator('#content em')).toContainText('Critique of Pure Reason');await expect(page.locator('#content code')).toHaveText('[@kant]');
    await page.evaluate(async()=>{await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');});await expect(page.locator('#preview-pane')).toContainText('Kant, 1781');await expect(page.locator('#preview-pane')).not.toContainText('bibliography:');await expect(page.locator('#preview-pane mjx-container')).toHaveCount(1);expect(await page.evaluate(()=>getEditContent())).toBe(original);
    await page.evaluate(()=>openFrontmatterModal());await page.locator('.metadata-bibliography > summary').click();await page.locator('#fm-input-csl').fill('ieee');await page.locator('#fm-modal-insert').click();
    await expect(page.locator('#preview-pane')).toContainText('[1]');await expect(page.locator('#preview-pane mjx-container')).toHaveCount(1);expect(fs.readFileSync(file,'utf8')).toBe(original);await page.keyboard.press('Control+z');expect(await page.evaluate(()=>getEditContent())).toBe(original);
    const prepared=await page.evaluate(async({content,baseDir})=>{const r=await apiFetch('/api/academic/document',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({content,baseDir})});return r.json();},{content:original,baseDir:folder});expect(prepared.active).toBe(true);expect(prepared.content).toContain('*Critique of Pure Reason*');expect(prepared.diagnostics).toEqual([]);
    for(const format of ['docx','pdf','epub']){
      const output=path.join(folder,'Research.'+format);const result=await page.evaluate(async args=>{const r=await apiFetch(args.format==='epub'?'/api/export/epub':'/api/export',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({...args,confirm:true})});return r.json();},{format,content:original,baseDir:folder,out_path:output,suggestedName:'Research',options:{}});expect(result.ok).toBe(true);expect(fs.statSync(output).size).toBeGreaterThan(500);
    }
    expect(fs.readFileSync(file,'utf8')).toBe(original);expect(fs.readdirSync(folder).sort()).toEqual(['Research.docx','Research.epub','Research.md','Research.pdf','references.bib'].sort());
  }finally{fs.rmSync(folder,{recursive:true,force:true});}
});

test('academic blocks are translated and never consume fenced examples',async({page})=>{
  await page.goto('/');await waitForPage(page, ()=>window.__readmdAppReady);
  const source='```md\n::: theorem [An example, not a block]\nKeep this literal.\n:::\n```\n\n::: theorem [<img src=x onerror=alert(1)>]\nFor $x>0$, we have **positive evidence**.\n:::\n\n::: proof\nThe conclusion follows.\n:::\n';
  await page.evaluate(async source=>{await i18n.setLanguage('zh-CN',false);await renderVirtual('clipboard','学术块.md','',source,[]);},source);
  await expect(page.locator('#content .academic-theorem')).toHaveCount(1);await expect(page.locator('#content .academic-theorem .academic-callout-tag')).toHaveText('定理');
  await expect(page.locator('#content .academic-proof .academic-callout-tag')).toHaveText('证明');await expect(page.locator('#content pre code')).toContainText('::: theorem [An example, not a block]');await expect(page.locator('#content .academic-theorem img')).toHaveCount(0);await expect(page.locator('#content .academic-theorem mjx-container')).toHaveCount(1);
  await page.evaluate(async source=>{await i18n.setLanguage('ja',false);await renderVirtual('clipboard','学術.md','',source,[]);},source);await expect(page.locator('#content .academic-theorem .academic-callout-tag')).toHaveText('定理');
});
