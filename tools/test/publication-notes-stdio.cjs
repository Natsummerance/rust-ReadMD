'use strict';
// Synthetic P06 acceptance through the source-bound Windows stdio host.
// No GUI, package installation, application automation or substitute renderer.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {spawnSync}=require('node:child_process');
const {StdioClient,exportRequestFor,assertToolCallSchema,verifyBinding,systemEnvironment,errorPayload}=require('./batch3-semantics-stdio.cjs');
const {payload}=require('./managed-tex-process-cold.cjs');
const {alpha}=require('./publication-numbering-oracle.cjs');
const zipParts=require('../../ui-tests/zip-parts.cjs');
const ROOT=path.resolve(__dirname,'../..'),BASE=path.join(ROOT,'.cache/capability-upgrade/batch3-20261020/notes');
const hash=value=>crypto.createHash('sha256').update(value).digest('hex'),fileHash=file=>hash(fs.readFileSync(file));
const QA_FILES=['tools/test/publication-notes-stdio.cjs','tools/test/publication-notes-stdio.test.cjs','tools/test/publication-numbering-oracle.cjs','tools/test/publication-numbering-oracle.test.cjs','tools/test/batch3-semantics-stdio.cjs','tools/test/managed-tex-process-cold.cjs','tools/test/inspect-mcp-publication-closure.py','tools/check-epub.mjs','tools/epubcheck.lock.json','ui-tests/zip-parts.cjs','ui-tests/zip-fixtures.cjs','ui-tests/publication-engines-native.cjs','tools/test/publication-runtime-reuse.cjs'];
const PRODUCT_FILES=['publication_layout.rs','publication_sections.rs','publication_blocks.rs','md_ast.rs','cross_references.rs','mdexport.rs','mcp.rs','docx_writer.rs','epub_writer.rs','typst_writer.rs','typst_writer_paragraphs.rs','typst_compile.rs','latex_compile.rs','publication_notes_tests.rs','docx_notes_layout_tests.rs','epub_notes_layout_tests.rs','typst_notes_layout_tests.rs','typst_heading_numbering_tests.rs'].map(name=>'rust/readmd-kernel/src/'+name);
const FENCE=String.fromCharCode(96).repeat(3);
const BODY='\uFEFF'+('<!-- readmd-section:v1 {"id":"notes-first"} -->\n\n# NOTESMATRIX First\n\nNOTESBEFORESAVE First[^a], repeat[^a], second[^b].\n\n## Heading in the same source section\n\nThird[^c].\n\n<!-- readmd-section:v1 {"id":"notes-second","break":"continuous"} -->\n\n# Later source section\n\nFourth[^d], repeated old[^a], fifth[^e].\n\n[^a]: NOTEA Retained **strong** source and [evidence](https://example.test/note).\n\n    > NOTEQUOTE Quoted evidence.\n\n    3. NOTELIST Ordered evidence.\n\n    | Measure | Value |\n    |---|---:|\n    | NOTETABLE | 98 |\n\n    '+FENCE+'rust\n    let NOTE_CODE = 98;\n    '+FENCE+'\n\n    $x^2$\n\n[^b]: NOTEB Second definition.\n\n[^c]: NOTEC Third definition.\n\n[^d]: NOTED Fourth definition.\n\n[^e]: NOTEE Fifth definition.\n').replaceAll('\n','\r\n');
const BASE_OPTIONS={cover:{enabled:false},toc:{enabled:false},frontMatter:{mode:'none'},footer:{pageNumbers:false},typography:{font:'Arial',size:11,lineHeight:1.2,spacing:6},footnotes:{size:9,lineHeight:1.2,start:27,numberFormat:'upperLetter'}};
const MARKS={continuous:['AA','AA','BB','CC','DD','AA','EE'],section:['AA','AA','BB','CC','AA','AA','BB'],page:['AA','AA','BB','CC','DD','AA','EE']};
const RICH_MARKERS=['NOTEA','NOTEB','NOTEC','NOTED','NOTEE','NOTEQUOTE','NOTELIST','NOTETABLE','NOTE_CODE'];
function optionsFor(restart,position,extras={}){return {...structuredClone(BASE_OPTIONS),...extras,footnotes:{...BASE_OPTIONS.footnotes,restart,position,...extras.footnotes}};}
function physicalPageSource(rows=72){
  assert(Number.isInteger(rows)&&rows>=12&&rows<=120);
  const paragraphs=Array.from({length:rows},(_,index)=>{
    const key=String(index).padStart(3,'0');
    return 'PAGEP'+key+'[^p'+key+'] then PAGEQ'+key+'[^q'+key+']. '+('Automatic physical pagination retains every note and its source. '.repeat(5))+'\n\n';
  }).join('');
  const definitions=Array.from({length:rows},(_,index)=>{const key=String(index).padStart(3,'0');return '[^p'+key+']: NOTEP'+key+' First evidence.\n\n[^q'+key+']: NOTEQ'+key+' Second evidence.\n\n';}).join('');
  return '# NOTESPHYSICAL\n\n'+paragraphs+definitions;
}
function longNoteSource(paragraphs=240){
  assert(Number.isInteger(paragraphs)&&paragraphs>=120&&paragraphs<=360);
  return '# NOTESLONG\n\nOne long note[^long], repeated[^long].\n\n[^long]: LONGNOTESTART A note must remain complete across automatic pages.\n\n'
    +Array.from({length:paragraphs},(_,index)=>'    LONGNOTE'+String(index).padStart(3,'0')+' A separately editable paragraph retains all evidence and ordinary line flow.\n\n').join('')
    +'    LONGNOTEEND Final evidence remains inside the note.\n';
}
function namespaceSource(){
  return '# ANCHORNOTE {#note:a}\n\nBody[^a], repeated[^a]. [LINKRETURN](#note-ref:a:1) and [LINKSECTION](#readmd-section-main).\n\n<!-- pagebreak -->\n\n# ANCHORRETURN {#note-ref:a:1}\n\n[LINKNOTE](#note:a).\n\n<!-- pagebreak -->\n\n<!-- readmd-section:v1 {"id":"main","break":"continuous"} -->\n\n# ANCHORSECTION {#readmd-section-main}\n\nSource section and author heading each retain their identity.\n\n[^a]: ANCHORNOTEBODY Unique retained note.\n';
}
function inspectNamespacePdf(value){
  assert.equal(value.kind,'pdf');assert(value.pages.length>=3,'Namespace proof requires physically distinct destinations');
  const locate=marker=>{
    const matches=value.pages.flatMap((page,index)=>page.words.filter(word=>word[4].replace(/[.,]$/,'')===marker).map(word=>({page:index,word})));
    assert.equal(matches.length,1,'Missing or duplicate namespace marker: '+marker);return matches[0];
  };
  const targets={LINKNOTE:locate('ANCHORNOTE'),LINKRETURN:locate('ANCHORRETURN'),LINKSECTION:locate('ANCHORSECTION')};locate('ANCHORNOTEBODY');
  for(const [marker,target]of Object.entries(targets)){
    const source=locate(marker),rect=source.word.slice(0,4);
    const links=(value.pages[source.page].links||[]).filter(link=>Array.isArray(link.from)&&link.from[0]<=rect[2]&&link.from[2]>=rect[0]&&link.from[1]<=rect[3]&&link.from[3]>=rect[1]);
    assert(links.some(link=>link.kind===1&&link.page===target.page&&Array.isArray(link.to)&&link.to[1]>=target.word[1]-100&&link.to[1]<=target.word[3]+40),'Namespace link targeted the wrong physical object: '+marker);
  }
  return {distinctPublicAndInternalNamespaces:true,actualPdfLinks:3,uniqueNoteBody:true,physicalPages:value.pages.length};
}
function inside(base,file){const relative=path.relative(base,path.resolve(file));assert(relative&&relative!=='..'&&!relative.startsWith('..'+path.sep)&&!path.isAbsolute(relative),'Path escaped owned evidence scope');return path.resolve(file);}
function sourceReadArguments(file,source){
  assert(typeof source==='string','Known source text is required for a bounded full read');
  const text=source.startsWith('\uFEFF')?source.slice(1):source,lines=Math.max(1,text.split('\n').length-Number(text.endsWith('\n')));
  assert(lines<=1000,'Full source read exceeds the real MCP 1000-line request limit');
  return {file_path:file,start_line:1,end_line:lines};
}
function argumentsFor(argv,env=process.env){
  const options={candidate:env.READMD_MCP_CANDIDATE,binary:env.READMD_BIN,assets:env.READMD_ASSETS_DIR,profile:env.READMD_ENGINE_REUSE_PROFILE,python:env.READMD_TEST_PYTHON,epubcheck:env.READMD_EPUBCHECK_JAR,pdf:false,plan:false};
  for(let index=0;index<argv.length;index++){
    const key=argv[index];if(key==='--plan'){options.plan=true;continue;}if(key==='--pdf'){options.pdf=true;continue;}if(key==='--help')return {help:true};
    assert(['--candidate','--binary','--assets','--profile','--python','--epubcheck','--output'].includes(key),'Unknown argument: '+key);
    assert(index+1<argv.length,'Missing argument: '+key);options[key.slice(2)]=argv[++index];
  }
  for(const key of ['candidate','binary','assets','profile','python','epubcheck','output'])if(options[key])options[key]=path.resolve(options[key]);
  if(options.output)inside(BASE,options.output);
  if(!options.plan){assert(options.candidate,'Required --candidate');assert(options.epubcheck,'Required installed offline --epubcheck jar');if(options.pdf){assert(options.python,'--pdf requires an existing independent --python reader');assert(options.profile,'--pdf requires the explicit reviewed owned --profile');}}
  return options;
}
function bindingFor(options){
  const candidate=JSON.parse(fs.readFileSync(options.candidate,'utf8')),binary=options.binary||candidate.binary,assets=options.assets||path.dirname(candidate.boot),boot=path.join(assets,'readmd.boot.js');
  const sourceFreeze=inside(ROOT,path.resolve(ROOT,candidate.sourceFreeze));assert.equal(fileHash(sourceFreeze),candidate.sourceFreezeSha256,'Source freeze changed');
  const freeze=JSON.parse(fs.readFileSync(sourceFreeze,'utf8')),boundFiles={};
  for(const name of [...QA_FILES,...PRODUCT_FILES]){
    const row=freeze.sources.find(row=>row.path===name);assert(row,'Candidate did not freeze notes source: '+name);
    const file=inside(ROOT,path.join(ROOT,name)),copy=inside(ROOT,path.join(ROOT,row.copy));boundFiles[name]={file,copy,sha256:row.sha256};
  }
  const binding={candidate:options.candidate,candidateSha256:fileHash(options.candidate),binary,assets,boot,binarySha256:candidate.binarySha256,bootSha256:candidate.bootSha256,sourceFreeze,sourceFreezeSha256:candidate.sourceFreezeSha256,boundFiles};
  verifyBinding(binding);return binding;
}
function attributes(tag){
  const result={};for(const [,key,value]of tag.matchAll(/([A-Za-z_:][-\w:.]*)="([^"]*)"/g)){assert(!Object.hasOwn(result,key),'Duplicate actual XML attribute');result[key]=value;}return result;
}
function inspectDocx(bytes,{position,restart,notice=true,normal=5,references=['1','1','2','3','4','1','5']}){
  const parts=zipParts(bytes),kind=position==='document'?'endnote':'footnote',xml=parts.get('word/'+kind+'s.xml'),settings=parts.get('word/settings.xml'),body=parts.get('word/document.xml');
  assert(xml&&settings&&body,'Missing native note story');
  const native={continuous:'continuous',section:'eachSect',page:'eachPage'}[restart];
  assert(settings.includes('<w:numRestart w:val="'+native+'"/>'),'Wrong native restart');
  assert(settings.includes('<w:numStart w:val="27"/>'));
  const refs=[...body.matchAll(new RegExp('<w:'+kind+'Reference\\b[^>]*/>','g'))].map(match=>attributes(match[0])['w:id']);
  assert.deepEqual(refs,references,'Native reference identities changed');
  const definitions=[...xml.matchAll(new RegExp('<w:'+kind+'\\b[^>]*>','g'))].map(match=>attributes(match[0]));
  const ids=definitions.filter(value=>!value['w:type']).map(value=>value['w:id']);assert.deepEqual(ids,Array.from({length:normal},(_,index)=>String(index+1)));
  assert.equal(new Set(definitions.map(value=>value['w:id'])).size,definitions.length,'Special story collided with a normal note');
  assert(!xml.includes('<w:separator/>'));assert(xml.includes('<w:continuationSeparator/>'));
  assert.equal(definitions.filter(value=>value['w:type']==='continuationNotice').length,notice?1:0);
  if(notice){assert(xml.includes('Continued &lt;next&gt; &amp; evidence'));assert(settings.includes('<w:'+kind+' w:id="'+(normal+1)+'"/>'));}
  for(const marker of RICH_MARKERS)assert(xml.includes(marker),'Rich note marker missing: '+marker);
  assert(xml.includes('<w:tbl>')&&xml.includes('<m:oMath')&&xml.includes('<w:numPr>'),'Structured note content flattened');
  const rels=parts.get('word/_rels/'+kind+'s.xml.rels');assert(rels?.includes('https://example.test/note'),'Part-local external link lost');
  return {kind,normalIds:ids,references:refs,specialStories:definitions.filter(value=>value['w:type']),nativeRestart:native,richContent:true,wordApplicationCertified:false};
}
function inspectEpub(bytes,expectedMarks){
  const parts=zipParts(bytes);assert.equal(parts.get('mimetype'),'application/epub+zip');
  const documents=[...parts].filter(([name])=>/chapter_\d+\.xhtml$/.test(name)),ids=new Map(),links=[],marks=[],identities=[];
  assert(documents.length>0);
  for(const [name,body]of documents){
    const localIds=new Set();for(const match of body.matchAll(/<[A-Za-z][\w:-]*\b[^>]*>/g)){
      const attrs=attributes(match[0]);if(attrs.id){assert(!localIds.has(attrs.id),'Duplicate EPUB id');localIds.add(attrs.id);}
      if(attrs.href?.includes('#')&&!/^[a-z][\w+.-]*:/i.test(attrs.href)){links.push({file:name,href:attrs.href});}
      if(attrs['epub:type']==='noteref')identities.push({file:name,id:attrs.id,href:attrs.href});
    }
    ids.set(name,localIds);
    marks.push(...[...body.matchAll(/<a\b[^>]*epub:type="noteref"[^>]*>\s*<sup>([^<]*)<\/sup><\/a>/g)].map(match=>match[1]));
  }
  for(const {file,href}of links){const [target,id]=href.split('#'),resolved=target?path.posix.normalize(path.posix.join(path.posix.dirname(file),target)):file;assert(ids.get(resolved)?.has(id),'Unresolved EPUB reference: '+file+' '+href);}
  if(expectedMarks)assert.deepEqual(marks,expectedMarks,'Chapter splitting replaced the requested numbering scope');
  const text=documents.map(([,body])=>body).join('\n');for(const marker of RICH_MARKERS)assert(text.includes(marker),'Missing EPUB note content: '+marker);
  return {marks,identities,internalLinks:links.length,allTargetsResolve:true,chapters:documents.length,physicalPagesCertified:false};
}
function noteLabels(page,markerPattern){
  const markers=page.words.filter(word=>markerPattern.test(word[4])).sort((a,b)=>a[1]-b[1]||a[0]-b[0]);
  return markers.map(marker=>{
    const matches=page.words.filter(word=>/^[A-Z]{1,3}$/.test(word[4])&&word[2]<=marker[0]+1&&marker[0]-word[2]<60&&Math.abs((word[1]+word[3]-marker[1]-marker[3])/2)<8)
      .sort((a,b)=>b[2]-a[2]);assert(matches.length,'Independent PDF reader did not find a native note label beside '+marker[4]);return {marker:marker[4],label:matches[0][4],bounds:marker.slice(0,4)};
  });
}
function inspectPageRestartPdf(value,rows=72){
  assert.equal(value.kind,'pdf');assert(value.pages.length>=3,'Automatic physical pagination did not reach three pages');
  const labels=value.pages.map(page=>noteLabels(page,/^NOTE[PQ]\d{3}$/));
  const all=labels.flat();assert.equal(all.length,rows*2);assert.equal(new Set(all.map(value=>value.marker)).size,rows*2);
  for(const page of labels.filter(page=>page.length))assert.deepEqual(page.map(value=>value.label),page.map((_,index)=>alpha(27+index)),'Native note numbers did not restart on an actual PDF page');
  const references=value.pages.map((page,index)=>page.words.flatMap(word=>{
    const match=/^PAGE([PQ]\d{3})([A-Z]+)[.]?$/.exec(word[4]);return match?[{id:match[1],label:match[2],page:index,bounds:word.slice(0,4)}]:[];
  }).sort((a,b)=>a.bounds[1]-b.bounds[1]||a.bounds[0]-b.bounds[0]));
  const refs=references.flat();assert.equal(refs.length,rows*2,'Missing native source callouts');assert.equal(new Set(refs.map(value=>value.id)).size,rows*2,'Duplicated native source callout');
  for(const page of references.filter(page=>page.length))assert.deepEqual(page.map(value=>value.label),page.map((_,index)=>alpha(27+index)),'Native source callouts did not restart on their physical reference page');
  let pairedLinks=0;
  for(let page=0;page<labels.length;page++)for(const definition of labels[page]){
    const ref=refs.find(ref=>'NOTE'+ref.id===definition.marker);assert(ref,'Missing source identity for '+definition.marker);
    assert.equal(page,ref.page,'Footnote definition starts on a different physical page from its reference: '+definition.marker);assert.equal(definition.label,ref.label,'Callout/definition label changed identity');
    const intersects=(a,b)=>a[0]<=b[2]&&a[2]>=b[0]&&a[1]<=b[3]&&a[3]>=b[1];
    const links=value.pages[page].links;assert(Array.isArray(links),'Independent PDF reader omitted native links');
    const forwards=links.filter(link=>link.kind===1&&link.page===page&&Array.isArray(link.from)&&intersects(link.from,ref.bounds)&&Array.isArray(link.to)&&Number.isFinite(link.to[1]));
    const returns=links.filter(link=>link.kind===1&&link.page===ref.page&&Array.isArray(link.from)&&link.from[2]<=definition.bounds[0]+1&&definition.bounds[0]-link.from[2]<60&&link.from[1]<=definition.bounds[3]&&link.from[3]>=definition.bounds[1]&&Array.isArray(link.to)&&Number.isFinite(link.to[1]));
    assert(forwards.some(forward=>returns.some(back=>{
      // Native entry anchors precede the superscript return link. This band
      // stays above that link, excluding the next note's adjacent anchor.
      const target=forward.to,point=back.to,height=back.from[3]-back.from[1];
      return target[1]>=back.from[1]-height-3&&target[1]<=back.from[1]+1
        &&point[1]>=forward.from[1]-2&&point[1]<=forward.from[3]+2
        &&(point[0]===null||Number.isFinite(point[0])&&point[0]>=forward.from[0]-2&&point[0]<=forward.from[2]+2);
    })),'Native forward/back link targets the wrong note or reference: '+definition.marker);pairedLinks++;
  }
  return {pages:value.pages.length,pagesWithNotes:labels.filter(page=>page.length).length,definitions:all.length,labels,references,nativeSourceCallouts:refs.length,pairedNativeLinks:pairedLinks,entryStartsOnReferencePage:true,actualPhysicalPageRestart:true};
}
function inspectNoteLayoutError(value,count=144){
  assert.equal(value.ok,false);assert.equal(value.error_code,'publication_note_layout');
  const details=value.details;assert(details&&typeof details==='object'&&!Array.isArray(details),'Native note layout error omitted structured geometry');
  assert.deepEqual(Object.keys(details).sort(),['coordinateSpace','definitionPage','noteIndex','referencePage'].sort());assert.equal(details.coordinateSpace,'pdfFootnoteStartPage');
  assert(Number.isInteger(details.noteIndex)&&details.noteIndex>=1&&details.noteIndex<=count);
  for(const key of ['definitionPage','referencePage'])assert(Number.isInteger(details[key])&&details[key]>=1&&details[key]<=100000);
  assert.notEqual(details.definitionPage,details.referencePage);return details;
}
async function verifyExistingNoteTarget(call,request,readTarget,expectedBytes){
  assert(!Object.hasOwn(call.arguments,'overwrite'),'First existing-target probe must omit overwrite');
  const expectedHash=hash(expectedBytes),preserved=stage=>{
    const actual=readTarget();assert.equal(hash(actual),expectedHash,stage+' replaced an existing real PDF');assert.deepEqual(actual,expectedBytes);
  };
  const exists=await request(call.name,call.arguments,'export-physical-pages-default-unsafe-no-overwrite','output_exists');
  assert.equal(exists.ok,false);assert.equal(exists.error_code,'output_exists');preserved('Missing-overwrite rejection');
  const failure=await request(call.name,{...call.arguments,overwrite:true},'export-physical-pages-default-unsafe-overwrite','publication_note_layout');
  const geometry=inspectNoteLayoutError(failure);preserved('Rejected default layout');
  return {exists,failure,geometry,existingTargetSha256:expectedHash};
}
function inspectSectionPdf(value){
  assert.equal(value.kind,'pdf');const labels=value.pages.flatMap(page=>noteLabels(page,/^NOTE[A-E]$/));
  assert.deepEqual(labels.map(value=>[value.marker,value.label]),[['NOTEA','AA'],['NOTEB','BB'],['NOTEC','CC'],['NOTED','AA'],['NOTEE','BB']]);
  assert(value.pages.some(page=>{const text=page.text;return text.includes('NOTEA')&&text.includes('NOTED');}),'Continuous same-page sections were not exercised');
  return {pages:value.pages.length,labels,samePageSectionRestart:true};
}
function inspectLongPdf(value,paragraphs=240){
  assert.equal(value.kind,'pdf');assert(value.pages.length>=3,'Long note did not span three physical pages');
  const text=value.pages.map(page=>page.text).join('\n'),markers=[...text.matchAll(/\bLONGNOTE(\d{3})\b/g)].map(match=>Number(match[1])).sort((a,b)=>a-b);
  assert.deepEqual(markers,Array.from({length:paragraphs},(_,index)=>index),'A long-note paragraph was lost or duplicated');
  assert.equal((text.match(/LONGNOTESTART/g)||[]).length,1);assert.equal((text.match(/LONGNOTEEND/g)||[]).length,1);
  for(const page of value.pages)for(const word of page.words.filter(word=>/^LONGNOTE/.test(word[4]))){assert(word[0]>=-1&&word[1]>=-1&&word[2]<=page.width+1&&word[3]<=page.height+1,'Long-note marker is clipped outside its physical page');}
  return {pages:value.pages.length,paragraphs,allMarkersOnce:true,wordApplicationCertified:false};
}
function markdownValue(result){assert.notEqual(result.isError,true);const text=(result.content||[]).filter(value=>value.type==='text').map(value=>value.text).join('\n');assert(text.trim(),'Actual conversion returned no Markdown');return text;}
function exportWarnings(value){
  const current=Object.hasOwn(value,'warnings'),legacy=Object.hasOwn(value,'warns');
  assert(current||legacy,'Actual export response omitted both known warning fields');
  for(const key of ['warnings','warns'])if(Object.hasOwn(value,key))assert(Array.isArray(value[key])&&value[key].every(item=>typeof item==='string'),'Invalid actual export '+key+' contract');
  if(current&&legacy)assert.deepEqual(value.warnings,value.warns,'Actual export warning fields disagree');
  if(Object.hasOwn(value,'warn_items'))assert(Array.isArray(value.warn_items)&&value.warn_items.every(item=>item&&typeof item==='object'&&!Array.isArray(item)&&typeof item.code==='string'),'Invalid actual typed warning contract');
  return current?value.warnings:value.warns;
}
function planFor(options={}){
  return {schemaVersion:1,scope:'P06 notes native restart, long-note flow and diagnosed reflow limits',transport:'real-stdio',synthetic:true,noProcessesStarted:true,noFilesWritten:true,
    inputSha256:hash(BODY),richMarkers:RICH_MARKERS,docx:{positions:['page','document'],restarts:['section','page'],specialStories:['separator','continuationSeparator','continuationNotice'],roundtrip:'actual conversion and second export preserve normal note identities; native settings are supplied again'},
    epub:{positions:['page','document'],restarts:['section','page'],splits:['none','h1','h2'],pageRestartWarning:'publication_note_page_restart_unsupported',independentValidator:'installed locked EPUBCheck 5.4.0',chapterRestartSubstitutionAllowed:false},
    pdf:{enabled:options.pdf===true,engine:'typst',physicalPagesMinimum:3,automaticPagination:true,rows:72,defaultPhysicalFixture:{expectedError:'publication_note_layout',existingTargetPreserved:true,successfulLayoutCertified:false},successfulPhysicalFixture:{typographyKeepLines:true,sourceUnchanged:true},longNoteParagraphs:240,longNotePolicies:['split','keepParagraphs'],internalAnchorCollisions:['note:a','note-ref:a:1','readmd-section-main'],existingIndependentReader:'PyMuPDF via inspect-mcp-publication-closure.py',actualWorkerRequired:true},
    limitations:['No Word/WPS/LibreOffice application rendering or editing is certified.','No independent EPUB reading application or fixed EPUB physical page is certified.','DOCX native restart/separator settings are tested as output controls, not lossless settings import.','Unsupported continuation stories and Typst document-end page restart must produce precise diagnostics.','Typst 0.15.1 default split-paragraph footnote placement is not certified; mismatched entry/reference starting pages must be rejected.','Reused managed runtime provenance is separate from current readiness; no OS-cold or release acceptance.'],
    fullP06Certified:false,fullBatch3Certified:false,guiStarted:false};
}
async function main(){
  const options=argumentsFor(process.argv.slice(2));if(options.help){console.log('node tools/test/publication-notes-stdio.cjs --plan [--pdf]\nnode tools/test/publication-notes-stdio.cjs --candidate CANDIDATE --epubcheck EXISTING_JAR [--pdf --profile REVIEWED_PROFILE --python EXISTING_QA_PYTHON]');return;}
  if(options.plan){console.log(JSON.stringify(planFor(options),null,2));return;}
  assert.equal(process.platform,'win32');const binding=bindingFor(options);fs.mkdirSync(BASE,{recursive:true});
  const evidence=options.output?(fs.mkdirSync(options.output),options.output):fs.mkdtempSync(path.join(BASE,'stdio-'));
  for(const name of ['profile','documents','outputs','requests','responses','inspection','wire','wire-reopened'])fs.mkdirSync(path.join(evidence,name));
  const retain=(name,value)=>{const file=inside(evidence,path.join(evidence,name));fs.writeFileSync(file,typeof value==='string'||Buffer.isBuffer(value)?value:JSON.stringify(value,null,2)+'\n',{flag:'wx'});return {path:name,sha256:fileHash(file)};};
  const report={...planFor(options),noProcessesStarted:undefined,noFilesWritten:undefined,passed:false,startedUtc:new Date().toISOString(),candidateBinding:binding,checks:[],outputs:[],sessions:[],independentEpubCheck:false,independentPdf:options.pdf?false:null,pdfReader:options.pdf?{path:options.python,sha256:fileHash(options.python),qaOnly:true}:null};
  let client,timer,reuse,assets=binding.assets,profile=path.join(evidence,'profile'),sequence=0;
  const stage=value=>{assert(!report.workflowTimedOut,'P06 workflow deadline already expired');report.stage=value;console.log(JSON.stringify({stage:value}));},check=value=>report.checks.push(value);
  const request=async(name,args,label,expectedError)=>{
    const call={name,arguments:args};assertToolCallSchema(call,client.tools);const id=String(++sequence).padStart(3,'0')+'-'+label;
    retain('requests/'+id+'.json',call);const result=await client.tool(name,args,120000);retain('responses/'+id+'.json',result);return expectedError?errorPayload(result,expectedError):result;
  };
  const exportFile=async(label,format,source,settings)=>{
    stage('Actual '+format+' '+label);verifyBinding(binding);
    const file=path.join(evidence,'outputs',label+'.'+format),call=exportRequestFor(format,{markdown_content:source,output_path:file,base_dir:path.join(evidence,'documents'),title:label,confirm:true,options:settings});
    const value=payload(await request(call.name,call.arguments,'export-'+label)),warnings=exportWarnings(value);assert(fs.statSync(file).size>0);report.outputs.push({label,format,path:path.relative(evidence,file),sha256:fileHash(file),bytes:fs.statSync(file).size,warnings,warn_items:value.warn_items});
    return {file,value,warnings,bytes:fs.readFileSync(file)};
  };
  const readback=async(file,label,markers=RICH_MARKERS)=>{
    const text=markdownValue(await request('readmd_convert_to_markdown',{file_path:file},'readback-'+label));retain('inspection/'+label+'-readback.md',text);for(const marker of markers)assert(text.includes(marker),'Actual file readback lost '+marker);return text;
  };
  const independentPdf=(file,label)=>{
    const result=spawnSync(options.python,[path.join(ROOT,'tools/test/inspect-mcp-publication-closure.py'),file],{windowsHide:true,timeout:60000,maxBuffer:32*1024*1024,env:systemEnvironment(profile,path.join(evidence,'documents'))});
    retain('inspection/'+label+'-pdf.stdout.json',result.stdout||Buffer.alloc(0));retain('inspection/'+label+'-pdf.stderr.log',result.stderr||Buffer.alloc(0));assert(!result.error,result.error?.message);assert.equal(result.status,0,'Independent PDF reader failed');return JSON.parse(result.stdout);
  };
  try{
    retain('plan.json',planFor(options));retain('documents/Section-notes.md',BODY);retain('documents/Physical-pages.md',physicalPageSource());retain('documents/Long-note.md',longNoteSource());
    if(options.profile){
      stage('Reviewed immutable engine reuse');
      const helpers=require('../../ui-tests/publication-engines-native.cjs'),reviewed=helpers.resolveEngineReuseProfile(options.profile);let preflight=0;
      const harness={root:ROOT,data:evidence,report,command(executable,args){const result=spawnSync(executable,args,{windowsHide:true,timeout:60000,maxBuffer:16*1024*1024});const label='runtime-'+String(++preflight).padStart(2,'0');retain('inspection/'+label+'.stdout.log',result.stdout||Buffer.alloc(0));retain('inspection/'+label+'.stderr.log',result.stderr||Buffer.alloc(0));assert.equal(result.status,0,'Reviewed runtime preflight failed');return result.stdout.toString('utf8');}};
      for(const [variable,name]of [['READMD_FONTS_PACK','readmd-fonts-2026.10.10-universal.readmd-pack'],['READMD_PDFIUM_PACK','readmd-pdfium-7881-windows-x64.readmd-pack'],['READMD_TYPST_PACK','readmd-typst-0.15.1-windows-x64.readmd-pack']])process.env[variable]??=path.join(ROOT,'.cache/capability-upgrade/locked-packs',name);
      reuse=helpers.prepareEngineProfileReuse(harness,reviewed,assets);assets=reuse.assets;profile=reviewed;
    }
    client=new StdioClient({binary:binding.binary,assets,profile,documents:path.join(evidence,'documents'),out:path.join(evidence,'wire')});
    timer=setTimeout(()=>{report.workflowTimedOut=true;if(client){client.transport.fail(Error('P06 workflow exceeded 600000ms'),'workflow_deadline');client.forceStop();}},600000);
    await client.initialize(true);
    const input=path.join(evidence,'documents/Section-notes.md'),read=payload(await request('readmd_read_document',sourceReadArguments(input,BODY),'read-source'));
    assert.equal(read.content,BODY.slice(1));assert.equal(read.encoding,'utf-8-sig');assert.equal(read.eol,'crlf');
    const beforeHash=fileHash(input),edit={file_path:input,expected_revision:read.revision,old_text:'NOTESBEFORESAVE',new_text:'NOTESAFTERSAVE'};
    const preview=payload(await request('readmd_edit_document',edit,'edit-preview'));assert.equal(preview.dry_run,true);assert.equal(fileHash(input),beforeHash);
    payload(await request('readmd_edit_document',{...edit,dry_run:false,confirm:true},'edit-commit'));
    await request('readmd_edit_document',{...edit,dry_run:false,confirm:true},'edit-stale','document_conflict');
    const saved=fs.readFileSync(input,'utf8');assert.equal(saved,BODY.replace('NOTESBEFORESAVE','NOTESAFTERSAVE'));check('Actual atomic source edit preserves BOM/CRLF, note labels, definitions and unrelated bytes; stale save is rejected.');
    const identities={};let baselineDocx;
    for(const position of ['page','document'])for(const restart of ['section','page']){
      const settings=optionsFor(restart,position,{footnotes:{separator:'none',continuationSeparator:'line',continuationNotice:'Continued <next> & evidence',longNote:'keepParagraphs'}});
      const label='notes-'+position+'-'+restart,out=await exportFile(label,'docx',saved,settings);
      retain('inspection/'+label+'-docx.json',inspectDocx(out.bytes,{position,restart}));
      const markdown=await readback(out.file,label);
      const roundtrip=await exportFile(label+'-reimported','docx',markdown,settings);
      assert.deepEqual(inspectDocx(roundtrip.bytes,{position,restart}).normalIds,['1','2','3','4','5']);
      baselineDocx??={settings,inspection:inspectDocx(out.bytes,{position,restart})};
      check(label+' native package and actual converter/re-export preserve five normal definitions and seven references.');
      for(const split of ['none','h1','h2']){
        const epubOptions={...settings,epub:{splitLevel:split}},name=label+'-'+split,out=await exportFile(name,'epub',saved,epubOptions),inspection=inspectEpub(out.bytes,MARKS[restart]);
        retain('inspection/'+name+'-epub.json',inspection);const key=position+'-'+split;
        if(identities[key])assert.deepEqual(identities[key],inspection.identities,'Restart changed note links');else identities[key]=inspection.identities;
        if(restart==='page')assert(out.warnings.includes('publication_note_page_restart_unsupported'),'EPUB page restart was silently approximated');
        assert(out.warnings.includes('publication_note_continuation_unsupported'),'EPUB continuation region was silently approximated');
        await readback(out.file,name);
      }
    }
    const epubFiles=report.outputs.filter(value=>value.format==='epub').map(value=>path.join(evidence,value.path));
    stage('Independent locked offline EPUBCheck');const {checkEpub}=await import('../check-epub.mjs');const epubcheck=await checkEpub(epubFiles,{jar:options.epubcheck,output:path.join(evidence,'inspection/epubcheck.json')});assert.equal(epubcheck.passed,true);report.independentEpubCheck=true;check('All twelve actual EPUBs pass the locked independent validator and every internal forward/back link resolves.');
    if(options.pdf){
      const pageSettings=optionsFor('page','page',{pdf:{engine:'typst'},footnotes:{separator:'line',continuationSeparator:'line',longNote:'split'}});
      const physicalSource=physicalPageSource(),physicalSettings={...pageSettings,typography:{...pageSettings.typography,keepLines:true}};
      const physical=await exportFile('physical-pages-keep-lines','pdf',physicalSource,physicalSettings);retain('inspection/physical-pages-keep-lines.json',inspectPageRestartPdf(independentPdf(physical.file,'physical-pages-keep-lines')));await readback(physical.file,'physical-pages-keep-lines',['NOTESPHYSICAL','NOTEP000','NOTEQ071']);
      stage('Actual default physical footnote rejection and preservation of an existing real PDF');verifyBinding(binding);
      const unsafeTarget=retain('outputs/physical-pages-default-unsafe.pdf',physical.bytes),unsafeFile=path.join(evidence,unsafeTarget.path);
      const unsafeCall=exportRequestFor('pdf',{markdown_content:physicalSource,output_path:unsafeFile,base_dir:path.join(evidence,'documents'),title:'physical-pages-default-unsafe',confirm:true,options:pageSettings});
      const {exists,failure,geometry}=await verifyExistingNoteTarget(unsafeCall,request,()=>fs.readFileSync(unsafeFile),physical.bytes);
      report.defaultPhysicalFixture={sourceSha256:hash(physicalSource),expectedError:'publication_note_layout',geometry,missingOverwriteError:'output_exists',explicitOverwriteRetried:true,existingTargetSeed:'physical-pages-keep-lines',existingTargetSha256:unsafeTarget.sha256,existingTargetPreservedAfterBothRejections:true,existingTargetPreserved:true,defaultLayoutCertified:false};
      retain('inspection/physical-pages-default-unsafe-rejection.json',{...report.defaultPhysicalFixture,missingOverwriteResponse:exists,response:failure});
      check('Original 72-paragraph default layout is rejected with precise native note geometry and preserves an existing real PDF; the same complete source passes physical restart/link checks only with explicit keepLines.');
      const sectionSettings=optionsFor('section','page',{pdf:{engine:'typst'},footnotes:{separator:'none',longNote:'split'}});
      const section=await exportFile('same-page-sections','pdf',saved,sectionSettings);retain('inspection/same-page-sections.json',inspectSectionPdf(independentPdf(section.file,'same-page-sections')));await readback(section.file,'same-page-sections');
      for(const policy of ['split','keepParagraphs']){
        const settings=optionsFor('continuous','page',{pdf:{engine:'typst'},footnotes:{longNote:policy,separator:'line'}});
        const out=await exportFile('long-note-'+policy,'pdf',longNoteSource(),settings);retain('inspection/long-note-'+policy+'.json',inspectLongPdf(independentPdf(out.file,'long-note-'+policy)));await readback(out.file,'long-note-'+policy,['LONGNOTESTART','LONGNOTE000','LONGNOTE239','LONGNOTEEND']);
      }
      const unsupported=await exportFile('endnote-page-diagnostic','pdf',saved,optionsFor('page','document',{pdf:{engine:'typst'},footnotes:{continuationNotice:'Continued evidence'}}));
      assert(unsupported.warnings.includes('publication_note_page_restart_unsupported')&&unsupported.warnings.includes('publication_note_continuation_unsupported'));
      retain('documents/Namespace-collisions.md',namespaceSource());
      for(const position of ['page','document']){
        const out=await exportFile('namespace-'+position,'pdf',namespaceSource(),optionsFor('continuous',position,{pdf:{engine:'typst'}}));
        retain('inspection/namespace-'+position+'.json',inspectNamespacePdf(independentPdf(out.file,'namespace-'+position)));
        await readback(out.file,'namespace-'+position,['ANCHORNOTE','ANCHORRETURN','ANCHORSECTION','ANCHORNOTEBODY']);
      }
      check('Author IDs which resemble private notes, return links and source sections remain distinct; three actual PDF link destinations resolve under footnote and endnote policies.');
      report.independentPdf=true;check('Pinned Typst workers produce physical per-page restarts with explicit keepLines, source-section restarts and complete three-page long notes under both policies; default inconsistent placements are rejected and unsupported document-end page restart is diagnosed.');
    }
    const firstSession={passed:true};await client.stop(firstSession);report.sessions.push(firstSession);assert(firstSession.passed);client=null;
    verifyBinding(binding);stage('Actual stdio process restart and saved note identities');
    client=new StdioClient({binary:binding.binary,assets,profile,documents:path.join(evidence,'documents'),out:path.join(evidence,'wire-reopened')});await client.initialize(true);
    const reopened=payload(await request('readmd_read_document',sourceReadArguments(input,saved),'read-source-after-process-restart'));assert.equal(reopened.content,saved.slice(1));assert.equal(reopened.revision,hash(saved));
    const out=await exportFile('reopened-source','docx',reopened.content,baselineDocx.settings);assert.deepEqual(inspectDocx(out.bytes,{position:'page',restart:'section'}).normalIds,baselineDocx.inspection.normalIds);
    check('Saved source and native normal note IDs survive an actual host process restart.');report.passed=true;
  }catch(error){report.error=String(error.stack||error);}
  finally{
    clearTimeout(timer);if(client){const session={passed:true};await client.stop(session);report.sessions.push(session);if(!session.passed)report.passed=false;}
    try{verifyBinding(binding);if(report.pdfReader)assert.equal(fileHash(options.python),report.pdfReader.sha256,'Independent PDF reader changed');reuse?.assertPreserved();report.finalCandidateBindingVerified=true;}catch(error){report.passed=false;report.bindingError=String(error.stack||error);}
    report.finishedUtc=new Date().toISOString();retain('report.json',report);
  }
  console.log(JSON.stringify({evidence:path.relative(ROOT,evidence),passed:report.passed,checks:report.checks.length,outputs:report.outputs.length,error:report.error,bindingError:report.bindingError}));if(!report.passed)process.exitCode=1;
}
if(require.main===module)main().catch(error=>{console.error(error.stack||error);process.exitCode=1;});
module.exports={ROOT,BASE,BODY,MARKS,RICH_MARKERS,BASE_OPTIONS,QA_FILES,PRODUCT_FILES,optionsFor,physicalPageSource,longNoteSource,namespaceSource,inspectNamespacePdf,argumentsFor,inside,sourceReadArguments,bindingFor,attributes,inspectDocx,inspectEpub,alpha,noteLabels,inspectPageRestartPdf,inspectNoteLayoutError,verifyExistingNoteTarget,inspectSectionPdf,inspectLongPdf,markdownValue,exportWarnings,planFor};
