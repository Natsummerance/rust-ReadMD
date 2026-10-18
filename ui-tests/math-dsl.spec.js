const {test,expect}=require('@playwright/test');
const {waitForPage}=require('./page-condition.cjs');
test.use({bypassCSP:false});
test.beforeEach(async({page})=>{
  await page.route('**/api/update/check',r=>r.fulfill({json:{ok:false}}));
  await page.addInitScript(()=>localStorage.setItem('readmd_language','en'));
  await page.goto('/');await waitForPage(page,()=>window.__readmdAppReady);
});
async function edit(page,source){
  await page.evaluate(async source=>{await renderVirtual('clipboard','Math DSL.md','',source,[]);await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');},source);
  await expect(page.locator('#preview-pane .readmd-dsl-math[data-readmd-ready="true"]')).not.toHaveCount(0);
}
async function ready(page){await expect(page.locator('.formula-inline-canvas svg')).toBeVisible();await expect(page.locator('.formula-inline-controls button').first()).toBeEnabled();}

test('large DSL documents defer offscreen math and bound actual compiler concurrency',async({page})=>{
  let pending=0,maximum=0;
  page.on('request',r=>{if(r.url().endsWith('/api/math/parse')){pending++;maximum=Math.max(maximum,pending);}});
  const finish=r=>{if(r.url().endsWith('/api/math/parse'))pending--;};page.on('requestfinished',finish);page.on('requestfailed',finish);
  const source=Array.from({length:200},(_,i)=>'## Observation '+i+'\n\n```readmd-math v1\nfrac(x_'+i+', n + '+i+')\n```\n').join('\n');
  await edit(page,source);
  expect(await page.locator('#preview-pane .readmd-dsl-math[data-readmd-ready="true"]').count()).toBeLessThan(200);
  const last=page.locator('#preview-pane .readmd-dsl-math').last();await last.scrollIntoViewIfNeeded();
  await expect(last).toHaveAttribute('data-readmd-ready','true');
  expect(maximum).toBeLessThanOrEqual(4);
  expect(await page.locator('#preview-pane mjx-container mjx-container').count()).toBe(0);
});
test('discarded formula renders abandon their queued backend work before sending',async({page})=>{
  let requests=0;page.on('request',r=>{if(r.url().endsWith('/api/math/parse'))requests++;});
  const outcomes=await page.evaluate(async()=>{
    const owners=Array.from({length:40},(_,i)=>{const element=document.createElement('span');element.dataset.readmdDsl=encodeURIComponent('frac(abandoned_'+i+', n + 1)');$('content').append(element);return element;});
    const pending=owners.map(element=>compileDslMath(decodeURIComponent(element.dataset.readmdDsl),true,element));
    owners.slice(4).forEach(element=>element.remove());
    const results=await Promise.allSettled(pending);owners.forEach(element=>element.remove());
    return results.map(r=>r.status==='fulfilled'?'rendered':r.reason.name);
  });
  expect(outcomes.filter(v=>v==='rendered')).toHaveLength(4);expect(outcomes.filter(v=>v==='AbortError')).toHaveLength(36);
  expect(requests).toBe(4);
});

test('DSL renders standard properties and editing a repeated slot changes only its exact source',async({page})=>{
  const source='First :math[frac(a, b, style: skewed)].\n\nSecond :math[frac(a, b, style: skewed)].\n\n`:math[a / b]`\n';
  await edit(page,source);
  await expect(page.locator('#preview-pane .readmd-dsl-math')).toHaveCount(2);
  await expect(page.locator('#preview-pane code')).toHaveText(':math[a / b]');
  await page.locator('#preview-pane .readmd-dsl-math [data-readmd-math-renderer]').nth(1).click();await ready(page);
  await page.locator('.formula-inline').getByRole('button',{name:'Denominator',exact:true}).click();
  await page.locator('.formula-inline-input').fill('θ + 1');await page.locator('.formula-inline-input').press('Enter');await ready(page);
  expect(await page.evaluate(()=>getEditContent())).toBe(source);
  await page.locator('.formula-inline-controls button').first().click();
  const changed=source.replace('Second :math[frac(a, b, style: skewed)]','Second :math[frac(a, θ + 1, style: skewed)]');
  expect(await page.evaluate(()=>getEditContent())).toBe(changed);
  await page.evaluate(()=>cmUndo());expect(await page.evaluate(()=>getEditContent())).toBe(source);
  await page.evaluate(()=>cmRedo());expect(await page.evaluate(()=>getEditContent())).toBe(changed);
  await expect(page.locator('#preview-pane .readmd-dsl-math[data-readmd-ready="true"]')).toHaveCount(2);
  await page.evaluate(()=>mathTypesetQueue);
  await expect(page.locator('#preview-pane mjx-container mjx-container')).toHaveCount(0);
  await expect(page.locator('#preview-pane .readmd-dsl-math [data-readmd-math-renderer]')).toHaveCount(2);
  await page.locator('#preview-pane .readmd-dsl-math [data-readmd-math-renderer]').nth(1).click();await ready(page);
  await page.locator('.formula-inline-controls button').nth(1).click();
});
test('advanced Office edits stay readable and survive save without compressed metadata',async({page})=>{
  const source='```readmd-math v1\nfrac(a, b)\n```\n';await edit(page,source);
  await page.locator('#preview-pane .readmd-dsl-math [data-readmd-math-renderer]').click();await ready(page);
  await page.locator('.formula-properties > summary').click();await page.locator('[data-math-container="fPr"] > summary').click();
  await page.locator('[data-math-property="fPr.type"] select').selectOption('skw');await ready(page);
  await page.locator('.formula-inline-controls button').first().click();
  const updated=await page.evaluate(()=>getEditContent());
  expect(updated).toContain('```readmd-math v1');expect(updated).toContain('style: skewed');expect(updated).not.toContain('<!--readmd:math:');
  const checked=await page.evaluate(async()=>{const r=ReadMDFormula.ranges(getEditContent())[0];return(await(await apiFetch('/api/math/serialize',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({source:r.tex,dialect:'dsl',target:'omml',display:true})})).json()).output;});
  expect(checked).toContain('m:val="skw"');
  await page.keyboard.press('Control+z');expect(await page.evaluate(()=>getEditContent())).toBe(source);
});
test('an invalid local DSL slot is retained for correction and cannot overwrite the body',async({page})=>{
  const source=':math[frac(a, b)]';await edit(page,source);
  await page.locator('#preview-pane .readmd-dsl-math [data-readmd-math-renderer]').click();await ready(page);
  await page.locator('.formula-inline-input').fill('frac(');await page.locator('.formula-inline-input').press('Enter');
  await expect(page.locator('.formula-inline-message')).not.toHaveText('');
  await expect(page.locator('.formula-inline-controls button').first()).toBeDisabled();
  expect(await page.evaluate(()=>getEditContent())).toBe(source);
  await page.locator('.formula-inline-input').fill('α');await page.locator('.formula-inline-input').press('Enter');await ready(page);
  await page.locator('.formula-inline-controls button').first().click();expect(await page.evaluate(()=>getEditContent())).toBe(':math[frac(α, b)]');
});

test('leaving a valid formula flushes the final keystroke and commits exactly one undo without stealing focus',async({page})=>{
  const source=':math[frac(a, b)]\n\nKeep this paragraph.';await edit(page,source);
  await page.locator('#preview-pane .readmd-dsl-math [data-readmd-math-renderer]').click();await ready(page);
  await page.locator('.formula-inline-input').fill('θ');
  await page.locator('#preview-pane p').last().click();
  await expect(page.locator('.formula-inline')).toHaveCount(0);
  expect(await page.evaluate(()=>getEditContent())).toBe(':math[frac(θ, b)]\n\nKeep this paragraph.');
  await page.evaluate(()=>cmUndo());expect(await page.evaluate(()=>getEditContent())).toBe(source);
});

test('leaving an invalid formula keeps its editable draft and Escape cancels it',async({page})=>{
  const source=':math[frac(a, b)]\n\nKeep this paragraph.';await edit(page,source);
  await page.locator('#preview-pane .readmd-dsl-math [data-readmd-math-renderer]').click();await ready(page);
  await page.locator('.formula-inline-input').fill('frac(');
  await page.locator('#preview-pane p').last().click();
  await expect(page.locator('.formula-inline-message')).not.toHaveText('');
  await expect(page.locator('.formula-inline-input')).toHaveValue('frac(');
  expect(await page.evaluate(()=>getEditContent())).toBe(source);
  await page.locator('.formula-inline-input').focus();await page.keyboard.press('Escape');
  await expect(page.locator('.formula-inline')).toHaveCount(0);
  expect(await page.evaluate(()=>getEditContent())).toBe(source);
});
test('source symbol completion is automatic in DSL and stays quiet in ordinary prose and code examples',async({page})=>{
  await edit(page,':math[x]');
  await page.evaluate(()=>{cmView.focus();cmView.dispatch({changes:{from:6,to:7,insert:'alph'},selection:{anchor:10},userEvent:'input.type'});});
  const alpha=page.getByRole('option').filter({hasText:/^alpha\b/});await expect(alpha).toBeVisible();
  await alpha.click();expect(await page.evaluate(()=>getEditContent())).toBe(':math[α]');
  await page.evaluate(()=>{cmView.dispatch({changes:{from:0,to:cmView.state.doc.length,insert:'Ordinary alph'},selection:{anchor:13},userEvent:'input.type'});});
  await expect(page.getByRole('listbox')).toHaveCount(0);
  const contexts=await page.evaluate(()=>{
    const make=(text)=>{const s=ReadMDCodeMirror.EditorState.create({doc:text,extensions:[ReadMDCodeMirror.markdown()]});return cmMathContext({state:s,pos:text.indexOf('alph')+4});};
    return {dsl:make('```readmd-math v1\nalph\n```'),example:make('```markdown\nalph\n```'),inline:make('`:math[alph]`')};
  });
  expect(contexts.dsl?.query).toBe('alph');expect(contexts.example).toBeNull();expect(contexts.inline).toBeNull();
});
