const {test,expect}=require('@playwright/test');
const {waitForPage}=require('./page-condition.cjs');
const fs=require('node:fs'),path=require('node:path'),os=require('node:os');
test.use({bypassCSP:false});let folder;
test.beforeEach(async({page})=>{
  folder=fs.mkdtempSync(path.join(os.tmpdir(),'readmd-scientific-'));
  const file=path.join(folder,'Measurements.md');fs.writeFileSync(file,'# Measurements\n\n| Group | Mean (mg/L) |\n|---|---:|\n| A | 12.4 |\n| B | 14.1 |\n');
  await page.goto('/');await waitForPage(page,()=>window.__readmdAppReady);
  await page.evaluate(async file=>{await i18n.setLanguage('zh-CN',false);await loadFile(file);await openExportModal();},file);
});
test.afterEach(()=>{expect(path.dirname(folder)).toBe(path.resolve(os.tmpdir()));expect(path.basename(folder).startsWith('readmd-scientific-')).toBe(true);fs.rmSync(folder,{recursive:true,force:true});});
async function choose(page,name){const details=page.locator('#exp-preset').locator('xpath=ancestor::details[1]');if(!await details.evaluate(e=>e.open))await details.locator(':scope > summary').click();await page.locator('#exp-preset').selectOption(name);}
test('academic three-line tables remove grids and shading in the actual HTML and preview',async({page})=>{
  await page.locator('#export-tab-html').click();await choose(page,'academic_research');
  expect(await page.evaluate(()=>collectExportOptions().table.style)).toBe('three-line');
  const table=page.locator('#export-preview-mini-content table');await expect(table).toHaveCount(1);
  const actual=await table.evaluate(e=>{const t=getComputedStyle(e),h=getComputedStyle(e.querySelector('th')),c=getComputedStyle(e.querySelector('td'));return{top:t.borderTopWidth,bottom:t.borderBottomWidth,left:h.borderLeftWidth,right:c.borderRightWidth,bodyBottom:c.borderBottomWidth,mid:h.borderBottomWidth,bg:h.backgroundColor};});
  expect(parseFloat(actual.top)).toBeGreaterThan(0);expect(parseFloat(actual.bottom)).toBeGreaterThan(0);expect(parseFloat(actual.mid)).toBeGreaterThan(0);expect(parseFloat(actual.top)).toBeGreaterThanOrEqual(parseFloat(actual.mid));expect(actual.left).toBe('0px');expect(actual.right).toBe('0px');expect(actual.bodyBottom).toBe('0px');expect(actual.bg).toBe('rgba(0, 0, 0, 0)');
  const result=await page.evaluate(async out=>(await apiFetch('/api/export',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({format:'html',content:currentExportContent(),baseDir:state.dir,out_path:out,options:collectExportOptions()})})).json(),path.join(folder,'Research.html'));expect(result.ok).toBe(true);
  expect(fs.readFileSync(path.join(folder,'Research.html'),'utf8')).toContain('border-left:0');
  const customize=page.locator('#export-box .export-customize');if(!await customize.evaluate(e=>e.open))await customize.locator(':scope > summary').click();
  await page.locator('[data-k="table.style"]').evaluate(e=>e.closest('.exp-sec').classList.add('open'));await page.locator('[data-k="table.style"]').selectOption('grid');
  await expect.poll(()=>table.locator('th').first().evaluate(e=>parseFloat(getComputedStyle(e).borderLeftWidth))).toBeGreaterThan(0);
});
test('research figures have useful defaults and five editable localized statistical templates',async({page})=>{
  expect(await page.evaluate(()=>collectExportOptions().figures)).toMatchObject({style:'original',widthMm:89,fontSizePt:7});
  await page.evaluate(()=>closeExportModal());await page.locator('#btn-edit').click();await page.evaluate(()=>openDiagramModal());
  await page.locator('#diagram-type').selectOption('vega-lite');await expect(page.locator('#diagram-research-template option')).toHaveCount(6);
  const catalog=JSON.parse(fs.readFileSync(path.join(__dirname,'../assets/data/scientific-figures.json')));
  for(const item of catalog.templates){await page.locator('#diagram-research-template').selectOption(item.id);expect(JSON.parse(await page.locator('#diagram-code').inputValue())).toEqual(item.spec);}
  expect((await page.locator('#diagram-research-template').textContent())).not.toContain('diagram.research');
  await page.locator('#diagram-type').selectOption('graphviz');await expect(page.locator('#diagram-research-options')).toBeHidden();
  for(const size of [{width:1024,height:680},{width:1160,height:820}]){await page.setViewportSize(size);const box=await page.locator('#diagram-code').boundingBox();expect(box.x+box.width).toBeLessThanOrEqual(size.width);}
});
test('46 locale files cover scientific controls using the translation catalog',async()=>{
  const spec=JSON.parse(fs.readFileSync(path.join(__dirname,'../tools/scientific-translations.json'))),locales=Object.keys(JSON.parse(fs.readFileSync(path.join(__dirname,'../assets/i18n/meta.json'))));
  for(const locale of locales){const texts=JSON.parse(fs.readFileSync(path.join(__dirname,'../assets/i18n',locale+'.json')));for(const key of Object.keys(spec))expect(texts[key],locale+' '+key).toBe(spec[key][locale]);}
});
test('client diagrams export as validated self-contained static SVG',async({page},testInfo)=>{
  const figures={style:'research',widthMm:89,fontSizePt:7},examples=[['mermaid','graph LR\nA[Observation] --> B[Comparison]'],['graphviz','digraph { A -> B; }'],['chart',JSON.stringify({type:'line',data:{labels:['A','B'],datasets:[{label:'Samples',data:[1,2]}]}})],['tikz','\\begin{tikzpicture}\\draw[->] (0,0)--(3,0);\\draw (0,0) parabola (2,2);\\node at (1,-.4) {$x$};\\end{tikzpicture}']];
  for(const [engine,code]of examples){const source='```'+engine+'\n'+code+'\n```';const result=await page.evaluate(async({source,figures})=>{const options=await prepareExportFigures(source,{figures});const r=await apiFetch('/api/diagram/render',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({action:'materialize',content:source,figures:options.figures})});return{svg:options.figures.rendered[0].svg,response:await r.json()};},{source,figures});fs.writeFileSync(testInfo.outputPath(engine+'.svg'),result.svg);expect(result.response,engine).toMatchObject({ok:true});expect(result.response.content).toContain('data:image/svg+xml;base64,');}
});
test('HTML preview materializes the same scientific figure instead of showing its JSON',async({page})=>{
  const catalog=JSON.parse(fs.readFileSync(path.join(__dirname,'../assets/data/scientific-figures.json'))),file=path.join(folder,'Figure.md');fs.writeFileSync(file,'# Research\n\n```vega-lite\n'+JSON.stringify(catalog.templates[0].spec)+'\n```\n');
  await page.evaluate(async file=>{await loadFile(file);await openExportModal();},file);await page.locator('#export-tab-html').click();await choose(page,'academic_research');
  const host=page.locator('#export-preview-mini-content'),image=host.locator('img');await expect(image).toHaveCount(1);await expect(host).toHaveAttribute('aria-busy','false');await expect.poll(()=>image.evaluate(e=>e.naturalWidth)).toBeGreaterThan(300);expect(await host.textContent()).not.toContain('"encoding"');
});
test('unsafe export SVG reports a localized error and preserves the old output',async({page})=>{
  const out=path.join(folder,'Existing.pdf');fs.writeFileSync(out,'Keep the existing result');
  const result=await page.evaluate(async out=>{
    const content='```mermaid\ngraph LR\nA-->B\n```',figures={style:'research',widthMm:89,fontSizePt:7};
    const request=async(route,body)=>(await apiFetch(route,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)})).json();
    const parsed=await request('/api/diagram/render',{action:'fences',content,figures});figures.rendered=[{key:parsed.fences[0].key,svg:'<svg><script>unsafe</script></svg>'}];
    const error=await request('/api/export',{format:'pdf',content,out_path:out,options:{figures}});
    return{error,message:apiMessage(error,'reader.diagramRenderFailed'),expected:i18n.t('reader.diagramInvalidInput')};
  },out);expect(result.error.ok).toBe(false);expect(result.message).toBe(result.expected);expect(result.message).not.toContain('svg_');expect(fs.readFileSync(out,'utf8')).toBe('Keep the existing result');
});
