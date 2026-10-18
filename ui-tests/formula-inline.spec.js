const {waitForPage}=require('./page-condition.cjs');
const {test,expect}=require('@playwright/test');
test.use({bypassCSP:false});
test.beforeEach(async({page})=>{
  await page.route('**/api/update/check',r=>r.fulfill({json:{ok:false}}));
  await page.addInitScript(()=>localStorage.setItem('readmd_language','en'));
});
async function edit(page,source){
  await page.goto('/');await waitForPage(page, ()=>window.__readmdAppReady);
  await page.evaluate(async source=>{await renderVirtual('clipboard','Math.md','',source,[]);await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');},source);
  await expect(page.locator('#preview-pane mjx-container')).not.toHaveCount(0);
}
async function ready(page){await expect(page.locator('.formula-inline-canvas svg')).toBeVisible();await expect(page.locator('.formula-inline-controls button').first()).toBeEnabled();}

test('duplicate preview refresh during asynchronous preparation cannot leave stale visible math',async({page})=>{
  await edit(page,'$x+1$');
  await page.evaluate(()=>{
    const prepare=ReadMDPublication.prepare,gate=new Promise(resolve=>window.__releasePreview=resolve);
    ReadMDPublication.prepare=async(...args)=>{await gate;return prepare(...args);};
    cmView.dispatch({changes:{from:1,to:2,insert:'y'}});
    window.__firstPreview=renderPreview();window.__secondPreview=renderPreview();
  });
  expect(await page.locator('#preview-pane mjx-container').textContent()).toContain('x');
  await page.evaluate(async()=>{window.__releasePreview();await Promise.all([window.__firstPreview,window.__secondPreview]);});
  await expect.poll(()=>page.locator('#preview-pane mjx-container').textContent()).toContain('y');
  expect(await page.evaluate(()=>getEditContent())).toBe('$y+1$');
});

test('incremental preview edits keep source order, repeated occurrences and visible undo in sync',async({page})=>{
  const source='First $\\frac{a}{b}$.\n\nSecond $\\frac{a}{b}$.\n\nLast $z$.';await edit(page,source);
  const rendered=()=>page.evaluate(()=>Array.from(document.querySelectorAll('#preview-pane mjx-container')).map(root=>({tex:ReadMDInlineMath.locate(root)?.tex,text:root.textContent})));
  await page.locator('#preview-pane mjx-container').first().click();await ready(page);
  await page.locator('.formula-inline-input').fill('q');await page.locator('.formula-inline-input').press('Enter');await ready(page);
  await page.locator('.formula-inline-controls button').first().click();
  await expect.poll(async()=>(await rendered()).map(r=>r.tex)).toEqual(['\\frac{q}{b}','\\frac{a}{b}','z']);
  expect((await rendered())[0].text).toContain('q');
  // The updated first MathJax item is appended after the unchanged items.
  // Opening it again and targeting the identical second expression must work.
  await page.locator('#preview-pane mjx-container').first().click();await ready(page);
  await expect(page.locator('.formula-inline-input')).toHaveValue('q');await page.keyboard.press('Escape');
  await page.locator('#preview-pane mjx-container').nth(1).dblclick();
  await expect(page.locator('#formula-source')).toHaveValue('\\frac{a}{b}');await page.keyboard.press('Escape');
  await page.keyboard.press('Control+z');
  await expect.poll(async()=>(await rendered()).map(r=>r.tex)).toEqual(['\\frac{a}{b}','\\frac{a}{b}','z']);
  expect((await rendered())[0].text).toContain('a');
  await page.keyboard.press('Control+y');
  await expect.poll(async()=>(await rendered()).map(r=>r.tex)).toEqual(['\\frac{q}{b}','\\frac{a}{b}','z']);
  expect((await rendered())[0].text).toContain('q');
});

test('block matrix properties open at the matrix while paragraph properties remain available',async({page})=>{
  await edit(page,'# Review\n\n$$\n\\begin{matrix}12&0.84\\\\128&0.96\\end{matrix}\n$$\n');
  await page.locator('#preview-pane mjx-container').click();await ready(page);await page.locator('.formula-properties > summary').click();
  await expect(page.locator('[data-math-matrix-alignment]')).toBeVisible();
  const structure=page.locator('.formula-property-body > select');
  const selected=await structure.evaluate(s=>s.selectedOptions[0].title);expect(selected).toMatch(/^m ·/);
  const paragraph=await structure.evaluate(s=>Array.from(s.options).find(o=>o.title.startsWith('oMathPara ·'))?.value);
  expect(paragraph).toBeTruthy();await structure.selectOption(paragraph);
  await expect(page.locator('[data-math-matrix-alignment]')).toHaveCount(0);
  await expect(page.locator('[data-math-container="oMathParaPr"]')).toBeVisible();
});
test('readable rich rendering never applies first-formula properties to an identical later formula',async({page})=>{
  await edit(page,'First $\\frac{a}{b}$.\n\nSecond $\\frac{a}{b}$.');
  await page.locator('#preview-pane mjx-container').first().click();await ready(page);
  await page.locator('.formula-properties > summary').click();await page.locator('[data-math-container="fPr"] > summary').click();
  await page.locator('[data-math-property="fPr.type"] select').selectOption('skw');await ready(page);
  await page.locator('.formula-inline-controls button').first().click();
  const first=page.locator('#preview-pane mjx-container').first(),second=page.locator('#preview-pane mjx-container').nth(1);
  await expect(page.locator('#preview-pane .readmd-dsl-math[data-readmd-ready="true"]')).toHaveCount(1);
  await expect(second).not.toHaveAttribute('data-readmd-metadata');
  await first.click();await ready(page);await page.locator('.formula-properties > summary').click();await page.locator('[data-math-container="fPr"] > summary').click();
  await expect(page.locator('[data-math-property="fPr.type"] select')).toHaveValue('skw');
  await page.keyboard.press('Escape');await second.click();await ready(page);await page.locator('.formula-properties > summary').click();await page.locator('[data-math-container="fPr"] > summary').click();
  // No explicit type means Office's default bar fraction, not the first
  // expression's explicit skewed property.
  await expect(page.locator('[data-math-property="fPr.type"] select')).toHaveValue('');
  await page.keyboard.press('Escape');await page.keyboard.press('Control+z');
  await expect(page.locator('#preview-pane .readmd-dsl-math')).toHaveCount(0);
  await expect(first).not.toHaveAttribute('data-readmd-metadata');await expect(second).not.toHaveAttribute('data-readmd-metadata');
});

test('real preview slot edits are local, revision guarded and one undo transaction',async({page})=>{
  const source='First $\\frac{x}{x}$.\n\nSecond $\\frac{x}{x}$.';await edit(page,source);
  await page.locator('#preview-pane mjx-container').nth(1).click();await ready(page);
  await page.locator('.formula-inline').getByRole('button',{name:'Denominator',exact:true}).click();
  await page.locator('.formula-inline-input').fill('\\theta+1');
  await page.locator('.formula-inline-input').press('Enter');await ready(page);
  expect(await page.evaluate(()=>getEditContent())).toBe(source);
  await page.locator('.formula-inline-controls button').first().click();
  await expect(page.locator('.formula-inline')).toHaveCount(0);
  expect(await page.evaluate(()=>getEditContent())).toBe('First $\\frac{x}{x}$.\n\nSecond $\\frac{x}{\\theta+1}$.');
  await page.keyboard.press('Control+z');expect(await page.evaluate(()=>getEditContent())).toBe(source);
});
test('slot keyboard navigation, cancellation and stale document do not overwrite content',async({page})=>{
  const source='$\\frac{a}{b}$';await edit(page,source);
  await page.locator('#preview-pane mjx-container').click();await ready(page);
  await expect(page.locator('.formula-inline-input')).toHaveValue('a');
  await page.locator('.formula-inline-input').press('Tab');await expect(page.locator('.formula-inline-input')).toHaveValue('b');
  await page.locator('.formula-inline-input').fill('c');await page.locator('.formula-inline-input').press('Enter');await ready(page);
  await page.keyboard.press('Escape');expect(await page.evaluate(()=>getEditContent())).toBe(source);
  await page.locator('#preview-pane mjx-container').click();await ready(page);
  await page.evaluate(()=>cmView.dispatch({changes:{from:0,insert:'Updated '}}));
  await expect(page.locator('.formula-inline')).toHaveCount(0);
  expect(await page.evaluate(()=>getEditContent())).toBe('Updated '+source);
});
test('invalid local drafts cannot commit and nested spans retain their structure',async({page})=>{
  const source='$\\frac{\\sqrt{x}}{\\frac{a}{b}}$';await edit(page,source);
  await page.locator('#preview-pane mjx-container').click();await ready(page);
  await page.locator('.formula-inline-input').fill('\\frac{');await page.locator('.formula-inline-input').press('Enter');
  await expect(page.locator('.formula-inline-message')).not.toHaveText('');
  await expect(page.locator('.formula-inline-controls button').first()).toBeDisabled();
  expect(await page.evaluate(()=>getEditContent())).toBe(source);
  await page.keyboard.press('Escape');expect(await page.evaluate(()=>getEditContent())).toBe(source);
});

test('complex standard properties become readable DSL, render on reopen and undo together',async({page})=>{
  const source='$\\frac{a}{b}$';await edit(page,source);
  await page.locator('#preview-pane mjx-container').click();await ready(page);
  await page.locator('.formula-properties > summary').click();
  const property=page.locator('.formula-property-fields [data-math-container="fPr"]');
  await property.locator('summary').click();
  await property.locator('[data-math-property="fPr.type"] select').selectOption('skw');await ready(page);
  await expect(page.locator('.formula-inline [data-mml-node="mfrac"]')).toHaveCount(1);
  await page.locator('.formula-inline-controls button').first().click();
  await expect(page.locator('.formula-inline')).toHaveCount(0);
  const changed=await page.evaluate(()=>getEditContent());
  expect(changed).toContain(':math[');expect(changed).toContain('office.type(val: "skw")');expect(changed).not.toContain('<!--readmd:math:v1:');
  const info=await page.evaluate(async source=>{
    const range=ReadMDFormula.ranges(source)[0];
    return (await (await apiFetch('/api/math/parse',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({source:range.tex,dialect:'dsl'})})).json());
  },changed);
  expect(JSON.stringify(info.document.root)).toContain('skw');
  await expect(page.locator('#preview-pane mjx-container')).toHaveCount(1);
  await page.locator('#preview-pane mjx-container').click();await ready(page);
  await expect(page.locator('.formula-inline [data-mml-node="mfrac"]')).toHaveCount(1);
  await page.keyboard.press('Escape');await page.keyboard.press('Control+z');
  expect(await page.evaluate(()=>getEditContent())).toBe(source);
});

test('legacy properties stay hidden and excluded from total and selected word counts before migration',async({page})=>{
  const visible='A readable formula $x$.';await edit(page,visible);
  const baseline=await page.evaluate(()=>{updateDocStatistics();return $('edit-doc-stats').textContent;});
  const metadata=await page.evaluate(async()=>{
    const response=await apiFetch('/api/math/serialize',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({source:'x',dialect:'latex',target:'metadata',latex:'x',id:'hidden-proof'})});return (await response.json()).output;
  });
  const source=visible.replace('$x$','$x$'+metadata);await edit(page,source);
  await expect(page.locator('.cm-math-metadata')).toHaveCount(1);
  expect(await page.evaluate(()=>{updateDocStatistics();return $('edit-doc-stats').textContent;})).toBe(baseline);
  const selected=await page.evaluate(()=>{cmView.dispatch({selection:{anchor:0,head:cmView.state.doc.length}});updateDocStatistics();return $('edit-doc-stats').textContent;});
  await edit(page,visible);expect(await page.evaluate(()=>{cmView.dispatch({selection:{anchor:0,head:cmView.state.doc.length}});updateDocStatistics();return $('edit-doc-stats').textContent;})).toBe(selected);
  await edit(page,source);await page.locator('.cm-math-metadata').focus();await page.keyboard.press('Enter');
  await ready(page);await expect(page.locator('.cm-math-metadata')).toHaveCount(1);expect(await page.evaluate(()=>getEditContent())).toBe(source);
  await expect(page.locator('.formula-inline-input')).toHaveValue('x');await page.keyboard.press('Escape');
});

