'use strict';
// Source-owned P05/P08 numbering over a candidate's actual, owned stdio host.
// --plan reads only; every actual run gets a new immutable evidence directory.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {spawnSync}=require('node:child_process');
const {StdioClient,assertToolCallSchema,exportRequestFor,systemEnvironment,verifyBinding,errorPayload}=require('./batch3-semantics-stdio.cjs');
const {payload}=require('./managed-tex-process-cold.cjs');
const {alpha}=require('./publication-numbering-oracle.cjs');
const zipParts=require('../../ui-tests/zip-parts.cjs');
const marked=require('../../assets/vendor/marked.min.js');
const {markdownStructure,sourceReadArguments,assertSavedEdit}=require('./mcp-docx-settings-roundtrip.cjs');
const ROOT=path.resolve(__dirname,'../..');
const BASE=path.join(ROOT,'.cache/capability-upgrade/batch3-20261020/numbering');
const hash=value=>crypto.createHash('sha256').update(value).digest('hex');
const fileHash=file=>hash(fs.readFileSync(file));
const QA_FILES=['tools/test/publication-numbering-stdio.cjs','tools/test/publication-numbering-stdio.test.cjs','tools/test/publication-numbering-oracle.cjs','tools/test/publication-numbering-oracle.test.cjs','tools/test/publication-numbering-ui.test.cjs','tools/test/publication-numbering-reader.test.cjs','tools/test/inspect-publication-numbering.py','tools/test/batch3-semantics-stdio.cjs','tools/test/managed-tex-process-cold.cjs','tools/test/inspect-mcp-publication-closure.py','ui-tests/zip-parts.cjs','ui-tests/zip-fixtures.cjs','tools/test/inspect-publication-numbering.test.py','tools/test/inspect-docx-settings-roundtrip.py','tools/test/mcp-docx-settings-roundtrip.cjs','assets/vendor/marked.min.js','assets/vendor/html5-character-references.json'];
const PRODUCT_FILES=['rust/readmd-kernel/src/document_assets.rs','rust/readmd-kernel/src/document_intelligence.rs','rust/readmd-kernel/src/convert.rs','rust/readmd-kernel/src/convert/docx_notes.rs','rust/readmd-kernel/src/cross_references.rs','rust/readmd-kernel/src/cross_references_numbering_tests.rs','rust/readmd-kernel/src/publication_layout.rs','rust/readmd-kernel/src/md_ast.rs','rust/readmd-kernel/src/publication.rs','rust/readmd-kernel/src/docx_writer.rs','rust/readmd-kernel/src/epub_writer.rs','rust/readmd-kernel/src/typst_writer.rs','rust/readmd-kernel/src/latex_writer.rs','rust/readmd-kernel/src/mdexport.rs','rust/readmd-kernel/src/upgrade_api.rs','rust/readmd-kernel/src/mcp.rs','rust/readmd-kernel/src/server.rs','rust/xtask/src/bundle.rs','assets/js/editor/publication-numbering.js','assets/js/reader/publication.js','assets/js/features/export.js','assets/css/panels.css'];
const CONFIG={headings:{levels:[{format:'decimal',start:1,restartAfter:0},{format:'decimal',start:1,restartAfter:1}],separator:'.',suffix:'space'},captions:{equation:{scope:'chapter',separator:'.'},figure:{scope:'chapter',separator:'-',captionSeparator:': '},table:{scope:'chapter',separator:'.'}}};
const COMMENT=value=>'<!-- readmd-numbering:v1 '+JSON.stringify(value).replaceAll('<','\\u003c').replaceAll('>','\\u003e').replaceAll('&','\\u0026')+' -->';
const ALPHA='# Alpha {#alpha}\n\n## Method {#method-alpha}\n\n$$x=1$$\n\n:label[eq:a] Initial\n\n$$y=2$$\n\n:label[eq:a2] Second\n\n| Key | Value |\n|---|---|\n| A | 1 |\n\n:label[tbl:a] First table\n\n![One](pixel.png)\n\n:label[fig:a] First figure\n\n';
const BETA='# Beta {#beta}\n\n## Method {#method-beta}\n\n$$z=3$$\n\n:label[eq:b] Final\n\n| Key | Value |\n|---|---|\n| B | 2 |\n\n:label[tbl:b] Second table\n\n![Two](pixel.png)\n\n:label[fig:b] Second figure\n\n';
const FRONT='---\ntitle: BATCH3NUMBERING20261020\nlang: en-US\n---\n';
const EXAMPLES='\nExample `:ref[eq:b]`.\n\n```md\n# Hidden\n:label[eq:hidden]\n:ref[eq:b]\n```\n\n> # Nested\n';
const SOURCE=FRONT+COMMENT(CONFIG)+'\n\nSee :ref[eq:b], :ref[tbl:b], :ref[fig:b].\n\n'+ALPHA+BETA+EXAMPLES;
const EXPECTED=['Equation 1.1','Equation 1.2','Table 1.1','Figure 1-1','Equation 2.1','Table 2.1','Figure 2-1'];
const IDS=['eq:a','eq:a2','tbl:a','fig:a','eq:b','tbl:b','fig:b'];
const HEADINGS=[['alpha','1','Alpha'],['method-alpha','1.1','Method'],['beta','2','Beta'],['method-beta','2.1','Method']];
const API_SOURCE='\ufeff'+(FRONT+'\n'+ALPHA+BETA+EXAMPLES+'\nUnicode 𝛂 stays authored.\n').replaceAll('\n','\r\n');
const OFFSET_CONFIG={headings:{levels:[{format:'upperRoman',start:3,restartAfter:0}],separator:'.',suffix:'space'},captions:{equation:{scope:'chapter',format:'upperLetter',start:26,separator:'/'}}};
const OFFSET_VALUES=[...Array.from({length:28},(_,index)=>26+index),26];
const OFFSET_SOURCE=FRONT+COMMENT(OFFSET_CONFIG)+'\n\n# Gamma {#gamma}\n\n'+OFFSET_VALUES.slice(0,-1).map(number=>'$$x='+number+'$$\n\n:label[eq:offset-'+number+'] Offset '+number+'\n\n').join('')+'# Delta {#delta}\n\n$$z=1$$\n\n:label[eq:offset-reset] Restart offset\n';
const OFFSET_EXPECTED=OFFSET_VALUES.map((number,index)=>'Equation '+(index<OFFSET_VALUES.length-1?'III':'IV')+'/'+alpha(number));
const PNG='iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNgYGD4DwABBAEAgLvRWwAAAABJRU5ErkJggg==';
const PNG_SHA256='dc5687b50f70cb95379bfced5a0eae768dd4382cd6b393ee77d65bbdd6373fbf';

