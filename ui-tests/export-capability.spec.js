const {test,expect}=require('@playwright/test');
const {waitForPage}=require('./page-condition.cjs');
test.use({bypassCSP:false});
for(const [engine,id] of [['typst','typst'],['xelatex','latex'],['pdflatex','latex']]){
  test('missing '+engine+' offers installation in preview and core remains usable',async({page})=>{
    await page.goto('/');await waitForPage(page,()=>window.__readmdAppReady);
    await page.evaluate(async engine=>{
      await i18n.setLanguage('en',false);
      await renderVirtual('clipboard','Publication.md','','# Evidence\n\nA practical note with $x^2$.\n',[]);
      await openExportModal();state.export.options.pdf={engine};renderExportSections();
    },engine);
    await expect.poll(()=>page.evaluate(()=>nativeExportPreview.missingCapability?.id)).toBe(id);
    const install=page.locator('#export-preview-capability button');
    await expect(install).toHaveText('Install and continue');
    await expect(page.locator('#export-preview-capability')).toHaveAttribute('aria-busy','false');
    await expect(page.locator('#export-preview-card')).toBeHidden();
    const target=await install.boundingBox();expect(target.height).toBeGreaterThanOrEqual(44);
    expect(await install.evaluate(el=>Boolean(el.parentElement.closest('button')))).toBe(false);
    await install.click();
    if(id==='typst'){
      await expect(page.locator('#capability-install-modal')).toBeVisible();
      await page.locator('#capability-install-modal button').first().click();
      await expect(page.locator('#capability-install-modal')).toHaveCount(0);
    }
    await page.evaluate(()=>{state.export.options.pdf={engine:'rust'};renderExportSections();});
    await expect.poll(()=>page.evaluate(()=>nativeExportPreview.data?.pages||0)).toBeGreaterThan(0);
    expect(await page.evaluate(()=>nativeExportPreview.missingCapability)).toBeNull();
    await expect(page.locator('#export-preview-capability')).toHaveCount(0);
    await expect(page.locator('#export-preview-card')).toBeVisible();
    await expect(page.locator('#export-preview-mini-content img')).toBeVisible();
    await page.evaluate(()=>closeExportModal());
  });
}