test('matrix row edits are structural, preserve other cells and remain cancellable',async({page})=>{
  const source='$\\begin{matrix}a&b\\\\c&d\\end{matrix}$';await edit(page,source);
  await page.locator('#preview-pane mjx-container').click();await ready(page);
  await page.locator('.formula-properties > summary').click();
  const rows=page.locator('[data-math-matrix-axis="row"]');
  await expect(rows.getByRole('spinbutton')).toHaveValue('1');
  await expect(rows.locator('span')).toHaveText('Matrix Row (2)');
  await rows.getByRole('button',{name:'Insert Matrix Row',exact:true}).click();await ready(page);
  await expect(page.locator('.formula-inline [data-mml-node="mtr"]')).toHaveCount(3);
  await expect(rows.locator('span')).toHaveText('Matrix Row (3)');
  await rows.getByRole('button',{name:'Delete Matrix Row',exact:true}).click();await ready(page);
  await expect(page.locator('.formula-inline [data-mml-node="mtr"]')).toHaveCount(2);
  await rows.getByRole('spinbutton').fill('3');
  await expect(rows.getByRole('button',{name:'Delete Matrix Row',exact:true})).toBeDisabled();
  await rows.getByRole('button',{name:'Insert Matrix Row',exact:true}).click();await ready(page);
  await expect(rows.getByRole('spinbutton')).toHaveValue('3');
  await expect(page.locator('.formula-inline [data-mml-node="mtr"]')).toHaveCount(3);
  await rows.getByRole('spinbutton').fill('0');
  await expect(rows.getByRole('button',{name:'Insert Matrix Row',exact:true})).toBeDisabled();
  await page.keyboard.press('Escape');expect(await page.evaluate(()=>getEditContent())).toBe(source);
});

