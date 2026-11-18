'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {randomUUID, createHash} = require('node:crypto');
const {openOwned, until} = require('../tools/test/owned-browser-host.cjs');
const {startKernel} = require('../tools/test/v006-refactor-host.cjs');
const zip = require('./zip-parts.cjs');
const binary = path.resolve(process.argv[2] || ('rust/target/debug/readmd' + (process.platform === 'win32' ? '.exe' : '')));
const output = path.resolve(process.argv[3] || 'table-geometry-evidence');
const playwright = process.argv[4];

const header = '| Name | Value | Note |';
const table = [header, '| --- | --- | --- |', '| **Alpha** | Bravo | East |', '| Charlie | [Delta](https://example.invalid) | West |'].join('\n');
const modified = attrs => table.replace('**Alpha**', '**Alpha** {' + attrs + '}');
const doc = grid => '# Tables\n\n' + table + '\n\n' + grid + '\n';
const initial = doc(table);
const span = 'colspan=2 rowspan=2', align = 'align=right valign=bottom';
const states = [initial, doc(modified(span)), doc(modified(span+' '+align)), doc(modified(align)), initial, doc(modified(span)), doc(modified(span+' '+align))];
const physical = s => Buffer.from('\ufeff'+s.replaceAll('\n','\r\n'));
const report = {platform:process.platform, passed:false, nativeWindow:false, checks:[]};
const check = name => report.checks.push(name);

async function exportsFor(host) {
  const content = '# ReadMD Grid\n\n'+modified(span+' '+align)+'\n';
  for (const format of ['docx','pptx','epub','html']) {
    const file = path.join(host.documents, 'ReadMD-Grid-Fixture.' + format);
    const result = await host.api('/api/export', { format, content, baseDir:host.documents, out_path:file, options:{} });
    assert(result.ok, format + ' ' + (result.error || 'export'));
    if (format === 'html') {
      const preview = await host.context.newPage();
      try {
        await preview.setContent(fs.readFileSync(file,'utf8'));
        const cell = preview.locator('#content td[colspan="2"][rowspan="2"]');
        await cell.waitFor();
        assert.equal(await cell.locator('strong').textContent(),'Alpha');
        assert.equal(await cell.locator('a').textContent(),'Delta');
        assert.equal(await cell.evaluate(e=>getComputedStyle(e).verticalAlign),'bottom');
      } finally { await preview.close(); }
    } else {
      const parts = zip(fs.readFileSync(file));
      const key = [...parts.keys()].find(k => format==='docx' ? k==='word/document.xml' : format==='pptx' ?
        /^ppt\/slides\/slide\d+\.xml$/.test(k) && parts.get(k).includes('<a:tbl>') : /\.xhtml$/.test(k) && parts.get(k).includes('<table'));
      assert(key,format);
      const xml = parts.get(key);
      for (const name of ['Alpha','Bravo','Charlie','Delta','East','West']) assert(xml.includes(name), name);
      if (format==='docx') { assert.match(xml,/<w:gridSpan w:val="2"/); assert.match(xml,/<w:vMerge w:val="restart"/); }
      if (format==='pptx') { assert.match(xml,/gridSpan="2"/); assert.match(xml,/rowSpan="2"/); }
      if (format==='epub') { assert.match(xml,/rowspan="2"/); assert.match(xml,/colspan="2"/); assert.match(xml,/vertical-align:bottom/); }
    }
    check(format + ' preserves merged cells and their four editable or readable contents');
  }
}

