'use strict';
// A new, bounded M07 lane. --plan and the pure controls start no host. Actual
// execution always uses the existing real StdioClient and a fresh candidate.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {spawnSync}=require('node:child_process');
const {StdioClient,assertToolCallSchema,verifyBinding,systemEnvironment}=require('./batch3-semantics-stdio.cjs');
const {payload}=require('./managed-tex-process-cold.cjs');
const previous=require('./batch3-math-section-stdio.cjs');
const marked=require('../../assets/vendor/marked.min.js');
const ROOT=path.resolve(__dirname,'../..'),BASE=path.join(ROOT,'.cache/capability-upgrade/batch3-m07-paragraph-20261023/qa');
const ORACLE='tools/test/inspect-math-word-paragraph.py',OLD_ORACLE='tools/test/inspect-math-section-roundtrip.py';
const WORD='http://schemas.openxmlformats.org/wordprocessingml/2006/main',MATH='http://schemas.openxmlformats.org/officeDocument/2006/math';
const SELF='tools/test/batch3-math-word-paragraph-stdio.cjs';
const QA_FILES=[...new Set([SELF,SELF.replace('.cjs','.test.cjs'),ORACLE,ORACLE.replace('.py','.test.py'),...previous.QA_FILES,
  'ui-tests/publication-engines-native.cjs','ui-tests/zip-parts.cjs','tools/test/publication-batch3-harness.cjs','tools/test/publication-batch3-fixtures.cjs','tools/test/publication-batch3-checks.cjs','tools/test/publication-runtime-reuse.cjs'])];
const PRODUCT_FILES=[...new Set([...previous.PRODUCT_FILES,...['math_word.rs','math_layout.rs','math_formats.rs','html_math.rs','pdf_render.rs','typst_compile.rs','export_styles.rs','export_profiles.rs','publication_fonts.rs','math_font_theme.rs','svg_render.rs'].map(name=>'rust/readmd-kernel/src/'+name),
  'assets/fonts/STIXTwoMath-Regular.ttf','assets/fonts/DejaVuSans.ttf','assets/fonts/DejaVuSans.LICENSE.txt',
  'rust/readmd-kernel/src/math_word/paragraph.rs','rust/readmd-kernel/src/math_word_projection.rs',
  'assets/js/reader/formula.js','assets/js/editor/formula-inline.js','assets/js/editor/formula-studio.js'])];
const sha=value=>crypto.createHash('sha256').update(value).digest('hex'),fileSha=file=>sha(fs.readFileSync(file));
const PRESETS={rust:'QA_M07_PARAGRAPH_RUST',typst:'QA_M07_PARAGRAPH_TYPST'};
const PAGE={size:'A4',orientation:'portrait',width:210,height:297,marginTop:20,marginRight:18,marginBottom:20,marginLeft:18};
const OUTPUT_BUDGET={perFileBytes:16*1024*1024,totalProductBytes:128*1024*1024};
const CASES=[
  {kind:'direct-no-styles',axis:'right',value:'540',before:{left:'720',right:'180',jc:'right'},after:{left:'720',right:'540',jc:'right'},deltaPt:-18},
  {kind:'styles-chain',axis:'left',value:null,before:{left:'0',right:'360',jc:'center'},after:{left:'480',right:'360',jc:'center'},deltaPt:12},
  {kind:'empty-zero',axis:'left',value:'720',before:{left:'0',right:'0',jc:'left'},after:{left:'720',right:'0',jc:'left'},deltaPt:36},
];
const REQUIRED_TOOLS=['readmd_math','readmd_content_transaction','readmd_read_document','readmd_edit_document','readmd_convert_to_markdown','readmd_export_document','readmd_export_epub','readmd_export_presets'];
function inside(base,file){const relative=path.relative(base,path.resolve(file));assert(relative&&relative!=='..'&&!relative.startsWith('..'+path.sep)&&!path.isAbsolute(relative),'Path escaped owned M07 scope');return path.resolve(file);}
function argumentsFor(argv,env=process.env){
  const out={candidate:env.READMD_MCP_CANDIDATE,binary:env.READMD_BIN,assets:env.READMD_ASSETS_DIR,profile:env.READMD_ENGINE_REUSE_PROFILE,python:env.READMD_TEST_PYTHON};
  for(let i=0;i<argv.length;i++){const key=argv[i];if(key==='--plan'){out.plan=true;continue;}if(key==='--help')return {help:true};assert(['--candidate','--binary','--assets','--profile','--python','--output'].includes(key),'Unknown '+key);assert(i+1<argv.length,'Missing '+key);out[key.slice(2)]=argv[++i];}
  for(const key of ['candidate','binary','assets','profile','python','output'])if(out[key])out[key]=path.resolve(out[key]);
  if(out.output)inside(BASE,out.output);if(!out.plan)for(const key of ['candidate','profile','python'])assert(out[key],'Required --'+key);return out;
}
function planFor(){return {schemaVersion:1,scope:'M07 first finite associated Word paragraph jc/physical-indent lifecycle',transport:'real-stdio',syntheticFixtures:true,
  noProcessesStarted:true,noFilesWritten:true,candidateRequiredForExecution:true,guiStarted:false,networkOrInstallAllowed:false,
  cases:CASES,selectedFormulaOrdinal:1,repeatedFormulaCount:3,preview:{fontSizePt:12,availableWidthPt:300,vectorTolerancePt:0.25},
  lifecycle:['independent native-only OPC fixture and hashes','real read-only convert; retain and revision-save the exact returned text','real migrate plus semantic XML, inverse transaction and idempotence','current UTF-16 MarkdownRange plus current Word SourceNode; edit only second declaration','actual formula and document stale revisions, undo and redo','revision save; stdin normal EOF exit0','different PID, same profile, actual disk read','options:{} DOCX/HTML/EPUB/Rust PDF/managed Typst PDF','native DOCX independent XML; actual reconvert, migrate and DOCX reexport'],
  baselineExports:['docx','pdf/rust','pdf/typst'],reopenedExports:['docx','html','epub','pdf/rust','pdf/typst'],reimportExports:['docx'],positiveExportAttempts:27,expectedChecks:9,productOutputBudget:OUTPUT_BUDGET,
  engineSelection:{mechanism:'two explicit custom presets in the same reviewed profile; engine and page are not injected into options',options:{},presets:PRESETS,pageUnit:'mm',page:PAGE,rawPresetBytesRetainedAndRestored:true,runtimeClone:false},
  independentEvidence:['native current OPC relationships and per-axis cascade','whole carrier/readable XML semantics','shared preview real SVG glyph paths in physical-point viewBox','actual PDF marker bands, page/media/crop and all vector curve coordinates','current EPUB MathML numerator/denominator plus root CSS and independent XML reparse','standalone HTML selected marked input and actual lowered vector/MathML nodes'],
  requiredTools:REQUIRED_TOOLS,qaFiles:QA_FILES,productFiles:PRODUCT_FILES,
  limitations:['This is the first three-fixture positive matrix, not all M07 design rows.','HTML headless marked reconstruction and EPUB package MathML do not certify browser/EPUB-reader physical geometry.','Word/WPS/LibreOffice application layout, TeX native OfficeMath, replacement MATH fonts, complete Word schema and all paragraph fields are untested.','Additional local-math-jc/dispDef=true/spacing differential, current auxiliary-source native mutations and unsupported/cancellation/budget matrices remain separate work.','A reused reviewed managed profile is not a cold-profile or cold-performance result.','Earlier caption/section external copy, EPUB conversion and mixed-load timeout issues remain unchanged.'],
  fullM07Certified:false,fullBatch3Certified:false};}
