'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),path=require('node:path'),crypto=require('node:crypto'),fs=require('node:fs');
const qa=require('./publication-numbering-stdio.cjs');
const {alpha}=require('./publication-numbering-oracle.cjs');

test('every QA_FILES and PRODUCT_FILES binding identifies an existing source file',()=>{
  for(const [group,files]of [['QA_FILES',qa.QA_FILES],['PRODUCT_FILES',qa.PRODUCT_FILES]])for(const file of files){
    assert(fs.statSync(path.join(qa.ROOT,file),{throwIfNoEntry:false})?.isFile(),group+' references a missing source file: '+file);
  }
});

function nativeDocx(){
  const headingRules=[{level:0,format:'decimal',start:'1',restart:'0',text:'%1',suffix:'space'},
    {level:1,format:'decimal',start:'1',restart:'1',text:'%1.%2',suffix:'space'}];
  const headings=qa.HEADINGS.map(([,number,title],index)=>({style:index%2?'Heading2':'Heading1',text:title,number:{id:'42',level:String(index%2)},fields:[],bookmarks:[]}));
  const captions=qa.EXPECTED.map((text,index)=>({style:'Caption',text,number:null,fields:[
    {instruction:' STYLEREF 1 \\s ',cached:index<4?'1':'2',kind:'simple'},
    {instruction:' SEQ ReadMD'+(index===2||index===5?'Table':index===3||index===6?'Figure':'Equation')+' \\* ARABIC \\s 1 ',cached:'1',kind:'simple'}],bookmarks:['caption-'+index]}));
  const refs={text:'See',style:'BodyText',number:null,fields:[0,1,2].map(index=>({instruction:' REF bookmark'+index+' \\h ',cached:'Field'}))};
  const nested={style:'ReadMDExampleHeading1',text:'Nested',number:null,fields:[],bookmarks:[]};
  return {kind:'docx-numbering',updateFields:'true',wordApplicationRoundtripCertified:false,abstract:{'2':headingRules},numbers:{'42':{abstract:'2',overrides:[]}},paragraphs:[refs,...headings,...captions,nested]};
}
function offsetDocx(){
  const paragraphs=['Gamma','Delta'].map(text=>({style:'Heading1',text,number:{id:'43',level:'0'},fields:[]}));
  paragraphs.push(...qa.OFFSET_EXPECTED.map((text,index)=>({style:'Caption',text,number:null,fields:[
    {instruction:' STYLEREF 1 \\s ',cached:index<qa.OFFSET_VALUES.length-1?'III':'IV',kind:'simple',depth:0},
    {instruction:' SEQ ReadMDEquation \\s 1 ',cached:String(index<qa.OFFSET_VALUES.length-1?index+1:1),kind:'complex',depth:1},
    {instruction:' =  + 25 \\* ALPHABETIC ',cached:alpha(qa.OFFSET_VALUES[index]),kind:'complex',depth:0}]})));
  return {kind:'docx-numbering',updateFields:'true',wordApplicationRoundtripCertified:false,abstract:{'3':[{level:0,format:'upperRoman',start:'3',restart:'0',text:'%1'}]},numbers:{'43':{abstract:'3',overrides:[]}},paragraphs};
}
function transactionFixture(source=qa.API_SOURCE){
  const hash=text=>crypto.createHash('sha256').update(text).digest('hex'),from=source.indexOf('# Alpha'),insert=qa.COMMENT(qa.CONFIG)+'\r\n\r\n',content=source.slice(0,from)+insert+source.slice(from);
  return {ok:true,config:qa.CONFIG,plan:{config:qa.CONFIG,from,to:from+insert.length},transaction:{base_revision:hash(source),origin:'editor',edits:[{from,to:from,insert}]},result:{content,revision:hash(content),changed:true,inverse:[{from,to:from+insert.length,insert:''}]}};
}

test('numbering plan is read-only and requests native rather than literal heading evidence',()=>{
  const plan=qa.planFor(qa.argumentsFor(['--plan','--exports'],{}));assert.equal(plan.noProcessesStarted,true);assert.equal(plan.noFilesWritten,true);assert.equal(plan.guiStarted,false);assert.equal(plan.transport,'real-stdio');assert.equal(plan.exports.enabled,true);assert.deepEqual(plan.expectedLabels,qa.EXPECTED);assert(plan.nativeWordRequired.some(text=>text.includes('numPr')));assert(plan.limitations.some(text=>text.includes('not certify recovery')));
});

