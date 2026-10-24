'use strict';
// Deterministic controls for the evidence validator; never a substitute for
// the candidate-bound real stdio run and independent OPC inspection.
const test=require('node:test'),assert=require('node:assert/strict'),path=require('node:path'),fs=require('node:fs'),crypto=require('node:crypto');
const {spawnSync}=require('node:child_process');
const qa=require('./mcp-docx-settings-roundtrip.cjs');
// Reduced actual candidate45 independent native inspection. The real exporter
// chooses this caption bookmark; a synthetic table_target is not its contract.
// Only the two requested font axes vary in the 11pt/14pt pure controls.
function nativeGlobalSize(points){
  const value={
  "kind": "docx-settings-roundtrip",
  "sourcePayloadUsedAsNativeEvidence": false,
  "paragraphs": [
    {
      "text": "First, end, repeated. Generated .",
      "style": "BodyText",
      "fields": [],
      "bookmarks": [],
      "runs": [
        {
          "text": "First",
          "properties": {
            "sz": "21",
            "szCs": "21"
          }
        },
        {
          "text": ", end",
          "properties": {
            "sz": "21",
            "szCs": "21"
          }
        },
        {
          "text": ", repeated",
          "properties": {
            "sz": "21",
            "szCs": "21"
          }
        },
        {
          "text": ". Generated ",
          "properties": {
            "sz": "21",
            "szCs": "21"
          }
        },
        {
          "text": ".",
          "properties": {
            "sz": "21",
            "szCs": "21"
          }
        }
      ]
    },
    {
      "text": "See Table III.4.",
      "style": "BodyText",
      "fields": [
        {
          "instruction": " REF _htbl3asettings_4f9c731dce0e638e \\h ",
          "cached": "Table III.4",
          "kind": "simple",
          "depth": 0
        }
      ],
      "bookmarks": [],
      "runs": [
        {
          "text": "See ",
          "properties": {
            "sz": "21",
            "szCs": "21"
          }
        },
        {
          "text": "Table III.4",
          "properties": {
            "sz": "21",
            "szCs": "21"
          }
        },
        {
          "text": ".",
          "properties": {
            "sz": "21",
            "szCs": "21"
          }
        }
      ]
    },
    {
      "text": "Table III.4 — SETTINGS_TABLE",
      "style": "Caption",
      "fields": [
        {
          "instruction": " STYLEREF 1 \\s ",
          "cached": "III",
          "kind": "simple",
          "depth": 0
        },
        {
          "instruction": " SEQ ReadMDTable \\s 1 ",
          "cached": "1",
          "kind": "complex",
          "depth": 1
        },
        {
          "instruction": " =  + 3 \\* ARABIC ",
          "cached": "4",
          "kind": "complex",
          "depth": 0
        }
      ],
      "bookmarks": [
        {
          "id": "3",
          "name": "_htbl3asettings_4f9c731dce0e638e"
        }
      ],
      "runs": [
        {
          "text": "Table ",
          "properties": {
            "sz": "18",
            "szCs": "21"
          }
        },
        {
          "text": "III",
          "properties": {
            "sz": "18",
            "szCs": "21"
          }
        },
        {
          "text": ".",
          "properties": {
            "sz": "18",
            "szCs": "21"
          }
        },
        {
          "text": "1",
          "properties": {
            "sz": "18",
            "szCs": "21"
          }
        },
        {
          "text": "4",
          "properties": {
            "sz": "18",
            "szCs": "21"
          }
        },
        {
          "text": " — SETTINGS_TABLE",
          "properties": {
            "sz": "18",
            "szCs": "21"
          }
        }
      ]
    },
    {
      "text": "SETTINGS_ALPHA",
      "style": "Heading1",
      "fields": [],
      "bookmarks": [
        {
          "id": "1",
          "name": "_hsettings_alpha_9d0b04aadff304ff"
        }
      ],
      "rule": {
        "level": "0",
        "start": "3",
        "format": "upperRoman",
        "text": "%1",
        "suffix": "space",
        "restart": "0",
        "style": "Heading1"
      },
      "runs": [
        {
          "text": "SETTINGS_ALPHA",
          "properties": {
            "sz": "36",
            "szCs": "36"
          }
        }
      ]
    },
    {
      "text": "SETTINGS_METHOD",
      "style": "Heading2",
      "fields": [],
      "bookmarks": [
        {
          "id": "2",
          "name": "_hsettings_method_5570ca615c0a57d0"
        }
      ],
      "rule": {
        "level": "1",
        "start": "2",
        "format": "lowerLetter",
        "text": "%1.%2",
        "suffix": "space",
        "restart": "1",
        "style": "Heading2"
      },
      "runs": [
        {
          "text": "SETTINGS_METHOD",
          "properties": {
            "sz": "29",
            "szCs": "29"
          }
        }
      ]
    }
  ]
};
  for(const [index,western]of [[0,points],[1,points],[2,9]])for(const run of value.paragraphs[index].runs){run.properties.sz=String(western*2);run.properties.szCs=String(points*2);}
  return value;
}
test('native common font baseline oracle preserves independent caption CS and rejects wrong/body-only evidence',()=>{
  for(const points of [10.5,11,14]){
    const value=nativeGlobalSize(points);qa.assertNativeGlobalSize(value,points);
    for(const [row,axis,wrong]of [[0,'sz',21],[1,'szCs',21],[2,'szCs',18],[2,'sz',points*2]]){
      const changed=structuredClone(value);changed.paragraphs[row].runs[0].properties[axis]=String(wrong===points*2&&row<2?wrong+1:wrong);
      assert.throws(()=>qa.assertNativeGlobalSize(changed,points));
    }
    const missing=structuredClone(value);missing.paragraphs[1].fields=[];assert.throws(()=>qa.assertNativeGlobalSize(missing,points));
    const duplicate=structuredClone(value);duplicate.paragraphs.push(duplicate.paragraphs[0]);assert.throws(()=>qa.assertNativeGlobalSize(duplicate,points));
    const source=structuredClone(value);source.sourcePayloadUsedAsNativeEvidence=true;assert.throws(()=>qa.assertNativeGlobalSize(source,points));
  }
});
test('global-size REF follows the actual unique caption bookmark instead of a synthetic fixed name',()=>{
  const value=nativeGlobalSize(10.5);value.paragraphs[2].bookmarks[0].name='_different_current_caption_1234';value.paragraphs[1].fields[0].instruction=' REF _different_current_caption_1234 \\h ';qa.assertNativeGlobalSize(value,10.5);
});
for(const [name,change]of Object.entries({
  wrongTarget:v=>v.paragraphs[1].fields[0].instruction=' REF table_target \\h ',
  missingBookmark:v=>v.paragraphs[2].bookmarks=[],
  duplicateCaptionBookmark:v=>v.paragraphs[2].bookmarks.push({...v.paragraphs[2].bookmarks[0],id:'99'}),
  duplicateDocumentBookmark:v=>v.paragraphs[0].bookmarks.push({...v.paragraphs[2].bookmarks[0],id:'99'}),
  missingCaption:v=>v.paragraphs.splice(2,1),
  duplicateCaption:v=>v.paragraphs.push(structuredClone(v.paragraphs[2])),
  missingReference:v=>v.paragraphs[1].fields=[],
  duplicateReferenceField:v=>v.paragraphs[1].fields.push(structuredClone(v.paragraphs[1].fields[0])),
  duplicateReferenceParagraph:v=>v.paragraphs.push(structuredClone(v.paragraphs[1])),
  staleReferenceCache:v=>v.paragraphs[1].fields[0].cached='Table III.1',
  changedReferenceVisible:v=>v.paragraphs[1].text='See Table III.1.',
  staleChapterCache:v=>v.paragraphs[2].fields[0].cached='IV',
  differentSequence:v=>v.paragraphs[2].fields[1].instruction=' SEQ DifferentTable \\s 1 ',
  staleOffsetCache:v=>v.paragraphs[2].fields[2].cached='1',
  literalCaption:v=>v.paragraphs[2].fields=[],
  emptyReferenceRuns:v=>v.paragraphs[1].runs=[],
}))test('global-size native caption and REF evidence rejects '+name,()=>{const value=nativeGlobalSize(10.5);change(value);assert.throws(()=>qa.assertNativeGlobalSize(value,10.5));});
const hash=value=>crypto.createHash('sha256').update(value).digest('hex');
function conversionError(code='docx_note_settings_invalid',details={part:'word/settings.xml',path:'/w:settings/w:footnotePr/w:numFmt',detail:'unknown native number format'}){
  const body={ok:false,error_code:code,error:code,error_details:details};return {isError:true,structuredContent:body,content:[{type:'text',text:JSON.stringify(body)}]};
}
function note(kind,marker,id,references,{start,format,size,line,rule,longNote,placement}={}){
  const row={id,type:'normal',text:marker,paragraphs:[{text:marker,properties:{spacing:{line:String(line),lineRule:rule},keepLines:longNote==='keepParagraphs',keepNext:true},runs:[{text:marker,properties:{sz:String(size*2)}}]}]};
  return {part:'word/'+kind+'s.xml',references:Array(references).fill(id),settings:{values:{numStart:String(start),numFmt:format,...(kind==='endnote'?{pos:placement}:{})},specialIds:[]},sections:kind==='footnote'?[{values:{pos:placement}}]:[null],definitions:[row],selectedSpecial:[]};
}
function sourceNative({mutated=false,overrideStart,overrideSize,format,footStart,footSize,footSizeCs}={}){
  const foot=note('footnote','SOURCE_FOOT_BODY','7',2,{start:footStart??overrideStart??(mutated?13:7),format:format??(mutated?'lowerLetter':'upperRoman'),size:footSize??overrideSize??10,line:210,rule:'exact',longNote:'split',placement:'beneathText'}),end=note('endnote','SOURCE_END_BODY','7',2,{start:overrideStart??(mutated?17:3),format:mutated?'upperRoman':'lowerLetter',size:overrideSize??11,line:260,rule:'atLeast',longNote:'keepParagraphs',placement:'docEnd'});
  if(footSizeCs!==undefined)foot.definitions[0].paragraphs[0].runs[0].properties.szCs=String(footSizeCs*2);
  const chapter=mutated?'Part v.':'III',label='Table '+chapter+'.4';
  return {kind:'docx-settings-roundtrip',sourcePayloadUsedAsNativeEvidence:false,wordApplicationRoundtripCertified:false,notes:{footnote:foot,endnote:end},paragraphs:[
    {text:'SETTINGS_ALPHA',rule:{format:mutated?'lowerRoman':'upperRoman',start:mutated?'5':'3',text:mutated?'Part %1.':'%1',suffix:mutated?'tab':'space'},fields:[]},
    {text:'SETTINGS_METHOD',rule:{format:'lowerLetter',start:'2',text:'%1.%2'},fields:[]},
    {text:label+' — SETTINGS_TABLE',bookmarks:[{name:'table_target',id:'21'}],fields:[{instruction:' STYLEREF 1 \\s ',cached:chapter},{instruction:' SEQ ReadMDTable \\s 1 ',cached:'1'},{instruction:' =  + 3 \\* ARABIC ',cached:'4'}]},
    {text:'See '+label,fields:[{instruction:' REF table_target \\h ',cached:label}]}]};
}
function nativeFixture(kind='native-mixed'){
  const caption=kind.startsWith('native-caption-'),base=caption?kind.replace('native-caption-','native-'):kind;
  const value=sourceNative();value.notes.footnote.definitions[0].text='NATIVE_FOOT_BODY';value.notes.footnote.definitions[0].paragraphs[0].text='NATIVE_FOOT_BODY';value.notes.footnote.definitions[0].paragraphs[0].runs[0].text='NATIVE_FOOT_BODY';
  value.notes.endnote.definitions[0].text='NATIVE_END_BODY';value.notes.endnote.definitions[0].paragraphs[0].text='NATIVE_END_BODY';value.notes.endnote.definitions[0].paragraphs[0].runs[0].text='NATIVE_END_BODY';value.notes.endnote.references=['7'];
  const special=[{id:'41',type:'separator',text:'',separatorElements:[]},{id:'42',type:'continuationSeparator',text:'',separatorElements:['continuationSeparator']},{id:'43',type:'continuationNotice',text:'NATIVE_CONTINUED <next> & evidence',separatorElements:[]}];
  value.notes.footnote.definitions.push(...special);value.notes.footnote.selectedSpecial=special;
  value.paragraphs[0].text='NATIVE_ALPHA';value.paragraphs[0].rule.text='Article %1:';value.paragraphs[0].rule.suffix='tab';value.paragraphs[0].rule.start=base==='native-zero-start'?'0':'3';
  value.paragraphs[1].text='NATIVE_METHOD';Object.assign(value.paragraphs[1].rule,{text:'%1(%2)',suffix:'space',restart:'1'});
  value.paragraphs.push({text:'ORDINARY_LIST_ONLY',rule:{format:'decimal',text:'%1)'},style:null,fields:[]});
  if(caption){value.paragraphs[2]={text:'Table 4 — NATIVE_CAPTION',bookmarks:[{name:'cap_table',id:'21'}],fields:[{instruction:' SEQ Table \\r 4 \\* ARABIC ',cached:'4'}]};value.paragraphs[3]={text:'NATIVE_FIELD Table 4',fields:[{instruction:' REF cap_table \\h ',cached:'Table 4'}]};}
  return value;
}
function multiSectionNative(){
  const value=nativeFixture('native-note-mixed');value.nativeSections=2;value.notes.footnote.settings.values.numRestart='eachSect';value.notes.footnote.sections=[{values:{pos:'beneathText'}},{values:{pos:'pageBottom'}}];value.notes.endnote.sections=[null,null];value.notes.footnote.references=['7','7','8'];
  const first=value.notes.footnote.definitions[0],end=value.notes.endnote.definitions[0];first.text='NATIVE_FOOT_BODY Evidence 😀.';end.text='NATIVE_END_BODY Evidence 😀.';
  const second=structuredClone(first);second.id='8';second.text='NATIVE_SECOND_SECTION_NOTE Distinct placement.';second.paragraphs[0].text=second.text;second.paragraphs[0].runs[0].text=second.text;value.notes.footnote.definitions.push(second);
  value.paragraphs=[{text:'NATIVE_ALPHA',properties:{},fields:[],bookmarks:[]},{text:'NATIVE_FIRST NATIVE_SECOND NATIVE_REPEAT',properties:{},fields:[],bookmarks:[]},{text:'NATIVE_METHOD',properties:{},fields:[],bookmarks:[]},{text:'NATIVE_FIELD NATIVE_ALPHA',properties:{},fields:[],bookmarks:[]},{text:'',properties:{sectPr:null},fields:[],bookmarks:[]},{text:'NATIVE_SECOND_SECTION',properties:{},fields:[],bookmarks:[]}];return value;
}
const MULTI_SECTION_SOURCE='<!-- readmd-section:v1 {"id":"native-first","break":"continuous","footnotes":{"kinds":{"footnote":{"placement":"beneathText"}}}} -->\n\n# NATIVE_ALPHA {#native-alpha}\n\nNATIVE_FIRST [^f], NATIVE_SECOND [^e], NATIVE_REPEAT [^f].\n\n## NATIVE_METHOD\n\nNATIVE_FIELD [NATIVE_ALPHA](#native-alpha).\n\n<!-- readmd-section:v1 {"id":"native-last","footnotes":{"kinds":{"footnote":{"placement":"pageBottom"}}}} -->\n\nNATIVE_SECOND_SECTION [^g].\n\n[^f]: NATIVE_FOOT_BODY Evidence 😀.\n\n[^g]: NATIVE_SECOND_SECTION_NOTE Distinct placement.\n\n[^e]: NATIVE_END_BODY Evidence 😀.\n';
const MULTI_SECTION_PROFILE={ok:true,sourceValues:{version:1,footnotes:{kindByLabel:{f:'footnote',g:'footnote',e:'endnote'}}}};
function transaction(source='\uFEFF---\r\ncatalog_id: \'00123\' # keep\r\n---\r\n# 𝛂\r\n'){
  const from=source.indexOf('---',4),insert='readmd: {publication: {version: 1, footnotes: {start: 4}}}\r\n',content=source.slice(0,from)+insert+source.slice(from);
  return {source,value:{ok:true,revision:hash(content),content,changed:true,transaction:{base_revision:hash(source),origin:'editor',edits:[{from,to:from,insert}]},inverse:[{from,to:from+insert.length,insert:''}]}};
}
const LITERAL_DIAGNOSTIC='<!--readmd-numbering-import:v1 [{"code":"docx_numbering_import_conflict","reason":"caption_result_modified","source_id":"tbl:settings","visible_content_preserved":true}]-->';
const LITERAL_SOURCE='<span id="tbl:settings"></span>\n\nTable III.4 — SETTINGS_TABLE\n\nSee [Table III.4](#tbl:settings).\n\n'+LITERAL_DIAGNOSTIC+'\n';
const SOURCE_GFM_TABLE='| **Measure** | **Value** |\n| --- | --- |\n| **SETTINGS_CELL** | 42 |\n';
const LITERAL_VARIANTS={raw:LITERAL_SOURCE,numeric:LITERAL_SOURCE.replaceAll('III.4','III&#46;4'),hex:LITERAL_SOURCE.replaceAll('III.4','III&#x2e;4'),angle:LITERAL_SOURCE.replace('](#tbl:settings)','](<#tbl:settings>)'),named:LITERAL_SOURCE.replaceAll('III.4','III&period;4').replace('](#tbl:settings)','](<#tbl&#58;settings>)'),escaped:LITERAL_SOURCE.replaceAll('III.4','III\\.4')};
for(const [name,text]of Object.entries(LITERAL_VARIANTS))test('literal caption guard accepts parsed '+name,()=>assert.equal(qa.assertCaptionConflict(text).length,1));
const INVALID_LITERAL_VARIANTS={
  changed:LITERAL_SOURCE.replace('Table III.4 — SETTINGS_TABLE','Table Part v..4 — SETTINGS_TABLE'),
  target:LITERAL_SOURCE.replace('](#tbl:settings)','](#wrong)'),
  angleTarget:LITERAL_SOURCE.replace('](#tbl:settings)','](<#wrong>)'),
  label:LITERAL_SOURCE.replace('[Table III.4]','[Table III.1]'),
  identity:LITERAL_SOURCE.replace('id="tbl:settings"','id="tbl:other"'),
  duplicateIdentity:LITERAL_SOURCE+'\n<span id="tbl:settings"></span>\n',
  hiddenCaption:LITERAL_SOURCE.replace('Table III.4 — SETTINGS_TABLE','<!--Table III.4 — SETTINGS_TABLE-->'),
  fencedCaption:LITERAL_SOURCE.replace('Table III.4 — SETTINGS_TABLE','```md\nTable III.4 — SETTINGS_TABLE\n```'),
  inlineCodeCaption:LITERAL_SOURCE.replace('Table III.4 — SETTINGS_TABLE','`Table III.4 — SETTINGS_TABLE`'),
  quotedCaption:LITERAL_SOURCE.replace('Table III.4 — SETTINGS_TABLE','> Table III.4 — SETTINGS_TABLE'),
  hiddenHtmlCaption:LITERAL_SOURCE.replace('Table III.4 — SETTINGS_TABLE','<span hidden>Table III.4 — SETTINGS_TABLE</span>'),
  rawPreCaption:LITERAL_SOURCE.replace('Table III.4 — SETTINGS_TABLE','<pre>\nTable III.4 — SETTINGS_TABLE\n</pre>'),
  hiddenLink:LITERAL_SOURCE.replace('See [Table III.4](#tbl:settings).','<!--See [Table III.4](#tbl:settings).-->'),
  codeLink:LITERAL_SOURCE.replace('See [Table III.4](#tbl:settings).','`See [Table III.4](#tbl:settings).`'),
  hiddenAnchor:LITERAL_SOURCE.replace('<span id="tbl:settings"></span>','<!--<span id="tbl:settings"></span>-->'),
  doubleEntity:LITERAL_SOURCE.replaceAll('III.4','III&amp;#46;4'),
  doubleTarget:LITERAL_SOURCE.replace('](#tbl:settings)','](<#tbl&amp;#58;settings>)'),
  falseDiagnostic:LITERAL_SOURCE.replace('caption_result_modified','unsupported_reference_field'),
  codeDiagnostic:LITERAL_SOURCE.replace(LITERAL_DIAGNOSTIC,'```html\n'+LITERAL_DIAGNOSTIC+'\n```'),
  quotedDiagnostic:LITERAL_SOURCE.replace(LITERAL_DIAGNOSTIC,'> '+LITERAL_DIAGNOSTIC),
  nestedCommentDiagnostic:LITERAL_SOURCE.replace(LITERAL_DIAGNOSTIC,'<!-- example\n'+LITERAL_DIAGNOSTIC+'\n-->'),
  reactivated:LITERAL_SOURCE+'\n:label[tbl:settings] SETTINGS_TABLE\n'
};
for(const [name,text]of Object.entries(INVALID_LITERAL_VARIANTS))test('literal caption guard rejects '+name,()=>assert.throws(()=>qa.assertCaptionConflict(text)));
test('retained candidate28 serialization has current literal identity, parsed text, link and three diagnostics',()=>{
  // Reduced verbatim serialization from the retained full candidate28 input.
  // The complete original bytes and first failure stay in owned evidence.
  const diagnostic='<!--readmd-numbering-import:v1 [{"code":"docx_numbering_import_conflict","reason":"caption_result_modified","kind":"table","source_id":"tbl:settings","visible_content_preserved":true},{"code":"docx_numbering_import_conflict","reason":"configuration_without_native_instance","visible_content_preserved":true},{"code":"docx_numbering_import_conflict","reason":"reference_preserved_as_link","source_id":"tbl:settings","visible_content_preserved":true}]-->';
  const text='---\nreadmd: {publication: {version: 1}}\n---\n\n<span id="tbl:settings"></span>\n\nTable III&#46;4 — SETTINGS_TABLE\n\nSee [Table III&#46;4](#tbl:settings)&#46;\n\n'+diagnostic+'\n';
  assert.equal(qa.assertCaptionConflict(text).length,3);
});
test('heading REF uses the same parsed link and actual heading identity rules',()=>{
  const diagnostic='<!--readmd-numbering-import:v1 [{"code":"docx_numbering_import_conflict","reason":"reference_preserved_as_link","source_id":"docx-current","visible_content_preserved":true}]-->';
  const source='# NATIVE&#95;ALPHA {#docx-current}\n\nNATIVE_FIELD [NATIVE&#95;ALPHA](<#docx&#45;current>)\n\n'+diagnostic+'\n';
  assert.equal(qa.assertHeadingReferenceOpen(source).length,1);
  for(const wrong of [source.replace('<#docx&#45;current>','<#wrong>'),source.replace('NATIVE_FIELD [NATIVE&#95;ALPHA](<#docx&#45;current>)','`NATIVE_FIELD [NATIVE&#95;ALPHA](<#docx&#45;current>)`'),source.replace('# NATIVE&#95;ALPHA {#docx-current}','```md\n# NATIVE&#95;ALPHA {#docx-current}\n```'),source.replace(diagnostic,'```html\n'+diagnostic+'\n```')])assert.throws(()=>qa.assertHeadingReferenceOpen(wrong));
});
test('numbering clear literal text is parsed once and examples cannot satisfy it',()=>{
  const source='# SETTINGS_ALPHA {#settings-alpha}\n\n## SETTINGS_METHOD {#settings-method}\n\nTable III&#46;4 — SETTINGS_TABLE\n\nSee Table III&#46;4&#46;\n\n# SETTINGS_BETA {#settings-beta}\n\n## SETTINGS_RESULTS {#settings-results}\n';
  qa.assertNumberingClearedImport(source);
  for(const wrong of [source.replace('Table III&#46;4 — SETTINGS_TABLE','<!--Table III&#46;4 — SETTINGS_TABLE-->'),source.replace('Table III&#46;4 — SETTINGS_TABLE','```md\nTable III&#46;4 — SETTINGS_TABLE\n```'),source.replaceAll('&#46;','&amp;#46;'),source.replace('# SETTINGS_ALPHA {#settings-alpha}','```md\n# SETTINGS_ALPHA {#settings-alpha}\n```')])assert.throws(()=>qa.assertNumberingClearedImport(wrong));
});
test('import diagnostics require actual top-level HTML comment nodes',()=>{
  for(const source of ['```html\n'+LITERAL_DIAGNOSTIC+'\n```','> '+LITERAL_DIAGNOSTIC,'<!-- example\n'+LITERAL_DIAGNOSTIC+'\n-->','<pre>\n'+LITERAL_DIAGNOSTIC+'\n</pre>',LITERAL_DIAGNOSTIC.replaceAll('<','&lt;').replaceAll('>','&gt;')])assert.deepEqual(qa.numberingImportDiagnostics(source),[]);
  assert.throws(()=>qa.numberingImportDiagnostics('<!--readmd-numbering-import:v1 [bad]-->'));
});

