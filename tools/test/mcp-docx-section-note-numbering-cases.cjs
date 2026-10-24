'use strict';
// P06-C cases inside the established settings driver. This module does not
// implement a transport, launch a host, or replace the independent OPC oracle.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const W='http://schemas.openxmlformats.org/wordprocessingml/2006/main';
const KINDS=['footnote','endnote'],NUMBER_FIELDS=['numFmt','numStart','numRestart'];
const INVALID_NATIVE=['p06c-invalid-start','p06c-missing-val','p06c-foreign-format','p06c-wrong-order'];
const INVALID_SOURCE=[
  ['notes-array','{"id":"invalid","footnotes":[]}'],['kinds-array','{"id":"invalid","footnotes":{"kinds":[]}}'],
  ['kind-array','{"id":"invalid","footnotes":{"kinds":{"endnote":[]}}}'],
  ['start-string','{"id":"invalid","footnotes":{"kinds":{"footnote":{"start":"7"}}}}'],
  ['start-bool','{"id":"invalid","footnotes":{"kinds":{"footnote":{"start":true}}}}'],
  ['start-fraction','{"id":"invalid","footnotes":{"kinds":{"footnote":{"start":1.5}}}}'],
  ['duplicate-null','{"id":"invalid","footnotes":{"kinds":{"footnote":{"start":null,"start":1}}}}'],
  ['section-size','{"id":"invalid","footnotes":{"kinds":{"footnote":{"size":10}}}}'],
];
function plan(){return {nativeOnlyMixedSections:3,fixedSchemaFormats:63,formatMatrixSections:63,formatMatrixKinds:2,startRepresentatives:[0,1,27,12000,65535],restartRepresentatives:['continuous','eachSect','eachPage'],formatCrossProductCertified:false,
  pipeline:'existing StdioClient and settings queue; real conversion -> explicit revision save -> normal EOF -> different PID read -> options:{} DOCX -> independent native inspection -> second import',
  nativeMutations:['p06c-section-change','p06c-section-clear','p06c-all-clear'],invalidNative:INVALID_NATIVE,invalidSource:INVALID_SOURCE.map(([name])=>name),
  sourceCases:['implicit prefix and empty section','CodeBlock terminator and duplicate headings','explicit default presence','pattern-only required val and format-only pattern clear','global patch inheritance and UTF-16 transaction inverse'],
  protectedConsumers:['html','tex','epub','pdf:typst','pdf:rust'],uniformEpubFormats:['decimal','lowerRoman','upperRoman','lowerLetter','upperLetter'],uniformEpubRestarts:['continuous','section'],htmlDefaultPositive:true,
  openCases:['Physical Word pagination/field refresh and custom-format display','Full 63 x 5 x 3 actual-host cross product','128-section/cancellation stress and stale/overlap exhaustiveness','Arbitrary native story rich-content and author-axis cross product'],
  endnoteEachPageWordDisplayCertified:false,wordGuiCertified:false,fullT01ThroughT14Certified:false};}
