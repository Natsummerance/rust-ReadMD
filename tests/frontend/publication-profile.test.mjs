import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

function model() {
  const context = vm.createContext({window:{}});
  vm.runInContext(fs.readFileSync('assets/js/editor/publication-profile.js','utf8'),context);
  return context.window.ReadMDPublicationProfile;
}
const plain = value => JSON.parse(JSON.stringify(value));

test('document inheritance does not enter remembered choices or the next document',()=>{
  const api=model(), remembered={footnotes:{size:8}}, source={footnotes:{start:4,kinds:{footnote:{size:10},endnote:{size:11}},kindByLabel:{'001': 'endnote'}}};
  const a=api.create(source,remembered);
  assert.equal(a.display('footnotes.size'),10);
  const raw=plain(a.collect({page:{size:'A4'}}));
  assert.deepEqual(raw.footnotes,{size:8});
  const request=plain(a.request(raw));
  assert.equal(request.footnotes.kinds.footnote.size,10);
  assert.equal(request.footnotes.kinds.endnote.size,11);
  assert.equal(request.footnotes.start,4);
  assert.equal(request.footnotes.kindByLabel['001'],'endnote');
  assert.equal(api.create({},raw).display('footnotes.size'),8);
  assert.deepEqual(remembered,{footnotes:{size:8}});
  assert.equal(source.footnotes.kinds.endnote.size,11);
});
test('a field edit affects the selected story and not the other note type',()=>{
  const a=model().create({footnotes:{kinds:{footnote:{size:10,start:0},endnote:{size:12,start:65535}}}},{});
  assert.equal(a.display('footnotes.start'),0);
  a.selectKind('endnote');assert.equal(a.display('footnotes.start'),65535);
  a.change('footnotes.size',30);
  const req=plain(a.request({})),saved=plain(a.collect({}));
  assert.equal(req.footnotes.kinds.footnote.size,10);
  assert.equal(req.footnotes.kinds.endnote.size,30);
  assert.equal(req.footnotes.kinds.endnote.start,65535);
  assert.deepEqual(saved.footnotes,{kinds:{endnote:{size:30}}});
});

test('remembered kinds without source kinds display their exact current policy',()=>{
  const remembered={footnotes:{kinds:{footnote:{size:30,start:12000,lineSpacing:{mode:'exact',pt:14.25}},endnote:{size:31,start:65535}}}},before=plain(remembered);
  const a=model().create({},remembered);
  assert.equal(a.hasKinds,true);assert.equal(a.kind(),'footnote');
  assert.equal(a.display('footnotes.size'),30);assert.equal(a.display('footnotes.start'),12000);
  assert.deepEqual(plain(a.display('footnotes.lineSpacing')),{mode:'exact',pt:14.25});
  assert.equal(a.controlKey('footnotes.size'),'footnotes.kinds.footnote.size');
  assert.deepEqual(plain(a.collect({})),before,'displaying a remembered policy must not add an override');
  a.selectKind('endnote');assert.equal(a.display('footnotes.size'),31);assert.equal(a.display('footnotes.start'),65535);
  a.change('footnotes.size',32);
  const request=plain(a.request({}));assert.equal(request.footnotes.kinds.footnote.size,30);assert.equal(request.footnotes.kinds.endnote.size,32);assert.equal(request.footnotes.kinds.endnote.start,65535);
  assert.deepEqual(remembered,before);
});

