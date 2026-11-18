'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {createHash} = require('node:crypto');
const {openOwned, until} = require('../tools/test/owned-browser-host.cjs');
const recover = require('./table-history-reopen.cjs');
const binary = path.resolve(process.argv[2] || ('rust/target/debug/readmd' + (process.platform === 'win32' ? '.exe' : '')));
const output = path.resolve(process.argv[3]);
const playwright = process.argv[4];
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const physical = text => Buffer.from('\ufeff' + text.replaceAll('\n','\r\n'));
const pad = n => String(n).padStart(2,'0');
const span = ' {colspan=2 rowspan=2}';
const data = Array.from({length:26},(_,i) => ['Lot'+pad(i+1),'B'+pad(i+1),'S'+pad(i+1),String(510+i)]);
data[0].splice(0,2,'**North**'+span,'Cold');
data[1].splice(0,2,'Dark','Tube');
const initial = ['| Group | Batch | Sample | RFU |','| --- | --- | --- | --- |',...data.map(row=>'| '+row.join(' | ')+' |'),''].join('\n');
const states=[initial];
const report={platform:process.platform,arch:process.arch,passed:false,checks:[]};
const cell=(r,c)=>'[data-table-row="'+r+'"][data-table-column="'+c+'"]';
const mod=process.platform==='darwin'?'Meta':'Control';

