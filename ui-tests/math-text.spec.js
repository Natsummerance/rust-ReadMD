const {test,expect}=require('@playwright/test');
const {waitForPage}=require('./page-condition.cjs');
test.use({bypassCSP:false});
const run='office.r(word.rPr(word.color(val: "336699")), office.t("ABA𝛂Aé"))';
const body='# Office character review\n\n😀 Author note.\n\nFirst :math['+run+']\n\nSecond :math['+run+']\n\nKeep this paragraph.\n';
async function start(page,open=true){await page.goto('/');await waitForPage(page,()=>window.__readmdAppReady);await page.evaluate(async body=>{await i18n.setLanguage('en',false);await renderVirtual('clipboard','Character-review.md','',body,[]);await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');},body);await expect(page.locator('#preview-pane .readmd-dsl-math[data-readmd-ready="true"]')).toHaveCount(2);if(open){await page.locator('#preview-pane [data-readmd-math-renderer]').nth(1).click();await ready(page);}}
async function ready(page){await expect(page.locator('.formula-inline-controls button').first()).toBeEnabled();}
const text=(page,from)=>page.locator('.formula-inline-canvas [data-math-span][data-math-text-from="'+from+'"]');

test('the first click on a painted Office glyph opens that exact character without an intermediate selection',async({page})=>{
 await start(page,false);
 const painted=page.locator('#preview-pane [data-readmd-math-renderer]').nth(1).locator('[data-math-owners]').filter({has:page.locator('path')});
 const index=await painted.evaluateAll(nodes=>nodes.findIndex(n=>/-text-3(?: |$)/.test(n.getAttribute('data-math-owners'))));expect(index).toBeGreaterThanOrEqual(0);
 await painted.nth(index).scrollIntoViewIfNeeded();const box=await painted.nth(index).boundingBox();expect(box.width).toBeGreaterThan(0);await page.mouse.click(box.x+box.width/2,box.y+box.height/2);await ready(page);await expect(page.locator('.formula-inline-input')).toHaveValue('𝛂');await page.keyboard.press('Escape');expect(await page.evaluate(()=>getEditContent())).toBe(body);
});
test('double-clicking a Rust-rendered formula keeps its inline editor available',async({page})=>{
 await start(page,false);await page.locator('#preview-pane [data-readmd-math-renderer]').nth(1).dblclick();await ready(page);await text(page,2).click();await expect(page.locator('.formula-inline-input')).toHaveValue('A');await page.keyboard.press('Escape');expect(await page.evaluate(()=>getEditContent())).toBe(body);
});

test('a real formula redraw during the click delay retains the exact second formula target',async({page})=>{
 await start(page,false);
 const replaced=await page.evaluate(async()=>{const original=document.querySelectorAll('#preview-pane [data-readmd-math-renderer]')[1],root=original.closest('.readmd-dsl-math');original.click();root.style.fontSize='19px';delete root.dataset.readmdReady;await renderDslMath(root);return !original.isConnected;});
 expect(replaced).toBe(true);await ready(page);await text(page,2).click();await page.locator('.formula-inline-input').fill('θ');await page.locator('.formula-inline-input').press('Enter');await ready(page);await page.locator('.formula-inline-controls button').first().click();expect(await page.evaluate(()=>getEditContent())).toBe(body.replace('Second :math['+run+']','Second :math['+run.replace('ABA𝛂Aé','ABθ𝛂Aé')+']'));
});

test('a pending formula click cannot bind to a newly changed body',async({page})=>{
 await start(page,false);await page.evaluate(()=>{document.querySelectorAll('#preview-pane [data-readmd-math-renderer]')[1].click();cmView.dispatch({changes:{from:0,insert:'Changed author text.\n\n'},userEvent:'input.type'});});
 await page.waitForTimeout(350);await expect(page.locator('.formula-inline-controls')).toHaveCount(0);expect(await page.evaluate(()=>getEditContent())).toBe('Changed author text.\n\n'+body);
});
test('Office multi-character runs edit the chosen repeated symbol and retain style and Unicode clusters',async({page})=>{
 await start(page);await expect(text(page,6)).toHaveAttribute('data-math-text-to','8');await expect(text(page,7)).toHaveCount(0);
 await text(page,2).click();await expect(page.locator('.formula-inline-input')).toHaveValue('A');await page.locator('.formula-inline-input').fill('θ');await page.locator('.formula-inline-input').press('Enter');await ready(page);
 await text(page,3).click();await expect(page.locator('.formula-inline-input')).toHaveValue('𝛂');await page.locator('.formula-inline-input').fill('βγ');await page.locator('.formula-inline-input').press('Enter');await ready(page);
 await text(page,6).focus();await page.keyboard.press('Enter');await expect(page.locator('.formula-inline-input')).toHaveValue('é');await page.locator('.formula-inline-input').fill('é');await page.locator('.formula-inline-input').press('Enter');await ready(page);
 expect(await page.evaluate(()=>getEditContent())).toBe(body);await page.locator('.formula-inline-controls button').first().click();
 const changed=await page.evaluate(()=>getEditContent());expect(changed).toBe(body.replace('Second :math['+run+']','Second :math['+run.replace('ABA𝛂Aé','ABθβγAé')+']'));
 expect(changed.match(/336699/g)).toHaveLength(2);await page.keyboard.press('Control+z');expect(await page.evaluate(()=>getEditContent())).toBe(body);await page.keyboard.press('Control+y');expect(await page.evaluate(()=>getEditContent())).toBe(changed);
});
test('clicking a later character flushes a length-changing edit and rebinds its original logical target',async({page})=>{
 await start(page);await text(page,2).click();await page.locator('.formula-inline-input').fill('pqrs');
 await text(page,5).click();await expect(page.locator('.formula-inline-input')).toHaveValue('A');await page.locator('.formula-inline-input').fill('ω');await page.locator('.formula-inline-input').press('Enter');await ready(page);
 await page.locator('.formula-inline-controls button').first().click();expect(await page.evaluate(()=>getEditContent())).toBe(body.replace('Second :math['+run+']','Second :math['+run.replace('ABA𝛂Aé','ABpqrs𝛂ωé')+']'));
});
test('invalid character input keeps the body and exact text range for correction or cancellation',async({page})=>{
 await start(page);await text(page,3).click();await page.locator('.formula-inline-input').fill('\u0001');await page.locator('.formula-inline-input').press('Enter');
 await expect(page.locator('.formula-inline-message')).not.toHaveText('');await expect(page.locator('.formula-inline-controls button').first()).toBeDisabled();expect(await page.evaluate(()=>getEditContent())).toBe(body);
 await page.locator('.formula-inline-input').fill('δ');await page.locator('.formula-inline-input').press('Enter');await ready(page);await page.keyboard.press('Escape');expect(await page.evaluate(()=>getEditContent())).toBe(body);
});
