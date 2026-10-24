'use strict';
const {test}=require('node:test'),assert=require('node:assert/strict'),path=require('node:path');
const qa=require('./publication-notes-stdio.cjs'),{zip}=require('../../ui-tests/zip-fixtures.cjs');

test('P06 plan fixes physical versus source-section semantics without starting or writing',()=>{
  const plan=qa.planFor(qa.argumentsFor(['--plan','--pdf'],{}));
  assert.equal(plan.noProcessesStarted,true);assert.equal(plan.noFilesWritten,true);assert.equal(plan.transport,'real-stdio');
  assert.equal(plan.pdf.enabled,true);assert.equal(plan.pdf.physicalPagesMinimum,3);assert.equal(plan.pdf.automaticPagination,true);
  assert.equal(plan.pdf.defaultPhysicalFixture.expectedError,'publication_note_layout');assert.equal(plan.pdf.defaultPhysicalFixture.successfulLayoutCertified,false);
  assert.equal(plan.pdf.successfulPhysicalFixture.typographyKeepLines,true);assert.equal(plan.pdf.successfulPhysicalFixture.sourceUnchanged,true);
  assert.equal(plan.epub.chapterRestartSubstitutionAllowed,false);assert.equal(plan.fullP06Certified,false);assert.equal(plan.guiStarted,false);
  assert(qa.BODY.startsWith('\uFEFF'));assert(!qa.BODY.replaceAll('\r\n','').includes('\n'));
  assert.equal((qa.BODY.match(/readmd-section:v1/g)||[]).length,2);
  const physical=qa.physicalPageSource();assert.equal((physical.match(/^\[\^[pq]\d{3}\]:/gm)||[]).length,144);assert(!physical.includes('readmd-section'));assert(!physical.includes('pagebreak'));
  const long=qa.longNoteSource();assert.equal((long.match(/    LONGNOTE\d{3}/g)||[]).length,240);assert(!long.includes('pagebreak'));
});

test('execution requires explicit source binding and existing validation runtimes',()=>{
  assert.throws(()=>qa.argumentsFor([],{}),/candidate/);
  assert.throws(()=>qa.argumentsFor(['--candidate','candidate'],{}),/epubcheck/);
  assert.throws(()=>qa.argumentsFor(['--candidate','candidate','--epubcheck','check.jar','--pdf'],{}),/python/);
  assert.throws(()=>qa.argumentsFor(['--candidate','candidate','--epubcheck','check.jar','--pdf','--python','python.exe'],{}),/profile/);
  assert.throws(()=>qa.argumentsFor(['--plan','--output',path.join(qa.ROOT,'outside')],{}),/escaped/);
  assert.throws(()=>qa.argumentsFor(['--plan','--unknown'],{}),/Unknown/);
  assert.throws(()=>qa.argumentsFor(['--plan','--candidate'],{}),/Missing/);
  assert.equal(qa.argumentsFor(['--help'],{}).help,true);
});

test('actual export warnings retain both known contracts and reject missing or inconsistent data',()=>{
  const code='publication_note_continuation_unsupported',warnings=['publication_layout_paged_unsupported',code];
  const actual={warnings,warn_items:warnings.map(code=>({code,params:{},text:code}))};assert.equal(qa.exportWarnings(actual),warnings);
  assert.equal(qa.exportWarnings({warns:warnings}),warnings);assert.equal(qa.exportWarnings({warnings,warns:[...warnings]}),warnings);
  assert.deepEqual(qa.exportWarnings({warnings:[]}),[]);
  assert.throws(()=>qa.exportWarnings({}),/omitted both/);assert.throws(()=>qa.exportWarnings({warnings:null}),/Invalid actual/);
  assert.throws(()=>qa.exportWarnings({warnings:['valid',{}]}),/Invalid actual/);
  assert.throws(()=>qa.exportWarnings({warnings:[],warns:warnings}),/fields disagree/);
  assert.throws(()=>qa.exportWarnings({warnings,warn_items:[{params:{}}]}),/typed warning/);
  // The real candidate19 response remains a genuine missing-note diagnostic,
  // even after correcting the field name; the QA never invents that warning.
  const retainedShape={warnings:['publication_layout_paged_unsupported']};
  assert(!qa.exportWarnings(retainedShape).includes(code));
});