test('matrix column alignment survives preview reopen and shares one document undo',async({page})=>{
  const source='$\\begin{matrix}a&b\\\\long&d\\end{matrix}$';await edit(page,source);
  await page.locator('#preview-pane mjx-container').click();await ready(page);
  await page.locator('.formula-properties > summary').click();
  const alignment=page.locator('[data-math-matrix-alignment]');
  await alignment.getByRole('spinbutton',{name:'Column',exact:true}).fill('2');
  await alignment.getByRole('combobox',{name:'Alignment',exact:true}).focus();
  await alignment.getByRole('combobox',{name:'Alignment',exact:true}).selectOption('right');await ready(page);
  await expect(alignment.getByRole('spinbutton')).toHaveValue('2');
  await expect(alignment.getByRole('combobox')).toHaveValue('right');
  await expect(alignment.getByRole('combobox')).toBeFocused();
  expect(await page.evaluate(()=>getEditContent())).toBe(source);
  await page.locator('.formula-inline-controls button').first().click();
  await expect(page.locator('.formula-inline')).toHaveCount(0);
  const changed=await page.evaluate(()=>getEditContent());expect(changed).toContain(':math[');expect(changed).not.toContain('<!--readmd:math:v1:');
  await page.locator('#preview-pane mjx-container').click();await ready(page);
  await page.locator('.formula-properties > summary').click();
  await alignment.getByRole('spinbutton').fill('2');await expect(alignment.getByRole('combobox')).toHaveValue('right');
  await page.keyboard.press('Escape');await page.keyboard.press('Control+z');
  expect(await page.evaluate(()=>getEditContent())).toBe(source);
});

