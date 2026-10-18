const {waitForPage}=require('./page-condition.cjs');
const {test,expect}=require('@playwright/test');
test.beforeEach(async({page})=>{
  await page.route('**/api/update/check',r=>r.fulfill({json:{ok:false}}));
  await page.addInitScript(()=>localStorage.setItem('readmd_language','en'));
});
async function edit(page,src,position=src.length){
  await page.goto('/');await waitForPage(page, ()=>window.__readmdAppReady);
  await page.evaluate(async({src,position})=>{await renderVirtual('clipboard','equation.md','',src,[]);await toggleEdit();setEditorPref('aiCompletion',false);cmView.dispatch({selection:{anchor:position}});},{src,position});
}
async function ready(page){await expect(page.locator('#formula-preview > mjx-container > svg')).toBeVisible();await expect(page.locator('#formula-apply')).toBeEnabled();}
test('visual symbol replacement stages the whole equation and applies one undoable change',async({page})=>{
  const src='# Research\n\n';await edit(page,src);
  await page.locator('#formula-open').click();
  await expect(page.locator('[data-math-id="fraction"] .formula-template-preview svg')).toBeVisible();
  await expect(page.locator('[data-math-id="fraction"] small')).toBeHidden();
  await page.locator('[data-math-id="fraction"]').click();await ready(page);
  expect(await page.evaluate(()=>getEditContent())).toBe(src);
  await page.getByRole('button',{name:'Edit symbol a',exact:true}).click();
  await page.locator('#formula-search').fill('theta');
  await page.locator('[data-math-id="U+03B8"]').click();await ready(page);
  const dsl=await page.locator('#formula-source').inputValue();expect(dsl).toContain('frac(');expect(dsl).toContain('θ');
  await page.locator('#formula-apply').click();
  expect(await page.evaluate(()=>getEditContent())).toBe(src+':math['+dsl+']');
  await page.keyboard.press('Control+z');expect(await page.evaluate(()=>getEditContent())).toBe(src);
});
test('editing the second identical formula preserves the first and original delimiters',async({page})=>{
  const src='First $x^2$. Second \\(x^2\\).';await edit(page,src,src.lastIndexOf('x^2')+1);
  await page.locator('#formula-open').click();await ready(page);
  await page.getByRole('button',{name:'Edit symbol x',exact:true}).click();
  await page.locator('#formula-token').fill('y');await page.locator('#formula-token').press('Enter');await ready(page);
  await page.locator('#formula-apply').click();
  expect(await page.evaluate(()=>getEditContent())).toBe('First $x^2$. Second \\(y^2\\).');
});
test('live preview opens the exact second occurrence and code examples remain untouched',async({page})=>{
  const src='`$x^2$`\n\nFirst $x^2$.\n\nSecond $x^2$.';await edit(page,src);
  await page.evaluate(()=>setPvLayout('right'));
  await expect(page.locator('#preview-pane mjx-container')).toHaveCount(2);
  await page.locator('#preview-pane mjx-container').nth(1).dblclick();await ready(page);
  await page.getByRole('button',{name:'Edit symbol 2',exact:true}).click();
  await page.locator('#formula-token').fill('3');await page.locator('#formula-token-replace').click();await ready(page);
  await page.locator('#formula-apply').click();
  expect(await page.evaluate(()=>getEditContent())).toBe('`$x^2$`\n\nFirst $x^2$.\n\nSecond $x^3$.');
});
test('cancel, invalid input and changes in the underlying document never overwrite content',async({page})=>{
  const src='Equation $x+1$.';await edit(page,src,11);
  await page.locator('#formula-open').click();await ready(page);
  await page.locator('#formula-source-details summary').click();await page.locator('#formula-source').fill('\\frac{');
  await expect(page.locator('#formula-error')).not.toHaveText('');await expect(page.locator('#formula-apply')).toBeDisabled();
  await page.keyboard.press('Escape');expect(await page.evaluate(()=>getEditContent())).toBe(src);
  await page.locator('#formula-open').click();await ready(page);
  await page.evaluate(()=>cmView.dispatch({changes:{from:0,insert:'New '}}));
  await page.locator('#formula-apply').click();await expect(page.locator('#formula-error')).toContainText('changed');
  expect(await page.evaluate(()=>getEditContent())).toBe('New '+src);
});
test('catalog covers Unicode math and every published template renders with selectable symbols',async({page})=>{
  await edit(page,'');await page.locator('#formula-open').click();
  const catalog=await page.evaluate(()=>ReadMDFormula.load());expect(catalog.symbols.length).toBeGreaterThanOrEqual(2400);
  expect(catalog.symbols.some(s=>s.id==='U+1D6C2')).toBe(true);
  for(const template of catalog.templates){
    await page.evaluate(tex=>ReadMDFormula.insert(tex),template.tex);await ready(page);
    await expect(page.locator('#formula-preview [data-mml-node="merror"]')).toHaveCount(0);
    expect(await page.locator('#formula-preview [role="button"]').count()).toBeGreaterThan(0);
  }
  await page.locator('#formula-search').fill('U+1D6C2');await expect(page.locator('[data-math-id="U+1D6C2"]')).toBeVisible();
});
test('compact windows and primary languages retain a visible apply button without horizontal overflow',async({page})=>{
  await edit(page,'$x^2$',3);await page.locator('#formula-open').click();await ready(page);
  for(const [width,height]of[[1024,680],[1160,820],[390,844]]){
    await page.setViewportSize({width,height});
    for(const language of ['zh-CN','zh-TW','en','ja']){
      await page.evaluate(async language=>{await i18n.setLanguage(language,false);renderFormulaPicker();},language);
      const box=await page.locator('#formula-apply').boundingBox();expect(box.y+box.height).toBeLessThanOrEqual(height);
      expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1)).toBe(true);
      const labels=await page.locator('#formula-modal').innerText();expect(labels).not.toMatch(/formula\.(?:template|category|apply|visual)/);
    }
  }
});
test('closing during catalog loading prevents delayed rendering into another session',async({page})=>{
  let release;const gate=new Promise(r=>release=r);
  await page.route('**/assets/math-catalog.json',async route=>{await gate;await route.continue();});
  await edit(page,'$x+1$',2);
  await page.locator('#formula-open').click();await expect(page.locator('#formula-modal')).toBeVisible();
  await page.locator('#formula-close').click();release();
  await expect(page.locator('#formula-modal')).toBeHidden();
  expect(await page.evaluate(()=>getEditContent())).toBe('$x+1$');
  await page.locator('#formula-open').click();await ready(page);
});

