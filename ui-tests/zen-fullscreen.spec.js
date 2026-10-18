const {test,expect}=require('@playwright/test');
const {waitForPage}=require('./page-condition.cjs');
test.beforeEach(async({page})=>{
  await page.goto('/');await waitForPage(page,()=>window.ReadMDViewMode&&window.__readmdAppReady);
  await page.evaluate(()=>renderVirtual('clipboard','Focus.md','','# Focus\n\nKeep this content unchanged.',[]));
});
async function reveal(page){
  await page.mouse.move(600,160);await page.mouse.move(200,3);
  await expect.poll(()=>page.locator('#btn-zen').evaluate(e=>{const r=e.getBoundingClientRect();return r.height>=44&&r.top>=0;})).toBe(true);
}
test('left click changes Zen only, right click enables fullscreen and left click can show chrome',async({page})=>{
  await page.locator('#btn-zen').click();await expect(page.locator('body')).toHaveClass(/zen-mode/);
  expect(await page.evaluate(()=>!!document.fullscreenElement)).toBe(false);
  await page.keyboard.press('Escape');await expect(page.locator('body')).not.toHaveClass(/zen-mode/);
  await page.locator('#btn-zen').click({button:'right'});await expect.poll(()=>page.evaluate(()=>!!document.fullscreenElement)).toBe(true);
  await expect(page.locator('#btn-zen')).toHaveAttribute('aria-pressed','true');
  await reveal(page);await page.locator('#btn-zen').click();await expect(page.locator('body')).not.toHaveClass(/zen-mode/);
  expect(await page.evaluate(()=>!!document.fullscreenElement)).toBe(true);
  await page.locator('#btn-zen').click({button:'right'});await expect.poll(()=>page.evaluate(()=>!!document.fullscreenElement)).toBe(false);
  await expect(page.locator('body')).not.toHaveClass(/zen-mode/);
});
test('F11 enters fullscreen with Zen and Escape exits pure fullscreen without closing the editor',async({page})=>{
  await page.evaluate(()=>toggleEdit());const before=await page.evaluate(()=>getEditContent());
  await page.keyboard.press('F11');await expect.poll(()=>page.evaluate(()=>!!document.fullscreenElement)).toBe(true);
  await expect(page.locator('body')).toHaveClass(/zen-mode/);await reveal(page);await page.locator('#btn-zen').click();
  await expect(page.locator('body')).not.toHaveClass(/zen-mode/);
  expect(await page.evaluate(()=>!!document.fullscreenElement)).toBe(true);
  await page.keyboard.press('Escape');await expect.poll(()=>page.evaluate(()=>!!document.fullscreenElement)).toBe(false);
  expect(await page.evaluate(()=>state.editing)).toBe(true);expect(await page.evaluate(()=>getEditContent())).toBe(before);
  await expect(page.locator('#close-confirm-modal')).toBeHidden();
});
test('F11 exits both modes and Esc leaves a modal open while exiting fullscreen',async({page})=>{
  await page.keyboard.press('F11');await expect.poll(()=>page.evaluate(()=>!!document.fullscreenElement)).toBe(true);
  await page.keyboard.press('F11');await expect.poll(()=>page.evaluate(()=>!!document.fullscreenElement)).toBe(false);
  await expect(page.locator('body')).not.toHaveClass(/zen-mode/);
  await page.keyboard.press('F11');await expect.poll(()=>page.evaluate(()=>!!document.fullscreenElement)).toBe(true);
  await page.evaluate(()=>openExportModal());await page.keyboard.press('Escape');
  await expect.poll(()=>page.evaluate(()=>!!document.fullscreenElement)).toBe(false);
  await expect(page.locator('#export-modal')).toBeVisible();await expect(page.locator('body')).not.toHaveClass(/zen-mode/);
});