test('rich symbol replacement parses an expression instead of displaying TeX as literal text',async({page})=>{
  const source='$\\frac{a}{b}$';await edit(page,source);
  await page.locator('#preview-pane mjx-container').click();await ready(page);
  await page.locator('.formula-properties > summary').click();
  const property=page.locator('[data-math-property="fPr.type"]');
  await page.locator('[data-math-container="fPr"] > summary').click();
  await property.locator('select').selectOption('skw');await ready(page);
  const symbol=page.locator('.formula-inline-canvas').getByRole('button',{name:'Edit symbol a',exact:true});
  await symbol.click();
  await page.locator('.formula-inline-input').fill('\\theta+\\frac{x}{y}');
  await page.locator('.formula-inline-input').press('Enter');await ready(page);
  await expect(page.locator('.formula-inline [data-mml-node="mfrac"]')).toHaveCount(2);
  expect(await page.evaluate(()=>getEditContent())).toBe(source);
  await page.locator('.formula-inline-controls button').first().click();await expect(page.locator('.formula-inline')).toHaveCount(0);
  const changed=await page.evaluate(()=>getEditContent());expect(changed).toMatch(/θ|\\theta/);expect(changed).toContain(':math[');expect(changed).not.toContain('<!--readmd:math:v1:');
  await page.locator('#preview-pane mjx-container').click();await ready(page);
  await expect(page.locator('.formula-inline [data-mml-node="mfrac"]')).toHaveCount(2);
  await page.keyboard.press('Escape');await page.keyboard.press('Control+z');expect(await page.evaluate(()=>getEditContent())).toBe(source);
});