async function exercise() {
  let file;
  const host = await openOwned(output, (_, dirs) => {
    file = path.join(dirs.documents, '双表 😀.md');
    fs.writeFileSync(file,physical(initial),{flag:'wx'});
  },{binary,playwright});
  const page=host.page;
  const snapshot=()=>page.evaluate(()=>ReadMDReview.snapshot(getActiveTab()));
  const edit=async (text)=>{
    await page.locator('#btn-edit').click();
    await until(()=>page.evaluate(s=>cmView?.state.doc.toString()===s,text),'editor source');
  };
  const save=async (text)=>{
    await page.locator('#edit-save').click();
    await until(()=>fs.readFileSync(file).equals(physical(text)),'save exact BOM/CRLF bytes');
    await until(()=>page.evaluate(()=>!state.editing),'return to reader');
  };
  const grid=async (text,r,c)=>{
    await page.evaluate(pos=>cmView.dispatch({selection:{anchor:pos}}),text.lastIndexOf(header)+3);
    await page.locator('[data-menu="md-insert-menu"]').click();
    await page.locator('#btn-insert-table').click();
    await until(()=>page.locator('#table-cell-apply').isEnabled(),'capture current table');
    await page.locator('[data-table-row="'+r+'"][data-table-column="'+c+'"]').click();
    const details=page.locator('#table-cell-properties');
    if (!(await details.evaluate(e=>e.open))) await details.locator('summary').click();
  };
  const submit=async (id,expected)=>{
    await page.locator('#table-cell-'+id).click();
    await page.locator('#table-modal').waitFor({state:'hidden'});
    await until(async()=> (await snapshot()).content===expected,'commit '+id);
  };
  try {
    await host.page.evaluate(p=>loadFile(p),file);
    await edit(initial);
    await grid(initial,1,0);
    await page.locator('#table-cell-rowspan').fill('2');
    await page.locator('#table-cell-colspan').fill('2');
    await submit('merge',states[1]);
    await grid(states[1],1,0);
    await page.locator('#table-cell-align').selectOption('right');
    await page.locator('#table-cell-valign').selectOption('bottom');
    await submit('properties-apply',states[2]);
    await save(states[2]);
    const reader = page.locator('#content table');
    await until(()=>reader.count().then(n=>n===2),'two reader tables');
    assert.equal(await reader.first().locator('[rowspan]').count(),0);
    const cell = reader.nth(1).locator('td[rowspan="2"][colspan="2"]');
    assert.equal(await cell.innerText(),'Alpha\nBravo\nCharlie\nDelta');
    assert.equal(await cell.locator('strong').innerText(),'Alpha');
    assert.equal(await cell.locator('a').innerText(),'Delta');
    assert.deepEqual(await cell.evaluate(e=>[getComputedStyle(e).textAlign,getComputedStyle(e).verticalAlign]),['right','bottom']);
    check('merge targets only the selected duplicate table, preserves rich content and applies both alignments');
    await edit(states[2]);
    await grid(states[2],2,1);
    await submit('split',states[3]);
    await grid(states[3],1,0);
    await page.locator('#table-cell-align').selectOption('');
    await page.locator('#table-cell-valign').selectOption('');
    await submit('properties-apply',initial);
    await save(initial);
    check('split from a covered slot and clear alignment restore exact source, BOM and CRLF');
    // Persist the merged state too. A plain final document alone cannot prove
    // that restored sessions carry cell geometry across a process boundary.
    await edit(initial);
    await grid(initial,1,0);
    await page.locator('#table-cell-rowspan').fill('2');
    await page.locator('#table-cell-colspan').fill('2');
    await submit('merge',states[5]);
    await grid(states[5],1,0);
    await page.locator('#table-cell-align').selectOption('right');
    await page.locator('#table-cell-valign').selectOption('bottom');
    await submit('properties-apply',states[6]);
    await save(states[6]);
    await exportsFor(host);
    const end=await snapshot();
    return {file,profile:host.profile,documents:host.documents,id:end.document_id,pid:host.host.pid};
  } finally { await host.close(); }
}

async function recover({file,profile,documents,id,pid}) {
  const tmp=fs.mkdtempSync(path.join(output,'reopen-'));
  Object.assign(process.env,{TMPDIR:tmp,TMP:tmp,TEMP:tmp});
  try {
    const server=await startKernel(binary,profile,documents);
    try {
      assert.notEqual(server.pid,pid);
      const call=async (route,body)=>{
        const response=await server.api(route,body?{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)}:{});
        assert.equal(response.status,200);return response.json();
      };
      await call('/api/file?p='+encodeURIComponent(file));
      const resume=await call('/api/review',{action:'resume',path:file});
      let head=resume.snapshot;
      assert.equal(head.document_id,id); assert.equal(head.content,states.at(-1));
      for(const [action,sequence] of [['undo',states.slice(0,-1).reverse()],['redo',states.slice(1)]]) {
        for (const expected of sequence) {
          head=await call('/api/review',{action,document_id:id,generation:head.generation,review_revision:head.review_revision,call_id:randomUUID()});
          assert.equal(head.content,expected,action);assert.deepEqual(fs.readFileSync(file),physical(states.at(-1)));
        }
      }
      check('a fresh Rust process reopens the saved merged table and restores all six undo/redo revisions');
    } finally { await server.stop(); }
  } finally { fs.rmdirSync(tmp); }
}
exercise().then(recover).then(()=>{report.passed=true;}).catch(e=>{
  report.failure=String(e.stack||e).replaceAll(output,'<fixture>').replaceAll(process.cwd(),'<source>');
  process.exitCode=1;
}).finally(()=>{
  report.binarySha256=createHash('sha256').update(fs.readFileSync(binary)).digest('hex');
  fs.writeFileSync(path.join(output,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx'});
  console.log(JSON.stringify(report));
});
