'use strict';
// Pure acceptance-oracle controls. Never starts ReadMD or an Office process.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const {spawnSync}=require('node:child_process');
const {gzipSync}=require('node:zlib');
const qa=require('./batch3-math-section-stdio.cjs');
const hash=value=>crypto.createHash('sha256').update(value).digest('hex');
const clone=value=>JSON.parse(JSON.stringify(value));
const declaration=(id,placement)=>'<!-- readmd-section:v1 '+JSON.stringify({id,footnotes:{kinds:{footnote:{placement}}}})+' -->\n\n';
function nativeError(overrides={}){return {ok:false,error_code:'docx_note_settings_invalid',error:'docx_note_settings_invalid',error_details:{part:'word/settings.xml',path:'footnotePr/numFmt',detail:'unknown native number format'},...overrides};}
const expectedError={code:'docx_note_settings_invalid',part:'word/settings.xml',pathIncludes:'numFmt',detailIncludes:'unknown native number format'};
function mcpError(body=nativeError()){return {isError:true,structuredContent:body,content:[{type:'text',text:JSON.stringify(body)}]};}
function nativeSections(){
  const note={footnote:{part:'word/notes/footnote-custom.xml',definitions:[{id:'7',type:'normal',text:'NATIVE_FOOT_BODY'},{id:'8',type:'normal',text:'NATIVE_SECOND_SECTION_NOTE'}]},endnote:{part:'word/notes/endnote-custom.xml',definitions:[{id:'7',type:'normal',text:'NATIVE_END_BODY'}]}};
  return {sourcePayloadUsedAsNativeEvidence:false,applicationRoundtripCertified:false,documentPart:'word/document.xml',auxiliaryParts:{'customXml/source.xml':'a'.repeat(64)},notes:note,
    sections:['beneathText','pageBottom'].map((placement,index)=>({ordinal:index,terminatorPath:[index*4+3],bodyEnd:index*4+3,nativePlacement:placement,effectivePlacement:placement,break:'continuous',paragraphs:[{text:'SECTION_SAME_HEADING',references:[],links:[]},{text:'SECTION_'+(index===0?'A':'B')+'_BODY ',references:[{kind:'footnote',id:index===0?'7':'8',text:index===0?'NATIVE_FOOT_BODY':'NATIVE_SECOND_SECTION_NOTE'},{kind:'footnote',id:index===0?'7':'8',text:index===0?'NATIVE_FOOT_BODY':'NATIVE_SECOND_SECTION_NOTE'},{kind:'endnote',id:'7',text:'NATIVE_END_BODY'}],links:[{text:'SECTION_EVIDENCE',target:'https://example.test/section-evidence'}]},...(index===0?[{text:'SECTION_A_CODE_END',references:[],links:[]}]:[])]}))};
}
function mathView(){return {sourcePayloadUsedAsNativeEvidence:false,applicationRoundtripCertified:false,formulas:[{text:'FORMULA_Aab01x01x',fractions:1,display:true,localAlignment:'left',effective:{...qa.EXPECTED_DEFAULTS,defJc:'left'},naries:[{localLimitLocation:'undOvr'},{localLimitLocation:'subSup'}]},{text:'FORMULA_BARE+x',display:true,localAlignment:'centerGroup',effective:{...qa.EXPECTED_DEFAULTS},fractions:0,naries:[]}]};}
const expectedMath=[{marker:'FORMULA_A',effective:{defJc:'left',smallFrac:false},alignment:'left',limits:['undOvr','subSup'],fractions:1},{marker:'FORMULA_BARE',effective:qa.EXPECTED_DEFAULTS,alignment:'centerGroup',fractions:0}];