function inside(base,file){const relative=path.relative(base,path.resolve(file));assert(relative&&!relative.startsWith('..'+path.sep)&&relative!=='..'&&!path.isAbsolute(relative),'Path escaped owned numbering scope: '+file);return path.resolve(file);}
PRODUCT_FILES.push('rust/readmd-kernel/src/convert/docx_numbering_import.rs','rust/readmd-kernel/src/convert/docx_numbering_import_tests.rs');
function argumentsFor(argv,env=process.env){
  const result={candidate:env.READMD_MCP_CANDIDATE,binary:env.READMD_BIN,assets:env.READMD_ASSETS_DIR,python:env.READMD_TEST_PYTHON,plan:false,exports:false};
  for(let index=0;index<argv.length;index++){
    const arg=argv[index];if(arg==='--help')return {help:true};if(arg==='--plan'){result.plan=true;continue;}if(arg==='--exports'){result.exports=true;continue;}
    assert(['--candidate','--binary','--assets','--python','--output'].includes(arg),'Unknown argument: '+arg);assert(index+1<argv.length,'Missing argument: '+arg);result[arg.slice(2)]=argv[++index];
  }
  for(const key of ['candidate','binary','assets','python','output'])if(result[key])result[key]=path.resolve(result[key]);
  if(!result.plan){assert(result.candidate,'Required --candidate or READMD_MCP_CANDIDATE');if(result.exports)assert(result.python,'Exports require an existing QA-only --python runtime');}
  if(result.output)inside(BASE,result.output);return result;
}
function planFor(options){return {schemaVersion:1,scope:'P05/P08 source-owned multilevel heading and chapter caption numbering',synthetic:true,transport:'real-stdio',noProcessesStarted:true,noFilesWritten:true,guiStarted:false,managedEnginesCopied:false,ownedProfileAndDocuments:true,sourceSha256:hash(SOURCE),expectedLabels:EXPECTED,headingIds:HEADINGS.map(row=>row[0]),numberingApi:['get absent configuration','apply one source transaction','get applied configuration','idempotent apply','stale revision rejection','common transaction inverse','null clears configuration','clear inverse'],imageReturn:{fixture:'real CRC-verified 1x1 RGBA8 opaque black PNG',sha256:PNG_SHA256,oldCandidate41DamagedPngRetainedAsNegative:true,description:['One','Two'],captionHalfPoints:[18,24],explicitRevisionSave:true,actualSavedContentAndUtf16AssetEdits:true,entityEncodedSave:{realMcpDryRunAndCommit:true,encodedSlashAndQuestion:true,closedHtmlPrefix:true,inertHtmlCodeLinkPreserved:true,independentSubdirectory:true},permanentRelativeImages:true,removeOwnedStagedPngBeforeReopen:true,normalEof:true,differentPid:true,secondOptions:{},actualReimport:true},nativeWordRequired:['Heading numPr resolves to real abstractNum levels','lvlText/start/numFmt/lvlRestart match the source contract','Native STYLEREF chapter field plus SEQ chapter restart','REF bookmark fields remain updateable','Non-one start uses an actual nested SEQ formula with independent visible-result readback'],exports:{enabled:options.exports,formats:['docx','epub','pdf'],nonOneStartDocxExpected:OFFSET_EXPECTED,independentZipXmlAndPdfReadback:true},limitations:['No Word application, GUI, external reader or field-update execution is certified.','The numbered image fixture certifies its bounded source IDs, fields and PNG through save/new-PID/options:{} return; it does not certify recovery of editable numbering configuration for arbitrary documents.','Subfigures, continued captions, all caption placement options, complete P05/P08 and full third-batch acceptance remain outside this scope.']};}
function bindingFor(options){
  const candidate=JSON.parse(fs.readFileSync(options.candidate,'utf8')),binary=options.binary||candidate.binary,assets=options.assets||path.dirname(candidate.boot),boot=path.join(assets,'readmd.boot.js');
  const sourceFreeze=inside(ROOT,path.resolve(ROOT,candidate.sourceFreeze));assert.equal(fileHash(sourceFreeze),candidate.sourceFreezeSha256);const freeze=JSON.parse(fs.readFileSync(sourceFreeze,'utf8')),boundFiles={};
  for(const name of [...QA_FILES,...PRODUCT_FILES]){const row=freeze.sources.find(row=>row.path===name);assert(row,'Candidate lacks frozen numbering QA/product source '+name);boundFiles[name]={file:inside(ROOT,path.join(ROOT,name)),copy:inside(ROOT,path.join(ROOT,row.copy)),sha256:row.sha256};}
  const result={candidate:options.candidate,candidateSha256:fileHash(options.candidate),binary,assets,boot,binarySha256:candidate.binarySha256,bootSha256:candidate.bootSha256,sourceFreeze,sourceFreezeSha256:candidate.sourceFreezeSha256,boundFiles};verifyBinding(result);return result;
}
function assertProjection(value,expected=EXPECTED,ids=IDS){
  assert.equal(value.ok,true);assert.deepEqual(value.diagnostics,[]);assert.deepEqual(value.source_labels.map(label=>label.text),expected);assert.deepEqual(value.source_labels.map(label=>label.id),ids);
  assert(value.source_numbering,'The shared numbering plan is absent');assert.deepEqual(value.source_numbering.headings.map(heading=>[heading.id,heading.numbering?.text]),HEADINGS.map(row=>row.slice(0,2)));
  for(const label of value.source_labels){assert(label.numbering.chapter,'Caption silently lost its chapter');assert(label.numbering.ordinal>=1);assert.equal(label.number,label.numbering.start+label.numbering.ordinal-1);}
  assert(value.content.includes('`'+':ref[eq:b]'+'`'));assert(value.content.includes('```md\n# Hidden\n:label[eq:hidden]\n:ref[eq:b]\n```'));return value;
}
function assertDocxInspection(value){
  assert.equal(value.kind,'docx-numbering');assert(['true','1'].includes(value.updateFields));assert.equal(value.wordApplicationRoundtripCertified,false);
  const headings=value.paragraphs.filter(p=>/^Heading[12]$/.test(p.style)&&p.number);assert.equal(headings.length,4);
  for(let index=0;index<headings.length;index++){
    const heading=headings[index];assert.equal(heading.text,HEADINGS[index][2],'Heading uses literal number text');assert(heading.number,'Heading lacks native numPr');
    const instance=value.numbers[heading.number.id];assert(instance,'Unresolved native heading numId');const levels=value.abstract[instance.abstract];assert(levels,'Unresolved native abstractNum');
    const rule=levels.find(level=>level.level===Number(heading.number.level));assert(rule,'No native rule for heading level');assert.equal(rule.format,'decimal');assert.equal(rule.start,'1');assert.equal(rule.text,index%2===0?'%1':'%1.%2');assert.equal(rule.restart,index%2===0?'0':'1');
  }
  const nested=value.paragraphs.find(p=>p.text==='Nested');assert(nested,'Nested heading example was lost');assert.equal(nested.number,null,'Nested heading consumed the native outline');assert(!/^Heading[1-6]$/.test(nested.style),'Nested sample can intercept native chapter fields');
  const captions=value.paragraphs.filter(p=>p.fields.some(field=>/\bSEQ\s+ReadMD(?:Equation|Figure|Table)\b/.test(field.instruction)));assert.equal(captions.length,7);
  for(let index=0;index<captions.length;index++){
    const caption=captions[index];assert(caption.text.startsWith(EXPECTED[index]),'Wrong actual caption field cache: '+caption.text);
    assert(caption.fields.some(field=>/\bSTYLEREF\s+(?:"?Heading\s*1"?|1)\b[^]*\\s\b/i.test(field.instruction)),'Chapter is literal text instead of STYLEREF');
    assert(caption.fields.some(field=>/\bSEQ\s+ReadMD(?:Equation|Figure|Table)\b[^]*\\s\s+1\b/.test(field.instruction)),'SEQ lacks native chapter restart');
  }
  const fields=value.paragraphs.flatMap(p=>p.fields);assert(fields.filter(field=>/^\s*REF\s+/.test(field.instruction)).length>=3,'References lost native REF fields');
  return {headings:headings.length,captions:captions.length,seqFields:captions.length,externalWordFieldUpdateCertified:false};
}
function applyEdits(source,edits){
  assert(Array.isArray(edits));let output='',cursor=0;
  for(const edit of [...edits].sort((a,b)=>a.from-b.from||a.to-b.to)){
    assert(Number.isInteger(edit.from)&&Number.isInteger(edit.to)&&edit.from>=cursor&&edit.from<=edit.to&&edit.to<=source.length,'Invalid UTF-16 edit');assert.equal(typeof edit.insert,'string');
    for(const offset of [edit.from,edit.to]){
      assert(!(offset>0&&offset<source.length&&/[\uD800-\uDBFF]/.test(source[offset-1])&&/[\uDC00-\uDFFF]/.test(source[offset])),'Edit split a surrogate pair');
      assert(!(offset>0&&source[offset-1]==='\r'&&source[offset]==='\n'),'Edit split an existing CRLF');
    }
    output+=source.slice(cursor,edit.from)+edit.insert;cursor=edit.to;
  }
  return output+source.slice(cursor);
}
function assertNumberingTransaction(value,source,changed=true){
  assert.equal(value.ok,true);assert.equal(value.transaction.base_revision,hash(source));assert.equal(value.transaction.origin,'editor');assert.equal(value.transaction.edits.length,changed?1:0,'Numbering must commit one source transaction');
  const result=value.result;assert.equal(result.content,applyEdits(source,value.transaction.edits));assert.equal(result.changed,changed);assert.equal(result.revision,hash(result.content));assert.equal(applyEdits(result.content,result.inverse),source,'Actual inverse is not exact');
  assert.equal(result.inverse.length,changed?1:0);
  if(value.plan){
    assert(Number.isInteger(value.plan.from)&&Number.isInteger(value.plan.to)&&value.plan.from>=0&&value.plan.to<=result.content.length);
    const declaration=result.content.slice(value.plan.from,value.plan.to).trim(),prefix='<!-- readmd-numbering:v1 ';
    assert(declaration.startsWith(prefix)&&declaration.endsWith('-->'),'Plan does not locate the actual source declaration');
    assert.deepEqual(value.config,JSON.parse(declaration.slice(prefix.length,-3).trim()),'Response materialized or erased source presence');
  }else assert.equal(value.config,null);
  return result;
}
async function verifyNumberingApi(call){
  const absent=await call('numbering-get-empty','readmd_publication_numbering',{action:'get',content:API_SOURCE});assert.equal(absent.revision,hash(API_SOURCE));assert.equal(absent.config,null);assert.equal(absent.plan,null);
  const applied=await call('numbering-apply','readmd_publication_numbering',{action:'apply',content:API_SOURCE,base_revision:absent.revision,config:CONFIG}),result=assertNumberingTransaction(applied,API_SOURCE);
  assert(result.content.startsWith('\ufeff---\r\n'));assert(!/(?<!\r)\n/.test(result.content),'API changed CRLF convention');assert(result.content.endsWith('Unicode 𝛂 stays authored.\r\n'));assert.equal((result.content.match(/<!-- readmd-numbering:v1 /g)||[]).length,1);
  assert.deepEqual(applied.plan.headings.map(heading=>[heading.id,heading.numbering?.text]),HEADINGS.map(row=>row.slice(0,2)));assert(!result.content.includes('# 1 Alpha'),'Projected numbers leaked into authored headings');
  const loaded=await call('numbering-get-applied','readmd_publication_numbering',{action:'get',content:result.content});assert.equal(loaded.revision,result.revision);assert.deepEqual(loaded.config,applied.config);assert.deepEqual(loaded.plan,applied.plan);
  const same=await call('numbering-apply-idempotent','readmd_publication_numbering',{action:'apply',content:result.content,base_revision:result.revision,config:CONFIG});assertNumberingTransaction(same,result.content,false);
  await call('numbering-apply-stale','readmd_publication_numbering',{action:'apply',content:result.content,base_revision:absent.revision,config:null},'transaction_conflict');
  const inverse=await call('numbering-common-inverse','readmd_content_transaction',{content:result.content,transaction:{base_revision:result.revision,origin:'editor',edits:result.inverse}});assert.equal(inverse.result.content,API_SOURCE);assert.equal(inverse.result.revision,absent.revision);
  const cleared=await call('numbering-clear','readmd_publication_numbering',{action:'apply',content:result.content,base_revision:result.revision,config:null});assertNumberingTransaction(cleared,result.content);assert.equal(cleared.config,null);assert.equal(cleared.plan,null);assert(!cleared.result.content.includes('readmd-numbering:'));assert(cleared.result.content.endsWith(API_SOURCE.slice(API_SOURCE.indexOf('# Alpha'))),'Clear altered authored body');
  const clearedGet=await call('numbering-get-cleared','readmd_publication_numbering',{action:'get',content:cleared.result.content});assert.equal(clearedGet.revision,cleared.result.revision);assert.equal(clearedGet.config,null);assert.equal(clearedGet.plan,null);
  const clearInverse=await call('numbering-clear-common-inverse','readmd_content_transaction',{content:cleared.result.content,transaction:{base_revision:cleared.result.revision,origin:'editor',edits:cleared.result.inverse}});assert.equal(clearInverse.result.content,result.content);assert.equal(clearInverse.result.revision,result.revision);
  return {requests:9,sourceSha256:hash(API_SOURCE),originalConfigAbsent:true,oneEdit:true,idempotent:true,staleRejected:true,exactInverse:true,clearAndInverse:true,sourceWritten:false};
}
function assertOffsetDocxInspection(value){
  assert.equal(value.kind,'docx-numbering');assert(['true','1'].includes(value.updateFields));assert.equal(value.wordApplicationRoundtripCertified,false);
  const headings=value.paragraphs.filter(p=>p.style==='Heading1'&&p.number);assert.equal(headings.length,2);assert.deepEqual(headings.map(p=>p.text),['Gamma','Delta']);
  for(const heading of headings){const instance=value.numbers[heading.number.id];assert(instance,'Unresolved offset numId');const rule=value.abstract[instance.abstract]?.find(row=>row.level===0);assert(rule,'Missing offset native level');assert.equal(rule.format,'upperRoman');assert.equal(rule.start,'3');assert.equal(rule.restart,'0');assert.equal(rule.text,'%1');}
  const captions=value.paragraphs.filter(p=>p.fields.some(field=>/\bSEQ\s+ReadMDEquation\b/.test(field.instruction)));assert.equal(captions.length,OFFSET_VALUES.length);
  for(let index=0;index<captions.length;index++){
    const caption=captions[index],label=OFFSET_EXPECTED[index];assert(caption.text.startsWith(label)&&!/[A-Za-z0-9]/.test(caption.text.slice(label.length,label.length+1)),'Non-one visible result corrupted: '+caption.text);
    assert(caption.fields.some(field=>/\bSTYLEREF\s+1\s+\\s\b/.test(field.instruction)&&field.cached===(index<OFFSET_VALUES.length-1?'III':'IV')),'Roman native chapter field missing');
    const seq=caption.fields.find(field=>/\bSEQ\s+ReadMDEquation\b/.test(field.instruction));assert.equal(seq.kind,'complex');assert.equal(seq.depth,1,'SEQ must be nested inside the formula');assert.equal(seq.cached,String(index<OFFSET_VALUES.length-1?index+1:1),'SEQ is the raw chapter ordinal');assert(/\\s\s+1\b/.test(seq.instruction));assert(!/\\r\s/.test(seq.instruction),'A literal SEQ reset cannot replace the native chapter restart');
    const formula=caption.fields.find(field=>/^\s*=\s*\+\s*25\s+\\\*\s+ALPHABETIC\s*$/.test(field.instruction));assert(formula,'Missing native offset formula');assert.equal(formula.kind,'complex');assert.equal(formula.depth,0);assert.equal(formula.cached,alpha(OFFSET_VALUES[index]));
  }
  return {headings:2,captions:captions.length,nestedSeqFields:captions.length,start:26,externalWordFieldUpdateCertified:false};
}
function assertOffsetReadback(markdown,projection){
  assert.equal(typeof markdown,'string');assert(markdown.includes('Gamma')&&markdown.includes('Delta'),'Offset readback lost chapters');
  if(markdown.includes('<!-- readmd-numbering:v1 ')){
    assert(projection&&projection.ok===true,'Editable readback requires an actual shared preparation response');
    assert.deepEqual((projection.source_labels||[]).map(label=>label.text),OFFSET_EXPECTED,'Actual imported rules no longer reproduce native field results');
    const declared=projection.source_numbering;assert(declared?.config?.captions?.equation,'Current imported native caption rules missing');
    assert.equal(declared.config.captions.equation.start,26);assert.equal(declared.config.captions.equation.format,'upperLetter');
    return {visibleLabels:OFFSET_EXPECTED,editableDeclarationPresent:true,numberingConfigurationRecovered:false};
  }
  const visible=markdown.replaceAll(/\s+/g,' ');for(const label of OFFSET_EXPECTED)assert(new RegExp(label.replace(/[.*+?^${}()|[\]\\]/g,'\\$&')+'(?![A-Za-z0-9])').test(visible),'Actual DOCX conversion lost non-one field result '+label);return {visibleLabels:OFFSET_EXPECTED,numberingConfigurationRecovered:false};
}
function callPayload(name,reply,error){
  if(error)return errorPayload(reply,error);
  if(name==='readmd_convert_to_markdown'){
    assert.notEqual(reply.isError,true,'Actual conversion returned an error');assert.notEqual(reply.structuredContent?.ok,false);
    const text=reply.content.filter(item=>item.type==='text').map(item=>item.text).join('\n');assert(text.length>0,'Actual conversion returned no Markdown');return text;
  }
  return payload(reply);
}