test('plan and CLI are read-only until a candidate is explicitly supplied for execution',()=>{
  const plan=qa.planFor();assert.equal(plan.transport,'real-stdio');assert.equal(plan.noProcessesStarted,true);assert.equal(plan.noFilesWritten,true);assert.equal(plan.applicationRoundtripCertified,false);assert.equal(plan.headlessMutationsAreWordInterop,false);assert(plan.sequence.some(step=>step.includes('different real process')));assert(plan.sequence.some(step=>step.includes('options:{}')));assert(plan.requiredTools.includes('readmd_edit_document'));
  assert.throws(()=>qa.argumentsFor([],{}),/candidate/);assert.throws(()=>qa.argumentsFor(['--candidate','candidate.json'],{}),/QA-only/);assert.throws(()=>qa.argumentsFor(['--plan','--unknown'],{}),/Unknown/);assert.throws(()=>qa.argumentsFor(['--plan','--python'],{}),/Missing/);assert.throws(()=>qa.argumentsFor(['--plan','--output',qa.ROOT],{}),/escaped/);assert.equal(qa.argumentsFor(['--help'],{}).help,true);
  const result=spawnSync(process.execPath,[path.join(__dirname,'mcp-docx-settings-roundtrip.cjs'),'--plan','--candidate','does-not-exist.json'],{windowsHide:true,encoding:'utf8'});assert.equal(result.status,0,result.stderr);assert.equal(JSON.parse(result.stdout).noFilesWritten,true);
});