test('plan is usable from a clean repository and has no process/file side effects',()=>{
  const result=spawnSync(process.execPath,[path.join(__dirname,'batch3-math-section-stdio.cjs'),'--plan'],{encoding:'utf8',timeout:10000,windowsHide:true});assert.equal(result.status,0,result.stderr);const value=JSON.parse(result.stdout);assert.equal(value.noProcessesStarted,true);assert.equal(value.noFilesWritten,true);assert.equal(value.httpProgressRoute,'/api/convert/progress?job=...');assert(value.requiredTools.includes('readmd_content_transaction'));assert.equal(value.applicationRoundtripCertified,false);for(const file of value.qaFiles)assert(fs.statSync(path.join(qa.ROOT,file)).isFile());
});
test('CLI requires a frozen candidate and an explicit QA Python',()=>{assert.throws(()=>qa.argumentsFor([],{}),/candidate/);assert.throws(()=>qa.argumentsFor(['--candidate','fake'],{}),/python/);assert.equal(qa.argumentsFor(['--plan'],{}).plan,true);assert.throws(()=>qa.argumentsFor(['--plan','--output',qa.ROOT],{}),/owned/);assert.throws(()=>qa.argumentsFor(['--unknown'],{}),/Unknown/);});
test('math split matrix preserves every original return case and separately covers both schema Choice branches',()=>{const original=['missing','empty','off','on','explicit-defaults','left','right','disabled-paragraph','empty-margin','explicit-margin','small-on','wrap-on'];for(const name of original)assert(qa.MATH_RETURN_VARIANTS.includes(name));assert.equal(new Set(qa.MATH_RETURN_VARIANTS).size,15);for(const name of ['wrap-empty','wrap-off','indent-zero'])assert(qa.MATH_RETURN_VARIANTS.includes(name));assert(qa.QA_FILES.includes('docs/design/office-math-coverage.json'));});
test('inline display-container guard material uses the exact reviewed native construction',()=>{assert(qa.INLINE_DISPLAY_MATH.includes(':math[row(office.mathPr(office.smallFrac()), office.oMathPara(office.oMath(frac(a,b))))]'));assert(qa.INLINE_DISPLAY_MATH.includes('$$c$$'));assert.equal(qa.mathBlocks(qa.INLINE_DISPLAY_MATH).length,0);});
test('legacy metadata oracle checks real tool capsule identity, digest and retained malformed native attribute',()=>{
  for(const item of qa.LEGACY_MATH_CASES){const omml=qa.legacyOmml(item.attribute),value={schema:1,id:item.id,source_revision:hash('x'),omml},encode=data=>'<!--readmd:math:v1:'+gzipSync(Buffer.from(JSON.stringify(data))).toString('base64')+'-->',metadata=encode(value);assert.equal(qa.assertLegacyMetadata(metadata,item.id,'x',item.attribute).omml,omml);assert.throws(()=>qa.assertLegacyMetadata(metadata,item.id,'changed',item.attribute));assert.throws(()=>qa.assertLegacyMetadata(metadata,'wrong-id','x',item.attribute));assert.throws(()=>qa.assertLegacyMetadata(encode({...value,omml:'<m:oMath/>'}),item.id,'x',item.attribute));}
  assert.throws(()=>qa.assertLegacyMetadata('ordinary HTML','legacy-id','x','val="0"'));
});
test('MCP typed errors require exact structured and text agreement',()=>{assert.deepEqual(qa.errorBody(mcpError()),nativeError());const changed=mcpError();changed.content[0].text=JSON.stringify({...nativeError(),error:'different'});assert.throws(()=>qa.errorBody(changed),/disagree/);assert.throws(()=>qa.errorBody({...mcpError(),isError:false}));assert.throws(()=>qa.errorBody({...mcpError(),structuredContent:undefined}));});
test('native error guard checks exact type and actual part/path/detail',()=>{assert.deepEqual(qa.assertNativeError(nativeError(),expectedError),nativeError().error_details);});
for(const [name,change]of Object.entries({wrongCode:body=>body.error_code='conversion_failed',humanError:body=>body.error='file may be corrupted',wrongPart:body=>body.error_details.part='word/document.xml',wrongPath:body=>body.error_details.path='endnote:7:[0]',wrongReason:body=>body.error_details.detail='ordinary conversion failed',notObject:body=>body.error_details=[],missingDetail:body=>delete body.error_details.detail,oversizedPath:body=>body.error_details.path='x'.repeat(4097)}))test('typed error rejects '+name,()=>{const body=nativeError();change(body);assert.throws(()=>qa.assertNativeError(body,expectedError));});
test('generic damaged conversion cannot acquire a known semantic label',()=>{assert(qa.assertGenericError({ok:false,error_code:'conversion_failed',error:'broken ZIP'}));assert.throws(()=>qa.assertGenericError(nativeError()));assert.throws(()=>qa.assertGenericError({ok:false,error_code:'conversion_failed',error_details:{part:'x'}}));});
test('section source parser reads actual top-level comments in order',()=>{const source=declaration('section-a','beneathText')+'# Same\n\n'+declaration('section-b','pageBottom')+'# Same\n';assert.deepEqual(qa.assertSectionSource(source,['beneathText','pageBottom']).map(row=>row.id),['section-a','section-b']);});
for(const [name,source]of Object.entries({code:'```html\n'+declaration('a','beneathText')+'```\n',quote:declaration('a','beneathText').split('\n').map(line=>'> '+line).join('\n'),inline:'Text `'+declaration('a','beneathText').trim()+'`\n',nestedHtml:'<pre>\n'+declaration('a','beneathText')+'</pre>\n',wrongNamespace:'<!-- readmd-other:v1 {"id":"a"} -->\n'}))test('section declaration cannot hide in '+name,()=>{assert.equal(qa.sectionDeclarations(source).length,0);assert.throws(()=>qa.assertSectionSource(source,['beneathText']));});
test('section source guard rejects stale placements and duplicate identity',()=>{assert.throws(()=>qa.assertSectionSource(declaration('a','pageBottom'),['beneathText']));assert.throws(()=>qa.assertSectionSource(declaration('a','beneathText')+declaration('a','pageBottom'),['beneathText','pageBottom']));});
test('native section oracle retains boundaries, repeated footnotes and mixed stories',()=>{assert.equal(qa.assertSections(nativeSections(),['beneathText','pageBottom']).nativeBoundaries,true);});
for(const [name,change]of Object.entries({placement:value=>value.sections[1].effectivePlacement='beneathText',bodyMoved:value=>value.sections[1].paragraphs.push(value.sections[0].paragraphs[1]),splitRepeatedFootnote:value=>value.sections[0].paragraphs[1].references[1].id='8',wrongStory:value=>value.sections[0].paragraphs[1].references[2].text='NATIVE_FOOT_BODY',linkTarget:value=>value.sections[0].paragraphs[1].links[0].target='https://wrong.example/',lostCodeTerminator:value=>value.sections[0].paragraphs.pop(),duplicateDefinition:value=>value.notes.footnote.definitions.push({...value.notes.footnote.definitions[0]}),splitEndnote:value=>value.sections[1].paragraphs[1].references[2].id='8',payloadOnly:value=>value.sourcePayloadUsedAsNativeEvidence=true}))test('native section guard rejects '+name,()=>{const value=nativeSections();change(value);assert.throws(()=>qa.assertSections(value,['beneathText','pageBottom']));});
test('single-section mutation requires one native part and byte-identical auxiliary source',()=>{const before=nativeSections(),after=clone(before);after.sections[0].nativePlacement=after.sections[0].effectivePlacement='pageBottom';const mutation={auxiliaryUnchanged:true,applicationEditingCertified:false,changedParts:['word/document.xml'],inspection:after};assert.equal(qa.assertSingleSectionMutation(before,mutation).sections[0].effectivePlacement,'pageBottom');const broken=clone(mutation);broken.inspection.auxiliaryParts['customXml/source.xml']='b'.repeat(64);assert.throws(()=>qa.assertSingleSectionMutation(before,broken));broken.inspection=clone(after);broken.inspection.sections[1].paragraphs[0].text='other';assert.throws(()=>qa.assertSingleSectionMutation(before,broken));});
test('math source is extracted only from an actual top-level editable fence with exact UTF16 range',()=>{const source='\uFEFF# 𝛂\r\n\r\n```readmd-math v1\r\nrow(office.mathPr(office.smallFrac()), office.oMath(x))\r\n```\r\n';const rows=qa.mathBlocks(source);assert.equal(rows.length,1);assert.equal(source.slice(rows[0].from,rows[0].to),rows[0].source);assert(rows[0].source.includes('office.smallFrac()'));assert.equal(qa.mathBlocks('> ```readmd-math v1\n> x\n> ```\n').length,0);assert.equal(qa.mathBlocks('<!--\n'+qa.fence('x')+'-->\n').length,0);});

test('candidate42 profile indentation may synthesize paragraph raw without changing exact math source ranges',()=>{
  const marked=require('../../assets/vendor/marked.min.js'),prefix='---\nreadmd:\n  publication:\n    typography:\n      size: 10.5\n    version: 1\n---\n<!-- readmd-numbering:v1 {"headings":{"levels":[{"format":"decimal","start":2}]}} -->\n\n# <span data-readmd-text="v1" data-size-pt="18" data-size-cs-pt="18">QA_MATH_AUX</span> {#qa-math-aux}\n\n<span data-readmd-text="v1" data-size-pt="11">MATH_SOURCE_FORMULA_OFF</span>\n\n',opening='```readmd-math v1\n',body='row(office.mathPr(office.smallFrac(val: "1")), office.oMathPara(office.oMath(x)))',source=prefix+opening+body+'\n```';
  const paragraph=marked.lexer(source,{gfm:true}).find(token=>token.type==='paragraph');assert(paragraph.raw.includes('size: 10.5\n\n    version: 1'));assert(!source.includes(paragraph.raw),'Control must reproduce the actual final-token raw mutation');assert.deepEqual(qa.mathBlocks(source),[{source:body,from:prefix.length+opening.length,to:prefix.length+opening.length+body.length}]);
});

test('range capture handles nonlocal paragraph merging and consumed link definitions without searching final raw tokens',()=>{
  const prefix='Introductory paragraph\n    indented code merged by marked\n\n[ref]: https://example.test/\n\n',opening='```readmd-math v1\n',body='row(office.oMath(x))',source=prefix+opening+body+'\n```\n';assert.deepEqual(qa.mathBlocks(source),[{source:body,from:prefix.length+opening.length,to:prefix.length+opening.length+body.length}]);
});

test('identical top-level fences retain distinct exact ranges despite earlier opaque and nested copies',()=>{
  const opening='```readmd-math v1\n',body='row(office.oMath(x))',fenced=opening+body+'\n```\n',nested=['> '+fenced.split('\n').filter(Boolean).join('\n> ')+'\n\n','- nested list\n\n  '+fenced.split('\n').filter(Boolean).join('\n  ')+'\n\n','1. ordered list\n\n   '+fenced.split('\n').filter(Boolean).join('\n   ')+'\n\n','<pre>\n'+fenced+'</pre>\n\n','<!--\n'+fenced+'-->\n\n','````example\n'+fenced+'````\n\n'];
  for(const prefix of nested){assert.equal(qa.mathBlocks(prefix).length,0,'Nested or opaque math is not a top-level editable object');const source=prefix+fenced+'\n'+fenced,first=prefix.length+opening.length,second=prefix.length+fenced.length+1+opening.length;assert.deepEqual(qa.mathBlocks(source),[{source:body,from:first,to:first+body.length},{source:body,from:second,to:second+body.length}]);}
});

