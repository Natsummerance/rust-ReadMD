'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),{spawnSync}=require('node:child_process'),path=require('node:path');
const qa=require('./batch3-citation-return-stdio.cjs');
test('plan explicitly distinguishes real stdio from headless OPC mutation and Word interoperability',()=>{
  const plan=qa.planFor();assert.equal(plan.noProcessesStarted,true);assert.equal(plan.noFilesWritten,true);assert.equal(plan.applicationRoundtripCertified,false);assert.equal(plan.headlessMutationsAreWordInterop,false);assert.equal(plan.requiredTools.length,6);assert(plan.productFiles.includes('rust/readmd-kernel/src/convert/docx_notes.rs'));assert(plan.cases.some(value=>value.includes('outside note controls')));
  assert.equal(plan.contextSignatures.version,2);assert.equal(plan.contextSignatures.sourceOwnedCitationImageBindingCertified,false);assert.equal(plan.contextSignatures.mcpImageScope,'authored-manual-note-resource-preservation');assert(plan.cases.some(value=>value.includes('Target-only')));assert(plan.cases.some(value=>value.includes('v1 XML-only')));
});
test('plan executable does not require a candidate or Python and prints only the plan',()=>{
  const result=spawnSync(process.execPath,[path.join(__dirname,'batch3-citation-return-stdio.cjs'),'--plan'],{windowsHide:true,encoding:'utf8',timeout:10000});assert.equal(result.status,0,result.stderr);assert.equal(result.stderr,'');assert.deepEqual(JSON.parse(result.stdout),qa.planFor());
});
test('execution requires bound candidate and existing QA-only Python arguments',()=>{
  assert.throws(()=>qa.argumentsFor([],{}),/candidate/);assert.throws(()=>qa.argumentsFor(['--candidate','candidate.json'],{}),/python/);assert.equal(qa.argumentsFor(['--plan'],{}).plan,true);assert.throws(()=>qa.argumentsFor(['--unexpected'],{}),/Unknown/);assert.throws(()=>qa.argumentsFor(['--candidate'],{}),/Missing/);
});
test('citation slots keep distinct occurrences even when their raw source is identical',()=>{
  const sources=[{span:{from:8,to:13,source:'[@42]'}},{span:{from:19,to:24,source:'[@42]'}}];assert.deepEqual(qa.slots({projection:{citations:sources.map(source=>({source}))}}),sources);assert.throws(()=>qa.slots({}));
});
test('conversion output never treats an error containing source text as successful Markdown',()=>{
  const text='USER_CITATION\n<!--readmd:citation-import:v1\n{}\n-->';assert.equal(qa.textResult({content:[{type:'text',text}]}),text);assert.throws(()=>qa.textResult({isError:true,content:[{type:'text',text}]}));
});
test('evidence arguments remain within the owned scope',()=>{
  assert.equal(qa.inside(qa.BASE,path.join(qa.BASE,'new-run')),path.join(qa.BASE,'new-run'));assert.throws(()=>qa.inside(qa.BASE,path.join(qa.BASE,'..','other-run')));assert.throws(()=>qa.inside(qa.BASE,qa.BASE));
});
test('complex fixture keeps the automatic bibliography between two source chapters and includes every shared feature',()=>{
  const source=qa.integrationSource('---\nbibliography: refs.json\ncsl: house.csl\n---\n');assert(source.includes('readmd-numbering:v1'));assert(source.indexOf('INTEGRATED_ALPHA')<source.indexOf('<!--readmd:bibliography-->'));assert(source.indexOf('<!--readmd:bibliography-->')<source.indexOf('# INTEGRATED_BETA'));assert.equal((source.match(/^## /gm)||[]).length,2);assert(source.includes('[^integrated]:')&&source.includes('[-@same, chap. 3]'));assert(source.includes(':label[tbl:integration-alpha]')&&source.includes(':ref[eq:integration-beta]'));assert.deepEqual(qa.planFor().integration.formats,['docx','epub','pdf']);assert.equal(qa.planFor().integration.fullNumberingRecoveryCertified,false);
});
function integrationProjection(){return {diagnostics:[],projection:{citations:[{},{},{}].map(source=>({source})),bibliography:{items:[{},{}]}},source_numbering:{headings:qa.INTEGRATION_HEADINGS.map(([id,text])=>({id,numbering:{text}}))},source_labels:qa.INTEGRATION_LABELS.map(([id,text])=>({id,text}))};}
test('combined projection rejects a generated-heading shift, missing citation, note citation or wrong chapter caption',()=>{
  assert.equal(qa.assertIntegrationProjection(integrationProjection()).citations,3);
  for(const mutate of [value=>{value.source_numbering.headings[2].numbering.text='3';},value=>{value.projection.citations.pop();},value=>{value.projection.bibliography.items.pop();},value=>{value.source_labels[2].text='Equation 3.1';}]){const value=integrationProjection();mutate(value);assert.throws(()=>qa.assertIntegrationProjection(value));}
});
function docxIntegration(){
  const headings=qa.INTEGRATION_HEADINGS.map(([,number,text,level])=>({text,style:'Heading'+level,number:{id:'7',level:String(level-1)},fields:[]}));
  const captions=qa.INTEGRATION_LABELS.map(([,text],index)=>({text,style:'Caption',number:null,fields:[{instruction:' STYLEREF 1 \\s '},{instruction:' SEQ ReadMD'+(index===1?'Table':'Equation')+' \\s 1 '}]}));
  const value={kind:'docx-numbering',updateFields:'true',numbers:{7:{abstract:'8'}},abstract:{8:[{level:0,text:'%1',start:'1',restart:'0',format:'decimal'},{level:1,text:'%1.%2',start:'1',restart:'1',format:'decimal'}]},paragraphs:[...headings,...captions,{text:'See',style:'BodyText',number:null,fields:[{instruction:' REF target1 \\h '},{instruction:' REF target2 \\h '}]}]};
  const noteRecord='<rc:record id="note-citation" kind="citation" part="word/footnotes.xml"><rc:source>[-@same, chap. 3]</rc:source><rc:note id="1" end="false"/><rc:item key="same" mode="suppress-author"><rc:locator>3</rc:locator><rc:kind>chapter</rc:kind></rc:item></rc:record>';
  const parts=new Map([['word/document.xml','<m:oMath><m:oMath><w:tbl>INTEGRATED_CELL<w:footnoteReference w:id="1"/><w:footnoteReference w:id="1"/>readmd:bibliography:v1:id INTEGRATED_BETA'],['word/footnotes.xml','INTEGRATED_NOTE <w:sdt><w:sdtPr><w:tag w:val="readmd:citation:v1:note-citation"/></w:sdtPr><w:sdtContent><w:r><w:t>(2</w:t></w:r><w:r><w:t>:</w:t></w:r><w:r><w:t>3)</w:t></w:r></w:sdtContent></w:sdt>'],['customXml/readmd-citations.xml',noteRecord+'kind="citation" kind="citation" kind="bibliography"']]);return {value,parts};
}
test('actual DOCX checker requires native source heading rules, caption and REF fields, note references and local citation controls together',()=>{
  const {value,parts}=docxIntegration();assert.equal(qa.assertIntegrationDocx(value,parts).editableCitations,3);
  for(const mutate of [({value})=>{value.paragraphs[2].text='3 INTEGRATED_BETA';},({value})=>{value.abstract[8][1].text='%2';},({value})=>{value.paragraphs[4].fields.pop();},({value})=>{value.paragraphs.at(-1).fields.pop();},({parts})=>parts.set('word/footnotes.xml','missing'),({parts})=>parts.set('customXml/readmd-citations.xml','kind="citation"'),({parts})=>parts.set('word/footnotes.xml',parts.get('word/footnotes.xml').replace('3)</w:t>','99)</w:t>')),({parts})=>parts.set('customXml/readmd-citations.xml',parts.get('customXml/readmd-citations.xml').replace('mode="suppress-author"','mode="normal"')),({parts})=>parts.set('word/document.xml',parts.get('word/document.xml').replace('w:id="1"','w:id="2"'))]){const fixture=docxIntegration();mutate(fixture);assert.throws(()=>qa.assertIntegrationDocx(fixture.value,fixture.parts));}
});
function epubIntegration(){
  const headings=qa.INTEGRATION_HEADINGS.map(([,number,title,level])=>'<h'+level+'>'+number+' '+title+'</h'+level+'>').join('');
  const labels=qa.INTEGRATION_LABELS.map(([,text])=>'<p>'+text+'</p>').join('');
  return new Map([['mimetype','application/epub+zip'],['OEBPS/chapter_1.xhtml',headings+labels+'<p>'+qa.INTEGRATION_MARKERS.join(' ')+'</p><section role="doc-bibliography"></section><aside id="note1"></aside><p id="equation1"></p><a href="#note1" epub:type="noteref">1</a><a href="#note1" epub:type="noteref">1</a><a href="#equation1">Equation 2.1</a><a href="#equation1">Table 1.1</a><math></math><math></math>']]);
}
test('EPUB checker requires correct post-bibliography headings and resolves every note and caption link',()=>{
  assert.equal(qa.assertIntegrationEpub(epubIntegration()).liveInternalLinks,4);
  for(const [before,after]of [['2 INTEGRATED_BETA','3 INTEGRATED_BETA'],['id="note1"','id="wrong"'],['<math></math>',''],['role="doc-bibliography"','role="none"']]){const parts=epubIntegration();parts.set('OEBPS/chapter_1.xhtml',parts.get('OEBPS/chapter_1.xhtml').replace(before,after));assert.throws(()=>qa.assertIntegrationEpub(parts));}
});
function pdfIntegration(){return {kind:'pdf',reader:{name:'independent reader fixture'},pages:[{text:qa.INTEGRATION_MARKERS.join(' ')+' '+qa.INTEGRATION_HEADINGS.map(([,number,title])=>number+' '+title).join(' ')+' '+qa.INTEGRATION_LABELS.map(([,text])=>text).join(' '),links:[{page:0},{page:0}],drawings:1,images:0}]};}
test('PDF checker rejects shifted numbering, missing note/bibliography text, lost links and image fallback',()=>{
  assert.equal(qa.assertIntegrationPdf(pdfIntegration()).vectorMath,true);
  for(const mutate of [value=>{value.pages[0].text=value.pages[0].text.replace('2 INTEGRATED_BETA','3 INTEGRATED_BETA');},value=>{value.pages[0].text=value.pages[0].text.replace('INTEGRATED_NOTE','');},value=>{value.pages[0].text=value.pages[0].text.replace('TITLE_A','');},value=>{value.pages[0].links=[];},value=>{value.pages[0].drawings=0;},value=>{value.pages[0].images=1;}]){const value=pdfIntegration();mutate(value);assert.throws(()=>qa.assertIntegrationPdf(value));}
});
function literalReader(){
  const encoded=qa.LITERAL.replace(/[&<>()[\]@*:$`!\\]/g,ch=>ch==='&'?'&amp;':ch==='<'?'&lt;':ch==='>'?'&gt;':'&#'+ch.codePointAt(0)+';');
  return {content:'**'+encoded+'** [*REAL_LINK &#91;literal&#93;*](<https://example.test/real?a=1&amp;b=2>) `` `CODE` [@same] ``',projection:{citations:[{source:{}},{source:{}}]},warnings:['docx_citation_conflict'],diagnostics:[]};
}
test('real reader parser checker requires visible literal syntax, no fake link activation, single entity decoding and retained genuine rich content',()=>{
  const view=literalReader();assert.equal(qa.assertLiteralReader(view,2).existingEntityTextDecodedOnce,true);
  for(const mutate of [value=>{value.content=qa.LITERAL+' [REAL_LINK](https://example.test/real?a=1&b=2) `` `CODE` [@same] ``';},value=>{value.content=value.content.replaceAll('&','&amp;');},value=>{value.content=value.content.replace('**','');},value=>{value.content+=' [FAKE](https://fake.test)';},value=>{value.projection.citations.push({source:{}});},value=>{value.content=value.content.replace('https://example.test/real','https://wrong.test');}]){const value=literalReader();mutate(value);assert.throws(()=>qa.assertLiteralReader(value,2));}
});
test('HTML evidence decoder decodes exactly once so pre-existing entity text is never mistaken for decoded user content',()=>{
  assert.equal(qa.decodeHtmlText('&amp;lt; &#91; &#x40; &amp;amp;'),'&lt; [ @ &amp;');assert(qa.QA_FILES.includes('assets/vendor/marked.min.js'));
});
function transactionFixture(){
  const hash=text=>require('node:crypto').createHash('sha256').update(text).digest('hex'),source='😀 First [@42, p. 12]. Second [@42, p. 99].\r\n',from=source.indexOf('99'),content=source.slice(0,from)+'111'+source.slice(from+2),transaction={base_revision:hash(source),origin:'mcp',edits:[{from,to:from+2,insert:'111'}]},reply={structuredContent:{ok:true,result:{content,revision:hash(content),changed:true,inverse:[{from,to:from+3,insert:'99'}]}}};return {source,transaction,reply};
}
test('actual transaction envelope supplies nested Markdown, its exact UTF-16 change, matching revision and full inverse',()=>{
  const {source,transaction,reply}=transactionFixture();assert.equal(reply.structuredContent.content,undefined);const result=qa.transactionResult(reply,source,transaction);assert.equal(result.content,reply.structuredContent.result.content);assert(result.content.includes('First [@42, p. 12]. Second [@42, p. 111]'));
  assert.deepEqual(qa.transactionResult({content:[{type:'text',text:JSON.stringify(reply.structuredContent)}]},source,transaction),result);
  assert.throws(()=>qa.academicRequest(reply.structuredContent.content,'owned'));assert.equal(qa.academicRequest(result.content,'owned').arguments.content,result.content);
});
test('transaction validation rejects flattened or failed envelopes, wrong changed/revision/content and an inexact inverse',()=>{
  for(const mutate of [value=>{value.reply.structuredContent={ok:true,...value.reply.structuredContent.result};},value=>{value.reply.structuredContent.ok=false;},value=>{value.reply.isError=true;},value=>{value.reply.structuredContent.result.changed=false;},value=>{value.reply.structuredContent.result.revision='0'.repeat(64);},value=>{value.reply.structuredContent.result.content+='unrequested';},value=>{value.reply.structuredContent.result.inverse[0].insert='wrong';},value=>{delete value.reply.structuredContent.result.inverse;},value=>{value.transaction.base_revision='0'.repeat(64);}]){const value=transactionFixture();mutate(value);assert.throws(()=>qa.transactionResult(value.reply,value.source,value.transaction));}
  assert.throws(()=>qa.applyUtf16Edits('😀',[{from:1,to:1,insert:'split'}]),/surrogate/);assert.throws(()=>qa.applyUtf16Edits('A\r\nB',[{from:2,to:2,insert:'split'}]),/CRLF/);
});
test('all academic request paths obey the actual MCP catalogue contract without unsupported HTTP-only view flags',()=>{
  const {assertToolCallSchema}=require('./batch3-semantics-stdio.cjs'),catalog=[{name:'readmd_academic_document',inputSchema:{type:'object',properties:{content:{type:'string'},baseDir:{type:'string'},options:{type:'object'}},required:['content','baseDir'],additionalProperties:false}}];
  for(const content of ['returned Markdown',qa.LITERAL,transactionFixture().reply.structuredContent.result.content])assertToolCallSchema(qa.academicRequest(content,'owned'),catalog);
  assert.throws(()=>assertToolCallSchema({name:'readmd_academic_document',arguments:{content:'text',baseDir:'owned',view:true}},catalog),/inputSchema/);assert.throws(()=>qa.academicRequest(undefined,'owned'));assert.throws(()=>qa.academicRequest('text',undefined));
});

test('PNG fixtures encode actual pixel changes with valid chunk CRCs and a readable zlib scanline',()=>{
  const first=qa.pngPixel([220,30,40,255]),second=qa.pngPixel([20,80,230,255]);assert.notDeepEqual(first,second);assert.deepEqual(qa.checkedPixel(first),[220,30,40,255]);assert.deepEqual(qa.checkedPixel(second),[20,80,230,255]);const broken=Buffer.from(second);broken[broken.length-1]^=1;assert.throws(()=>qa.checkedPixel(broken));assert.throws(()=>qa.checkedPixel(second.subarray(0,second.length-1)));assert.throws(()=>qa.pngPixel([1,2,3]));assert.throws(()=>qa.pngPixel([1,2,3,999]));
});
function relationshipReader(){return {content:'[evidence](<https://new.example/>) RELATIONSHIP_MANUAL_IMAGE ![image](/raw?p=owned-pixel)\n\n<!--readmd:citation-import:v1\n{"original_source":"[see [evidence](https://old.example/) @42]"}\n-->\n',projection:{citations:[]},warnings:['docx_citation_conflict'],diagnostics:[{code:'docx_citation_conflict',reason:'citation_render_modified',original_source:'[see [evidence](https://old.example/) @42]'}]};}
test('relationship checker inspects actual reader destinations while excluding preserved old source annotations',()=>{
  const view=relationshipReader();assert(view.content.includes('https://old.example/'));assert.equal(qa.assertRelationshipReader(view,{target:'https://new.example/',citations:0,conflict:true}).editableCitations,0);assert.deepEqual(qa.readerStructure(view).links,['https://new.example/']);
  for(const mutate of [value=>{value.content=value.content.replace('https://new.example/','https://wrong.example/');},value=>{value.content+=' [STALE](https://old.example/)';},value=>{value.projection.citations.push({source:{}});},value=>{value.warnings=[];},value=>{value.diagnostics=[];},value=>{value.content=value.content.replace('RELATIONSHIP_MANUAL_IMAGE','');},value=>{value.content=value.content.replace('![image](/raw?p=owned-pixel)','');}]){const value=relationshipReader();mutate(value);assert.throws(()=>qa.assertRelationshipReader(value,{target:'https://new.example/',citations:0,conflict:true}));}
});
test('legacy reader checker requires explicit unsupported-signature provenance and prevents regenerated bibliography',()=>{
  const view=relationshipReader();view.content=view.content.replace('https://new.example/','https://old.example/');view.diagnostics[0].reason='citation_render_signature_unsupported';view.manual_bibliography=true;view.projection.bibliography=null;assert.equal(qa.assertRelationshipReader(view,{citations:0,conflict:true,legacy:true}).legacyContextDowngraded,true);
  for(const mutate of [value=>{value.diagnostics[0].reason='citation_source_package_invalid';},value=>{value.manual_bibliography=false;},value=>{value.projection.bibliography={items:[]};}]){const value=structuredClone(view);mutate(value);assert.throws(()=>qa.assertRelationshipReader(value,{citations:0,conflict:true,legacy:true}));}
});
test('staged media checker reads only owned current resource bytes and rejects the old pixel or a host-path reference',()=>{
  const fs=require('node:fs'),crypto=require('node:crypto');fs.mkdirSync(qa.BASE,{recursive:true});const evidence=fs.mkdtempSync(path.join(qa.BASE,'png-selftest-')),profile=path.join(evidence,'profile');fs.mkdirSync(profile);const bytes=qa.pngPixel([20,80,230,255]),asset=path.join(profile,'current.png');fs.writeFileSync(asset,bytes,{flag:'wx'});const digest=crypto.createHash('sha256').update(bytes).digest('hex'),view={content:'![image](/raw?p='+encodeURIComponent(asset)+')'};assert.equal(qa.assertStagedPixel(view,evidence,digest,[20,80,230,255]).sha256,digest);assert.throws(()=>qa.assertStagedPixel(view,evidence,'0'.repeat(64),[20,80,230,255]));assert.throws(()=>qa.assertStagedPixel(view,evidence,digest,[220,30,40,255]));const outside={content:'![image](/raw?p='+encodeURIComponent(path.join(evidence,'host.png'))+')'};assert.throws(()=>qa.assertStagedPixel(outside,evidence,digest,[20,80,230,255]),/owned scope/);
});

function foreignTextFixture(){
  const original='[see [evidence](https://old.example/) @42]',binding='readmd:citation:v1:foreign-guard',mutation={manifestUnchanged:true,signatureMetadataUnchanged:true,renamedTextOnlyInsideControl:true,manifestSha256Before:'a'.repeat(64),manifestSha256After:'a'.repeat(64),foreignNamespace:'urn:readmd:qa:foreign-citation-text',foreignTextStory:'word/document.xml',foreignTextBinding:binding,originalSource:original,foreignNodeText:'Alpha',remainingWordText:'see evidence (1:)'};
  const view={content:'see [evidence](https://old.example/) (1:)\n\nRELATIONSHIP_MANUAL_IMAGE ![image](/raw?p=owned-pixel)\n\nDOCX_RETURN_EDITED_BODY_中文\n\n<!--readmd:citation-import:v1\n{"original_source":"'+original+'","original_author":"Alpha"}\n-->\n',projection:{citations:[]},warnings:['docx_citation_conflict'],diagnostics:[{code:'docx_citation_conflict',reason:'citation_render_modified',part:'word/document.xml',binding,original_source:original,current_source:original}]};return {view,mutation};
}
test('foreign namespace text checker requires changed-render provenance and preserves only actual current Word text',()=>{
  const {view,mutation}=foreignTextFixture();assert(view.content.includes('Alpha'));assert.equal(qa.assertForeignTextReader(view,mutation).foreignTextNotReintroduced,true);assert(!qa.readerStructure(view).text.includes('Alpha'));
  for(const mutate of [value=>{value.view.projection.citations.push({source:{}});},value=>{value.view.content+=' Alpha';},value=>{value.view.content=value.view.content.replace('evidence]','LOST]');},value=>{value.view.content=value.view.content.replace('DOCX_RETURN_EDITED_BODY_中文','');},value=>{value.view.warnings=[];},value=>{value.view.diagnostics=[];},value=>{value.view.diagnostics[0].reason='generated_citation_note_modified_or_unbound';},value=>{value.view.diagnostics[0].original_source='[@42]';},value=>{value.view.diagnostics[0].current_source='[@same]';},value=>{value.view.diagnostics[0].part='word/footnotes.xml';},value=>{value.view.diagnostics[0].binding='different';},value=>{value.mutation.manifestSha256After='b'.repeat(64);},value=>{value.mutation.signatureMetadataUnchanged=false;},value=>{value.mutation.renamedTextOnlyInsideControl=false;}]){const value=foreignTextFixture();mutate(value);assert.throws(()=>qa.assertForeignTextReader(value.view,value.mutation));}
});