test('execution and output directories remain bound to a candidate and owned scope',()=>{
  assert.throws(()=>qa.argumentsFor([],{}),/candidate/);assert.throws(()=>qa.argumentsFor(['--candidate','c.json','--exports'],{}),/QA-only/);assert.throws(()=>qa.argumentsFor(['--plan','--other'],{}),/Unknown/);assert.throws(()=>qa.argumentsFor(['--plan','--candidate'],{}),/Missing/);assert.throws(()=>qa.argumentsFor(['--plan','--output',qa.ROOT],{}),/escaped/);assert.equal(qa.argumentsFor(['--help'],{}).help,true);assert.equal(qa.inside(qa.BASE,path.join(qa.BASE,'new-owned-run')),path.join(qa.BASE,'new-owned-run'));
});

test('DOCX verification accepts actual native outline and chapter-field contracts',()=>{
  assert.deepEqual(qa.assertDocxInspection(nativeDocx()),{headings:4,captions:7,seqFields:7,externalWordFieldUpdateCertified:false});
});

test('literal heading numbers, missing native rule and wrong ancestor restart are failures',()=>{
  for(const mutate of [value=>{value.paragraphs[1].text='1 Alpha';},value=>{value.paragraphs[1].number=null;},value=>{value.numbers['42'].abstract='missing';},value=>{value.abstract['2'][1].restart='0';},value=>{value.abstract['2'][1].text='%2';},value=>{value.paragraphs.at(-1).style='Heading1';}]){const value=nativeDocx();mutate(value);assert.throws(()=>qa.assertDocxInspection(value));}
});

test('cached caption text never substitutes for STYLEREF, chapter SEQ or REF fields',()=>{
  for(const mutate of [value=>{value.paragraphs[5].fields.shift();},value=>{value.paragraphs[5].fields[1].instruction=' SEQ ReadMDEquation \\* ARABIC ';},value=>{value.paragraphs[5].text='Equation 99.1';},value=>{value.paragraphs[0].fields=[];},value=>{value.updateFields='false';}]){const value=nativeDocx();mutate(value);assert.throws(()=>qa.assertDocxInspection(value));}
});

test('a non-one start requires a nested SEQ field and a native formatted offset formula',()=>{
  assert.deepEqual(qa.assertOffsetDocxInspection(offsetDocx()),{headings:2,captions:29,nestedSeqFields:29,start:26,externalWordFieldUpdateCertified:false});
  const plan=qa.planFor({exports:true});assert.equal(plan.exports.nonOneStartDocxExpected.length,29);for(const label of ['Equation III/Z','Equation III/AA','Equation III/BB','Equation III/ZZ','Equation III/AAA','Equation IV/Z'])assert(plan.exports.nonOneStartDocxExpected.includes(label));assert(plan.nativeWordRequired.some(text=>text.includes('nested SEQ')));
});

test('offset verification rejects literal caches, flat SEQ fields and forced chapter resets',()=>{
  for(const mutate of [value=>{value.paragraphs[2].fields[1].kind='simple';},value=>{value.paragraphs[2].fields[1].depth=0;},value=>{value.paragraphs[2].fields[1].cached='26';},value=>{value.paragraphs[2].fields[1].instruction=' SEQ ReadMDEquation \\s 1 \\r 26 ';},value=>{value.paragraphs[2].fields[2].instruction=' Z ';},value=>{value.paragraphs[2].fields[2].cached='1Z';},value=>{value.paragraphs[2].text='Equation III/1Z';},value=>{value.abstract['3'][0].start='1';},value=>{value.paragraphs[2].fields[0].cached='3';}]){const value=offsetDocx();mutate(value);assert.throws(()=>qa.assertOffsetDocxInspection(value));}
});

test('real offset readback must include formatted field results across both chapters',()=>{
  const markdown='# Gamma\n\n'+qa.OFFSET_EXPECTED.slice(0,-1).join('\n\n')+'\n\n# Delta\n\n'+qa.OFFSET_EXPECTED.at(-1);
  assert.deepEqual(qa.assertOffsetReadback(markdown).visibleLabels,qa.OFFSET_EXPECTED);assert.throws(()=>qa.assertOffsetReadback('# Gamma\n# Delta'));assert.throws(()=>qa.assertOffsetReadback(markdown.replace('III/Z','III/1Z')));assert.throws(()=>qa.assertOffsetReadback(markdown.replace('Equation III/AA\n','Equation III/AB\n')));assert.equal(qa.assertOffsetReadback(markdown).numberingConfigurationRecovered,false);
});

test('API evidence validates a single UTF-16 source edit and its exact common inverse',()=>{
  const fixture=transactionFixture();assert.equal(qa.assertNumberingTransaction(fixture,qa.API_SOURCE).content,fixture.result.content);assert(qa.API_SOURCE.includes('𝛂'));assert(qa.API_SOURCE.startsWith('\ufeff---\r\n'));assert.equal(qa.planFor({exports:false}).numberingApi.length,8);
  for(const mutate of [value=>{value.transaction.base_revision='stale';},value=>{value.transaction.origin='export';},value=>{value.transaction.edits.push({from:qa.API_SOURCE.length,to:qa.API_SOURCE.length,insert:'number'});},value=>{value.result.revision='projected';},value=>{value.result.inverse[0].insert='changed body';},value=>{value.result.content+='number';},value=>{value.config=null;}]){const value=transactionFixture();mutate(value);assert.throws(()=>qa.assertNumberingTransaction(value,qa.API_SOURCE));}
});

