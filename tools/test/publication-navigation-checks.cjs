'use strict';
const assert=require('node:assert/strict');
function checkNavigation(summary,expected,{pages=true,landmarks=expected.landmarks,chapterCount=expected.chapters,sourceOf=expected.sourceOfPagination}={}){
  assert.equal(summary.parseErrors.length,0,'Actual packaged XML is malformed');
  assert.equal(summary.chapterCount,chapterCount);
  assert.deepEqual(summary.pages,pages?expected.pages:[],'Actual page-list links/order/labels differ');
  assert.deepEqual(summary.landmarks,landmarks,'Actual landmark roles/links/labels differ');
  assert.equal(summary.tocCount,1,'Exactly one required EPUB toc nav must remain');
  assert.equal(summary.pageListCount,pages?1:0);assert.equal(summary.landmarksCount,landmarks.length?1:0);
  assert.equal(summary.sourceOfPagination,pages?sourceOf:null,'Final OPF source-of pagination metadata differs');
  assert.equal(summary.sourceOfRelations,pages&&sourceOf?1:0);
  assert.equal(summary.title,'Synthetic reference pages');assert.deepEqual(summary.creators,['Example, Ada']);
  assert.equal(summary.rights,'Original synthetic QA material');
  assert.equal(summary.customMetadata.title,'Synthetic reference pages');assert.equal(summary.customMetadata.authors[0].name,'Example, Ada');
  for(const page of expected.pages){
    const [file,id]=page.href.split('#'),actual=summary.anchors.filter(anchor=>anchor.file===file&&anchor.id===id);
    assert.equal(actual.length,1,'Reference page target missing/duplicated: '+page.href);
    assert.equal(actual[0].owner,'source');assert.equal(actual[0].type,'pagebreak');assert.equal(actual[0].role,'doc-pagebreak');
    assert.equal(actual[0].label,page.label);assert.equal(actual[0].text,'','Reference page marker polluted body copy');
  }
  assert.equal(summary.anchors.filter(anchor=>anchor.owner==='source').length,expected.pages.length);
  assert.equal(summary.anchors.filter(anchor=>anchor.owner==='bodymatter').length,1);
  for(const token of expected.bodyTokens)assert.equal(summary.bodyText.split(token).length-1,1,'Original body token missing/duplicated: '+token);
  assert.equal(summary.sourceDirectiveLeaked,false,'Source-only navigation metadata escaped into visible/exported content');
  for(const link of summary.internalLinks){
    const [file,fragment]=link.href.split('#'),target=file||link.file;
    assert(summary.files.includes(target),'Internal destination file missing: '+link.href);
    if(fragment)assert.equal(summary.ids.filter(id=>id.file===target&&id.id===fragment).length,1,'Internal destination missing/duplicated: '+link.href);
  }
  return {actualPages:summary.pages.length,actualLandmarks:summary.landmarks.length,chapterCount,allHrefTargetsUnique:true,bodyTokensPreserved:true,p01Preserved:true,sourceOfPagination:summary.sourceOfPagination};
}
function selfTest(){
  const expected={chapters:1,pages:[{href:'chapter_1.xhtml#h-readmd-epub-first',label:'1'}],landmarks:[{href:'nav.xhtml#toc',label:'Contents',type:'toc'}],sourceOfPagination:'urn:synthetic:13',bodyTokens:['BODY']};
  const good={parseErrors:[],chapterCount:1,pages:expected.pages,landmarks:expected.landmarks,tocCount:1,pageListCount:1,landmarksCount:1,sourceOfPagination:expected.sourceOfPagination,sourceOfRelations:1,
    title:'Synthetic reference pages',creators:['Example, Ada'],rights:'Original synthetic QA material',customMetadata:{title:'Synthetic reference pages',authors:[{name:'Example, Ada'}]},
    anchors:[{file:'chapter_1.xhtml',id:'h-readmd-epub-first',owner:'source',type:'pagebreak',role:'doc-pagebreak',label:'1',text:''},{file:'chapter_1.xhtml',id:'body',owner:'bodymatter'}],
    bodyText:'BODY',sourceDirectiveLeaked:false,internalLinks:[{file:'nav.xhtml',href:'chapter_1.xhtml#h-readmd-epub-first'}],files:['chapter_1.xhtml','nav.xhtml'],ids:[{file:'chapter_1.xhtml',id:'h-readmd-epub-first'}]};
  checkNavigation(good,expected);
  const controls=[
    value=>value.pages.pop(),value=>value.pages.push({...value.pages[0]}),value=>value.pages[0].href='chapter_2.xhtml#lost',value=>value.pages[0].label='2',
    value=>value.landmarks[0].type='index',value=>value.anchors[0].owner='ordinary',value=>value.anchors[0].text='pollution',value=>value.anchors.push({...value.anchors[0]}),
    value=>value.bodyText='',value=>value.bodyText='BODY BODY',value=>value.ids=[],value=>value.sourceOfPagination='wrong',value=>value.sourceOfRelations=2,value=>value.creators=['Other'],
    value=>value.customMetadata.authors=[],value=>value.sourceDirectiveLeaked=true,value=>value.parseErrors=['bad xml']
  ];
  for(const corrupt of controls){const value=structuredClone(good);corrupt(value);assert.throws(()=>checkNavigation(value,expected));}
  return {scope:'Independent checker controls only',passed:true,accepted:1,rejected:controls.length,productInvocations:0};
}
module.exports={checkNavigation,selfTest};
if(require.main===module)console.log(JSON.stringify(selfTest(),null,2));