test('full-source reads use exact one-based inclusive lines within the actual MCP budget',()=>{
  for(const [source,lines]of [['',1],['\uFEFF',1],['one',1],['one\n',1],['one\ntwo',2],['\uFEFFone\r\n\r\n',2],['one\r\ntwo\r\n',2],['one\n'.repeat(1000),1000]]){
    const args=qa.sourceReadArguments('owned.md',source);assert.deepEqual(args,{file_path:'owned.md',start_line:1,end_line:lines});
    assert(args.start_line>0&&args.end_line-args.start_line<1000);
  }
  assert.throws(()=>qa.sourceReadArguments('owned.md','one\n'.repeat(1001)),/1000-line/);
  assert.throws(()=>qa.sourceReadArguments('owned.md',undefined),/Known source/);
  for(const source of [qa.BODY,qa.physicalPageSource(),qa.longNoteSource()]){
    const args=qa.sourceReadArguments('owned.md',source),text=source.replace(/^\uFEFF/,'');
    assert.equal(args.end_line,(text.match(/\n/g)||[]).length+Number(!text.endsWith('\n')));
  }
});

function docx({restart='eachPage',collision=false,refs=['1','1','2','3','4','1','5']}={}){
  const special='<w:footnote w:type="separator" w:id="-1"><w:p/></w:footnote><w:footnote w:type="continuationSeparator" w:id="0"><w:p><w:continuationSeparator/></w:p></w:footnote>';
  const definitions=Array.from({length:5},(_,index)=>'<w:footnote w:id="'+(index+1)+'"><w:p>'+qa.RICH_MARKERS.map(marker=>'<w:r><w:t>'+marker+'</w:t></w:r>').join('')+'<w:numPr><w:ilvl w:val="0"/></w:numPr></w:p><w:tbl><w:tr/></w:tbl><m:oMath/></w:footnote>').join('');
  return zip([
    ['word/document.xml','<w:document><w:body>'+refs.map(id=>'<w:footnoteReference w:id="'+id+'"/>').join('')+'</w:body></w:document>'],
    ['word/settings.xml','<w:settings><w:footnotePr><w:numStart w:val="27"/><w:numRestart w:val="'+restart+'"/><w:footnote w:id="6"/></w:footnotePr></w:settings>'],
    ['word/footnotes.xml','<w:footnotes>'+special+definitions+'<w:footnote w:type="continuationNotice" w:id="'+(collision?'1':'6')+'"><w:p><w:r><w:t>Continued &lt;next&gt; &amp; evidence</w:t></w:r></w:p></w:footnote></w:footnotes>'],
    ['word/_rels/footnotes.xml.rels','<Relationships><Relationship Id="r1" Target="https://example.test/note"/></Relationships>']
  ]);
}
test('independent OPC inspector rejects wrong restart, changed identities and special-ID collisions',()=>{
  const good=qa.inspectDocx(docx(),{position:'page',restart:'page'});assert.deepEqual(good.normalIds,['1','2','3','4','5']);assert.equal(good.wordApplicationCertified,false);
  assert.throws(()=>qa.inspectDocx(docx({restart:'eachSect'}),{position:'page',restart:'page'}),/Wrong native restart/);
  assert.throws(()=>qa.inspectDocx(docx({collision:true}),{position:'page',restart:'page'}),/collided/);
  assert.throws(()=>qa.inspectDocx(docx({refs:['27','27','28']}),{position:'page',restart:'page'}),/identities/);
});

function epub({destination='notes.xhtml#note-a',duplicate=false,mark='AA'}={}){
  const markers=qa.RICH_MARKERS.join(' ');
  return zip([
    ['mimetype','application/epub+zip'],
    ['EPUB/chapter_1.xhtml','<html><body><a epub:type="noteref" href="'+destination+'" id="ref-a"><sup>'+mark+'</sup></a>'+markers+(duplicate?'<span id="ref-a"></span>':'')+'</body></html>'],
    ['EPUB/chapter_2.xhtml','<html><body><aside id="note-a"><a epub:type="backlink" href="chapter_1.xhtml#ref-a">AA.</a>'+markers+'</aside></body></html>']
  ]);
}
test('EPUB inspector resolves every actual file and fragment and protects display labels separately',()=>{
  const output=epub({destination:'chapter_2.xhtml#note-a'}),good=qa.inspectEpub(output,['AA']);
  assert.equal(good.allTargetsResolve,true);assert.equal(good.internalLinks,2);assert.equal(good.physicalPagesCertified,false);
  assert.throws(()=>qa.inspectEpub(epub(),['AA']),/Unresolved EPUB/);
  assert.throws(()=>qa.inspectEpub(epub({destination:'chapter_2.xhtml#missing'}),['AA']),/Unresolved EPUB/);
  assert.throws(()=>qa.inspectEpub(epub({destination:'chapter_2.xhtml#note-a',duplicate:true}),['AA']),/Duplicate EPUB/);
  assert.throws(()=>qa.inspectEpub(output,['AB']),/scope/);
});