test('editable offset readback requires actual prepared labels and exact native starts',()=>{
  const markdown=qa.COMMENT(qa.OFFSET_CONFIG)+'\n\n# Gamma\n\n# Delta\n',projection={ok:true,source_labels:qa.OFFSET_EXPECTED.map(text=>({text})),source_numbering:{config:qa.OFFSET_CONFIG}};
  assert.equal(qa.assertOffsetReadback(markdown,projection).editableDeclarationPresent,true);
  assert.throws(()=>qa.assertOffsetReadback(markdown));assert.throws(()=>qa.assertOffsetReadback(markdown,{...projection,source_labels:projection.source_labels.slice(1)}));
  const wrong=JSON.parse(JSON.stringify(projection));wrong.source_numbering.config.captions.equation.start=1;assert.throws(()=>qa.assertOffsetReadback(markdown,wrong));
});

test('API source presence is checked against the actual declaration rather than a materialized plan',()=>{
  const value=transactionFixture();value.plan.config=JSON.parse(JSON.stringify(qa.CONFIG));value.plan.config.captions.equation.prefix=null;
  assert.equal(qa.assertNumberingTransaction(value,qa.API_SOURCE).content,value.result.content);
  value.config={...qa.CONFIG,headings:null};assert.throws(()=>qa.assertNumberingTransaction(value,qa.API_SOURCE));
});

test('idempotent API evidence requires no edits and keeps the original revision',()=>{
  const source=qa.API_SOURCE,hash=crypto.createHash('sha256').update(source).digest('hex'),value={ok:true,config:null,plan:null,transaction:{base_revision:hash,origin:'editor',edits:[]},result:{content:source,revision:hash,changed:false,inverse:[]}};assert.equal(qa.assertNumberingTransaction(value,source,false).content,source);assert.throws(()=>qa.assertNumberingTransaction(value,source,true));
});

test('numbering candidate binding includes both real UI hosts and their behavioral evidence',()=>{
  for(const file of ['assets/js/editor/publication-numbering.js','assets/js/reader/publication.js','assets/js/features/export.js','rust/readmd-kernel/src/mcp.rs','rust/readmd-kernel/src/server.rs'])assert(qa.PRODUCT_FILES.includes(file));for(const file of ['tools/test/publication-numbering-ui.test.cjs','tools/test/publication-numbering-reader.test.cjs'])assert(qa.QA_FILES.includes(file));
});

test('convert tool successful plain Markdown is distinct from structured publication payloads',()=>{
  assert.equal(qa.callPayload('readmd_convert_to_markdown',{content:[{type:'text',text:'# Alpha\n\nBeta'}]}),'# Alpha\n\nBeta');
  assert.throws(()=>qa.callPayload('readmd_convert_to_markdown',{isError:true,content:[{type:'text',text:'Alpha Beta'}]}));
  assert.throws(()=>qa.callPayload('readmd_convert_to_markdown',{structuredContent:{ok:false},content:[{type:'text',text:'Alpha Beta'}]}));
  assert.throws(()=>qa.callPayload('readmd_convert_to_markdown',{content:[]}));
  assert.equal(qa.callPayload('readmd_academic_document',{structuredContent:{ok:true,active:true}}).active,true);
});

test('EPUB reference inspection resolves the actual named chapter and destination',()=>{
  const allLabels=qa.EXPECTED.join(' '),chapters=[['OEBPS/chapter_1.xhtml','<a href="chapter_3.xhtml#eq-destination">Equation 2.1</a>'+allLabels],['OEBPS/chapter_3.xhtml','<p id="eq-destination">Equation 2.1</p>']];
  assert.deepEqual(qa.assertEpubLinks(chapters),{targetChapter:'chapter_3.xhtml',targetId:'eq-destination'});
  assert.throws(()=>qa.assertEpubLinks([chapters[0]]),/chapter does not exist/);
  assert.throws(()=>qa.assertEpubLinks([chapters[0],[chapters[1][0],'<p id="other">Equation 2.1</p>']]),/target missing/);
});

test('public plan reports source projection and conversion limits without certifying editable recovery',()=>{
  const plan=qa.planFor({exports:false});assert.equal(plan.exports.enabled,false);assert(plan.limitations.some(text=>text.includes('Subfigures')));assert(plan.limitations.some(text=>text.includes('field-update')));assert(qa.SOURCE.includes(':label[eq:hidden]'));assert(qa.SOURCE.includes('> # Nested'));
});