test('unknown commands and oversized drafts cannot be applied; empty drafts remain disabled',async({page})=>{
  const src='# Untouched';await edit(page,src);await page.locator('#formula-open').click();
  await expect(page.locator('#formula-apply')).toBeDisabled();
  await page.evaluate(()=>ReadMDFormula.insert('\\ReadMDUnknownCommand{x}'));
  await expect(page.locator('#formula-error')).not.toHaveText('');
  await expect(page.locator('#formula-apply')).toBeDisabled();
  await page.evaluate(()=>ReadMDFormula.insert('x+'.repeat(9000)));
  await expect(page.locator('#formula-error')).toContainText('budget');
  await expect(page.locator('#formula-apply')).toBeDisabled();
  expect(await page.evaluate(()=>getEditContent())).toBe(src);
});

test('formula HTML annotations cannot create navigation or escape the preview bounds',async({page})=>{
  await edit(page,'');
  await page.evaluate(()=>ReadMDFormula.insert('\\href{javascript:alert(1)}{x}+\\style{position:fixed;top:0;width:99999px}{y}'));
  await ready(page);
  const unsafe=await page.locator('#formula-preview').evaluate(el=>Array.from(el.querySelectorAll('*')).filter(n=>
    /javascript:|https?:/i.test(n.getAttribute('href')||n.getAttribute('xlink:href')||'')||
    /position\s*:\s*(?:fixed|absolute)|99999/.test(n.getAttribute('style')||'')).map(n=>n.outerHTML));
  expect(unsafe).toEqual([]);
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1)).toBe(true);
});

test('ordinary numeric math renders while currency and fenced examples remain literal',async({page})=>{
  await edit(page,'Price $5 and $6.\n\nNumeric $2+2$.\n\n```text\n$x^2$\n```');
  await page.evaluate(()=>setPvLayout('right'));
  await expect(page.locator('#preview-pane mjx-container')).toHaveCount(1);
  await expect(page.locator('#preview-pane')).toContainText('Price $5 and $6.');
  await expect(page.locator('#preview-pane code')).toHaveText('$x^2$');
});
