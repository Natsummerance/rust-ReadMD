'use strict';
const {test}=require('node:test'),assert=require('node:assert/strict'),path=require('node:path');
const lane=require('./batch3-typst-baseline-stdio.cjs');

test('P04 plan is bounded, candidate-independent and does not claim GUI/cold/full acceptance',()=>{
 const value=lane.planFor();assert(value.noProcessesStarted&&value.noFilesWritten);assert.equal(value.guiStarted,false);assert.equal(value.fullP04Certified,false);assert.equal(value.fullBatch3Certified,false);assert.equal(value.coldProfileCertified,false);
 assert.equal(value.positivePdfFiles,18);assert.equal(value.stdioSessions,2);assert.equal(value.httpSessions,1);assert.equal(value.cases.length,8);assert.equal(value.coordinateTolerancePt,.002);
 assert(value.noInkWhitespace.includes('empty'));assert(lane.argumentsFor(['--plan'],{}).plan);
});
test('baseline CLI accepts only fresh bounded owned output shape',()=>{
 for(const args of [['--plan','--gui'],['--candidate','x'],['--plan','--output',path.join(lane.BASE,'../escape')],['--plan','--output',path.join(lane.BASE,'old-output')]])assert.throws(()=>lane.argumentsFor(args,{}));
 assert.equal(lane.argumentsFor(['--plan','--output',path.join(lane.BASE,'p04-controls')],{}).output,path.join(lane.BASE,'p04-controls'));
});
test('eight cases cover zero signed fractional rises and all four line spacing contracts',()=>{
 const rows=lane.cases();assert.deepEqual([...new Set(rows.map(row=>row.rise))].sort((a,b)=>a-b),[-2.5,0,3,3.25]);assert.deepEqual(new Set(rows.map(row=>row.spacing?.mode||'default')),new Set(['default','multiple','exact','atLeast']));
 for(const row of rows){const zero=lane.optionsFor(row,true),shift=lane.optionsFor(row);assert.equal(zero.typography.font,'Arial');assert.equal(shift.typography.font,'Arial');assert(!Object.hasOwn(zero.typography,'baselineShiftPt'));assert.equal(shift.typography.baselineShiftPt,row.rise);assert(!Object.hasOwn(shift.typography,'lineHeight'));delete shift.typography.baselineShiftPt;assert.deepEqual(shift,zero);}
 const rich=rows.find(row=>row.kind==='rich');assert.equal(rich.localFont,'Consolas');assert.equal(rich.font,undefined);assert.equal(lane.material('rich',3.25).contract.requirePhysicalItalic,true);
});

test('rich mono selection is an exact public paragraph patch while every global font remains supported Arial',()=>{
 const rich=lane.cases().find(row=>row.kind==='rich'),sample=lane.material(rich.kind,rich.rise),selected=lane.localFontPatchFor(rich);assert.deepEqual(selected,{title:'PLAIN_START',patch:{text:{font:'Consolas'}}});assert(sample.source.includes('PLAIN_START gjpqy **BOLDgjpy** and *EMgjpy*'));assert(!sample.source.includes('readmd:layout'));
 assert.deepEqual(sample.contract.segments.filter(row=>row.expectedSize===17||row.expectedSize===23).map(row=>row.expectedSize),[17,23]);assert.equal(sample.contract.requirePhysicalItalic,true);
 for(const entry of lane.cases())if(entry.kind!=='rich')assert.equal(lane.localFontPatchFor(entry),null);
 for(const bad of [{...rich,kind:'plain'},{...rich,localFont:'Times New Roman'}])assert.throws(()=>lane.localFontPatchFor(bad));
 for(const zero of [true,false]){const request=lane.exportRequest(rich,'actual API-patched source','rich.pdf','documents',zero);assert.equal(request.markdown_content,'actual API-patched source');assert.equal(request.options.typography.font,'Arial');assert.equal(request.style_preset,lane.PRESETS.mono[zero?'zero':'shift']);}
});
test('fixed complete corpus binds every segment including Code and all duplicate URI links',()=>{
 const protectedText=lane.material('protected',3);assert(protectedText.source.includes('`CODEgjpy`')&&protectedText.source.includes('$x^2+1$'));assert.equal(protectedText.contract.segments.filter(row=>row.role==='protected').length,1);assert(protectedText.contract.segments.filter(row=>row.role==='protected').every(row=>row.rise===0));
 for(const entry of lane.cases()){const value=lane.material(entry.kind,entry.rise);assert(value.contract.segments.every(row=>row.text&&row.block));assert.equal(value.contract.segments[0].rise,0);assert.equal(value.contract.segments[0].anchor,'anchor');}
 const links=lane.material('links',-2.5);assert.equal(links.contract.links.a.uri,links.contract.links.b.uri);assert.equal(links.contract.links.a.minPages,2);assert.equal(links.contract.links.b.minPages,2);assert.equal(links.contract.links.jump.goto,'anchor');assert.equal(links.contract.minPages,2);
});
test('four presets preserve unrelated data and last selection without modifying the input',()=>{
 const before={custom:{KEEP:{typography:{font:'Keep original'}}},last:{format:'docx',style:'KEEP'}},raw=JSON.stringify(before),after=lane.presetOverlay(raw);assert.deepEqual(JSON.parse(raw),before);assert.deepEqual(after.custom.KEEP,before.custom.KEEP);assert.deepEqual(after.last,before.last);
 const keys=Object.values(lane.PRESETS).flatMap(row=>Object.values(row));assert.equal(new Set(keys).size,4);for(const key of keys){assert.equal(after.custom[key].pdf.engine,'typst');assert.equal(after.custom[key].typography.font,'Arial');}assert.throws(()=>lane.presetOverlay(JSON.stringify(after)),/collision/);
});
test('actual PDF request keeps exact engine/options and zero control omits the baseline field',()=>{
 for(const row of lane.cases()){const source=lane.material(row.kind,row.rise).source;for(const zero of [true,false]){const request=lane.exportRequest(row,source,'output.pdf','documents',zero);assert.equal(request.markdown_content,source);assert.equal(request.output_format,'pdf');assert.equal(request.options.pdf.engine,'typst');assert.equal(request.base_dir,'documents');assert.equal(request.confirm,true);assert.equal(Object.hasOwn(request.options.typography,'baselineShiftPt'),!zero);}}
});
const PNG='iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4//8/AAX+Av4N70a4AAAAAElFTkSuQmCC';
function preview(source){return {ok:true,key:'a'.repeat(64),format:'pdf',mode:'artifact',pdf:Buffer.from('%PDF pure schema').toString('base64'),firstPage:PNG,pages:1,previewEngine:'typst',sourceMap:{revision:lane.sha(source),nodes:[{id:'p',kind:'paragraph',from:0,to:source.length,digest:lane.sha(source)}],hits:[{id:'p',page:1,rect:[10,20,40,16]}],pages:[[100,200]]}};}
test('HTTP schema binds full source revision, exact UTF16 node digest and real positive pages',()=>{
 const source='Body 😀 astral.\n',value=preview(source);assert(lane.validatePreview(value,source).length>0);
 for(const mutate of [v=>v.sourceMap.revision='old',v=>v.sourceMap.nodes[0].digest='wrong',v=>v.sourceMap.nodes[0].to++,v=>v.sourceMap.hits[0].id='unknown',v=>v.sourceMap.hits[0].page=0,v=>v.sourceMap.hits[0].rect[2]=0,v=>v.pages=1.5,v=>v.previewEngine='rust',v=>v.format='docx',v=>v.mode='shared-layout',v=>v.key='old',v=>v.pdf='YWJj===',v=>v.firstPage=null,v=>v.firstPage='',v=>v.firstPage=Buffer.from('NOT_PNG').toString('base64')]){const bad=structuredClone(value);mutate(bad);assert.throws(()=>lane.validatePreview(bad,source));}
});