async function exercise() {
  let file;
  const host=await openOwned(output,(_,dirs)=>{
    file=path.join(dirs.documents,'tracks.md');
    fs.writeFileSync(file,physical(initial),{flag:'wx'});
  },{binary,playwright});
  const {page}=host;
  const doc=()=>page.evaluate(()=>cmView?.state.doc.toString());
  const snapshot=()=>page.evaluate(()=>ReadMDReview.snapshot(getActiveTab()));
  const revision=async()=> (await snapshot()).review_revision;
  async function grid() {
    if(await page.locator('#table-modal').isVisible()) return;
    await page.evaluate(()=>cmView.dispatch({selection:{anchor:3}}));
    await page.locator('[data-menu="md-insert-menu"]').click();
    await page.locator('#btn-insert-table').click();
    await page.locator(cell(1,0)).waitFor({state:'visible'});
  }
  async function focus(row,col) {
    await grid();
    await page.locator(cell(row,col)).click();
    await page.locator(cell(row,col)).focus();
  }
  async function record(expected,label) {
    await until(async()=> (await snapshot()).content===expected,label);
    assert.equal(await doc(),expected,label);
    states.push(expected);
    report.checks.push(label);
  }
  async function drag(axis,index,target,cancel=false) {
    await grid();
    const handle=page.locator('[data-table-resize="'+axis+'"][data-table-track="'+index+'"]');
    assert.equal(await handle.count(),1,axis+' track handle exists');
    await handle.scrollIntoViewIfNeeded();
    const current=Number(await handle.getAttribute('aria-valuenow'));
    assert(current>0,axis+' reported dimension');
    const box=await handle.boundingBox();
    assert(box && box.width>0 && box.height>0,axis+' hit target');
    const x=box.x+box.width/2, y=box.y+box.height/2;
    await page.mouse.move(x,y);
    await page.mouse.down();
    await page.mouse.move(x+(axis==='width'?target-current:0),y+(axis==='height'?target-current:0),{steps:7});
    if(cancel) await page.keyboard.press('Escape');
    await page.mouse.up();
  }
  async function pasteMime(type,value) {
    return page.evaluate(({type,value})=>{
      const data=new DataTransfer();data.setData(type,value);
      return !document.activeElement.dispatchEvent(new ClipboardEvent('paste',{clipboardData:data,bubbles:true,cancelable:true}));
    },{type,value});
  }
  try {
    await host.context.grantPermissions(['clipboard-read','clipboard-write'],{origin:new URL(page.url()).origin});
    await page.evaluate(p=>loadFile(p),file);
    await page.locator('#btn-edit').click();
    await until(async()=> (await doc())===initial,'open source');
    await grid();
    const initialRev=await revision();
    await drag('width',0,160,true);
    assert.equal(await doc(),initial); assert.equal(await revision(),initialRev);
    report.checks.push('Escape cancels the drag without changing source or creating undo history');
    await drag('width',0,160);
    await until(async()=> /\{width=\d+\}/.test(await doc()),'commit column width');
    const width=Number((await doc()).match(/\{width=(\d+)\}/)[1]);
    assert(Math.abs(width-160)<=2,'pointer displacement determines column width: '+width);
    await record(initial.replace('Group |','Group {width='+width+'} |'),'column drag affects only the intended header slot');
    await drag('height',5,96);
    await until(async()=> /\{height=\d+\}/.test(await doc()),'commit row height');
    const height=Number((await doc()).match(/\{height=(\d+)\}/)[1]);
    assert(Math.abs(height-96)<=2,'pointer displacement determines row height: '+height);
    const first=states.at(-1).replace('Lot05 |','Lot05 {height='+height+'} |');
    await record(first,'row drag persists a minimum height as one edit');
    report.widthPx=width;report.heightPx=height;
    await focus(1,0);
    await page.evaluate(()=>{
      window.tableClipboardEvents=[];
      for(const name of ['copy','paste']) document.addEventListener(name,e=>{
        window.tableClipboardEvents.push({name,target:e.target.tagName,focus:document.activeElement.tagName,trusted:e.isTrusted,types:Array.from(e.clipboardData?.types||[])});
      },true);
    });
    await page.evaluate(()=>navigator.clipboard.writeText('test clipboard'));
    await page.keyboard.press(mod+'+c');
    report.nativeFormats=await page.evaluate(async()=>(await navigator.clipboard.read()).flatMap(item=>item.types));
    await focus(3,0);
    await page.keyboard.press(mod+'+v');
    const copied=first.replace('Lot03 | B03','**North**'+span+' | Cold').replace('Lot04 | B04','Dark | Tube');
    await record(copied,'system copy and paste retains all four source cells, bold Markdown and rectangular merge');
    const events=await page.evaluate(()=>window.tableClipboardEvents);
    assert(events.some(e=>e.name==='copy'&&e.trusted) && events.some(e=>e.name==='paste'&&e.trusted));
    report.clipboard=events;
    assert.equal(await page.evaluate(()=>state.tabs.length),1,'paste inside the table must not open a clipboard document');
    await focus(1,0);
    assert(await pasteMime('text/csv','"West, A",Ice\r\nBlue,Jar'),'CSV event consumed');
    const csv=copied.replace('**North**'+span+' | Cold','West, A'+span+' | Ice').replace('Dark | Tube','Blue | Jar');
    await record(csv,'explicit CSV paste keeps quoted comma, merge and the separate copied rectangle');
    await focus(1,0);
    const before=await revision();
    await pasteMime('text/csv','X,Y,Z\nA,B,C\nD,E,F');
    assert.equal(await doc(),csv);
    assert.equal(await revision(),before);
    report.checks.push('a paste bisecting another merge is refused atomically');
    await focus(1,0);
    await page.evaluate(()=>navigator.clipboard.writeText('West\tDry\nRed\tVial'));
    await page.keyboard.press(mod+'+v');
    const final=csv.replace('West, A'+span+' | Ice','West'+span+' | Dry').replace('Blue | Jar','Red | Vial');
    await record(final,'system TSV paste replaces merged contents without changing sizing or the copied block');
    await focus(1,0);
    await page.locator('#table-cell-copy-csv').click();
    const plain=await page.evaluate(()=>navigator.clipboard.readText());
    assert.equal(plain.replace(/\r\n/g,'\n').trimEnd(),'West,Dry\nRed,Vial','Copy CSV is consumable as plain text');
    report.checks.push('Copy CSV returns all four cells as plain text for external programs');
    if(await page.locator('#table-modal').isVisible()) await page.locator('#table-cell-cancel').click();
    await page.locator('#edit-save').click();
    await until(()=>fs.readFileSync(file).equals(physical(final)),'physical saved source with BOM and CRLF');
    await until(()=>page.evaluate(()=>!state.editing),'return to reader');
    const table=page.locator('#content table').first();
    await until(()=>table.isVisible(),'reader paints table');
    const dimensions=await table.evaluate(node=>{
      const h=node.tHead.rows[0].cells[0].getBoundingClientRect().width;
      const r=node.tBodies[0].rows[4].getBoundingClientRect().height;
      return {width:h,height:r,merges:node.querySelectorAll('td[rowspan="2"][colspan="2"]').length};
    });
    assert.equal(dimensions.merges,2);
    assert(Math.abs(dimensions.width-width)<2 && dimensions.height>=height-1,'reader geometry: '+JSON.stringify(dimensions));
    report.reader=dimensions;report.savedSha256=sha(fs.readFileSync(file));
    const head=await snapshot();
    return {file,profile:host.profile,documents:host.documents,id:head.document_id,pid:host.host.pid};
  } catch (error) {
    report.observed=await page.evaluate(()=>({
      rows:cmView?.state.doc.toString().split('\n').slice(0,8),
      events:window.tableClipboardEvents,
      hint:document.querySelector('#table-cell-controls .hint')?.textContent,
    })).catch(()=>null);
    throw error;
  } finally {await host.close();}
}
exercise().then(result=>recover(result,{binary,output,states,physical})).then(history=>{
  report.history=history;report.passed=true;
}).catch(error=>{
  report.failure=String(error.stack||error).replaceAll(output,'<fixture>').replaceAll(process.cwd(),'<source>');
  process.exitCode=1;
}).finally(()=>{
  report.binarySha256=sha(fs.readFileSync(binary));
  report.bootSha256=sha(fs.readFileSync(path.join(__dirname,'../assets/readmd.boot.js')));
  fs.writeFileSync(path.join(output,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx'});
  console.log(JSON.stringify(report));
});