function bindingFor(options){
  const candidate=JSON.parse(fs.readFileSync(options.candidate,'utf8')),binary=options.binary||candidate.binary,assets=options.assets||path.dirname(candidate.boot),boot=path.join(assets,'readmd.boot.js');
  const sourceFreeze=inside(ROOT,path.resolve(ROOT,candidate.sourceFreeze));assert.equal(fileSha(sourceFreeze),candidate.sourceFreezeSha256);const freeze=JSON.parse(fs.readFileSync(sourceFreeze,'utf8')),boundFiles={};
  for(const name of [...QA_FILES,...PRODUCT_FILES]){const row=freeze.sources.find(row=>row.path===name);assert(row,'New candidate did not freeze '+name);boundFiles[name]={file:inside(ROOT,path.join(ROOT,name)),copy:inside(ROOT,path.join(ROOT,row.copy)),sha256:row.sha256};}
  const binding={candidate:options.candidate,candidateSha256:fileSha(options.candidate),binary,assets,boot,binarySha256:candidate.binarySha256,bootSha256:candidate.bootSha256,sourceFreeze,sourceFreezeSha256:candidate.sourceFreezeSha256,boundFiles};verifyBinding(binding);return binding;
}
const children=node=>(node.content||[]).filter(part=>part.type==='element').map(part=>part.value),local=node=>node.name.split(':').at(-1);
function walk(node,ancestors=[]){return [{node,ancestors},...children(node).flatMap(child=>walk(child,[...ancestors,node]))];}
function wordTarget(document,axis){
  assert(['left','right','jc'].includes(axis));const declarations=walk(document.root).filter(({node,ancestors})=>node.namespace===WORD&&local(node)==='pPr'&&!ancestors.some(parent=>parent.namespace===WORD&&['styles','style','docDefaults'].includes(local(parent))));
  assert.equal(declarations.length,1,'Exactly one current direct Word paragraph declaration required');
  const selected=children(declarations[0].node).filter(node=>node.namespace===WORD&&local(node)===(axis==='jc'?'jc':'ind'));assert.equal(selected.length,1);
  const node=selected[0],attributes=Object.keys(node.attributes||{}).filter(key=>key.split(':').at(-1)===(axis==='jc'?'val':axis)&&node.attribute_namespaces?.[key]===WORD);assert.equal(attributes.length,1,'Exact existing Word URI attribute required');
  const matches=document.nodes.filter(source=>source.id===node.id);assert.equal(matches.length,1,'Target is not its current SourceNode');const source=matches[0];
  assert(Number.isSafeInteger(source.from)&&Number.isSafeInteger(source.to)&&0<=source.from&&source.from<source.to&&source.to<=document.source.length);assert.equal(document.revision,sha(document.source));
  return {node,source,name:attributes[0],value:node.attributes[attributes[0]]};
}
function assertSecondChange(before,after,patchSource){
  const old=previous.mathBlocks(before),current=previous.mathBlocks(after);assert.equal(old.length,3);assert.equal(current.length,3);
  for(const index of [0,2])assert.equal(current[index].source,old[index].source,'An unselected identical formula changed');
  assert.equal(current[1].source,patchSource);assert.notEqual(current[1].source,old[1].source);
  assert.equal(after,before.slice(0,old[1].from)+patchSource+before.slice(old[1].to),'Bytes outside the selected current UTF-16 interval changed');
  return current;
}
function assertFormulaAxisOnly(before,after,axis){
  const reduced=(node,ancestors=[])=>{
    const currentDirect=node.namespace===WORD&&local(node)===(axis==='jc'?'jc':'ind')&&ancestors.at(-1)?.namespace===WORD&&local(ancestors.at(-1))==='pPr'&&!ancestors.some(parent=>parent.namespace===WORD&&['styles','style','docDefaults'].includes(local(parent)));
    const attributes={};for(const [key,value]of Object.entries(node.attributes||{})){
      if(key==='xmlns'||key.startsWith('xmlns:'))continue;
      const uri=node.attribute_namespaces?.[key]||'',name=key.split(':').at(-1);
      if(currentDirect&&uri===WORD&&name===(axis==='jc'?'val':axis))continue;
      const expanded='{'+uri+'}'+name;assert(!Object.hasOwn(attributes,expanded),'Duplicate expanded XML attribute');attributes[expanded]=value;
    }
    return {name:'{'+(node.namespace||'')+'}'+local(node),attributes,content:(node.content||[]).map(part=>part.type==='element'?{type:'element',value:reduced(part.value,[...ancestors,node])}:part)};
  };
  assert.deepEqual(reduced(after.root),reduced(before.root),'Word patch changed another context, math default, operand, comment or attribute');
  assert.equal(after.revision,sha(after.source));assert.notEqual(after.source,before.source);return true;
}
function migrationPlan(value,source){
  assert.equal(value.ok,true);const result=value.migration;assert(result&&typeof result==='object');assert.equal(result.revision,sha(source));
  assert.deepEqual(result.diagnostics,[]);assert.deepEqual(result.recoveries,[]);assert(Array.isArray(result.edits));assert.equal(previous.applyEdits(source,result.edits),result.content);
  const blocks=previous.mathBlocks(result.content);assert.equal(blocks.length,3,'Exactly three migrated current display formulas required');
  let carriers=[];
  if(result.edits.length){assert.equal(result.edits.length,3,'Expected one real current carrier per owned formula');carriers=[...result.edits].sort((a,b)=>a.from-b.from).map(edit=>{const raw=source.slice(edit.from,edit.to).trimEnd(),at=raw.indexOf('<!--readmd:math:v1:');assert(at>0);const comment=raw.slice(at),metadata=previous.decodeLegacyMathMetadata(comment),expression=raw.slice(0,at).trimEnd();assert(expression.startsWith('$$')&&expression.endsWith('$$'));const tex=expression.slice(2,-2);assert([sha(tex),sha(tex.trim())].includes(metadata.source_revision),'The selected original native carrier is stale');return metadata;});}
  else{assert.equal(result.content,source);assert.equal(previous.mathBlocks(source).length,3);}
  return {...result,blocks,carriers};
}
function mathShape(node){const name=node.name.split('}').at(-1);return {name,text:name==='t'?node.text:'',children:(node.children||[]).filter(child=>child.name.startsWith('{'+MATH+'}')&&!child.name.split('}').at(-1).endsWith('Pr')).map(mathShape)};}
function assertFragment(value,native,{sparse=true}={}){
  assert.equal(value.kind,'math-word-fragment');assert.equal(value.independentXmlReparsed,true);assert.deepEqual(value.effective.unsupported,[]);
  assert.deepEqual(value.effective.axes,native.effective.axes,'Current per-axis inheritance differs from the independent native source');
  if(sparse)assert.deepEqual(value.direct,native.direct,'Migration normalized the sparse native declaration');
  assert.deepEqual(value.mathObjects,native.mathObjects,'Current native formula/run properties changed during readable migration');return true;
}
function assertNative(value,sourceRows,{baseline}={}){
  assert.equal(value.sourcePayloadUsedAsNativeEvidence,false);assert.equal(value.wordApplicationCertified,false);assert.equal(value.globalMathPrCount,1);assert.equal(value.paragraphs.length,3);
  const defaults=Object.fromEntries(value.mathPr.children.map(node=>[node.name.split('}').at(-1),node.attributes['{'+MATH+'}val']]));
  assert(['0','false','off'].includes(defaults.dispDef),'Current native dispDef must remain explicitly false');
  for(const [key,wanted]of Object.entries({preSp:120,postSp:240,interSp:40,intraSp:60})){assert(/^\d+$/.test(defaults[key]));assert.equal(Number(defaults[key]),wanted,'Current native global mathPr changed '+key);}
  for(let i=0;i<3;i++){const row=value.paragraphs[i],wanted=sourceRows[i];assert.deepEqual(row.effective.unsupported,[]);assert.deepEqual(row.mathTexts,['ab']);
    assert.deepEqual(row.mathObjects.map(mathShape),wanted.mathObjects.map(mathShape));for(const [axis,setting]of Object.entries(wanted.effective.axes))assert.equal(row.effective.axes[axis],setting,'Native formula '+i+' lost effective '+axis);
    assert.equal(row.runDeclarations.length,2);for(const run of row.runDeclarations){assert.equal(run.sizeHalfPoints,'24','Native math run lost its actual 12pt size');assert.equal(run.complexSizeHalfPoints,'24');assert(Object.values(run.fonts).length>0&&Object.values(run.fonts).every(value=>value==='STIX Two Math'),'Native math run changed its explicit STIX family');}
    if(!('left' in wanted.effective.axes)&&!('right' in wanted.effective.axes)){assert(!('left' in row.effective.axes)&&!('right' in row.effective.axes),'Empty context invented physical indent zero');}
    if(baseline&&i!==1){assert.deepEqual(row.direct,baseline.paragraphs[i].direct);assert.deepEqual(row.effective.axes,baseline.paragraphs[i].effective.axes,'An adjacent identical native formula acquired the selected override');}
  }return true;
}
function assertMathml(value,expected){
  assert.equal(value.independentXmlReparsed,true);assert.equal(value.display,'block');assert.deepEqual(value.tokens,['a','b']);assert.equal(value.fractions,1);
  for(const [axis,setting]of Object.entries(expected))if(axis==='jc')assert.equal(value.css['text-align'],setting);else assert.equal(value.css['margin-'+axis],Number(setting)/20+'pt');
  return true;
}
function numericClose(a,b,tolerance=0.25,message='Physical vector coordinate differs'){assert(Number.isFinite(a)&&Number.isFinite(b));assert(Math.abs(a-b)<=tolerance,message+': '+a+' vs '+b);}
function assertSvgDelta(before,after,deltaPt){
  assert(before.glyphPaths.length>=2);assert.equal(after.glyphPaths.length,before.glyphPaths.length);assert.equal(before.width,after.width);assert.equal(before.height,after.height);assert.equal(before.viewBox,after.viewBox);
  // Layout inputs are physical points. The drawing can use normalized SVG
  // coordinates: derive its units/em from both real dimensions, then convert
  // using the requested 12pt/em instead of treating user units as points.
  const width=/^([+0-9.e-]+)em$/.exec(before.width),height=/^([+0-9.e-]+)em$/.exec(before.height),box=before.viewBox.split(/\s+/).map(Number);assert(width&&height&&box.length===4&&box.every(Number.isFinite));
  const unitsPerEm=box[2]/Number(width[1]);assert(unitsPerEm>0&&Number.isFinite(unitsPerEm));const lexicalUncertainty=0.0005*unitsPerEm+0.0005+Number(height[1])*unitsPerEm*0.0005/Number(width[1]);numericClose(box[3],Number(height[1])*unitsPerEm,lexicalUncertainty+1e-8,'Inconsistent SVG axes/em scale');const pointsPerUnit=12/unitsPerEm;
  for(let i=0;i<before.glyphPaths.length;i++){const a=before.glyphPaths[i],b=after.glyphPaths[i];assert.equal(a.outlineSha256,b.outlineSha256);numericClose((b.x-a.x)*pointsPerUnit,deltaPt);numericClose(b.y*pointsPerUnit,a.y*pointsPerUnit);numericClose(b.scaleX,a.scaleX,1e-10);numericClose(b.scaleY,a.scaleY,1e-10);}
  return {deltaPt,tolerancePt:0.25,actualOutlineCount:before.glyphPaths.length,unitsPerEm,pointsPerSvgUnit:pointsPerUnit,pointUnitsVerified:true};
}
function assertPdfDelta(before,after,deltaPt){
  assert.equal(before.kind,'math-word-pdf');assert.equal(after.kind,before.kind);assert.equal(before.coordinateSpace,'unrotated-page-physical-points-top-left');assert.equal(after.coordinateSpace,before.coordinateSpace);
  assert.deepEqual(after.pages,before.pages,'Page/media/crop/rotation changed');assert.deepEqual(after.markers,before.markers,'Nonformula physical marker positions changed');assert.equal(before.pages.length,1,'The small wide fixture must fit one real page');
  const page=before.pages[0];assertPageFrame(page);numericClose(page.rect[2]-page.rect[0],PAGE.width*72/25.4,0.1);numericClose(page.rect[3]-page.rect[1],PAGE.height*72/25.4,0.1);
  const host=pdfFrame(before),currentHost=pdfFrame(after);assert.deepEqual(currentHost,host,'Actual PDF backend/frame contract changed');
  const x0=host.actualOriginPt,availablePt=host.availablePt;
  assert.equal(before.formulas.length,3);assert.equal(after.formulas.length,3);
  for(let ordinal=0;ordinal<3;ordinal++){const a=before.formulas[ordinal],b=after.formulas[ordinal],dx=ordinal===1?deltaPt:0;assert.equal(a.ordinal,ordinal);assert.equal(b.ordinal,ordinal);assert.equal(b.page,a.page,'Formula changed physical page');assert.equal(b.drawings.length,a.drawings.length);assert(a.drawings.length>=3);
    for(let n=0;n<4;n++)numericClose(b.bounds[n],a.bounds[n]+(n%2===0?dx:0));
    for(let d=0;d<a.drawings.length;d++){const x=a.drawings[d],y=b.drawings[d];for(const key of ['fill','color','width','closePath','type','lineCap','lineJoin','fillOpacity','strokeOpacity'])assert.deepEqual(y[key],x[key]);assert.equal(y.items.length,x.items.length);
      for(let n=0;n<4;n++)numericClose(y.bounds[n],x.bounds[n]+(n%2===0?dx:0));
      for(let j=0;j<x.items.length;j++){const p=x.items[j],q=y.items[j];assert.equal(q.operation,p.operation);assert.deepEqual(q.scalar,p.scalar);assert.equal(q.coordinates.length,p.coordinates.length);for(let k=0;k<p.coordinates.length;k++){assert.equal(q.coordinates[k].length,p.coordinates[k].length);for(let n=0;n<p.coordinates[k].length;n++)numericClose(q.coordinates[k][n],p.coordinates[k][n]+(n%2===0?dx:0));}}
    }
    assert(b.bounds[0]>=x0-0.25&&b.bounds[2]<=x0+availablePt+0.25,'Actual formula lies outside its host content box');
  }
  return {deltaPt,tolerancePt:0.25,actualHostOriginPt:x0,actualPage:page,availablePt,frame:host,actualVectorPathsVerified:true,otherTwoFormulasUnchanged:true};
}
function assertPageFrame(page){assert.equal(page.rotation,0,'The finite portrait fixture must not acquire a page rotation');assert.deepEqual(page.mediaBox,page.cropBox,'A cropped-page transform needs separate physical evidence');assert.deepEqual(page.cropBox,page.rect);assert.equal(page.rect[0],0);assert.equal(page.rect[1],0);assert(page.rect[2]>0&&page.rect[3]>0);}
function assertPdfEngine(value,engine){
  assert(value.metadata&&typeof value.metadata==='object');
  if(engine==='rust'){assert.equal(value.metadata.producer,'readmd rust pdf_render');assert.equal(value.metadata.creator,'readmd');}
  else{assert.equal(engine,'typst');assert.equal(value.metadata.creator,'Typst 0.15.1');assert.notEqual(value.metadata.producer,'readmd rust pdf_render');}
  return true;
}
function pdfFrame(actual){
  // Fixed existing engine contracts, selected from independently inspected PDF
  // metadata. Formula positions/ink never determine the host origin or width.
  const engine=actual.metadata?.producer==='readmd rust pdf_render'?'rust':actual.metadata?.creator==='Typst 0.15.1'?'typst':null;
  assert(engine,'Unknown PDF backend cannot select a physical frame');assertPdfEngine(actual,engine);
  assert.equal(actual.pages.length,1);const page=actual.pages[0];assertPageFrame(page);
  const pageWidth=page.rect[2]-page.rect[0];numericClose(pageWidth,PAGE.width*72/25.4,0.1);numericClose(page.rect[3]-page.rect[1],PAGE.height*72/25.4,0.1);
  // pdf_render: DocGeometry.avail=width-12 and Sheet.x0=left+FRAME_PAD(6).
  // Typst: page margins and Layout.column_width have no such frame padding.
  const paddingPt=engine==='rust'?6:0,expectedOriginPt=PAGE.marginLeft*72/25.4+paddingPt;
  for(const name of ['M07_BEFORE_1','M07_BEFORE_2','M07_BEFORE_3','M07_SUFFIX_KEEP']){
    const marker=actual.markers[name];assert(marker&&Array.isArray(marker.bounds)&&marker.bounds.length===4&&marker.bounds.every(Number.isFinite),'Missing/invalid current PDF host marker '+name);assert.equal(marker.page,0);
    numericClose(marker.bounds[0],expectedOriginPt,0.25,'Actual host left origin disagrees with saved preset and fixed '+engine+' frame: '+name);
  }
  const actualOriginPt=actual.markers.M07_BEFORE_1.bounds[0],availablePt=pageWidth-actualOriginPt-PAGE.marginRight*72/25.4-paddingPt;assert(availablePt>0);
  return {engine,paddingLeftPt:paddingPt,paddingRightPt:paddingPt,expectedOriginPt,actualOriginPt,actualPageWidthPt:pageWidth,availablePt,markerCount:4};
}
function originFor(available,advance,axes){
  const left=Number(axes.left??0)/20,right=Number(axes.right??0)/20,inner=available-left-right;assert(inner>advance,'Owned simple fraction must fit its actual inner content box');
  const alignment=axes.jc??'center';assert(['left','center','right'].includes(alignment));
  return alignment==='left'?left:alignment==='right'?available-right-advance:left+(inner-advance)/2;
}
function assertSvgPlacement(actual,intrinsic,axes){
  assert.equal(actual.actualBezierExtremaInspected,true);assert.equal(intrinsic.actualBezierExtremaInspected,true);assert.equal(actual.glyphs.length,2);assert.equal(intrinsic.glyphs.length,2);
  const hasWordBox=Object.keys(axes).length>0,shift=hasWordBox?originFor(300,intrinsic.layoutAdvancePt,axes):0;
  if(hasWordBox)numericClose(actual.layoutAdvancePt,300,0.01,'Word paragraph preview frame is not its actual requested available width');
  for(let index=0;index<2;index++){const a=actual.glyphs[index],b=intrinsic.glyphs[index];assert.equal(a.outlineSha256,b.outlineSha256);for(const edge of [0,2])numericClose(a.inkBoundsPt[edge],b.inkBoundsPt[edge]+shift);}
  return {requestedAvailablePt:300,intrinsicAdvancePt:intrinsic.layoutAdvancePt,expectedFormulaOriginPt:shift,actualGlyphOutlinesVerified:true,sideBearingsPt:intrinsic.glyphs.map(row=>row.sideBearingXPt)};
}
function assertPdfPlacement(actual,intrinsic,rows){
  const host=pdfFrame(actual),x0=host.actualOriginPt,available=host.availablePt;assert.equal(actual.formulas.length,3);
  const placements=[];for(let index=0;index<3;index++){const shift=originFor(available,intrinsic.layoutAdvancePt,rows[index].effective.axes),formula=actual.formulas[index];assert(Array.isArray(formula.inkBounds)&&formula.inkBounds.length===4);
    for(const edge of [0,2])numericClose(formula.inkBounds[edge],x0+intrinsic.inkBoundsPt[edge]+shift,0.25,'Actual PDF ink does not match current natural advance/side bearing and paragraph origin');
    placements.push({ordinal:index,actualInkBoundsPt:formula.inkBounds,expectedFormulaOriginPt:x0+shift,intrinsicAdvancePt:intrinsic.layoutAdvancePt,sideBearingsPt:intrinsic.glyphs.map(row=>row.sideBearingXPt)});
  }return {actualOriginPt:x0,availablePt:available,tolerancePt:0.25,frame:host,placements,actualAbsoluteInkVerified:true};
}
function assertHtmlFlow(value,rows,intrinsic){
  assert.equal(value.sourcePayloadUsedAsEvidence,false);assert.equal(value.browserGeometryCertified,false);assert.equal(value.objects.length,3);
  for(let index=0;index<3;index++){const object=value.objects[index],axes=rows[index].effective.axes;if(object.kind==='mathml'){assertMathml(object,axes);continue;}
    assert.equal(object.kind,'svg');assert(object.wrapper&&object.wrapper.attributes.class.split(/\s+/).includes('readmd-export-math-block'),'Current vector lost its real display wrapper');
    const css={};for(const property of (object.wrapper.attributes.style||'').split(';'))if(property.trim()){const colon=property.indexOf(':');assert(colon>0);const key=property.slice(0,colon).trim();assert(!Object.hasOwn(css,key));css[key]=property.slice(colon+1).trim();}
    if(axes.jc!==undefined)assert.equal(css['text-align'],axes.jc,'Current HTML display wrapper lost effective Word alignment');
    const left=Number(axes.left??0)/20,right=Number(axes.right??0)/20;numericClose(object.ink.layoutAdvancePt,intrinsic.layoutAdvancePt+left+right,0.02,'Intrinsic HTML vector lost physical Word margin');
    assert.equal(object.ink.glyphs.length,2);for(let glyph=0;glyph<2;glyph++){const actual=object.ink.glyphs[glyph],bare=intrinsic.glyphs[glyph];assert.equal(actual.outlineSha256,bare.outlineSha256);for(const edge of [0,2])numericClose(actual.inkBoundsPt[edge],bare.inkBoundsPt[edge]+left);}
  }return {currentSvgOrMathmlVerified:true,browserGeometryCertified:false};
}
function htmlSelectedMarkdown(raw){
  const matches=[...raw.matchAll(/<script\b[^>]*\bid="md-source"[^>]*>([\s\S]*?)<\/script\s*>/g)];assert.equal(matches.length,1);assert(raw.includes("html = marked.parse(md, { gfm: true, breaks: true });"));assert(raw.includes("document.getElementById('content').innerHTML = html;"));
  assert(!raw.includes('id="readmd-footnote-html"'),'A different actual HTML rendering branch needs its own oracle');const content=JSON.parse(matches[0][1]);assert.equal(typeof content,'string');return content;
}
function presetOverlay(original){const data=JSON.parse(original);assert(data&&typeof data==='object'&&!Array.isArray(data));const custom=data.custom??{};assert(custom&&typeof custom==='object'&&!Array.isArray(custom));for(const key of Object.values(PRESETS))assert(!Object.hasOwn(custom,key),'A prior M07 preset was not restored');const out=structuredClone(data);out.custom={...custom};
  for(const [engine,key]of Object.entries(PRESETS))out.custom[key]={page:PAGE,typography:{font:'Arial',size:12,lineHeight:1.6,spacing:6,align:'left',firstLineIndent:0},cover:{enabled:false},toc:{enabled:false},header:{text:''},footer:{text:'',pageNumbers:false},pdf:{engine}};
  assert.deepEqual(out.last,data.last);for(const key of Object.keys(custom))assert.deepEqual(out.custom[key],custom[key]);return out;
}
function exportRequestFor(label,source,format,engine,documents,output){
  assert(['docx','html','epub','pdf'].includes(format));assert(Object.hasOwn(PRESETS,engine));
  const args={markdown_content:source,output_path:output,base_dir:documents,options:{},confirm:true};
  if(format!=='epub'){args.output_format=format;args.style_preset=PRESETS[engine];}
  // Dedicated EPUB has no style_preset/output_format in its real MCP schema.
  return {name:format==='epub'?'readmd_export_epub':'readmd_export_document',arguments:args};
}