test('CRLF, bare CR, BOM, astral text and indented fences preserve original body bytes and UTF16 boundaries',()=>{
  for(const eol of ['\n','\r\n','\r'])for(const marker of ['```','~~~~']){
    const prefix='\uFEFF# 😀 𝛂'+eol+eol,opening='  '+marker+'readmd-math v1'+eol,body='  row('+eol+'    office.oMath("😀")'+eol+'  )',source=prefix+opening+body+eol+'  '+marker+eol,from=prefix.length+opening.length;assert.deepEqual(qa.mathBlocks(source),[{source:body,from,to:from+body.length}]);assert.equal(source.slice(from,from+body.length),body);
  }
});

test('formula body equal to its info string is selected after the opening line, never from the header',()=>{
  const source='```readmd-math v1\nreadmd-math v1\n```\n';assert.deepEqual(qa.mathBlocks(source),[{source:'readmd-math v1',from:'```readmd-math v1\n'.length,to:'```readmd-math v1\nreadmd-math v1'.length}]);
  for(const ordinary of ['```readmd-math v10\nx\n```\n','```javascript\nreadmd-math v1\n```\n','    ```readmd-math v1\n    x\n    ```\n','Inline `readmd-math v1` x.\n'])assert.deepEqual(qa.mathBlocks(ordinary),[]);
});

test('exact root suffix, captured raw, final token identity and final raw guards reject corrupted lexer controls',()=>{
  const marked=require('../../assets/vendor/marked.min.js'),source=qa.fence('row(office.oMath(x))'),fences=marked.Tokenizer.prototype.fences,blocks=marked.Lexer.prototype.blockTokens,lex=marked.Lexer.prototype.lex;
  try{
    marked.Tokenizer.prototype.fences=function(input){const token=fences.call(this,input);if(token?.lang==='readmd-math v1')token.raw='wrong'+token.raw;return token;};assert.throws(()=>qa.mathBlocks(source),/raw/);
  }finally{marked.Tokenizer.prototype.fences=fences;}
  try{
    marked.Lexer.prototype.blockTokens=function(input,...args){return blocks.call(this,' '+input,...args);};assert.throws(()=>qa.mathBlocks(source),/suffix/);
  }finally{marked.Lexer.prototype.blockTokens=blocks;}
  try{
    marked.Lexer.prototype.lex=function(input){return lex.call(this,input).filter(token=>token.lang!=='readmd-math v1');};assert.throws(()=>qa.mathBlocks(source),/top-level/);
  }finally{marked.Lexer.prototype.lex=lex;}
  try{
    marked.Lexer.prototype.lex=function(input){const tokens=lex.call(this,input);tokens.find(token=>token.lang==='readmd-math v1').raw+='not-in-source';return tokens;};assert.throws(()=>qa.mathBlocks(source),/interval/);
  }finally{marked.Lexer.prototype.lex=lex;}
});
test('native math oracle requires per-object jc/limLoc and leaves a bare object at kernel defaults',()=>{assert(qa.assertMath(mathView(),expectedMath));});
for(const [name,change]of Object.entries({alignment:value=>value.formulas[0].localAlignment='right',limit:value=>value.formulas[0].naries[0].localLimitLocation=null,bareLeak:value=>value.formulas[1].effective.smallFrac=true,duplicate:value=>value.formulas.push({...value.formulas[0]}),missingFraction:value=>value.formulas[0].fractions=0,payloadOnly:value=>value.sourcePayloadUsedAsNativeEvidence=true}))test('native math oracle rejects '+name,()=>{const value=mathView();change(value);assert.throws(()=>qa.assertMath(value,expectedMath));});
test('equivalent defaults must not receive a false conflict warning',()=>{qa.assertNoDefaultsConflict({warns:[]});assert.throws(()=>qa.assertNoDefaultsConflict({warnings:['math_document_defaults_conflict']}));assert.throws(()=>qa.assertNoDefaultsConflict({warns:[{code:'math_document_defaults_conflict'}]}));});
test('true math conflict uses existing details contract and exact 1-based formula ordinals',()=>{const body={ok:false,error_code:'math_document_defaults_conflict',details:{property:'smallFrac',firstFormula:1,formula:2}};assert.equal(qa.assertConflict(mcpError(body),'smallFrac').formula,2);assert.throws(()=>qa.assertConflict(mcpError({...body,details:{...body.details,formula:1}}),'smallFrac'));assert.throws(()=>qa.assertConflict(mcpError(body),'wrapRight'));assert.throws(()=>qa.assertConflict(mcpError({...body,details:{...body.details,source:'private'}}),'smallFrac'));const wrong={ok:false,error_code:body.error_code,error_details:body.details};assert.throws(()=>qa.assertConflict(mcpError(wrong),'smallFrac'));});
test('section preflight refusal must locate the actual section leaf, not a global note limitation',()=>{const body={ok:false,error_code:'publication_note_policy_unsupported',path:'sections["section-a"].footnotes.kinds.footnote.placement'};assert.equal(qa.assertSectionProtection(mcpError(body),'section-a').path,body.path);assert.throws(()=>qa.assertSectionProtection(mcpError({...body,path:'footnotes.kinds.endnote.placement'}),'section-a'));assert.throws(()=>qa.assertSectionProtection(mcpError(body),'section-b'));});
test('uniform source sections inherit the current global note placement instead of old local overrides',()=>{const source='<!-- readmd-section:v1 {"id":"a"} -->\n\nBody A\n\n<!-- readmd-section:v1 {"id":"b"} -->\n\nBody B\n';assert.equal(qa.assertSectionSource(source,['pageBottom','pageBottom'],{globalPlacement:'pageBottom'}).length,2);assert.throws(()=>qa.assertSectionSource(source,['pageBottom','pageBottom'],{globalPlacement:'beneathText'}));});
test('transaction unwrapping checks actual envelope, UTF16 edit and inverse',()=>{const source='𝛂 original\n',edits=[{from:3,to:11,insert:'changed'}],content=qa.applyEdits(source,edits),value={ok:true,result:{content,revision:hash(content),changed:true,inverse:[{from:3,to:10,insert:'original'}]}};assert.equal(qa.transactionResult(value,source,edits).content,'𝛂 changed\n');assert.throws(()=>qa.transactionResult({...value,result:undefined},source,edits));assert.throws(()=>qa.transactionResult({...value,result:{...value.result,revision:'wrong'}},source,edits));assert.throws(()=>qa.applyEdits(source,[{from:1,to:2,insert:'x'}]),/surrogate/);});
test('explicit file save requires real revision/change/checkpoint result',()=>{const before='sentinel\n',after='source\n',value={ok:true,dry_run:false,changed:true,replacements:1,revision:hash(after),proposed_revision:hash(after),checkpoint_id:'checkpoint'};qa.savedEdit(value,before,after);for(const bad of [{dry_run:true},{changed:false},{checkpoint_id:null},{revision:hash(before)}])assert.throws(()=>qa.savedEdit({...value,...bad},before,after));});
test('source read uses the actual full line count under the real schema bound',()=>{assert.deepEqual(qa.sourceReadArguments('/owned/a.md','a\nb\n'),{file_path:'/owned/a.md',start_line:1,end_line:2});assert.throws(()=>qa.sourceReadArguments('/owned/a.md','x\n'.repeat(1001)));});
test('HTTP helper contract uses only loopback headless creation and targeted cleanup',()=>{const source=fs.readFileSync(path.join(qa.ROOT,qa.ORACLE),'utf8'),driver=fs.readFileSync(path.join(__dirname,'batch3-math-section-stdio.cjs'),'utf8');assert(source.includes('startup.wShowWindow = 0'));assert(source.includes('creationflags=subprocess.CREATE_NEW_CONSOLE'));assert(source.includes('creationflags=subprocess.CREATE_NEW_PROCESS_GROUP'));assert(source.includes('child.send_signal(signal.CTRL_BREAK_EVENT)'));assert(!source.includes('GenerateConsoleCtrlEvent(0, 0)'));assert(driver.includes("'--no-window'")===false,'Only the owned broker constructs host flags');assert(source.includes('"--no-window", "--host", "127.0.0.1"'));assert(source.includes('child.returncode == 0 and not (profile / "instance.json").exists()'));assert(!driver.includes('/api/system/quit'));});

