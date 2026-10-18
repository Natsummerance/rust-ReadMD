const {test,expect}=require('@playwright/test');
const {waitForPage}=require('./page-condition.cjs');
test.use({bypassCSP:false});
const source='# Layout review\n\n> ## Quoted heading ## {#quoted}\n>\n> Repeated evidence.\n>\n> Repeated evidence.\n>\n> - Parent **strong** [reference](#quoted)\n>   - Child remains unchanged.\n\n3. Ordered first.\n4. Ordered second.\n\n- [x] Reviewed task.\n';
async function start(page,body=source){await page.goto('/');await waitForPage(page,()=>window.__readmdAppReady);await page.evaluate(async body=>{await i18n.setLanguage('zh-CN',false);await renderVirtual('clipboard','Layout-review.md','',body,[]);await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');},body);await expect(page.locator('#preview-pane blockquote')).toBeVisible();}
async function open(page,text,last=false){await page.evaluate(({text,last})=>{const source=cmView.state.doc.toString(),at=last?source.lastIndexOf(text):source.indexOf(text);cmView.dispatch({selection:{anchor:at}});cmView.focus();},{text,last});await page.locator('[data-menu="md-text-menu"]').click();await page.locator('#md-text-menu [data-md="publication-style"]').click();await expect(page.locator('#publication-style-modal')).toBeVisible();}
async function apply(page){await page.locator('[data-local="text.size"]').fill('17');await page.locator('[data-local="paragraph.align"]').selectOption('right');await page.locator('#publication-style-modal button[type=submit]').click();await expect(page.locator('#publication-style-modal')).toHaveCount(0);}

test('second repeated quote formats through the existing menu, folds metadata, and undoes as one edit',async({page})=>{
 await start(page);await open(page,'Repeated evidence.',true);await apply(page);
 const changed=await page.evaluate(()=>getEditContent());expect(changed).toContain('> Repeated evidence.\n>\n> Repeated evidence.<!-- readmd:layout');
 const styles=page.locator('#preview-pane [data-readmd-layout-inline="true"]');await expect(styles).toHaveCount(1);await expect(styles).toHaveText('Repeated evidence.');await expect(styles).toHaveCSS('font-size','22.6667px');
 await expect(page.locator('.cm-math-metadata')).toHaveCount(1);expect(await page.locator('.cm-content').textContent()).not.toContain('"size":17');
 await page.keyboard.press('Control+z');expect(await page.evaluate(()=>getEditContent())).toBe(source);await page.keyboard.press('Control+y');expect(await page.evaluate(()=>getEditContent())).toBe(changed);
 await page.locator('.cm-math-metadata').click();await expect(page.locator('[data-local="text.size"]')).toHaveValue('17');await expect(page.locator('.publication-style-target')).toHaveText('Repeated evidence.');await page.locator('[data-local-action="reset"]').click();await expect.poll(()=>page.evaluate(()=>getEditContent())).toBe(source);
});
test('tight list formatting stays inside its own paragraph and keeps child items, tasks and numbering',async({page})=>{
 await start(page);await open(page,'Parent **strong**');await apply(page);
 const selected=page.locator('#preview-pane [data-readmd-layout-inline="true"]');await expect(selected).toHaveText('Parent strong reference');expect(await selected.locator('li').count()).toBe(0);await expect(page.locator('#preview-pane ol')).toHaveAttribute('start','3');await expect(page.locator('#preview-pane input[type=checkbox]')).toBeChecked();
 const child=page.locator('#preview-pane li').filter({hasText:/^Child remains unchanged\.$/});expect(await child.evaluate(e=>getComputedStyle(e).fontSize)).not.toBe('22.6667px');
 expect(await page.evaluate(()=>getEditContent())).toContain('>   - Child remains unchanged.');
});
test('heading attributes and explicit closing hashes survive styling and metadata reset',async({page})=>{
 await start(page);await open(page,'Quoted heading');await apply(page);await expect(page.locator('#preview-pane h2#quoted')).toHaveText('Quoted heading');
 expect(await page.evaluate(()=>getEditContent())).toMatch(/Quoted heading<!-- readmd:layout[^\n]+--> ## \{#quoted\}/);
 await page.locator('.cm-math-metadata').click();await expect(page.locator('.publication-style-target')).toHaveText('## Quoted heading ## {#quoted}');await page.locator('[data-local-action="reset"]').click();await expect.poll(()=>page.evaluate(()=>getEditContent())).toBe(source);
});
test('literal style comments in code stay visible while only the real nested directive is folded',async({page})=>{
 const marker='<!-- readmd:layout v1 {"id":"block-0123456789abcdef01234567","text":{"size":17}} -->';
 const body='> Real paragraph'+marker+'\n\n`'+marker+'`\n\n```markdown\n- Example'+marker+'\n```\n';await start(page,body);
 await expect(page.locator('.cm-math-metadata')).toHaveCount(1);expect((await page.locator('.cm-content').textContent()).match(/"size":17/g)).toHaveLength(2);
 await page.locator('.cm-math-metadata').click();await expect(page.locator('.publication-style-target')).toHaveText('Real paragraph');await page.keyboard.press('Escape');expect(await page.evaluate(()=>getEditContent())).toBe(body);
});
