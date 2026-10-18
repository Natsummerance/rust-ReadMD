const {test,expect}=require('@playwright/test');
const {waitForPage}=require('./page-condition.cjs');
const {source,applyStyle}=require('./publication-note-content-checks.cjs');
test.use({bypassCSP:false});
async function start(page){await page.goto('/');await waitForPage(page,()=>window.__readmdAppReady);await page.evaluate(async source=>{await i18n.setLanguage('zh-CN',false);await renderVirtual('clipboard','Footnote-study.md','',source,[]);await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');},source);await expect(page.locator('#preview-pane blockquote')).toBeVisible();}
test('only the selected repeated footnote paragraph receives local formatting, with one undo and redo',async({page})=>{
 await start(page);await applyStyle(page,'Repeated evidence.',{last:true,size:17});
 const edited=await page.evaluate(()=>getEditContent());expect(edited).toContain('[^b]: Repeated evidence.\n\n    Repeated evidence.<!-- readmd:layout');
 const styled=page.locator('#preview-pane [data-readmd-layout-inline="true"]');await expect(styled).toHaveCount(1);await expect(styled).toHaveText('Repeated evidence.');await expect(styled).toHaveCSS('font-size','22.6667px');
 expect(await styled.evaluate(n=>Number(n.closest('[data-source-line]').dataset.sourceLine))).toBe(source.split('\n').findLastIndex(s=>s==='    Repeated evidence.')+1);
 // CodeMirror renders decorations only within its viewport. Bring the final
 // note into view before asserting its folded metadata control.
 await page.evaluate(()=>{cmView.dispatch({selection:{anchor:cmView.state.doc.length},scrollIntoView:true});cmView.focus();});
 await expect(page.locator('.cm-math-metadata')).toHaveCount(1);await page.keyboard.press('Control+z');expect(await page.evaluate(()=>getEditContent())).toBe(source);await page.keyboard.press('Control+y');expect(await page.evaluate(()=>getEditContent())).toBe(edited);
});
test('note examples and escaped definitions stay literal, and rendered note HTML is sanitized',async({page})=>{
 await start(page);const input=source+'\n````markdown\n[^literal]: Keep this example.\n````\n\n\\[^escaped]: Keep this literal paragraph.\n\nUnsafe[^unsafe].\n\n[^unsafe]: <img src="missing.png" onerror="window.__noteXss=true"> Safe note.\n';
 await page.evaluate(input=>cmView.dispatch({changes:{from:0,to:cmView.state.doc.length,insert:input}}),input);
 await expect(page.locator('#preview-pane .rd-footnotes > ol > li')).toHaveCount(3);await expect(page.locator('#preview-pane pre').filter({hasText:'[^literal]'})).toContainText('[^literal]: Keep this example.');await expect(page.locator('#preview-pane .rd-footnotes pre')).toContainText('let evidence');
 await expect(page.locator('#preview-pane')).toContainText('[^escaped]: Keep this literal paragraph.');await expect(page.locator('#preview-pane .rd-footnotes [onerror]')).toHaveCount(0);expect(await page.evaluate(()=>window.__noteXss)).toBeUndefined();expect(await page.evaluate(()=>getEditContent())).toBe(input);
});
test('first footnote paragraph uses the existing formatting menu and metadata reset preserves every nested block',async({page})=>{
 await start(page);await applyStyle(page,'First note paragraph.',{size:14});
 const edited=await page.evaluate(()=>getEditContent());expect(edited).toContain('[^a]: First note paragraph.<!-- readmd:layout');await expect(page.locator('#preview-pane [data-readmd-layout-inline="true"]')).toHaveText('First note paragraph.');
 await page.locator('.cm-math-metadata').click();await expect(page.locator('[data-local="text.size"]')).toHaveValue('14');await page.locator('[data-local-action="reset"]').click();await expect.poll(()=>page.evaluate(()=>getEditContent())).toBe(source);
 await expect(page.locator('#preview-pane table')).toBeVisible();await expect(page.locator('#preview-pane blockquote')).toContainText('Quoted evidence');await expect(page.locator('#preview-pane ol[start="3"]')).toBeVisible();
});
test('footnote code examples remain literal while the real continuation property stays editable',async({page})=>{
 await start(page);const literal='<!-- readmd:layout v1 {"id":"block-aaaaaaaaaaaaaaaaaaaaaaaa","text":{"size":17}} -->',input=source.replace('    let evidence = 98;','    '+literal);
 await page.evaluate(input=>cmView.dispatch({changes:{from:0,to:cmView.state.doc.length,insert:input}}),input);await applyStyle(page,'Repeated evidence.',{last:true,size:17});
 await expect(page.locator('#preview-pane .rd-footnotes pre')).toContainText(literal);
 expect(await page.evaluate(()=>ReadMDPublication.metadataRanges(getEditContent(),cmView.state).length)).toBe(1);
 await page.evaluate(()=>{cmView.dispatch({selection:{anchor:cmView.state.doc.length},scrollIntoView:true});cmView.focus();});await expect(page.locator('.cm-math-metadata')).toHaveCount(1);
 await page.locator('.cm-math-metadata').click();await expect(page.locator('[data-local="text.size"]')).toHaveValue('17');await page.locator('[data-local-action="reset"]').click();await expect.poll(()=>page.evaluate(()=>getEditContent())).toBe(input);
});