function page(entries){
  const words=entries.flatMap(([marker,label],index)=>{const y=300+index*12;return [[45,y,58,y+6,label],[64,y,130,y+9,marker]];});
  return {width:595,height:842,text:entries.map(([marker])=>marker).join('\n'),words};
}
function physicalPdf(){
  return {kind:'pdf',pages:Array.from({length:3},(_,p)=>{
    const entries=Array.from({length:4},(_,index)=>{
    const row=p*2+Math.floor(index/2),marker='NOTE'+(index%2===0?'P':'Q')+String(row).padStart(3,'0');return [marker,qa.alpha(27+index)];
    }),value=page(entries);value.links=[];
    for(let index=0;index<entries.length;index++){
      const [marker,label]=entries[index],bodyY=70+index*20,noteY=300+index*12;
      value.words.push([45,bodyY,160,bodyY+9,'PAGE'+marker.slice(4)+label]);
      value.links.push({kind:1,page:p,from:[145,bodyY-1,160,bodyY+7],to:[45,noteY-10]},
        {kind:1,page:p,from:[45,noteY-1,58,noteY+6],to:[145,bodyY]});
    }
    return value;
  })};
}
test('physical-page proof rejects continuous numbers on later pages and missing definitions',()=>{
  const value=physicalPdf(),good=qa.inspectPageRestartPdf(value,6);assert.equal(good.actualPhysicalPageRestart,true);assert.equal(good.definitions,12);
  assert.equal(good.entryStartsOnReferencePage,true);assert.equal(good.nativeSourceCallouts,12);assert.equal(good.pairedNativeLinks,12);
  const continuous=structuredClone(value);continuous.pages[1].words[0][4]='EE';assert.throws(()=>qa.inspectPageRestartPdf(continuous,6),/actual PDF page/);
  const missing=structuredClone(value);missing.pages[2].words.splice(0,2);assert.throws(()=>qa.inspectPageRestartPdf(missing,6));
  const fakeChapter=structuredClone(value);fakeChapter.pages=fakeChapter.pages.slice(0,1);assert.throws(()=>qa.inspectPageRestartPdf(fakeChapter,6),/three pages/);
  const noCallout=structuredClone(value);noCallout.pages[0].words=noCallout.pages[0].words.filter(word=>word[4]!=='PAGEP000AA');assert.throws(()=>qa.inspectPageRestartPdf(noCallout,6),/source callouts/);
  const wrongTarget=structuredClone(value);wrongTarget.pages[0].links[0].to[1]+=12;assert.throws(()=>qa.inspectPageRestartPdf(wrongTarget,6),/wrong note or reference/);
  const wrongReturn=structuredClone(value);wrongReturn.pages[0].links[1].to[1]+=20;assert.throws(()=>qa.inspectPageRestartPdf(wrongReturn,6),/wrong note or reference/);
  const otherPage=structuredClone(value);otherPage.pages[0].links[0].page=1;assert.throws(()=>qa.inspectPageRestartPdf(otherPage,6),/wrong note or reference/);
});

test('default physical layout requires the dedicated typed geometry failure',()=>{
  const value={ok:false,error_code:'publication_note_layout',details:{coordinateSpace:'pdfFootnoteStartPage',noteIndex:31,definitionPage:2,referencePage:3}};
  assert.deepEqual(qa.inspectNoteLayoutError(value),value.details);
  const delayed=structuredClone(value);delayed.details.definitionPage=4;assert.deepEqual(qa.inspectNoteLayoutError(delayed),delayed.details);
  for(const changed of [{...value,error_code:'typst_compilation_failed'},{...value,ok:true},{...value,details:JSON.stringify(value.details)}])assert.throws(()=>qa.inspectNoteLayoutError(changed));
  for(const [key,item]of [['noteIndex',0],['noteIndex',145],['noteIndex','31'],['definitionPage',3],['referencePage',100001],['unexpected',true]]){
    const changed=structuredClone(value);changed.details[key]=item;assert.throws(()=>qa.inspectNoteLayoutError(changed));
  }
});