test('QA source files are real and candidate binding names cover the shared consumer chain',()=>{
  for(const name of qa.QA_FILES)assert(fs.statSync(path.join(qa.ROOT,name)).isFile(),name);
  for(const suffix of ['publication_profile.rs','publication_note_policies.rs','publication_sections.rs','conversion_errors.rs','convert/docx_note_settings.rs','convert/docx_text_style.rs','publication_inline_text.rs','publication_rich.rs','publication_blocks.rs','docx_local_style.rs','convert/docx_numbering_import.rs','export_preview.rs','mcp.rs','mdexport.rs'])assert(qa.PRODUCT_FILES.some(name=>name.endsWith('/'+suffix)),'Missing binding '+suffix);
  assert.equal(new Set([...qa.QA_FILES,...qa.PRODUCT_FILES]).size,qa.QA_FILES.length+qa.PRODUCT_FILES.length);
});

test('owned evidence paths refuse sibling-prefix and parent traversal escapes',()=>{
  assert.throws(()=>qa.inside(qa.BASE,qa.BASE),/escaped/);assert.throws(()=>qa.inside(qa.BASE,path.join(qa.BASE,'../settings-other/run')),/escaped/);assert.throws(()=>qa.inside(qa.BASE,qa.BASE+'-other/run'),/escaped/);assert.equal(qa.inside(qa.BASE,path.join(qa.BASE,'owned-run')),path.join(qa.BASE,'owned-run'));
});

test('source fixture keeps authored mixed labels, typed spacing, explicit settings and neighboring YAML',()=>{
  const source=qa.sourceFor(qa.PROFILE,{citation:true});assert(source.startsWith('\uFEFF---\r\n'));assert(!/(?<!\r)\n/.test(source));assert(source.includes("catalog_id: '00123' # KEEP_NEIGHBOR"));assert(source.includes("ownedNeighbor: '007' # KEEP_READMD_NEIGHBOR"));assert(source.includes('readmd-numbering:v1'));assert(source.includes('Generated [@work, p. 12]'));assert(source.includes('SOURCE_FOOT_BODY'));assert(source.includes('SOURCE_END_BODY'));assert(source.includes('Unicode 𝛂'));
  assert.deepEqual(qa.PROFILE.footnotes.kindByLabel,{'manual-f':'footnote','manual-e':'endnote'});assert.equal(qa.PROFILE.footnotes.kinds.footnote.lineSpacing.pt,10.5);assert.equal(qa.PROFILE.footnotes.kinds.endnote.lineSpacing.mode,'atLeast');assert.equal(qa.PROFILE.footnotes.kinds.footnote.placement,'beneathText');assert(!source.includes('pdf.engine'));assert(!source.includes('C:/'));
});