function numberedChildren(props){
  const children=(props?.children||[]).filter(row=>NUMBER_FIELDS.some(name=>row.name===`{${W}}${name}`));
  const ranks=children.map(row=>NUMBER_FIELDS.indexOf(row.name.slice(W.length+2)));
  assert(ranks.every((rank,index)=>index===0||rank>ranks[index-1]),'Native numbering children are duplicated or out of schema order');
  return children.map(row=>{
    assert.equal(typeof row.val,'string','Native required val is missing');
    const name=row.name.slice(W.length+2);
    if(name==='numStart')assert(/^\d+$/.test(row.val)&&Number(row.val)<=65535,'Native start is not an UInt16');
    if(name==='numRestart')assert(['continuous','eachSect','eachPage'].includes(row.val));
    return {name,val:row.val,...(name==='numFmt'?{formatPresent:row.formatPresent,format:row.format}: {})};
  });
}
function numberingSnapshot(value){
  assert.equal(value.sourcePayloadUsedAsNativeEvidence,false);
  assert(Array.isArray(value.sectionScopes)&&value.sectionScopes.length===value.nativeSections);
  return value.sectionScopes.map((scope,index)=>{
    assert.equal(scope.ordinal,index);assert(Array.isArray(scope.terminatorPath)&&scope.terminatorPath.length>0);
    return Object.fromEntries(KINDS.map(kind=>{const row=scope.notes[kind],effective=row.effective;return [kind,{sparse:numberedChildren(row.sparse),effective:Object.fromEntries(['numFmt','numStart','numRestart','numberPattern'].map(key=>[key,effective[key]]))}];}));
  });
}
function storySignature(value){
  return Object.fromEntries(KINDS.map(kind=>{
    const notes=value.notes[kind],definitions=notes.definitions.filter(row=>row.type==='normal');
    const markerFor=id=>{const rows=definitions.filter(row=>row.id===id);assert.equal(rows.length,1,'Unresolved or duplicate note identity');const text=rows[0].text;return ['NATIVE_FOOT_BODY','NATIVE_END_BODY','P06C_NEW_FOOTNOTE','P06C_NEW_ENDNOTE'].find(marker=>text.includes(marker))||text.trim();};
    return [kind,{definitions:definitions.map(row=>markerFor(row.id)).sort(),references:notes.references.map(markerFor),scopes:value.sectionScopes.map(scope=>scope.notes[kind].references.map(markerFor))}];
  }));
}
function assertNativeReturn(expected,actual){
  assert.deepEqual(numberingSnapshot(actual),numberingSnapshot(expected),'Native sparse/effective section numbering changed');
  assert.deepEqual(storySignature(actual),storySignature(expected),'Story definitions, repeated references or native section ownership changed');
  for(let index=0;index<expected.sectionScopes.length;index++){
    const marker=`P06C_SCOPE_${String(index).padStart(2,'0')}`;
    if(expected.sectionScopes[index].text.includes(marker))assert(actual.sectionScopes[index].text.includes(marker),'Actual terminator moved '+marker);
  }
  return {nativeSections:actual.nativeSections,sparseAndEffectiveNumberingChecked:true,storyIdentityChecked:true,wordApplicationCertified:false};
}
function sourceSections(text,helpers){
  const tokens=helpers.markdownTokens(text);
  return tokens.filter(token=>token.type==='html'&&token.raw.trim().startsWith('<!-- readmd-section:')).map(token=>{
    const match=/^<!-- readmd-section:v1 ([\s\S]*?) -->$/.exec(token.raw.trim());assert(match,'Invalid readable section declaration');return JSON.parse(match[1]);
  });
}
function assertReadable(text,current,expected,helpers){
  const sections=sourceSections(text,helpers);assert.equal(sections.length,expected.nativeSections);
  const ids=sections.map(section=>section.id);assert.equal(new Set(ids).size,ids.length);
  for(let index=0;index<sections.length;index++)for(const kind of KINDS){
    const policy=sections[index].footnotes?.kinds?.[kind]||{},native=numberedChildren(expected.sectionScopes[index].notes[kind].sparse);
    for(const [source,nativeName]of [['numberFormat','numFmt'],['start','numStart'],['restart','numRestart']]){
      const leaf=native.find(row=>row.name===nativeName);assert.equal(Object.hasOwn(policy,source),Boolean(leaf),`Sparse ${kind}.${source} presence changed at ${index}`);
      if(leaf)assert.equal(policy[source],source==='start'?Number(leaf.val):source==='restart'?({continuous:'continuous',eachSect:'section',eachPage:'page'})[leaf.val]:leaf.val);
    }
    const format=native.find(row=>row.name==='numFmt');assert.equal(Object.hasOwn(policy,'numberPattern'),Boolean(format?.formatPresent));if(format?.formatPresent)assert.equal(policy.numberPattern,format.format);
  }
  const bindings={};
  for(const kind of KINDS){const marker=kind==='footnote'?'NATIVE_FOOT_BODY':'NATIVE_END_BODY';if(expected.notes[kind].definitions.some(row=>row.type==='normal'&&row.text.includes(marker))){const label=helpers.noteLabel(text,marker);assert.equal(current.sourceValues.footnotes.kindByLabel[label],kind);bindings[kind]={label,references:[...text.matchAll(/(?<!\\)\[\^([^\]\r\n]+)\](?!:)/g)].filter(match=>match[1]===label).length};}}
  return {ids,bindings};
}
function authoredSource(kind){
  const global={version:1,footnotes:{kinds:{footnote:{numberFormat:'lowerLetter',start:3,restart:'continuous'},endnote:{numberFormat:'lowerRoman',start:1,restart:'continuous'}},kindByLabel:{'edge-f':'footnote','edge-e':'endnote'}}};
  const marker=(id,notes)=>'<!-- readmd-section:v1 '+JSON.stringify({id,break:'continuous',...(notes?{footnotes:notes}:{})})+' -->\r\n\r\n';
  if(kind==='unused')return marker('unusednotes',{kinds:{footnote:{numberFormat:'chicago',start:65535,restart:'continuous'}}})+'Unused but meaningful note settings stay authored.\r\n';
  if(kind==='varied')return '---\nreadmd: {publication: {version: 1}}\n---\n'+marker('varied1',{kinds:{footnote:{numberFormat:'upperLetter',start:27,restart:'continuous'}}})+'First[^v].\n\n'+marker('varied2',{kinds:{footnote:{numberFormat:'decimal',start:1,restart:'continuous'}}})+'Last[^v].\n\n[^v]: VARIED_NOTE_BODY\n';
  if(kind==='pattern'){
    global.footnotes.kinds.footnote={numberFormat:'custom',numberPattern:'GLOBAL_A',start:65535,restart:'continuous'};
    return '\uFEFF---\r\nreadmd:\r\n  publication: '+JSON.stringify(global)+'\r\n---\r\n'+marker('pattern1',{kinds:{footnote:{numberPattern:'LOCAL_A',start:27}}})+'Pattern[^edge-f], end[^edge-e].\r\n\r\n'+marker('pattern2',{kinds:{footnote:{numberFormat:'decimal',start:0,restart:'continuous'}}})+'Clear[^edge-f].\r\n\r\n'+marker('pattern3')+'Inherit[^edge-f].\r\n\r\n[^edge-f]: NATIVE_FOOT_BODY\r\n\r\n[^edge-e]: NATIVE_END_BODY\r\n';
  }
  return '\uFEFF---\r\nkeep_neighbor: \'00123\' # KEEP_P06C\r\nreadmd:\r\n  publication: '+JSON.stringify(global)+'\r\n---\r\nPrefix 😀[^edge-f], end[^edge-e].\r\n\r\n'+marker('edgeempty',{kinds:{footnote:{numberFormat:'decimal',start:0,restart:'continuous'}}})+marker('edgecode',{kinds:{footnote:{numberFormat:'upperRoman',start:7,restart:'section'}}})+'# P06C_DUPLICATE_HEADING\r\n\r\nCode[^edge-f].\r\n\r\n```text\r\nP06C_CODE_END\r\n```\r\n\r\n'+marker('edgelast')+'# P06C_DUPLICATE_HEADING\r\n\r\nLast[^edge-f].\r\n\r\n[^edge-f]: NATIVE_FOOT_BODY\r\n\r\n[^edge-e]: NATIVE_END_BODY\r\n';
}
function uniformSource(format,restart,start=27){
  const marker=id=>'<!-- readmd-section:v1 '+JSON.stringify({id,break:'continuous',footnotes:{kinds:{footnote:{numberFormat:format,start,restart}}}})+' -->\n\n';
  return marker('uniform1')+'UNIFORM_SCOPE_ONE [^u1], [^u2].\n\n'+marker('uniform2')+'UNIFORM_SCOPE_TWO again[^u1], new[^u3], last[^u4].\n\n[^u1]: UNIFORM_BODY_ONE\n\n[^u2]: UNIFORM_BODY_TWO\n\n[^u3]: UNIFORM_BODY_THREE\n\n[^u4]: UNIFORM_BODY_FOUR\n';
}
const VISIBLE_NUMBERS={decimal:['27','28','29','30'],lowerRoman:['xxvii','xxviii','xxix','xxx'],upperRoman:['XXVII','XXVIII','XXIX','XXX'],lowerLetter:['aa','bb','cc','dd'],upperLetter:['AA','BB','CC','DD']};
function assertError(result,code,helpers){assert.equal(result.isError,true);const value=helpers.toolValue(result);assert.equal(value.error_code,code,JSON.stringify(value));return value;}
function assertVariedPolicyError(result,format,helpers){
  assert(['html','tex','epub','pdf'].includes(format));const value=assertError(result,'publication_note_policy_unsupported',helpers);
  const field='sections['+JSON.stringify(format==='tex'?'varied1':'varied2')+'].footnotes.kinds.footnote.numberFormat';assert.equal(value.path,field);assert.equal(value.field,field);return value;
}
function assertProtectedState(actual,expected){assert.deepEqual(actual,expected,'Consumer rejection changed protected target/source/directory');}
async function prepare(ctx){
  const {oracle,exportDocx,importAndSave,convertOnly,profile,call,retain,check,report,evidence,documents,helpers}=ctx;
  const state={invalid:[],invalidSource:[],uniform:[],varied:null};report.p06c={plan:plan(),cases:[],firstFailuresPreserved:true};
  const importCase=async(label,file,expected)=>{
    let first;
    const row=await importAndSave(label,file,{verifyImported:(text,current)=>{first=assertReadable(text,current,expected,helpers);},verifyOutput:async(actual,output)=>{
      const native=assertNativeReturn(expected,actual);const second=await convertOnly(label+'-second-import',output.file),current=await profile(second),reimport=assertReadable(second,current,actual,helpers);
      assert.deepEqual(reimport.ids,first.ids,'Section IDs drifted at the second DOCX import');for(const kind of Object.keys(first.bindings))assert.equal(reimport.bindings[kind].references,first.bindings[kind].references);
      check(label+': independent native numbering and second-import IDs/kind bindings',native);
    }});report.p06c.cases.push({label,scopes:expected.nativeSections});return row;
  };
  const fixtures={};
  for(const kind of ['p06c-native-mixed-three','p06c-native-format-matrix']){
    const file=path.join(evidence,'fixtures',kind+'.docx'),created=oracle(kind+'-fixture',['fixture','--kind',kind,'--output',file]);assert.equal(created.nativeOnly,true);assert.deepEqual(created.inspection.auxiliaryParts,{});
    if(kind.endsWith('format-matrix')){assert.equal(created.fixedSchema.formats.length,63);assert.equal(new Set(created.fixedSchema.formats).size,63);for(const noteKind of KINDS)assert.deepEqual(created.inspection.sectionScopes.map(scope=>scope.notes[noteKind].effective.numFmt),created.fixedSchema.formats);report.p06c.fixedSchema=created.fixedSchema;}
    else{assert.equal(created.inspection.nativeSections,3);assert(created.inspection.notes.footnote.references.includes('7'));assert(created.inspection.notes.endnote.references.includes('7'));assert.equal(created.inspection.sectionScopes[2].notes.footnote.sparse,null);}
    await importCase(kind,file,created.inspection);fixtures[kind]=created;
  }
  const mixedText=await convertOnly('p06c-bound-input',path.join(evidence,'fixtures','p06c-native-mixed-three.docx'));
  const numbering='<!-- readmd-numbering:v1 '+JSON.stringify({headings:{levels:[{format:'decimal',start:1,restartAfter:0}]}})+' -->\n\n',front=/^(?:\uFEFF)?---\r?\n[\s\S]*?\r?\n---(?:\r?\n|$)/.exec(mixedText);
  const boundSource=front?mixedText.slice(0,front[0].length)+numbering+mixedText.slice(front[0].length):numbering+mixedText;
  const bound=await exportDocx('p06c-bound-source',boundSource);assert(Object.keys(bound.value.auxiliaryParts).length,'Current-native precedence requires a real source-bound package');
  for(const mode of ['p06c-section-change','p06c-section-clear','p06c-all-clear']){
    const file=path.join(evidence,'fixtures',mode+'.docx'),changed=oracle(mode+'-mutation',['mutate','--mode',mode,'--input',bound.file,'--output',file]);assert.equal(changed.allOtherPartsByteIdentical,true);assert.deepEqual(changed.inspection.auxiliaryParts,bound.value.auxiliaryParts);
    if(mode==='p06c-section-change'){const old=numberingSnapshot(bound.value),actual=numberingSnapshot(changed.inspection);assert.deepEqual(actual[0],old[0]);assert.deepEqual(actual[2],old[2]);assert.equal(actual[1].footnote.effective.numStart,'65535');assert.equal(actual[1].footnote.effective.numRestart,'eachPage');}
    if(mode.endsWith('clear'))for(const scope of changed.inspection.sectionScopes)for(const kind of KINDS)assert.deepEqual(numberedChildren(scope.notes[kind].sparse),[]);
    await importCase(mode,file,changed.inspection);
  }
  for(const mode of INVALID_NATIVE){
    const file=path.join(evidence,'fixtures',mode+'.docx');oracle(mode+'-mutation',['mutate','--mode',mode,'--input',bound.file,'--output',file]);const before=helpers.fileHash(file),names=fs.readdirSync(path.dirname(file)).sort();
    const code=mode==='p06c-foreign-format'?'docx_note_settings_unsupported':'docx_note_settings_invalid';helpers.assertConversionError(await call('readmd_convert_to_markdown',{file_path:file}),code);assert.equal(helpers.fileHash(file),before);assert.deepEqual(fs.readdirSync(path.dirname(file)).sort(),names);state.invalid.push({mode,file,code,sha256:before,names});
  }
  for(const kind of ['edge','pattern','varied','unused']){
    const source=authoredSource(kind);retain('documents/p06c-authored-'+kind+'.md',source);
    const first=await exportDocx('p06c-'+kind+'-initial',source);if(kind==='pattern'){
      const snapshot=numberingSnapshot(first.value);assert.equal(snapshot[0].footnote.sparse[0].val,'custom');assert.equal(snapshot[0].footnote.sparse[0].format,'LOCAL_A');assert.equal(snapshot[1].footnote.sparse[0].formatPresent,false);assert.deepEqual(snapshot[2].footnote.sparse,[]);
    }
    const row=await importCase('p06c-'+kind,first.file,first.value);if(kind==='varied')state.varied=row;if(kind==='unused'){state.unused=row;assert.equal(first.value.notes.footnote.references.length+first.value.notes.endnote.references.length,0);assert.equal(numberingSnapshot(first.value)[0].footnote.sparse.find(row=>row.name==='numStart').val,'65535');}
    if(kind==='edge'){
      assert.equal(first.value.nativeSections,4);assert(first.value.sectionScopes[0].text.includes('Prefix 😀'));assert(!first.value.sectionScopes[1].text.trim());assert(first.value.sectionScopes[2].text.includes('P06C_CODE_END'));
      const before=await profile(source),patched=helpers.payload(await call('readmd_publication_profile',{action:'patch',content:source,base_revision:before.revision,patch:{footnotes:{kinds:{footnote:{start:12000}}}}}));helpers.assertProfileTransaction(patched,source);assert(patched.content.includes('# KEEP_P06C\r\n'));assert.deepEqual(sourceSections(patched.content,helpers),sourceSections(source,helpers));
      const edited=await exportDocx('p06c-global-edit',patched.content),old=numberingSnapshot(first.value),current=numberingSnapshot(edited.value);assert.equal(current[0].footnote.effective.numStart,'12000');assert.equal(current[3].footnote.effective.numStart,'12000');assert.deepEqual(current[1],old[1]);assert.deepEqual(current[2],old[2]);
      const inverse=helpers.payload(await call('readmd_content_transaction',{content:patched.content,transaction:{base_revision:patched.revision,origin:'editor',edits:patched.inverse}}));assert.equal(inverse.result.content,source);assertError(await call('readmd_publication_profile',{action:'patch',content:patched.content,base_revision:before.revision,patch:{}}),'transaction_conflict',helpers);
      const declaration=helpers.markdownTokens(source).find(token=>token.type==='html'&&token.raw.includes('"id":"edgeempty"')).raw.trim(),from=source.indexOf(declaration),insert=declaration.replace('"start":0','"start":1');assert(from>source.indexOf('😀'));assert.notEqual(insert,declaration);
      const edit={from,to:from+declaration.length,insert},transaction={base_revision:before.revision,origin:'editor',edits:[edit]},changed=helpers.payload(await call('readmd_content_transaction',{content:source,transaction})).result;
      assert.equal(changed.content,source.slice(0,from)+insert+source.slice(from+declaration.length));const undone=helpers.payload(await call('readmd_content_transaction',{content:changed.content,transaction:{base_revision:changed.revision,origin:'editor',edits:changed.inverse}}));assert.equal(undone.result.content,source);
      assertError(await call('readmd_content_transaction',{content:changed.content,transaction}),'transaction_conflict',helpers);assertError(await call('readmd_content_transaction',{content:source,transaction:{...transaction,edits:[edit,edit]}}),'transaction_overlapping_edits',helpers);
      const changedNative=await exportDocx('p06c-section-transaction-initial',changed.content);assert.equal(numberingSnapshot(changedNative.value)[1].footnote.effective.numStart,'1');await importCase('p06c-section-transaction',changedNative.file,changedNative.value);
    }
  }
  for(const row of state.invalid)row.names=fs.readdirSync(path.dirname(row.file)).sort();
  for(const format of Object.keys(VISIBLE_NUMBERS))for(const restart of ['continuous','section']){
    const label='p06c-uniform-'+format+'-'+restart,source=uniformSource(format,restart),first=await exportDocx(label+'-initial',source),row=await importCase(label,first.file,first.value);state.uniform.push({row,format,restart});
  }
  const htmlSource=uniformSource('decimal','continuous',1),htmlFirst=await exportDocx('p06c-html-default-initial',htmlSource);state.htmlDefault=await importCase('p06c-html-default',htmlFirst.file,htmlFirst.value);
  for(const [name,declaration]of INVALID_SOURCE){
    const label='p06c-invalid-source-'+name,file=path.join(documents,label+'.md'),sentinel='OWNED_INVALID_SOURCE_SENTINEL_'+name+'\n',text='<!-- readmd-section:v1 '+declaration+' -->\n\nInvalid authored settings stay editable.\n';retain('documents/'+label+'.md',sentinel);
    const current=helpers.payload(await call('readmd_read_document',{file_path:file,start_line:1,end_line:1}));assert.equal(current.content,sentinel);const saved=helpers.payload(await call('readmd_edit_document',{file_path:file,expected_revision:current.revision,old_text:sentinel,new_text:text,dry_run:false,confirm:true}));helpers.assertSavedEdit(saved,sentinel,text);assert.equal(fs.readFileSync(file,'utf8'),text);state.invalidSource.push({name,label,file,text,sha256:helpers.fileHash(file)});
  }
  check('P06-C prepared native-only 3/63 scopes, source-bound current XML changes/clear, explicit saves and consumer negatives for reopened PID');return state;
}
async function reopened(ctx,state){
  const {call,check,oracle,report,evidence,documents,helpers}=ctx;
  assert.equal(typeof state.varied.reopenedMarkdown,'string','Consumer negatives require actual new-PID read bytes');
  for(const row of state.invalid){helpers.assertConversionError(await call('readmd_convert_to_markdown',{file_path:row.file}),row.code);assert.equal(helpers.fileHash(row.file),row.sha256);assert.deepEqual(fs.readdirSync(path.dirname(row.file)).sort(),row.names);check(row.mode+': exact typed native rejection retried after normal EOF/new PID');}
  for(const row of state.invalidSource){
    const read=helpers.payload(await call('readmd_read_document',{file_path:row.file,start_line:1,end_line:3}));assert.equal(read.content,row.text);assert.equal(read.revision,row.sha256);const folder=path.join(evidence,'outputs',row.label);fs.mkdirSync(folder);const target=path.join(folder,'owned-old.docx');fs.writeFileSync(target,'OWNED_OLD_INVALID_SOURCE_'+row.name+'\n',{flag:'wx'});const before=helpers.fileHash(target),names=fs.readdirSync(folder).sort(),request={markdown_content:read.content,output_format:'docx',output_path:target,base_dir:documents,confirm:true,options:{}};
    assertError(await call('readmd_export_document',request),'output_exists',helpers);assertError(await call('readmd_export_document',{...request,overwrite:true}),'publication_section_invalid',helpers);assert.equal(helpers.fileHash(target),before);assert.equal(helpers.fileHash(row.file),row.sha256);assert.deepEqual(fs.readdirSync(folder).sort(),names);check(row.label+': explicit raw save/reopen remains editable while invalid policy cannot replace old DOCX');
  }
  for(const {row,format,restart}of state.uniform){
    assert.equal(typeof row.reopenedMarkdown,'string');const label=row.label+'-epub',target=path.join(evidence,'outputs',label+'.epub'),args={markdown_content:row.reopenedMarkdown,output_path:target,base_dir:documents,title:'P06C Uniform Notes',language:'en',confirm:true,options:{}};
    const exported=helpers.payload(await call('readmd_export_epub',args,90000));assert(fs.statSync(target).size>0);const actual=oracle(label+'-native',['inspect-epub-notes',target]),numbers=VISIBLE_NUMBERS[format],expected=restart==='section'?[numbers[0],numbers[1],numbers[0],numbers[0],numbers[1]]:[numbers[0],numbers[1],numbers[0],numbers[2],numbers[3]];
    assert.deepEqual(actual.references.map(ref=>ref.text),expected,'EPUB did not render the saved uniform note numbering');assert.equal(actual.notes.length,4);for(const name of ['ONE','TWO','THREE','FOUR'])assert.equal(actual.notes.filter(note=>note.text.includes('UNIFORM_BODY_'+name)).length,1);assert.equal(actual.references[0].targetId,actual.references[2].targetId,'Repeated EPUB note lost its stable target');
    report.outputs.push({label,path:path.relative(evidence,target),bytes:fs.statSync(target).size,sha256:helpers.fileHash(target),warnings:exported.warnings||[]});check(label+': actual saved/reopened uniform labels, unique notes, forward/reverse links',{format,restart,visibleLabels:expected,independentZipXml:true,externalReaderCertified:false});
  }
  {
    const row=state.htmlDefault;assert.equal(typeof row.reopenedMarkdown,'string');const target=path.join(evidence,'outputs','p06c-html-default.html'),exported=helpers.payload(await call('readmd_export_document',{markdown_content:row.reopenedMarkdown,output_path:target,output_format:'html',base_dir:documents,confirm:true,options:{}},90000)),html=fs.readFileSync(target,'utf8'),match=/<script type="application\/json" id="readmd-footnote-html">([\s\S]*?)<\/script>/.exec(html);assert(match,'Actual typed standalone HTML note projection missing');const notes=JSON.parse(match[1]);assert.equal(typeof notes,'string');assert.deepEqual([...notes.matchAll(/role="doc-noteref">(\d+)<\/a>/g)].map(row=>row[1]),['1','2','1','3','4']);assert.equal((notes.match(/role="doc-backlink"/g)||[]).length,5);report.outputs.push({label:'p06c-html-default',path:path.relative(evidence,target),bytes:fs.statSync(target).size,sha256:helpers.fileHash(target),warnings:exported.warnings||[]});check('HTML default P06-C numbering has actual decimal callouts and all reverse links');
  }
  for(const [format,engine]of [['html'],['tex'],['epub'],['pdf','typst'],['pdf','rust']]){
    const label='p06c-reject-'+format+(engine?'-'+engine:''),folder=path.join(evidence,'outputs',label);fs.mkdirSync(folder);const target=path.join(folder,'owned-old.'+format),sentinel='OWNED_P06C_OLD_TARGET_'+label+'\n';fs.writeFileSync(target,sentinel,{flag:'wx'});
    const request={markdown_content:state.varied.reopenedMarkdown,output_path:target,base_dir:documents,confirm:true,options:engine?{pdf:{engine}}:{}};if(format!=='epub')request.output_format=format;
    const name=format==='epub'?'readmd_export_epub':'readmd_export_document',before=helpers.fileHash(target),names=fs.readdirSync(folder).sort();
    const protectedState={targetSha256:before,sourceSha256:state.varied.sha256,directoryMembers:names},assertProtected=()=>assertProtectedState({targetSha256:helpers.fileHash(target),sourceSha256:helpers.fileHash(state.varied.file),directoryMembers:fs.readdirSync(folder).sort()},protectedState);
    assertError(await call(name,request),'output_exists',helpers);assertProtected();
    const failure=assertVariedPolicyError(await call(name,{...request,overwrite:true}),format,helpers);assertProtected();check(label+': saved/reopened varied numbering rejects before overwriting or staging',{code:failure.error_code,path:failure.path,field:failure.field,oldTargetSha256:before,missingOverwriteProtected:true,explicitOverwriteProtected:true,targetPreserved:true,sourcePreserved:true,directoryPreserved:true});
  }
  {
    const row=state.unused;assert.equal(typeof row.reopenedMarkdown,'string');const folder=path.join(evidence,'outputs','p06c-unused-guard');fs.mkdirSync(folder);const target=path.join(folder,'owned-old.epub');fs.writeFileSync(target,'OWNED_UNUSED_CONFIG_OLD_TARGET\n',{flag:'wx'});const before=helpers.fileHash(target),names=fs.readdirSync(folder).sort(),request={markdown_content:row.reopenedMarkdown,output_path:target,base_dir:documents,confirm:true,options:{}};assertError(await call('readmd_export_epub',request),'output_exists',helpers);assertError(await call('readmd_export_epub',{...request,overwrite:true}),'publication_note_policy_unsupported',helpers);assert.equal(helpers.fileHash(target),before);assert.deepEqual(fs.readdirSync(folder).sort(),names);assert.equal(helpers.fileHash(row.file),row.sha256);check('Meaningful unused native section numbering saves/reopens and cannot bypass consumer capability checks');
  }
  if(report.p06c.fixedSchema)assert.equal(helpers.fileHash(path.resolve(__dirname,'../..',report.p06c.fixedSchema.path)),report.p06c.fixedSchema.sha256,'Fixed schema changed during actual roundtrip');
}
module.exports={assertVariedPolicyError,assertProtectedState,plan,numberedChildren,numberingSnapshot,storySignature,assertNativeReturn,sourceSections,assertReadable,authoredSource,uniformSource,VISIBLE_NUMBERS,prepare,reopened,INVALID_NATIVE,INVALID_SOURCE};
