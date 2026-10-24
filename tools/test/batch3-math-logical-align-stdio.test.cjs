'use strict';
// Process-free controls. These model assertions never simulate MCP success or
// count synthetic vector/OPC data as real product evidence.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const qa=require('./batch3-math-logical-align-stdio.cjs'),physical=require('./batch3-math-word-paragraph-stdio.cjs'),life=require('./batch3-math-section-stdio.cjs');
const clone=value=>structuredClone(value);
function node(name,id,attributes={},parts=[],namespace=qa.WORD){return {name:'w:'+name,id,namespace,attributes,attribute_namespaces:Object.fromEntries(Object.keys(attributes).map(key=>[key,namespace])),content:parts.map(value=>({type:'element',value}))};}
function model(){
  const inherited='word.jc(val: "end")',selected='word.pStyle(val: "Equation")',pr='word.pPr('+selected+')';
  const source='// author 😀\r\nrow(word.styles('+inherited+'), '+pr+', office.oMath(x))';
  const jc=node('jc','inherited-jc',{'w:val':'end'}),style=node('styles','styles',{},[node('pPr','style-pr',{},[jc])]);
  const selectedNode=node('pStyle','selected-style',{'w:val':'Equation'}),direct=node('pPr','direct-pr',{},[selectedNode]);
  const range=(id,text)=>({id,kind:id,from:source.indexOf(text),to:source.indexOf(text)+text.length,tex:text,slots:[]});
  return {source,revision:qa.sha(source),root:node('#root','root',{},[style,direct,node('oMath','math',{},[],qa.MATH)],''),nodes:[range('inherited-jc',inherited),range('selected-style',selected),range('direct-pr',pr)]};
}
function added(value,kind='end'){
  const before=clone(value),target=qa.insertDirectJc(before,kind),out=clone(before),pr=out.root.content[1].value;
  pr.content.push({type:'element',value:node('jc','direct-jc',{'w:val':kind})});
  out.source=before.source.slice(0,target.source.from)+target.replacement+before.source.slice(target.source.to);out.revision=qa.sha(out.source);
  const range=out.nodes.find(row=>row.id==='direct-pr');range.to=range.from+target.replacement.length;range.tex=target.replacement;
  const text='word.jc(val: '+JSON.stringify(kind)+')',from=out.source.lastIndexOf(text);out.nodes.push({id:'direct-jc',kind:'jc',from,to:from+text.length,tex:text,slots:[]});return out;
}
function markdown(){const text='// a comment 😀\r\nrow(word.pPr(word.pStyle(val: "Equation")), office.oMathPara(office.oMath(frac(a,b))))';return '\uFEFFKEEP_PREFIX_𝛂\r\n\r\n'+Array.from({length:3},()=> '```readmd-math v1\r\n'+text+'\r\n```\r\n\r\n').join('')+'KEEP_SUFFIX\r\n';}
function native(kind,generated=false){
  const mathObject={name:'{'+qa.MATH+'}oMath',text:'',children:[{name:'{'+qa.MATH+'}f',text:'',children:[]}]};
  return {kind:'math-word-paragraph',sourcePayloadUsedAsNativeEvidence:false,wordApplicationCertified:false,auxiliaryParts:{},globalMathPrCount:1,
    mathPr:{children:Object.entries({dispDef:'0',preSp:'120',postSp:'240',interSp:'40',intraSp:'60'}).map(([key,value])=>({name:'{'+qa.MATH+'}'+key,attributes:{['{'+qa.MATH+'}val']:value}}))},
    paragraphs:Array.from({length:3},(_,i)=>({direct:{jcPresent:generated||i===0,indPresent:generated||i===0,axes:generated?{left:'720',right:'360',jc:kind}:i===0?{left:'720',right:'360',jc:kind}:{}},
      effective:{axes:{left:'720',right:'360',jc:kind},physicalAxes:{left:'720',right:'360',jc:qa.LOGICAL[kind]},provenance:generated?{left:'direct',right:'direct',jc:'direct'}:qa.expectedOrigins()[i],unsupported:[],selectedStyle:generated?'ReadMDMath':i===0?'Opposite':i===1?'Equation':'DefaultsOnly',styleChain:generated?['ReadMDMath']:i===1?['Base','Equation']:[i===0?'Opposite':'DefaultsOnly']},
      mathTexts:['ab'],mathObjects:[clone(mathObject)],runDeclarations:Array.from({length:2},()=>({fonts:{ascii:'STIX Two Math',hAnsi:'STIX Two Math'},sizeHalfPoints:'24',complexSizeHalfPoints:'24'}))}))};
}
function pdf(engine='rust'){
  const padding=engine==='rust'?6:0,x0=qa.PAGE.marginLeft*72/25.4+padding,box=[0,0,qa.PAGE.width*72/25.4,qa.PAGE.height*72/25.4],markers={};
  for(let i=1;i<=3;i++)markers['M07_BEFORE_'+i]={page:0,bounds:[x0,i*80,x0+90,i*80+10]};markers.M07_SUFFIX_KEEP={page:0,bounds:[x0,330,x0+110,340]};
  return {kind:'math-word-pdf',coordinateSpace:'unrotated-page-physical-points-top-left',metadata:engine==='rust'?{producer:'readmd rust pdf_render',creator:'readmd'}:{producer:'',creator:'Typst 0.15.1'},pages:[{mediaBox:box,cropBox:box,rect:box,rotation:0,imageCount:0}],markers,
    formulas:Array.from({length:3},(_,ordinal)=>({ordinal,page:0,bounds:[x0+36,100+ordinal*80,x0+56,120+ordinal*80],drawings:Array.from({length:3},(_,d)=>({bounds:[x0+36+d,100+ordinal*80,x0+46+d,120+ordinal*80],fill:[0,0,0],color:null,width:0,closePath:true,items:[{operation:'c',coordinates:[[x0+36+d,100+ordinal*80],[x0+36+d,120+ordinal*80],[x0+46+d,120+ordinal*80],[x0+46+d,100+ordinal*80]],scalar:[]}]}))}))};
}