async function main(){
  const options=argumentsFor(process.argv.slice(2));if(options.help){console.log('--plan | --candidate JSON --profile REVIEWED_PROFILE --python EXISTING_PYTHON [--output OWNED_NEW_DIRECTORY]');return;}if(options.plan){console.log(JSON.stringify(planFor(),null,2));return;}
  assert.equal(process.platform,'win32');const binding=bindingFor(options);assert(fs.statSync(options.python).isFile());fs.mkdirSync(BASE,{recursive:true});const evidence=options.output?(fs.mkdirSync(options.output),options.output):fs.mkdtempSync(path.join(BASE,'stdio-'));
  for(const name of ['documents','fixtures','outputs','inspection','requests','responses','wire-first','wire-reopened'])fs.mkdirSync(path.join(evidence,name));
  const retain=(name,value)=>{const file=inside(evidence,path.join(evidence,name));fs.writeFileSync(file,typeof value==='string'||Buffer.isBuffer(value)?value:JSON.stringify(value,null,2)+'\n',{flag:'wx'});return file;};
  const report={...planFor(),noProcessesStarted:undefined,noFilesWritten:undefined,startedUtc:new Date().toISOString(),candidateBinding:binding,passed:false,checks:[],inputs:[],outputs:[],sessions:[],roundtrips:[],migrations:[],geometry:[],errors:[],runtimeCopied:false,python:{path:options.python,sha256:fileSha(options.python),qaOnly:true}};
  let client,reuse,assets=binding.assets,profile=options.profile,presetsFile,presetsBefore,presetsInstalledSha,timer,serial=0,oracleSerial=0,stage='setup';const queue=[];
  const documents=path.join(evidence,'documents');
  const check=(name,details={})=>report.checks.push({name,passed:true,...details});
  const call=async(name,args,timeout=120000)=>{verifyBinding(binding);assertToolCallSchema({name,arguments:args},client.tools);const key=String(++serial).padStart(3,'0')+'-'+name;retain('requests/'+key+'.json',{name,arguments:args});const result=await client.tool(name,args,timeout);retain('responses/'+key+'.json',result);verifyBinding(binding);return result;};
  const oracle=(label,args,script=ORACLE)=>{verifyBinding(binding);const key=String(++oracleSerial).padStart(3,'0')+'-'+label,result=spawnSync(options.python,['-I','-B',path.join(ROOT,script),...args],{windowsHide:true,timeout:60000,maxBuffer:32*1024*1024,env:systemEnvironment(profile,documents)});retain('inspection/'+key+'.stdout.json',result.stdout||Buffer.alloc(0));retain('inspection/'+key+'.stderr.log',result.stderr||Buffer.alloc(0));assert(!result.error,result.error?.message);assert.equal(result.status,0,'Independent oracle failed '+key);verifyBinding(binding);return JSON.parse(result.stdout);};
  const convert=async(label,file)=>{const before=fileSha(file),names=fs.readdirSync(path.dirname(file)).sort(),result=await call('readmd_convert_to_markdown',{file_path:file});assert.notEqual(result.isError,true);const raw=result.content.filter(row=>row.type==='text').map(row=>row.text).join('\n');assert(raw.trim());retain('inspection/'+label+'-actual-convert.md',raw);assert.equal(fileSha(file),before);assert.deepEqual(fs.readdirSync(path.dirname(file)).sort(),names,'Read-only convert created a sidecar');return raw;};
  const read=async(file,text)=>{const result=payload(await call('readmd_read_document',previous.sourceReadArguments(file,text)));assert.equal(result.content,text);assert.equal(result.revision,sha(text));return result;};
  const save=async(file,before,after)=>{const value=await read(file,before);previous.savedEdit(payload(await call('readmd_edit_document',{file_path:file,expected_revision:value.revision,old_text:before,new_text:after,dry_run:false,confirm:true})),before,after);assert.equal(fs.readFileSync(file,'utf8'),after);};
  const start=async(name,budget)=>{verifyBinding(binding);assert.equal(fileSha(presetsFile),presetsInstalledSha,'Preset overlay changed between PID sessions');client=new StdioClient({binary:binding.binary,assets,profile,documents,out:path.join(evidence,name)});await client.initialize(true);for(const tool of REQUIRED_TOOLS)assert(client.tools.some(value=>value.name===tool));retain(name+'/tools.json',client.tools);const catalog=payload(await call('readmd_export_presets',{}));for(const [engine,key]of Object.entries(PRESETS)){assert.equal(catalog.custom[key].pdf.engine,engine);assert.deepEqual(catalog.custom[key].page,PAGE);}report.presetBinding.actualMcpCatalogVerifiedPids??=[];report.presetBinding.actualMcpCatalogVerifiedPids.push(client.info.pid);const started=Date.now();report.hostBudgets??=[];report.hostBudgets.push({name,pid:client.info.pid,budgetMs:budget,startedUtc:new Date(started).toISOString()});timer=setTimeout(()=>{report.workflowTimedOut=true;client?.transport.fail(Error('M07 owned host workflow deadline'),'workflow_deadline');client?.forceStop();},budget);};
  const stop=async()=>{clearTimeout(timer);const session={passed:true};await client.stop(session);report.sessions.push(session);assert.equal(session.passed,true);assert.equal(session.session.forcedStop,false);assert.equal(session.session.hostExit.code,0);client=null;};
  const parse=async(label,source,{intrinsic=false}={})=>{const request={operation:'parse',source,dialect:intrinsic?'omml':'dsl',display:true,font_size:12};if(!intrinsic)request.layout_width=300;const value=payload(await call('readmd_math',request));assert.equal(value.document.source,source);assert.equal(value.document.revision,sha(source));assert.deepEqual(value.document.diagnostics,[]);assert.equal(typeof value.vector_preview,'string','A missing/unsupported preview is not positive layout evidence');assert(value.vector_preview.startsWith('<svg'));const file=retain('inspection/'+label+'.svg',value.vector_preview),vector=oracle(label+'-svg',['svg',file],OLD_ORACLE),ink=oracle(label+'-actual-ink',['svg-ink',file]);return {value,vector,ink};};
  const fragment=async(label,source)=>{const value=payload(await call('readmd_math',{operation:'serialize',source,dialect:'dsl',display:true,target:'omml'}));const file=retain('inspection/'+label+'.omml',value.output);return {word:oracle(label+'-word',['fragment',file]),semantic:oracle(label+'-whole',['fragment',file],OLD_ORACLE)};};
  const migrate=async(label,raw,native)=>{const migration=migrationPlan(payload(await call('readmd_math',{operation:'migrate',content:raw,expected_revision:sha(raw)})),raw),rows=[];
    for(let index=0;index<3;index++){const row=await fragment(label+'-'+index,migration.blocks[index].source);assertFragment(row.word,native.paragraphs[index]);assert.deepEqual(row.word.mathPr,native.mathPr,'Migration changed current native global mathPr declarations');if(migration.carriers.length){const original=retain('inspection/'+label+'-carrier-'+index+'.omml',migration.carriers[index].omml),before=oracle(label+'-carrier-'+index,['fragment',original],OLD_ORACLE);assert.equal(row.semantic.semanticTreeSha256,before.semanticTreeSha256,'Migration changed the complete native carrier semantic tree');assert.deepEqual(row.semantic.semanticTree,before.semanticTree);}rows.push(row.word);}
    const transaction={base_revision:sha(raw),origin:'formula',edits:migration.edits},applied=previous.transactionResult(payload(await call('readmd_content_transaction',{content:raw,transaction})),raw,migration.edits);assert.equal(applied.content,migration.content);
    const reversed=previous.transactionResult(payload(await call('readmd_content_transaction',{content:applied.content,transaction:{base_revision:applied.revision,origin:'formula',edits:applied.inverse}})),applied.content,applied.inverse);assert.equal(reversed.content,raw);
    previous.assertIdempotentMathMigration(payload(await call('readmd_math',{operation:'migrate',content:applied.content,expected_revision:applied.revision})),applied.content);retain('inspection/'+label+'-readable.md',applied.content);report.migrations.push({label,rawSha256:sha(raw),readableSha256:sha(applied.content),carrierCount:migration.carriers.length,actualTransactionInverse:true,actualIdempotence:true,wholeSemanticTreesVerified:true,sparseDeclarationsVerified:true});return {source:applied.content,rows};};
  const exportActual=async(label,source,format,engine='rust')=>{const file=path.join(evidence,'outputs',label+'.'+format),request=exportRequestFor(label,source,format,engine,documents,file),response=payload(await call(request.name,request.arguments));const bytes=fs.statSync(file).size;assert(bytes>0&&bytes<=OUTPUT_BUDGET.perFileBytes,'Actual product output exceeds its fixed per-file byte budget');assert.equal(response.file_size,bytes);assert.equal(path.resolve(response.output_path).toLowerCase(),path.resolve(file).toLowerCase());if(format!=='epub')assert.equal(response.format,format);assert(report.outputs.reduce((sum,row)=>sum+row.bytes,0)+bytes<=OUTPUT_BUDGET.totalProductBytes,'Actual product outputs exceed their fixed total byte budget');const warnings=response.warnings;assert(Array.isArray(warnings),'Actual MCP export must return its warnings array');assert(!/glyph_missing|missing character:|missing.{0,30}glyph|glyph.{0,30}missing/iu.test(JSON.stringify(warnings)),'Actual missing-glyph output');const row={label,format,engine:format==='pdf'?engine:undefined,path:path.relative(evidence,file),sha256:fileSha(file),bytes,sourceMarkdownSha256:sha(source),emptyOptions:true,explicitSavedProfilePreset:request.arguments.style_preset??null,warnings};report.outputs.push(row);return {file,response,row};};
  try{
    retain('plan.json',planFor());stage='reviewed offline resources and temporary preset overlay';const helpers=require('../../ui-tests/publication-engines-native.cjs'),reviewed=helpers.resolveEngineReuseProfile(profile);let runtimeSerial=0;
    const harness={root:ROOT,data:evidence,report,command(binary,args){const result=spawnSync(binary,args,{windowsHide:true,timeout:60000,maxBuffer:24*1024*1024}),key='runtime-'+String(++runtimeSerial).padStart(2,'0');retain('inspection/'+key+'.stdout.log',result.stdout||Buffer.alloc(0));retain('inspection/'+key+'.stderr.log',result.stderr||Buffer.alloc(0));assert(!result.error);assert.equal(result.status,0);return result.stdout.toString('utf8');}};
    for(const [key,name]of [['READMD_FONTS_PACK','readmd-fonts-2026.10.10-universal.readmd-pack'],['READMD_PDFIUM_PACK','readmd-pdfium-7881-windows-x64.readmd-pack'],['READMD_TYPST_PACK','readmd-typst-0.15.1-windows-x64.readmd-pack']])process.env[key]??=path.join(ROOT,'.cache/capability-upgrade/locked-packs',name);
    reuse=helpers.prepareEngineProfileReuse(harness,reviewed,assets);assets=reuse.assets;profile=reviewed;presetsFile=path.join(profile,'export_presets.json');assert(fs.statSync(presetsFile).isFile(),'Reviewed preset file must already exist; no absence/delete restoration');presetsBefore=fs.readFileSync(presetsFile);retain('inspection/export-presets-before.json',presetsBefore);const overlay=presetOverlay(presetsBefore.toString('utf8'));const installed=JSON.stringify(overlay,null,2)+'\n';retain('inspection/export-presets-overlay.json',installed);assert.equal(fileSha(presetsFile),sha(presetsBefore));fs.writeFileSync(presetsFile,installed);presetsInstalledSha=sha(installed);report.presetBinding={path:presetsFile,beforeSha256:sha(presetsBefore),installedSha256:presetsInstalledSha,oldCustomAndLastPreserved:true,restored:false};
    await start('wire-first',360000);
    for(const testCase of CASES){stage='native convert/migrate/second-source transaction '+testCase.kind;console.log(JSON.stringify({stage}));const label=testCase.kind,file=path.join(evidence,'fixtures',label+'.docx'),input=oracle(label+'-fixture',['fixture','--kind',label,'--output',file]);assert.equal(input.nativeOnly,true);assert.deepEqual(input.inspection.auxiliaryParts,{});assert.equal(input.sha256,fileSha(file));assert.deepEqual(input.inspection.paragraphs[1].effective.axes,testCase.before);report.inputs.push({label,path:path.relative(evidence,file),sha256:fileSha(file),bytes:fs.statSync(file).size,nativeOnly:true,mathRunDeclarations:input.inspection.paragraphs.map(row=>row.runDeclarations)});const raw=await convert(label,file),saved=retain('documents/'+label+'.md','OWNED_M07_SENTINEL_'+label+'\n');const sentinel=fs.readFileSync(saved,'utf8');await save(saved,sentinel,raw);check(label+': exact raw conversion explicitly revision saved',{rawSha256:sha(raw),inputSha256:input.sha256});
      const readable=await migrate(label,raw,input.inspection);if(raw!==readable.source)await save(saved,raw,readable.source);const blocks=previous.mathBlocks(readable.source),previews=[],intrinsic=await parse(label+'-intrinsic-current',readable.rows[1].intrinsicOmml,{intrinsic:true});for(let index=0;index<3;index++){const preview=await parse(label+'-before-'+index,blocks[index].source);previews.push(preview);report.geometry.push({label,consumer:'shared-vector-absolute-before',ordinal:index,...assertSvgPlacement(preview.ink,intrinsic.ink,readable.rows[index].effective.axes)});}const target=wordTarget(previews[1].value.document,testCase.axis);assert.equal(target.value,testCase.before[testCase.axis]);const patch={operation:'set_attribute',node_id:target.node.id,name:target.name,value:testCase.value},changed=payload(await call('readmd_math',{operation:'patch',source:blocks[1].source,dialect:'dsl',display:true,font_size:12,layout_width:300,expected_revision:previews[1].value.document.revision,patch}));assert.notEqual(changed.document.source,blocks[1].source);
      assertFormulaAxisOnly(previews[1].value.document,changed.document,testCase.axis);assert.equal(previous.errorBody(await call('readmd_math',{operation:'patch',source:changed.document.source,dialect:'dsl',display:true,expected_revision:previews[1].value.document.revision,patch})).error_code,'transaction_conflict');const edits=[{from:blocks[1].from,to:blocks[1].to,insert:changed.document.source}],transaction={base_revision:sha(readable.source),origin:'formula',edits},applied=previous.transactionResult(payload(await call('readmd_content_transaction',{content:readable.source,transaction})),readable.source,edits);assertSecondChange(readable.source,applied.content,changed.document.source);
      assert.equal(previous.errorBody(await call('readmd_content_transaction',{content:applied.content,transaction})).error_code,'transaction_conflict');const undone=previous.transactionResult(payload(await call('readmd_content_transaction',{content:applied.content,transaction:{base_revision:applied.revision,origin:'formula',edits:applied.inverse}})),applied.content,applied.inverse);assert.equal(undone.content,readable.source);const redone=previous.transactionResult(payload(await call('readmd_content_transaction',{content:undone.content,transaction})),undone.content,edits);assert.equal(redone.content,applied.content);
      const changedFragment=await fragment(label+'-changed',changed.document.source);assert.deepEqual(changedFragment.word.effective.axes,testCase.after);assert.deepEqual(changedFragment.word.mathObjects,readable.rows[1].mathObjects);const rows=readable.rows.map((row,index)=>index===1?changedFragment.word:row);
      const baseline={};for(const [format,engine]of [['docx','rust'],['pdf','rust'],['pdf','typst']]){const exported=await exportActual(label+'-before-'+(format==='pdf'?engine:format),readable.source,format,engine);const inspected=oracle(label+'-before-'+(format==='pdf'?engine:format),[format==='docx'?'inspect':'pdf',exported.file]);if(format==='docx')assertNative(inspected,readable.rows);else{assertPdfEngine(inspected,engine);report.geometry.push({label,consumer:engine+'-pdf-absolute-before',...assertPdfPlacement(inspected,intrinsic.ink,readable.rows)});}baseline[format==='docx'?'docx':engine]=inspected;}
      await save(saved,readable.source,redone.content);report.roundtrips.push({label,inputSha256:input.sha256,saved:path.relative(evidence,saved),savedSha256:fileSha(saved),rawConversionSaved:true,explicitRevisionSave:true,actualUndoRedo:true,actualStaleRevisions:true,onlySecondCurrentRangeChanged:true,reopenedInNewPid:false});queue.push({testCase,file:saved,source:redone.content,rows,baseline,previews,intrinsic});check(label+': only second current Word SourceNode changed; inverse, redo and stale revisions');
    }
    const firstPid=client.info.pid;await stop();report.firstPid=firstPid;await start('wire-reopened',420000);assert.notEqual(client.info.pid,firstPid);report.reopenedPid=client.info.pid;
    for(const entry of queue){const {testCase,source,rows}=entry,label=testCase.kind;stage='different PID options-empty outputs '+label;console.log(JSON.stringify({stage}));await read(entry.file,source);const blocks=previous.mathBlocks(source);
      for(let index=0;index<3;index++){const after=await parse(label+'-reopened-'+index,blocks[index].source),details=assertSvgDelta(entry.previews[index].vector,after.vector,index===1?testCase.deltaPt:0);report.geometry.push({label,consumer:'shared-vector-preview',ordinal:index,...details,...assertSvgPlacement(after.ink,entry.intrinsic.ink,rows[index].effective.axes)});const serialized=payload(await call('readmd_math',{operation:'serialize',source:blocks[index].source,dialect:'dsl',display:true,font_size:12,target:'mathml'}));const mathml=retain('inspection/'+label+'-reopened-'+index+'.mathml',serialized.output);assertMathml(oracle(label+'-mathml-'+index,['mathml',mathml]),rows[index].effective.axes);}
      for(const [format,engine]of [['docx','rust'],['html','rust'],['epub','rust'],['pdf','rust'],['pdf','typst']]){const exported=await exportActual(label+'-reopened-'+(format==='pdf'?engine:format),source,format,engine);
        if(format==='docx'){const native=oracle(label+'-reopened-docx',['inspect',exported.file]);assertNative(native,rows,{baseline:entry.baseline.docx});const raw=await convert(label+'-native-reimport',exported.file),again=await migrate(label+'-native-reimport',raw,native),second=await exportActual(label+'-native-reimport-docx',again.source,'docx');assertNative(oracle(label+'-second-native',['inspect',second.file]),native.paragraphs);}
        else if(format==='pdf'){const physical=oracle(label+'-reopened-'+engine,['pdf',exported.file]);assertPdfEngine(physical,engine);const proof=assertPdfDelta(entry.baseline[engine],physical,testCase.deltaPt);report.geometry.push({label,consumer:engine+'-pdf',...proof,...assertPdfPlacement(physical,entry.intrinsic.ink,rows)});}
        else if(format==='epub'){const epub=oracle(label+'-current-epub',['epub',exported.file]);for(let index=0;index<3;index++)assertMathml(epub.math[index],rows[index].effective.axes);}
        else{const markdown=htmlSelectedMarkdown(fs.readFileSync(exported.file,'utf8')),rendered=marked.parse(markdown,{gfm:true,breaks:true}),current=retain('inspection/'+label+'-html-selected-marked.html',rendered),flow=oracle(label+'-current-html',['flow',current]);Object.assign(exported.row,{htmlSelectedRendererVerified:true,currentObjectKinds:flow.objects.map(row=>row.kind),...assertHtmlFlow(flow,rows,entry.intrinsic.ink)});}
      }
      Object.assign(report.roundtrips.find(row=>row.label===label),{reopenedInNewPid:true,firstPid,reopenedPid:client.info.pid,secondExportEmptyOptions:true,nativeReimportReexportVerified:true});check(label+': different PID options:{} native/flow/vector and both physical PDF consumers');
    }
    await stop();assert.equal(report.inputs.length,3);assert.equal(report.sessions.length,2);assert.equal(report.outputs.length,planFor().positiveExportAttempts);assert.equal(report.checks.length,planFor().expectedChecks);assert.equal(report.migrations.length,6);assert.equal(report.roundtrips.length,3);assert.equal(report.geometry.length,30);
    for(const row of [...report.inputs,...report.outputs])assert.equal(fileSha(inside(evidence,path.join(evidence,row.path))),row.sha256,'An earlier input/output changed during the workflow');for(const row of report.roundtrips)assert.equal(fileSha(inside(evidence,path.join(evidence,row.saved))),row.savedSha256);
    report.passed=true;
  }catch(error){const failure={name:stage,passed:false,error:String(error.stack||error)};report.checks.push(failure);report.errors.push(failure);report.firstFailure??=failure;report.error=failure.error;process.exitCode=1;
  }finally{
    clearTimeout(timer);if(client)try{await stop();}catch(error){report.passed=false;report.cleanupError=String(error.stack||error);process.exitCode=1;}
    if(presetsBefore)try{assert.equal(fileSha(presetsFile),presetsInstalledSha,'Do not overwrite an unexpected concurrent preset change');fs.writeFileSync(presetsFile,presetsBefore);assert.equal(fileSha(presetsFile),sha(presetsBefore));report.presetBinding.restored=true;}catch(error){report.passed=false;report.presetRestoreError=String(error.stack||error);process.exitCode=1;}
    if(reuse)try{reuse.assertPreserved();}catch(error){report.passed=false;report.runtimePreservationError=String(error.stack||error);process.exitCode=1;}
    try{verifyBinding(binding);report.finalCandidateBindingVerified=true;}catch(error){report.passed=false;report.bindingError=String(error.stack||error);process.exitCode=1;}
    report.finishedUtc=new Date().toISOString();report.stage=stage;retain('report.json',report);console.log(JSON.stringify({evidence,passed:report.passed,checks:report.checks.length,outputs:report.outputs.length,errors:report.errors.length}));
  }
}
if(require.main===module)main().catch(error=>{console.error(error.stack||error);process.exitCode=1;});
module.exports={ROOT,BASE,ORACLE,QA_FILES,PRODUCT_FILES,WORD,MATH,CASES,PAGE,PRESETS,argumentsFor,planFor,bindingFor,wordTarget,assertSecondChange,assertFormulaAxisOnly,migrationPlan,mathShape,assertFragment,assertNative,assertMathml,assertSvgDelta,assertPdfDelta,assertPdfEngine,pdfFrame,originFor,assertSvgPlacement,assertPdfPlacement,assertHtmlFlow,htmlSelectedMarkdown,presetOverlay,exportRequestFor};
