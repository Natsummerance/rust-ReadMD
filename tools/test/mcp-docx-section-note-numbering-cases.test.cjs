'use strict';
// Deterministic negative controls for the oracle consumers; no product host.
const test=require('node:test'),assert=require('node:assert/strict');
const cases=require('./mcp-docx-section-note-numbering-cases.cjs'),settings=require('./mcp-docx-settings-roundtrip.cjs');
const W='http://schemas.openxmlformats.org/wordprocessingml/2006/main';
function native(){
  const leaf=(name,val,format)=>({name:`{${W}}${name}`,val,format:format??null,formatPresent:format!==undefined});
  const effective={numFmt:'upperRoman',numStart:'7',numRestart:'eachSect',numberPattern:null};
  const sparse={children:[leaf('numFmt','upperRoman'),leaf('numStart','7'),leaf('numRestart','eachSect')]};
  const definition=kind=>({id:'7',type:'normal',text:kind==='footnote'?'NATIVE_FOOT_BODY':'NATIVE_END_BODY'});
  return {sourcePayloadUsedAsNativeEvidence:false,nativeSections:1,sectionScopes:[{ordinal:0,terminatorPath:[1],text:'P06C_SCOPE_00',notes:Object.fromEntries(['footnote','endnote'].map(kind=>[kind,{sparse:structuredClone(sparse),effective:{...effective},references:['7','7']}]))}],notes:Object.fromEntries(['footnote','endnote'].map(kind=>[kind,{definitions:[definition(kind)],references:['7','7']}]))};
}
test('native comparisons prove sparse presence, format attributes and per-story reference identities',()=>{
  const expected=native();cases.assertNativeReturn(expected,structuredClone(expected));
  for(const mutation of [value=>value.sectionScopes[0].notes.footnote.sparse.children.pop(),value=>value.sectionScopes[0].notes.endnote.effective.numStart='1',value=>value.sectionScopes[0].notes.footnote.sparse.children[0].formatPresent=true,value=>value.sectionScopes[0].notes.footnote.sparse.children[0].format='A',value=>value.notes.endnote.references.pop(),value=>value.sectionScopes[0].notes.footnote.references.pop(),value=>value.sectionScopes[0].text='Wrong terminator content']){
    const value=structuredClone(expected);mutation(value);assert.throws(()=>cases.assertNativeReturn(expected,value));
  }
});
test('independent numbering leaf control rejects missing val, wrong order and an out-of-range UInt16',()=>{
  const props=native().sectionScopes[0].notes.footnote.sparse;
  for(const mutation of [value=>value.children[0].val=null,value=>value.children.reverse(),value=>value.children[1].val='65536',value=>value.children[1].val='1.5',value=>value.children[2].val='eachChapter']){const value=structuredClone(props);mutation(value);assert.throws(()=>cases.numberedChildren(value));}
  assert.equal(cases.numberedChildren({...props,children:props.children.map(row=>row.name.endsWith('numStart')?{...row,val:'0'}:row)})[1].val,'0');
});
test('readable declarations are taken from actual top-level Markdown HTML tokens',()=>{
  const marker='<!-- readmd-section:v1 {"id":"a","footnotes":{"kinds":{"footnote":{"start":0,"restart":"continuous"}}}} -->';
  const helpers={markdownTokens:settings.markdownTokens};assert.equal(cases.sourceSections(marker+'\n\nBody.\n',helpers)[0].footnotes.kinds.footnote.start,0);
  for(const source of ['```html\n'+marker+'\n```','> '+marker,'    '+marker])assert.deepEqual(cases.sourceSections(source,helpers),[]);
});
test('authored P06-C cases keep source-owned sparse defaults, BOM/CRLF and pattern-only input',()=>{
  const edge=cases.authoredSource('edge'),pattern=cases.authoredSource('pattern');assert(edge.startsWith('\uFEFF---\r\n'));assert(!/(?<!\r)\n/.test(edge));assert(edge.includes('Prefix 😀'));assert(edge.includes('# KEEP_P06C'));assert(edge.includes('P06C_CODE_END'));
  const declarations=cases.sourceSections(pattern,{markdownTokens:settings.markdownTokens});assert.equal(declarations.length,3);assert.deepEqual(declarations[0].footnotes.kinds.footnote,{numberPattern:'LOCAL_A',start:27});assert.equal(declarations[1].footnotes.kinds.footnote.numberFormat,'decimal');assert(!Object.hasOwn(declarations[1].footnotes.kinds.footnote,'numberPattern'));assert(!Object.hasOwn(declarations[2],'footnotes'));
});
test('large reopened Markdown is read in valid product chunks without increasing its 1000-line bound',()=>{
  const source='\uFEFF'+Array.from({length:2001},(_,index)=>'line'+index+'\r\n').join('');const chunks=settings.sourceReadChunks('owned.md',source);assert.deepEqual(chunks,[{file_path:'owned.md',start_line:1,end_line:1000},{file_path:'owned.md',start_line:1001,end_line:2000},{file_path:'owned.md',start_line:2001,end_line:2001}]);assert.throws(()=>settings.sourceReadArguments('owned.md',source),/read bound/);
});
test('P06-C plan is bounded and old placement-only vocabulary limitation is removed',()=>{
  const plan=settings.planFor();assert.equal(plan.sectionNoteNumbering.fixedSchemaFormats,63);assert.equal(plan.sectionNoteNumbering.formatMatrixKinds,2);assert.equal(plan.sectionNoteNumbering.fullT01ThroughT14Certified,false);assert(!plan.openCases.some(value=>value.includes('placement-only source vocabulary')));assert(plan.openCases.some(value=>value.includes('endnote placement/suppression')));assert(settings.QA_FILES.includes('tools/test/mcp-docx-section-note-numbering-cases.cjs'));assert(settings.QA_FILES.includes('tools/test/mcp-docx-section-note-numbering-cases.test.cjs'));
  assert.equal(settings.argumentsFor(['--plan','--output',settings.SECTION_BASE+'/owned-run'],{}).output,require('node:path').resolve(settings.SECTION_BASE,'owned-run'));
});
test('uniform EPUB inputs declare actual section-owned five-format counters and stable repeated labels',()=>{
  for(const format of Object.keys(cases.VISIBLE_NUMBERS))for(const restart of ['continuous','section']){
    const source=cases.uniformSource(format,restart),sections=cases.sourceSections(source,{markdownTokens:settings.markdownTokens});assert.equal(sections.length,2);for(const section of sections)assert.deepEqual(section.footnotes.kinds.footnote,{numberFormat:format,start:27,restart});assert.equal((source.match(/\[\^u1\](?!:)/g)||[]).length,2);assert.equal((source.match(/\[\^u[1-4]\]:/g)||[]).length,4);
  }
  assert.deepEqual(cases.VISIBLE_NUMBERS.upperLetter,['AA','BB','CC','DD']);assert.deepEqual(cases.VISIBLE_NUMBERS.lowerRoman,['xxvii','xxviii','xxix','xxx']);
});
test('unused native numbering remains an explicit meaningful source case and invalid-source arrays are literal raw documents',()=>{
  const source=cases.authoredSource('unused'),sections=cases.sourceSections(source,{markdownTokens:settings.markdownTokens});assert.deepEqual(sections[0].footnotes.kinds.footnote,{numberFormat:'chicago',start:65535,restart:'continuous'});assert(!source.includes('[^'));
  assert(cases.INVALID_SOURCE.some(([name,raw])=>name==='notes-array'&&raw.includes('"footnotes":[]')));assert(cases.INVALID_SOURCE.some(([name,raw])=>name==='duplicate-null'&&raw.includes('"start":null,"start":1')));
});