test('external formula edits retain old properties for recovery and show the current source',async({page})=>{
  await edit(page,'$x$');
  const metadata=await page.evaluate(async()=>{
    const source='<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:f><m:fPr><m:type m:val="skw"/></m:fPr><m:num><m:r><m:t>a</m:t></m:r></m:num><m:den><m:r><m:t>b</m:t></m:r></m:den></m:f></m:oMath>';
    const response=await apiFetch('/api/math/serialize',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({source,dialect:'omml',target:'metadata',latex:'\\frac{a}{b}',id:'external-change'})});return (await response.json()).output;
  });
  const source='$\\frac{z}{b}$'+metadata;await edit(page,source);
  await page.locator('#preview-pane mjx-container').click();await ready(page);
  await expect(page.locator('.formula-inline-input')).toHaveValue('z');
  await expect(page.locator('.formula-inline-message')).toContainText('Previous math properties remain');
  await page.locator('.formula-inline-controls button').first().click();await expect(page.locator('.formula-inline')).toHaveCount(0);
  expect(await page.evaluate(()=>getEditContent())).toBe(source);
});

test('damaged math properties cannot replace or delete a valid current formula',async({page})=>{
  const source='$\\frac{z}{b}$<!--readmd:math:v1:YQ==-->';await edit(page,source);
  await page.locator('#preview-pane mjx-container').click();await ready(page);
  await expect(page.locator('.formula-inline-input')).toHaveValue('z');
  await expect(page.locator('.formula-inline-message')).toHaveText('Saved math properties could not be read. The current formula source is preserved.');
  await page.keyboard.press('Escape');expect(await page.evaluate(()=>getEditContent())).toBe(source);
});

test('a hidden identical formula cannot redirect an edit away from the visible occurrence',async({page})=>{
  const source='<!-- $\\frac{a}{b}$ -->\n\nVisible $\\frac{a}{b}$.';await edit(page,source);
  await expect(page.locator('#preview-pane mjx-container')).toHaveCount(1);
  await page.locator('#preview-pane mjx-container').click();await ready(page);
  await page.locator('.formula-inline-input').fill('q');await page.locator('.formula-inline-input').press('Enter');await ready(page);
  await page.locator('.formula-inline-controls button').first().click();await expect(page.locator('.formula-inline')).toHaveCount(0);
  expect(await page.evaluate(()=>getEditContent())).toBe('<!-- $\\frac{a}{b}$ -->\n\nVisible $\\frac{q}{b}$.');
});

