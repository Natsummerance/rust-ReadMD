const {waitForPage}=require('./page-condition.cjs');
const {test,expect}=require('@playwright/test');
test.use({bypassCSP:false});
const source='---\ntitle: Heading fixture\n---\n# **Intro**\n\n[Second copy](#intro-1) · [API](#API_V1) · [Greek](#%CE%B1--%CE%B2)\n\n# Intro\n\n# Intro-1\n\n# API {#API_V1}\n\n# API {#API-V1}\n\n# α &amp; β\n\nSection\n=======\n\n```md\n# Ignored\n```\n\n# !!!\n\n# a  b\n';
const expected=['intro','intro-1','intro-1-1','API_V1','API-V1','α--β','section','section-1','a--b'];

test('closing heading hashes before explicit attributes never leak into reading titles or outlines',async({page})=>{
  const body='# **Title** ## {#title}\n\n> ## Quote ## {#quote}\n\n# Hash code `x ##` {#code}\n';
  await page.goto('/');await waitForPage(page,()=>window.__readmdAppReady);await page.evaluate(body=>renderVirtual('clipboard','Heading-closers.md','',body,[]),body);
  expect(await page.locator('#content h1,#content h2').allTextContents()).toEqual(['Title','Quote','Hash code x ##']);
  expect(await page.evaluate(body=>ReadMDHeadings.fromMarkdown(body).map(({id,text})=>({id,text})),body)).toEqual([{id:'title',text:'Title'},{id:'quote',text:'Quote'},{id:'code',text:'Hash code x ##'}]);
  await page.evaluate(async()=>{await toggleEdit();setPvLayout('right');});await expect(page.locator('#preview-pane #quote')).toHaveText('Quote');expect(await page.evaluate(()=>getEditContent())).toBe(body);
});
test('reader, preview, inserted TOC and Rust inspection share exact identities',async({page})=>{
  await page.goto('/');await waitForPage(page, ()=>window.__readmdAppReady);
  await page.evaluate(async source=>{await renderVirtual('clipboard','Headings.md','',source,[]);},source);
  expect(await page.locator('#content h1').evaluateAll(nodes=>nodes.map(n=>n.id))).toEqual(expected);
  expect(await page.evaluate(source=>ReadMDHeadings.fromMarkdown(source).map(h=>h.id),source)).toEqual(expected);
  expect(await page.evaluate(source=>Array.from(ReadMDTransforms.tocMarkdown(source).matchAll(/\]\(#([^)]*)\)/g),m=>decodeURIComponent(m[1])),source)).toEqual(expected);
  const inspected=await page.evaluate(async content=>{const r=await apiFetch('/api/document/analyze',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({content})});return r.json();},source);
  expect(inspected.ok).toBe(true);expect(inspected.headings.map(h=>h.id)).toEqual(expected);
  await page.locator('#content').getByRole('link',{name:'Second copy',exact:true}).click();await expect(page.locator('#content h1[id="intro-1"]')).toBeFocused();
  await page.evaluate(async()=>{await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');});
  await expect(page.locator('#preview-pane h1')).toHaveCount(expected.length);
  expect(await page.locator('#preview-pane h1').evaluateAll(nodes=>nodes.map(n=>n.id))).toEqual(expected);
  await page.locator('#preview-pane a[href="#intro-1"]').click();await expect(page.locator('#preview-pane h1[id="intro-1"]')).toBeFocused();
  expect(await page.evaluate(()=>getEditContent())).toBe(source);
});

test('incremental preview keeps math heading identity and reallocates duplicate text',async({page})=>{
  await page.goto('/');await waitForPage(page, ()=>window.__readmdAppReady);
  await page.evaluate(async()=>{await renderVirtual('clipboard','Math-heading.md','','# Value $\\alpha$\n\n# Same\n\n# Same\n\nParagraph.\n',[]);await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');});
  await expect(page.locator('#preview-pane mjx-container')).toHaveCount(1);
  const initial=await page.locator('#preview-pane h1').evaluateAll(nodes=>nodes.map(n=>n.id));
  await page.evaluate(()=>cmView.dispatch({changes:{from:cmView.state.doc.length,insert:'More text.\n'}}));
  await expect(page.locator('#preview-pane')).toContainText('More text.');
  expect(await page.locator('#preview-pane h1').evaluateAll(nodes=>nodes.map(n=>n.id))).toEqual(initial);
  expect(initial).toEqual(['value-alpha','same','same-1']);
});