// Candidate32 reached a real publication_numbering_invalid because QA appended
// its auxiliary declaration after body/note content. These controls are wholly
// self-contained; the retained real failure is separate evidence.
const numberingSources={
  bare:'# Body {#body}\n\nCurrent body.\n',
  bom:'\uFEFF# Body\n',
  yaml:'---\ntitle: Owned QA\ncatalog_id: "00123"\n---\n'+declaration('s','beneathText')+'# Body\n',
  unicodeCRLF:'\uFEFF---\r\ntitle: "😀 𝛂 -- source"\r\nreadmd:\r\n  publication:\r\n    version: 1\r\n---\r\n# Body {#stable}\r\n\r\n[^n]: Current note.\r\n',
  metadataTerminator:'---\ntitle: Owned\n...\n# Body\n',
};
function numberingReply(source,{at=qa.sourceHeaderEnd(source),insert,config=qa.AUX_NUMBERING_CONFIG}={}){
  const eol=source.includes('\r\n')?'\r\n':'\n';insert??='<!-- readmd-numbering:v1 '+JSON.stringify(config)+' -->'+eol+eol;
  const content=source.slice(0,at)+insert+source.slice(at);
  return {ok:true,transaction:{base_revision:hash(source),origin:'editor',edits:[{from:at,to:at,insert}]},result:{content,revision:hash(content),changed:true,inverse:[{from:at,to:at+insert.length,insert:''}]},config:clone(config),plan:{from:at+insert.indexOf('<!-- readmd-numbering:v1 '),to:at+insert.length}};
}
test('source binding requires the actual typed numbering tool in the candidate contract',()=>{assert(qa.planFor().requiredTools.includes('readmd_publication_numbering'));for(const file of ['cross_references.rs','document_transaction.rs'])assert(qa.PRODUCT_FILES.includes('rust/readmd-kernel/src/'+file));});
for(const [name,source]of Object.entries(numberingSources))test('typed numbering insertion preserves source, revision and inverse for '+name,()=>{const request=qa.auxNumberingRequest(source);assert.deepEqual(Object.keys(request).sort(),['action','base_revision','config','content']);assert.equal(request.action,'apply');assert.equal(request.base_revision,hash(source));assert.equal(request.content,source);assert.deepEqual(request.config,qa.AUX_NUMBERING_CONFIG);assert.equal(qa.assertAuxNumbering(numberingReply(source),source).slice(0,qa.sourceHeaderEnd(source)),source.slice(0,qa.sourceHeaderEnd(source)));});
test('EOF-appended numbering is rejected even with consistent transaction digest and inverse',()=>{const source=numberingSources.yaml;assert.throws(()=>qa.assertAuxNumbering(numberingReply(source,{at:source.length}),source),/precede body/);});
test('numbering cannot be prefixed ahead of YAML source metadata',()=>{const source=numberingSources.yaml;assert.throws(()=>qa.assertAuxNumbering(numberingReply(source,{at:0}),source),/front matter/);});
for(const [name,change]of Object.entries({stale:v=>v.transaction.base_revision='0'.repeat(64),badRevision:v=>v.result.revision='0'.repeat(64),lostInverse:v=>v.result.inverse=[],rewrite:v=>v.transaction.edits[0].to++,wrongConfig:v=>v.config.headings.levels[0].start=1,wrongPlan:v=>v.plan.from++,missingEnvelope:v=>v.ok=false,extraEdits:v=>v.transaction.edits.push({from:0,to:0,insert:'x'})}))test('typed numbering response rejects '+name,()=>{const source=numberingSources.unicodeCRLF,value=numberingReply(source);change(value);assert.throws(()=>qa.assertAuxNumbering(value,source));});
test('a prior caller cannot mutate later auxiliary numbering requests',()=>{const first=qa.auxNumberingRequest(numberingSources.bare);first.config.headings.levels[0].start=99;assert.equal(qa.auxNumberingRequest(numberingSources.bare).config.headings.levels[0].start,2);});

const metadataFor=(omml,id='legacy-native-display-mode')=>'<!--readmd:math:v1:'+gzipSync(Buffer.from(JSON.stringify({schema:1,id,source_revision:hash('x'),omml}))).toString('base64')+'-->';
function legacyModeView(row){const effective={...qa.EXPECTED_DEFAULTS,...(row.current?{smallFrac:true,preSp:120}:{})};return {sourcePayloadUsedAsNativeEvidence:false,applicationRoundtripCertified:false,mathEffective:effective,formulas:[{text:row.tex,display:row.mode==='display',localAlignment:row.mode==='display'?'centerGroup':null,effective:{...effective,...(row.mode==='display'?{}:{smallFrac:false,preSp:0})},naries:[],fractions:0}],sections:[{paragraphs:row.mode==='prose'?[{text:'Before  after'}]:[{text:''}]}]};}
test('legacy mode fixtures retain the genuine capsule bytes and vary only current literal math/context',()=>{
  assert.equal(qa.LEGACY_MODE_CASES.length,7);const metadata=metadataFor(qa.legacyDisplayOmml());
  for(const row of qa.LEGACY_MODE_CASES){const source=qa.legacyModeSource(row,metadata);assert.equal(source.split(metadata).length,2);assert(source.includes((row.mode==='inline'?'$':'$$')+row.tex+(row.mode==='inline'?'$':'$$')+row.gap+metadata));assert.equal(qa.assertLegacyMetadata(metadata,'legacy-native-display-mode','x','<m:oMathPara').source_revision,hash('x'));}
  assert(qa.LEGACY_MODE_CASES.some(row=>row.mode==='display'&&!row.current&&row.gap==='\n\n'));assert(qa.LEGACY_MODE_CASES.some(row=>row.mode==='inline'&&!row.current));assert(qa.LEGACY_MODE_CASES.some(row=>row.mode==='prose'&&!row.current));
});
test('legacy native oracle checks exact current text, true display mode, document defaults and baseline structure',()=>{for(const row of qa.LEGACY_MODE_CASES){const value=legacyModeView(row);assert(qa.assertLegacyNativeMode(value,row,row.current?undefined:legacyModeView(row)));}});
for(const [name,change]of Object.entries({oldText:v=>v.formulas[0].text='x',wrongMode:v=>v.formulas[0].display=false,oldDefaults:v=>v.mathEffective.smallFrac=true,duplicate:v=>v.formulas.push(clone(v.formulas[0])),payloadOnly:v=>v.sourcePayloadUsedAsNativeEvidence=true,hiddenBody:v=>v.sections[0].paragraphs=[]}))test('stale capsule oracle rejects '+name,()=>{const row={name:'stale-prose',tex:'y',mode:'prose',gap:'',current:false},value=legacyModeView(row),baseline=clone(value);if(name==='wrongMode')row.mode='display';if(name==='wrongMode'){value.formulas[0].display=true;value.formulas[0].localAlignment='centerGroup';}change(value);assert.throws(()=>qa.assertLegacyNativeMode(value,row,baseline));});
function migrationReply(source,metadata){const from=source.indexOf(metadata),content=source.slice(0,from)+source.slice(from+metadata.length);return {ok:true,migration:{revision:hash(source),content,edits:[{from,to:from+metadata.length,insert:''}],recoveries:[{from:0,to:from+metadata.length,code:'math_metadata_stale',formula_id:'legacy-native-display-mode',content:qa.fence('row(office.mathPr(office.smallFrac(), office.preSp(val: "120")), office.oMathPara(office.oMath(x)))')}],diagnostics:[{code:'math_metadata_stale',from,to:from+metadata.length}]}};}
test('stale migration oracle preserves current literal expression and readable old recovery with exact edits',()=>{const metadata=metadataFor(qa.legacyDisplayOmml()),source='$$y$$'+metadata+'\n',reply=migrationReply(source,metadata);assert.equal(qa.assertLegacyMigration(reply,source,metadata,'legacy-native-display-mode').content,'$$y$$\n');});
for(const [name,change]of Object.entries({stale:v=>v.migration.revision='wrong',reactivated:v=>v.migration.content='$$x$$\n',lostRecovery:v=>v.migration.recoveries=[],wrongIdentity:v=>v.migration.recoveries[0].formula_id='other',hiddenRecovery:v=>v.migration.recoveries[0].content='<!-- smallFrac preSp -->',wrongDiagnostic:v=>v.migration.diagnostics[0].code='other'}))test('stale migration guard rejects '+name,()=>{const metadata=metadataFor(qa.legacyDisplayOmml()),source='$$y$$'+metadata+'\n',reply=migrationReply(source,metadata);change(reply);assert.throws(()=>qa.assertLegacyMigration(reply,source,metadata,'legacy-native-display-mode'));});
test('nested mathPr negative fixture is genuine nested native XML, not a forged producer capsule',()=>{const xml=qa.nestedMathPrOmml();assert(xml.includes('<m:oMath><m:mathPr>'));assert(xml.includes('<m:smallFrac/>'));assert(!xml.includes('readmd:math:'));});