test('preset changes dynamically enable and release kind-specific controls',()=>{
  const a=model().create({},{footnotes:{size:8}});
  assert.equal(a.hasKinds,false);
  const preset={footnotes:{kinds:{endnote:{size:30,start:65535,lineSpacing:{mode:'atLeast',pt:20}}}}},before=plain(preset);
  a.selectPreset(preset);
  assert.equal(a.hasKinds,true);assert.equal(a.kind(),'endnote');assert.equal(a.display('footnotes.size'),30);assert.equal(a.display('footnotes.start'),65535);
  assert.equal(a.controlKey('footnotes.start'),'footnotes.kinds.endnote.start');
  a.change('footnotes.start',0);assert.equal(a.display('footnotes.start'),0);assert.deepEqual(plain(a.display('footnotes.lineSpacing')),{mode:'atLeast',pt:20});
  assert.deepEqual(preset,before);
  a.selectPreset({footnotes:{size:9,start:4}});
  assert.equal(a.hasKinds,false);assert.equal(a.controlKey('footnotes.size'),'footnotes.size');assert.equal(a.display('footnotes.size'),9);assert.equal(a.display('footnotes.start'),4);
  assert.deepEqual(plain(a.collect({})),{footnotes:{size:9,start:4}});
  a.selectPreset({footnotes:{kinds:{footnote:{size:10}}}});assert.equal(a.hasKinds,true);assert.equal(a.kind(),'footnote');assert.equal(a.display('footnotes.size'),10);
});

test('flat source inheritance stays out of Last while remembered kind controls stay independent',()=>{
  const source={footnotes:{size:11,start:4}},remembered={footnotes:{kinds:{endnote:{size:30,start:65535}}}};
  const a=model().create(source,remembered);assert.equal(a.hasKinds,true);assert.equal(a.display('footnotes.size'),11);
  assert.deepEqual(plain(a.collect({})),remembered);
  a.selectKind('endnote');a.change('footnotes.size',20);
  const request=plain(a.request({})),saved=plain(a.collect({}));assert.equal(request.footnotes.kinds.footnote.size,11);assert.equal(request.footnotes.kinds.endnote.size,20);assert.equal(request.footnotes.kinds.endnote.start,4);
  assert.deepEqual(saved,{footnotes:{kinds:{endnote:{size:20,start:65535}}}});assert.equal(source.footnotes.size,11);
});
test('layered legacy fields override lower kind values and typed unions stay atomic',()=>{
  const api=model();
  const merged=plain(api.mergeNotes({kinds:{footnote:{size:10,lineSpacing:{mode:'exact',pt:11}}}}, {size:9,lineHeight:1.2}));
  assert.equal(merged.kinds.footnote.size,9);
  assert.equal(merged.kinds.endnote.size,9);
  assert.equal(merged.kinds.footnote.lineSpacing,undefined);
  assert.equal(merged.kinds.footnote.lineHeight,1.2);
  const changed=plain(api.mergeNotes({lineSpacing:{mode:'exact',pt:11}},{lineSpacing:{mode:'multiple',multiple:1.5}}));
  assert.deepEqual(changed.lineSpacing,{mode:'multiple',multiple:1.5});
});
test('explicit preset and explicit false/zero/empty values win without copying source',()=>{
  const a=model().create({footnotes:{start:4,size:10,continuationNotice:'from-source'}},{footnotes:{size:8}});
  a.selectPreset({footnotes:{size:9}});a.change('footnotes.start',0);a.change('footnotes.continuationNotice','');
  assert.equal(a.display('footnotes.size'),9);
  assert.equal(a.display('footnotes.start'),0);
  assert.equal(a.display('footnotes.continuationNotice'),'');
  assert.deepEqual(plain(a.collect({})).footnotes,{size:9,start:0,continuationNotice:''});
});
test('null means inherit and ordinary documents remain sparse',()=>{
  const api=model(),empty=api.create({},{});
  assert.deepEqual(plain(empty.collect({})),{});
  assert.deepEqual(plain(empty.request({})),{});
  const a=api.create({footnotes:{start:4}},{});a.selectPreset({footnotes:{start:null}});
  assert.equal(a.display('footnotes.start'),4);
});
test('choosing a new preset releases the old remembered note fallback',()=>{
  const api=model();
  const a=api.create({}, {footnotes:{position:'document',start:23,size:12}});
  a.selectPreset({});
  assert.deepEqual(plain(a.collect({})),{});
  assert.deepEqual(plain(a.request({})),{});
  const b=api.create({footnotes:{start:4,size:10}}, {footnotes:{start:23,size:12}});
  b.selectPreset({footnotes:{size:9}});
  assert.deepEqual(plain(b.request({})).footnotes,{start:4,size:9});
  assert.deepEqual(plain(b.collect({})).footnotes,{size:9});
});
test('legal special note labels survive merge without writing to a prototype',()=>{
  const source=JSON.parse('{"footnotes":{"kindByLabel":{"__proto__":"endnote","constructor":"footnote"}}}');
  const a=model().create(source,{footnotes:{kindByLabel:{ordinary:'footnote'}}});
  const result=plain(a.request({}));
  assert(Object.hasOwn(result.footnotes.kindByLabel,'__proto__'));
  assert.equal(result.footnotes.kindByLabel.__proto__,'endnote');
  assert.equal(result.footnotes.kindByLabel.constructor,'footnote');
  assert.equal({}.ordinary,undefined);
});
test('AI option paths and nested merges cannot modify a prototype',()=>{
  const context=vm.createContext({window:{addEventListener(){}},AbortController,AbortSignal,setTimeout,clearTimeout,console});
  vm.runInContext(fs.readFileSync('assets/js/features/export.js','utf8'),context);
  for (const payload of ['{"table.__proto__.polluted":true}','{"table":{"__proto__":{"polluted":true}}}','{"table.constructor.prototype.polluted":true}']) {
    context.payload=payload;
    vm.runInContext('expDeepMerge({},normalizeExportAiPayload(JSON.parse(payload)))',context);
    assert.equal(vm.runInContext('({}).polluted',context),undefined);
  }
  const exact=vm.runInContext('expDeepMerge({footnotes:{kindByLabel:{ordinary:"footnote"}}},JSON.parse(\'{"footnotes":{"kindByLabel":{"__proto__":"endnote"}}}\'))',context);
  assert.equal(plain(exact).footnotes.kindByLabel.__proto__,'endnote');
});
test('a delayed profile response cannot attach to another document with identical text',async()=>{
  let respond;
  const pending=new Promise(resolve=>{respond=resolve;});
  const context=vm.createContext({window:{addEventListener(){},READMD_ENGINE:'rust'},AbortController,AbortSignal,setTimeout,clearTimeout,console,
    state:{mode:'file',original:'Same text',activeTabId:'a',file:'a.md',sourceName:'a.md',export:{options:{}}},
    apiFetch:()=>pending,showToast(){},i18n:{t:k=>k}});
  vm.runInContext(fs.readFileSync('assets/js/editor/publication-profile.js','utf8'),context);
  vm.runInContext(fs.readFileSync('assets/js/features/export.js','utf8'),context);
  const result=vm.runInContext('exportOpenEpoch=1;prepareExportProfile(1)',context);
  context.state.activeTabId='b';context.state.file='b.md';context.state.sourceName='b.md';
  respond({ok:true,json:async()=>({ok:true,options:{footnotes:{start:4}}})});
  assert.equal(await result,false);
  assert.equal(vm.runInContext('exportProfileSession()',context),null);
});

