'use strict';
// Candidate-bound real consumer evidence. --plan/--selftest never start ReadMD,
// a layout engine, a browser or a GUI. Runtime installation is never attempted.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {spawnSync}=require('node:child_process');
const {StdioClient,assertToolCallSchema,verifyBinding,systemEnvironment,exportRequestFor:legacyExportRequestFor}=require('./batch3-semantics-stdio.cjs');
const {payload}=require('./managed-tex-process-cold.cjs');
const captionQa=require('./batch3-caption-author-engine-cases.cjs');
const nativeQa=require('./batch3-author-inline-stdio.cjs');
const zipParts=require('../../ui-tests/zip-parts.cjs');
const ROOT=path.resolve(__dirname,'../..'),BASE=path.join(ROOT,'.cache/capability-upgrade/batch3-author-inline-20261021/consumers/engines');
const INSPECTOR='tools/test/inspect-publication-batch3.py',LINK_INSPECTOR='tools/test/inspect-mcp-publication-closure.py',AUTHOR_INSPECTOR='tools/test/inspect-author-inline.py';
const QA_FILES=['tools/test/batch3-author-inline-engines-stdio.cjs','tools/test/batch3-author-inline-engines-stdio.test.cjs','tools/test/batch3-caption-author-engine-cases.cjs','tools/test/batch3-caption-author-engine-cases.test.cjs','tools/test/batch3-author-inline-stdio.cjs','tools/test/batch3-author-inline-stdio.test.cjs','tools/test/inspect-author-inline.test.py','assets/vendor/marked.min.js','tools/test/batch3-semantics-stdio.cjs','tools/test/managed-tex-process-cold.cjs',INSPECTOR,LINK_INSPECTOR,AUTHOR_INSPECTOR,'tools/test/publication-smallcaps-proof.py','ui-tests/zip-parts.cjs','ui-tests/zip-fixtures.cjs','ui-tests/publication-engines-native.cjs','tools/test/publication-runtime-reuse.cjs'];
const PRODUCT_FILES=['publication_inline_text.rs','publication_inline_view.rs','publication_text_runs.rs','publication_fonts.rs','math_dsl.rs','html_math.rs','math_migration.rs','math_extension.rs','md_ast.rs','publication.rs','publication_typography.rs','publication_blocks.rs','publication_note_policies.rs','publication_profile.rs','publication_layout.rs','publication_metadata.rs','publication_metadata_formats.rs','publication_rich.rs','cross_references.rs','document_transaction.rs','upgrade_api.rs','server.rs','html_footnotes.rs','pdf_render.rs','publication_pdf_paragraph.rs','typst_writer.rs','typst_writer_paragraphs.rs','latex_writer.rs','latex_writer_paragraphs.rs','epub_writer.rs','docx_writer.rs','convert/docx_text_style.rs','mdexport.rs','typst_compile.rs','latex_compile.rs','mcp.rs','lib.rs'].map(name=>'rust/readmd-kernel/src/'+name).concat(['assets/js/reader/author-text.js','assets/js/reader/publication.js','assets/fonts/Waree-Regular.otf','assets/fonts/Waree.LICENSE.txt','assets/fonts/Waree.source.json']);
PRODUCT_FILES.push(...['convert.rs','docx_local_style.rs','publication_sections.rs','convert/docx_note_settings.rs','convert/docx_notes.rs','convert/docx_numbering_import.rs'].map(name=>'rust/readmd-kernel/src/'+name));
const sha=value=>crypto.createHash('sha256').update(value).digest('hex'),fileHash=file=>sha(fs.readFileSync(file));
const LINK='https://example.test/author-inline-physical';
const SIZE_TOLERANCE_PT=.025;
const TOOL_TIMEOUT_MS=120000,HOST_BUDGETS={first:1800000,reopened:1200000};
const SCRIPTS=[{id:'arabic',text:'العربية',rtl:true},{id:'hebrew',text:'עברית',rtl:true},{id:'devanagari',text:'देवनागरी',rtl:false},{id:'thai',text:'ภาษาไทย',rtl:false}];
const COMMON_MARKERS=[
  ['HEADBASE23',23],['HEADBIG30',30],['HEADRESET23',23],
  ['BODY11',11],['BODY30',30],['SMALL10',10],['CSLATIN10',10],['LINK30',30],['AFTER11',11],
  ['LOCALBASE175',17.5],['LOCALBIG30',30],['LOCALRESET175',17.5],['CODE105',10.5],
  ['CELLRESET105',10.5],['QUOTED125',12.5],['LIST125',12.5],
  ['METRICBASE11',11],['METRICBIG30',30],['METRICRESET11',11],
  ['NOTEBASE10',10],['NOTEBIG30',30],['NOTERESET10',10],['FINAL11',11],['中文字形',20]
].map(([text,size])=>({text,size}));
const SOURCE_ORDER=['HEADBASE23','HEADBIG30','HEADRESET23','BODY11','BODY30','SMALL10','CSLATIN10','LINK30','AFTER11','LOCALBASE175','LOCALBIG30','LOCALRESET175','CODE105','CELLRESET105','QUOTED125','LIST125','METRICBASE11','METRICBIG30','METRICRESET11','FINAL11'];
function span(attributes,content){const names={size:'data-size-pt',sizeCs:'data-size-cs-pt',cs:'data-cs',rtl:'data-rtl'};return '<span data-readmd-text="v1"'+Object.entries(attributes).map(([key,value])=>{assert(names[key]);return ' '+names[key]+'="'+value+'"';}).join('')+'>'+content+'</span>';}
function commonSource({reflowable=false}={}){return [
  '# HEADBASE23 '+span({size:30,sizeCs:10},'HEADBIG30 '+span({size:'base'},'HEADRESET23'))+' {#author-head}',
  'BODY11 '+span({size:30,sizeCs:10},'BODY30 '+span({size:10},'SMALL10')+' '+span({cs:true},'CSLATIN10')+' **['+span({size:30},'LINK30')+']('+LINK+')**')+' AFTER11',
  '<!-- readmd:layout v1 {"id":"author-local-base","text":{"size":17.5}} -->',
  'LOCALBASE175 '+span({size:30},'LOCALBIG30 '+span({size:'base'},'LOCALRESET175')),
  span({size:10.5},'`CODE105`')+' '+span({size:20,sizeCs:10},'中文字形'),
  '| ColA | ColB |\n| --- | --- |\n| '+span({size:30},span({size:'base'},'CELLRESET105'))+' | VALUE42 |',
  '> '+span({size:12.5},'QUOTED125'),
  '- '+span({size:12.5},'LIST125'),
  ...(reflowable?[]:['<!-- readmd:layout v1 {"id":"author-metric","paragraph":{"lineSpacing":{"mode":"exact","pt":60}}} -->']),
  'METRICBASE11 '+span({size:30},'METRICBIG30 '+span({size:'base'},'METRICRESET11')),
  'NOTE_CALL[^author-note] '+span({size:'base'},'FINAL11'),
  '[^author-note]: NOTEBASE10 '+span({size:30},'NOTEBIG30 '+span({size:'base'},'NOTERESET10')),
  ''
].join('\n\n');}
function scriptSource(script){return 'SCRIPTBEGIN '+span({size:30,sizeCs:10},script.text)+' SCRIPTEND\n';}
function explicitRtlSource(){return 'RTLBEGIN '+span({size:30,sizeCs:10,rtl:true},'Latin 123 العربية - 45')+' RTLEND\n';}
function optionsFor(engine){return {frontMatter:{mode:'none'},cover:{enabled:false},toc:{enabled:false},footer:{pageNumbers:false},typography:{font:'ReadMD Sans SC',size:11,lineHeight:1.5,spacing:6},headings:{h1:{size:23,before:4,after:4,pageBreakBefore:false,keepNext:false}},table:{cellSize:10.5},footnotes:{size:10,sizeCs:10},epub:{splitLevel:'none',fontSize:11},tex:{docClass:'article',useCtex:true},pdf:{engine}};}
// The older semantics workflow deliberately owns only three formats. This
// consumer lane also exercises HTML and standalone TeX through the real tool;
// call() still checks every resulting request against this host's tool schema.
function exportRequestFor(format,parameters){
  if(format!=='html'&&format!=='tex')return legacyExportRequestFor(format,parameters);
  assert(parameters&&typeof parameters==='object'&&!Array.isArray(parameters));
  assert(!Object.hasOwn(parameters,'output_format'),'The export request builder owns output_format');
  return {name:'readmd_export_document',arguments:{...parameters,output_format:format}};
}
function inside(base,file){const relative=path.relative(base,path.resolve(file));assert(relative&&relative!=='..'&&!relative.startsWith('..'+path.sep)&&!path.isAbsolute(relative),'Path escaped owned evidence directory');return path.resolve(file);}
function argumentsFor(argv,env=process.env){
  const options={candidate:env.READMD_MCP_CANDIDATE,binary:env.READMD_BIN,assets:env.READMD_ASSETS_DIR,profile:env.READMD_ENGINE_REUSE_PROFILE,python:env.READMD_TEST_PYTHON,plan:false,selftest:false};
  for(let at=0;at<argv.length;at++){const key=argv[at];if(key==='--plan'){options.plan=true;continue;}if(key==='--selftest'){options.selftest=true;continue;}if(key==='--help')return {help:true};assert(['--candidate','--binary','--assets','--profile','--python','--output'].includes(key),'Unknown argument '+key);assert(at+1<argv.length,'Missing value '+key);options[key.slice(2)]=argv[++at];}
  for(const key of ['candidate','binary','assets','profile','python','output'])if(options[key])options[key]=path.resolve(options[key]);
  if(options.output)inside(BASE,options.output);if(!options.plan&&!options.selftest){for(const key of ['candidate','profile','python'])assert(options[key],'Required --'+key);}return options;
}
function bindingFor(options){
  const candidate=JSON.parse(fs.readFileSync(options.candidate,'utf8')),binary=options.binary||candidate.binary,assets=options.assets||path.dirname(candidate.boot),boot=path.join(assets,'readmd.boot.js');
  const sourceFreeze=inside(ROOT,path.resolve(ROOT,candidate.sourceFreeze));assert.equal(fileHash(sourceFreeze),candidate.sourceFreezeSha256);const freeze=JSON.parse(fs.readFileSync(sourceFreeze,'utf8')),boundFiles={};
  for(const name of [...QA_FILES,...PRODUCT_FILES]){const row=freeze.sources.find(row=>row.path===name);assert(row,'Candidate did not bind '+name);boundFiles[name]={file:inside(ROOT,path.join(ROOT,name)),copy:inside(ROOT,path.join(ROOT,row.copy)),sha256:row.sha256};}
  const binding={candidate:options.candidate,candidateSha256:fileHash(options.candidate),binary,assets,boot,binarySha256:candidate.binarySha256,bootSha256:candidate.bootSha256,sourceFreeze,sourceFreezeSha256:candidate.sourceFreezeSha256,boundFiles};verifyBinding(binding);return binding;
}
function normalized(value){return value.normalize('NFKC').replace(/[\u200e\u200f\u202a-\u202e\u2066-\u2069]/gu,'');}
function locate(characters,marker,key='c'){
  const text=characters.map(char=>char[key]).join(''),from=text.indexOf(marker);assert(from>=0,'Actual character stream lost '+marker);assert.equal(text.indexOf(marker,from+marker.length),-1,'Actual marker duplicated '+marker);
  let offset=0;const result=characters.filter(char=>{const start=offset;offset+=char[key].length;return start<from+marker.length&&offset>from;});assert.equal(result.map(char=>char[key]).join(''),marker,'Marker boundaries do not match actual characters');return {from,characters:result};
}
function union(characters,key='bbox'){return [Math.min(...characters.map(c=>c[key][0])),Math.min(...characters.map(c=>c[key][1])),Math.max(...characters.map(c=>c[key][2])),Math.max(...characters.map(c=>c[key][3]))];}
function overlap(a,b){return Math.min(a[2],b[2])>Math.max(a[0],b[0])&&Math.min(a[3],b[3])>Math.max(a[1],b[1]);}
function near(actual,expected,label){assert(Number.isFinite(actual)&&Math.abs(actual-expected)<=SIZE_TOLERANCE_PT,label+': '+actual+' != '+expected+' physical pt');}
function checkTrace(trace){
  // PyMuPDF documents a -1 GID only for additional Unicode components of
  // one ligature: the actual preceding glyph owns their shared origin and
  // the continuation has an exactly zero-width bbox. GID 0 remains missing.
  let head=null;for(const char of trace.chars){assert(Number.isInteger(char.unicode)&&char.unicode>=0&&char.unicode<=0x10ffff);if(char.unicode<=32||char.unicode===127)continue;assert(![0xfffd,0x25a1].includes(char.unicode),'Actual output contains replacement/tofu text');assert(Number.isInteger(char.glyphId));if(char.glyphId>0){head=char;continue;}assert(char.glyphId===-1&&head&&head.glyphId>0&&char.origin.length===2&&char.origin.every((n,i)=>Number.isFinite(n)&&Math.abs(n-head.origin[i])<.00001)&&char.bbox.length===4&&char.bbox.every(Number.isFinite)&&Math.abs(char.bbox[2]-char.bbox[0])<.00001,'Actual output contains a missing or unbound glyph');}
}
function checkGlyphs(value){
  assert.equal(value.kind,'pdf');assert(value.independentParsers?.pypdf&&value.independentParsers?.pymupdf,'Two actual PDF parsers are required');assert(value.pages.length>0&&value.pages.length<=32);
  const characters=[];for(const [pageIndex,page]of value.pages.entries()){
    assert(Number.isFinite(page.width)&&Number.isFinite(page.height)&&page.width>0&&page.height>0);
    assert(Array.isArray(page.characters)&&page.characters.length>0,'Actual glyph-bound inspection was not enabled');
    for(const char of page.characters){assert(typeof char.c==='string'&&char.c.length);assert(char.bbox.length===4&&char.bbox.every(Number.isFinite));assert(char.bbox[0]<=char.bbox[2]&&char.bbox[1]<=char.bbox[3]);assert(char.origin.length===2&&char.origin.every(Number.isFinite));assert(Number.isFinite(char.size)&&char.size>0);if(!/^\s+$/u.test(char.c)){assert(char.bbox[0]>=-.75&&char.bbox[1]>=-.75&&char.bbox[2]<=page.width+.75&&char.bbox[3]<=page.height+.75,'Actual author glyph is outside the page');}characters.push({...char,page:pageIndex});}
    assert(Array.isArray(page.glyphTraces)&&page.glyphTraces.length,'Actual font glyph IDs unavailable');
    for(const trace of page.glyphTraces)checkTrace(trace);
  }
  assert(!/[\uFFFD]/u.test(value.text+value.pypdfText));return characters;
}
function assertOperatorSizes(value,markers){
  assert.equal(value.reader,'pypdf content visitor');assert(Array.isArray(value.characters));
  for(const marker of markers){const actual=locate(value.characters,marker.text,'c').characters;for(const char of actual)near(char.size,marker.size,'Independent PDF text-operator size '+marker.text);}
  return {markers:markers.length,reader:value.reader,version:value.version};
}
function epubRoles(value){
  assert.equal(value?.kind,'author-inline-epub');assert(/^[a-f0-9]{64}$/u.test(value.sha256));
  const safePart=part=>{assert(typeof part==='string'&&part.length>0&&part.length<=4096&&!/[\\:#?\x00-\x1f\x7f]/u.test(part)&&!part.startsWith('/'));assert.equal(path.posix.normalize(part),part);assert(part!=='.'&&part!=='..'&&!part.startsWith('../'));return part;};
  safePart(value.packageDocument);assert(Array.isArray(value.manifest)&&value.manifest.length>0&&value.manifest.length<=4096);
  const manifest=new Map(),parts=new Set();
  for(const row of value.manifest){assert(typeof row.id==='string'&&row.id.length>0&&!manifest.has(row.id));safePart(row.part);assert(!parts.has(row.part));assert(Array.isArray(row.properties)&&new Set(row.properties).size===row.properties.length);manifest.set(row.id,row);parts.add(row.part);}
  const navParts=new Set(value.manifest.filter(row=>row.properties.includes('nav')).map(row=>row.part));assert.equal(navParts.size,1,'One actual OPF nav item is required');assert.deepEqual([...value.navigationParts].sort(),[...navParts].sort());
  assert(Array.isArray(value.spine)&&value.spine.length>0&&value.spine.length<=32);const spineParts=new Set();
  for(const row of value.spine){const declared=manifest.get(row.idref);assert(declared&&declared.part===row.part&&declared.mediaType==='application/xhtml+xml');assert(typeof row.linear==='boolean'&&!spineParts.has(row.part));spineParts.add(row.part);}
  assert(value.manifest.filter(row=>navParts.has(row.part)).every(row=>row.mediaType==='application/xhtml+xml'));
  assert(Array.isArray(value.texts)&&Array.isArray(value.links)&&Array.isArray(value.identities));
  const bodyParts=new Set([...spineParts].filter(part=>!navParts.has(part)));
  for(const row of value.texts){assert(typeof row.text==='string');if(row.contentRole==='body'){assert(spineParts.has(row.part),'Body text is outside the actual OPF spine');if(row.text.trim())bodyParts.add(row.part);}else if(row.contentRole==='navigation')assert(navParts.has(row.part),'Non-nav text claimed navigation role');else assert(['auxiliary','metadata'].includes(row.contentRole));}
  assert(bodyParts.size>0);assert.deepEqual([...value.bodyParts].sort(),[...bodyParts].sort());
  const identities=new Set();
  for(const row of value.identities){assert(typeof row.id==='string'&&row.id.length>0&&typeof row.text==='string');assert(parts.has(row.part));const key=JSON.stringify([row.part,row.id]);assert(!identities.has(key),'Duplicate current XHTML identity');identities.add(key);}
  assert(Array.isArray(value.navigation)&&value.navigation.some(row=>row.navigationType==='toc'),'Actual TOC anchors are required');
  for(const link of value.navigation){
    assert(navParts.has(link.part)&&link.contentRole==='navigation'&&['toc','landmarks','page-list'].includes(link.navigationType));
    assert(value.links.some(row=>row.part===link.part&&row.href===link.href&&row.text===link.text&&row.navigationType===link.navigationType&&row.contentRole==='navigation'),'Navigation has no actual XHTML anchor');
    assert(typeof link.href==='string'&&link.href.length>0);const segments=link.href.split('#');assert(segments.length<=2);const file=decodeURIComponent(segments[0]);assert(!/[\\:\x00-\x1f\x7f?]/u.test(file)&&!file.startsWith('/'));
    const targetPart=file?path.posix.normalize(path.posix.join(path.posix.dirname(link.part),file)):link.part;safePart(targetPart);assert.equal(link.targetPart,targetPart);assert(bodyParts.has(targetPart)||navParts.has(targetPart));
    const fragment=segments[1]?decodeURIComponent(segments[1]):null;assert.equal(link.fragment,fragment);let role,text;
    if(fragment!==null){const targets=value.identities.filter(row=>row.part===targetPart&&row.id===fragment);assert.equal(targets.length,1,'Navigation identity is missing/duplicate');role=targets[0].contentRole;text=targets[0].text;}else{role=bodyParts.has(targetPart)?'body':'navigation';text=value.texts.filter(row=>row.part===targetPart&&row.contentRole===role).map(row=>row.text).join('');}
    assert.equal(link.targetContentRole,role);assert.equal(link.targetText,text);if(link.navigationType==='toc'){assert(bodyParts.has(targetPart));assert.equal(role,'body','TOC targets must be actual body content');}
  }
  for(const part of navParts)assert(!bodyParts.has(part),'Combined navigation and body requires region evidence; body text cannot be excluded');
  return {bodyParts,navParts};
}
function epubChapterInput(value){
  epubRoles(value);const spine=value.spine.map(row=>{const anchors=value.identities.filter(identity=>identity.part===row.part).map(identity=>identity.id);assert(anchors.length>0&&anchors.length<=8192,'Every mapped spine part needs real current XHTML anchors');return {...row,anchors};});
  const input={sourceSha256:value.sha256,spine};assert(Buffer.byteLength(JSON.stringify(input))<=2*1024*1024);return input;
}
function epubBodyPages(native,reader,physicalPageCount){
  const roles=epubRoles(native),input=epubChapterInput(native);assert.equal(reader?.reader,'MuPDF EPUB');assert.equal(reader.sourceSha256,native.sha256);assert.equal(reader.javascriptExecuted,false);assert.equal(reader.hostFontsAdded,false);
  const mapping=reader.chapterMapping;assert.equal(mapping?.coordinateSpace,'pdfPageIndex');assert.equal(mapping.chapterCount,input.spine.length);assert.equal(mapping.physicalPageCount,physicalPageCount);assert(Array.isArray(mapping.spine)&&mapping.spine.length===input.spine.length);
  const bodyPages=[],navigationPages=[];let nextPage=0;
  for(const [chapter,row]of mapping.spine.entries()){
    const declared=input.spine[chapter];for(const key of ['idref','part','linear'])assert.equal(row[key],declared[key],'MuPDF chapter does not match actual OPF spine');assert.equal(row.chapter,chapter);assert(Number.isInteger(row.pageCount)&&row.pageCount>0&&row.pageCount<=32);
    const pages=Array.from({length:row.pageCount},(_,index)=>nextPage+index);assert.deepEqual(row.pages,pages,'MuPDF chapter page coverage is not exact');assert.deepEqual(row.locations,pages.map((page,index)=>({page,chapter,pageWithinChapter:index})),'MuPDF inverse locations disagree with physical pages');
    assert.deepEqual(row.partLocation,{chapter,pageWithinChapter:0,page:nextPage},'MuPDF part URI does not resolve to this chapter');
    assert(Array.isArray(row.anchors));assert.deepEqual(row.anchors.map(anchor=>anchor.id).sort(),[...declared.anchors].sort(),'Actual source anchors were lost or substituted');
    for(const anchor of row.anchors){assert.equal(anchor.chapter,chapter);assert(Number.isInteger(anchor.pageWithinChapter)&&anchor.pageWithinChapter>=0&&anchor.pageWithinChapter<row.pageCount);assert.equal(anchor.page,pages[anchor.pageWithinChapter]);}
    if(roles.bodyParts.has(row.part))bodyPages.push(...pages);else{assert(roles.navParts.has(row.part),'Unclassified spine chapter cannot be excluded');navigationPages.push(...pages);}nextPage+=row.pageCount;
  }
  assert.equal(nextPage,physicalPageCount,'Some real reader pages have no OPF chapter');assert(bodyPages.length>0);
  return {coordinateSpace:'pdfPageIndex',bodyPages,navigationPages,bodyParts:[...roles.bodyParts],navigationParts:[...roles.navParts],spine:mapping.spine};
}
function assertCommonPdf(value,links,operators,{epubReader=false,epubNative,epubReflow}={}){
  const allCharacters=checkGlyphs(value),epubScope=epubReader?epubBodyPages(epubNative,epubReflow,value.pages.length):null,bodyPages=epubScope?new Set(epubScope.bodyPages):null;
  const characters=bodyPages?allCharacters.filter(char=>bodyPages.has(char.page)):allCharacters,locations=[];for(const marker of COMMON_MARKERS){const actual=locate(characters,marker.text);for(const char of actual.characters)near(char.size,marker.size,'Actual glyph size '+marker.text);assert.equal(new Set(actual.characters.map(c=>c.page)).size,1,'One short marker was split over pages');locations.push({text:marker.text,size:marker.size,page:actual.characters[0].page,bounds:union(actual.characters),from:actual.from});}
  const ordered=SOURCE_ORDER.map(marker=>locations.find(item=>item.text===marker).from);assert(ordered.every((from,index)=>index===0||from>ordered[index-1]),'Actual author text was reordered');
  let copy=value.pypdfText,bodyOperators=operators;
  if(bodyPages){assert(Array.isArray(operators.pages)&&operators.pages.length===value.pages.length);for(const [index,row]of operators.pages.entries()){assert.equal(row.page,index);assert(typeof row.text==='string'&&row.text.length<=1024*1024);}assert.equal(operators.pages.map(row=>row.text).join('\n'),value.pypdfText,'Per-page copy evidence differs from the independently inspected PDF');copy=operators.pages.filter(row=>bodyPages.has(row.page)).map(row=>row.text).join('\n');bodyOperators={...operators,characters:operators.characters.filter(char=>bodyPages.has(char.page))};}
  for(const marker of COMMON_MARKERS){const text=normalized(copy),needle=normalized(marker.text),from=text.indexOf(needle);assert(from>=0,'Independent pypdf copy text lost '+marker.text);if(bodyPages)assert.equal(text.indexOf(needle,from+needle.length),-1,'Independent pypdf body copy duplicated '+marker.text);}
  const target=locations.find(item=>item.text==='LINK30'),annotations=links.pages[target.page].links.filter(link=>Array.isArray(link.from)&&overlap(link.from,target.bounds));assert(annotations.some(link=>link.uri===LINK),'Actual link hit region or destination was lost');
  assertOperatorSizes(bodyOperators,COMMON_MARKERS.filter(item=>item.text!=='中文字形'));
  assert.equal(value.leakedCitationSlot,false);return {markers:locations,pointTolerance:SIZE_TOLERANCE_PT,sourceOrder:true,linkAnnotations:annotations,independentParsers:value.independentParsers,pdfOperatorsObserved:true,epubReader,...(epubScope?{epubScope}:{}),physicalSizesObserved:true};
}
function assertScriptPdf(value,script,operators,{outerSize=11}={}){
  const characters=checkGlyphs(value),text=normalized(characters.map(c=>c.c).join('')),expected=normalized(script.text);assert(text.includes(expected),'Actual '+script.id+' logical text missing or reordered');assert(normalized(value.pypdfText).includes(expected),'pypdf lost actual '+script.id+' text');
  const relevant=characters.filter(char=>[...script.text].some(c=>char.c.includes(c)));assert(relevant.length>=new Set([...script.text]).size,'Actual script character bounds missing');for(const char of relevant)near(char.size,10,'Actual complex-axis glyph size '+script.id);
  if(script.rtl)assert(relevant.every((char,index)=>index===0||char.origin[0]<=relevant[index-1].origin[0]+.1),'Actual RTL letter positions run left to right');
  const matched=locate(characters,'SCRIPTBEGIN');const end=locate(characters,'SCRIPTEND');assert(matched.from<end.from);assert(text.indexOf(expected)>text.indexOf('SCRIPTBEGIN')&&text.indexOf(expected)<text.indexOf('SCRIPTEND'),'Surrounding Latin/script order changed');
  assertOperatorSizes(operators,[{text:'SCRIPTBEGIN',size:outerSize},{text:'SCRIPTEND',size:outerSize}]);
  return {script:script.id,sourceText:script.text,logicalTextObserved:true,noNotdef:true,complexAxisPt:10,actualFonts:[...new Set(relevant.map(c=>c.font))],naturalBidi:true,explicitWordRtlCertified:false};
}
const DOCX_EXPECTED={BODY30:[60,20],SMALL10:[20,20],CSLATIN10:[60,20],LINK30:[60,20],LOCALRESET175:[35,35],CODE105:[21,22],CELLRESET105:[21,21],HEADRESET23:[46,20],NOTERESET10:[20,20],'中文字形':[40,20]};
function assertDocx(value,effective){
  assert.equal(value.kind,'docx');assert(Array.isArray(value.runs));assert.equal(effective.kind,'author-inline-docx');assert.equal(effective.sourcePayloadUsedAsNativeEvidence,false);const details=[];let linkRun;
  for(const [marker,[western,complex]]of Object.entries(DOCX_EXPECTED)){const declared=value.runs.filter(run=>run.text.includes(marker));assert.equal(declared.length,1,'Actual native run missing/ambiguous '+marker);const properties=declared[0].properties;assert.equal(Number(properties.sz?.val),western,'Native sz '+marker);if(properties.szCs)assert.equal(Number(properties.szCs.val),complex,'Declared native szCs '+marker);
    const rows=effective.paragraphs.filter(row=>row.text.includes(marker));assert.equal(rows.length,1,'Effective native paragraph missing/ambiguous '+marker);const row=rows[0],from=row.text.indexOf(marker);assert.equal(row.text.indexOf(marker,from+1),-1);const runs=row.runs.filter(run=>run.from<from+marker.length&&from<run.to);assert(runs.length);assert.equal(runs.map(run=>run.text.slice(Math.max(0,from-run.from),Math.min(run.text.length,from+marker.length-run.from))).join(''),marker);
    for(const run of runs){assert.equal(run.halfPoints,western,'Effective native sz '+marker);assert.equal(run.halfPointsCs,complex,'Effective native szCs '+marker);if(marker==='CSLATIN10')assert.equal(run.cs,true,'Forced-CS Latin native flag missing');else assert.notEqual(run.cs,true,'Unexpected native forced-CS route '+marker);assert.notEqual(run.rtl,true,'Unexpected native RTL route '+marker);if(marker==='LINK30'){assert(!linkRun);linkRun=run;}}
    details.push({marker,part:declared[0].part,size:western/2,sizeCs:complex/2});
  }
  assert(value.links.some(link=>link.Target===LINK),'Native hyperlink relationship lost');assert(linkRun?.hyperlink?.relationship);const links=effective.hyperlinks.filter(link=>link.part===linkRun.hyperlink.part&&link.Id===linkRun.hyperlink.relationship);assert.equal(links.length,1);assert.equal(links[0].Target,LINK);assert.equal(links[0].TargetMode,'External');assert(!value.nestedNoteReferences?.length);return {nativeAxes:details,independentEffectiveCascade:true,forcedCsLatin:true,nativeLink:true,wordApplicationCertified:false};
}
function assertEpubPackage(value){assert.equal(value.kind,'epub');assert(value.content.some(item=>item.name.endsWith('.xhtml')));assert(!value.leakedCitationSlot);const xml=value.content.map(item=>item.xml).join('\n');assert(!xml.includes('data-readmd-text='),'Source marker leaked into actual XHTML');assert(/font-size:\s*30pt/u.test(xml)&&/font-size:\s*10pt/u.test(xml),'Actual EPUB lacks numeric author CSS');assert(xml.includes(LINK),'Actual EPUB hyperlink missing');return {nativeXhtml:true,numericPointCss:true,sourceMarkerExecuted:false,readerRenderingRequired:true};}
function errorValue(result,code){assert.equal(result.isError,true,'Expected typed author guard '+code);const text=JSON.parse(result.content.find(item=>item.type==='text').text),value=result.structuredContent??text;assert.deepEqual(value,text,'MCP structured/text errors differ');assert.equal(value.ok,false);assert.equal(value.error_code,code);if(Object.hasOwn(value,'error'))assert.equal(value.error,code);return value;}
function assertTexRtlDetails(value){const details=value.details;assert(details&&typeof details==='object'&&!Array.isArray(details),'TeX author guard must retain its validated object');assert.deepEqual(Object.keys(details).sort(),['consumer','coordinateSpace','from','reason','to']);assert.equal(details.consumer,'tex');assert.equal(details.reason,'rtl');assert.equal(details.coordinateSpace,'textUtf8Bytes');assert(Number.isSafeInteger(details.from)&&Number.isSafeInteger(details.to)&&details.from>=0&&details.from<details.to&&details.to<=32*1024*1024);return details;}
function guardRequest(format,source,file,documents,label,engine,overwrite){return exportRequestFor(format,{markdown_content:source,output_path:file,base_dir:documents,title:label,confirm:true,...(overwrite?{overwrite:true}:{}),options:optionsFor(engine)});}
const PYTHON_READER=String.raw`
import sys,json,hashlib,math
from pathlib import Path
sys.stdout.reconfigure(encoding='utf-8')
mode=sys.argv[1]
source=Path(sys.argv[2]); assert source.is_file() and source.stat().st_size<=32*1024*1024
if mode=='epub':
 import pymupdf as fitz
 target=Path(sys.argv[3]); assert not target.exists() and target.parent.is_dir()
 assert len(sys.argv[4].encode('utf-8'))<=2*1024*1024
 expected=json.loads(sys.argv[4]); assert isinstance(expected,dict) and set(expected)=={'sourceSha256','spine'}
 assert expected['sourceSha256']==hashlib.sha256(source.read_bytes()).hexdigest()
 fitz.TOOLS.mupdf_warnings(reset=True)
 document=fitz.open(source); assert document.is_reflowable
 document.layout(width=595,height=842,fontsize=11)
 assert 0<len(document)<=32
 assert isinstance(expected['spine'],list) and 0<len(expected['spine'])==document.chapter_count<=32
 chapters=[]; covered=[]
 for chapter,row in enumerate(expected['spine']):
  assert isinstance(row,dict) and set(row)=={'idref','part','linear','anchors'}
  assert all(isinstance(row[key],str) and row[key] for key in ('idref','part')) and type(row['linear']) is bool
  assert isinstance(row['anchors'],list) and 0<len(row['anchors'])<=8192 and all(isinstance(identity,str) and identity for identity in row['anchors'])
  count=document.chapter_page_count(chapter); assert type(count) is int and 0<count<=32
  indices=[]; locations=[]
  for page_within in range(count):
   physical=document.page_number_from_location((chapter,page_within)); assert type(physical) is int and physical==len(covered)
   assert document.location_from_page_number(physical)==(chapter,page_within)
   indices.append(physical); covered.append(physical); locations.append({'page':physical,'chapter':chapter,'pageWithinChapter':page_within})
  part_location,_,_=document.resolve_link(row['part'],chapters=True); assert part_location==(chapter,0)
  anchors=[]
  for identity in row['anchors']:
   location,x,y=document.resolve_link(row['part']+'#'+identity,chapters=True)
   assert location[0]==chapter and type(location[1]) is int and 0<=location[1]<count and all(math.isfinite(float(n)) for n in (x,y))
   anchors.append({'id':identity,'chapter':chapter,'pageWithinChapter':location[1],'page':indices[location[1]],'to':[float(x),float(y)]})
  chapters.append({key:row[key] for key in ('idref','part','linear')}|{'chapter':chapter,'pageCount':count,'pages':indices,'locations':locations,'partLocation':{'chapter':chapter,'pageWithinChapter':0,'page':indices[0]},'anchors':anchors})
 assert covered==list(range(len(document)))
 mapping={'coordinateSpace':'pdfPageIndex','chapterCount':document.chapter_count,'physicalPageCount':len(document),'spine':chapters}
 original_links=[]
 for physical in range(len(document)):
  links=[]
  for link in document.load_page(physical).get_links():
   assert len(links)<8192
   links.append({key:list(value) if isinstance(value,(fitz.Point,fitz.Rect)) else value for key,value in link.items()})
  original_links.append({'page':physical,'location':list(document.location_from_page_number(physical)),'links':links})
 data=document.convert_to_pdf(); document.close()
 assert 0<len(data)<=32*1024*1024
 with target.open('xb') as output: output.write(data)
 print(json.dumps({'reader':'MuPDF EPUB','version':fitz.VersionBind,'sourceSha256':hashlib.sha256(source.read_bytes()).hexdigest(),'outputSha256':hashlib.sha256(data).hexdigest(),'chapterMapping':mapping,'originalEpubPageLinks':original_links,'warnings':fitz.TOOLS.mupdf_warnings(reset=True),'javascriptExecuted':False,'hostFontsAdded':False},ensure_ascii=False,allow_nan=False))
elif mode=='operators':
 import pypdf
 reader=pypdf.PdfReader(str(source),strict=True); characters=[]; pages=[]
 for page_index,page in enumerate(reader.pages):
  def visit(text,cm,tm,font,size):
   assert isinstance(text,str) and len(text)<=1024*1024
   if not text:return
   assert len(cm)==len(tm)==6 and all(math.isfinite(float(n)) for n in cm+tm)
   physical=float(size)*math.hypot(float(tm[2])*float(cm[0])+float(tm[3])*float(cm[2]),float(tm[2])*float(cm[1])+float(tm[3])*float(cm[3]))
   assert math.isfinite(physical) and physical>0
   for c in text:characters.append({'c':c,'size':physical,'page':page_index,'font':str((font or {}).get('/BaseFont',''))})
  text=page.extract_text(visitor_text=visit) or ''; assert isinstance(text,str) and len(text)<=1024*1024
  pages.append({'page':page_index,'text':text})
 assert len(characters)<=2*1024*1024
 print(json.dumps({'reader':'pypdf content visitor','version':pypdf.__version__,'sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'characters':characters,'pages':pages},ensure_ascii=False,allow_nan=False))
else:raise AssertionError('Unknown owned reader operation')
`;
function planFor(){return {schemaVersion:1,scope:'P06-B actual author-size export consumers',transport:'real-stdio',syntheticMaterials:true,noProcessesStarted:true,noFilesWritten:true,guiStarted:false,profileReuse:'fixed reviewed publication-engines-native-n2e7Ug; no runtime installation or copying',formats:['docx','pdf/rust','pdf/typst','pdf/xelatex','epub'],commonSourceSha256:sha(commonSource()),reflowableSourceSha256:sha(commonSource({reflowable:true})),reflowableSourceDifference:'Only physical exact-line-spacing declaration omitted; every author-size marker retained. FINAL11 explicitly resets author size to block base in both sources.',commonMarkers:COMMON_MARKERS,scriptCases:SCRIPTS.map(script=>({...script,sourceSha256:sha(scriptSource(script)),formats:['pdf/typst','epub'],missingGlyphAllowed:false})),actualPositiveProductOutputs:13,actualEpubReaderPdfOutputs:5,guards:{rustPdfNaturalShaping:4,texNaturalRtl:2,explicitWordRtl:4,total:10,oldTargetsMustBeUnchanged:true,missingAndExplicitOverwriteChecked:true,countsAsPositiveImplementation:false},expectedConsumerChecks:23,readers:['Python stdlib XML/ZIP and independent QName-aware effective DOCX style cascade','PyMuPDF actual raw character bounds/glyph IDs','pypdf content stream text and physical font matrices','MuPDF EPUB reflow and real reader PDF'],epubBodyScope:{packageEvidence:'Actual container/OPF manifest, spine, nav role, current unique XHTML identities and real navigation targets from AUTHOR_INSPECTOR',physicalMapping:'MuPDF chapter counts, part URI resolution and exact forward/inverse chapter-page locations must match every actual spine item and PDF page',navigationExclusion:'Only verified navigation-only spine documents; mixed navigation/body documents fail until exact regions can be proved',bodyAssertions:'Unique complete markers, source order, physical points, independent operator points, per-page pypdf copy and actual external link',allPagesStillRequire:'No missing GID/CID, replacement/tofu text or invalid bounds; four known script-copy failures remain strict failures'},ligatureContinuationPolicy:{source:'https://pymupdf.readthedocs.io/en/latest/functions.html#Page.get_texttrace',acceptedGid:-1,requires:'preceding real glyph, shared origin, zero-width bbox; GID 0 always fails'},tolerancePt:SIZE_TOLERANCE_PT,qaFiles:QA_FILES,productFiles:PRODUCT_FILES,limitations:['MuPDF EPUB is one actual independent reader; this is not browser-wide EPUB certification.','Natural RTL engine output and explicit native Word rtl are separate contracts.','No real Word/WPS/LibreOffice application editing, GUI, cold performance or full third batch certification.','Missing fonts/glyphs and the known external script-copy gaps remain failed results, never accepted fallback claims.']};}

const fullPlan=()=>({...planFor(),scope:'P06-B actual inline/caption author-size and P06-C uniform section consumers',captionAuthor:captionQa.planFor(),legacyPositiveProductOutputs:13,positiveExportAttempts:47,actualPositiveProductOutputs:undefined,expectedConsumerChecks:105,actualEpubReaderPdfOutputs:17,ownedHostBudgets:{...HOST_BUDGETS,toolRequestMs:TOOL_TIMEOUT_MS,oracleProcessMs:60000,maximumOwnedHostMs:HOST_BUDGETS.first+HOST_BUDGETS.reopened,reason:'First host has 79 checks and 28 positive export attempts; reopened host has 26 checks and 19 export attempts. Earlier 23-check actual runs took 174-207 seconds. Each host has a fresh bounded deadline; individual product requests remain bounded at 120 seconds and reader processes at 60 seconds.'},formats:['docx','html','epub','tex','pdf/rust','pdf/typst','pdf/xelatex']});
function epubLinkEvidence(native,reader,converted){
  assert.equal(native.sha256,reader.sourceSha256);assert.equal(converted.sha256,reader.outputSha256);assert.equal(reader.chapterMapping.coordinateSpace,'pdfPageIndex');assert(Array.isArray(reader.originalEpubPageLinks));
  const currentXhtmlLinks=native.links.filter(link=>link.contentRole==='body'&&!link.navigationType&&link.href.includes('#')&&!/^[a-z][a-z0-9+.-]*:/iu.test(link.href)).map(link=>{
    const [file,fragment]=link.href.split('#'),part=file?path.posix.normalize(path.posix.join(path.posix.dirname(link.part),file)):link.part,id=decodeURIComponent(fragment),targets=native.identities.filter(row=>row.part===part&&row.id===id);assert.equal(targets.length,1,'Current EPUB link target is missing/duplicated');
    const chapters=reader.chapterMapping.spine.filter(row=>row.part===part);assert.equal(chapters.length,1);const anchors=chapters[0].anchors.filter(row=>row.id===id);assert.equal(anchors.length,1);const target=anchors[0];assert(Array.isArray(target.to)&&target.to.length===2&&target.to.every(Number.isFinite));
    return {...link,currentTargetId:id,currentTargetText:targets[0].text,resolvedSource:{uri:part+'#'+id,chapter:target.chapter,pageWithinChapter:target.pageWithinChapter,physicalPage:target.page,to:target.to}};
  });
  return {version:reader.version,sourceSha256:reader.sourceSha256,outputSha256:reader.outputSha256,coordinateSpace:'pdfPageIndex',sourceFilesMutated:false,currentXhtmlLinks,originalEpubPageLinks:reader.originalEpubPageLinks,convertedPdfLinks:converted.pages.flatMap((row,sourcePage)=>row.links.filter(link=>link.kind===1).map(link=>({sourcePage,kind:link.kind,page:link.page,from:link.from,to:link.to})))};
}
function selftest(){
  const tests=[],check=(name,fn)=>{try{fn();tests.push({name,passed:true});}catch(error){tests.push({name,passed:false,error:String(error.stack||error)});}};
  check('source fixture keeps readable independent axes and reset markers',()=>{const source=commonSource();for(const marker of COMMON_MARKERS)assert(source.includes(marker.text));assert(source.includes('data-size-cs-pt="10"')&&source.includes('data-cs="true"')&&source.includes('data-size-pt="base"'));assert(!source.includes('data-rtl='));assert.equal(SCRIPTS.length,4);});
  check('FINAL11 is an explicit author base reset without relaxing physical pt tolerance',()=>{assert(commonSource().includes('NOTE_CALL[^author-note] '+span({size:'base'},'FINAL11')));assert.equal(SIZE_TOLERANCE_PT,.025);assert.throws(()=>near(11*72/72.27,11,'old unmarked TeX point control'));});
  check('flow fixture removes only incompatible exact spacing and preserves every author-size marker',()=>{const paged=commonSource(),flow=commonSource({reflowable:true}),declaration='<!-- readmd:layout v1 {"id":"author-metric","paragraph":{"lineSpacing":{"mode":"exact","pt":60}}} -->\n\n';assert(paged.includes(declaration));assert.equal(flow,paged.replace(declaration,''));assert(!flow.includes('lineSpacing'));for(const marker of COMMON_MARKERS)assert(flow.includes(marker.text));assert.equal(planFor().reflowableSourceSha256,sha(flow));});
  const native={kind:'docx',runs:Object.entries(DOCX_EXPECTED).map(([text,[west,cs]])=>({part:'word/document.xml',text,properties:{sz:{val:String(west)},...(text==='CODE105'?{}:{szCs:{val:String(cs)}})}})),links:[{Target:LINK}],nestedNoteReferences:[]},effective={kind:'author-inline-docx',sourcePayloadUsedAsNativeEvidence:false,paragraphs:Object.entries(DOCX_EXPECTED).map(([text,[west,cs]])=>({text,runs:[{text,from:0,to:text.length,halfPoints:west,halfPointsCs:cs,cs:text==='CSLATIN10'?true:null,rtl:null,...(text==='LINK30'?{hyperlink:{part:'word/document.xml',relationship:'actual-link'}}:{})}]})),hyperlinks:[{part:'word/document.xml',Id:'actual-link',Target:LINK,TargetMode:'External'}]};
  check('actual runs schema and independent inherited complex axis remain distinct',()=>assertDocx(native,effective));
  check('native oracle rejects wrong inherited CS, unexpected route and spare hyperlink',()=>{const wrong=structuredClone(effective);wrong.paragraphs.find(row=>row.text==='CODE105').runs[0].halfPointsCs=21;assert.throws(()=>assertDocx(native,wrong));const route=structuredClone(effective);route.paragraphs.find(row=>row.text==='BODY30').runs[0].cs=true;assert.throws(()=>assertDocx(native,route));const link=structuredClone(effective);link.hyperlinks[0].Id='spare-link';assert.throws(()=>assertDocx(native,link));assert.throws(()=>assertDocx({...native,runs:undefined,formatting:native.runs},effective));});
  check('guard requests preserve normal overwrite protection and explicitly reach typed preflight',()=>{for(const format of ['docx','epub','pdf']){const missing=guardRequest(format,'owned','file','documents','label','rust',false),confirmed=guardRequest(format,'owned','file','documents','label','rust',true);assert(!Object.hasOwn(missing.arguments,'overwrite'));assert.equal(confirmed.arguments.overwrite,true);assert.deepEqual({...confirmed.arguments,overwrite:undefined},{...missing.arguments,overwrite:undefined});}const body={ok:false,error_code:'output_exists'},result={isError:true,structuredContent:body,content:[{type:'text',text:JSON.stringify(body)}]};errorValue(result,'output_exists');assert.throws(()=>errorValue(result,'author_inline_text_unsupported'));});
  check('CLI plan and selftest need no candidate or runtime',()=>{assert(argumentsFor(['--plan'],{}).plan);assert(argumentsFor(['--selftest'],{}).selftest);assert.throws(()=>argumentsFor([],{}));assert.throws(()=>argumentsFor(['--unknown'],{}));});
  const chars=COMMON_MARKERS.flatMap((marker,index)=>[...marker.text].map((c,offset)=>({c,size:marker.size,page:0,origin:[10+offset*8,20+index*40],bbox:[10+offset*8,10+index*40,17+offset*8,23+index*40],font:'Synthetic negative control'})));
  const value={kind:'pdf',independentParsers:{pypdf:'synthetic',pymupdf:'synthetic'},text:COMMON_MARKERS.map(v=>v.text).join(' '),pypdfText:COMMON_MARKERS.map(v=>v.text).join(' '),leakedCitationSlot:false,pages:[{width:600,height:1200,characters:chars,glyphTraces:[{chars:chars.map(c=>({unicode:c.c.codePointAt(0),glyphId:c.c.codePointAt(0),bbox:c.bbox,origin:c.origin})),size:11,font:'Synthetic'}]}]};
  const link=locate(chars,'LINK30').characters,links={pages:[{links:[{uri:LINK,from:union(link)}]}]},operators={reader:'pypdf content visitor',version:'synthetic',characters:chars};
  check('positive oracle-only physical fixture accepts exact bound sizes',()=>assertCommonPdf(value,links,operators));
  for(const [name,mutate]of [
    ['old TeX pt instead of bp',v=>{for(const char of locate(v.pages[0].characters,'BODY30').characters)char.size=30*72/72.27;}],
    ['PDF retarget erased explicit author size',v=>{for(const char of locate(v.pages[0].characters,'BODY30').characters)char.size=11;}],
    ['complex axis was copied from Western size',v=>{for(const char of locate(v.pages[0].characters,'CSLATIN10').characters)char.size=30;}],
    ['one glyph missing',v=>{v.pages[0].glyphTraces[0].chars[4].glyphId=0;}],
    ['character escaped actual page',v=>{v.pages[0].characters[4].bbox[0]=-20;}],
    ['pypdf source text missing',v=>{v.pypdfText=v.pypdfText.replace('BODY30','');}],
    ['actual source order changed',v=>{const all=v.pages[0].characters,first=locate(all,'BODY30').characters,second=locate(all,'SMALL10').characters,a=all.indexOf(first[0]),b=all.indexOf(second[0]);v.pages[0].characters=[...all.slice(0,a),...second,...all.slice(a+first.length,b),...first,...all.slice(b+second.length)];}],
  ])check('reject '+name,()=>{const changed=structuredClone(value);mutate(changed);assert.throws(()=>assertCommonPdf(changed,links,operators));});
  check('reject actual link destination/hit-region regression',()=>assert.throws(()=>assertCommonPdf(value,{pages:[{links:[{uri:'https://wrong.test/',from:union(link)}]}]},operators)));
  check('independent text operators reject a font-size regression',()=>{const changed=structuredClone(operators);for(const char of locate(changed.characters,'BODY30').characters)char.size=11;assert.throws(()=>assertOperatorSizes(changed,COMMON_MARKERS));});
  check('documented ligature continuation keeps the actual bound glyph',()=>{checkTrace({chars:[{unicode:102,glyphId:80,origin:[10,20],bbox:[10,10,20,22]},{unicode:105,glyphId:-1,origin:[10,20],bbox:[10,10,10,22]}]});});
  check('unbound, nonzero-width and .notdef ligature controls fail',()=>{for(const tail of [{unicode:105,glyphId:-1,origin:[11,20],bbox:[10,10,10,22]},{unicode:105,glyphId:-1,origin:[10,20],bbox:[10,10,12,22]},{unicode:105,glyphId:0,origin:[10,20],bbox:[10,10,10,22]}])assert.throws(()=>checkTrace({chars:[{unicode:102,glyphId:80,origin:[10,20],bbox:[10,10,20,22]},tail]}));assert.throws(()=>checkTrace({chars:[{unicode:105,glyphId:-1,origin:[10,20],bbox:[10,10,10,22]}]}));});
  check('bound product and QA file names exist before candidate execution',()=>{for(const file of [...QA_FILES,...PRODUCT_FILES])assert(fs.statSync(path.join(ROOT,file)).isFile(),file);});
  check('fixture marker identities never depend on substring ambiguity',()=>{for(const marker of COMMON_MARKERS)for(const other of COMMON_MARKERS)if(marker!==other)assert(!other.text.includes(marker.text));});
  for(const script of SCRIPTS){
    const text='SCRIPTBEGIN '+script.text+' SCRIPTEND',letters=[...text],first=text.indexOf(script.text),last=first+script.text.length;
    const characters=letters.map((c,index)=>({c,size:index>=first&&index<last?10:11,font:'Synthetic script oracle control',origin:[index>=first&&index<last&&script.rtl?300-(index-first)*12:20+index*10,50],bbox:[20+index*10,30,27+index*10,60]}));
    const actual={kind:'pdf',independentParsers:{pypdf:'synthetic',pymupdf:'synthetic'},text,pypdfText:text,pages:[{width:700,height:200,characters,glyphTraces:[{font:'Synthetic',size:11,chars:characters.map(c=>({unicode:c.c.codePointAt(0),glyphId:c.c.codePointAt(0),origin:c.origin,bbox:c.bbox}))}]}]},operator={reader:'pypdf content visitor',characters};
    check('oracle-only '+script.id+' exact text and complex-axis fixture',()=>assertScriptPdf(actual,script,operator));
    check('reject '+script.id+' wrong complex-axis glyph size',()=>{const wrong=structuredClone(actual);wrong.pages[0].characters[first].size=30;assert.throws(()=>assertScriptPdf(wrong,script,operator));});
    if(script.rtl)check('reject '+script.id+' reversed actual glyph direction',()=>{const wrong=structuredClone(actual);for(let i=first;i<last;i++)wrong.pages[0].characters[i].origin[0]=20+i*10;assert.throws(()=>assertScriptPdf(wrong,script,operator));});
  }
  check('typed guard envelope must agree in both MCP channels',()=>{const body={ok:false,error:'author_inline_text_unsupported',error_code:'author_inline_text_unsupported'};errorValue({isError:true,structuredContent:body,content:[{type:'text',text:JSON.stringify(body)}]},body.error_code);assert.throws(()=>errorValue({isError:true,structuredContent:body,content:[{type:'text',text:'{}'}]},body.error_code));});
  return {schemaVersion:1,scope:'QA oracle-only controls; no product, host or engine execution',passed:tests.every(test=>test.passed),tests,count:tests.length};
}

async function main(){
  const options=argumentsFor(process.argv.slice(2));if(options.help){console.log('node tools/test/batch3-author-inline-engines-stdio.cjs --plan | --selftest\n--candidate JSON --profile REVIEWED_PROFILE --python EXISTING_QA_PYTHON [--output OWNED_DIR]');return;}
  if(options.plan){console.log(JSON.stringify(fullPlan(),null,2));return;}
  if(options.selftest){fs.mkdirSync(BASE,{recursive:true});const dir=fs.mkdtempSync(path.join(BASE,'selftest-')),result=selftest();fs.writeFileSync(path.join(dir,'source.cjs'),fs.readFileSync(__filename),{flag:'wx'});fs.writeFileSync(path.join(dir,'report.json'),JSON.stringify(result,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify({evidence:path.relative(ROOT,dir),...result},null,2));if(!result.passed)process.exitCode=1;return;}
  assert.equal(process.platform,'win32');const binding=bindingFor(options);assert(fs.statSync(options.python).isFile());fs.mkdirSync(BASE,{recursive:true});const evidence=options.output?(fs.mkdirSync(options.output),options.output):fs.mkdtempSync(path.join(BASE,'stdio-'));
  for(const name of ['documents','outputs','requests','responses','inspection','wire','wire-reopened','reader-output'])fs.mkdirSync(path.join(evidence,name));
  const retain=(name,value)=>{const target=inside(evidence,path.join(evidence,name));fs.writeFileSync(target,typeof value==='string'||Buffer.isBuffer(value)?value:JSON.stringify(value,null,2)+'\n',{flag:'wx'});return target;};
  const report={...fullPlan(),noProcessesStarted:undefined,noFilesWritten:undefined,passed:false,startedUtc:new Date().toISOString(),candidateBinding:binding,checks:[],outputs:[],readerOutputs:[],sessions:[],hostBudgets:[],scriptAvailability:[],errors:[],runtimeCopied:false,python:{path:options.python,sha256:fileHash(options.python),qaOnly:true}};
  let client,timer,reuse,assets=binding.assets,profile=options.profile,serial=0,readerSerial=0;
  const stage=value=>{assert(!report.workflowTimedOut);report.stage=value;console.log(JSON.stringify({stage:value}));};
  const call=async(name,args,label)=>{verifyBinding(binding);assertToolCallSchema({name,arguments:args},client.tools);const key=String(++serial).padStart(3,'0')+'-'+label;retain('requests/'+key+'.json',{name,arguments:args});const result=await client.tool(name,args,TOOL_TIMEOUT_MS);retain('responses/'+key+'.json',result);verifyBinding(binding);return result;};
  const hostBudget=name=>{clearTimeout(timer);const budgetMs=HOST_BUDGETS[name],started=Date.now(),row={name,pid:client.info.pid,budgetMs,toolRequestMs:TOOL_TIMEOUT_MS,startedUtc:new Date(started).toISOString(),deadlineUtc:new Date(started+budgetMs).toISOString(),timedOut:false};report.hostBudgets.push(row);timer=setTimeout(()=>{row.timedOut=true;row.timeoutUtc=new Date().toISOString();report.workflowTimedOut=true;client?.transport.fail(Error('Owned '+name+' host workflow deadline'),'workflow_deadline');client?.forceStop();},budgetMs);return row;};
  const python=(label,args)=>{verifyBinding(binding);const key=String(++readerSerial).padStart(3,'0')+'-'+label,result=spawnSync(options.python,['-I','-B',...args],{windowsHide:true,timeout:60000,maxBuffer:48*1024*1024,env:systemEnvironment(profile,path.join(evidence,'documents'))});retain('inspection/'+key+'.stdout.json',result.stdout||Buffer.alloc(0));retain('inspection/'+key+'.stderr.log',result.stderr||Buffer.alloc(0));assert(!result.error,result.error?.message);assert.equal(result.status,0,'Independent actual-file reader failed '+label);verifyBinding(binding);return JSON.parse(result.stdout);};
  const inspect=(file,label)=>python(label,[path.join(ROOT,INSPECTOR),file,...(file.endsWith('.pdf')?['--glyphs','--render',path.join(evidence,'inspection',label+'-pages')]:[])]);
  const pdfData=(file,label)=>{const value=inspect(file,label),links=python(label+'-links',[path.join(ROOT,LINK_INSPECTOR),file,'--character-bounds']),operators=python(label+'-operators',['-c',PYTHON_READER,'operators',file]);for(const actual of [value,links,operators])assert.equal(actual.sha256,fileHash(file),'Reader inspected different bytes');return {value,links,operators};};
  const epubData=(file,label)=>{const native=inspect(file,label+'-package'),epubNative=python(label+'-roles',[path.join(ROOT,AUTHOR_INSPECTOR),'inspect',file]);assert.equal(epubNative.sha256,fileHash(file));const chapterInput=epubChapterInput(epubNative),output=path.join(evidence,'reader-output',label+'.pdf'),reader=python(label+'-render',['-c',PYTHON_READER,'epub',file,output,JSON.stringify(chapterInput)]);assert.equal(reader.sourceSha256,fileHash(file));assert.equal(reader.outputSha256,fileHash(output));report.readerOutputs.push({label,path:path.relative(evidence,output),sha256:fileHash(output),reader});const actual=pdfData(output,label+'-reader');if(['caption-first-epub','caption-common-reopened-epub','uniform-section-continuous-reopened-epub','uniform-section-section-reopened-epub'].includes(label)){const raw=epubLinkEvidence(epubNative,reader,actual.links),proof=retain('inspection/'+label+'-epub-links-current.json',raw),exported=report.outputs.find(row=>row.label===label);assert(exported?.sourceMarkdownSha256);(report.epubReaderLinkEvidence??=[]).push({label,sourceSha256:reader.sourceSha256,sourceMarkdownSha256:exported.sourceMarkdownSha256,convertedPdfSha256:reader.outputSha256,evidence:{path:path.relative(evidence,proof),sha256:fileHash(proof)}});}return {native,epubNative,reader,...actual};};
  const exportActual=async(label,format,source,engine='rust',outputOverride,requestOptions)=>{stage('real '+label+' '+format);const output=outputOverride||path.join(evidence,'outputs',label+'.'+format),request=exportRequestFor(format,{markdown_content:source,output_path:output,base_dir:path.join(evidence,'documents'),title:label,confirm:true,options:requestOptions===undefined?optionsFor(engine):requestOptions}),result=await call(request.name,request.arguments,'export-'+label),value=payload(result);assert(fs.statSync(output).size>0);assert(Array.isArray(value.warnings));assert(!/glyph_missing|missing character:|missing[^\n]{0,40}glyph|glyph[^\n]{0,40}missing/iu.test(JSON.stringify({warnings:value.warnings,warn_items:value.warn_items})),'Engine reported missing glyphs');report.outputs.push({label,format,engine:format==='pdf'?engine:undefined,path:path.relative(evidence,output),sha256:fileHash(output),sourceMarkdownSha256:sha(source),bytes:fs.statSync(output).size,warnings:value.warnings,warn_items:value.warn_items,...(requestOptions===undefined?{}:{explicitOptions:requestOptions})});return output;};
  const runCheck=async(name,fn)=>{try{const details=await fn();report.checks.push({name,passed:true,...details});return true;}catch(error){const record={name,passed:false,error:String(error.stack||error)};report.checks.push(record);report.errors.push(record);report.firstFailure??=record;return false;}};
  const outputSnapshot=()=>{const rows=[];const walk=folder=>{for(const name of fs.readdirSync(folder).sort()){const file=path.join(folder,name);if(fs.statSync(file).isDirectory())walk(file);else rows.push([path.relative(evidence,file),fileHash(file)]);}};walk(path.join(evidence,'outputs'));return rows;};
  const guard=async(label,source,format,engine,code,{texRtlDetails=true}={})=>runCheck(label,async()=>{const file=path.join(evidence,'outputs',label+'.'+format),old=Buffer.from('QA_OWNED_OLD_TARGET_'+label);retain('outputs/'+path.basename(file),old);const before=outputSnapshot(),missing=guardRequest(format,source,file,path.join(evidence,'documents'),label,engine,false);errorValue(await call(missing.name,missing.arguments,label+'-missing-overwrite'),'output_exists');assert.deepEqual(fs.readFileSync(file),old);assert.deepEqual(outputSnapshot(),before);const request=guardRequest(format,source,file,path.join(evidence,'documents'),label,engine,true),value=errorValue(await call(request.name,request.arguments,label+'-overwrite'),code);if(texRtlDetails&&engine==='xelatex'&&code==='author_inline_text_unsupported')assertTexRtlDetails(value);assert.deepEqual(fs.readFileSync(file),old,'Failed author export replaced existing target');assert.deepEqual(outputSnapshot(),before,'Failed author export changed the existing output directory');return {code,error_details:value.error_details??value.details,oldSha256:sha(old),oldTargetPreserved:true,outputDirectoryPreserved:true,missingOverwriteProtected:true,explicitOverwriteProtected:true,countsAsPositiveImplementation:false};});
  try{
    retain('plan.json',fullPlan());stage('reviewed offline runtime reuse');const helpers=require('../../ui-tests/publication-engines-native.cjs'),reviewed=helpers.resolveEngineReuseProfile(profile);let runtimeSerial=0;
    const harness={root:ROOT,data:evidence,report,command(binary,args){const result=spawnSync(binary,args,{windowsHide:true,timeout:60000,maxBuffer:24*1024*1024}),key='runtime-'+String(++runtimeSerial).padStart(2,'0');retain('inspection/'+key+'.stdout.log',result.stdout||Buffer.alloc(0));retain('inspection/'+key+'.stderr.log',result.stderr||Buffer.alloc(0));assert(!result.error);assert.equal(result.status,0,'Reviewed runtime preflight failed');return result.stdout.toString('utf8');}};
    for(const [key,name]of [['READMD_FONTS_PACK','readmd-fonts-2026.10.10-universal.readmd-pack'],['READMD_PDFIUM_PACK','readmd-pdfium-7881-windows-x64.readmd-pack'],['READMD_TYPST_PACK','readmd-typst-0.15.1-windows-x64.readmd-pack']])process.env[key]??=path.join(ROOT,'.cache/capability-upgrade/locked-packs',name);
    reuse=helpers.prepareEngineProfileReuse(harness,reviewed,assets);assets=reuse.assets;profile=reviewed;const presets=path.join(profile,'export_presets.json'),presetHash=fs.existsSync(presets)?fileHash(presets):null;
    client=new StdioClient({binary:binding.binary,assets,profile,documents:path.join(evidence,'documents'),out:path.join(evidence,'wire')});hostBudget('first');await client.initialize(true);retain('tools-list.json',client.tools);
    const source=commonSource(),sourcePath=retain('documents/common.md',source),sourceHash=fileHash(sourcePath);
    await runCheck('actual DOCX independent native author-axis control',async()=>{const file=await exportActual('common-native','docx',source);zipParts(fs.readFileSync(file));const native=inspect(file,'common-native'),effective=python('common-native-effective',[path.join(ROOT,AUTHOR_INSPECTOR),'inspect',file]);assert.equal(effective.sha256,fileHash(file));return assertDocx(native,effective);});
    for(const engine of ['rust','typst','xelatex'])await runCheck('actual common '+engine+' physical PDF',async()=>{const file=await exportActual('common-'+engine,'pdf',source,engine),actual=pdfData(file,'common-'+engine);return assertCommonPdf(actual.value,actual.links,actual.operators);});
    const flowSource=commonSource({reflowable:true}),flowPath=retain('documents/common-epub.md',flowSource),flowHash=fileHash(flowPath);await runCheck('actual common EPUB and independent real reader glyphs',async()=>{const file=await exportActual('common-epub','epub',flowSource),actual=epubData(file,'common-epub');assertEpubPackage(actual.native);return assertCommonPdf(actual.value,actual.links,actual.operators,{epubReader:true,epubNative:actual.epubNative,epubReflow:actual.reader});});assert.equal(fileHash(flowPath),flowHash);
    for(const script of SCRIPTS){const input=scriptSource(script);retain('documents/'+script.id+'.md',input);for(const format of ['pdf','epub']){const label='script-'+script.id+'-'+(format==='pdf'?'typst':'epub'),passed=await runCheck(label,async()=>{const file=await exportActual(label,format,input,'typst'),actual=format==='pdf'?pdfData(file,label):epubData(file,label);if(format==='epub'){assert.equal(actual.native.kind,'epub');assert(!actual.native.leakedCitationSlot);}return assertScriptPdf(actual.value,script,actual.operators);});report.scriptAvailability.push({script:script.id,consumer:format==='pdf'?'typst':'MuPDF EPUB',passed,missingGlyphAllowed:false});}
      await guard('guard-rust-'+script.id,input,'pdf','rust','author_inline_text_pdf_shaping_unsupported');if(script.rtl)await guard('guard-tex-'+script.id,input,'pdf','xelatex','author_inline_text_unsupported');
    }
    for(const [format,engine]of [['pdf','rust'],['pdf','typst'],['pdf','xelatex'],['epub','rust']])await guard('guard-explicit-rtl-'+(format==='epub'?'epub':engine),explicitRtlSource(),format,engine,engine==='rust'&&format==='pdf'?'author_inline_text_pdf_shaping_unsupported':'author_inline_text_unsupported');
    report.legacyConsumerCounts={checks:report.checks.length,outputs:report.outputs.length,readerOutputs:report.readerOutputs.length,scriptAvailability:report.scriptAvailability.length};
    const captionContext={root:ROOT,evidence,report,retain,call,python,payload,exportActual,runCheck,guard,pdfData,epubData,authorInspector:AUTHOR_INSPECTOR,scripts:SCRIPTS,qa:{...module.exports,near,overlap}};
    const captionQueue=await captionQa.firstPhase(captionContext);report.hostBudgets.at(-1).workFinishedUtc=new Date().toISOString();const firstSession={passed:true};await client.stop(firstSession);report.sessions.push(firstSession);report.hostBudgets.at(-1).finishedUtc=new Date().toISOString();const firstPid=nativeQa.assertStoppedSession(firstSession);report.hostBudgets.at(-1).normalEof=true;clearTimeout(timer);client=null;
    stage('caption different PID saved-source reopen');client=new StdioClient({binary:binding.binary,assets,profile,documents:path.join(evidence,'documents'),out:path.join(evidence,'wire-reopened')});hostBudget('reopened');await client.initialize(true);assert.notEqual(client.info.pid,firstPid);retain('tools-reopened.json',client.tools);await captionQa.reopenedPhase(captionContext,captionQueue,firstPid,client.info.pid);
    assert.equal(fileHash(sourcePath),sourceHash,'Export changed author source');if(presetHash)assert.equal(fileHash(presets),presetHash,'Source-owned author styles polluted user presets');report.sourceAndPresetsPreserved=true;
    assert.deepEqual(report.legacyConsumerCounts,{checks:23,outputs:13,readerOutputs:5,scriptAvailability:8});assert.equal(captionQueue.length,7);assert(report.captionRoundtrips.every(row=>row.reopenedInDifferentProcess));assert.equal(report.captionScriptAvailability.length,16);assert.equal(report.uniformSectionAvailability.length,4);assert.equal(report.checks.length,105);report.passed=report.errors.length===0;
  }catch(error){report.error=String(error.stack||error);report.firstFailure??={stage:report.stage,error:report.error};}
  finally{
    clearTimeout(timer);if(client){const row=report.hostBudgets.at(-1);if(row)row.workFinishedUtc??=new Date().toISOString();const session={passed:true};try{await client.stop(session);}catch(error){session.passed=false;session.error=String(error.stack||error);}report.sessions.push(session);if(row){row.finishedUtc=new Date().toISOString();row.normalEof=session.passed===true&&session.transportFatal===null&&session.session?.hostExit?.code===0&&session.session?.forcedStop===false;}if(!session.passed)report.passed=false;}
    try{verifyBinding(binding);assert.equal(fileHash(options.python),report.python.sha256);reuse?.assertPreserved();report.finalCandidateBindingVerified=true;}catch(error){report.passed=false;report.bindingError=String(error.stack||error);}
    report.finishedUtc=new Date().toISOString();retain('report.json',report);
  }
  console.log(JSON.stringify({evidence:path.relative(ROOT,evidence),passed:report.passed,checks:report.checks.length,outputs:report.outputs.length,readerOutputs:report.readerOutputs.length,firstFailure:report.firstFailure,error:report.error,bindingError:report.bindingError}));if(!report.passed)process.exitCode=1;
}
if(require.main===module)main().catch(error=>{console.error(error.stack||error);process.exitCode=1;});
module.exports={ROOT,BASE,QA_FILES,PRODUCT_FILES,SCRIPTS,COMMON_MARKERS,SOURCE_ORDER,DOCX_EXPECTED,PYTHON_READER,span,commonSource,scriptSource,explicitRtlSource,optionsFor,exportRequestFor,argumentsFor,bindingFor,normalized,locate,union,checkTrace,checkGlyphs,assertOperatorSizes,epubRoles,epubChapterInput,epubBodyPages,assertCommonPdf,assertScriptPdf,assertDocx,assertEpubPackage,errorValue,assertTexRtlDetails,epubLinkEvidence,guardRequest,planFor,selftest};
module.exports.planFor=fullPlan;
