'use strict';
const {test}=require('node:test'),assert=require('node:assert/strict');
const {initialBytes,expected:e,service,app,modal,lf,sha,clone}=require('./v007-table-cell-host.cjs');
const tick=()=>new Promise(setImmediate);
async function ready(options){const s=service(options),f=app(s);await f.ready();return f;}
async function until(fn){for(let i=0;i<200;i++){if(fn())return;await tick();}throw Error('Controlled UI did not settle');}
const manual=s=>s.calls.filter(x=>x.url==='/api/review'&&x.body.action==='edit');
function cellEdits(s){return manual(s).filter(x=>x.body.edits.some(edit=>edit.from===e.target.from&&edit.to===e.target.to));}

test('fixed BOM/CRLF input has two identical 3x3 tables and independent UTF16 target168..170',()=>{
 const bytes=initialBytes;assert.equal(sha(bytes),e.physical.initialSha256);assert.equal(bytes.subarray(0,3).toString('hex'),'efbbbf');assert(!/(^|[^\r])\n/.test(bytes.toString()));
 const lines=e.canonical.initial.split('\n');assert.deepEqual(lines.slice(2,6),lines.slice(9,13));assert.equal(lines[12],'| 21 | 23 | 24 |');assert.equal(e.canonical.initial.slice(168,170),'23');assert.equal(e.target.utf16Delta,'42😀'.length-2);
});
test('actual CM Table spans bind the second duplicate, not the first matching23',async()=>{
 const f=await ready(),hit=f.hit();assert.equal(hit.unsupported,undefined);assert.equal(hit.rows.length,3);assert(hit.rows.every(r=>r.length===3));
 assert.equal(hit.selected.row,2);assert.equal(hit.selected.column,1);assert.equal(hit.selected.from,168);assert.equal(hit.selected.to,170);assert.equal(hit.rows[1][1].original,'left\\|right');assert.equal(hit.rows[1][1].value,'left|right');
 f.select(65);const first=f.hit();assert(first.from<hit.from);assert.notEqual(first.selected.from,hit.selected.from);
});
test('one exact manual CM/FIFO edit leaves every other source byte and pending AI identity intact',async()=>{
 const f=await ready(),pending=await f.pending(),beforeCalls=f.calls.filter(x=>x.body.action==='propose').length,stamp=await f.capture();
 assert(Object.isFrozen(stamp));assert.equal(stamp.original,'23');assert.equal(stamp.document_id,'owned-document');assert(stamp.revision);assert(stamp.editorDoc===f.view.state.doc);
 await f.apply(stamp);assert.equal(f.text(),e.canonical.expected);assert.equal(manual(f.store).length,1);assert.equal(cellEdits(f.store).length,1);
 const request=manual(f.store)[0].body;assert.equal(request.origin,'editor');assert.deepEqual(request.edits,[{from:168,to:170,insert:'42😀'}]);assert.equal(request.document_id,stamp.document_id);assert.equal(request.generation,stamp.generation);assert.equal(request.review_revision,stamp.review_revision);
 assert.equal(f.calls.filter(x=>x.body.action==='propose').length,beforeCalls);assert.equal(f.calls.some(x=>x.body.action==='resolve'),false);
 const after=f.review.snapshot(f.tab).changes[0];for(const key of['id','original','replacement','decision','application','origin'])assert.deepEqual(after[key],pending[key]);assert.equal(after.from,pending.from+2);assert.equal(after.to,pending.to+2);
 assert.equal(f.view.transactions.filter(t=>t.isUserEvent('input.table.cell')).length,1);assert.equal(f.events.filter(x=>x.type==='readmd:document-transaction'&&x.detail.origin==='editor').length,1);
});
test('existing Save preserves physical BOM/CRLF and never accepts the outside AI proposal',async()=>{
 const f=await ready(),pending=await f.pending();await f.apply(await f.capture());assert.equal(await f.save(),true);
 assert.equal(sha(f.store.disk()),e.physical.savedSha256);assert.equal(f.tab.isDirty,false);assert.equal(f.calls.filter(x=>x.url==='/api/save').length,1);
 assert.equal(f.review.snapshot(f.tab).changes[0].id,pending.id);assert.equal(f.review.snapshot(f.tab).changes[0].decision,'pending');assert.equal(f.calls.some(x=>x.body.action==='resolve'),false);
});
test('a fresh adapter resume and durable-history protocol undo only the cell; disk waits explicit Save',async()=>{
 const first=await ready();await first.pending();await first.apply(await first.capture());assert.equal(await first.save(),true);const disk=Buffer.from(first.store.disk());
 const next=app(first.store);await next.ready();assert.equal(next.text(),e.canonical.expected);const pending=clone(next.review.snapshot(next.tab).changes[0]);
 await next.history(false);assert.equal(next.text(),e.canonical.pending);assert.deepEqual(first.store.disk(),disk);assert.equal(next.review.snapshot(next.tab).changes[0].id,pending.id);assert.equal(next.review.snapshot(next.tab).changes[0].decision,'pending');
 await next.history(true);assert.equal(next.text(),e.canonical.expected);assert.deepEqual(first.store.disk(),disk);
 assert.equal(first.store.calls.filter(x=>x.body.action==='undo').length,1);assert.equal(first.store.calls.filter(x=>x.body.action==='redo').length,1);
 // A fresh VM object is not a new Rust/OS process or SQLite certificate.
});
test('edit then durable undo equal-text ABA rejects the old Apply',async()=>{
 const f=await ready(),stamp=await f.capture(),before=f.text();f.type({from:0,insert:'X'});await f.review.flush(f.tab);await f.history(false);assert.equal(f.text(),before);assert(f.tab.editGeneration>stamp.editGeneration);
 const count=cellEdits(f.store).length;await assert.rejects(f.apply(stamp),{code:'review_stale_generation'});assert.equal(f.text(),before);assert.equal(cellEdits(f.store).length,count);
});
for(const [name,mutate]of[
 ['tab object reuse',f=>{f.state.tabs=[{...f.tab}];}],
 ['path change',f=>{f.tab.path+='-other';}],
 ['file revision',f=>{f.tab.revision='other-revision';}],
 ['CM editGeneration',f=>{f.tab.editGeneration++;}],
 ['lifecycle decision',f=>{f.context.ReadMDLifecycle={busy:()=>true};}],
 ['session restoring',f=>{f.context.ReadMDSession={restoring:()=>true};}],
 ['read-only collaboration',f=>{f.context.ReadMDCollaborationShare={canEdit:()=>false};}],
 ['read-only CM',f=>{f.view.dispatch({effects:f.CM.StateEffect.appendConfig.of(f.CM.EditorState.readOnly.of(true))});}]
])test('stale Apply refuses before mutation: '+name,async()=>{
 const f=await ready(),stamp=await f.capture(),before=f.text();mutate(f);await assert.rejects(f.apply(stamp),{code:'review_stale_generation'});assert.equal(f.text(),before);assert.equal(manual(f.store).length,0);
});
test('an externally advanced durable head rejects old Apply despite identical CM text',async()=>{
 const f=await ready(),stamp=await f.capture(),before=f.text();f.store.remoteHead();await assert.rejects(f.apply(stamp),{code:'review_stale_generation'});assert.equal(f.text(),before);assert.equal(manual(f.store).length,0);
});
test('an actual owned-file external change is reported by resume before any cell mutation',async()=>{
 const f=await ready(),stamp=await f.capture(),before=f.text(),outside='OUTSIDE OWNED CHANGE\r\n';f.store.external(outside);
 await assert.rejects(f.apply(stamp),{code:'review_stale_generation'});assert.equal(f.text(),before);assert.equal(f.store.disk().toString(),outside);assert.equal(manual(f.store).length,0);
});
test('tab change while physical resume is pending refuses old Apply',async()=>{
 const f=await ready(),stamp=await f.capture(),gate=f.store.pause('resume'),work=f.apply(stamp);await gate.entered.promise;f.state.tabs=[{...f.tab,id:'new-tab'}];f.state.activeTabId='new-tab';gate.release.resolve();
 await assert.rejects(work,{code:'review_stale_generation'});assert.equal(manual(f.store).length,0);assert.equal(f.text(),e.canonical.initial);
});
test('a filtered CM transaction cannot be reported as a committed manual cell',async()=>{
 const f=await ready(),stamp=await f.capture();f.view.dispatch({effects:f.CM.StateEffect.appendConfig.of(f.CM.EditorState.transactionFilter.of(tx=>tx.isUserEvent('input.table.cell')?[]:tx))});
 await assert.rejects(f.apply(stamp),{code:'review_conflict'});assert.equal(f.text(),e.canonical.initial);assert.equal(manual(f.store).length,0);
});
test('non-document CM effects preserve the exact target while split-surrogate spans are rejected',async()=>{
 const f=await ready(),stamp=await f.capture();f.view.dispatch({effects:f.CM.StateEffect.appendConfig.of([])});await f.apply(stamp);assert.equal(cellEdits(f.store).length,1);
 const other=await ready();await assert.rejects(other.review.captureManualRange(6,7,other.tab),{code:'review_conflict'});assert.equal(manual(other.store).length,0);
});
test('a manual persistence failure keeps the dirty draft and never writes disk or auto-replays',async()=>{
 const f=await ready(),stamp=await f.capture(),before=f.store.disk();f.store.fail('edit');await assert.rejects(f.apply(stamp),{code:'review_persistence_failed'});
 assert.equal(f.text(),e.canonical.initial.slice(0,168)+'42😀'+e.canonical.initial.slice(170));assert(f.tab.isDirty);assert.deepEqual(f.store.disk(),before);assert.equal(manual(f.store).length,1);
 await assert.rejects(f.apply(stamp),{code:'review_stale_generation'});assert.equal(manual(f.store).length,1);
});
test('lost acknowledgement retries only the same FIFO call ID and does not add another history entry',async()=>{
 const f=await ready(),stamp=await f.capture();f.store.fail('edit',true);await assert.rejects(f.apply(stamp),{code:'review_persistence_failed'});const first=clone(manual(f.store)[0].body);
 assert.equal(f.store.history.length,1);assert.equal(await f.review.retry(f.tab),true);assert.equal(manual(f.store).length,2);assert.deepEqual(manual(f.store)[1].body,first);assert.equal(f.store.history.length,1);
});
test('failed Save retains disk, pending AI and dirty corrected draft',async()=>{
 const f=await ready(),pending=await f.pending(),before=f.store.disk();await f.apply(await f.capture());f.store.fail('save');assert.equal(await f.save(),false);
 assert.deepEqual(f.store.disk(),before);assert.equal(f.text(),e.canonical.expected);assert(f.tab.isDirty);assert.equal(f.review.snapshot(f.tab).changes[0].id,pending.id);assert.equal(f.review.snapshot(f.tab).changes[0].decision,'pending');
});
test('existing modal presents nine real cell targets and applies only the selected second-table cell',async()=>{
 const f=await ready();await f.pending();const ui=modal(f);f.context.openTableModal();await until(()=>ui.get('table-cell-apply')&&!ui.get('table-cell-apply').disabled);
 assert.equal(ui.buttons().length,9);assert.equal(ui.get('table-cell-input').value,'23');ui.get('table-cell-input').value='42😀';await ui.get('table-cell-apply').click();
 assert.equal(f.text(),e.canonical.expected);assert.equal(manual(f.store).length,1);assert(ui.root.classList.contains('hidden'));
});
test('cancel and late capture cannot mutate or reopen the old modal',async()=>{
 const f=await ready(),gate=f.store.pause('resume'),ui=modal(f);f.context.openTableModal();await gate.entered.promise;ui.get('table-cell-cancel').click();gate.release.resolve();await tick();await tick();
 assert(ui.root.classList.contains('hidden'));assert.equal(ui.get('table-cell-controls'),null);assert.equal(manual(f.store).length,0);assert.equal(f.text(),e.canonical.initial);
});
for(const close of ['cancel','escape'])test('actual modal Apply pending at resume is cancelled by '+close+' before dispatch',async()=>{
 const f=await ready(),ui=modal(f),physical=Buffer.from(f.store.disk());f.context.openTableModal();await until(()=>ui.get('table-cell-apply')&&!ui.get('table-cell-apply').disabled);
 ui.get('table-cell-input').value='42😀';const gate=f.store.pause('resume'),work=ui.get('table-cell-apply').click();await gate.entered.promise;
 if(close==='cancel')ui.get('table-cell-cancel').click();else ui.escape();assert(ui.root.classList.contains('hidden'));gate.release.resolve();await work;
 assert.deepEqual({manualEdits:manual(f.store).length,cmUnchanged:f.text()===e.canonical.initial,diskUnchanged:f.store.disk().equals(physical)},{manualEdits:0,cmUnchanged:true,diskUnchanged:true});
});
for(const queued of [false,true])test('a '+(queued?'queued':'processing')+' Review operation during fresh resume invalidates old Apply',async()=>{
 const f=await ready(),ui=modal(f);f.context.openTableModal();await until(()=>ui.get('table-cell-apply')&&!ui.get('table-cell-apply').disabled);const base=await f.review.capture(f.tab);
 ui.get('table-cell-input').value='42😀';const gate=f.store.pause('resume'),work=ui.get('table-cell-apply').click();await gate.entered.promise;
 const backgroundGate=f.store.pause('propose'),at=f.text().length,background=f.review.propose([{from:at,to:at,insert:'\nBACKGROUND_ONLY\n'}],base,f.tab);await backgroundGate.entered.promise;
 if(queued)assert.equal(f.review.history(false),true);gate.release.resolve();await tick();await tick();const beforeBackground={cell:f.text().slice(168,170),cellEdits:cellEdits(f.store).length};
 backgroundGate.release.resolve();await Promise.all([background,work]);await f.review.flush(f.tab);
 assert.deepEqual(beforeBackground,{cell:'23',cellEdits:0});assert.equal(cellEdits(f.store).length,0);
});
test('unknown Markdown escapes are read-only instead of showing the wrong literal value',async()=>{
 const f=await ready();assert.equal(f.context.cmTableCellLiteral('\\!'),null);assert.equal(f.context.cmTableCellLiteral('left\\|right'),'left|right');assert.equal(f.context.cmTableCellLiteral('a\\\\b'),'a\\b');
});
test('cancelling after dispatch does not claim rollback of the authorized manual edit',async()=>{
 const f=await ready(),ui=modal(f),physical=Buffer.from(f.store.disk());f.context.openTableModal();await until(()=>ui.get('table-cell-apply')&&!ui.get('table-cell-apply').disabled);
 ui.get('table-cell-input').value='42😀';const gate=f.store.pause('edit'),work=ui.get('table-cell-apply').click();await gate.entered.promise;
 assert.equal(f.text().slice(168,172),'42😀');ui.get('table-cell-cancel').click();gate.release.resolve();await work;
 assert.equal(manual(f.store).length,1);assert.equal(f.text().slice(168,172),'42😀');assert.deepEqual(f.store.disk(),physical);
});
test('an unknown-escape selected cell is visibly read-only and has no active Apply',async()=>{
 const source=e.canonical.initial.slice(0,168)+'\\!'+e.canonical.initial.slice(170),f=await ready(source),ui=modal(f);f.context.openTableModal();
 assert.equal(ui.get('table-cell-input').disabled,true);assert.equal(ui.get('table-cell-apply').disabled,true);ui.get('table-cell-apply').click();assert.equal(f.text(),source);assert.equal(manual(f.store).length,0);
});
test('the existing alignment control still changes only its second-table delimiter token',async()=>{
 const f=await ready(),ui=modal(f);f.context.openTableModal();await until(()=>ui.get('table-cell-apply')&&!ui.get('table-cell-apply').disabled);
 const right=ui.get('table-align-controls').children.find(button=>button.dataset.tableAlign==='right');right.click();assert.equal(await f.review.flush(f.tab),true);
 const lines=e.canonical.initial.split('\n');lines[10]='| --- | ---: | ---: |';assert.equal(f.text(),lines.join('\n'));assert.equal(cellEdits(f.store).length,0);assert(ui.root.classList.contains('hidden'));
});
test('outside a source table the original insertion picker remains available and inert until a choice',async()=>{
 const f=await ready(),ui=modal(f);f.select(0);f.context.openTableModal();assert.equal(ui.get('table-grid-picker').children.length,100);assert.equal(ui.get('table-cell-apply'),null);assert.equal(f.text(),e.canonical.initial);assert.equal(manual(f.store).length,0);
});
for(const [name,source,marker]of[
 ['ragged', '| A | B | C |\n| --- | --- | --- |\n| 1 | 2 |\n','1'],
 ['HTML merged','<table><tr><td colspan="2">merged</td></tr></table>\n','merged'],
 ['nested quote','> | A | B |\n> | --- | --- |\n> | 1 | 2 |\n','1'],
 ['oversized cells','| A | B |\n| --- | --- |\n| '+ 'X'.repeat(65537)+' | 2 |\n','2']
])test('complex or unknown geometry is explicitly refused: '+name,async()=>{
 const f=await ready(source);f.select(lf(source).lastIndexOf(marker));const hit=f.hit();assert.equal(hit?.unsupported,true);const ui=modal(f);f.context.openTableModal();assert.equal(ui.get('table-cell-apply'),null);assert.equal(manual(f.store).length,0);assert.equal(f.text(),lf(source));
});
test('unsupported rich cells remain source-exact and replacement rejects newlines or broken surrogate values',async()=>{
 const f=await ready();assert.equal(f.context.cmTableCellLiteral('**bold**'),null);assert.equal(f.context.cmTableCellLiteral('left\\|right'),'left|right');assert.equal(f.context.cmTableCellLiteral('&#124; &#92; &amp;'),'| \\ &');
 for(const value of['line\nnext','line\rnext','\ud800','\udc00','a\0b'])assert.equal(f.context.cmTableCellReplacement(value),null);
 assert.equal(f.context.cmTableCellReplacement('42😀'),'42😀');
});