test('export diagnostics reuse localized error messages and retain unknown warning codes',()=>{
  const element=()=>({children:[],appendChild(child){this.children.push(child);},classList:{add(){},remove(){}}});
  const box=element(),translations={'error.docx_numbering_import_conflict':'编号内容已保留，请检查自动引用。'};
  const i18n={t:key=>translations[key]||key};
  const context=vm.createContext({window:{addEventListener(){},i18n},i18n,document:{createElement:element},
    $:id=>id==='export-warns'?box:null,AbortController,AbortSignal,setTimeout,clearTimeout,console});
  vm.runInContext(fs.readFileSync('assets/js/features/export.js','utf8'),context);
  vm.runInContext('renderExportWarns(["docx_numbering_import_conflict","unknown_future_warning","docx_numbering_import_conflict","docx_numbering_import_conflict: tbl:current","unknown_future_warning: preserve details"])',context);
  assert.equal(box.children[1].children[0].textContent,translations['error.docx_numbering_import_conflict']+' ×2');
  assert.equal(box.children[1].children[1].textContent,'unknown_future_warning');
  assert.equal(box.children[1].children[2].textContent,translations['error.docx_numbering_import_conflict']+' · tbl:current');
  assert.equal(box.children[1].children[3].textContent,'unknown_future_warning: preserve details');
});