function assertNumberedImages(value,previous){
  assertDocxInspection(value);assert.equal(value.images?.length,2,'Actual drawing count changed');
  assert.equal(value.paragraphs.filter(row=>row.style==='Caption').length,7,'An alt visual Caption duplicated a real automatic caption');
  const bindings=[];
  for(const [index,alt]of ['One','Two'].entries()){
    const image=value.images[index];assert.equal(image.descriptionPresent,true);assert.equal(image.description,alt,'Actual docPr alt changed');
    assert.equal(image.sha256,PNG_SHA256,'Drawing does not reference the original PNG bytes');assert.equal(image.bytes,70);assert.equal(image.targetMode,'Internal');
    assert(['http://schemas.openxmlformats.org/officeDocument/2006/relationships/image','http://purl.oclc.org/ooxml/officeDocument/relationships/image'].includes(image.relationshipType));
    assert.equal(image.png.allChunkCrcValid,true);assert.deepEqual(image.png.dimensions,[1,1]);assert.deepEqual(image.png.rgba,[0,0,0,255]);
    const title=index===0?'First figure':'Second figure',label='Figure '+(index+1)+'-1',paragraph=value.paragraphs.find(row=>row.bodyIndex===image.bodyIndex+1);
    assert(paragraph,'Drawing is not immediately followed by its real caption');assert.equal(paragraph.style,'Caption');assert.equal(paragraph.text,label+': '+title);
    const seq=paragraph.fields.filter(row=>/^\s*SEQ\s+ReadMDFigure\b/.test(row.instruction));assert.equal(seq.length,1);assert.equal(seq[0].cached,'1');assert(/\\s\s+1\b/.test(seq[0].instruction));
    const chapter=paragraph.fields.filter(row=>/^\s*STYLEREF\s+(?:1|"Heading 1")\s+\\s\b/.test(row.instruction));assert.equal(chapter.length,1);assert.equal(chapter[0].cached,String(index+1));
    assert(paragraph.visibleRuns?.length>0);for(const run of paragraph.visibleRuns){assert.equal(run.properties.sz,'18','Caption Western axis changed');assert.equal(run.properties.szCs,'24','Caption CS axis changed');}
    const marks=(paragraph.bookmarkRanges||[]).filter(row=>row.text===label);assert.equal(marks.length,1,'Actual field label lacks one unique covering bookmark');assert(paragraph.bookmarks.includes(marks[0].name));
    bindings.push({id:index===0?'fig:a':'fig:b',bookmark:marks[0].name,label,alt,sha256:image.sha256});
  }
  assert.notEqual(bindings[0].bookmark,bindings[1].bookmark);
  const refs=value.paragraphs.flatMap(row=>row.fields).filter(field=>/^\s*REF\s+/.test(field.instruction));
  const figureRefs=refs.filter(field=>new RegExp('^\\s*REF\\s+'+bindings[1].bookmark+'\\s+\\\\h\\b').test(field.instruction));
  assert.equal(figureRefs.length,1,'The current figure REF targets another bookmark');assert.equal(figureRefs[0].cached,'Figure 2-1');
  const identity={bindings,bodyVisibleText:value.bodyVisibleText,tableCount:value.tableCount,mathTexts:value.mathTexts};
  assert.equal(identity.tableCount,2);assert.equal(typeof identity.bodyVisibleText,'string');assert(identity.bodyVisibleText.length>0);
  if(previous)assert.deepEqual(identity,previous,'Second native return lost body, image, caption or reference identity');
  return identity;
}
function assertNumberedImageSource(source,projection){
  assert.equal(projection.ok,true);assert.deepEqual(projection.diagnostics,[]);
  assert.deepEqual(projection.source_labels.map(row=>[row.id,row.text]),IDS.map((id,index)=>[id,EXPECTED[index]]),'Current source identities/labels changed');
  const body=source.replace(/^\uFEFF?---[ \t]*\r?\n[\s\S]*?\r?\n(?:---|\.\.\.)[ \t]*(?:\r?\n|$)/,''),tokens=marked.lexer(body,{gfm:true}).filter(row=>row.type!=='space');
  const structure=markdownStructure(source),images=[];
  for(let at=0;at<tokens.length;at++){
    const token=tokens[at];if(token.type!=='paragraph')continue;
    const pictures=(token.tokens||[]).filter(row=>row.type==='image');if(!pictures.length)continue;
    assert.equal(pictures.length,1);assert((token.tokens||[]).every(row=>row.type==='image'||row.type==='text'&&/^\s*$/.test(row.text)),'Image must be a real standalone top-level object');
    const next=tokens[at+1];assert.equal(next?.type,'paragraph','Image has no adjacent label paragraph');
    const paragraph=structure.paragraphs.find(row=>row.raw===next.raw);assert(paragraph,'Image label is hidden in markup/code');
    const label=/^:label\[(fig:[^\]\s]+)\]\s+([\s\S]+)$/.exec(paragraph.text);assert(label,'Image has no current figure source label');
    images.push({alt:pictures[0].text,href:pictures[0].href,id:label[1],title:label[2]});
  }
  assert.deepEqual(images.map(({alt,id,title})=>({alt,id,title})),[{alt:'One',id:'fig:a',title:'First figure'},{alt:'Two',id:'fig:b',title:'Second figure'}]);
  const refs=structure.paragraphs.flatMap(row=>[...row.text.matchAll(/:ref\[([^\]\s]+)\]/g)].map(match=>match[1]));assert(refs.includes('fig:b'),'Current figure REF source is missing');
  return images;
}
function imageAssetPath(href,documents,profile){
  assert.equal(typeof href,'string');let file;
  if(href.startsWith('/raw?')){const url=new URL(href,'http://owned.invalid');assert.equal(url.pathname,'/raw');assert.deepEqual([...url.searchParams.keys()],['p']);file=inside(profile,url.searchParams.get('p'));}
  else {assert(!/^[a-z][a-z0-9+.-]*:/i.test(href)&&!path.isAbsolute(href),'Unexpected external/absolute source image');file=inside(documents,path.resolve(documents,decodeURIComponent(href)));}
  return file;
}
function assertSavedImageAssets(images,documents,profile){
  return images.map(image=>{const file=imageAssetPath(image.href,documents,profile);assert.equal(fileHash(file),PNG_SHA256,'Current Markdown image resource differs from PNG');return {alt:image.alt,id:image.id,sha256:fileHash(file)};});
}
function assertMaterializedNumberedSave(value,before,logical,projection,documents,profile){
  assert.equal(typeof value.saved_content,'string','A staged-image save must return its actual final Markdown');
  const saved=value.saved_content;assertSavedEdit(value,before,saved);
  assert(Array.isArray(value.asset_edits));assert.equal(value.asset_edits.length,2,'Only the two current image destinations may change');
  const original=assertNumberedImageSource(logical,projection),images=assertNumberedImageSource(saved,projection);
  assert.equal(applyEdits(logical,value.asset_edits),saved,'Asset edits do not prove the actual saved source');
  for(const [index,edit]of value.asset_edits.entries()){
    assert.deepEqual(Object.keys(edit).sort(),['from','insert','to']);
    assert.equal(logical.slice(edit.from,edit.to),original[index].href,'Asset edit changed source outside its actual image destination');
    assert.equal(edit.insert,images[index].href);assert(/^readmd-assets\/[^/?#]+$/.test(edit.insert),'A saved image still depends on staging or an external target');
  }
  assert.deepEqual(images.map(({alt,id,title})=>({alt,id,title})),original.map(({alt,id,title})=>({alt,id,title})));
  assertSavedImageAssets(images,documents,profile);
  assert(Array.isArray(value.saved_assets));assert.equal(value.saved_assets.length,1,'Identical PNG payloads must use one permanent resource');
  const permanent=[...new Set(images.map(image=>imageAssetPath(image.href,documents,profile)))];assert.equal(permanent.length,1);
  const asset=value.saved_assets[0];assert.equal(asset.path,permanent[0]);assert.equal(asset.name,path.basename(asset.path));assert(images.every(image=>image.href===asset.reference));
  assert.equal(fileHash(asset.path),PNG_SHA256);
  const staged=[...new Set(original.map(image=>imageAssetPath(image.href,documents,profile)))];assert.equal(staged.length,1,'Fixture must own exactly one deduplicated staged PNG');
  assert.equal(path.dirname(staged[0]),path.join(profile,'resource-staging'));assert.equal(path.basename(staged[0]),PNG_SHA256+'.png');assert.equal(fileHash(staged[0]),PNG_SHA256);
  return {source:saved,images,staged,permanent};
}
function entityStagedSaveSource(stagedHref){
  assert.equal(typeof stagedHref,'string');assert(stagedHref.startsWith('/raw?p='));
  const encoded=stagedHref.replace('/raw?p=','&#47;raw&#63;p='),image='![Entity save]('+encoded+' "Entity title")';
  const prefix='<div>OWNED_CLOSED_HTML</div>\r\n\r\nUnicode 🚀\r\n\r\n';
  const suffix='\r\n\r\n[ordinary]('+encoded+')\r\n\r\n`![code]('+encoded+')`\r\n\r\n<div>\r\n![html-example]('+encoded+')\r\n</div>\r\n';
  const source=prefix+image+suffix;assert(!source.includes('/raw?'),'Entity control must exercise the original raw-URI fastpath miss');
  const tokens=marked.lexer(source.replace(/^\uFEFF/,''),{gfm:true});assert.equal(tokens[0].type,'html');assert(tokens[0].raw.startsWith('<div>OWNED_CLOSED_HTML</div>'));
  const images=tokens.filter(token=>token.type==='paragraph').flatMap(token=>token.tokens||[]).filter(token=>token.type==='image');
  assert.equal(images.length,1);assert.equal(images[0].raw,image);assert.equal(images[0].href,encoded);assert.equal(images[0].text,'Entity save');assert.equal(images[0].title,'Entity title');
  return {source,stagedHref,encoded,image,from:prefix.length,to:prefix.length+image.length,prefix,suffix};
}
function assertEntitySavePreview(value,before,logical){
  assert.equal(value.ok,true);assert.equal(value.dry_run,true);assert.equal(value.resource_rewrite_required,true);assert.equal(value.revision,hash(before));assert.equal(value.proposed_revision,hash(logical));assert.equal(value.checkpoint_id,null);assert.equal(value.saved_content,undefined);assert.equal(value.saved_assets,undefined);
}
function imageSavePreviewArguments(file,revision,before,source){
  // The advertised optional confirm property permits only true when present.
  // A read-only dry-run omits it; the kernel defaults it to false.
  return {file_path:file,expected_revision:revision,old_text:before,new_text:source,dry_run:true};
}
function assertEntityStagedSave(value,before,fixture,documents,profile){
  assert.equal(fixture.source,entityStagedSaveSource(fixture.stagedHref).source);
  assert.equal(typeof value.saved_content,'string','Entity image save did not materialize its current resource');
  assertSavedEdit(value,before,value.saved_content);
  assert(Array.isArray(value.saved_assets));assert.equal(value.saved_assets.length,1);const asset=value.saved_assets[0];
  assert.equal(typeof asset.reference,'string');assert(/^readmd-assets\/[^/?#]+$/.test(asset.reference),'Entity image must become a permanent relative destination');
  const permanent=imageAssetPath(asset.reference,documents,profile);assert.equal(path.dirname(permanent),path.join(documents,'readmd-assets'));
  assert.equal(asset.path,permanent);assert.equal(asset.name,path.basename(permanent));assert.equal(fileHash(permanent),PNG_SHA256);
  const image='![Entity save]('+asset.reference+' "Entity title")',expected=fixture.prefix+image+fixture.suffix;
  assert.equal(value.saved_content,expected,'Entity save changed authored HTML, code, link, image alt or title');
  assert.deepEqual(value.asset_edits,[{from:fixture.from,to:fixture.to,insert:image}],'Only the current real image may be rewritten');
  assert.equal(applyEdits(fixture.source,value.asset_edits),value.saved_content,'Entity save edits do not replay on the exact UTF16 source');
  const staged=imageAssetPath(fixture.stagedHref,documents,profile);assert.equal(path.dirname(staged),path.join(profile,'resource-staging'));assert.equal(path.basename(staged),PNG_SHA256+'.png');assert.equal(fileHash(staged),PNG_SHA256);
  return {source:value.saved_content,permanent,staged};
}
async function verifyEntityStagedSave(call,retain,evidence,stagedHref,report){
  const folder=path.join(evidence,'documents','EntitySave'),profile=path.join(evidence,'profile');fs.mkdirSync(folder);
  const file=retain('documents/EntitySave/EntityEncoded.md','OWNED_ENTITY_SAVE_SENTINEL\r\n'),before=fs.readFileSync(file,'utf8'),fixture=entityStagedSaveSource(stagedHref);
  retain('inspection/entity-image-logical.md',fixture.source);const initial=await call('entity-image-read-before-save','readmd_read_document',sourceReadArguments(file,before));assert.equal(initial.content,before);assert.equal(initial.revision,hash(before));
  const preview=await call('entity-image-save-preview','readmd_edit_document',imageSavePreviewArguments(file,initial.revision,before,fixture.source));assertEntitySavePreview(preview,before,fixture.source);assert.equal(fs.readFileSync(file,'utf8'),before);assert(!fs.existsSync(path.join(folder,'readmd-assets')));report.checks.push('Actual entity-encoded staged Image after closed HTML requires resource rewrite in dry-run without writing Markdown or permanent resources.');
  const value=await call('entity-image-save','readmd_edit_document',{file_path:file,expected_revision:initial.revision,old_text:before,new_text:fixture.source,dry_run:false,confirm:true}),saved=assertEntityStagedSave(value,before,fixture,folder,profile);assert.equal(fs.readFileSync(file,'utf8'),saved.source);
  const reopened=await call('entity-image-read-after-save','readmd_read_document',sourceReadArguments(file,saved.source));assert.equal(reopened.content,saved.source);assert.equal(reopened.revision,hash(saved.source));
  retain('inspection/entity-image-saved.md',saved.source);report.entityImageSave={file:path.relative(evidence,file),sha256:fileHash(file),permanent:path.relative(evidence,saved.permanent),permanentSha256:fileHash(saved.permanent),sameCurrentStagedPng:true,stagedSha256:fileHash(saved.staged),singleRealImageEdit:true,inertHtmlCodeLinkUnchanged:true};
  report.checks.push('Actual entity-encoded staged Image save commits one exact UTF16 rewrite and verified permanent PNG; identical URI in ordinary link, code and HTML stays authored.');
}

function removeOwnedStagedNumberedPng(staged,profile){
  assert(Array.isArray(staged));assert.equal(staged.length,1);
  const root=path.join(profile,'resource-staging'),file=inside(root,staged[0]);
  assert.equal(path.dirname(file),root);assert.equal(path.basename(file),PNG_SHA256+'.png');
  assert.equal(fs.lstatSync(root).isSymbolicLink(),false);assert.equal(fs.lstatSync(file).isSymbolicLink(),false);assert(fs.lstatSync(file).isFile());
  assert.equal(path.dirname(fs.realpathSync(file)),fs.realpathSync(root));
  assert.deepEqual(fs.readdirSync(root),[PNG_SHA256+'.png'],'Removal must be limited to this fixture\'s single staged PNG');
  assert.equal(fileHash(file),PNG_SHA256);const bytes=fs.statSync(file).size;fs.unlinkSync(file);assert(!fs.existsSync(file));
  return {file:path.basename(file),sha256:PNG_SHA256,bytes,removedBeforeNewProcess:true};
}
function recordNumberingSession(report,label,closed,eofRequested){
  const session=closed.session;assert(session,'Missing actual host session');
  const row={label,session,normalEofRequested:eofRequested,transportFatal:closed.transportFatal??null,logDescriptorsClosed:closed.logDescriptorsClosed};
  report.sessions.push(row);for(const [key,value]of Object.entries(closed))if(!['passed','error','sessions'].includes(key))report[key]=value;
  if(closed.passed===false){report.passed=false;report.error??=closed.error;}
  assert.equal(eofRequested,true);assert.equal(session.forcedStop,false);assert.equal(session.pendingAtClose,0);assert.deepEqual(session.hostExit,{code:0,signal:null});assert.equal(closed.transportFatal??null,null);assert.equal(closed.logDescriptorsClosed,true);
  if(report.sessions.length>1)assert.notEqual(session.pid,report.sessions[report.sessions.length-2].session.pid,'Reopen reused the same process');return row;
}
function assertEpubLinks(chapters){
  const content=chapters.map(([,text])=>text).join('\n');
  for(const expected of EXPECTED)assert(content.includes(expected),'Actual EPUB lost '+expected);
  const href=/href="([^"#]+\.xhtml)#([^"]+)"[^>]*>Equation 2\.1<\/a>/.exec(content);assert(href,'Actual equation cross-chapter reference missing');
  const target=chapters.find(([name])=>name.endsWith('/'+href[1])||name===href[1]);assert(target,'Actual reference chapter does not exist');assert(target[1].includes('id="'+href[2]+'"'),'Actual cross-chapter reference target missing');
  assert(!content.includes('readmd-numbering:v1'));return {targetChapter:href[1],targetId:href[2]};
}
function inspectWithPython(options,binding,evidence,retain,file,script,label){
  const inspected=spawnSync(options.python,[path.join(ROOT,script),file],{windowsHide:true,timeout:60000,maxBuffer:16*1024*1024,env:systemEnvironment(path.join(evidence,'profile'),path.join(evidence,'documents'))});
  retain('inspection/'+label+'.stdout.json',inspected.stdout||Buffer.alloc(0));retain('inspection/'+label+'.stderr.log',inspected.stderr||Buffer.alloc(0));assert(!inspected.error,inspected.error?.message);assert.equal(inspected.status,0,'Independent actual-file reader failed');verifyBinding(binding);return JSON.parse(inspected.stdout);
}
async function main(){
  const options=argumentsFor(process.argv.slice(2));if(options.help){console.log('node tools/test/publication-numbering-stdio.cjs --plan [--exports]\nnode tools/test/publication-numbering-stdio.cjs --candidate CANDIDATE_JSON [--exports --python EXISTING_QA_PYTHON]');return;}if(options.plan){console.log(JSON.stringify(planFor(options),null,2));return;}
  assert.equal(process.platform,'win32');const binding=bindingFor(options);if(options.exports)assert(fs.statSync(options.python).isFile());fs.mkdirSync(BASE,{recursive:true});const evidence=options.output?(fs.mkdirSync(options.output),options.output):fs.mkdtempSync(path.join(BASE,'stdio-'));
  for(const name of ['profile','documents','requests','responses','outputs','inspection','wire','wire-reopened'])fs.mkdirSync(path.join(evidence,name));
  const retain=(name,value)=>{const file=inside(evidence,path.join(evidence,name));fs.writeFileSync(file,typeof value==='string'||Buffer.isBuffer(value)?value:JSON.stringify(value,null,2)+'\n',{flag:'wx'});return file;};
  const report={schemaVersion:1,passed:false,scope:'bounded-P05-P08-numbering-real-stdio',synthetic:true,startedUtc:new Date().toISOString(),candidateBinding:binding,checks:[],outputs:[],sessions:[],roundtrips:[],fullP05Certified:false,fullP08Certified:false,fullBatch3Certified:false,guiStarted:false,wordApplicationRoundtripCertified:false,numberingReadbackLosslessCertified:false,pythonRuntime:options.exports?{path:options.python,sha256:fileHash(options.python),qaOnly:true}:null,limitations:planFor(options).limitations};let client,timer,imageReturn,sessionLabel='first';
  const call=async(label,name,args,error)=>{verifyBinding(binding);const request={name,arguments:args};assertToolCallSchema(request,client.tools);retain('requests/'+label+'.json',request);const reply=await client.tool(name,args,90000);retain('responses/'+label+'.json',reply);return callPayload(name,reply,error);};
  try{
    retain('plan.json',planFor(options));retain('documents/Input.md',SOURCE);retain('documents/pixel.png',Buffer.from(PNG,'base64'));const originalSourceHash=fileHash(path.join(evidence,'documents/Input.md'));
    client=new StdioClient({binary:binding.binary,assets:binding.assets,profile:path.join(evidence,'profile'),documents:path.join(evidence,'documents'),out:path.join(evidence,'wire')});timer=setTimeout(()=>{if(!client)return;client.transport.fail(Error('Numbering workflow deadline'),'workflow_deadline');client.forceStop();},240000);await client.initialize(options.exports);
    assert(client.tools.some(tool=>tool.name==='readmd_academic_document'));assert(client.tools.some(tool=>tool.name==='readmd_publication_numbering'));assert(client.tools.some(tool=>tool.name==='readmd_content_transaction'));
    report.numberingApi=await verifyNumberingApi(call);report.checks.push('Real numbering get/apply/idempotency/null/inverse/stale requests share one reversible source transaction; BOM, CRLF and Unicode remain authored.');
    const prepared=await call('prepare','readmd_academic_document',{content:SOURCE,baseDir:path.join(evidence,'documents')});assertProjection(prepared);report.checks.push('Actual source-owned plan and seven chapter captions agree; examples remain literal.');
    const reordered=FRONT+COMMENT(CONFIG)+'\n\n'+BETA+ALPHA+EXAMPLES;
    const moved=await call('chapter-reorder','readmd_academic_document',{content:reordered,baseDir:path.join(evidence,'documents')});assert.deepEqual(moved.source_labels.map(label=>[label.id,label.text]),[['eq:b','Equation 1.1'],['tbl:b','Table 1.1'],['fig:b','Figure 1-1'],['eq:a','Equation 2.1'],['eq:a2','Equation 2.2'],['tbl:a','Table 2.1'],['fig:a','Figure 2-1']]);report.checks.push('Actual chapter reorder recomputes per-kind counters and preserves label IDs.');
    const invalid=SOURCE.replace('"restartAfter":0','"restartAfter":1');await call('invalid-config','readmd_academic_document',{content:invalid,baseDir:path.join(evidence,'documents')},'publication_numbering_invalid');report.checks.push('Actual API rejects an impossible native heading restart.');
    const inline='Inline $x$.\n\n:label[eq:invalid]\n';const inlineResult=await call('inline-boundary','readmd_academic_document',{content:inline,baseDir:path.join(evidence,'documents')});assert.deepEqual(inlineResult.source_labels||[],[]);assert(inlineResult.diagnostics.some(d=>d.code==='academic_invalid_label'));report.checks.push('Inline math cannot masquerade as a display-equation object.');
    if(options.exports){
      for(const name of ['readmd_read_document','readmd_edit_document'])assert(client.tools.some(tool=>tool.name===name),'Missing actual save/read tool '+name);
      const settings={pdf:{engine:'rust'},cover:{enabled:false},toc:{enabled:false},frontMatter:{mode:'none'},footer:{pageNumbers:false},typography:{font:'Arial',size:12}};
      for(const format of ['docx','epub','pdf']){
        const file=path.join(evidence,'outputs','Numbered.'+format);const request=exportRequestFor(format,{markdown_content:SOURCE,output_path:file,base_dir:path.join(evidence,'documents'),confirm:true,title:'BATCH3NUMBERING20261020',options:settings});await call('export-'+format,request.name,request.arguments);assert(fs.statSync(file).size>0);
        const output={format,path:path.relative(evidence,file),sha256:fileHash(file),bytes:fs.statSync(file).size,inspectionPassed:false,numberingConfigurationRecovered:false};report.outputs.push(output);
        if(format==='docx'){const native=inspectWithPython(options,binding,evidence,retain,file,'tools/test/inspect-publication-numbering.py','docx');Object.assign(output,assertDocxInspection(native));imageReturn={identity:assertNumberedImages(native),input:file};}
        else if(format==='epub'){
          const parts=zipParts(fs.readFileSync(file)),chapters=[...parts].filter(([name])=>name.endsWith('.xhtml'));const html=chapters.map(([,text])=>text).join('\n');retain('inspection/epub-xhtml.xml',html);Object.assign(output,assertEpubLinks(chapters));output.captionTextVerified=true;
        }else{
          const inspected=inspectWithPython(options,binding,evidence,retain,file,'tools/test/inspect-mcp-publication-closure.py','pdf');assert.equal(inspected.kind,'pdf');const text=inspected.pages.map(page=>page.text).join('\n');assert(text.includes('BATCH3NUMBERING20261020')||text.includes('Alpha'));for(const expected of EXPECTED)assert(text.replaceAll(/\s+/g,' ').includes(expected),'Actual PDF lost '+expected);output.pages=inspected.pages.length;output.independentReader=inspected.reader;
        }
        const readback=await call('readback-'+format,'readmd_convert_to_markdown',{file_path:file});const visible=typeof readback==='string'?readback:JSON.stringify(readback);assert(visible.includes('Alpha')&&visible.includes('Beta'),'Actual file readback lost chapter text');retain('inspection/readback-'+format+'.json',readback);if(format==='docx'){
          const projection=await call('prepare-numbered-readback','readmd_academic_document',{content:readback,baseDir:path.join(evidence,'documents')}),images=assertNumberedImageSource(readback,projection);
          assertSavedImageAssets(images,path.join(evidence,'documents'),path.join(evidence,'profile'));
          await verifyEntityStagedSave(call,retain,evidence,images[0].href,report);
          const saved=retain('documents/NumberedReturned.md','OWNED_NUMBERED_SAVE_SENTINEL\n'),sentinel=fs.readFileSync(saved,'utf8'),before=await call('numbered-read-before-save','readmd_read_document',sourceReadArguments(saved,sentinel));assert.equal(before.content,sentinel);assert.equal(before.revision,hash(sentinel));
          const preview=await call('numbered-save-preview','readmd_edit_document',imageSavePreviewArguments(saved,before.revision,sentinel,readback));assert.equal(preview.ok,true);assert.equal(preview.dry_run,true);assert.equal(preview.resource_rewrite_required,true);assert.equal(preview.revision,before.revision);assert.equal(preview.proposed_revision,hash(readback));assert.equal(preview.checkpoint_id,null);assert.equal(fs.readFileSync(saved,'utf8'),sentinel);assert(!fs.existsSync(path.join(evidence,'documents/readmd-assets')));report.checks.push('Actual staged-image save preview marks resource rewrite without changing Markdown, history checkpoint or permanent assets.');
          const edited=await call('numbered-save','readmd_edit_document',{file_path:saved,expected_revision:before.revision,old_text:sentinel,new_text:readback,dry_run:false,confirm:true}),materialized=assertMaterializedNumberedSave(edited,sentinel,readback,projection,path.join(evidence,'documents'),path.join(evidence,'profile'));assert.equal(fs.readFileSync(saved,'utf8'),materialized.source);
          imageReturn={...imageReturn,file:saved,source:materialized.source,staged:materialized.staged,permanent:materialized.permanent,sha256:fileHash(saved)};report.roundtrips.push({label:'numbered-images',input:path.relative(evidence,file),saved:path.relative(evidence,saved),sha256:fileHash(saved),explicitRevisionProtectedSave:true,reopenedInNewProcess:false,secondExportWithoutOldOptions:false});report.checks.push('Actual numbered DOCX drawing/alt/PNG and figure fields survive conversion and explicit revision-protected source save.');
        }
        output.inspectionPassed=true;report.checks.push('Actual '+format+' inspected and converted back; editable numbering recovery is not inferred.');
      }
      const offsetFile=path.join(evidence,'outputs','NumberedOffset.docx'),offsetRequest=exportRequestFor('docx',{markdown_content:OFFSET_SOURCE,output_path:offsetFile,base_dir:path.join(evidence,'documents'),confirm:true,options:settings});retain('documents/Offset.md',OFFSET_SOURCE);await call('export-docx-offset',offsetRequest.name,offsetRequest.arguments);assert(fs.statSync(offsetFile).size>0);
      const offsetOutput={format:'docx',fixture:'non-one-start',path:path.relative(evidence,offsetFile),sha256:fileHash(offsetFile),bytes:fs.statSync(offsetFile).size,inspectionPassed:false,numberingConfigurationRecovered:false};report.outputs.push(offsetOutput);Object.assign(offsetOutput,assertOffsetDocxInspection(inspectWithPython(options,binding,evidence,retain,offsetFile,'tools/test/inspect-publication-numbering.py','docx-offset')));
      const offsetReadback=await call('readback-docx-offset','readmd_convert_to_markdown',{file_path:offsetFile});retain('inspection/readback-docx-offset.json',offsetReadback);
      const offsetProjection=offsetReadback.includes('<!-- readmd-numbering:v1 ')?await call('prepare-readback-docx-offset','readmd_academic_document',{content:offsetReadback,baseDir:path.join(evidence,'documents')}):null;
      Object.assign(offsetOutput,assertOffsetReadback(offsetReadback,offsetProjection));offsetOutput.inspectionPassed=true;report.checks.push('Actual DOCX offset fields contain nested native SEQ and Roman STYLEREF; independent visible results and real Markdown conversion preserve III/Z, III/AA, III/BB, III/ZZ, III/AAA and IV/Z.');
      const protectedFile=path.join(evidence,'outputs','Protected.docx'),sentinel='OWNED-NUMBERING-TARGET';retain('outputs/Protected.docx',sentinel);const request=exportRequestFor('docx',{markdown_content:invalid,output_path:protectedFile,base_dir:path.join(evidence,'documents'),confirm:true,overwrite:true,options:settings});await call('invalid-target-preserved',request.name,request.arguments,'publication_numbering_invalid');assert.equal(fs.readFileSync(protectedFile,'utf8'),sentinel);report.checks.push('Invalid numbering fails before replacing an existing owned export target.');
    }
    if(imageReturn){
      const closed={passed:true},eofRequested=!client.closed;await client.stop(closed);client=null;recordNumberingSession(report,sessionLabel,closed,eofRequested);
      for(const permanent of imageReturn.permanent)assert.equal(fileHash(permanent),PNG_SHA256);const removed=removeOwnedStagedNumberedPng(imageReturn.staged,path.join(evidence,'profile'));retain('inspection/owned-staged-png-removal.json',removed);Object.assign(report.roundtrips.find(row=>row.label==='numbered-images'),{permanentAssetsVerified:true,stagedPngRemovedBeforeReopen:removed});report.checks.push('Permanent source image hashes remain correct after normal EOF and removal of the only staged PNG owned by this fixture.');
      sessionLabel='reopened';client=new StdioClient({binary:binding.binary,assets:binding.assets,profile:path.join(evidence,'profile'),documents:path.join(evidence,'documents'),out:path.join(evidence,'wire-reopened')});await client.initialize(true);
      const reopened=await call('numbered-reopened-read','readmd_read_document',sourceReadArguments(imageReturn.file,imageReturn.source));assert.equal(reopened.content,imageReturn.source);assert.equal(reopened.revision,imageReturn.sha256);assert.equal(fileHash(imageReturn.file),imageReturn.sha256);
      const projection=await call('prepare-numbered-reopened','readmd_academic_document',{content:reopened.content,baseDir:path.join(evidence,'documents')});assertSavedImageAssets(assertNumberedImageSource(reopened.content,projection),path.join(evidence,'documents'),path.join(evidence,'profile'));
      const file=path.join(evidence,'outputs/NumberedReopened.docx'),request=exportRequestFor('docx',{markdown_content:reopened.content,output_path:file,base_dir:path.join(evidence,'documents'),confirm:true,options:{}});assert.deepEqual(request.arguments.options,{});assert(!Object.hasOwn(request.arguments,'preset'));
      await call('export-numbered-reopened',request.name,request.arguments);const native=inspectWithPython(options,binding,evidence,retain,file,'tools/test/inspect-publication-numbering.py','docx-reopened');assertNumberedImages(native,imageReturn.identity);
      const back=await call('readback-numbered-reopened','readmd_convert_to_markdown',{file_path:file});retain('inspection/readback-numbered-reopened.md',back);const current=await call('prepare-numbered-reopened-reimport','readmd_academic_document',{content:back,baseDir:path.join(evidence,'documents')});assertSavedImageAssets(assertNumberedImageSource(back,current),path.join(evidence,'documents'),path.join(evidence,'profile'));
      report.outputs.push({format:'docx',fixture:'numbered-image-reopened',path:path.relative(evidence,file),sha256:fileHash(file),bytes:fs.statSync(file).size,inspectionPassed:true,emptyOptions:true,noOldPreset:true,actualReimport:true});Object.assign(report.roundtrips[0],{reopenedInNewProcess:true,secondExportWithoutOldOptions:true,firstPid:report.sessions[0].session.pid,secondPid:client.info.pid,output:path.relative(evidence,file),actualReimport:true});report.checks.push('Different-PID source reopen and options:{} DOCX return retain actual PNG, alt, current figure IDs/REF, body and native 18/24 caption axes.');
    }
    assert.equal(fileHash(path.join(evidence,'documents/Input.md')),originalSourceHash);report.sourceUnchanged=true;report.passed=true;
  }catch(error){report.error=String(error.stack||error);}
  finally{clearTimeout(timer);if(client){const closed={passed:true},eofRequested=!client.closed;await client.stop(closed);try{recordNumberingSession(report,sessionLabel,closed,eofRequested);}catch(error){report.passed=false;report.error??=String(error.stack||error);}}else if(!report.sessions.length)report.processStarted=false;try{verifyBinding(binding);if(report.pythonRuntime)assert.equal(fileHash(options.python),report.pythonRuntime.sha256,'Existing QA runtime changed');report.finalCandidateBindingVerified=true;}catch(error){report.passed=false;report.bindingError=String(error.message);}report.finishedUtc=new Date().toISOString();retain('report.json',report);}
  console.log(JSON.stringify({evidence:path.relative(ROOT,evidence),passed:report.passed,checks:report.checks.length,outputs:report.outputs.length,error:report.error,transportFatal:report.transportFatal}));if(!report.passed)process.exitCode=1;
}
if(require.main===module)main().catch(error=>{console.error(error.stack);process.exitCode=1;});
module.exports={argumentsFor,planFor,assertProjection,assertDocxInspection,assertOffsetDocxInspection,assertOffsetReadback,assertNumberingTransaction,verifyNumberingApi,callPayload,assertEpubLinks,assertNumberedImages,assertNumberedImageSource,imageAssetPath,assertSavedImageAssets,assertMaterializedNumberedSave,entityStagedSaveSource,imageSavePreviewArguments,assertEntitySavePreview,assertEntityStagedSave,removeOwnedStagedNumberedPng,recordNumberingSession,PNG,PNG_SHA256,inside,CONFIG,COMMENT,SOURCE,EXPECTED,IDS,HEADINGS,API_SOURCE,OFFSET_CONFIG,OFFSET_SOURCE,OFFSET_VALUES,OFFSET_EXPECTED,QA_FILES,PRODUCT_FILES,BASE,ROOT};