test('real export fixture uses an unknown catalog key for the numeric lexical neighbor',()=>{
  // This is the same factory used by the real source export, including its
  // citation-enabled input. ISBN is a recognized, checksum-validated field;
  // an arbitrary leading-zero catalog value must stay an unknown neighbor.
  for(const citation of [false,true])for(const numbering of [false,true]){
    const source=qa.sourceFor(qa.PROFILE,{citation,numbering}),header=source.slice(0,source.indexOf('\r\n---\r\n'));
    assert.match(header,/^\uFEFF---\r\ncatalog_id: '00123' # KEEP_NEIGHBOR\r\n/,'The real export fixture must not declare its lexical test value as an ISBN');
    assert.doesNotMatch(header,/^isbn\s*:/im);
    assert.match(header,/^  ownedNeighbor: '007' # KEEP_READMD_NEIGHBOR\r?$/m);
    assert.equal(source.includes('Generated [@work, p. 12]'),citation);
    assert.equal(source.includes('readmd-numbering:v1'),numbering);
  }
});

test('actual conversion plain text and typed errors are not confused with JSON envelopes',()=>{
  assert.equal(qa.plainText({content:[{type:'text',text:'# actual\n'}]}),'# actual\n');assert.throws(()=>qa.plainText({isError:true,structuredContent:{error_code:'conversion_failed'},content:[]}),/conversion_failed/);assert.throws(()=>qa.plainText({content:[{type:'text',text:''}]}),/Empty/);
  const error=conversionError();assert.equal(qa.assertConversionError(error,'docx_note_settings_invalid').code,'docx_note_settings_invalid');assert.throws(()=>qa.assertConversionError({...error,isError:false},'docx_note_settings_invalid'));assert.throws(()=>qa.assertConversionError(error,'docx_note_settings_'));assert.throws(()=>qa.assertConversionError(error,'docx_note_settings_unsupported'));
});

test('native conversion errors require an exact bounded wire body and matching MCP text',()=>{
  for(const code of ['docx_note_settings_invalid','docx_note_settings_unsupported']){
    const error=conversionError(code);assert.deepEqual(qa.assertConversionError(error,code).native,error.structuredContent.error_details);
  }
  const quoted=conversionError('docx_note_settings_invalid',{part:'word/notes/作者.xml',path:'/w:settings/w:footnotePr',detail:'Quoted "part": and braces { } remain plain diagnostic text'});qa.assertConversionError(quoted,'docx_note_settings_invalid');
  const malformed=[
    row=>{row.structuredContent.error_code='conversion_failed';row.structuredContent.error='docx_note_settings_invalid:'+JSON.stringify(row.structuredContent.error_details);},
    row=>{row.structuredContent.error='prefix docx_note_settings_invalid';},row=>{row.structuredContent.ok=true;},row=>{row.structuredContent.extra='unexpected';},
    row=>{delete row.structuredContent.error_details;},row=>{row.structuredContent.error_details=null;},row=>{row.structuredContent.error_details=[];},
    row=>{delete row.structuredContent.error_details.path;},row=>{row.structuredContent.error_details.extra='untrusted';},row=>{row.structuredContent.error_details.detail=42;},
    row=>{row.structuredContent.error_details.part='';},row=>{row.structuredContent.error_details.path='bad\nlocator';},row=>{row.structuredContent.error_details.detail='bad\u0085control';},
    row=>{row.structuredContent.error_details.part='x'.repeat(513);},row=>{row.structuredContent.error_details.path='x'.repeat(2049);},row=>{row.structuredContent.error_details.detail='😀'.repeat(513);},
  ];
  for(const mutate of malformed){const bad=conversionError();mutate(bad);bad.content[0].text=JSON.stringify(bad.structuredContent);assert.throws(()=>qa.assertConversionError(bad,'docx_note_settings_invalid'));}
  for(const mutate of [row=>{row.content=[];},row=>{row.content[0].text='not json';},row=>{row.content[0].text=JSON.stringify({...row.structuredContent,error:'other'});},row=>{row.content[0].text=row.content[0].text.replace('"part":','"part":"word/forged.xml","part":');},row=>{row.content[0].text=row.content[0].text.replace('"part":','"pa\\u0072t":"word/forged.xml","part":');},row=>{row.content[0].text=' '.repeat(16385)+row.content[0].text;},row=>{row.content.push({type:'text',text:row.content[0].text});},row=>{delete row.structuredContent;}]){
    const bad=conversionError();mutate(bad);assert.throws(()=>qa.assertConversionError(bad,'docx_note_settings_invalid'));
  }
});

test('full source reopen bounds count BOM/CRLF and refuse truncating more than 1000 lines',()=>{
  assert.deepEqual(qa.sourceReadArguments('owned.md','\uFEFFa\r\nb\r\n'),{file_path:'owned.md',start_line:1,end_line:2});assert.equal(qa.sourceReadArguments('owned.md','x\n'.repeat(1000)).end_line,1000);assert.throws(()=>qa.sourceReadArguments('owned.md','x\n'.repeat(1001)),/read bound/);
});

test('profile transactions validate byte hashes, UTF-16 boundaries and exact inverse',()=>{
  const {source,value}=transaction();assert.equal(qa.assertProfileTransaction(value,source),value.content);
  for(const mutate of [row=>{row.transaction.base_revision='old';},row=>{row.transaction.origin='export';},row=>{row.revision='not-byte-revision';},row=>{row.inverse[0].insert='lost content';},row=>{row.content+='untracked';}]){const row=structuredClone(value);mutate(row);assert.throws(()=>qa.assertProfileTransaction(row,source));}
  const at=source.indexOf('𝛂');assert.throws(()=>qa.applyEdits(source,[{from:at+1,to:at+1,insert:'x'}]),/surrogate/);const eol=source.indexOf('\r\n');assert.throws(()=>qa.applyEdits(source,[{from:eol+1,to:eol+1,insert:'x'}]),/CRLF/);
});

test('idempotent profile patch proves zero edits, unchanged content and revision',()=>{
  const {source}=transaction(),value={ok:true,content:source,revision:hash(source),changed:false,transaction:{base_revision:hash(source),origin:'editor',edits:[]},inverse:[]};assert.equal(qa.assertProfileTransaction(value,source,{changed:false}),source);
  const bad=structuredClone(value);bad.transaction.edits=[{from:0,to:0,insert:''}];assert.throws(()=>qa.assertProfileTransaction(bad,source,{changed:false}));
});

test('actual save evidence requires committed revision, single replacement and recovery checkpoint',()=>{
  const before='sentinel\n',after='# Saved\n',value={ok:true,dry_run:false,changed:true,replacements:1,revision:hash(after),proposed_revision:hash(after),checkpoint_id:'owned-checkpoint'};qa.assertSavedEdit(value,before,after);
  for(const [key,next]of [['dry_run',true],['changed',false],['replacements',0],['revision',hash(before)],['checkpoint_id',null]])assert.throws(()=>qa.assertSavedEdit({...value,[key]:next},before,after));
});

test('source native validator requires actual mixed stories, fields and per-level automatic rules',()=>{
  assert.equal(qa.assertSourceNative(sourceNative()).mixedNativeStories,true);assert.equal(qa.assertSourceNative(sourceNative({mutated:true}),{mutated:true}).mutated,true);qa.assertSourceNative(sourceNative({overrideStart:23,overrideSize:12}),{overrideStart:23,overrideSize:12});qa.assertSourceNative(sourceNative({format:'chicago',footStart:12000,footSize:30,footSizeCs:30}),{format:'chicago',footStart:12000,footSize:30,footSizeCs:30});
  assert.throws(()=>qa.assertSourceNative(sourceNative({format:'chicago',footStart:12000,footSize:30,footSizeCs:10}),{format:'chicago',footStart:12000,footSize:30,footSizeCs:30}));
});

test('uniform 30pt positive checks every actual normal-footnote text run',()=>{
  const value=sourceNative({format:'chicago',footStart:12000,footSize:30,footSizeCs:30});
  const p=value.notes.footnote.definitions[0].paragraphs[0];p.runs.push({text:' ',properties:{sz:'60',szCs:'60'}},{text:'strong',properties:{sz:'60',szCs:'60',b:true}},{text:'evidence',properties:{sz:'60',szCs:'60'}});
  value.notes.footnote.definitions.push({id:'8',type:'normal',text:'SOURCE_GENERATED_CITATION p. 12',paragraphs:[{runs:[{text:'SOURCE_GENERATED_CITATION',properties:{sz:'60',szCs:'60'}},{text:' p. 12',properties:{sz:'60',szCs:'60'}}]}]});
  assert.equal(qa.assertUniformNormalFootnoteRuns(value).textRunsChecked,6);
  for(const [note,run]of [[0,0],[0,1],[0,2],[0,3],[1,0],[1,1]]){const bad=structuredClone(value);bad.notes.footnote.definitions[note].paragraphs[0].runs[run].properties.sz='20';assert.throws(()=>qa.assertUniformNormalFootnoteRuns(bad));}
  const bad=structuredClone(value);bad.notes.footnote.definitions[1].paragraphs[0].runs[1].properties.szCs='20';assert.throws(()=>qa.assertUniformNormalFootnoteRuns(bad));
});

function styleOnlyMixedNative(){
  const value=sourceNative({format:'chicago',footStart:12000,footSize:10,footSizeCs:10}),manual=value.notes.footnote.definitions[0],end=value.notes.endnote.definitions[0];
  const run=(text,half,extra={})=>({text,properties:{sz:String(half),szCs:String(half),...extra}});
  manual.text=' SOURCE_FOOT_BODY strong and evidence.';Object.assign(manual.paragraphs[0],{text:manual.text,fields:[],runs:[run(' ',60),run('SOURCE_FOOT_BODY ',20),run('strong',20,{b:true}),run(' and ',20),run('evidence',20,{rStyle:'Hyperlink',u:'single'}),run('.',20)]});
  const generated=structuredClone(manual);generated.id='8';generated.text=' SOURCE_GENERATED_CITATION p. 12';generated.paragraphs[0].text=generated.text;generated.paragraphs[0].runs=[run(' ',60),run('SOURCE_GENERATED_CITATION',20),run(' p. 12',20)];value.notes.footnote.definitions.push(generated);value.notes.footnote.references.push(generated.id);
  end.text=' SOURCE_END_BODY End evidence.';Object.assign(end.paragraphs[0],{text:end.text,fields:[],runs:[run(' ',22),run('SOURCE_END_BODY End evidence.',22)]});
  value.paragraphs.push({text:'First, end, repeated. Generated .',fields:[]},{text:'Again end. Unicode 𝛂 remains authored.',fields:[]},{text:'SOURCE_GENERATED_CITATION',fields:[]});value.styleDefinitions={FootnoteText:{r:{sz:'60',szCs:'60'}}};value.notes.footnote.selectedSpecial=[{type:'separator',text:'',separatorElements:[]},{type:'continuationSeparator',text:'',separatorElements:['continuationSeparator']},{type:'continuationNotice',text:'SOURCE_CONTINUED <next> & evidence',separatorElements:[]}];return value;
}
test('style-only note return proves30pt template and10pt current manual/generated author axes independently',()=>{
  const actual=qa.assertStyleOnlyMixedNoteFixture(styleOnlyMixedNative());assert.deepEqual(actual.currentTemplateHalfPoints,[60,60]);assert.deepEqual(actual.currentAuthorHalfPoints,[20,20]);assert.deepEqual(actual.referenceCounts,[2,1,2]);assert.equal(actual.actualTextRunsChecked,9);
  for(const mutate of [
    row=>{row.notes.footnote.definitions[0].paragraphs[0].runs[1].properties.sz='60';},row=>{row.notes.footnote.definitions[0].paragraphs[0].runs[4].properties.szCs='60';},
    row=>{row.notes.footnote.definitions[1].paragraphs[0].runs[1].properties.sz='60';},row=>{row.notes.footnote.definitions[1].paragraphs[0].runs[2].properties.szCs='60';},
    row=>{row.notes.footnote.definitions[0].paragraphs[0].runs[0].properties.sz='20';},row=>{row.notes.footnote.definitions[1].paragraphs[0].runs[0].properties.szCs='20';},
    row=>{row.notes.footnote.definitions.pop();},row=>{row.notes.footnote.definitions.push(structuredClone(row.notes.footnote.definitions[1]));},row=>{row.notes.footnote.references.pop();},
    row=>{row.notes.footnote.references.reverse();},row=>{row.notes.endnote.references.pop();},row=>{row.notes.endnote.definitions[0].paragraphs[0].runs[1].properties.szCs='60';},
    row=>{row.notes.footnote.definitions[1].text=' SOURCE_GENERATED_CITATION p. 99';},row=>{row.notes.footnote.definitions[0].paragraphs[0].runs[2].properties.b=false;},
    row=>{row.notes.footnote.definitions[1].paragraphs[0].fields=[{instruction:' DATE ',cached:'SOURCE_GENERATED_CITATION'}];},row=>{row.styleDefinitions.FootnoteText.r.szCs='20';},
    row=>{row.notes.footnote.selectedSpecial.pop();},row=>{row.notes.footnote.selectedSpecial[2].text='stale';},row=>{row.paragraphs.pop();},row=>{row.paragraphs.push({text:'SOURCE_GENERATED_CITATION',fields:[]});},row=>{row.sourcePayloadUsedAsNativeEvidence=true;}
  ]){const bad=styleOnlyMixedNative();mutate(bad);assert.throws(()=>qa.assertStyleOnlyMixedNoteFixture(bad));}
  const plan=qa.planFor();assert(plan.cases.some(row=>row.includes('both manual and generated-note')));assert(!plan.openCases.some(row=>row.includes('mixed inline note sizes remain explicitly unsupported')));assert(plan.openCases.some(row=>row.includes('live-field')));
});

test('native multi-section positive requires both current positions, actual body boundary and story identities',()=>{
  assert.equal(qa.assertNativeMultiSection(multiSectionNative()).paragraphBoundaryVerified,true);
  for(const mutate of [row=>{row.nativeSections=1;},row=>{row.notes.footnote.sections.reverse();},row=>{row.notes.footnote.sections[1].values.pos='beneathText';},row=>{row.notes.endnote.sections[0]={values:{pos:'sectEnd'}};},row=>{row.notes.footnote.references=['7','8','8'];},row=>{row.notes.footnote.definitions.at(-1).id='7';},row=>{row.notes.footnote.definitions.at(-1).text='NATIVE_SECOND_SECTION_NOTE Stale body';},row=>{delete row.paragraphs[4].properties.sectPr;},row=>{row.paragraphs.splice(1,0,row.paragraphs.pop());},row=>{row.paragraphs[1].text='NATIVE_FIRST NATIVE_SECOND';},row=>{row.notes.footnote.settings.values.numRestart='continuous';}]){
    const bad=multiSectionNative();mutate(bad);assert.throws(()=>qa.assertNativeMultiSection(bad));
  }
});

test('source multi-section positive verifies actual top-level declarations and exact per-scope note labels',()=>{
  assert.deepEqual(qa.assertMultiSectionImport(MULTI_SECTION_SOURCE,MULTI_SECTION_PROFILE).sectionIds,['native-first','native-last']);
  for(const source of [MULTI_SECTION_SOURCE.replace('"pageBottom"','"beneathText"'),MULTI_SECTION_SOURCE.replace('"native-last"','"native-first"'),MULTI_SECTION_SOURCE.replace('"continuous"','"nextPage"'),MULTI_SECTION_SOURCE.replace('NATIVE_SECOND_SECTION [^g]','NATIVE_SECOND_SECTION [^f]'),MULTI_SECTION_SOURCE.replace('NATIVE_REPEAT [^f]','NATIVE_REPEAT [^g]'),MULTI_SECTION_SOURCE.replace('NATIVE_METHOD','LOST_METHOD'),MULTI_SECTION_SOURCE.replace(/^(<!-- readmd-section:[^\n]+ -->)/,'```html\n$1\n```'),MULTI_SECTION_SOURCE.replace('NATIVE_SECOND_SECTION [^g].','<!--NATIVE_SECOND_SECTION [^g].-->')])assert.throws(()=>qa.assertMultiSectionImport(source,MULTI_SECTION_PROFILE));
  const bad=structuredClone(MULTI_SECTION_PROFILE);bad.sourceValues.footnotes.kindByLabel.g='endnote';assert.throws(()=>qa.assertMultiSectionImport(MULTI_SECTION_SOURCE,bad));
  const plan=qa.planFor();assert(plan.cases.some(row=>row.includes('native-multi-section')&&row.includes('options:{}')));assert(!plan.openCases.some(row=>row.includes('source section integration')));
});

test('resized generated note keeps current visible content and exact source diagnostics',()=>{
  const diagnostics=[{code:'docx_citation_conflict',reason:'generated_citation_note_modified_or_unbound',part:'word/document.xml',original_source:'[@work, p. 12]',visible_content_preserved:true},{code:'docx_citation_conflict',reason:'citation_render_modified',part:'word/footnotes.xml',original_source:'[@work, p. 12]',visible_content_preserved:true}];
  const annotation='<!--readmd:citation-import:v1\n'+JSON.stringify({version:1,diagnostics})+'\n-->',markdown='Generated [^current-generated].\n\n[^current-generated]: SOURCE&#95;GENERATED&#95;CITATION p&#46; 12\n\n'+annotation+'\n';
  assert.equal(qa.assertChangedGeneratedNote(markdown).label,'current-generated');
  for(const bad of [markdown.replace('SOURCE&#95;GENERATED&#95;CITATION','HIDDEN'),markdown.replace('p&#46; 12','p&#46; 99'),markdown.replace('Generated [^current-generated]','Generated [@work, p. 12]'),markdown.replace('citation_render_modified','unchanged'),markdown.replace(annotation,'```html\n'+annotation+'\n```')])assert.throws(()=>qa.assertChangedGeneratedNote(bad));
});

test('native numbering clear rejects resurrected rules, fields and changed visible caches',()=>{
  const value=sourceNative();for(const row of value.paragraphs){delete row.rule;row.fields=[];row.number={numId:'0'};}
  value.paragraphs[0].style='Heading1';value.paragraphs[1].style='Heading2';
  value.paragraphs.push({text:'SETTINGS_BETA',style:'Heading1',fields:[],number:{numId:'0'}},{text:'SETTINGS_RESULTS',style:'Heading2',fields:[],number:{numId:'0'}});
  value.paragraphs.find(row=>row.text==='See Table III.4').text+='.';
  assert.equal(qa.assertNumberingCleared(value).nativeNumberingCleared,true);
  for(const mutate of [row=>{row.paragraphs[0].rule={format:'upperRoman'};},row=>{row.paragraphs[0].number={numId:'2'};},row=>{row.paragraphs[0].style='BodyText';},row=>{row.paragraphs[2].fields=[{instruction:' SEQ ReadMDTable ',cached:'4'}];},row=>{row.paragraphs[2].text='Table III.1 — SETTINGS_TABLE';}]){const bad=structuredClone(value);mutate(bad);assert.throws(()=>qa.assertNumberingCleared(bad));}
  const markdown='# SETTINGS_ALPHA {#settings-alpha}\n\n## SETTINGS_METHOD {#settings-method}\n\nTable III.4 — SETTINGS_TABLE\n\nSee Table III.4.\n\n# SETTINGS_BETA {#settings-beta}\n\n## SETTINGS_RESULTS {#settings-results}\n';
  qa.assertNumberingClearedImport(markdown);
  for(const prefix of ['<!-- readmd-numbering:v1 {"headings":{"levels":[{}]}} -->\n',':label[tbl:settings] SETTINGS_TABLE\n',':ref[tbl:settings]\n'])assert.throws(()=>qa.assertNumberingClearedImport(prefix+markdown));
});

test('native note clear uses Word defaults and rejects stale settings or selected old notice',()=>{
  const value=sourceNative();for(const story of Object.values(value.notes)){story.settings=null;story.sections=[null];story.selectedSpecial=[];}
  assert.equal(qa.assertNotesCleared(value,{input:true}).nativeNoteSettingsCleared,true);
  for(const mutate of [row=>{row.notes.footnote.settings={values:{numFmt:'upperRoman',numStart:'7'}};},row=>{row.notes.endnote.settings={values:{numFmt:'decimal',numStart:'1'}};},row=>{row.notes.footnote.sections=[{values:{pos:'beneathText'}}];},row=>{row.notes.footnote.selectedSpecial=[{type:'continuationNotice',text:'SOURCE_CONTINUED <next> & evidence'}];}]){const bad=structuredClone(value);mutate(bad);assert.throws(()=>qa.assertNotesCleared(bad));}
  const profile={sourceValues:{footnotes:{kinds:{footnote:{numberFormat:'decimal',start:1,restart:'continuous',placement:'pageBottom',separator:'none',continuationSeparator:'none',continuationNotice:''},endnote:{numberFormat:'lowerRoman',start:1,restart:'continuous',placement:'docEnd',separator:'none',continuationSeparator:'none',continuationNotice:''}}}}};
  qa.assertNotesClearedProfile(profile);const bad=structuredClone(profile);bad.sourceValues.footnotes.kinds.endnote.numberFormat='lowerLetter';assert.throws(()=>qa.assertNotesClearedProfile(bad));
});

test('same-id story merging, lost repeat, clamped note size/spacing and stale native changes fail',()=>{
  for(const mutate of [row=>{row.notes.endnote=row.notes.footnote;},row=>{row.notes.footnote.references.pop();},row=>{row.notes.footnote.settings.values.numStart='1';},row=>{row.notes.footnote.definitions[0].paragraphs[0].properties.spacing.line='252';},row=>{row.notes.footnote.definitions[0].paragraphs[0].properties.spacing.lineRule='auto';},row=>{row.notes.endnote.definitions[0].paragraphs[0].runs[0].properties.sz='18';},row=>{row.notes.endnote.definitions[0].paragraphs[0].properties.keepLines=false;},row=>{row.paragraphs[0].rule.text='%1';row.paragraphs[0].text='III SETTINGS_ALPHA';},row=>{row.paragraphs[2].fields=[];},row=>{row.paragraphs[3].fields=[];}]){const row=sourceNative();mutate(row);assert.throws(()=>qa.assertSourceNative(row));}
  assert.throws(()=>qa.assertSourceNative(sourceNative(),{mutated:true}));
});

test('source native checks reject stale chapter caches and REF caches or targets despite live field presence',()=>{
  const stale=sourceNative({mutated:true}),chapter=stale.paragraphs[2].fields.find(field=>/\bSTYLEREF\b/.test(field.instruction));
  if(chapter)chapter.cached='III';else stale.paragraphs[2].fields.unshift({instruction:' STYLEREF 1 \\s ',cached:'III'});
  assert.throws(()=>qa.assertSourceNative(stale,{mutated:true}));
  for(const mutate of [row=>{row.paragraphs[3].fields[0].instruction=' REF wrong_target \\h ';},row=>{row.paragraphs[3].fields[0].cached='Table 99';}]){const value=sourceNative();mutate(value);assert.throws(()=>qa.assertSourceNative(value));}
});

test('parameter-only chapter cache conflict stays literal while refreshed automatic source IDs remain editable',()=>{
  const diagnostic='<!--readmd-numbering-import:v1 [{"code":"docx_numbering_import_conflict","reason":"caption_result_modified","source_id":"tbl:settings","visible_content_preserved":true}]-->';
  const text='<span id="tbl:settings"></span>\n\nTable III.4 — SETTINGS_TABLE\n\nSee [Table III.4](#tbl:settings).\n\n'+diagnostic;
  assert.equal(qa.assertCaptionConflict(text).length,1);
  for(const bad of [text.replace('Table III.4 — SETTINGS_TABLE','Table Part v..4 — SETTINGS_TABLE'),text.replace('<span id="tbl:settings"></span>',':label[tbl:settings]'),text.replace('[Table III.4](#tbl:settings)','[Table III.4](#wrong)'),text.replace('caption_result_modified','unsupported_reference_field')])assert.throws(()=>qa.assertCaptionConflict(bad));
  const automatic=SOURCE_GFM_TABLE+'\n:label[tbl:settings] SETTINGS_TABLE\n\nSee :ref[tbl:settings].\n';qa.assertSourceCaptionIdentity(automatic);assert.throws(()=>qa.assertSourceCaptionIdentity(automatic.replace('tbl:settings','tbl:changed')));assert.throws(()=>qa.assertSourceCaptionIdentity('<!--\n'+automatic+'-->\nLiteral caption'));
});

test('candidate30 form-list import cannot masquerade as a table caption source',()=>{
  // Minimal verbatim failing shape, independent of local evidence folders.
  const imported='## SETTINGS_METHOD {#settings-method}\n\n- ****Measure****: **Value**\n- **SETTINGS_CELL**: 42\n\n:label[tbl:settings] SETTINGS_TABLE\n\nSee :ref[tbl:settings]&#46;\n';
  assert.throws(()=>qa.assertSourceCaptionIdentity(imported));
});

test('automatic source caption binds one adjacent top-level GFM table with exact cells',()=>{
  const tail='\n:label[tbl:settings] SETTINGS_TABLE\n\nSee :ref[tbl:settings]&#46;\n',source=SOURCE_GFM_TABLE+tail;
  assert.equal(qa.assertSourceCaptionIdentity(source).oneTable,true);qa.assertSourceCaptionIdentity(SOURCE_GFM_TABLE+'\n<!-- structural source comment -->\n'+tail);
  for(const table of [SOURCE_GFM_TABLE.replace('Measure','Other'),SOURCE_GFM_TABLE.replace('Value','Other'),SOURCE_GFM_TABLE.replace('SETTINGS_CELL','Other'),SOURCE_GFM_TABLE.replace('42','43'),SOURCE_GFM_TABLE+SOURCE_GFM_TABLE,'<!--\n'+SOURCE_GFM_TABLE+'-->\n','```md\n'+SOURCE_GFM_TABLE+'```\n',SOURCE_GFM_TABLE.split('\n').map(line=>'> '+line).join('\n')])assert.throws(()=>qa.assertSourceCaptionIdentity(table+tail));
  assert.throws(()=>qa.assertSourceCaptionIdentity(SOURCE_GFM_TABLE+'\nUnrelated intervening paragraph.\n'+tail));
  assert.throws(()=>qa.assertSourceCaptionIdentity(SOURCE_GFM_TABLE+tail.replace(':label[tbl:settings] SETTINGS_TABLE','`'+':label[tbl:settings] SETTINGS_TABLE'+'`')));
  assert.throws(()=>qa.assertSourceCaptionIdentity(SOURCE_GFM_TABLE+tail.replace('See :ref[tbl:settings]&#46;','`See :ref[tbl:settings].`')));
});

test('footnote placement uses section scope while endnote placement remains document scope',()=>{
  const value=sourceNative();value.notes.footnote.settings.values.pos='pageBottom';assert.equal(qa.kindProperties(value,'footnote').pos,'beneathText');value.notes.endnote.sections=[{values:{pos:'sectEnd'}}];assert.equal(qa.kindProperties(value,'endnote').pos,'docEnd');qa.assertSourceNative(value);
});

test('native-only validator distinguishes numbering rules from literal caches and ordinary lists',()=>{
  assert.equal(qa.assertNativeFixture(nativeFixture()).nativeOnlyInput,true);assert.equal(qa.assertNativeFixture(nativeFixture('native-zero-start'),{kind:'native-zero-start'}).startZero,true);
  for(const mutate of [row=>{row.paragraphs[0].rule.text='%1';},row=>{row.paragraphs[0].rule.suffix='space';},row=>{row.paragraphs[1].rule.start='1';},row=>{row.paragraphs[1].rule.text='%1.%2';},row=>{row.paragraphs.at(-1).style='Heading1';},row=>{row.notes.footnote.selectedSpecial[0].separatorElements=['separator'];},row=>{row.notes.footnote.definitions.find(note=>note.type==='continuationNotice').text='';}]){const value=nativeFixture();mutate(value);assert.throws(()=>qa.assertNativeFixture(value));}
});

test('standard caption positives retain SEQ sequence identity/start and live REF target; heading REF stays open',()=>{
  const value=nativeFixture('native-caption-mixed');assert.equal(qa.assertNativeFixture(value,{kind:'native-caption-mixed'}).nativeCaption,true);
  for(const mutate of [row=>{row.paragraphs[2].fields[0].instruction=' SEQ ReadMDTable \\r 4 ';},row=>{row.paragraphs[2].fields[0].instruction=' SEQ Table ';},row=>{row.paragraphs[2].text='Table 1 — NATIVE_CAPTION';},row=>{row.paragraphs[3].fields[0].instruction=' REF missing \\h ';},row=>{row.paragraphs[3].fields[0].cached='Table 9';}]){const control=nativeFixture('native-caption-mixed');mutate(control);assert.throws(()=>qa.assertNativeFixture(control,{kind:'native-caption-mixed'}));}
  const nested=nativeFixture('native-caption-mixed');nested.paragraphs[2].fields=[{instruction:' SEQ Table \\* ARABIC ',cached:'1'},{instruction:' =  + 3 \\* ARABIC ',cached:'4'}];qa.assertNativeFixture(nested,{kind:'native-caption-mixed'});
  assert(qa.planFor().openCases.some(value=>value.includes('heading-bookmark REF')));assert.equal(qa.NATIVE_POSITIVES.length,4);assert(qa.NATIVE_POSITIVES.every(kind=>kind.startsWith('native-caption-')));
});

test('heading REF open-case diagnostic is explicit and does not certify automatic native return',()=>{
  const text='NATIVE_FIELD [NATIVE_ALPHA](#native-alpha)\n\n<!--readmd-numbering-import:v1 [{"code":"docx_numbering_import_conflict","reason":"reference_preserved_as_link","source_id":"native-alpha","visible_content_preserved":true}]-->\n';
  assert.deepEqual(qa.numberingImportDiagnostics(text),[{code:'docx_numbering_import_conflict',reason:'reference_preserved_as_link',source_id:'native-alpha',visible_content_preserved:true}]);assert.deepEqual(qa.numberingImportDiagnostics('NATIVE_ALPHA'),[]);assert.throws(()=>qa.numberingImportDiagnostics('<!--readmd-numbering-import:v1 [not valid]-->'));
  const current='# NATIVE_ALPHA {#docx-current}\n\nNATIVE_FIELD [NATIVE_ALPHA](#docx-current)\n\n<!--readmd-numbering-import:v1 [{"code":"docx_numbering_import_conflict","reason":"reference_preserved_as_link","source_id":"docx-current","visible_content_preserved":true}]-->\n';
  assert.equal(qa.assertHeadingReferenceOpen(current).length,1);
  for(const changed of [current.replace('[NATIVE_ALPHA](#docx-current)','NATIVE_ALPHA'),current.replace('[NATIVE_ALPHA](#docx-current)','[NATIVE_ALPHA](#wrong)'),current.replace('{#docx-current}','{#another}'),current.replace('reference_preserved_as_link','unsupported_reference_field'),current.replace('"visible_content_preserved":true','"visible_content_preserved":false')])assert.throws(()=>qa.assertHeadingReferenceOpen(changed));
});

test('source profile note mapping uses exact current labels and never guesses by prefix',()=>{
  const markdown='T[^same-f] and [^same-e].\n\n[^same-f]: SOURCE_FOOT_BODY\n\n[^same-e]: SOURCE_END_BODY\n',profile={ok:true,sourceValues:{version:1,footnotes:{kindByLabel:{'same-f':'footnote','same-e':'endnote'}}}};
  assert.deepEqual(qa.assertProfileMatchesNotes(profile,markdown),{footnoteLabel:'same-f',endnoteLabel:'same-e'});assert.throws(()=>qa.assertProfileMatchesNotes({ok:true,sourceValues:{version:1,footnotes:{kindByLabel:{'word-footnote-7':'footnote','word-endnote-7':'endnote'}}}},markdown));assert.throws(()=>qa.assertProfileMatchesNotes(profile,markdown+'\n[^other]: SOURCE_FOOT_BODY\n'));
});

test('current note labels follow multiline definitions after native paragraph layout comments',()=>{
  const markdown='---\nreadmd: {publication: {version: 1}}\n---\nText[^current-f][^current-e].\n\n[^current-f]: <!-- readmd:layout v1 {"paragraph":{"keepNext":true}} -->\n\n    SOURCE_FOOT_BODY **strong** and current evidence.\n\n[^current-e]: <!-- readmd:layout v1 {"paragraph":{"widowControl":false}} -->\n\n    SOURCE_END_BODY Current end evidence.\n';
  const profile={ok:true,sourceValues:{version:1,footnotes:{kindByLabel:{'current-f':'footnote','current-e':'endnote'}}}};
  assert.deepEqual(qa.assertProfileMatchesNotes(profile,markdown),{footnoteLabel:'current-f',endnoteLabel:'current-e'});
  const duplicate=markdown+'\n[^other-f]: <!-- readmd:layout v1 {} -->\n\n    SOURCE_FOOT_BODY Duplicate current definition.\n';
  assert.throws(()=>qa.assertProfileMatchesNotes(profile,duplicate),/Missing\/duplicate/);
});

test('note identity checks exclude comments, code samples and unrelated unindented paragraphs',()=>{
  assert.throws(()=>qa.noteLabel('[^a]: <!-- SOURCE_FOOT_BODY is old metadata -->\n','SOURCE_FOOT_BODY'),/Missing\/duplicate/);
  assert.throws(()=>qa.noteLabel('[^a]: Other content.\n\nSOURCE_FOOT_BODY belongs to the body.\n','SOURCE_FOOT_BODY'),/Missing\/duplicate/);
  const markdown='```md\n[^old]: SOURCE_FOOT_BODY example\n```\n\n[^current]: Current content.\n\n    SOURCE_FOOT_BODY Current continuation.\n';
  assert.equal(qa.noteLabel(markdown,'SOURCE_FOOT_BODY'),'current');
});

test('mutation evidence binds package bytes and proves provenance and unrelated members unchanged',()=>{
  const before={sha256:'input',parts:{'word/settings.xml':'old','customXml/readmd-source.xml':'source','word/styles.xml':'styles'},auxiliaryParts:{'customXml/readmd-source.xml':'source'}},after={sha256:'output',parts:{'word/settings.xml':'new','customXml/readmd-source.xml':'source','word/styles.xml':'styles'},auxiliaryParts:{'customXml/readmd-source.xml':'source'}},report={inputSha256:'input',outputSha256:'output',metadataResigned:false,applicationEditingCertified:false,changedParts:['word/settings.xml'],unchangedParts:{'customXml/readmd-source.xml':'source','word/styles.xml':'styles'}};
  qa.assertMutation(report,before,after);for(const mutate of [row=>{row.metadataResigned=true;},row=>{row.inputSha256='unbound';},row=>{row.changedParts=[];},row=>{row.unchangedParts['word/styles.xml']='different';}]){const value=structuredClone(report);mutate(value);assert.throws(()=>qa.assertMutation(value,before,after));}
  assert.throws(()=>qa.assertMutation(report,before,{...after,auxiliaryParts:{'customXml/readmd-source.xml':'resigned'}}));
});

const authorSpan=(content,attributes='data-size-pt="9.5" data-size-cs-pt="9.5"')=>'<span data-readmd-text="v1" '+attributes+'>'+content+'</span>';
const authoredTable='| '+authorSpan('**Measure**')+' | '+authorSpan('**Value**')+' |\n| --- | --- |\n| '+authorSpan('SETTINGS_CELL')+' | '+authorSpan('42')+' |\n';
const captionTail='\n:label[tbl:settings] SETTINGS_TABLE\n\nSee :ref[tbl:settings]&#46;\n';

test('candidate41 exact table text and all heading identities survive only controlled author spans',()=>{
  const source='\uFEFF---\r\nreadmd: {publication: {version: 1, typography: {size: 10.5}}}\r\n---\r\n\r\n# '+authorSpan('SETTINGS_ALPHA','data-size-pt="18" data-size-cs-pt="18"')+' {#settings-alpha}\n\n## '+authorSpan('SETTINGS_METHOD','data-size-pt="14.5" data-size-cs-pt="14.5"')+' {#settings-method}\n\n'+authoredTable+captionTail+'\n# '+authorSpan('SETTINGS_BETA','data-size-pt="18" data-size-cs-pt="18"')+' {#settings-beta}\n\n## '+authorSpan('SETTINGS_RESULTS','data-size-pt="14.5" data-size-cs-pt="14.5"')+' {#settings-results}\n';
  assert.equal(qa.assertSourceCaptionIdentity(source).oneTable,true);
  assert.deepEqual(qa.markdownStructure(source).headings,[{text:'SETTINGS_ALPHA',id:'settings-alpha',level:1},{text:'SETTINGS_METHOD',id:'settings-method',level:2},{text:'SETTINGS_BETA',id:'settings-beta',level:1},{text:'SETTINGS_RESULTS',id:'settings-results',level:2}]);
  for(const bad of [source.replace('SETTINGS_CELL','OTHER_CELL'),source.replace('>42<','>43<'),source.replace(authoredTable,authoredTable+authoredTable),source.replace(captionTail,'\nUnrelated text.\n'+captionTail)])assert.throws(()=>qa.assertSourceCaptionIdentity(bad));
});

test('owned span grammar retains independent missing axes, base and exact explicit false without accepting injection',()=>{
  assert.deepEqual(qa.controlledSpanTag('<SPAN DATA-READMD-TEXT=\'v1\' DATA-SIZE-PT=\'11\' data-cs="false" data-rtl="false">').style,{size:11,cs:false,rtl:false});
  assert.deepEqual(qa.controlledSpanTag('<span data-readmd-text="v1" data-size-cs-pt="base">').style,{sizeCs:'base'});
  for(const value of ['0.5','9.5','11.0','1073741823.5'])assert.equal(qa.controlledSpanTag('<span data-readmd-text="v1" data-size-pt="'+value+'">').style.size,Number(value));
  const invalid=['data-readmd-text="v2"','data-readmd-text="v1" DATA-READMD-TEXT="v1"','data-readmd-text="v1" data-size-pt="11" DATA-SIZE-PT="12"','data-readmd-text="v1" style="font-size:11pt"','data-readmd-text="v1" hidden="hidden"','data-readmd-text="v1" class="owned"','data-readmd-text="v1" id="identity"','data-readmd-text="v1" onclick="alert(1)"','data-readmd-text="v1" data-future="11"','data-readmd-text="v1" data-cs="0"','data-readmd-text="v1" data-rtl="False"','data-readmd-text=v1','data-readmd-text="v1"data-cs="false"'];
  for(const attributes of invalid)assert.throws(()=>qa.controlledSpanTag('<span '+attributes+'>'));
  for(const value of ['0','-1','9.25','NaN','Infinity','1073741824','0x10','&num;11',''])assert.throws(()=>qa.controlledSpanTag('<span data-readmd-text="v1" data-size-pt="'+value+'">'));
  for(const tag of ['<span data-readmd-text="v1"/>','<span data-readmd-text="v1"','<span data-readmd-text="v1">tail','<span data-readmd-text="v1" data-size-pt="'+'1'.repeat(4096)+'">'])assert.throws(()=>qa.controlledSpanTag(tag));
});

test('first-run controlled spans preserve parsed native heading REF identity with no invented complex size',()=>{
  const diagnostic='<!--readmd-numbering-import:v1 [{"code":"docx_numbering_import_conflict","reason":"reference_preserved_as_link","source_id":"docx-current","visible_content_preserved":true}]-->';
  const source='# '+authorSpan('NATIVE&#95;ALPHA','data-size-pt="11"')+' {#docx-current}\n\n'+authorSpan('NATIVE_FIELD [NATIVE&#95;ALPHA](<#docx&#45;current>)','data-size-pt="11"')+'\n\n'+diagnostic+'\n';
  assert.equal(qa.assertHeadingReferenceOpen(source).length,1);
  for(const bad of [source.replace('<#docx&#45;current>','<#wrong>'),source.replace('{#docx-current}','{#wrong}'),source+'\n# NATIVE_ALPHA {#docx-current}\n',source.replace('data-size-pt="11"','data-size-pt="11" style="display:none"'),source.replace(diagnostic,authorSpan(diagnostic,'data-size-pt="11"'))])assert.throws(()=>qa.assertHeadingReferenceOpen(bad));
});

test('paired nested controlled spans remain transparent while ordinary HTML, code, images and crossed scopes do not',()=>{
  const marked=require('../../assets/vendor/marked.min.js'),inline=source=>qa.inlineStructure(marked.Lexer.lexInline(source,{gfm:true}));
  const good=authorSpan('**'+authorSpan('Measure','data-size-cs-pt="base" data-cs="false"')+'** and [Value](<#target>)','data-size-pt="11"');
  const row=inline(good);assert.equal(row.supported,true);assert.equal(row.text,'Measure and Value');assert.deepEqual(row.links,[{text:'Value',target:'#target'}]);
  for(const bad of [good.replace('</span>',''),'</span>'+good,good+'</span>',authorSpan('<span>Measure</span>'),authorSpan('<em>Measure</em>'),authorSpan('<img src="x" alt="Measure">'),authorSpan('![Measure](x)'),authorSpan('`Measure`'),'<span data-readmd-text="v1" data-size-pt="11">**Measure</span>**','**<span data-readmd-text="v1" data-size-pt="11">Measure**</span>',authorSpan('Measure','data-size-pt="11" style="display:none"')])assert.equal(inline(bad).supported,false,bad);
  for(const replacement of ['<span hidden>**Measure**</span>','`Measure`','![Measure](x)',authorSpan('<span hidden>Measure</span>')])assert.throws(()=>qa.assertSourceCaptionIdentity(authoredTable.replace(authorSpan('**Measure**'),replacement)+captionTail));
});

test('transparent author tags never decode escaped source directives into live caption or REF identities',()=>{
  const automatic=authoredTable+'\n'+authorSpan(':label[tbl:settings] SETTINGS_TABLE','data-size-pt="11"')+'\n\nSee '+authorSpan(':ref[tbl:settings]','data-size-pt="11"')+'&#46;\n';qa.assertSourceCaptionIdentity(automatic);
  for(const bad of [automatic.replace(':label[','&#58;label['),automatic.replace(':label[','\\:label['),automatic.replace(':ref[','&#58;ref['),automatic.replace(':ref[','\\:ref['),automatic.replace('tbl:settings','tbl:wrong'),automatic+'\nSee :ref[tbl:settings].\n'])assert.throws(()=>qa.assertSourceCaptionIdentity(bad));
  const literal=LITERAL_SOURCE.replace('Table III.4 — SETTINGS_TABLE',authorSpan('Table III&#46;4 — SETTINGS_TABLE','data-size-pt="11"')).replace('[Table III.4](#tbl:settings)','['+authorSpan('Table III&#46;4','data-size-pt="11"')+'](#tbl:settings)');assert.equal(qa.assertCaptionConflict(literal).length,1);
});

test('actual marked type7 remains inline-only and cannot promote nested source declarations or headings',()=>{
  const marked=require('../../assets/vendor/marked.min.js'),source='<span data-readmd-text="v1" data-size-pt="11">\nNATIVE_FIELD [NATIVE_ALPHA](#docx-current)\n</span>\n';
  assert.equal(marked.lexer(source,{gfm:true})[0].type,'html');const tokens=qa.markdownTokens(source);assert.equal(tokens[0].type,'paragraph');assert.equal(tokens[0].controlledBlock,true);const view=qa.markdownStructure(source);assert.equal(view.paragraphs.length,1);assert.equal(view.paragraphs[0].text,'\nNATIVE_FIELD NATIVE_ALPHA\n\n');assert.deepEqual(view.paragraphs[0].links,[{text:'NATIVE_ALPHA',target:'#docx-current'}]);
  for(const body of ['# NATIVE_ALPHA {#docx-current}','<!-- readmd-numbering:v1 {"headings":{}} -->',LITERAL_DIAGNOSTIC]){const nested='<span data-readmd-text="v1" data-size-pt="11">\n'+body+'\n</span>\n',current=qa.markdownStructure(nested);assert.deepEqual(current.headings,[]);assert.deepEqual(current.numberingDeclarations,[]);assert.deepEqual(current.diagnostics,[]);}
  for(const bad of ['<div>\n'+source+'</div>\n',source.replace('</span>','</div>'),source+'<script>bad</script>\n',source.replace('data-size-pt="11"','style="display:none"')])assert.equal(qa.markdownTokens(bad)[0].type,'html');
});

test('ordinary legacy inline evidence receives no new author node budget',()=>{
  const text={type:'text',raw:'x',text:'x'},many=Array(32769).fill(text);assert.equal(qa.inlineStructure(many).supported,true);assert.equal(qa.inlineStructure(many).text.length,many.length);
  const controlled=[{type:'html',raw:'<span data-readmd-text="v1" data-size-pt="11">'},...many,{type:'html',raw:'</span>'}],result=qa.inlineStructure(controlled);assert.equal(result.supported,false);assert.match(result.error,/Bounded owned inline nodes/);
  const nested=Array(33).fill('<span data-readmd-text="v1" data-size-pt="11">').join('')+'x'+'</span>'.repeat(33),marked=require('../../assets/vendor/marked.min.js');assert.equal(qa.inlineStructure(marked.Lexer.lexInline(nested)).supported,false);
});

test('style-only source fallback preserves current generated note, independent binding and whole literal bibliography',()=>{
  const profile=structuredClone(qa.PROFILE);Object.assign(profile.footnotes.kinds.footnote,{size:30,sizeCs:30,start:12000,numberFormat:'chicago'});profile.footnotes.kinds.endnote.sizeCs=11;profile.footnotes.kindByLabel.currentGenerated='footnote';
  const diagnostics=[...['word/document.xml','word/footnotes.xml'].map((part,index)=>({code:'docx_citation_conflict',reason:'generated_citation_note_author_format_not_reproducible',part,binding:'owned-'+index,original_source:'[@work, p. 12]',current_source:'[@work, p. 12]',visible_content_preserved:true})),{code:'docx_citation_conflict',reason:'citation_family_author_format_not_reproducible',part:'word/document.xml',binding:'owned-bibliography',visible_content_preserved:true}];
  const annotation='<!--readmd:citation-import:v1\n'+JSON.stringify({version:1,diagnostics})+'\n-->';
  const markdown=qa.sourceFor(profile).replace('repeated[^manual-f].','repeated[^manual-f]. Generated [^currentGenerated].')+'\n[^currentGenerated]: <span data-readmd-text="v1" data-size-pt="10" data-size-cs-pt="10">SOURCE_GENERATED_CITATION p. 12</span>\n\n## References\n\nSOURCE_GENERATED_CITATION\n\n'+annotation+'\n',current={ok:true,sourceValues:profile};
  assert.equal(qa.assertStyleOnlyMixedNoteImport(markdown,current).currentGeneratedNoteLabel,'currentGenerated');
  for(const text of [markdown.replace('[^currentGenerated]:','[^other]:'),markdown.replace(' p. 12</span>',' p. 99</span>'),markdown.replace('\nSOURCE_GENERATED_CITATION\n','\nLOST_BIBLIOGRAPHY\n'),markdown.replace('generated_citation_note_author_format_not_reproducible','citation_render_modified'),markdown.replace(annotation,'\x60\x60\x60html\n'+annotation+'\n\x60\x60\x60'),markdown.replace('word/footnotes.xml','word/endnotes.xml')])assert.throws(()=>qa.assertStyleOnlyMixedNoteImport(text,current));
  const wrong=structuredClone(current);wrong.sourceValues.footnotes.kindByLabel.currentGenerated='endnote';assert.throws(()=>qa.assertStyleOnlyMixedNoteImport(markdown,wrong));
});

function nativeMultiSectionReferencePair(){
  const before=multiSectionNative();before.documentPart='word/document.xml';before.parts={'word/document.xml':'old-document','word/styles.xml':'same-styles','word/notes/footnote-custom.xml':'same-foot','word/notes/endnote-custom.xml':'same-end'};before.bytes=1000;before.sha256='old-package';
  before.paragraphs[0].bookmarks=[{id:'11',name:'native-alpha'}];before.paragraphs[0].runs=[{text:'NATIVE_ALPHA',properties:{sz:'22'}}];before.paragraphs[3].runs=[{text:'NATIVE_FIELD ',properties:{sz:'22'}},{text:'NATIVE_ALPHA',properties:{sz:'22'}}];before.paragraphs[3].fields=[{instruction:' REF native-alpha \\h ',cached:'NATIVE_ALPHA',kind:'simple',depth:0}];
  before.sectionScopes=[{ordinal:0,bodyRange:[0,5],terminatorPath:'/w:body/[4]/w:pPr/w:sectPr',break:'continuous',text:'NATIVE_ALPHA\nNATIVE_FIELD NATIVE_ALPHA',bookmarks:[{id:'11',name:'native-alpha'}],hyperlinks:[],notes:{footnote:{sparse:{pos:'beneathText'}}}},{ordinal:1,bodyRange:[5,6],terminatorPath:'/w:body/[6]',break:'nextPage',text:'NATIVE_SECOND_SECTION',bookmarks:[],hyperlinks:[],notes:{footnote:{sparse:{pos:'pageBottom'}}}}];
  const after=structuredClone(before);after.parts['word/document.xml']='new-document';after.bytes=1001;after.sha256='new-package';after.paragraphs[0].bookmarks[0].name='native_alpha';after.paragraphs[3].fields[0].instruction=' REF native_alpha \\h ';after.sectionScopes[0].bookmarks[0].name='native_alpha';return {before,after};
}
test('safe multi-section reference fixture keeps a real REF and differs from legacy only in its two bound names',()=>{const {before,after}=nativeMultiSectionReferencePair();assert.equal(qa.assertSafeMultiSectionFixture(before,after).liveRefRetained,true);assert.equal(before.paragraphs[3].fields[0].instruction,' REF native-alpha \\h ');});
for(const [name,change]of Object.entries({
  missingField:v=>v.paragraphs[3].fields=[],wrongCache:v=>v.paragraphs[3].fields[0].cached='ALTERED',wrongTarget:v=>v.paragraphs[3].fields[0].instruction=' REF wrong \\h ',
  missingBookmark:v=>v.paragraphs[0].bookmarks=[],changedWestern:v=>v.paragraphs[3].runs[1].properties.sz='60',inventedComplexAxis:v=>v.paragraphs[3].runs[1].properties.szCs='22',
  changedStylesPart:v=>v.parts['word/styles.xml']='different',changedPlacement:v=>v.notes.footnote.sections[0].values.pos='pageBottom',changedBody:v=>v.paragraphs[3].text='LOST_FIELD',
  missingSectionScopes:v=>delete v.sectionScopes,missingScopedBookmark:v=>v.sectionScopes[0].bookmarks=[],wrongScopedBookmark:v=>v.sectionScopes[0].bookmarks[0].name='wrong',
  duplicateScopedBookmark:v=>v.sectionScopes[0].bookmarks.push({...v.sectionScopes[0].bookmarks[0]}),movedScopedBookmark:v=>v.sectionScopes[1].bookmarks.push(v.sectionScopes[0].bookmarks.pop()),
  changedScopedId:v=>v.sectionScopes[0].bookmarks[0].id='12',extraSecondScopedBookmark:v=>v.sectionScopes[1].bookmarks.push({id:'12',name:'native-alpha'}),
  changedScopedBody:v=>v.sectionScopes[0].text='LOST_FIELD',changedScopedPlacement:v=>v.sectionScopes[0].notes.footnote.sparse.pos='pageBottom',changedScopedTerminator:v=>v.sectionScopes[0].terminatorPath='/w:body/[3]',
}))test('safe multi-section fixture rejects '+name,()=>{const {before,after}=nativeMultiSectionReferencePair();change(after);assert.throws(()=>qa.assertSafeMultiSectionFixture(before,after));});
function legacyMultiSectionError(){const value={ok:false,error_code:'author_inline_text_unsupported',error:'author_inline_text_unsupported',error_details:{part:'word/document.xml',path:'[3, 1, 0]',detail:'author-sized live field result has no editable field-run size model'}};return {isError:true,structuredContent:value,content:[{type:'text',text:JSON.stringify(value)}]};}
test('actual candidate44 legacy REF refusal remains a typed negative with the precise field-run location',()=>{assert.equal(qa.assertLegacyMultiSectionReferenceRejection(legacyMultiSectionError()).code,'author_inline_text_unsupported');for(const change of [v=>v.error_details.part='word/styles.xml',v=>v.error_details.path='[0]',v=>v.error_details.detail='possibly damaged',v=>v.error_code=v.error='conversion_failed']){const reply=legacyMultiSectionError();change(reply.structuredContent);reply.content[0].text=JSON.stringify(reply.structuredContent);assert.throws(()=>qa.assertLegacyMultiSectionReferenceRejection(reply));}});
test('safe multi-section import also requires the preserved current heading link and conflict diagnostic',()=>{
  const diagnostic='<!--readmd-numbering-import:v1 [{"code":"docx_numbering_import_conflict","reason":"reference_preserved_as_link","source_id":"docx-current","visible_content_preserved":true}]-->',source=MULTI_SECTION_SOURCE.replaceAll('native-alpha','docx-current')+'\n'+diagnostic+'\n';
  assert.equal(qa.assertSafeMultiSectionImport(source,MULTI_SECTION_PROFILE).currentHeadingLinkPreserved,true);
  for(const wrong of [source.replace('[NATIVE_ALPHA](#docx-current)','[NATIVE_ALPHA](#wrong)'),source.replace(diagnostic,'```html\n'+diagnostic+'\n```'),source.replace('NATIVE_FIELD [NATIVE_ALPHA](#docx-current)','NATIVE_FIELD `[NATIVE_ALPHA](#docx-current)`')])assert.throws(()=>qa.assertSafeMultiSectionImport(wrong,MULTI_SECTION_PROFILE));
});