test('native exact note spacing is shown as inherited points rather than a false one-line multiplier',()=>{
  const session=model().create({footnotes:{kinds:{footnote:{lineSpacing:{mode:'exact',pt:10.5}}}}},{});
  const input={dataset:{k:'footnotes.lineHeight',default:'1'},type:'number',value:'old',placeholder:''};
  const i18n={t:key=>({'export.lineSpacingExact':'固定值','pm.inherit':'继承'}[key]||key)};
  const context=vm.createContext({window:{addEventListener(){},i18n},i18n,session,
    state:{export:{options:{},defaults:{}}},document:{querySelectorAll:()=>[input],querySelector:()=>null},
    $:()=>null,AbortController,AbortSignal,setTimeout,clearTimeout,console});
  vm.runInContext(fs.readFileSync('assets/js/features/export.js','utf8'),context);
  vm.runInContext('exportProfileContext={session};validateEpubNumericControls=()=>true;applyExportOptionsToDom()',context);
  assert.equal(input.value,'');assert.equal(input.placeholder,'固定值 · 10.5 pt');
  assert.deepEqual(plain(session.request({})).footnotes.kinds.footnote.lineSpacing,{mode:'exact',pt:10.5});
  session.change('footnotes.lineHeight',1.5);
  vm.runInContext('applyExportOptionsToDom()',context);
  assert.equal(input.value,1.5);assert.equal(input.placeholder,'');
  assert.equal(plain(session.request({})).footnotes.kinds.footnote.lineSpacing,undefined);
});

test('export controls preserve remembered native ranges and spacing while changing the displayed kind',()=>{
  const remembered={footnotes:{kinds:{footnote:{size:30,start:0,lineSpacing:{mode:'exact',pt:14.25}},endnote:{size:31,start:65535,lineSpacing:{mode:'atLeast',pt:20}}}}},before=plain(remembered);
  const session=model().create({},remembered),size={dataset:{k:'footnotes.size',default:'9'},type:'number',tagName:'INPUT',min:'6',max:'24',value:''},start={dataset:{k:'footnotes.start',default:'1'},type:'number',tagName:'INPUT',min:'1',max:'9999',value:''},spacing={dataset:{k:'footnotes.lineHeight',default:'1'},type:'number',tagName:'INPUT',min:'1',max:'3',value:''};
  const i18n={t:key=>({'export.lineSpacingExact':'Exact','export.lineSpacingAtLeast':'At least','pm.inherit':'Inherit'}[key]||key)};
  const context=vm.createContext({window:{addEventListener(){},i18n},i18n,session,
    state:{export:{options:plain(remembered),defaults:{}}},document:{querySelectorAll:()=>[size,start,spacing],querySelector:()=>null},
    $:()=>null,AbortController,AbortSignal,setTimeout,clearTimeout,console});
  vm.runInContext(fs.readFileSync('assets/js/features/export.js','utf8'),context);
  vm.runInContext('exportProfileContext={session};applyExportOptionsToDom()',context);
  assert.equal(size.value,30);assert.equal(size.max,'30');assert.equal(start.value,0);assert.equal(start.min,'0');assert.equal(spacing.value,'');assert.equal(spacing.placeholder,'Exact · 14.25 pt');
  assert.deepEqual(plain(vm.runInContext('collectExportOptions()',context)).footnotes,before.footnotes);
  session.selectKind('endnote');vm.runInContext('applyExportOptionsToDom()',context);
  assert.equal(size.value,31);assert.equal(size.max,'31');assert.equal(start.value,65535);assert.equal(start.max,'65535');assert.equal(spacing.placeholder,'At least · 20 pt');
  const collected=plain(vm.runInContext('collectExportOptions()',context));assert.deepEqual(collected.footnotes,before.footnotes);assert.deepEqual(plain(session.request(collected)).footnotes,before.footnotes);assert.deepEqual(remembered,before);
});
