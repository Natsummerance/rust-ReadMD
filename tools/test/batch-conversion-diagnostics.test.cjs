'use strict';
// Exercise real row rendering and message localization; no backend replies
// are used as evidence of a successful production conversion.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const root=path.resolve(__dirname,'../..');
function environment(locale){
  const strings=JSON.parse(fs.readFileSync(path.join(root,'assets/i18n',locale+'.json'),'utf8'));
  const elements=new Map(),context={window:{i18n:{t:key=>strings[key]??key}},document:{querySelector:()=>null},localStorage:{},setTimeout,clearTimeout,AbortController,TextEncoder,console};
  context.$=id=>{if(!elements.has(id))elements.set(id,{textContent:''});return elements.get(id);};
  context.document.getElementById=context.$;
  vm.createContext(context);
  for(const file of ['assets/js/core/state.js','assets/js/features/batch.js'])vm.runInContext(fs.readFileSync(path.join(root,file),'utf8'),context,{filename:file});
  function add(source){const row={dataset:{},className:'',title:'',attributes:{},state:{textContent:'',title:''},querySelector(){return this.state;},setAttribute(key,value){this.attributes[key]=value;}};Object.defineProperty(row,'innerHTML',{set(){throw Error('Technical details must remain plain text');}});context.source=source;context.row=row;vm.runInContext('batchRowsBySrc[source]=row;',context);return row;}
  function render(items){context.items=items;vm.runInContext('renderBatchProgress({items,running:false});',context);}
  return {strings,context,add,render};
}
test('typed conversion diagnostics keep the localized summary and bounded native locator accessible as plain text',()=>{
  for(const locale of ['zh-CN','en','ar']){
    const e=environment(locale),row=e.add('owned.docx');
    const details={part:'word/notes/footnote-custom.xml',path:'footnote:7:[0]',detail:'mixed run font sizes require an inline size model'};
    e.render([{src:'owned.docx',status:'error',error_code:'docx_note_settings_unsupported',error:'docx_note_settings_unsupported',error_details:details}]);
    assert.equal(row.title,[e.strings['error.docx_note_settings_unsupported'],...Object.values(details)].join('\n'));
    assert.equal(row.state.title,row.title);assert.equal(row.state.textContent,e.strings['batch.statusError']);assert.equal(row.dataset.done,'1');
    assert.equal(row.tabIndex,0);assert.equal(row.attributes['aria-description'],row.title);assert.notEqual(row.title,'docx_note_settings_unsupported');
  }
});
test('only registered bounded diagnostics enter the existing tooltip, never arbitrary objects or control payloads',()=>{
  const valid={part:'word/footnotes.xml',path:'footnote:7:[0]',detail:'unsupported author format'};
  for(const details of [null,[],{...valid,extra:'unexpected'},{...valid,part:42},{...valid,path:''},{...valid,detail:'bad\nline'},{...valid,detail:'bad\u0085line'},{...valid,part:'字'.repeat(171)},{...valid,path:'x'.repeat(2049)},{...valid,detail:'x'.repeat(2049)}]){
    const e=environment('en'),row=e.add('bad.docx');e.render([{src:'bad.docx',status:'error',error_code:'docx_note_settings_invalid',error_details:details}]);assert.equal(row.title,e.strings['error.docx_note_settings_invalid']);
  }
  const e=environment('en'),row=e.add('unknown.docx');e.render([{src:'unknown.docx',status:'error',error_code:'conversion_failed',error_details:valid}]);assert.equal(row.title,e.strings['error.conversion_failed']);
  const literal=environment('en'),safe=literal.add('literal.docx');literal.render([{src:'literal.docx',status:'error',error_code:'docx_note_settings_unsupported',error_details:{...valid,path:'<img src=x onerror=alert(1)>'}}]);assert(safe.title.includes('<img src=x onerror=alert(1)>'));
});
test('registered numbering reasons are bounded identifiers and legacy shapes cannot add details',()=>{
  for(const reason of ['abstract_numbering_missing','style_cycle']){
    const e=environment('en'),row=e.add(reason+'.docx');e.render([{src:reason+'.docx',status:'error',error_code:'docx_numbering_unrepresentable',error_details:{reason}}]);assert.equal(row.title,e.strings['error.docx_numbering_unrepresentable']+'\n'+reason);
  }
  for(const details of [{reason:'x\nsecret'},{reason:'x'.repeat(129)},{reason:'<html>'},{reason:1},{reason:'ok',extra:'not allowed'}]){
    const e=environment('en'),row=e.add('number.docx');e.render([{src:'number.docx',status:'error',error_code:'docx_numbering_invalid',error_details:details}]);assert.equal(row.title,e.strings['error.docx_numbering_invalid']);
  }
});
test('legacy diagnostics remain plain text and terminal batch rows cannot regress',()=>{
  const e=environment('en'),failed=e.add('old.docx'),done=e.add('good.docx');
  e.render([{src:'old.docx',status:'error',error:'Legacy converter unavailable'}, {src:'good.docx',status:'ok',out:'good.md'}]);
  assert.equal(failed.title,'Legacy converter unavailable');assert.equal(done.dataset.out,'good.md');
  e.render([{src:'old.docx',status:'running'},{src:'good.docx',status:'error',error_code:'conversion_failed'}]);
  assert.equal(failed.className,'batch-item error');assert.equal(done.className,'batch-item ok');assert.equal(done.dataset.out,'good.md');
});
test('missing error text gets the localized generic conversion explanation',()=>{
  const e=environment('zh-CN'),row=e.add('empty.docx');e.render([{src:'empty.docx',status:'error'}]);
  assert.equal(row.title,e.strings['error.conversion_failed']);assert(row.title.length>0);
});