function variedReply(format){const field='sections['+JSON.stringify(format==='tex'?'varied1':'varied2')+'].footnotes.kinds.footnote.numberFormat',value={ok:false,error_code:'publication_note_policy_unsupported',path:field,field};return {isError:true,structuredContent:value,content:[{type:'text',text:JSON.stringify(value)}]};}
test('varied numbering fails the shared source policy before any Rust PDF renderer-specific section guard',()=>{for(const format of ['html','tex','epub','pdf']){const reply=variedReply(format);assert.equal(cases.assertVariedPolicyError(reply,format,{toolValue:settings.toolValue}).error_code,'publication_note_policy_unsupported');}});
test('varied policy guard requires one exact code and exact path plus field, including the TeX default-only scope',()=>{for(const change of [v=>v.error_code='publication_sections_require_paged_engine',v=>v.error_code='publication_section_invalid',v=>v.path=v.field='sections["varied1"].footnotes.kinds.footnote.numberFormat',v=>v.path='sections["varied2"].footnotes.kinds.endnote.numberFormat',v=>v.field='sections',v=>delete v.path,v=>delete v.field]){const reply=variedReply('pdf');change(reply.structuredContent);reply.content[0].text=JSON.stringify(reply.structuredContent);assert.throws(()=>cases.assertVariedPolicyError(reply,'pdf',{toolValue:settings.toolValue}));}assert.throws(()=>cases.assertVariedPolicyError(variedReply('pdf'),'tex',{toolValue:settings.toolValue}));});
test('both overwrite attempts require unchanged target bytes, saved source bytes and exact directory membership',()=>{const expected={targetSha256:'old-target',sourceSha256:'saved-source',directoryMembers:['owned-old.pdf']};cases.assertProtectedState(structuredClone(expected),expected);for(const change of [v=>v.targetSha256='new-target',v=>v.sourceSha256='edited-source',v=>v.directoryMembers.push('sidecar'),v=>v.directoryMembers=[]]){const current=structuredClone(expected);change(current);assert.throws(()=>cases.assertProtectedState(current,expected));}assert.deepEqual(expected,{targetSha256:'old-target',sourceSha256:'saved-source',directoryMembers:['owned-old.pdf']});});