test('plan is bounded to fourteen actual outputs, explicit origin boundary and two PID lifecycle',()=>{
  const p=qa.planFor();assert.equal(p.noProcessesStarted,true);assert.equal(p.noFilesWritten,true);assert.equal(p.positiveExportAttempts,14);assert.equal(p.productOutputBudget.positiveFiles,14);assert.equal(p.nativeOnlyInputs,2);assert.equal(p.derivedCrlfInputs,2);assert.equal(p.totalOwnedInputFixtures,4);assert.equal(p.expectedChecks,7);assert.equal(p.expectedGeometryRecords,28);assert(p.crlfSetup.hashContract.includes('never normalize'));assert.deepEqual(p.mapping,{start:'left',end:'right'});assert.equal(p.nativeStyleProvenanceRoundtripCertified,false);assert.equal(p.nativeOutputContract.generatedAxesOrigin,'direct');for(const key of ['fullM07Certified','fullP04Certified','fullBatch3Certified'])assert.equal(p[key],false);assert.equal(p.preview.tolerancePt,.25);assert(p.lifecycle.includes('normal stdin EOF exit0'));
});
test('only owned new attempt paths and fixed CLI arguments are accepted',()=>{
  assert.throws(()=>qa.argumentsFor([],{}));assert.equal(qa.argumentsFor(['--plan'],{}).plan,true);assert.throws(()=>qa.argumentsFor(['--plan','--output',qa.BASE],{}));assert.throws(()=>qa.argumentsFor(['--plan','--output',qa.ROOT],{}));assert.throws(()=>qa.argumentsFor(['--plan','--binary','unbound.exe'],{}));assert.equal(qa.argumentsFor(['--plan','--output',path.join(qa.BASE,'fresh')],{}).output,path.join(qa.BASE,'fresh'));
});
test('all new and unchanged shared checker/product sources must be frozen',()=>{
  for(const name of [...qa.QA_FILES,...qa.PRODUCT_FILES])assert(fs.statSync(path.join(qa.ROOT,name)).isFile(),name);assert.equal(new Set(qa.QA_FILES).size,qa.QA_FILES.length);assert.equal(new Set(qa.PRODUCT_FILES).size,qa.PRODUCT_FILES.length);assert(qa.QA_FILES.includes('tools/test/inspect-math-word-paragraph.py'));assert(qa.QA_FILES.includes('tools/test/inspect-docx-settings-roundtrip.py'));assert(qa.PRODUCT_FILES.includes('rust/readmd-kernel/src/math_word/paragraph.rs'));
});
test('current direct pPr ignores inherited context and binds exact UTF-16 SourceNode text',()=>{
  const value=model(),selected=qa.directParagraph(value);assert.equal(selected.node.id,'direct-pr');assert.equal(selected.source.tex,'word.pPr(word.pStyle(val: "Equation"))');assert.equal(value.source.slice(selected.source.from,selected.source.to),selected.source.tex);assert(value.source.indexOf('😀')<selected.source.from);
});
test('source range/ID/revision mismatches and duplicate/foreign pPr cannot grant authority',()=>{
  for(const mode of ['revision','tex','range','id','duplicate','uri']){const value=model();if(mode==='revision')value.revision='0'.repeat(64);if(mode==='tex')value.nodes.at(-1).tex='word.pPr()';if(mode==='range')value.nodes.at(-1).from++;if(mode==='id')value.nodes.at(-1).id='old';if(mode==='duplicate')value.root.content.push(clone(value.root.content[1]));if(mode==='uri')value.root.content[1].value.namespace='urn:fake';assert.throws(()=>qa.directParagraph(value),mode);}
});
test('insert whole jc with required val while retaining current sparse pStyle and source bytes',()=>{
  const value=model(),before=clone(value),target=qa.insertDirectJc(value,'start');assert.equal(target.replacement,'word.pPr(word.pStyle(val: "Equation"), word.jc(val: "start"))');assert.deepEqual(value,before);qa.assertDirectJcOnly(value,added(value,'start'));assert.throws(()=>qa.insertDirectJc(value,'left'));assert.throws(()=>qa.insertDirectJc(added(value),'start'));
});
test('direct-jc-only proof also protects shadowed style declarations, math and namespace',()=>{
  const value=model(),after=added(value,'start');for(const mode of ['inherited','operand','namespace','other-attribute']){const wrong=clone(after);if(mode==='inherited')wrong.root.content[0].value.content[0].value.content[0].value.attributes['w:val']='both';if(mode==='operand')wrong.root.content[2].value.name='m:f';if(mode==='namespace')wrong.root.content[1].value.namespace='urn:fake';if(mode==='other-attribute')wrong.root.content[1].value.content[0].value.attributes['w:val']='Other';assert.throws(()=>qa.assertDirectJcOnly(value,wrong),mode);}
});
test('current Word jc targets require the real URI-bound val attribute',()=>{
  const value=added(model(),'start'),targets=qa.wordJcTargets(value);assert.equal(targets.length,2);assert.deepEqual(targets.map(row=>row.value),['end','start']);for(const mode of ['uri','duplicate']){const wrong=clone(value),jc=wrong.root.content[1].value.content[1].value;if(mode==='uri')jc.attribute_namespaces['w:val']='urn:fake';else{jc.attributes['alias:val']='end';jc.attribute_namespaces['alias:val']=qa.WORD;}assert.throws(()=>qa.wordJcTargets(wrong));}
});
test('physical control changes precisely one actual Word jc and nothing hidden by cascade',()=>{
  const before=added(model(),'start'),target=qa.wordJcTargets(before)[0],after=clone(before);after.root.content[0].value.content[0].value.content[0].value.attributes['w:val']='right';after.source+=' ';after.revision=qa.sha(after.source);qa.assertWordJcMapping(before,after,target);const bad=clone(after);bad.root.content[1].value.content[1].value.attributes['w:val']='end';assert.throws(()=>qa.assertWordJcMapping(before,bad,target));
});
test('decorated source uses actual three ranges, astral comments and original body semantics',()=>{
  const before=markdown(),edits=qa.decoratedEdits(before,'start'),after=life.applyEdits(before,edits);assert.equal(edits.length,3);assert(after.startsWith('\uFEFFKEEP_PREFIX_𝛂\r\n'));assert(after.endsWith('KEEP_SUFFIX\r\n'));assert.equal((after.match(/OWNED_M07_LOGICAL/g)||[]).length,3);for(let i=0;i<3;i++)assert.equal(life.mathBlocks(after)[i].source,edits[i].insert);
});
test('CRLF fixture derives only from migrated LF and preserves exact non-EOL bytes and three UTF-16 ranges',()=>{
  const original=markdown(),lf=original.replace(/\r\n/g,'\n'),before=lf,derived=qa.deriveCrlfInput(lf);assert.equal(lf,before);assert.equal(derived.content,original);assert.equal(derived.content.replace(/\r\n/g,'\n'),lf);assert.equal(derived.lfSha256,qa.sha(lf));assert.equal(derived.crlfSha256,qa.sha(original));assert.notEqual(derived.lfSha256,derived.crlfSha256);assert.equal(derived.productSaveAsOrCopyCertified,false);assert.equal(derived.legacyCarrierPresent,false);assert.equal(derived.fullDocumentEolEquivalent,true);assert.equal(derived.allThreeFormulaBodiesEolEquivalent,true);assert.equal(Buffer.byteLength(original)-Buffer.byteLength(lf),derived.lineBreaks);
  const oldBlocks=life.mathBlocks(lf),newBlocks=life.mathBlocks(derived.content);for(let i=0;i<3;i++)assert.equal(newBlocks[i].source.replace(/\r\n/g,'\n'),oldBlocks[i].source);
});
test('setup rejects mixed existing EOL and legacy carriers rather than rewriting their digest assumptions',()=>{
  const lf=markdown().replace(/\r\n/g,'\n');for(const bad of [markdown(),lf.replace('\n','\r\n'),lf+'<!--readmd:math:v1:old-capsule-->',lf.replace(/\n/g,'')])assert.throws(()=>qa.deriveCrlfInput(bad));for(const bad of ['a\nb\n','a\r\nb\n','a\r\nb\r','no line breaks'])assert.throws(()=>qa.assertFullCrlf(bad));qa.assertFullCrlf(qa.deriveCrlfInput(lf).content);
});
test('owned CRLF decoration remains wholly CRLF and protects astral and nonformula bytes',()=>{
  const lf=markdown().replace(/\r\n/g,'\n'),derived=qa.deriveCrlfInput(lf),edits=qa.decoratedEdits(derived.content,'end'),after=life.applyEdits(derived.content,edits);qa.assertFullCrlf(after);assert(after.startsWith('\uFEFFKEEP_PREFIX_𝛂\r\n'));assert(after.endsWith('KEEP_SUFFIX\r\n'));assert.equal(after.replace(/\/\/ OWNED_M07_LOGICAL_end_[0-2] 😀\r\n/g,''),derived.content);assert.equal((after.match(/OWNED_M07_LOGICAL/g)||[]).length,3);
});
test('strict savedEdit still rejects LF-normalized response hashes when a CRLF result was requested',()=>{
  const before='ORIGINAL\r\n',after=markdown(),result={ok:true,dry_run:false,changed:true,replacements:1,revision:qa.sha(after),proposed_revision:qa.sha(after),checkpoint_id:'pure-assertion-control'};life.savedEdit(result,before,after);const normalized=clone(result);normalized.revision=normalized.proposed_revision=qa.sha(after.replace(/\r\n/g,'\n'));assert.throws(()=>life.savedEdit(normalized,before,after));assert.throws(()=>life.savedEdit(result,before,after.replace(/\r\n/g,'\n')));
});
test('historical raw LF revision is not mislabeled as the current migrated file hash',()=>{
  const file='documents/start-native-lf.md',raw='RAW NATIVE CARRIER\n',migrated='MIGRATED DSL\n',proof=qa.lfSaveHistory(file,raw,migrated,'inspection/start-actual-convert.md');assert.equal(proof.historicalRawSave.file,file);assert.equal(proof.historicalRawSave.committedRevision,qa.sha(raw));assert.equal(proof.historicalRawSave.supersededByRevision,qa.sha(migrated));assert.deepEqual(proof.historicalRawSave.retainedExactContent,{path:'inspection/start-actual-convert.md',sha256:qa.sha(raw)});assert.deepEqual(proof.migrationSaved,{path:file,sha256:qa.sha(migrated)});assert(!Object.hasOwn(proof.historicalRawSave,'path'));assert(!Object.hasOwn(proof.historicalRawSave,'sha256'));assert.notEqual(proof.historicalRawSave.committedRevision,proof.migrationSaved.sha256);
});
test('only the second repeated formula may change across BOM CRLF and surrogate prefix',()=>{
  const before=markdown(),blocks=life.mathBlocks(before),insert=blocks[1].source.replace('Equation','Other'),edits=[{from:blocks[1].from,to:blocks[1].to,insert}],after=life.applyEdits(before,edits);physical.assertSecondChange(before,after,insert);for(const i of [0,2])assert.throws(()=>physical.assertSecondChange(before,life.applyEdits(before,[{from:blocks[i].from,to:blocks[i].to,insert}]),insert));assert.throws(()=>physical.assertSecondChange(before,after.replace('KEEP_SUFFIX','BROKEN'),insert));assert.throws(()=>life.applyEdits('A😀B',[{from:2,to:3,insert:'x'}]));
});
test('original native axes must be logical and origins direct/style/defaults precisely',()=>{
  for(const kind of qa.CASES){const value=native(kind);qa.assertInitialNative(value,kind);for(const mode of ['physical-enum','origin','sparse','axis']){const bad=clone(value);if(mode==='physical-enum')bad.paragraphs[1].effective.axes.jc=qa.LOGICAL[kind];if(mode==='origin')bad.paragraphs[1].effective.provenance.jc='direct';if(mode==='sparse')bad.paragraphs[1].direct.jcPresent=true;if(mode==='axis')bad.paragraphs[2].effective.axes.right='0';assert.throws(()=>qa.assertInitialNative(bad,kind),mode);}}
});
test('generated DOCX preserves logical enum while honestly expecting ReadMDMath/direct origins',()=>{
  for(const kind of qa.CASES){const original=native(kind),generated=native(kind,true);qa.assertGeneratedNative(generated,original.paragraphs,kind);for(const mode of ['physical','origin','style','run-size']){const bad=clone(generated);if(mode==='physical')bad.paragraphs[1].direct.axes.jc=qa.LOGICAL[kind];if(mode==='origin')bad.paragraphs[1].effective.provenance.jc='style:Equation';if(mode==='style')bad.paragraphs[1].effective.selectedStyle='Equation';if(mode==='run-size')bad.paragraphs[1].runDeclarations[0].sizeHalfPoints='22';assert.throws(()=>qa.assertGeneratedNative(bad,original.paragraphs,kind),mode);}}
});
test('a physical consumer copy never mutates original axes or provenance',()=>{
  const rows=native('start').paragraphs,before=clone(rows),copy=qa.physicalRows(rows);assert.deepEqual(rows,before);assert.deepEqual(copy.map(row=>row.effective.axes.jc),['left','left','left']);assert.deepEqual(copy.map(row=>row.effective.provenance),rows.map(row=>row.effective.provenance));assert.deepEqual(rows.map(row=>row.effective.axes.jc),['start','start','start']);
});
test('MathML root CSS must use physical alignment and actual fraction operands',()=>{
  const row=native('end').paragraphs[0],good={independentXmlReparsed:true,display:'block',tokens:['a','b'],fractions:1,css:{'text-align':'right','margin-left':'36pt','margin-right':'18pt'}};qa.assertMathml(good,row);for(const mode of ['logical','operand','indent']){const bad=clone(good);if(mode==='logical')bad.css['text-align']='end';if(mode==='operand')bad.tokens.reverse();if(mode==='indent')bad.css['margin-left']='0pt';assert.throws(()=>qa.assertMathml(bad,row),mode);}
});
test('preset overlay keeps every custom value and Last and refuses a prior same-name overlay',()=>{
  const original={custom:{Existing:{typography:{size:23}}},last:{format:'docx',preset:'Existing'}},copy=qa.presetOverlay(JSON.stringify(original));assert.deepEqual(copy.custom.Existing,original.custom.Existing);assert.deepEqual(copy.last,original.last);for(const [engine,name]of Object.entries(qa.PRESETS)){assert.equal(copy.custom[name].pdf.engine,engine);assert.deepEqual(copy.custom[name].page,qa.PAGE);}assert.throws(()=>qa.presetOverlay(JSON.stringify(copy)));
});
test('positive export schema stays options:{} and dedicated EPUB receives no document-only fields',()=>{
  for(const format of ['docx','html','epub','pdf']){const value=qa.exportRequestFor('CURRENT',format,'typst','Z:/owned/documents','Z:/owned/'+format);assert.deepEqual(value.arguments.options,{});if(format==='epub'){assert.equal(value.name,'readmd_export_epub');assert(!Object.hasOwn(value.arguments,'style_preset'));assert(!Object.hasOwn(value.arguments,'output_format'));}else{assert.equal(value.name,'readmd_export_document');assert.equal(value.arguments.style_preset,qa.PRESETS.typst);assert.equal(value.arguments.output_format,format);}}
});
test('negative contract distinguishes legal source-only guards from invalid schema and fixed DSL byte budget',()=>{
  const rows=qa.negativeCases();assert.equal(rows.length,12);assert.equal(new Set(rows.map(row=>row.label)).size,rows.length);assert.equal(rows.filter(row=>row.sourceOnly).length,6);assert(rows.find(row=>row.label==='dsl-byte-budget').source.length>65536);assert.equal(rows.find(row=>row.label==='dsl-byte-budget').code,'math_budget');assert.equal(rows.find(row=>row.label==='wrong-uri').code,'math_invalid_property');assert(rows.find(row=>row.label==='inherited-bidi').source.includes('styleId: "RTL"'));assert(rows.find(row=>row.label==='unproved-tb').source.includes('"tb"'));
});
test('physical PDF equality requires all actual path operations and fixed frame at unchanged .25pt',()=>{
  for(const engine of ['rust','typst']){const control=pdf(engine),logical=clone(control),proof=physical.assertPdfDelta(control,logical,0);assert.equal(proof.tolerancePt,.25);assert.equal(proof.frame.engine,engine);for(const mode of ['curve','glyph-y','neighbor','margin','backend']){const bad=clone(logical);if(mode==='curve')bad.formulas[1].drawings[0].items[0].coordinates[1][0]+=1;if(mode==='glyph-y')bad.formulas[1].drawings[0].items[0].coordinates[1][1]+=1;if(mode==='neighbor')bad.formulas[0].bounds[0]+=1;if(mode==='margin')bad.markers.M07_BEFORE_1.bounds[0]+=1;if(mode==='backend')bad.metadata={};assert.throws(()=>physical.assertPdfDelta(control,bad,0),mode);}}
});
test('absolute origins use independent advance and both physical indents rather than output ink',()=>{
  assert.equal(physical.originFor(300,20,{left:'720',right:'360',jc:'left'}),36);assert.equal(physical.originFor(300,20,{left:'720',right:'360',jc:'right'}),262);assert.throws(()=>physical.originFor(300,20,{left:'720',right:'360',jc:'start'}));
  const natural={actualBezierExtremaInspected:true,layoutAdvancePt:20,inkBoundsPt:[0,0,20,12],glyphs:[{outlineSha256:'a',inkBoundsPt:[1,0,6,5],sideBearingXPt:1},{outlineSha256:'b',inkBoundsPt:[2,6,8,12],sideBearingXPt:2}]},logical=clone(natural);logical.layoutAdvancePt=300;for(const glyph of logical.glyphs){glyph.inkBoundsPt[0]+=262;glyph.inkBoundsPt[2]+=262;}physical.assertSvgPlacement(logical,natural,{left:'720',right:'360',jc:'right'});const bad=clone(logical);bad.glyphs[0].inkBoundsPt[0]+=1;assert.throws(()=>physical.assertSvgPlacement(bad,natural,{left:'720',right:'360',jc:'right'}));
});