// Exact source and structured response from candidate33/stdio-2HRJCc.
// Owned native input; this self-contained fixture never reads local .cache.
const candidate33NumberingSource="---\nreadmd:\n  publication:\n    footnotes:\n      kinds:\n        endnote:\n          size: 11.0\n          numberFormat: \"lowerLetter\"\n          start: 3\n          placement: \"docEnd\"\n          separator: \"line\"\n          continuationSeparator: \"line\"\n          continuationNotice: \"\"\n          longNote: \"keepParagraphs\"\n          lineSpacing:\n            mode: \"atLeast\"\n            pt: 13.0\n          restart: \"continuous\"\n        footnote:\n          size: 10.0\n          numberFormat: \"upperRoman\"\n          start: 7\n          placement: \"pageBottom\"\n          separator: \"none\"\n          continuationSeparator: \"line\"\n          continuationNotice: \"NATIVE_CONTINUED <next> & evidence\"\n          longNote: \"split\"\n          lineSpacing:\n            mode: \"exact\"\n            pt: 10.5\n          restart: \"section\"\n      kindByLabel:\n        \"word-endnote-7\": \"endnote\"\n        \"word-footnote-7\": \"footnote\"\n        \"word-footnote-8\": \"footnote\"\n    version: 1\n---\n<!-- readmd-section:v1 {\"id\":\"s80efb847ba\",\"break\":\"continuous\",\"footnotes\":{\"kinds\":{\"footnote\":{\"placement\":\"beneathText\"}}}} -->\n\n# SECTION_SAME_HEADING {#docx-9c8a69673f1be0deb28d2507}\n\nSECTION_A_BODY [^word-footnote-7] repeated [^word-footnote-7] end [^word-endnote-7]<!-- -->[SECTION_EVIDENCE](<https://example.test/section-evidence>)\n\n```\nSECTION_A_CODE_END\n```\n\n<!-- readmd-section:v1 {\"id\":\"s32e5974488\",\"break\":\"nextPage\",\"footnotes\":{\"kinds\":{\"footnote\":{\"placement\":\"pageBottom\"}}}} -->\n\n# SECTION_SAME_HEADING {#docx-e75dda1971de78ebf624cf82}\n\nSECTION_B_BODY [^word-footnote-8] repeated [^word-footnote-8] end [^word-endnote-7]<!-- -->[SECTION_EVIDENCE](<https://example.test/section-evidence>)\n\n[^word-footnote-7]: <!-- readmd:layout v1 {\"id\":\"note-f-3782cb4da678431a8f55fc5dbb4256a357dd41f15c8c8161a193fc934ec1f54e\",\"paragraph\":{\"keepNext\":true}} -->\n\n    NATIVE_FOOT_BODY Evidence 😀&#46;\n\n[^word-endnote-7]: <!-- readmd:layout v1 {\"id\":\"note-e-c94353a791ac23f2c2e931e487552cc4c3fc4ff83e990254d695bd1d63083d9d\",\"paragraph\":{\"keepNext\":true}} -->\n\n    NATIVE_END_BODY Evidence 😀&#46;\n\n[^word-footnote-8]: <!-- readmd:layout v1 {\"id\":\"note-f-410fcc5a5256842d9bd1b3f8b06d7fe3f030390e217c07dd079cba1e5a1acd0b\",\"paragraph\":{\"keepNext\":true}} -->\n\n    NATIVE_SECOND_SECTION_NOTE Distinct placement&#46;";
const candidate33NumberingResponse={
  "ok": true,
  "transaction": {
    "base_revision": "8ddc3bd89f755b148a63bd23b3904c32ae46dbdf48462fe398c562a2c35fbd6b",
    "origin": "editor",
    "edits": [
      {
        "from": 962,
        "to": 962,
        "insert": "\n<!-- readmd-numbering:v1 {\"headings\":{\"levels\":[{\"format\":\"decimal\",\"start\":2}]}} -->\n\n"
      }
    ]
  },
  "result": {
    "content": "---\nreadmd:\n  publication:\n    footnotes:\n      kinds:\n        endnote:\n          size: 11.0\n          numberFormat: \"lowerLetter\"\n          start: 3\n          placement: \"docEnd\"\n          separator: \"line\"\n          continuationSeparator: \"line\"\n          continuationNotice: \"\"\n          longNote: \"keepParagraphs\"\n          lineSpacing:\n            mode: \"atLeast\"\n            pt: 13.0\n          restart: \"continuous\"\n        footnote:\n          size: 10.0\n          numberFormat: \"upperRoman\"\n          start: 7\n          placement: \"pageBottom\"\n          separator: \"none\"\n          continuationSeparator: \"line\"\n          continuationNotice: \"NATIVE_CONTINUED <next> & evidence\"\n          longNote: \"split\"\n          lineSpacing:\n            mode: \"exact\"\n            pt: 10.5\n          restart: \"section\"\n      kindByLabel:\n        \"word-endnote-7\": \"endnote\"\n        \"word-footnote-7\": \"footnote\"\n        \"word-footnote-8\": \"footnote\"\n    version: 1\n---\n<!-- readmd-numbering:v1 {\"headings\":{\"levels\":[{\"format\":\"decimal\",\"start\":2}]}} -->\n\n\n<!-- readmd-section:v1 {\"id\":\"s80efb847ba\",\"break\":\"continuous\",\"footnotes\":{\"kinds\":{\"footnote\":{\"placement\":\"beneathText\"}}}} -->\n\n# SECTION_SAME_HEADING {#docx-9c8a69673f1be0deb28d2507}\n\nSECTION_A_BODY [^word-footnote-7] repeated [^word-footnote-7] end [^word-endnote-7]<!-- -->[SECTION_EVIDENCE](<https://example.test/section-evidence>)\n\n```\nSECTION_A_CODE_END\n```\n\n<!-- readmd-section:v1 {\"id\":\"s32e5974488\",\"break\":\"nextPage\",\"footnotes\":{\"kinds\":{\"footnote\":{\"placement\":\"pageBottom\"}}}} -->\n\n# SECTION_SAME_HEADING {#docx-e75dda1971de78ebf624cf82}\n\nSECTION_B_BODY [^word-footnote-8] repeated [^word-footnote-8] end [^word-endnote-7]<!-- -->[SECTION_EVIDENCE](<https://example.test/section-evidence>)\n\n[^word-footnote-7]: <!-- readmd:layout v1 {\"id\":\"note-f-3782cb4da678431a8f55fc5dbb4256a357dd41f15c8c8161a193fc934ec1f54e\",\"paragraph\":{\"keepNext\":true}} -->\n\n    NATIVE_FOOT_BODY Evidence 😀&#46;\n\n[^word-endnote-7]: <!-- readmd:layout v1 {\"id\":\"note-e-c94353a791ac23f2c2e931e487552cc4c3fc4ff83e990254d695bd1d63083d9d\",\"paragraph\":{\"keepNext\":true}} -->\n\n    NATIVE_END_BODY Evidence 😀&#46;\n\n[^word-footnote-8]: <!-- readmd:layout v1 {\"id\":\"note-f-410fcc5a5256842d9bd1b3f8b06d7fe3f030390e217c07dd079cba1e5a1acd0b\",\"paragraph\":{\"keepNext\":true}} -->\n\n    NATIVE_SECOND_SECTION_NOTE Distinct placement&#46;",
    "revision": "a3a91d3fdc088dae1157541b1d66c89b40b60de2da24f5289b44416dcdfee0c0",
    "changed": true,
    "inverse": [
      {
        "from": 962,
        "to": 1050,
        "insert": ""
      }
    ]
  },
  "config": {
    "headings": {
      "levels": [
        {
          "format": "decimal",
          "start": 2
        }
      ]
    }
  },
  "plan": {
    "config": {
      "headings": {
        "levels": [
          {
            "format": "decimal",
            "start": 2,
            "restartAfter": null
          }
        ],
        "separator": ".",
        "suffix": "space"
      },
      "captions": {}
    },
    "from": 963,
    "to": 1049,
    "headings": [
      {
        "ordinal": 0,
        "level": 1,
        "id": "docx-9c8a69673f1be0deb28d2507",
        "from": 1184,
        "to": 1240,
        "numbering": {
          "numbers": [
            2
          ],
          "text": "2",
          "suffix": " ",
          "rules": [
            {
              "format": "decimal",
              "start": 2,
              "restart_after": 0,
              "text": "%1",
              "suffix": "space",
              "is_legal": false
            }
          ]
        },
        "unnumbered": false
      },
      {
        "ordinal": 1,
        "level": 1,
        "id": "docx-e75dda1971de78ebf624cf82",
        "from": 1551,
        "to": 1607,
        "numbering": {
          "numbers": [
            3
          ],
          "text": "3",
          "suffix": " ",
          "rules": [
            {
              "format": "decimal",
              "start": 2,
              "restart_after": 0,
              "text": "%1",
              "suffix": "space",
              "is_legal": false
            }
          ]
        },
        "unnumbered": false
      }
    ]
  }
};
test('actual candidate33 source and response distinguish edit insertion from the comment plan range',()=>{
  assert.equal(hash(candidate33NumberingSource),'8ddc3bd89f755b148a63bd23b3904c32ae46dbdf48462fe398c562a2c35fbd6b');
  assert.equal(candidate33NumberingResponse.transaction.edits[0].from,962);assert.equal(candidate33NumberingResponse.plan.from,963);
  assert.equal(candidate33NumberingResponse.result.revision,'a3a91d3fdc088dae1157541b1d66c89b40b60de2da24f5289b44416dcdfee0c0');
  assert.equal(qa.assertAuxNumbering(clone(candidate33NumberingResponse),candidate33NumberingSource),candidate33NumberingResponse.result.content);
});
// Minimal source excerpt retains the observed version/closing-delimiter/Section
// boundary. Unrelated note policy leaves and later body are removed explicitly.
const observedNumberingBoundary='---\nreadmd:\n  publication:\n    version: 1\n---\n<!-- readmd-section:v1 {"id":"s80efb847ba","break":"continuous","footnotes":{"kinds":{"footnote":{"placement":"beneathText"}}}} -->\n\n# SECTION_SAME_HEADING {#docx-9c8a69673f1be0deb28d2507}\n';
for(const [label,eol]of [['LF','\n'],['CRLF','\r\n']])test('observed source boundary allows an inserted leading '+label+' before the original closing-line ending',()=>{const source=observedNumberingBoundary.replaceAll('\n',eol),at=qa.sourceHeaderEnd(source)-eol.length,insert=eol+'<!-- readmd-numbering:v1 '+JSON.stringify(qa.AUX_NUMBERING_CONFIG)+' -->'+eol+eol,value=numberingReply(source,{at,insert});assert.equal(value.plan.from,at+eol.length);assert.equal(qa.assertAuxNumbering(value,source),value.result.content);});
test('before-ending insertion still requires the actual declaration to start on its own line',()=>{const source=observedNumberingBoundary,at=qa.sourceHeaderEnd(source)-1;assert.throws(()=>qa.assertAuxNumbering(numberingReply(source,{at}),source));});
test('a typed transaction may not split the original CRLF boundary',()=>{const source=observedNumberingBoundary.replaceAll('\n','\r\n'),at=qa.sourceHeaderEnd(source)-1,insert='\n<!-- readmd-numbering:v1 '+JSON.stringify(qa.AUX_NUMBERING_CONFIG)+' -->\r\n\r\n';assert.throws(()=>qa.assertAuxNumbering(numberingReply(source,{at,insert}),source));});