function nativeImages(){
  const value=nativeDocx();value.paragraphs.forEach((row,index)=>row.bodyIndex=2*index+2);
  value.images=[];for(const [index,title]of ['First figure','Second figure'].entries()){
    const label='Figure '+(index+1)+'-1',caption=value.paragraphs.find(row=>row.text===label);
    caption.text=label+': '+title;caption.visibleRuns=[{text:caption.text,properties:{sz:'18',szCs:'24'}}];caption.bookmarkRanges=[{name:caption.bookmarks[0],id:String(index+1),text:label}];
    const image={bodyIndex:caption.bodyIndex-1,descriptionPresent:true,description:index?'Two':'One',sha256:qa.PNG_SHA256,bytes:70,targetMode:'Internal',relationshipType:'http://schemas.openxmlformats.org/officeDocument/2006/relationships/image',member:'word/media/deduplicated.png',png:{allChunkCrcValid:true,dimensions:[1,1],rgba:[0,0,0,255]}};
    value.images.push(image);value.paragraphs.push({bodyIndex:image.bodyIndex,text:'',style:null,fields:[],bookmarks:[],images:[image]});
  }
  const last=value.paragraphs.find(row=>row.text==='Figure 2-1: Second figure');value.paragraphs[0].fields[2]={instruction:' REF '+last.bookmarks[0]+' \\h ',cached:'Figure 2-1'};
  value.bodyVisibleText='CURRENT_COMPLETE_NATIVE_BODY';value.tableCount=2;value.mathTexts=['x=1','y=2','z=3'];return value;
}
const returnedImages='See :ref[fig:b].\n\n![One](pixel.png)\n\n:label[fig:a] First figure\n\n![Two](pixel.png)\n\n:label[fig:b] Second figure\n';
const returnedProjection=()=>({ok:true,diagnostics:[],source_labels:qa.IDS.map((id,index)=>({id,text:qa.EXPECTED[index]}))});
test('new numbered PNG contract is deterministic 70-byte opaque-black RGBA8 and does not reuse candidate41 bytes',()=>{assert.equal(Buffer.from(qa.PNG,'base64').length,70);assert.equal(crypto.createHash('sha256').update(Buffer.from(qa.PNG,'base64')).digest('hex'),qa.PNG_SHA256);assert.equal(qa.PNG_SHA256,'dc5687b50f70cb95379bfced5a0eae768dd4382cd6b393ee77d65bbdd6373fbf');assert.equal(qa.planFor({exports:true}).imageReturn.oldCandidate41DamagedPngRetainedAsNegative,true);});
test('actual image oracle binds each drawing to PNG, current alt, adjacent figure fields and independent axes',()=>{const value=nativeImages(),identity=qa.assertNumberedImages(value);assert.deepEqual(identity.bindings.map(row=>row.id),['fig:a','fig:b']);assert.equal(value.images[0].member,value.images[1].member,'Legal media deduplication is allowed');qa.assertNumberedImages(JSON.parse(JSON.stringify(value)),identity);});
for(const [name,change]of Object.entries({
  wrongDescription:value=>value.images[0].description='image',missingDescription:value=>value.images[0].descriptionPresent=false,wrongMedia:value=>value.images[0].sha256='0'.repeat(64),wrongPixels:value=>value.images[0].png.rgba=[255,255,255,255],badCrc:value=>value.images[0].png.allChunkCrcValid=false,external:value=>value.images[0].targetMode='External',foreignRelationship:value=>value.images[0].relationshipType='urn:foreign/image',missingDrawing:value=>value.images.pop(),duplicateVisualAlt:value=>value.paragraphs.push({style:'Caption',text:'One',fields:[],bookmarks:[]}),interveningBody:value=>value.images[0].bodyIndex-=1,wrongChapter:value=>value.paragraphs.find(row=>row.text==='Figure 1-1: First figure').fields[0].cached='2',wrongRefTarget:value=>value.paragraphs[0].fields[2].instruction=' REF other \\h ',westernFromBody:value=>value.paragraphs.find(row=>row.text==='Figure 1-1: First figure').visibleRuns[0].properties.sz='24',missingCs:value=>delete value.paragraphs.find(row=>row.text==='Figure 1-1: First figure').visibleRuns[0].properties.szCs,complexAxisAliased:value=>value.paragraphs.find(row=>row.text==='Figure 1-1: First figure').visibleRuns[0].properties.szCs='18',
}))test('numbered image return rejects '+name,()=>{const value=nativeImages();change(value);assert.throws(()=>qa.assertNumberedImages(value));});
test('second native return compares complete visible body, tables, formula text and figure bookmark identity',()=>{const value=nativeImages(),identity=qa.assertNumberedImages(value);for(const change of [v=>v.bodyVisibleText+='LOST_OR_ADDED',v=>v.mathTexts.pop(),v=>v.tableCount=1]){const next=JSON.parse(JSON.stringify(value));change(next);assert.throws(()=>qa.assertNumberedImages(next,identity));}});
test('image source evidence is actual top-level Markdown Image followed by its current label and REF',()=>{assert.equal(qa.assertNumberedImageSource(returnedImages,returnedProjection()).length,2);assert.equal(qa.assertNumberedImageSource(returnedImages.replace('First figure','First&#32;figure'),returnedProjection())[0].title,'First figure');});
for(const [name,change]of Object.entries({list:s=>s.split('\n').map(line=>'- '+line).join('\n'),quote:s=>s.split('\n').map(line=>'> '+line).join('\n'),code:s=>'```md\n'+s+'```\n',comment:s=>'<!--\n'+s+'-->\n',defaultAlt:s=>s.replace('![One]','![image]'),wrongLabel:s=>s.replace(':label[fig:a]',':label[tbl:a]'),hiddenRef:s=>s.replace(':ref[fig:b]','`:ref[fig:b]`'),separatedObject:s=>s.replace(':label[fig:a]','Intervening author paragraph.\n\n:label[fig:a]')}))test('source image oracle rejects '+name,()=>{assert.throws(()=>qa.assertNumberedImageSource(change(returnedImages),returnedProjection()));});
test('source image bytes can be read only from the explicit owned documents or owned staging profile',()=>{const docs=path.join(qa.BASE,'pure-docs'),profile=path.join(qa.BASE,'pure-profile');assert.equal(qa.imageAssetPath('pixel.png',docs,profile),path.join(docs,'pixel.png'));assert.equal(qa.imageAssetPath('/raw?p='+encodeURIComponent(path.join(profile,'resource-staging/pixel.png')),docs,profile),path.join(profile,'resource-staging/pixel.png'));for(const href of ['../private.png','https://outside.example/image.png','/raw?p='+encodeURIComponent(path.join(qa.ROOT,'private.png')),'/raw?p='+encodeURIComponent(path.join(profile,'pixel.png'))+'&p=other'])assert.throws(()=>qa.imageAssetPath(href,docs,profile));});
function normalSession(pid){return {passed:true,session:{pid,forcedStop:false,pendingAtClose:0,hostExit:{code:0,signal:null}},transportFatal:null,logDescriptorsClosed:true};}
test('both actual host EOF sessions are retained and legacy session points at the final PID',()=>{const report={passed:true,sessions:[]};qa.recordNumberingSession(report,'first',normalSession(101),true);qa.recordNumberingSession(report,'reopened',normalSession(202),true);assert.deepEqual(report.sessions.map(row=>row.session.pid),[101,202]);assert.equal(report.session.pid,202);assert(report.sessions.every(row=>row.normalEofRequested));});
for(const [name,change]of Object.entries({forced:row=>row.session.forcedStop=true,pending:row=>row.session.pendingAtClose=1,exit:row=>row.session.hostExit.code=1,signal:row=>row.session.hostExit.signal='SIGTERM',transport:row=>row.transportFatal={message:'failed'},logs:row=>row.logDescriptorsClosed=false}))test('numbering lifecycle rejects '+name,()=>{const report={passed:true,sessions:[]},closed=normalSession(202);change(closed);assert.throws(()=>qa.recordNumberingSession(report,'first',closed,true));});
test('reuse of a PID or absence of requested EOF cannot count as a real reopen',()=>{const report={passed:true,sessions:[]};qa.recordNumberingSession(report,'first',normalSession(101),true);assert.throws(()=>qa.recordNumberingSession(report,'reopened',normalSession(101),true));assert.throws(()=>qa.recordNumberingSession({sessions:[]},'first',normalSession(202),false));});