test('invalid inline input stays editable and a correction can be applied without reopening',async({page})=>{
  const source='$\\frac{a}{b}$';await edit(page,source);
  await page.locator('#preview-pane mjx-container').click();await ready(page);
  const input=page.locator('.formula-inline-input');
  await input.fill('\\frac{');await input.press('Enter');
  await expect(page.locator('.formula-inline-message')).not.toHaveText('');
  await expect(input).toBeEnabled();
  await input.fill('q');await input.press('Enter');await ready(page);
  await page.locator('.formula-inline-controls button').first().click();
  await expect(page.locator('.formula-inline')).toHaveCount(0);
  expect(await page.evaluate(()=>getEditContent())).toBe('$\\frac{q}{b}$');
});

test('rich replacement serializes catalog loading and a simultaneous save into one transaction',async({page})=>{
  let release,requested=false;const gate=new Promise(resolve=>release=resolve);
  await page.route('**/assets/math-catalog.json',async route=>{requested=true;await gate;await route.continue();});
  const source='$\\frac{a}{b}$';await edit(page,source);
  await page.locator('#preview-pane mjx-container').click();await ready(page);
  await page.locator('.formula-properties > summary').click();await page.locator('[data-math-container="fPr"] > summary').click();
  await page.locator('[data-math-property="fPr.type"] select').selectOption('skw');await ready(page);
  await page.locator('.formula-inline-canvas').getByRole('button',{name:'Edit symbol a',exact:true}).click();
  await page.locator('.formula-inline-input').fill('\\theta+1');await page.locator('.formula-inline-input').press('Enter');
  try{
    await expect.poll(()=>requested).toBe(true);
    await expect(page.locator('.formula-inline-controls button').first()).toBeDisabled();
    await page.evaluate(()=>{window.__pendingMathCommit=ReadMDInlineMath.commit();});
    expect(await page.evaluate(()=>getEditContent())).toBe(source);
  }finally{release();}
  expect(await page.evaluate(()=>window.__pendingMathCommit)).toBe(true);
  await expect(page.locator('.formula-inline')).toHaveCount(0);
  const projected=await page.evaluate(async()=>{const range=ReadMDFormula.ranges(getEditContent())[0];return (await(await apiFetch('/api/math/serialize',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({source:range.tex,dialect:range.dialect||'latex',target:'latex'})})).json()).output;});
  expect(projected).toMatch(/θ\+1|\\theta\+1/);
  await page.keyboard.press('Control+z');expect(await page.evaluate(()=>getEditContent())).toBe(source);
});

test('imported namespace aliases keep Word fonts separate from editable math styles',async({page})=>{
  await edit(page,'$x$');
  const metadata=await page.evaluate(async()=>{
    const source='<equation:oMath xmlns:equation="http://schemas.openxmlformats.org/officeDocument/2006/math" xmlns:math="http://schemas.openxmlformats.org/officeDocument/2006/math" xmlns:font="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><equation:r><font:rPr><font:b/><font:i font:val="0"/></font:rPr><math:rPr><math:sty math:val="bi"/></math:rPr><equation:t>x</equation:t></equation:r></equation:oMath>';
    const response=await apiFetch('/api/math/serialize',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({source,dialect:'omml',target:'metadata',latex:'x',id:'aliased-fonts'})});return (await response.json()).output;
  });
  await edit(page,'$x$'+metadata);await page.locator('#preview-pane mjx-container').click();await ready(page);
  await page.locator('.formula-properties > summary').click();await page.locator('[data-math-container="rPr"] > summary').click();
  const style=page.locator('[data-math-property="rPr.sty"] select');await expect(style).toHaveValue('bi');
  await style.selectOption('p');await ready(page);await expect(style).toHaveValue('p');
  await page.locator('.formula-inline-controls button').first().click();await expect(page.locator('.formula-inline')).toHaveCount(0);
  const result=await page.evaluate(async()=>{
    const metadata=getEditContent().match(/<!--readmd:math:v1:[A-Za-z0-9+/=]+-->/)[0];
    return (await (await apiFetch('/api/math/parse',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({source:'x',metadata})})).json()).document;
  });
  const props=await page.evaluate(doc=>ReadMDMathProperties.flatten(doc.root).filter(n=>ReadMDMathProperties.local(n)==='rPr').map(n=>({namespace:n.namespace,name:n.name})),result);
  expect(props.filter(n=>n.namespace==='http://schemas.openxmlformats.org/officeDocument/2006/math')).toHaveLength(1);
  expect(props.filter(n=>n.namespace==='http://schemas.openxmlformats.org/wordprocessingml/2006/main')).toHaveLength(1);
  expect(result.source).toContain('<font:i font:val="0"/>');
});