test('real cached page request and response bind exact key and 1-based page rather than a PNG header alone',()=>{
 const key='b'.repeat(64);assert.deepEqual(lane.previewPageRequest(key,3),{key,page:3});const value={ok:true,key,page:3,png:PNG};assert.deepEqual(lane.validatePreviewPage(value,key,3),Buffer.from(PNG,'base64'));
 for(const mutate of [v=>v.key='c'.repeat(64),v=>v.page=2,v=>v.page='3',v=>v.ok=false,v=>v.png=null,v=>v.png='',v=>v.png=PNG+'===']){const bad=structuredClone(value);mutate(bad);assert.throws(()=>lane.validatePreviewPage(bad,key,3));}
 for(const page of [0,-1,1.5,'1',9])assert.throws(()=>lane.previewPageRequest(key,page));assert.throws(()=>lane.previewPageRequest('unknown-key',1));
});

test('visible PNG contract fixes measured WinRT AA bounds before actual and covers every cached page',()=>{
 const expected={weakContrast:8,strongContrast:32,coreContrast:192,strongRadiusPx:1,fringeRadiusPx:2,hintingRadiusPx:2,maxHintingFraction:0.001,coreRgbDelta:64,edgeRgbDelta:128},http=lane.planFor().http;assert.equal(http.visibleFirstAndAllCachedPages,true);assert.equal(http.pageRoute,'/api/export/preview');assert.equal(http.renderer,'Windows.Data.Pdf (WinRT)');assert.deepEqual(http.aa,expected);
 const altered=lane.planFor();altered.http.aa.strongRadiusPx=20;assert.deepEqual(lane.planFor().http.aa,expected);assert.equal(lane.planFor().positivePdfFiles,18);
});
test('source-map physical coverage requires each actual glyph, actual block and page',()=>{
 const source='PLAIN_ONE glyph.\n',value=preview(source),proof={pages:1,pageFrames:[[100,200]],corpusInk:[{block:'PLAIN_ONE',page:1,ink:[15,22,20,30]}]};assert(lane.assertSourceMap(value.sourceMap,source,proof));
 for(const mutate of [v=>v.hits[0].rect=[50,100,1,1],v=>v.hits[0].page=2,v=>v.pages[0][1]=201]){const bad=structuredClone(value.sourceMap);mutate(bad);assert.throws(()=>lane.assertSourceMap(bad,source,proof));}
});
test('typed ink errors retain page object, nullable identities and finite ordered ink without admitting generic strings',()=>{
 const value={ok:false,error_code:'publication_layout_geometry',details:{coordinateSpace:'pdfBaselineInk',blockId:'p',page:1,pageObject:22,reason:'ink-collision',ink:[1,2,3,4],otherBlockId:null}};assert.equal(lane.geometryError(value,'ink-collision'),value.details);
 for(const mutate of [v=>v.details.coordinateSpace='source',v=>v.details.page=0,v=>v.details.pageObject=0,v=>v.details.ink=[3,2,1,4],v=>v.details.ink[0]=Infinity,v=>v.details.otherBlockId='',v=>v.details.extra=true,v=>v.details='opaque']){const bad=structuredClone(value);mutate(bad);assert.throws(()=>lane.geometryError(bad,'ink-collision'));}
});