function materializedSaveFixture(){
  const base=path.join(qa.ROOT,'.cache/capability-upgrade/batch3-author-inline-20261021/consumers/numbering42-image-save-stage-01/pure-controls');fs.mkdirSync(base,{recursive:true});
  const temp=fs.mkdtempSync(path.join(base,'numbered-save-')),documents=path.join(temp,'documents'),profile=path.join(temp,'profile');fs.mkdirSync(documents);fs.mkdirSync(profile);fs.mkdirSync(path.join(profile,'resource-staging'));
  const staged=path.join(profile,'resource-staging',qa.PNG_SHA256+'.png');fs.writeFileSync(staged,Buffer.from(qa.PNG,'base64'));
  const href='/raw?p='+encodeURIComponent(staged),logical='\ufeff🚀 source\r\n\r\n'+returnedImages.replaceAll('pixel.png',href).replaceAll('\n','\r\n'),before='OWNED_NUMBERED_SAVE_SENTINEL\n';
  const reference='readmd-assets/image-'+qa.PNG_SHA256.slice(0,24)+'.png',permanent=path.join(documents,reference);fs.mkdirSync(path.dirname(permanent));fs.writeFileSync(permanent,Buffer.from(qa.PNG,'base64'));
  const edits=[...logical.matchAll(/!\[(?:One|Two)\]\(([^)]+)\)/g)].map(match=>({from:match.index+match[0].indexOf('](')+2,to:match.index+match[0].length-1,insert:reference}));
  let saved=logical;for(const edit of [...edits].reverse())saved=saved.slice(0,edit.from)+edit.insert+saved.slice(edit.to);
  const value={ok:true,dry_run:false,changed:true,replacements:1,revision:crypto.createHash('sha256').update(saved).digest('hex'),proposed_revision:crypto.createHash('sha256').update(saved).digest('hex'),checkpoint_id:'owned-history-checkpoint',saved_content:saved,asset_edits:edits,saved_assets:[{path:permanent,name:path.basename(permanent),reference}]};
  return {temp,base,documents,profile,staged,permanent,logical,before,value,cleanup(){assert.equal(qa.inside(base,temp),temp);fs.rmSync(temp,{recursive:true,force:true});}};
}
test('the actual candidate42 legacy save envelope cannot count as permanent image materialization',()=>{
  const fixture=materializedSaveFixture();try{
    // Minimal source-independent excerpt of the real candidate42 save envelope:
    // it committed the logical revision, with no saved_content/asset_edits/assets.
    const old={ok:true,dry_run:false,changed:true,replacements:1,revision:crypto.createHash('sha256').update(fixture.logical).digest('hex'),proposed_revision:crypto.createHash('sha256').update(fixture.logical).digest('hex'),encoding:'utf-8',checkpoint_id:'f201cc76b3e47183d6868e7b11fabf4d0a1def2e0baaf8d46c784dc27ae7807b'};
    assert.throws(()=>qa.assertMaterializedNumberedSave(old,fixture.before,fixture.logical,returnedProjection(),fixture.documents,fixture.profile),/actual final Markdown/);
  }finally{fixture.cleanup();}
});
test('saved staged images require exact UTF16 destination edits, current permanent bytes and one deduplicated resource',()=>{
  const fixture=materializedSaveFixture();try{
    const result=qa.assertMaterializedNumberedSave(fixture.value,fixture.before,fixture.logical,returnedProjection(),fixture.documents,fixture.profile);
    assert.equal(result.source,fixture.value.saved_content);assert.deepEqual(result.staged,[fixture.staged]);assert.deepEqual(result.permanent,[fixture.permanent]);
    const receipt=qa.removeOwnedStagedNumberedPng(result.staged,fixture.profile);assert.equal(receipt.removedBeforeNewProcess,true);assert(!fs.existsSync(fixture.staged));assert.equal(crypto.createHash('sha256').update(fs.readFileSync(fixture.permanent)).digest('hex'),qa.PNG_SHA256);
  }finally{fixture.cleanup();}
});
for(const [name,change]of Object.entries({
  arbitrarySavedBody:v=>{v.saved_content+='UNAUTHORIZED_BODY';v.revision=v.proposed_revision=crypto.createHash('sha256').update(v.saved_content).digest('hex');},
  wrongCurrentRevision:v=>v.revision='0'.repeat(64),missingCheckpoint:v=>delete v.checkpoint_id,
  extraBodyEdit:v=>v.asset_edits.push({from:0,to:0,insert:'HIDDEN'}),
  wrongDestinationRange:v=>v.asset_edits[0].from--,
  wrongAssetReference:v=>v.saved_assets[0].reference='readmd-assets/other.png',
  wrongAssetPath:v=>v.saved_assets[0].path=path.join(qa.ROOT,'unrelated.png'),
  duplicateAssets:v=>v.saved_assets.push({...v.saved_assets[0]}),
  unsavedStagingSource:(v,f)=>{v.saved_content=f.logical;v.asset_edits=[];v.revision=v.proposed_revision=crypto.createHash('sha256').update(f.logical).digest('hex');},
}))test('permanent save proof rejects '+name,()=>{const f=materializedSaveFixture();try{const value=JSON.parse(JSON.stringify(f.value));change(value,f);assert.throws(()=>qa.assertMaterializedNumberedSave(value,f.before,f.logical,returnedProjection(),f.documents,f.profile));}finally{f.cleanup();}});
test('permanent save proof checks the actual PNG bytes and never removes an unrelated staged file',()=>{
  const f=materializedSaveFixture();try{
    fs.writeFileSync(f.permanent,'WRONG_PNG');assert.throws(()=>qa.assertMaterializedNumberedSave(f.value,f.before,f.logical,returnedProjection(),f.documents,f.profile));fs.writeFileSync(f.permanent,Buffer.from(qa.PNG,'base64'));
    const unrelated=path.join(f.profile,'resource-staging/unrelated.png');fs.writeFileSync(unrelated,'MUST_REMAIN');assert.throws(()=>qa.removeOwnedStagedNumberedPng([f.staged],f.profile));assert(fs.existsSync(f.staged));assert.equal(fs.readFileSync(unrelated,'utf8'),'MUST_REMAIN');
    assert.throws(()=>qa.removeOwnedStagedNumberedPng([f.permanent],f.profile));assert(fs.existsSync(f.permanent));
  }finally{f.cleanup();}
});
test('numbered return explicitly binds managed-image saving and removal of temporary dependence',()=>{
  for(const name of ['rust/readmd-kernel/src/document_assets.rs','rust/readmd-kernel/src/document_intelligence.rs','rust/readmd-kernel/src/convert.rs','rust/readmd-kernel/src/convert/docx_notes.rs'])assert(qa.PRODUCT_FILES.includes(name));
  const plan=qa.planFor({exports:true}).imageReturn;assert.equal(plan.actualSavedContentAndUtf16AssetEdits,true);assert.equal(plan.permanentRelativeImages,true);assert.equal(plan.removeOwnedStagedPngBeforeReopen,true);assert.deepEqual(plan.secondOptions,{});
});