// Exact native conversion returned in candidate34/stdio-EAK0D8; no .cache dependency.
const candidate34RawMathMissing="MATH_SOURCE_FORMULA_MISSING\n\n$$FORMULA_MISSING\\frac{a}{b}\\int_{0}^{1} x\\sum_{0}^{1} x$$<!--readmd:math:v1:H4sIAAAAAAAE/82Tz27UMBDGX2Xlc5d482eTWEskpApUiYUVFbeVKseeNJHiONhO04K4c+U5eCuehHE2LdstEghx4JLJeH7z5YvH/kSsqEFxwlZnpJGEEfiwzMN1TilNlryKkmVMo2zJkzBfClyNZbpeA6zJGbF6MAKuDNw0ttEd9ua5jCNI4hjCMitpTvNMZjSlaSziSEAioYrSMKE0LUMuJF+JjEZxGOcSsDPnKKqValFpMzLr7lqwi1vVdpaNz/ekdq5nQXAwbJ/pHjosVtoo7jA118GojeyNFmBt012rNggpXQeKN92eFCgptTiHig+tsz41OzOnc+aD/bgY2Q1v8YNhuCdBsQlGhuR9fOjA5Ud6mB8sFxvF9Ja7escNn/1zlPu9f2n4+MT4rKD+SEFXVSPgXItBQefu/9/VezLLmL+WMdByh3O2ddPbB71/MZh5u/y+Gf9wxcu377bvX7+42l5cXl68ebUJFHM4AMWmeuWhblA+TAuu4I8QxOeyhO6IKk+pqYz0QZGbOw93GHeTrqjNQs2H4fuXb4fToNhPwA6l75hgV9BT+blsh/6IWj2lpjIcMbenDBbRpv+wp47jr5x+/U+couXpIkyjm978lSjI5x+L6CLGdwQAAA==-->";
test('actual candidate34 native missing-default output is a current legacy capsule, not a DSL fence',()=>{
  assert.equal(qa.mathBlocks(candidate34RawMathMissing).length,0);const comment=candidate34RawMathMissing.match(/<!--readmd:math:v1:[A-Za-z0-9+/=]+-->/g);assert.equal(comment.length,1);
  const value=qa.decodeLegacyMathMetadata(comment[0]),from=candidate34RawMathMissing.indexOf('$$'),to=candidate34RawMathMissing.indexOf('$$',from+2),tex=candidate34RawMathMissing.slice(from+2,to);assert.equal(value.source_revision,hash(tex));assert(!value.omml.includes('<m:mathPr'));assert(value.omml.includes('<m:oMathPara'));
});
function readableReply34(){const source=candidate34RawMathMissing,from=source.indexOf('$$'),to=source.indexOf('-->',from)+3,insert=qa.fence('row(office.oMathPara(office.oMath(x)))').trimEnd(),edits=[{from,to,insert}];return {ok:true,migration:{revision:hash(source),content:qa.applyEdits(source,edits),edits,recoveries:[],diagnostics:[]}};}
test('readable migration validates real original carrier digest, source ranges and exact returned content',()=>{const value=qa.assertReadableMathMigration(readableReply34(),candidate34RawMathMissing);assert.equal(value.content,readableReply34().migration.content);assert.equal(value.carrier.source_revision,hash(value.carrier.tex));assert.equal(value.block.source,'row(office.oMathPara(office.oMath(x)))');});
test('readable migration rejects stale revision, wrong ranges, altered content and any unreviewed diagnostic/recovery',()=>{for(const change of [v=>v.migration.revision='wrong',v=>v.migration.edits[0].from++,v=>v.migration.content+='hidden',v=>v.migration.diagnostics.push({code:'math_migration_loss'}),v=>v.migration.recoveries.push({code:'math_metadata_stale'}),v=>v.migration.edits[0].to--]){const value=readableReply34();change(value);assert.throws(()=>qa.assertReadableMathMigration(value,candidate34RawMathMissing));}});
test('migration idempotence requires unchanged exact source, revision, empty edits and no diagnostics/recoveries',()=>{const source=qa.fence('office.oMath(x)'),value={ok:true,migration:{revision:hash(source),content:source,edits:[],diagnostics:[],recoveries:[]}};qa.assertIdempotentMathMigration(value,source);for(const change of [v=>v.migration.revision='stale',v=>v.migration.content+='x',v=>v.migration.edits.push({from:0,to:0,insert:''}),v=>v.migration.diagnostics.push({code:'hidden'}),v=>v.migration.recoveries.push({code:'stale'})]){const bad=clone(value);change(bad);assert.throws(()=>qa.assertIdempotentMathMigration(bad,source));}});
test('migration tree comparison covers the whole XML context plus actual native math objects',()=>{const object={name:'oMathPara',attributes:{},content:[{element:{name:'oMath',attributes:{},content:[{text:'x'}]}}]},before={semanticTree:{name:'qa',attributes:{},content:[{element:{name:'styles',attributes:{color:'336699'},content:[]}},{element:object}]},semanticTreeSha256:'a'.repeat(64),mathObjects:[object],mathProperties:{}},after=clone(before),native={mathObjects:clone(before.mathObjects),mathProperties:{}};qa.assertMathMigrationTrees(before,after,native);for(const change of [v=>v.semanticTree.content[0].element.attributes.color='other',v=>v.mathObjects[0].content=[],v=>v.mathProperties.smallFrac={state:'empty',value:null},v=>delete v.semanticTree,v=>v.semanticTreeSha256='b'.repeat(64)]){const bad=clone(after);change(bad);assert.throws(()=>qa.assertMathMigrationTrees(before,bad,native));}});