for(const rich of [false,true])test(`mouse selection preserves pending symbol input and the next target (${rich?'OMML':'LaTeX'})`,async({page})=>{
  const source='$a+b+c$';await edit(page,source);await page.locator('#preview-pane mjx-container').click();await ready(page);
  if(rich){await page.locator('.formula-properties > summary').click();await page.locator('[data-math-container="rPr"] > summary').click();await page.locator('[data-math-property="rPr.sty"] select').selectOption('bi');await ready(page);}
  await expect(page.locator('.formula-inline-input')).toHaveValue('a');
  await page.locator('.formula-inline-input').fill('\\theta+\\frac{x}{y}');
  await page.locator('.formula-inline-canvas').getByRole('button',{name:'Edit symbol c',exact:true}).click();
  await expect(page.locator('.formula-inline-input')).toHaveValue('c');
  await page.locator('.formula-inline-input').fill('z');await page.locator('.formula-inline-input').press('Enter');await ready(page);
  await page.locator('.formula-inline-controls button').first().click();await expect(page.locator('.formula-inline')).toHaveCount(0);
  const changed=await page.evaluate(()=>getEditContent());expect(changed).toMatch(/θ|\\theta/);
  if(rich){
    expect(changed).toContain(':math[');
    const projection=await page.evaluate(async()=>{
      const range=ReadMDFormula.ranges(getEditContent())[0],output=(await(await apiFetch('/api/math/serialize',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({source:range.tex,dialect:'dsl',target:'omml'})})).json()).output;
      const xml=new DOMParser().parseFromString(output,'application/xml'),ns='http://schemas.openxmlformats.org/officeDocument/2006/math';
      return {text:Array.from(xml.getElementsByTagNameNS(ns,'t'),n=>n.textContent).join(''),fractions:Array.from(xml.getElementsByTagNameNS(ns,'f'),f=>({num:f.getElementsByTagNameNS(ns,'num')[0].textContent,den:f.getElementsByTagNameNS(ns,'den')[0].textContent})),styles:Array.from(xml.getElementsByTagNameNS(ns,'sty'),n=>n.getAttributeNS(ns,'val'))};
    });
    expect(projection.text).toBe('θ+xy+b+z');expect(projection.fractions).toEqual([{num:'x',den:'y'}]);expect(projection.styles).toContain('bi');
  }else{expect(changed).toContain('\\frac{x}{y}');expect(changed).toContain('+b+z');}
  await page.keyboard.press('Control+z');expect(await page.evaluate(()=>getEditContent())).toBe(source);
});