function entitySaveFixture(){
  const base=path.join(qa.ROOT,'.cache/capability-upgrade/batch3-author-inline-20261021/consumers/staged-image-selector-02/pure-controls');fs.mkdirSync(base,{recursive:true});
  const temp=fs.mkdtempSync(path.join(base,'entity-save-')),documents=path.join(temp,'documents'),profile=path.join(temp,'profile');fs.mkdirSync(documents);fs.mkdirSync(profile);fs.mkdirSync(path.join(profile,'resource-staging'));
  const staged=path.join(profile,'resource-staging',qa.PNG_SHA256+'.png');fs.writeFileSync(staged,Buffer.from(qa.PNG,'base64'));
  const fixture=qa.entityStagedSaveSource('/raw?p='+encodeURIComponent(staged)),before='OWNED_ENTITY_SAVE_SENTINEL\r\n',reference='readmd-assets/image-'+qa.PNG_SHA256.slice(0,24)+'.png',permanent=path.join(documents,reference);fs.mkdirSync(path.dirname(permanent));fs.writeFileSync(permanent,Buffer.from(qa.PNG,'base64'));
  const image='![Entity save]('+reference+' "Entity title")',saved=fixture.prefix+image+fixture.suffix;
  const value={ok:true,dry_run:false,changed:true,replacements:1,revision:crypto.createHash('sha256').update(saved).digest('hex'),proposed_revision:crypto.createHash('sha256').update(saved).digest('hex'),checkpoint_id:'owned-entity-checkpoint',saved_content:saved,asset_edits:[{from:fixture.from,to:fixture.to,insert:image}],saved_assets:[{path:permanent,name:path.basename(permanent),reference}]};
  return {fixture,before,value,documents,profile,staged,permanent,cleanup(){assert.equal(qa.inside(base,temp),temp);fs.rmSync(temp,{recursive:true,force:true});}};
}
test('entity save fixture has one real image after closed HTML and no literal raw-URI shortcut',()=>{
  const f=entitySaveFixture();try{assert(f.fixture.encoded.includes('&#47;raw&#63;p='));assert(!f.fixture.source.includes('/raw?'));assert(f.fixture.source.startsWith('<div>OWNED_CLOSED_HTML</div>\r\n'));assert.equal(f.fixture.source.slice(f.fixture.from,f.fixture.to),f.fixture.image);assert.equal(f.fixture.from,f.fixture.prefix.length);assert.notEqual(f.fixture.from,Buffer.byteLength(f.fixture.prefix,'utf8'));assert(qa.planFor({exports:true}).imageReturn.entityEncodedSave.independentSubdirectory);}finally{f.cleanup();}
});
test('entity save requires exact UTF16 image-only replay, actual permanent PNG and untouched inert contexts',()=>{
  const f=entitySaveFixture();try{const result=qa.assertEntityStagedSave(f.value,f.before,f.fixture,f.documents,f.profile);assert.equal(result.source,f.value.saved_content);assert.equal(result.permanent,f.permanent);assert.equal(result.staged,f.staged);assert(result.source.endsWith(f.fixture.suffix));}finally{f.cleanup();}
});
test('old unmaterialized entity envelope cannot pass the independent real-save oracle',()=>{
  const f=entitySaveFixture();try{const value={...f.value};delete value.saved_content;delete value.asset_edits;delete value.saved_assets;assert.throws(()=>qa.assertEntityStagedSave(value,f.before,f.fixture,f.documents,f.profile),/did not materialize/);}finally{f.cleanup();}
});
for(const [name,change]of Object.entries({
  oldLogicalSource:(v,f)=>{v.saved_content=f.fixture.source;v.revision=v.proposed_revision=crypto.createHash('sha256').update(v.saved_content).digest('hex');},
  changedOrdinaryLink:v=>{v.saved_content=v.saved_content.replace('[ordinary](','[changed](');v.revision=v.proposed_revision=crypto.createHash('sha256').update(v.saved_content).digest('hex');},
  changedCode:v=>{v.saved_content=v.saved_content.replace('![code]','![lost-code]');v.revision=v.proposed_revision=crypto.createHash('sha256').update(v.saved_content).digest('hex');},
  changedHtml:v=>{v.saved_content=v.saved_content.replace('![html-example]','![lost-html]');v.revision=v.proposed_revision=crypto.createHash('sha256').update(v.saved_content).digest('hex');},
  changedPrefix:v=>{v.saved_content=v.saved_content.replace('OWNED_CLOSED_HTML','DIFFERENT_HTML');v.revision=v.proposed_revision=crypto.createHash('sha256').update(v.saved_content).digest('hex');},
  wrongUtf16Range:v=>v.asset_edits[0].from++,
  unrelatedEdit:v=>v.asset_edits.push({from:0,to:0,insert:'UNAUTHORIZED'}),
  wrongResource:v=>v.saved_assets[0].reference='readmd-assets/other.png',
  wrongCurrentRevision:v=>v.revision='0'.repeat(64),
}))test('entity saved-content proof rejects '+name,()=>{const f=entitySaveFixture();try{const value=JSON.parse(JSON.stringify(f.value));change(value,f);assert.throws(()=>qa.assertEntityStagedSave(value,f.before,f.fixture,f.documents,f.profile));}finally{f.cleanup();}});
test('entity save validates both actual permanent and still-current staged bytes',()=>{const f=entitySaveFixture();try{fs.writeFileSync(f.permanent,'changed');assert.throws(()=>qa.assertEntityStagedSave(f.value,f.before,f.fixture,f.documents,f.profile));fs.writeFileSync(f.permanent,Buffer.from(qa.PNG,'base64'));fs.writeFileSync(f.staged,'changed');assert.throws(()=>qa.assertEntityStagedSave(f.value,f.before,f.fixture,f.documents,f.profile));}finally{f.cleanup();}});
test('entity dry-run must flag the actual rewrite and preserve revision and empty materialization fields',()=>{
  const before='OWNED\n',logical='![current](&#47;raw&#63;p=owned)',value={ok:true,dry_run:true,resource_rewrite_required:true,revision:crypto.createHash('sha256').update(before).digest('hex'),proposed_revision:crypto.createHash('sha256').update(logical).digest('hex'),checkpoint_id:null};
  qa.assertEntitySavePreview(value,before,logical);for(const change of [v=>delete v.resource_rewrite_required,v=>v.checkpoint_id='unexpected',v=>v.revision='0'.repeat(64),v=>v.saved_content=logical,v=>v.saved_assets=[]]){const copy={...value};change(copy);assert.throws(()=>qa.assertEntitySavePreview(copy,before,logical));}
});
test('image dry-run follows the advertised optional confirmation schema',()=>{
  const {assertToolCallSchema}=require('./batch3-semantics-stdio.cjs');
  const catalog=[{name:'readmd_edit_document',inputSchema:{type:'object',properties:{file_path:{type:'string'},expected_revision:{type:'string'},old_text:{type:'string'},new_text:{type:'string'},dry_run:{type:'boolean'},confirm:{type:'boolean',const:true}},required:['file_path','expected_revision','old_text','new_text'],additionalProperties:false}}];
  const args=qa.imageSavePreviewArguments('Owned.md','a'.repeat(64),'before','after');
  assert.equal(args.dry_run,true);assert(!Object.hasOwn(args,'confirm'));
  assertToolCallSchema({name:'readmd_edit_document',arguments:args},catalog);
  assert.throws(()=>assertToolCallSchema({name:'readmd_edit_document',arguments:{...args,confirm:false}},catalog),/inputSchema/);
});
test('entity material matches its existing target EOL without inserting an encoding BOM into logical text',()=>{
  const f=entitySaveFixture();try{
    assert(f.before.endsWith('\r\n'));assert(!f.fixture.source.startsWith('\ufeff'));
    assert(!f.fixture.source.replaceAll('\r\n','').includes('\n'));
    assert.notEqual(crypto.createHash('sha256').update(f.fixture.source).digest('hex'),crypto.createHash('sha256').update(f.fixture.source.replaceAll('\r\n','\n')).digest('hex'));
  }finally{f.cleanup();}
});