// Exact public source/response from candidate41/stdio-DSqxJ6 request 23.
// No .cache file or historical executable is needed by these pure controls.
const candidate41PlainSource="---\nreadmd:\n  publication:\n    typography:\n      size: 10.5\n    version: 1\n---\n# <span data-readmd-text=\"v1\" data-size-pt=\"18\" data-size-cs-pt=\"18\">Ordinary document</span> {#docx-8d8b9a19d20ab0f8336e7afc}\n\nNO_NOTE_NO_POLICY&#46;";
const candidate41PlainProfile={"ok":true,"revision":"b7b11c0ad7b154deedb0241f5a38d83efd21dd27b4fed4da57d2c846935da32d","sourceValues":{"typography":{"size":10.5},"version":1},"ranges":{"readmd.publication":{"from":31,"to":74}},"options":{"typography":{"size":10.5}},"diagnostics":[]};
test('actual candidate41 no-note native import owns exactly its consumed 10.5pt body baseline',()=>{
  assert.equal(hash(candidate41PlainSource),'b7b11c0ad7b154deedb0241f5a38d83efd21dd27b4fed4da57d2c846935da32d');
  assert.throws(()=>assert.deepEqual(candidate41PlainProfile.sourceValues,{}),'The old empty-profile oracle must fail on the retained real response');
  assert.equal(qa.assertNoNoteProfile(clone(candidate41PlainProfile),candidate41PlainSource,10.5).sourceValues.typography.size,10.5);
});
test('an explicitly expected native 11pt baseline is preserved without weakening the 10.5pt case',()=>{
  const source='---\nreadmd:\n  publication:\n    version: 1\n    typography: {size: 11}\n---\nBODY_NATIVE_ELEVEN.\n',value={ok:true,revision:hash(source),sourceValues:{version:1,typography:{size:11}},options:{typography:{size:11}},diagnostics:[]};
  assert.equal(qa.assertNoNoteProfile(value,source,11).sourceValues.typography.size,11);
  assert.throws(()=>qa.assertNoNoteProfile(value,source,10.5),'Minimal must not silently become 11pt');
});
for(const [name,change]of Object.entries({
  staleRevision:value=>value.revision='0'.repeat(64),
  emptyProfile:value=>value.sourceValues={},
  missingSize:value=>delete value.sourceValues.typography.size,
  wrongMinimalSize:value=>{value.sourceValues.typography.size=11;value.options.typography.size=11;},
  lexicalSize:value=>value.sourceValues.typography.size='10.5',
  wrongVersion:value=>value.sourceValues.version=2,
  extraNotePolicy:value=>value.sourceValues.footnotes={},
  extraNumberingProfile:value=>value.sourceValues.numbering={headings:{}},
  unknownSourceLeaf:value=>value.sourceValues.future=true,
  extraTypographyLeaf:value=>value.sourceValues.typography.lineHeight=1.5,
  extraNoteOptions:value=>value.options.footnotes={},
  unknownOptionsLeaf:value=>value.options.future=true,
  diagnostics:value=>value.diagnostics.push({code:'unreviewed'}),
}))test('no-note baseline rejects '+name,()=>{const value=clone(candidate41PlainProfile);change(value);assert.throws(()=>qa.assertNoNoteProfile(value,candidate41PlainSource,10.5));});
test('no-note source numbering requires an actual revision-bound empty get result',()=>{
  const value={ok:true,revision:hash(candidate41PlainSource),config:null,plan:null};assert.equal(qa.assertNoSourceNumbering(value,candidate41PlainSource),value);
});
for(const [name,change]of Object.entries({
  falseOk:value=>value.ok=false,
  staleRevision:value=>value.revision='0'.repeat(64),
  missingConfig:value=>delete value.config,
  hiddenConfig:value=>value.config={headings:{levels:[{format:'decimal',start:2}]}},
  missingPlan:value=>delete value.plan,
  hiddenPlan:value=>value.plan={headings:[]},
}))test('no-note numbering rejects '+name,()=>{const value={ok:true,revision:hash(candidate41PlainSource),config:null,plan:null};change(value);assert.throws(()=>qa.assertNoSourceNumbering(value,candidate41PlainSource));});