test('applying a formula restores immediate keyboard undo before recovery cleanup and never steals later focus',async({page})=>{
  const source='$a+b+c$';await edit(page,source);await page.locator('#preview-pane mjx-container').click();await ready(page);
  await page.locator('.formula-inline-input').fill('z');await page.locator('.formula-inline-input').press('Enter');await ready(page);expect(await page.evaluate(()=>ReadMDInlineMath.flush())).toBe(true);
  let release,started;const gate=new Promise(resolve=>release=resolve),waiting=new Promise(resolve=>started=resolve);
  await page.route('**/api/documents/history',async route=>{if(route.request().postDataJSON()?.op==='delete'){started();await gate;}await route.continue();});
  try{
    await page.locator('.formula-inline-controls button').first().click();await waiting;await expect(page.locator('.formula-inline')).toHaveCount(0);
    expect(await page.evaluate(()=>cmView.hasFocus)).toBe(true);await page.keyboard.press('Control+z');expect(await page.evaluate(()=>getEditContent())).toBe(source);
    // Reading settings intentionally disappear while editing. Use a visible
    // content action to verify late recovery cleanup never steals focus.
    await expect(page.locator('#btn-ai')).toBeVisible();await page.locator('#btn-ai').focus();release();await page.evaluate(()=>ReadMDInlineMath.flush());await expect(page.locator('#btn-ai')).toBeFocused();
  }finally{release();}
});

test('failed readable serialization preserves the draft and an explicit retry applies once',async({page})=>{
  const source='$a$';await edit(page,source);await page.locator('#preview-pane mjx-container').click();await ready(page);
  await page.locator('.formula-properties > summary').click();await page.locator('[data-math-container="rPr"] > summary').click();await page.locator('[data-math-property="rPr.sty"] select').selectOption('bi');await ready(page);
  let requests=0;await page.route('**/api/math/serialize',route=>{requests++;return requests===1?route.fulfill({status:503,json:{ok:false,error_code:'math_invalid_xml'}}):route.continue();});
  await page.locator('.formula-inline-controls button').first().click();
  await expect(page.locator('.formula-inline-message')).not.toHaveText('');await expect(page.locator('.formula-inline-controls button').first()).toBeEnabled();
  expect(await page.evaluate(()=>getEditContent())).toBe(source);
  await page.locator('.formula-inline-controls button').first().click();await expect(page.locator('.formula-inline')).toHaveCount(0);
  const changed=await page.evaluate(()=>getEditContent());expect(changed.match(/:math\[/g)).toHaveLength(1);expect(changed).not.toContain('<!--readmd:math:v1:');expect(requests).toBe(2);
  await page.keyboard.press('Control+z');expect(await page.evaluate(()=>getEditContent())).toBe(source);
});

test('script argument size applies when Tab leaves properties and persists through one undoable save',async({page})=>{
  const source='$x^y$';await edit(page,source);await page.locator('#preview-pane mjx-container').click();await ready(page);
  await page.locator('.formula-properties > summary').click();await ready(page);
  const structure=page.locator('.formula-property-body > select');
  const sup=await structure.evaluate(select=>Array.from(select.options).find(option=>option.title.startsWith('sup ·')).value);
  await structure.selectOption(sup);await page.locator('[data-math-container="argPr"] > summary').click();
  const size=page.locator('[data-math-property="argPr.argSz"] input');
  const glyph=page.locator('.formula-inline-canvas').getByRole('button',{name:'Edit symbol y',exact:true});
  const before=await glyph.boundingBox();await size.fill('1');await size.press('Tab');
  await expect(page.locator('.formula-inline')).toHaveCount(0);
  const changed=await page.evaluate(()=>getEditContent());expect(changed).toContain(':math[');expect(changed).not.toContain('<!--readmd:math:v1:');
  await page.locator('#preview-pane mjx-container').click();await ready(page);await page.locator('.formula-properties > summary').click();await ready(page);
  await expect.poll(async()=> (await glyph.boundingBox()).height).toBeGreaterThan(before.height*1.2);
  await structure.selectOption(sup);await page.locator('[data-math-container="argPr"] > summary').click();await expect(size).toHaveValue('1');
  await page.keyboard.press('Escape');await page.keyboard.press('Control+z');expect(await page.evaluate(()=>getEditContent())).toBe(source);
});