test('existing PDF probes require explicit overwrite and preserve bytes after both rejection stages',async()=>{
  const call={name:'readmd_export_document',arguments:{markdown_content:qa.physicalPageSource(),output_path:'owned.pdf',output_format:'pdf',confirm:true,options:qa.optionsFor('page','page')}};
  const original=Buffer.from('retained real-PDF bytes for the orchestration unit test'),calls=[];
  const geometry={ok:false,error_code:'publication_note_layout',details:{coordinateSpace:'pdfFootnoteStartPage',noteIndex:31,definitionPage:2,referencePage:3}};
  let current=original,reads=0;
  const request=async(name,args,label,code)=>{calls.push({name,args,label,code});return code==='output_exists'?{ok:false,error_code:code}:geometry;};
  const result=await qa.verifyExistingNoteTarget(call,request,()=>{reads++;return current;},original);
  assert.equal(calls.length,2);assert.equal(reads,2);assert.equal(calls[0].code,'output_exists');assert(!Object.hasOwn(calls[0].args,'overwrite'));
  assert.equal(calls[1].code,'publication_note_layout');assert.equal(calls[1].args.overwrite,true);assert.equal(calls[1].args.confirm,true);
  assert.deepEqual(calls[1].args,{...calls[0].args,overwrite:true});assert.equal(result.geometry,geometry.details);
  for(const corruptAt of [1,2]){
    let step=0;current=original;
    await assert.rejects(()=>qa.verifyExistingNoteTarget(call,async(...args)=>{step++;if(step===corruptAt)current=Buffer.from('changed');return request(...args);},()=>current,original),/replaced an existing real PDF/);
    assert.equal(step,corruptAt,'A failed preservation check must stop before any later request');
  }
  await assert.rejects(()=>qa.verifyExistingNoteTarget({...call,arguments:{...call.arguments,overwrite:true}},request,()=>original,original),/must omit overwrite/);
});

test('same-page section proof requires a genuine within-page restart and a unique repeated definition',()=>{
  const value={kind:'pdf',pages:[page([['NOTEA','AA'],['NOTEB','BB'],['NOTEC','CC'],['NOTED','AA'],['NOTEE','BB']])]};
  assert.equal(qa.inspectSectionPdf(value).samePageSectionRestart,true);
  const continuous=structuredClone(value);continuous.pages[0].words[6][4]='DD';assert.throws(()=>qa.inspectSectionPdf(continuous));
  const duplicated=structuredClone(value);duplicated.pages[0].words.push(...page([['NOTEA','AA']]).words);assert.throws(()=>qa.inspectSectionPdf(duplicated));
});

test('three-page long-note proof detects dropped, duplicated or clipped final evidence',()=>{
  const value={kind:'pdf',pages:Array.from({length:3},(_,p)=>({width:595,height:842,words:[[60,400,160,410,'LONGNOTE'+String(p*40).padStart(3,'0')]],text:(p===0?'LONGNOTESTART\n':'')+Array.from({length:40},(_,index)=>'LONGNOTE'+String(p*40+index).padStart(3,'0')).join('\n')+(p===2?'\nLONGNOTEEND':'')}))};
  assert.equal(qa.inspectLongPdf(value,120).allMarkersOnce,true);
  const dropped=structuredClone(value);dropped.pages[2].text=dropped.pages[2].text.replace('LONGNOTE119','');assert.throws(()=>qa.inspectLongPdf(dropped,120),/lost or duplicated/);
  const duplicated=structuredClone(value);duplicated.pages[0].text+='\nLONGNOTE000';assert.throws(()=>qa.inspectLongPdf(duplicated,120),/lost or duplicated/);
  const clipped=structuredClone(value);clipped.pages[2].words[0][3]=900;assert.throws(()=>qa.inspectLongPdf(clipped,120),/clipped/);
});

test('note namespace proof checks real link rectangles, distinct physical destinations and one note body',()=>{
  const words=marker=>[[60,90,180,110,marker]];
  const link=(page,y=90)=>({kind:1,page,from:[60,120,200,140],to:[60,y]});
  const value={kind:'pdf',pages:[
    {words:[...words('ANCHORNOTE'),[60,120,180,140,'LINKRETURN'],[220,120,340,140,'LINKSECTION'],[60,700,180,710,'ANCHORNOTEBODY']],links:[link(1),{...link(2),from:[220,120,340,140]}]},
    {words:[...words('ANCHORRETURN'),[60,120,180,140,'LINKNOTE']],links:[link(0)]},
    {words:words('ANCHORSECTION'),links:[]}
  ]};
  assert.equal(qa.inspectNamespacePdf(value).actualPdfLinks,3);
  const wrong=structuredClone(value);wrong.pages[1].links[0].page=2;assert.throws(()=>qa.inspectNamespacePdf(wrong),/wrong physical/);
  const note=structuredClone(value);note.pages[1].links[0].to[1]=700;assert.throws(()=>qa.inspectNamespacePdf(note),/wrong physical/);
  const duplicate=structuredClone(value);duplicate.pages[2].words.push(...words('ANCHORNOTEBODY'));assert.throws(()=>qa.inspectNamespacePdf(duplicate),/duplicate namespace/);
});