// Actual candidate44 ordinary mixed-note returned source; it is now a positive,
// separate from the still-invalid/foreign native number-format cases.
const mixedNoteSource="[^word-footnote-7]: <!-- readmd:layout v1 {\"id\":\"note-f-3782cb4da678431a8f55fc5dbb4256a357dd41f15c8c8161a193fc934ec1f54e\",\"paragraph\":{\"keepNext\":true}} -->\n\n    NATIVE_FOOT_BODY Evidence 😀&#46; MIXED_TEN<span data-readmd-text=\"v1\" data-size-pt=\"30\" data-size-cs-pt=\"30\">&#32;MIXED_THIRTY</span>\n\n[^word-endnote-7]: <!-- readmd:layout v1 {\"id\":\"note-e-c94353a791ac23f2c2e931e487552cc4c3fc4ff83e990254d695bd1d63083d9d\",\"paragraph\":{\"keepNext\":true}} -->\n\n    NATIVE_END_BODY Evidence 😀&#46;\n\n[^word-footnote-8]: <!-- readmd:layout v1 {\"id\":\"note-f-410fcc5a5256842d9bd1b3f8b06d7fe3f030390e217c07dd079cba1e5a1acd0b\",\"paragraph\":{\"keepNext\":true}} -->\n\n    NATIVE_SECOND_SECTION_NOTE Distinct placement&#46;";

function mixedProfile(source=mixedNoteSource){return {ok:true,revision:hash(source),sourceValues:{version:1,footnotes:{kinds:{footnote:{size:10},endnote:{size:11}},kindByLabel:{'word-footnote-7':'footnote','word-footnote-8':'footnote','word-endnote-7':'endnote'}}}};}
function mixedNative(){const current={kind:'docx-settings-roundtrip',sourcePayloadUsedAsNativeEvidence:false,wordApplicationRoundtripCertified:false,notes:{}};for(const kind of ['footnote','endnote']){const ids=kind==='footnote'?['17','18']:['27'];current.notes[kind]={part:'word/'+kind+'s.xml',references:kind==='footnote'?[ids[0],ids[0],ids[1],ids[1]]:[ids[0],ids[0]],settings:{values:kind==='footnote'?{pos:'pageBottom',numFmt:'upperRoman',numStart:'7',numRestart:'eachSect'}:{pos:'docEnd',numFmt:'lowerLetter',numStart:'3',numRestart:'continuous'}},selectedSpecial:kind==='footnote'?[{type:'separator',text:'',separatorElements:[]},{type:'continuationSeparator',text:'',separatorElements:['continuationSeparator']},{type:'continuationNotice',text:'NATIVE_CONTINUED <next> & evidence',separatorElements:[]}]:[{type:'separator',text:'',separatorElements:['separator']},{type:'continuationSeparator',text:'',separatorElements:['continuationSeparator']}],definitions:qa.MIXED_NOTE_TEXT[kind].map((body,index)=>{const at=body.indexOf(' MIXED_THIRTY'),prefix=' '+(at<0?body:body.slice(0,at)),runs=[{text:prefix,properties:{sz:kind==='footnote'?'20':'22',szCs:kind==='footnote'?'20':'22'}}];if(at>=0)runs.push({text:body.slice(at),properties:{sz:'60',szCs:'60'}});return {id:ids[index],type:'normal',text:' '+body,paragraphs:[{text:' '+body,fields:[],runs}]};})};}return current;}
test('candidate44 actual readable mixed note source retains both author axes and every current story',()=>{assert.equal(qa.assertMixedNoteSource(mixedNoteSource,mixedProfile()).westernAndComplexAxes,true);assert(qa.QA_FILES.includes(qa.SETTINGS_ORACLE));assert(qa.PRODUCT_FILES.includes('rust/readmd-kernel/src/convert/docx_text_style.rs'));});
for(const [name,change]of Object.entries({wrongWestern:source=>source.replace('data-size-pt="30"','data-size-pt="10"'),wrongComplex:source=>source.replace('data-size-cs-pt="30"','data-size-cs-pt="10"'),missingComplex:source=>source.replace(' data-size-cs-pt="30"',''),plainMarker:source=>source.replace(/<span[^>]*>(&#32;MIXED_THIRTY)<\/span>/,'$1'),missingText:source=>source.replace(' MIXED_TEN',''),foreignSpan:source=>source.replace('data-readmd-text="v1"','data-foreign="v1"'),liveRoute:source=>source.replace('data-size-pt="30"','data-size-pt="30" data-cs="true"'),codeExample:source=>'~~~~markdown\n'+source+'\n~~~~\n',quotedExample:source=>source.split('\n').map(line=>'> '+line).join('\n')}))test('mixed note source oracle rejects '+name,()=>{const source=change(mixedNoteSource);assert.throws(()=>qa.assertMixedNoteSource(source,mixedProfile(source)));});
test('mixed note source requires independent current kind bindings and unchanged global base axes',()=>{for(const change of [p=>p.sourceValues.footnotes.kindByLabel['word-footnote-7']='endnote',p=>p.sourceValues.footnotes.kinds.footnote.sizeCs=30,p=>p.sourceValues.footnotes.kinds.endnote.size=30,p=>p.revision='stale']){const profile=mixedProfile();change(profile);assert.throws(()=>qa.assertMixedNoteSource(mixedNoteSource,profile));}});
test('native mixed note oracle accepts changed native IDs with exact text, axes and repeated identity',()=>{assert.equal(qa.assertMixedNoteNative(mixedNative()).westernAndComplexAxes,true);});
for(const [name,change]of Object.entries({wrongWestern:v=>v.notes.footnote.definitions[0].paragraphs[0].runs[1].properties.sz='20',wrongComplex:v=>v.notes.footnote.definitions[0].paragraphs[0].runs[1].properties.szCs='20',missingComplex:v=>delete v.notes.footnote.definitions[0].paragraphs[0].runs[1].properties.szCs,endnotePollution:v=>v.notes.endnote.definitions[0].paragraphs[0].runs[0].properties.szCs='60',lostAuthorText:v=>v.notes.footnote.definitions[0].text=' NATIVE_FOOT_BODY',duplicateDefinition:v=>v.notes.footnote.definitions.push(clone(v.notes.footnote.definitions[0])),splitRepeat:v=>v.notes.footnote.references[1]='18',lostSecondStory:v=>v.notes.endnote.definitions=[],changedContinuation:v=>v.notes.footnote.selectedSpecial[2].text='other',changedNumber:v=>v.notes.footnote.settings.values.numStart='1',lostRunCoverage:v=>v.notes.footnote.definitions[0].paragraphs[0].runs.pop(),liveField:v=>v.notes.footnote.definitions[0].paragraphs[0].fields.push({instruction:'REF changed'}),payloadEvidence:v=>v.sourcePayloadUsedAsNativeEvidence=true}))test('native mixed note oracle rejects '+name,()=>{const value=mixedNative();change(value);assert.throws(()=>qa.assertMixedNoteNative(value));});

test('mixed note native placement omission is the equivalent Word default, not a lost section override',()=>{const value=mixedNative();delete value.notes.footnote.settings.values.pos;assert.equal(qa.assertMixedNoteNative(value).westernAndComplexAxes,true);value.notes.footnote.settings.values.pos='beneathText';assert.throws(()=>qa.assertMixedNoteNative(value));});
