'use strict';
// Candidate-bound, real stdio DOCX settings return.  OPC edits are explicitly
// headless test operations, not an Office application interoperability claim.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {spawnSync}=require('node:child_process');
const {StdioClient,assertToolCallSchema,verifyBinding,systemEnvironment}=require('./batch3-semantics-stdio.cjs');
const {payload}=require('./managed-tex-process-cold.cjs');
const marked=require('../../assets/vendor/marked.min.js');
const htmlReferences=require('../../assets/vendor/html5-character-references.json');
const sectionNoteCases=require('./mcp-docx-section-note-numbering-cases.cjs');
const ROOT=path.resolve(__dirname,'../..'),BASE=path.join(ROOT,'.cache/capability-upgrade/batch3-20261020/settings'),SECTION_BASE=path.join(ROOT,'.cache/capability-upgrade/batch3-caption-section-20261023/settings');
const ORACLE='tools/test/inspect-docx-settings-roundtrip.py';
const QA_FILES=['tools/test/mcp-docx-settings-roundtrip.cjs','tools/test/mcp-docx-settings-roundtrip.test.cjs','tools/test/mcp-docx-section-note-numbering-cases.cjs','tools/test/mcp-docx-section-note-numbering-cases.test.cjs',ORACLE,'tools/test/inspect-docx-settings-roundtrip.test.py','tools/test/batch3-semantics-stdio.cjs','tools/test/managed-tex-process-cold.cjs','assets/vendor/marked.min.js','assets/vendor/html5-character-references.json'];
const PRODUCT_FILES=['publication_profile.rs','publication_note_policies.rs','publication_sections.rs','publication_metadata.rs','publication_typography.rs','export_styles.rs','export_profiles.rs','publication_layout.rs','publication.rs','mdexport.rs','mcp.rs','server.rs','export_preview.rs','convert.rs','conversion_errors.rs','convert/docx_note_settings.rs','convert/docx_text_style.rs','publication_inline_text.rs','publication_rich.rs','publication_blocks.rs','docx_local_style.rs','convert/docx_numbering_import.rs','convert/docx_notes.rs','convert/docx_citations.rs','publication_docx_citations.rs','docx_writer.rs','epub_writer.rs','typst_writer.rs','latex_writer.rs','html_footnotes.rs','pdf_render.rs','md_ast.rs','cross_references.rs','lib.rs'].map(name=>'rust/readmd-kernel/src/'+name);
PRODUCT_FILES.push(...['document_intelligence.rs','document_transaction.rs','document_history.rs','text_encoding.rs'].map(name=>'rust/readmd-kernel/src/'+name));
const NUMBERING={headings:{levels:[{format:'upperRoman',start:3,restartAfter:0},{format:'lowerLetter',start:2,restartAfter:1}],separator:'.',suffix:'space'},captions:{table:{scope:'chapter',format:'decimal',start:4,separator:'.'}}};
const PROFILE={version:1,footnotes:{restart:'continuous',kinds:{
  footnote:{numberFormat:'upperRoman',start:7,size:10,lineSpacing:{mode:'exact',pt:10.5},placement:'beneathText',separator:'none',continuationSeparator:'line',continuationNotice:'SOURCE_CONTINUED <next> & evidence',longNote:'split'},
  endnote:{numberFormat:'lowerLetter',start:3,size:11,lineSpacing:{mode:'atLeast',pt:13},placement:'docEnd',separator:'line',continuationSeparator:'none',longNote:'keepParagraphs'}},
  kindByLabel:{'manual-f':'footnote','manual-e':'endnote'}}};
const PRESET='QA_NOTE_OVERRIDE',PRESET_DATA={custom:{[PRESET]:{footnotes:{start:23,size:12}}},last:{fmt:'docx',preset:PRESET,options:{footnotes:{start:23,size:12}}}};
const NATIVE_POSITIVES=['native-caption-mixed','native-caption-note-mixed','native-caption-numbering','native-caption-zero-start'];
const hash=value=>crypto.createHash('sha256').update(value).digest('hex'),fileHash=file=>hash(fs.readFileSync(file));
function inside(base,file){const relative=path.relative(base,path.resolve(file));assert(relative&&relative!=='..'&&!relative.startsWith('..'+path.sep)&&!path.isAbsolute(relative),'Path escaped owned evidence scope: '+file);return path.resolve(file);}
function argumentsFor(argv,env=process.env){
  const options={candidate:env.READMD_MCP_CANDIDATE,binary:env.READMD_BIN,assets:env.READMD_ASSETS_DIR,python:env.READMD_TEST_PYTHON,plan:false};
  for(let index=0;index<argv.length;index++){
    const key=argv[index];if(key==='--plan'){options.plan=true;continue;}if(key==='--help')return {help:true};
    assert(['--candidate','--binary','--assets','--python','--output'].includes(key),'Unknown argument '+key);assert(index+1<argv.length,'Missing argument '+key);options[key.slice(2)]=argv[++index];
  }
  for(const key of ['candidate','binary','assets','python','output'])if(options[key])options[key]=path.resolve(options[key]);
  if(options.output){const base=[BASE,SECTION_BASE].find(base=>{try{inside(base,options.output);return true;}catch{return false;}});assert(base,'Path escaped owned evidence scope: '+options.output);}
  if(!options.plan){assert(options.candidate,'--candidate is required');assert(options.python,'Existing QA-only --python is required');}return options;
}
function planFor(){return {schemaVersion:1,scope:'DOCX source-owned editable numbering and note settings',transport:'real-stdio',synthetic:true,noProcessesStarted:true,noFilesWritten:true,
  ownedProfileAndDocuments:true,system32OnlyPath:true,guiStarted:false,applicationRoundtripCertified:false,headlessMutationsAreWordInterop:false,
  sourceContract:'readmd.publication.version=1; typography.size; footnotes.kinds.footnote/endnote; exact kindByLabel; readmd-section:v1 footnotes.kinds.footnote.placement and both kinds numberFormat/numberPattern/start/restart; existing readmd-numbering:v1',
  sectionNoteNumbering:sectionNoteCases.plan(),
  sequence:['Real source export and hand-authored native-only OPC fixtures','Native-only mutations leave source/provenance members byte-identical','Real convert returns only in-memory Markdown','Revision-guarded readmd_edit_document explicitly saves an owned Markdown file','Normal stdin EOF and exit zero','A different real process reopens actual saved bytes','Second export has options:{} and omits style_preset','Independent ZIP/XML verifies effective native fields, settings, styles, relationships and stories'],
  cases:['source mixed notes and source numbering','source retained while native numbering/note parameters change','native numbering disabled and live fields frozen at their current visible results while auxiliary source bytes stay unchanged','native note properties deleted globally and per section while both stories and auxiliary source stay unchanged; native defaults cannot resurrect old source settings','native-only heading style inheritance and ordinary list separation','native-only mixed stories sharing normal id=7 with repeated footnote reference','per-level text/suffix/start including zero','exact and atLeast note spacing, inherited font sizes and longNote distinct from keepNext','section footnote placement and document endnote placement','nondefault story paths and referenced special IDs 41/42/43','legal chicago/start12000/30pt survive without old UI clamping; style-only30pt templates retain current10pt authors in both manual and generated-note content through real save/reopen/export; exact typed invalid/unsupported errors retain bounded part/path/detail with matching MCP text','native-multi-section legacy hyphen REF is a typed unsupported negative without changing its native file; native-multi-section-safe-ref keeps its real field/cache and beneathText/pageBottom, imports, explicitly saves, reopens in another process and exports with options:{} while preserving section/body/reference identities','source > implicit fallback; explicit selected preset and options > source; null inherits','explicit default source values remain explicit; no-note documents retain only their consumed global font baseline, without unused note/numbering metadata; 10.5/11/14pt survive new-process export with options:{}','bounded source patch preserves BOM/CRLF/unknown YAML, inverse and stale revision rejection','owned preset/Last file never absorbs document source values'],
  requiredTools:['readmd_export_document','readmd_export_epub','readmd_convert_to_markdown','readmd_read_document','readmd_edit_document','readmd_publication_profile','readmd_publication_numbering','readmd_content_transaction','readmd_export_presets','readmd_academic_document'],
  openCases:['arbitrary heading-bookmark REF must retain current text/link plus diagnostic; existing source language does not yet recreate automatic heading REF semantics','section font/separator, endnote placement/suppression, actual Word eachPage/endnote pagination and complex custom-format display remain outside P06-C native numbering certification','ordinary author-run note sizes have a readable source model; mixed or unknown live-field result author axes remain strict unsupported boundaries covered by author-inline QA, and this settings run does not certify arbitrary semantic-field formatting','ordinary list remains a list instead of an outline heading; existing %1) to Markdown to %1. punctuation projection is not certified as exact list-format return'],
  qaFiles:QA_FILES,productFiles:PRODUCT_FILES,fullWordNumberingCertified:false,fullBatch3Certified:false};}
function bindingFor(options){
  const candidate=JSON.parse(fs.readFileSync(options.candidate,'utf8')),binary=options.binary||candidate.binary,assets=options.assets||path.dirname(candidate.boot),boot=path.join(assets,'readmd.boot.js');
  const sourceFreeze=inside(ROOT,path.resolve(ROOT,candidate.sourceFreeze));assert.equal(fileHash(sourceFreeze),candidate.sourceFreezeSha256,'Source freeze changed');
  const freeze=JSON.parse(fs.readFileSync(sourceFreeze,'utf8')),boundFiles={};
  for(const name of [...QA_FILES,...PRODUCT_FILES]){const row=freeze.sources.find(row=>row.path===name);assert(row,'Candidate did not freeze '+name);boundFiles[name]={file:inside(ROOT,path.join(ROOT,name)),copy:inside(ROOT,path.join(ROOT,row.copy)),sha256:row.sha256};}
  const binding={candidate:options.candidate,candidateSha256:fileHash(options.candidate),binary,assets,boot,binarySha256:candidate.binarySha256,bootSha256:candidate.bootSha256,sourceFreeze,sourceFreezeSha256:candidate.sourceFreezeSha256,boundFiles};verifyBinding(binding);return binding;
}
function plainText(result){assert.notEqual(result.isError,true,JSON.stringify(result.structuredContent||result.content));const text=(result.content||[]).filter(row=>row.type==='text').map(row=>row.text).join('\n');assert(text.trim(),'Empty conversion');return text;}
function toolValue(result){return result.structuredContent||JSON.parse(result.content.find(row=>row.type==='text').text);}
function sourceReadArguments(file,source){const text=source.startsWith('\uFEFF')?source.slice(1):source,lines=Math.max(1,text.split('\n').length-Number(text.endsWith('\n')));assert(lines<=1000,'Fixture exceeds real read bound');return {file_path:file,start_line:1,end_line:lines};}
function sourceReadChunks(file,source){const text=source.startsWith('\uFEFF')?source.slice(1):source,lines=Math.max(1,text.split('\n').length-Number(text.endsWith('\n'))),requests=[];for(let start=1;start<=lines;start+=1000)requests.push({file_path:file,start_line:start,end_line:Math.min(lines,start+999)});return requests;}
function sourceFor(profile=PROFILE,{citation=false,numbering=true}={}){
  const header='\uFEFF---\r\ncatalog_id: \'00123\' # KEEP_NEIGHBOR\r\nreadmd:\r\n  ownedNeighbor: \'007\' # KEEP_READMD_NEIGHBOR\r\n  publication: '+JSON.stringify(profile)+'\r\n'+(citation?'bibliography: refs.json\r\ncsl: note.csl\r\n':'')+'---\r\n';
  const config=numbering?'<!-- readmd-numbering:v1 '+JSON.stringify(NUMBERING)+' -->\r\n\r\n':'';
  return header+config+'# SETTINGS_ALPHA {#settings-alpha}\r\n\r\nFirst[^manual-f], end[^manual-e], repeated[^manual-f].'+(citation?' Generated [@work, p. 12].':'')+'\r\n\r\n## SETTINGS_METHOD {#settings-method}\r\n\r\n| Measure | Value |\r\n|---|---:|\r\n| SETTINGS_CELL | 42 |\r\n\r\n:label[tbl:settings] SETTINGS_TABLE\r\n\r\nSee :ref[tbl:settings].\r\n\r\n# SETTINGS_BETA {#settings-beta}\r\n\r\nAgain end[^manual-e]. Unicode 𝛂 remains authored.\r\n\r\n## SETTINGS_RESULTS {#settings-results}\r\n\r\n[^manual-f]: SOURCE_FOOT_BODY **strong** and [evidence](https://example.test/note).\r\n\r\n[^manual-e]: SOURCE_END_BODY End evidence.\r\n'+(citation?'\r\n<!--readmd:bibliography-->\r\n':'');
}
function noteStyle(){return '<?xml version="1.0" encoding="utf-8"?><style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="note"><info><title>Owned settings QA</title><id>urn:readmd:qa:docx-settings</id></info><citation><layout><text variable="title"/><text variable="locator" prefix=" p. "/></layout></citation><bibliography><layout><text variable="title"/></layout></bibliography></style>';}
function applyEdits(source,edits){
  let result='',cursor=0;for(const edit of [...edits].sort((a,b)=>a.from-b.from||a.to-b.to)){
    assert(Number.isInteger(edit.from)&&Number.isInteger(edit.to)&&edit.from>=cursor&&edit.from<=edit.to&&edit.to<=source.length,'Invalid UTF-16 edit');assert.equal(typeof edit.insert,'string');
    for(const offset of [edit.from,edit.to]){assert(!(offset&&offset<source.length&&/[\uD800-\uDBFF]/.test(source[offset-1])&&/[\uDC00-\uDFFF]/.test(source[offset])),'Split surrogate');assert(!(offset&&source[offset-1]==='\r'&&source[offset]==='\n'),'Split CRLF');}
    result+=source.slice(cursor,edit.from)+edit.insert;cursor=edit.to;
  }return result+source.slice(cursor);
}
function assertProfileTransaction(value,source,{changed=true}={}){
  assert.equal(value.ok,true);assert.equal(value.transaction.base_revision,hash(source));assert.equal(value.transaction.origin,'editor');assert.equal(value.changed,changed);
  assert.equal(value.content,applyEdits(source,value.transaction.edits));assert.equal(value.revision,hash(value.content));assert.equal(applyEdits(value.content,value.inverse),source,'Profile inverse is not exact');
  if(!changed)assert.equal(value.transaction.edits.length,0);return value.content;
}
function assertSavedEdit(value,before,after){
  assert.equal(value.ok,true);assert.equal(value.dry_run,false);assert.equal(value.changed,true);assert.equal(value.replacements,1);assert.equal(value.revision,hash(after));assert.equal(value.proposed_revision,hash(after));assert.notEqual(value.revision,hash(before));assert(value.checkpoint_id,'Actual save must retain recoverable history');
}
function assertNativeGlobalSize(value,points){
  assert.equal(value.kind,'docx-settings-roundtrip');assert.equal(value.sourcePayloadUsedAsNativeEvidence,false);
  assert(Number.isFinite(points)&&points>=8&&points<=20&&Number.isInteger(points*2));assertSourceNumbering(value);
  const body=value.paragraphs.filter(row=>row.text.startsWith('First'));
  const caption=value.paragraphs.filter(row=>row.text==='Table III.4 — SETTINGS_TABLE'&&row.fields.some(field=>/^\s*SEQ\s+ReadMDTable\b/u.test(field.instruction)));
  assert.equal(caption.length,1,'Actual global-size caption missing or duplicated');assert.equal(caption[0].bookmarks.length,1,'Actual global-size caption bookmark missing or duplicated');
  const target=caption[0].bookmarks[0].name;assert.equal(typeof target,'string');assert(/^[A-Za-z_][A-Za-z0-9_]*$/.test(target),'Actual caption bookmark is invalid');
  assert.equal(value.paragraphs.flatMap(row=>row.bookmarks||[]).filter(row=>row.name===target).length,1,'Actual global-size caption identity is duplicated');
  const reference=value.paragraphs.filter(row=>row.text==='See Table III.4.'&&row.fields.some(field=>{const match=/^\s*REF\s+(?:"([^"]+)"|([^\s]+))(?:\s|$)/u.exec(field.instruction);return match&&(match[1]||match[2])===target&&field.cached==='Table III.4';}));
  for(const [label,rows,west,complex]of [['body',body,points,points],['reference',reference,points,points],['caption',caption,9,points]]){
    assert.equal(rows.length,1,'Actual global-size '+label+' missing or duplicated');
    const runs=rows[0].runs.filter(run=>run.text.trim());assert(runs.length,'Actual native font cascade absent for '+label);
    for(const run of runs){assert.equal(run.properties.sz,String(west*2),'Actual '+label+' western size');assert.equal(run.properties.szCs,String(complex*2),'Actual '+label+' complex size');}
  }
  return {points,independentNativeAxes:true,automaticReference:true,captionWesternPt:9,sourcePayloadUsed:false};
}
function kindProperties(value,kind){
  const story=value.notes[kind],properties={...(story.settings?.values||{})};
  // Word uses section-scoped footnote placement/overrides.  Endnote settings
  // remain document-owned even when a section contains ignored properties.
  if(kind==='footnote')for(const section of story.sections)if(section)Object.assign(properties,section.values);
  return properties;
}
function normalNote(value,kind,marker){const rows=value.notes[kind].definitions.filter(row=>row.type==='normal'&&row.text.includes(marker));assert.equal(rows.length,1,'Missing or duplicated '+kind+' '+marker);return rows[0];}
function assertNote(value,{kind,marker,references,start,format,size,sizeCs,line,rule,longNote,placement}){
  const story=value.notes[kind],note=normalNote(value,kind,marker),properties=kindProperties(value,kind);
  assert.equal(story.references.filter(id=>id===note.id).length,references,'Repeated reference/story identity changed: '+marker);
  if(start!==undefined)assert.equal(properties.numStart,String(start),'Native note start changed');
  if(format!==undefined)assert.equal(properties.numFmt,format,'Native note format changed');
  if(placement!==undefined)assert.equal(properties.pos,placement,'Native note placement changed');
  const actual=note.paragraphs.find(row=>row.text.includes(marker));assert(actual);
  if(line!==undefined)assert.equal(actual.properties.spacing?.line,String(line),'Native note line unit/value changed');
  if(rule!==undefined)assert.equal(actual.properties.spacing?.lineRule,rule,'Native note line rule changed');
  if(size!==undefined){const runs=actual.runs.filter(row=>row.text.includes(marker));assert.equal(runs.length,1);assert.equal(runs[0].properties.sz,String(size*2),'Effective note font size changed');}
  if(sizeCs!==undefined){const runs=actual.runs.filter(row=>row.text.includes(marker));assert.equal(runs.length,1);assert.equal(runs[0].properties.szCs,String(sizeCs*2),'Effective complex-script note size changed');}
  if(longNote!==undefined)assert.equal(actual.properties.keepLines??false,longNote==='keepParagraphs','keepNext must not stand in for keepLines');
  return note;
}
function assertSourceNative(value,{mutated=false,format,overrideStart,overrideSize,footStart,footSize,footSizeCs}={}){
  assert.equal(value.kind,'docx-settings-roundtrip');assert.equal(value.sourcePayloadUsedAsNativeEvidence,false);assert.equal(value.wordApplicationRoundtripCertified,false);
  assertNote(value,{kind:'footnote',marker:'SOURCE_FOOT_BODY',references:2,start:footStart??overrideStart??(mutated?13:7),format:format??(mutated?'lowerLetter':'upperRoman'),size:footSize??overrideSize??10,sizeCs:footSizeCs,line:210,rule:'exact',longNote:'split',placement:'beneathText'});
  assertNote(value,{kind:'endnote',marker:'SOURCE_END_BODY',references:2,start:overrideStart??(mutated?17:3),format:mutated?'upperRoman':'lowerLetter',size:overrideSize??11,line:260,rule:'atLeast',longNote:'keepParagraphs',placement:'docEnd'});
  assertSourceNumbering(value,{mutated});
  return {mixedNativeStories:true,nativeHeadingLevels:2,repeatedAuthorReferences:4,liveFields:true,mutated};
}
function assertUniformNormalFootnoteRuns(value){
  assert.equal(value.sourcePayloadUsedAsNativeEvidence,false);const notes=value.notes.footnote.definitions.filter(note=>note.type==='normal');assert(notes.length>=2,'Uniform positive must retain manual and current generated-note content');let count=0;
  for(const note of notes)for(const paragraph of note.paragraphs)for(const run of paragraph.runs){
    if(!run.text)continue;count++;assert.equal(run.properties.sz,'60','A normal footnote text run is not actually 30pt: '+JSON.stringify(run.text));assert.equal(run.properties.szCs,'60','A normal footnote complex-script text run is not actually 30pt');
  }
  assert(count>=5,'Uniform native fixture lost actual text runs');return {normalNotesChecked:notes.length,textRunsChecked:count,halfPoints:60};
}
function assertStyleOnlyMixedNoteFixture(value){
  assertSourceNative(value,{format:'chicago',footStart:12000,footSize:10,footSizeCs:10});
  const foot=value.notes.footnote,end=value.notes.endnote,normalFoot=foot.definitions.filter(note=>note.type==='normal'),normalEnd=end.definitions.filter(note=>note.type==='normal');
  assert.equal(normalFoot.length,2,'Style-only return lost or duplicated a current normal footnote');assert.equal(normalEnd.length,1,'Style-only return lost or duplicated the endnote');
  const manual=normalNote(value,'footnote','SOURCE_FOOT_BODY'),generated=normalNote(value,'footnote','SOURCE_GENERATED_CITATION'),endnote=normalNote(value,'endnote','SOURCE_END_BODY');
  assert.notEqual(manual.id,generated.id,'Manual and generated-note content merged into one native identity');assert.deepEqual(foot.references,[manual.id,manual.id,generated.id],'Footnote reference order/repeats or generated-note identity changed');assert.deepEqual(end.references,[endnote.id,endnote.id],'Endnote repeated reference identity changed');
  assert.equal(value.styleDefinitions?.FootnoteText?.r.sz,'60','Current native FootnoteText western template was lost');assert.equal(value.styleDefinitions?.FootnoteText?.r.szCs,'60','Current native FootnoteText CS template was lost');
  let checked=0;
  for(const [note,body]of [[manual,'SOURCE_FOOT_BODY strong and evidence.'],[generated,'SOURCE_GENERATED_CITATION p. 12']]){
    assert.equal(note.text,' '+body,'Current note body/locator or structural marker gap changed');assert.equal(note.paragraphs.length,1,'Current ordinary note paragraph split or disappeared');const paragraph=note.paragraphs[0];assert.equal(paragraph.text,note.text);assert.equal(paragraph.fields.length,0,'Ordinary note text became a live field');
    let offset=0,gap=0,authors=0;for(const run of paragraph.runs){if(!run.text)continue;const end=offset+run.text.length;if(end<=1){assert.equal(offset,0);assert.equal(run.text,' ');gap++;assert.equal(run.properties.sz,'60','Actual marker gap lost its inherited30pt western size');assert.equal(run.properties.szCs,'60','Actual marker gap lost its inherited30pt CS size');}else{assert(offset>=1,'Different-size marker gap was merged with authored text');authors++;assert.equal(run.properties.sz,'20','Current note author western size changed from10pt: '+JSON.stringify(run.text));assert.equal(run.properties.szCs,'20','Current note author CS size changed from10pt: '+JSON.stringify(run.text));}offset=end;checked++;}
    assert.equal(offset,note.text.length,'Actual native text run coverage differs from current note text');assert.equal(gap,1);assert(authors>0,'Current note author text runs disappeared');assert.equal(paragraph.runs.map(run=>run.text).join(''),note.text);
    assert.equal(paragraph.properties.spacing.line,'210');assert.equal(paragraph.properties.spacing.lineRule,'exact');assert.equal(paragraph.properties.keepLines??false,false);
  }
  const strong=manual.paragraphs[0].runs.filter(run=>run.text==='strong');assert.equal(strong.length,1,'Current rich note emphasis lost its exact text');assert.equal(strong[0].properties.b,true,'Current rich note strong formatting disappeared');
  assert.equal(endnote.text,' SOURCE_END_BODY End evidence.');for(const paragraph of endnote.paragraphs)for(const run of paragraph.runs){if(!run.text)continue;assert.equal(run.properties.sz,'22','Endnote western size was polluted by footnote template');assert.equal(run.properties.szCs,'22','Endnote CS size was polluted by footnote template');}
  for(const text of ['First, end, repeated. Generated .','Again end. Unicode 𝛂 remains authored.','SOURCE_GENERATED_CITATION'])assert.equal(value.paragraphs.filter(paragraph=>paragraph.text===text).length,1,'Current complete body or bibliography text was lost/duplicated: '+text);
  const selected=foot.selectedSpecial;assert.equal(selected.length,3,'Selected native special stories changed');assert.equal(selected.filter(note=>note.type==='separator'&&note.separatorElements.length===0).length,1);assert.equal(selected.filter(note=>note.type==='continuationSeparator'&&JSON.stringify(note.separatorElements)===JSON.stringify(['continuationSeparator'])).length,1);assert.equal(selected.filter(note=>note.type==='continuationNotice'&&note.text==='SOURCE_CONTINUED <next> & evidence').length,1,'Current continuation notice changed');
  return {currentTemplateHalfPoints:[60,60],currentAuthorHalfPoints:[20,20],normalFootnotes:2,normalEndnotes:1,referenceCounts:[2,1,2],actualTextRunsChecked:checked,sourcePayloadUsed:false};
}
function assertStyleOnlyMixedNoteImport(markdown,current){
  const identities=assertProfileMatchesNotes(current,markdown);assertSourceCaptionIdentity(markdown);noteLabel(markdown,'SOURCE_FOOT_BODY','SOURCE_FOOT_BODY strong and evidence.');noteLabel(markdown,'SOURCE_END_BODY','SOURCE_END_BODY End evidence.');
  const generated=noteLabel(markdown,'SOURCE_GENERATED_CITATION','SOURCE_GENERATED_CITATION p. 12');assert.notEqual(generated,identities.footnoteLabel);assert.notEqual(generated,identities.endnoteLabel);assert.equal(current.sourceValues.footnotes.kindByLabel[generated],'footnote','Current generated-note content lost its explicit native story binding');
  const foot=current.sourceValues.footnotes.kinds.footnote,end=current.sourceValues.footnotes.kinds.endnote;assert.equal(foot.size,30);assert.equal(foot.sizeCs,30);assert.equal(foot.start,12000);assert.equal(foot.numberFormat,'chicago');assert.equal(end.size,11);assert.equal(end.sizeCs,11);
  const view=markdownStructure(markdown);assert.equal(view.paragraphs.filter(paragraph=>paragraph.text==='SOURCE_GENERATED_CITATION').length,1,'The current bibliography must survive as one actual editable entry');
  for(const part of ['word/document.xml','word/footnotes.xml'])assert(view.citationDiagnostics.some(row=>row.code==='docx_citation_conflict'&&row.reason==='generated_citation_note_author_format_not_reproducible'&&row.part===part&&row.original_source==='[@work, p. 12]'&&row.current_source==='[@work, p. 12]'&&typeof row.binding==='string'&&row.binding.length>0&&row.visible_content_preserved===true),'Current note author-format/source diagnostic missing: '+part);
  assert(view.citationDiagnostics.some(row=>row.code==='docx_citation_conflict'&&row.reason==='citation_family_author_format_not_reproducible'&&row.part==='word/document.xml'&&typeof row.binding==='string'&&row.binding.length>0&&row.visible_content_preserved===true),'The dependent current bibliography was not explicitly retained');assert(!view.citationDiagnostics.some(row=>row.reason==='citation_render_modified'),'Unchanged SDT XML was falsely diagnosed as modified');
  // This source phase does not certify a regeneration. Both author axes,
  // marker gaps, current bibliography and all repeats are independently
  // checked from the second real DOCX after a normal EOF and a different PID.
  return {sourceTemplatePt:[30,30],currentManualBody:true,endnoteBody:true,currentGeneratedNoteLabel:generated,currentBibliographyLiteral:true,generatedNoteReturnPendingIndependentNativeInspection:true};
}
function assertSourceNumbering(value,{mutated=false}={}){
  const alpha=value.paragraphs.find(row=>row.text==='SETTINGS_ALPHA');assert(alpha?.rule,'Missing native source heading numbering');assert.equal(alpha.rule.format,mutated?'lowerRoman':'upperRoman');assert.equal(alpha.rule.start,mutated?'5':'3');assert.equal(alpha.rule.text,mutated?'Part %1.':'%1');assert.equal(alpha.rule.suffix,mutated?'tab':'space');
  const method=value.paragraphs.find(row=>row.text==='SETTINGS_METHOD');assert(method?.rule);assert.equal(method.rule.format,'lowerLetter');assert.equal(method.rule.start,'2');assert.equal(method.rule.text,'%1.%2');
  const chapter=mutated?'Part v.':'III',label='Table '+chapter+'.4',caption=value.paragraphs.find(row=>row.text===label+' — SETTINGS_TABLE');assert(caption,'Current source caption text/cache was lost, stale or duplicated');
  const chapterField=caption.fields.find(field=>/^\s*STYLEREF\s+1\s+\\s\s*$/.test(field.instruction));assert(chapterField,'Caption lost its native chapter field');assert.equal(chapterField.cached,chapter,'Caption chapter cache does not match the current native heading rule');
  const sequence=caption.fields.find(field=>/^\s*SEQ\s+ReadMDTable\b/.test(field.instruction));assert(sequence,'Table lost native SEQ identity');assert(/\\s\s+1\b/.test(sequence.instruction),'Caption SEQ lost chapter scope');
  assert(caption.fields.some(field=>/^\s*=\s*\+\s*3\b/.test(field.instruction)&&field.cached==='4')||/\\r\s+4\b/.test(sequence.instruction)&&sequence.cached==='4','Caption start 4 became a literal or stale field cache');
  const bookmarks=new Set(caption.bookmarks.map(row=>row.name)),references=value.paragraphs.flatMap(row=>row.fields).filter(field=>/^\s*REF\s+/.test(field.instruction));
  const current=references.filter(field=>{const target=/^\s*REF\s+(?:"([^"]+)"|([^\s]+))/.exec(field.instruction);return target&&bookmarks.has(target[1]||target[2])&&field.cached===label;});assert.equal(current.length,1,'Current source REF lost its exact caption target/cache');
}
function assertNumberingCleared(value){
  assert.equal(value.sourcePayloadUsedAsNativeEvidence,false);assert.equal(value.wordApplicationRoundtripCertified,false);
  for(const [marker,level]of [['SETTINGS_ALPHA',1],['SETTINGS_METHOD',2],['SETTINGS_BETA',1],['SETTINGS_RESULTS',2]]){
    const rows=value.paragraphs.filter(row=>row.text===marker);assert.equal(rows.length,1,'Cleared heading content disappeared or duplicated: '+marker);
    assert.equal(rows[0].style,'Heading'+level,'Clearing numbering changed the actual native heading role');
    assert(!rows[0].rule,'Auxiliary source resurrected heading numbering');assert(!rows[0].number||rows[0].number.numId==='0','A cleared heading still consumes native numbering');
  }
  assert(value.paragraphs.every(row=>row.fields.every(field=>!/^\s*(?:SEQ\b|REF\b|STYLEREF\b|=)/i.test(field.instruction))),'Cleared numbering fields were resurrected');
  for(const text of ['Table III.4 — SETTINGS_TABLE','See Table III.4.'])assert.equal(value.paragraphs.filter(row=>row.text===text).length,1,'Clearing fields changed their current outer visible result');
  assertNote(value,{kind:'footnote',marker:'SOURCE_FOOT_BODY',references:2,start:7,format:'upperRoman',size:10,line:210,rule:'exact',longNote:'split',placement:'beneathText'});
  assertNote(value,{kind:'endnote',marker:'SOURCE_END_BODY',references:2,start:3,format:'lowerLetter',size:11,line:260,rule:'atLeast',longNote:'keepParagraphs',placement:'docEnd'});
  return {nativeNumberingCleared:true,currentCaptionAndReferenceVisible:true,mixedNotesPreserved:true};
}
function assertNumberingClearedImport(markdown){
  const view=markdownStructure(markdown);assert.equal(view.numberingDeclarations.length,0,'Cleared native numbering resurrected its source declaration');
  assert(!view.paragraphs.some(row=>/:label\[tbl:settings\]|:ref\[tbl:settings\]/.test(row.text)),'Cleared native fields became editable automatic source again');
  for(const text of ['Table III.4 — SETTINGS_TABLE','See Table III.4.'])assert.equal(view.paragraphs.filter(row=>row.text===text).length,1,'Cleared field results lost their current visible content');
  for(const [level,name,id]of [[1,'SETTINGS_ALPHA','settings-alpha'],[2,'SETTINGS_METHOD','settings-method'],[1,'SETTINGS_BETA','settings-beta'],[2,'SETTINGS_RESULTS','settings-results']]){
    const headings=view.headings.filter(row=>row.text===name);assert.equal(headings.length,1,'Cleared heading disappeared or duplicated: '+name);assert.equal(headings[0].level,level);assert.equal(headings[0].id,id,'Cleared heading lost its current source identity');
  }
}
function assertNotesCleared(value,{input=false}={}){
  assert.equal(value.sourcePayloadUsedAsNativeEvidence,false);assert.equal(value.wordApplicationRoundtripCertified,false);assertSourceNumbering(value);
  for(const [kind,marker,format,size,line,rule,longNote,placement]of [['footnote','SOURCE_FOOT_BODY','decimal',10,210,'exact','split','pageBottom'],['endnote','SOURCE_END_BODY','lowerRoman',11,260,'atLeast','keepParagraphs','docEnd']]){
    const story=value.notes[kind],properties=kindProperties(value,kind);
    if(input){assert.equal(story.settings,null,'Native global note settings were not deleted');assert(story.sections.every(section=>section===null),'Native section note settings were not deleted');}
    assert.equal(properties.numFmt??format,format,'Old source note format overrode the cleared native default');assert.equal(properties.numStart??'1','1','Old source note start was resurrected');assert.equal(properties.numRestart??'continuous','continuous');assert.equal(properties.pos??placement,placement,'Old source note placement was resurrected');
    assertNote(value,{kind,marker,references:2,size,line,rule,longNote});
    assert(story.selectedSpecial.every(row=>row.type==='continuationNotice'?!(row.text||'').trim():!(row.separatorElements||[]).length),'An unselected old special note was resurrected');
  }
  return {nativeNoteSettingsCleared:true,wordDefaultFormats:{footnote:'decimal',endnote:'lowerRoman'},mixedNotesPreserved:true};
}
function assertNotesClearedProfile(profile){
  for(const [kind,format,placement]of [['footnote','decimal','pageBottom'],['endnote','lowerRoman','docEnd']]){
    const policy=profile.sourceValues.footnotes.kinds[kind];assert.equal(policy.numberFormat,format);assert.equal(policy.start,1);assert.equal(policy.restart,'continuous');assert.equal(policy.placement,placement);
    assert.equal(policy.separator,'none');assert.equal(policy.continuationSeparator,'none');assert.equal(policy.continuationNotice,'');
  }
}
function assertNativeFixture(value,{kind='native-mixed',mutated=false}={}){
  const caption=kind.startsWith('native-caption-'),base=caption?kind.replace('native-caption-','native-'):kind,notes=base!=='native-numbering',numbering=base!=='native-note-mixed';
  if(notes){
    const foot=assertNote(value,{kind:'footnote',marker:'NATIVE_FOOT_BODY',references:2,start:7,format:'upperRoman',size:10,line:210,rule:'exact',longNote:'split',placement:'beneathText'});
    const end=assertNote(value,{kind:'endnote',marker:'NATIVE_END_BODY',references:1,start:3,format:'lowerLetter',size:11,line:260,rule:'atLeast',longNote:'keepParagraphs',placement:'docEnd'});
    assert.notEqual(value.notes.footnote.part,value.notes.endnote.part);assert(foot.id&&end.id);
    const special=value.notes.footnote.definitions.filter(row=>row.type!=='normal');assert(special.some(row=>row.type==='continuationNotice'&&row.text==='NATIVE_CONTINUED <next> & evidence'));
    const selected=value.notes.footnote.selectedSpecial;assert(selected.some(row=>row.type==='separator'&&row.separatorElements.length===0));assert(selected.some(row=>row.type==='continuationSeparator'&&row.separatorElements.includes('continuationSeparator')));
  }
  if(numbering){
    const alpha=value.paragraphs.find(row=>row.text==='NATIVE_ALPHA'),method=value.paragraphs.find(row=>row.text==='NATIVE_METHOD');assert(alpha?.rule&&method?.rule,'Native style-inherited heading numbering was lost');
    assert.equal(alpha.rule.format,'upperRoman');assert.equal(alpha.rule.start,base==='native-zero-start'?'0':'3');assert.equal(alpha.rule.text,'Article %1:');assert.equal(alpha.rule.suffix,'tab');
    assert.equal(method.rule.format,'lowerLetter');assert.equal(method.rule.start,'2');assert.equal(method.rule.text,'%1(%2)');assert.equal(method.rule.suffix,'space');assert.equal(method.rule.restart,'1');
    const ordinary=value.paragraphs.find(row=>row.text==='ORDINARY_LIST_ONLY');assert(ordinary?.rule,'Ordinary list was lost');assert(!/^Heading/.test(ordinary.style||''),'Ordinary list became an outline heading');
  }
  const fields=value.paragraphs.flatMap(row=>row.fields);assert(fields.some(field=>/^\s*REF\s+/.test(field.instruction)),'Native live cross-reference disappeared');
  if(caption){
    const para=value.paragraphs.find(row=>row.text==='Table 4 — NATIVE_CAPTION');assert(para,'Current native caption text was lost or duplicated');
    const sequence=para.fields.find(field=>/^\s*SEQ\s+Table\b/.test(field.instruction));assert(sequence,'Native-only sequence identity was not preserved');
    assert(/\\r\s+4\b/.test(sequence.instruction)||para.fields.some(field=>/^\s*=\s*\+\s*3\b/.test(field.instruction)),'Non-one caption start became a literal cache');
    const bookmarks=new Set(para.bookmarks.map(row=>row.name)),refs=fields.filter(field=>/^\s*REF\s+/.test(field.instruction));assert(refs.some(field=>{const target=/^\s*REF\s+(?:"([^"]+)"|([^\s]+))/.exec(field.instruction);return target&&bookmarks.has(target[1]||target[2])&&field.cached==='Table 4';}),'REF no longer points to the actual automatic caption bookmark');
  }
  return {nativeOnlyInput:true,mixedStories:notes,perLevelNumbering:numbering,startZero:base==='native-zero-start',nativeCaption:caption,cachedTextUsedAsRule:false};
}
function assertNativeMultiSection(value){
  assert.equal(value.kind,'docx-settings-roundtrip');assert.equal(value.sourcePayloadUsedAsNativeEvidence,false);assert.equal(value.wordApplicationRoundtripCertified,false);assert.equal(value.nativeSections,2);
  const foot=value.notes.footnote,end=value.notes.endnote;assert.equal(foot.sections.length,2);assert.equal(end.sections.length,2);assert.deepEqual(foot.sections.map(row=>row?.values?.pos),['beneathText','pageBottom'],'Current native section positions changed or merged');assert(end.sections.every(row=>row===null),'Document-owned endnotes must not acquire a per-section override');
  const first=assertNote(value,{kind:'footnote',marker:'NATIVE_FOOT_BODY',references:2,start:7,format:'upperRoman',size:10,line:210,rule:'exact',longNote:'split'}),second=assertNote(value,{kind:'footnote',marker:'NATIVE_SECOND_SECTION_NOTE',references:1,start:7,format:'upperRoman',size:10,line:210,rule:'exact',longNote:'split'});
  const last=assertNote(value,{kind:'endnote',marker:'NATIVE_END_BODY',references:1,start:3,format:'lowerLetter',size:11,line:260,rule:'atLeast',longNote:'keepParagraphs',placement:'docEnd'});
  assert.notEqual(first.id,second.id,'Distinct section notes were merged');assert.notEqual(foot.part,end.part);assert.deepEqual(foot.references,[first.id,first.id,second.id],'Repeated native note identity/order changed');assert.deepEqual(end.references,[last.id]);assert.equal(foot.definitions.filter(row=>row.type==='normal').length,2);assert.equal(end.definitions.filter(row=>row.type==='normal').length,1);
  assert.equal(first.text.trim(),'NATIVE_FOOT_BODY Evidence 😀.');assert.equal(second.text.trim(),'NATIVE_SECOND_SECTION_NOTE Distinct placement.');assert.equal(last.text.trim(),'NATIVE_END_BODY Evidence 😀.');assert.equal(foot.settings.values.numRestart,'eachSect');
  const boundaries=value.paragraphs.map((row,index)=>Object.hasOwn(row.properties||{},'sectPr')?index:-1).filter(index=>index>=0);assert.equal(boundaries.length,1,'Two native sections require one actual paragraph terminator');const boundary=boundaries[0];
  for(const marker of ['NATIVE_ALPHA','NATIVE_FIRST','NATIVE_SECOND','NATIVE_REPEAT','NATIVE_METHOD','NATIVE_FIELD']){
    const rows=value.paragraphs.map((row,index)=>({row,index})).filter(({row})=>new RegExp('\\b'+marker+'\\b').test(row.text));assert(rows.length>0,'Missing native body marker '+marker);assert(rows.every(({index})=>index<boundary),'First-section body crossed its actual native terminator: '+marker);
  }
  const later=value.paragraphs.map((row,index)=>({row,index})).filter(({row})=>/\bNATIVE_SECOND_SECTION\b/.test(row.text));assert.equal(later.length,1);assert(later[0].index>boundary,'Second-section body moved before its native boundary');
  assert(foot.selectedSpecial.some(row=>row.type==='continuationNotice'&&row.text==='NATIVE_CONTINUED <next> & evidence'),'Native selected special story was lost');
  return {nativeSections:2,positions:['beneathText','pageBottom'],normalFootnotes:2,repeatedFootnoteReferences:3,normalEndnotes:1,paragraphBoundaryVerified:true};
}
function assertLegacyMultiSectionReferenceRejection(reply){
  const failure=assertConversionError(reply,'author_inline_text_unsupported');
  assert.equal(failure.native.part,'word/document.xml');assert.equal(failure.native.path,'[3, 1, 0]');assert.equal(failure.native.detail,'author-sized live field result has no editable field-run size model');return failure;
}
function assertSafeMultiSectionFixture(legacy,current){
  assertNativeMultiSection(legacy);assertNativeMultiSection(current);
  assert.equal(legacy.documentPart,'word/document.xml');assert.equal(current.documentPart,legacy.documentPart);
  assert.deepEqual(Object.keys(current.parts).sort(),Object.keys(legacy.parts).sort());
  assert.deepEqual(Object.keys(legacy.parts).filter(name=>legacy.parts[name]!==current.parts[name]),['word/document.xml'],'Safe REF variant changed native styles, notes, relationships or other parts');
  const before=structuredClone(legacy),after=structuredClone(current),oldHeading=before.paragraphs.filter(row=>row.text==='NATIVE_ALPHA'),newHeading=after.paragraphs.filter(row=>row.text==='NATIVE_ALPHA');
  assert.equal(oldHeading.length,1);assert.equal(newHeading.length,1);assert.deepEqual(oldHeading[0].bookmarks,[{id:'11',name:'native-alpha'}]);assert.deepEqual(newHeading[0].bookmarks,[{id:'11',name:'native_alpha'}]);
  const oldFields=before.paragraphs.flatMap(row=>row.fields||[]),newFields=after.paragraphs.flatMap(row=>row.fields||[]);assert.equal(oldFields.length,1);assert.equal(newFields.length,1);
  assert.deepEqual(oldFields[0],{instruction:' REF native-alpha \\h ',cached:'NATIVE_ALPHA',kind:'simple',depth:0});assert.deepEqual(newFields[0],{instruction:' REF native_alpha \\h ',cached:'NATIVE_ALPHA',kind:'simple',depth:0});
  assert.equal(before.sectionScopes.length,2);assert.equal(after.sectionScopes.length,2);
  assert.deepEqual(before.sectionScopes.map(scope=>scope.bookmarks),[[{id:'11',name:'native-alpha'}],[]]);
  assert.deepEqual(after.sectionScopes.map(scope=>scope.bookmarks),[[{id:'11',name:'native_alpha'}],[]]);
  oldHeading[0].bookmarks[0].name='native_alpha';oldFields[0].instruction=' REF native_alpha \\h ';
  before.sectionScopes[0].bookmarks[0].name='native_alpha';
  before.parts['word/document.xml']=after.parts['word/document.xml'];before.bytes=after.bytes;before.sha256=after.sha256;
  assert.deepEqual(before,after,'Safe REF variant changed current body, caches, effective font axes or native note placement');
  return {onlyBookmarkAndReferenceNameChanged:true,liveRefRetained:true,cache:'NATIVE_ALPHA',knownWesternHalfPoints:22,unknownComplexAxisNotInvented:true};
}
function assertSafeMultiSectionImport(markdown,current){
  const sections=assertMultiSectionImport(markdown,current),diagnostics=assertHeadingReferenceOpen(markdown);
  return {...sections,currentHeadingLinkPreserved:true,headingReferenceDiagnostics:diagnostics};
}

function assertMultiSectionImport(markdown,current){
  const scopes=[];for(const token of markdownTokens(markdown)){
    const raw=token.raw||'';if(token.type==='html'&&/^\s*<!--\s*readmd-section:/.test(raw)){
      const match=/^\s*<!--\s*readmd-section:v1\s+(\{[\s\S]*\})\s*-->\s*$/.exec(raw);assert(match,'Malformed top-level source section declaration');const section=JSON.parse(match[1]);assert(typeof section.id==='string'&&/^[A-Za-z][A-Za-z0-9_-]{0,63}$/.test(section.id));scopes.push({section,tokens:[]});
    }else if(scopes.length)scopes.at(-1).tokens.push(token);
  }
  assert.equal(scopes.length,2,'Native position differences require two actual top-level source scopes');assert.notEqual(scopes[0].section.id,scopes[1].section.id);assert.deepEqual(scopes.map(({section})=>section.footnotes?.kinds?.footnote?.placement),['beneathText','pageBottom']);assert.equal(scopes[0].section.break??'nextPage','continuous');assert.equal(scopes[1].section.break??'nextPage','nextPage');
  assert.equal(current.ok,true);const map=current.sourceValues.footnotes?.kindByLabel;assert(map&&typeof map==='object');const first=noteLabel(markdown,'NATIVE_FOOT_BODY'),second=noteLabel(markdown,'NATIVE_SECOND_SECTION_NOTE'),end=noteLabel(markdown,'NATIVE_END_BODY');assert.equal(new Set([first,second,end]).size,3);assert.deepEqual(map,{[first]:'footnote',[second]:'footnote',[end]:'endnote'});
  const references=scopes.map(({tokens})=>tokens.filter(token=>token.type==='paragraph'&&!/^\s*\[\^[^\]\r\n]+\]:/.test(token.raw||'')).flatMap(token=>[...(token.raw||'').matchAll(/(?<!\\)\[\^([^\]\r\n]+)\](?!:)/g)].map(match=>match[1])));assert.deepEqual(references,[[first,end,first],[second]],'Current source note callouts changed identity or native section scope');
  const body=scopes.map(({tokens})=>tokens.filter(token=>token.type==='paragraph'||token.type==='heading').map(token=>inlineStructure(token.tokens)).filter(row=>row.supported).map(row=>row.text).join('\n'));
  for(const marker of ['NATIVE_FIRST','NATIVE_SECOND','NATIVE_REPEAT','NATIVE_METHOD','NATIVE_FIELD'])assert(new RegExp('\\b'+marker+'\\b').test(body[0]),'First native source scope lost body marker '+marker);assert(!/\bNATIVE_SECOND_SECTION\b/.test(body[0]));assert(/\bNATIVE_SECOND_SECTION\b/.test(body[1]));assert(!/\bNATIVE_FIRST\b/.test(body[1]));
  return {sectionIds:scopes.map(({section})=>section.id),positions:['beneathText','pageBottom'],footnoteLabels:[first,second],endnoteLabel:end};
}
function noteLabel(markdown,marker,expectedText){
  const lines=markdown.replace(/\r\n?/g,'\n').split('\n'),rows=[];let fence=null,from=0;
  if(lines[0]?.replace(/^\uFEFF/,'').trim()==='---'){const end=lines.findIndex((line,index)=>index>0&&/^(?:---|\.\.\.)[ \t]*$/.test(line));if(end>=0)from=end+1;}
  for(let at=from;at<lines.length;at++){
    const line=lines[at],delimiter=/^ {0,3}(`{3,}|~{3,})(.*)$/.exec(line);
    if(fence){if(delimiter&&delimiter[1][0]===fence.char&&delimiter[1].length>=fence.length&&!delimiter[2].trim())fence=null;continue;}
    if(delimiter){fence={char:delimiter[1][0],length:delimiter[1].length};continue;}
    const definition=/^ {0,3}\[\^([^\]]+)\]:[ \t]*(.*)$/.exec(line);if(!definition)continue;
    const body=[definition[2]];while(at+1<lines.length&&(!lines[at+1].trim()||/^(?: {4}|\t)/.test(lines[at+1])))body.push(lines[++at].replace(/^(?: {4}|\t)/,''));
    const paragraphs=markdownStructure(body.join('\n')).paragraphs;
    if(paragraphs.some(row=>row.text.includes(marker))){if(expectedText!==undefined)assert.equal(paragraphs.filter(row=>row.text.trim()===expectedText).length,1,'Current note text or locator changed');rows.push(definition[1]);}
  }
  assert.equal(rows.length,1,'Missing/duplicate editable note definition '+marker);return rows[0];
}
function assertProfileMatchesNotes(profile,markdown,markers=['SOURCE_FOOT_BODY','SOURCE_END_BODY']){
  assert.equal(profile.ok,true);assert.equal(profile.sourceValues.version,1);const map=profile.sourceValues.footnotes?.kindByLabel;assert(map&&typeof map==='object');
  const foot=noteLabel(markdown,markers[0]),end=noteLabel(markdown,markers[1]);assert.notEqual(foot,end,'Two stories shared a Markdown identity');assert.equal(map[foot],'footnote');assert.equal(map[end],'endnote');return {footnoteLabel:foot,endnoteLabel:end};
}
function nativeErrorText(text){
  assert.equal(typeof text,'string');assert(Buffer.byteLength(text,'utf8')<=16384,'Native error text exceeds its bounded wire body');const value=JSON.parse(text),frames=[];
  // JSON.parse alone discards duplicate object keys. Do not let a disagreeing
  // earlier part/path hide behind a last-value-wins text projection.
  for(let at=0;at<text.length;at++){
    const ch=text[at],frame=frames.at(-1);if(ch==='"'){
      const start=at++;for(;at<text.length;at++){if(text[at]==='\\')at++;else if(text[at]==='"')break;}
      if(frame?.keys&&frame.key){const key=JSON.parse(text.slice(start,at+1));assert(!frame.keys.has(key),'Duplicate native error text field: '+key);frame.keys.add(key);}
    }else if(ch==='{')frames.push({keys:new Set(),key:true});else if(ch==='[')frames.push({});else if(ch==='}'||ch===']')frames.pop();else if(ch===':'&&frame?.keys)frame.key=false;else if(ch===','&&frame?.keys)frame.key=true;
  }return value;
}
function assertConversionError(result,expected){
  assert(['docx_note_settings_invalid','docx_note_settings_unsupported','author_inline_text_unsupported'].includes(expected),'Expected one exact registered native import error code');
  assert.equal(result?.isError,true,'Invalid native data was silently accepted');const value=result.structuredContent;
  assert(value&&typeof value==='object'&&!Array.isArray(value),'Missing structured native error body');
  assert.deepEqual(Object.keys(value).sort(),['error','error_code','error_details','ok']);assert.equal(value.ok,false);assert.equal(value.error_code,expected);assert.equal(value.error,expected,'Raw prefixes or damaged-file suffixes cannot substitute for the typed error');
  const native=value.error_details;assert(native&&typeof native==='object'&&!Array.isArray(native),'Missing native error_details object');assert.deepEqual(Object.keys(native).sort(),['detail','part','path']);
  for(const [key,limit]of [['part',512],['path',2048],['detail',2048]]){assert.equal(typeof native[key],'string','Native error detail must be a string: '+key);assert(native[key].length&&Buffer.byteLength(native[key],'utf8')<=limit&&!/\p{Cc}/u.test(native[key]),'Native error detail outside wire bounds: '+key);}
  assert(Array.isArray(result.content)&&result.content.length===1&&result.content[0].type==='text','Native conversion error must have one matching text body');assert.deepEqual(nativeErrorText(result.content[0].text),value,'Structured and text native error bodies disagree');
  return {code:value.error_code,detail:native.detail,native};
}
function assertMutation(report,before,after){
  assert.equal(report.metadataResigned,false);assert.equal(report.applicationEditingCertified,false);assert.equal(report.inputSha256,before.sha256);assert.equal(report.outputSha256,after.sha256);
  assert.deepEqual(after.auxiliaryParts,before.auxiliaryParts,'Native mutation rewrote provenance');
  const changed=Object.keys(before.parts).filter(name=>before.parts[name]!==after.parts[name]).sort();assert.deepEqual(changed,report.changedParts);for(const [name,digest]of Object.entries(report.unchangedParts))assert.equal(after.parts[name],digest);
}
function textReferences(text){
  // Decode once after tokenization. Decoding Markdown itself would activate
  // literal comments/links or incorrectly accept doubly escaped content.
  return String(text).replace(/&(#(?:[xX][\da-fA-F]+|\d+)|[A-Za-z][\dA-Za-z]*);/g,(raw,name)=>{
    if(name[0]!=='#')return Object.hasOwn(htmlReferences,name+';')?htmlReferences[name+';']:raw;
    const value=/^#[xX]/.test(name)?parseInt(name.slice(2),16):Number(name.slice(1));
    return String.fromCodePoint(!value||value>0x10ffff||value>=0xd800&&value<=0xdfff?0xfffd:value);
  });
}
function legacyInlineStructure(tokens){
  let text='',supported=true;const links=[];
  for(const token of tokens||[]){
    if(token.type==='html'){if(!/^<!--[\s\S]*-->$/.test(token.raw||''))supported=false;}
    else if(token.type==='codespan'||token.type==='image')supported=false;
    else if(token.type==='escape')text+=token.text||'';
    else if(token.type==='br')text+='\n';
    else if(token.tokens){const child=legacyInlineStructure(token.tokens);text+=child.text;supported&&=child.supported;links.push(...child.links);if(token.type==='link')links.push({text:child.text,target:textReferences(token.href||'')});}
    else if(token.type==='text')text+=textReferences(token.text||'');
    else supported=false;
  }
  return {text,supported,links};
}
// Independent expected-source grammar. This intentionally does not call the
// product author parser or trust a product projection/model as its own oracle.
function ownedSpanCandidate(raw){return /^<span(?=[\t\r\n >])/i.test(raw||'')&&/\bdata-readmd-text\b/i.test(raw);}
function controlledSpanTag(raw){
  if(/^<\/span[\t\r\n ]*>$/i.test(raw))return {close:true};
  assert(ownedSpanCandidate(raw),'Only an owned author span can be transparent');assert(raw.length<=4096,'Bounded owned author marker');
  const attributes={},style={};let at=5;
  while(at<raw.length){
    const spaces=/^[\t\r\n ]*/.exec(raw.slice(at))[0];at+=spaces.length;
    if(raw[at]==='>'){assert.equal(at+1,raw.length,'Trailing owned marker content');break;}
    assert(spaces.length,'Owned attributes require whitespace');const match=/^([A-Za-z_:][A-Za-z0-9_.:-]*)[\t\r\n ]*=[\t\r\n ]*(?:"([^"]*)"|'([^']*)')/.exec(raw.slice(at));assert(match,'Malformed owned author attribute');
    const key=match[1].toLowerCase();assert(!Object.hasOwn(attributes,key),'Duplicate owned author attribute');attributes[key]=match[2]??match[3];at+=match[0].length;
  }
  assert.equal(raw.at(-1),'>');assert(at<raw.length,'Unclosed owned author marker');assert.equal(attributes['data-readmd-text'],'v1','Unknown owned author version');
  const sizes={'data-size-pt':'size','data-size-cs-pt':'sizeCs'},flags={'data-cs':'cs','data-rtl':'rtl'};
  for(const [key,value]of Object.entries(attributes)){
    if(key==='data-readmd-text')continue;
    if(Object.hasOwn(sizes,key)){
      if(value==='base')style[sizes[key]]='base';
      else {assert(/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/.test(value),'Expected numeric author points');const points=Number(value),half=points*2;assert(Number.isFinite(points)&&half>0&&Number.isInteger(half)&&half<=2147483647,'Expected exact positive native half-points');style[sizes[key]]=points;}
    }else if(Object.hasOwn(flags,key)){assert(value==='true'||value==='false','Expected exact boolean author flag');style[flags[key]]=value==='true';}
    else assert.fail('Unreviewed owned author attribute: '+key);
  }
  return {style};
}
function hasOwnedSpan(tokens){const pending=[tokens||[]];while(pending.length)for(const token of pending.pop()){if(token.type==='html'&&ownedSpanCandidate(token.raw||''))return true;if(token.tokens)pending.push(token.tokens);}return false;}
function controlledChildRaw(token,child){
  const original=(token.tokens||[]).map(item=>item.raw||'').join(''),raw=token.raw||'';if(child.semanticRaw===original)return raw;
  const from=token.type==='strong'||token.type==='del'?2:token.type==='em'||token.type==='link'?1:token.type==='text'?0:-1;
  assert(from>=0&&raw.slice(from,from+original.length)===original,'Exact owned child source boundary');return raw.slice(0,from)+child.semanticRaw+raw.slice(from+original.length);
}
function controlledInlineStructure(tokens){
  const state={stack:[],nodes:0,outside:false};
  function walk(values,depth){
    assert(depth+state.stack.length<=32,'Bounded owned author depth');const entered=state.stack.length,links=[];let text='',semanticRaw='';
    function append(value){text+=value;if(!state.stack.length&&value.trim())state.outside=true;}
    for(const token of values||[]){
      assert(++state.nodes<=32768,'Bounded owned inline nodes');const raw=token.raw||'';
      if(token.type==='html'){
        if(/^<!--[\s\S]*-->$/.test(raw)){semanticRaw+=raw;continue;}
        const tag=controlledSpanTag(raw);if(tag.close){assert(state.stack.length>entered,'Unbalanced/crossed owned close');state.stack.pop();}
        else {state.stack.push(tag.style);assert(depth+state.stack.length<=32,'Bounded owned author depth');}
      }else if(token.type==='codespan'||token.type==='image')assert.fail('Code and images cannot supply owned source evidence');
      else if(token.type==='escape'){append(token.text||'');semanticRaw+=raw;}
      else if(token.type==='br'){append('\n');semanticRaw+=raw;}
      else if(token.tokens){const child=walk(token.tokens,depth+1);append(child.text);links.push(...child.links);if(token.type==='link')links.push({text:child.text,target:textReferences(token.href||'')});semanticRaw+=controlledChildRaw(token,child);}
      else if(token.type==='text'){append(textReferences(token.text||''));semanticRaw+=raw;}
      else assert.fail('Unreviewed owned inline token: '+token.type);
    }
    assert.equal(state.stack.length,entered,'Unbalanced owned author span');return {text,links,semanticRaw};
  }
  try{return {...walk(tokens,0),supported:true,outside:state.outside};}catch(error){return {text:'',links:[],semanticRaw:'',supported:false,error:error.message};}
}
function inlineStructure(tokens){return hasOwnedSpan(tokens)?controlledInlineStructure(tokens):legacyInlineStructure(tokens);}
function markdownTokens(source){
  const tokens=marked.lexer(String(source).replace(/^\uFEFF?---[ \t]*\r?\n[\s\S]*?\r?\n(?:---|\.\.\.)[ \t]*(?:\r?\n|$)/,''),{gfm:true});
  return tokens.map(token=>{
    // CommonMark type 7 can turn a whole-line first author span into opaque
    // HTML. Only a fully paired owned inline scope may become a paragraph;
    // its body is never re-lexed as blocks/headings/source declarations.
    if(token.type!=='html'||!ownedSpanCandidate(token.raw||''))return token;
    const inline=marked.Lexer.lexInline(token.raw,{gfm:true}),row=controlledInlineStructure(inline);if(!row.supported||row.outside)return token;
    return {type:'paragraph',raw:token.raw,text:token.raw,tokens:inline,controlledBlock:true};
  });
}
function markdownStructure(source){
  const paragraphs=[],anchors=[],diagnostics=[],headings=[],numberingDeclarations=[],citationDiagnostics=[];
  for(const token of markdownTokens(source)){
    if(token.type==='html'){
      const comment=/^<!--readmd-numbering-import:v1\s+(\[[\s\S]*\])\s*-->\s*$/.exec(token.raw||'');
      if(comment){const rows=JSON.parse(comment[1]);assert(Array.isArray(rows));diagnostics.push(...rows);}
      const citation=/^<!--readmd:citation-import:v1\s+(\{[\s\S]*\})\s*-->\s*$/.exec(token.raw||'');
      if(citation){const data=JSON.parse(citation[1]);assert.equal(data.version,1);assert(Array.isArray(data.diagnostics));citationDiagnostics.push(...data.diagnostics);}
      if(/^<!--\s*readmd-numbering:v1\b/.test(token.raw||''))numberingDeclarations.push(token.raw);
    }else if(token.type==='paragraph'){
      const anchor=/^<span\s+id=(['"])([^'"]+)\1\s*><\/span>\s*$/.exec(token.raw||'');
      if(anchor)anchors.push(textReferences(anchor[2]));
      else {const row=inlineStructure(token.tokens);if(row.supported)paragraphs.push({...row,raw:token.raw});}
    }else if(token.type==='heading'){
      const row=inlineStructure(token.tokens);if(!row.supported)continue;
      const attrs=/[ \t]*\{#([^\s{}]+)(?:[ \t]+\.[\w-]+)*\}[ \t]*$/.exec(row.text);
      headings.push({text:attrs?row.text.slice(0,attrs.index).trimEnd():row.text,id:attrs?attrs[1]:null,level:token.depth});
    }
    // The owned literal must occupy actual top-level content. Opaque HTML,
    // code and quoted examples cannot supply its text, link or diagnostic.
  }
  return {paragraphs,anchors,diagnostics,headings,numberingDeclarations,citationDiagnostics};
}
function assertChangedGeneratedNote(markdown){
  const view=markdownStructure(markdown),label=noteLabel(markdown,'SOURCE_GENERATED_CITATION','SOURCE_GENERATED_CITATION p. 12');
  assert(view.paragraphs.some(row=>row.raw.includes('Generated [^'+label+']')),'Changed generated note lost its current body reference or was falsely restored');
  for(const [reason,part]of [['generated_citation_note_modified_or_unbound','word/document.xml'],['citation_render_modified','word/footnotes.xml']]){
    assert(view.citationDiagnostics.some(row=>row.code==='docx_citation_conflict'&&row.reason===reason&&row.part===part&&row.original_source==='[@work, p. 12]'&&row.visible_content_preserved===true),'Resized generated citation lost current-content/source diagnostics: '+reason);
  }
  return {label,sourcePreserved:true,currentNoteVisible:true,editableCitationRestored:false};
}
function numberingImportDiagnostics(markdown){return markdownStructure(markdown).diagnostics;}
function assertHeadingReferenceOpen(markdown){
  const view=markdownStructure(markdown),diagnostics=view.diagnostics,matches=diagnostics.filter(row=>row.code==='docx_numbering_import_conflict'&&row.reason==='reference_preserved_as_link'&&row.visible_content_preserved===true);
  assert.equal(matches.length,1,'Unsupported automatic heading REF lost its explicit diagnostic');
  const target=matches[0].source_id;assert.equal(typeof target,'string');assert(/^[A-Za-z0-9_-]+$/.test(target),'Invalid current heading identity');
  const links=view.paragraphs.flatMap(row=>row.links).filter(row=>row.text==='NATIVE_ALPHA'&&row.target==='#'+target);assert.equal(links.length,1,'Current heading REF text/link was lost or retargeted');
  const headings=view.headings.filter(row=>row.text==='NATIVE_ALPHA');assert.equal(headings.length,1,'Current target heading identity was lost or duplicated');assert.equal(headings[0].level,1);assert.equal(headings[0].id,target,'Preserved link no longer targets its actual current heading');
  return diagnostics;
}
function assertCaptionConflict(markdown){
  const view=markdownStructure(markdown),diagnostics=view.diagnostics,matches=diagnostics.filter(row=>row.code==='docx_numbering_import_conflict'&&row.reason==='caption_result_modified'&&row.source_id==='tbl:settings'&&row.visible_content_preserved===true);assert.equal(matches.length,1,'Unrefreshed native chapter cache lost its exact hand-edit diagnostic');
  assert.equal(view.paragraphs.filter(row=>row.text==='Table III.4 — SETTINGS_TABLE').length,1,'Current unrefreshed caption was hidden or overwritten');assert.equal(view.anchors.filter(id=>id==='tbl:settings').length,1,'Preserved caption source identity was lost or duplicated');
  assert(!view.paragraphs.some(row=>/:label\[tbl:settings\]|:ref\[tbl:settings\]/.test(row.text)),'Modified caption was silently reactivated');const links=view.paragraphs.flatMap(row=>row.links).filter(row=>row.text==='Table III.4'&&row.target==='#tbl:settings');assert.equal(links.length,1,'Current literal REF text/link was lost or retargeted');return diagnostics;
}
function assertSourceCaptionIdentity(markdown){
  const blocks=markdownTokens(markdown).filter(token=>token.type!=='space'&&!(token.type==='html'&&/^<!--[\s\S]*-->\s*$/.test(token.raw||''))),tables=blocks.filter(token=>token.type==='table');
  assert.equal(tables.length,1,'Current automatic caption requires exactly one top-level GFM table');
  const table=tables[0],headers=table.header.map(cell=>inlineStructure(cell.tokens)),rows=table.rows.map(row=>row.map(cell=>inlineStructure(cell.tokens)));
  assert(headers.every(cell=>cell.supported)&&rows.every(row=>row.every(cell=>cell.supported)),'Hidden HTML or code cannot supply table cells');assert.deepEqual(headers.map(cell=>cell.text),['Measure','Value'],'Current source table header changed');assert.deepEqual(rows.map(row=>row.map(cell=>cell.text)),[['SETTINGS_CELL','42']],'Current source table cells changed');
  const paragraphs=blocks.map((token,index)=>({token,index,...(token.type==='paragraph'?inlineStructure(token.tokens):{supported:false})})).filter(row=>row.supported),captions=paragraphs.filter(row=>row.text===':label[tbl:settings] SETTINGS_TABLE'&&/^:label\[tbl:settings\] SETTINGS_TABLE\s*$/.test(row.semanticRaw??row.token.raw));
  assert.equal(captions.length,1,'Current automatic caption lost its original source identity');assert.equal(blocks[captions[0].index-1],table,'Automatic caption is no longer adjacent to its actual top-level table');
  const refs=paragraphs.filter(row=>row.text==='See :ref[tbl:settings].'&&(row.semanticRaw??row.token.raw).trimStart().startsWith('See :ref[tbl:settings]'));assert.equal(refs.length,1,'Current editable reference lost its original source identity');
  return {sourceId:'tbl:settings',oneTable:true,adjacent:true};
}
async function main(){
  const options=argumentsFor(process.argv.slice(2));if(options.help){console.log('--candidate <json> --python <existing QA Python> [--binary exe] [--assets dir] [--output owned-dir] [--plan]');return;}if(options.plan){console.log(JSON.stringify(planFor(),null,2));return;}
  assert.equal(process.platform,'win32');const binding=bindingFor(options);assert(fs.statSync(options.python).isFile());fs.mkdirSync(BASE,{recursive:true});fs.mkdirSync(SECTION_BASE,{recursive:true});
  const evidence=options.output?(fs.mkdirSync(options.output),options.output):fs.mkdtempSync(path.join(SECTION_BASE,'stdio-'));
  for(const folder of ['profile','documents','fixtures','outputs','requests','responses','inspection','wire-first','wire-reopened'])fs.mkdirSync(path.join(evidence,folder));
  const retain=(name,value)=>{const file=inside(evidence,path.join(evidence,name));fs.writeFileSync(file,typeof value==='string'||Buffer.isBuffer(value)?value:JSON.stringify(value,null,2)+'\n',{flag:'wx'});return file;};
  const report={schemaVersion:1,scope:'real-stdio-docx-settings-save-restart-native-return',synthetic:true,passed:false,startedUtc:new Date().toISOString(),candidateBinding:binding,
    python:{path:options.python,sha256:fileHash(options.python),qaOnly:true,standardLibraryOnly:true},checks:[],outputs:[],sessions:[],roundtrips:[],openCases:[],guiStarted:false,applicationRoundtripCertified:false,headlessMutationsAreWordInterop:false,fullBatch3Certified:false};
  const documents=path.join(evidence,'documents'),profilePath=path.join(evidence,'profile');let client,timer,serial=0,sectionState;
  const check=(name,details={})=>report.checks.push({name,...details});
  const call=async(name,args,timeout=60000)=>{const index=++serial,request={name,arguments:args};assertToolCallSchema(request,client.tools);retain(`requests/${index}-${name}.json`,request);const result=await client.tool(name,args,timeout);retain(`responses/${index}-${name}.json`,result);return result;};
  const oracle=(label,args)=>{verifyBinding(binding);const result=spawnSync(options.python,['-I','-B',path.join(ROOT,ORACLE),...args],{windowsHide:true,timeout:60000,maxBuffer:32*1024*1024,env:systemEnvironment(profilePath,documents)});retain('inspection/'+label+'.stdout.json',result.stdout||Buffer.alloc(0));retain('inspection/'+label+'.stderr.log',result.stderr||Buffer.alloc(0));assert(!result.error,result.error?.message);assert.equal(result.status,0,'Independent native oracle failed: '+label);verifyBinding(binding);return JSON.parse(result.stdout);};
  const inspect=(label,file)=>oracle(label,['inspect',file]);
  const exportDocx=async(label,source,{options:overrides={},preset}={})=>{
    verifyBinding(binding);const output=path.join(evidence,'outputs',label+'.docx'),args={markdown_content:source,output_path:output,output_format:'docx',base_dir:documents,confirm:true,options:overrides};if(preset!==undefined)args.style_preset=preset;
    const result=payload(await call('readmd_export_document',args,90000));assert(fs.statSync(output).size>0);const value=inspect(label+'-native',output);report.outputs.push({label,path:path.relative(evidence,output),bytes:fs.statSync(output).size,sha256:fileHash(output),warnings:result.warnings||result.warns||[]});return {file:output,value,args};
  };
  const profile=async content=>payload(await call('readmd_publication_profile',{action:'get',content}));
  const queue=[];
  const convertOnly=async(label,file)=>{
    const parent=path.dirname(file),beforeNames=fs.readdirSync(parent).sort(),beforeHash=fileHash(file),text=plainText(await call('readmd_convert_to_markdown',{file_path:file}));retain('inspection/'+label+'-imported.md',text);
    assert.equal(fileHash(file),beforeHash,'Conversion changed original DOCX');assert.deepEqual(fs.readdirSync(parent).sort(),beforeNames,'Conversion created an unsolicited sidecar');
    return text;
  };
  const importAndSave=async(label,file,{verifyImported,verifyOutput,markers}={})=>{
    const text=await convertOnly(label,file);
    const current=await profile(text);if(markers)assertProfileMatchesNotes(current,text,markers);if(verifyImported)await verifyImported(text,current);
    const saved=path.join(documents,label+'.md'),sentinel='OWNED_SETTINGS_SAVE_SENTINEL_'+label+'\n';retain('documents/'+label+'.md',sentinel);
    const read=payload(await call('readmd_read_document',sourceReadArguments(saved,sentinel)));assert.equal(read.content,sentinel);assert.equal(read.revision,hash(sentinel));
    const request={file_path:saved,expected_revision:read.revision,old_text:sentinel,new_text:text,dry_run:false,confirm:true};assertSavedEdit(payload(await call('readmd_edit_document',request)),sentinel,text);assert.equal(fs.readFileSync(saved,'utf8'),text);
    const row={label,file:saved,markdown:text,sha256:fileHash(saved),verifyOutput};queue.push(row);report.roundtrips.push({label,input:path.relative(evidence,file),saved:path.relative(evidence,saved),savedSha256:row.sha256,conversionDidNotSave:true,savedViaRevisionProtectedMcp:true,reopenedInNewProcess:false,secondExportWithoutOldOptions:false});
    check(label+': actual conversion then explicit revision-guarded Markdown save');return row;
  };
  const sectionContext={oracle,inspect,exportDocx,importAndSave,convertOnly,profile,call,retain,check,report,evidence,documents,
    helpers:{markdownStructure,markdownTokens,noteLabel,toolValue,payload,fileHash,assertConversionError,assertProfileTransaction,assertSavedEdit}};
  try{
    retain('plan.json',planFor());retain('profile/export_presets.json',PRESET_DATA);const presetFile=path.join(profilePath,'export_presets.json'),presetHash=fileHash(presetFile);
    retain('documents/refs.json',JSON.stringify([{id:'work',type:'book',title:'SOURCE_GENERATED_CITATION',author:[{family:'SettingsAuthor'}]}]));retain('documents/note.csl',noteStyle());
    const source=sourceFor(PROFILE,{citation:true});retain('documents/authored-input.md',source);
    const fixtures=[];for(const kind of NATIVE_POSITIVES){const file=path.join(evidence,'fixtures',kind+'.docx'),created=oracle('fixture-'+kind,['fixture','--kind',kind,'--output',file]);assert.equal(created.nativeOnly,true);assert.equal(created.sourcePayloadPresent,false);assert.deepEqual(created.inspection.auxiliaryParts,{});assertNativeFixture(created.inspection,{kind});fixtures.push({kind,file});}
    report.openCases.push({name:'ordinary-list-punctuation-projection',nativeReturnCertified:false,certifiedScope:'Ordinary lists remain lists instead of becoming outline headings',knownBoundary:'Existing native %1) list punctuation projects through Markdown to %1.; exact ordinary-list formatting is not certified'});
    const start=async wire=>{verifyBinding(binding);client=new StdioClient({binary:binding.binary,assets:binding.assets,profile:profilePath,documents,out:path.join(evidence,wire)});await client.initialize(true);for(const name of planFor().requiredTools)assert(client.tools.some(tool=>tool.name===name),'Missing real '+name);retain(wire+'/tools.json',client.tools);};
    timer=setTimeout(()=>{report.workflowTimedOut=true;if(client){client.transport.fail(Error('Settings roundtrip exceeded 600000ms'),'workflow_deadline');client.forceStop();}},600000);
    await start('wire-first');const catalog=payload(await call('readmd_export_presets',{}));assert(catalog.custom?.[PRESET],'Owned custom preset was not read by the product');
    const get=await profile(source);assert.equal(get.revision,hash(source));assert.equal(get.sourceValues.version,1);assert.deepEqual(get.sourceValues.footnotes.kindByLabel,PROFILE.footnotes.kindByLabel);
    const patch=payload(await call('readmd_publication_profile',{action:'patch',content:source,base_revision:get.revision,patch:{footnotes:{kinds:{footnote:{size:10.5}}}}}));assertProfileTransaction(patch,source);assert(patch.content.startsWith('\uFEFF---\r\n'));assert(patch.content.includes("catalog_id: '00123' # KEEP_NEIGHBOR\r\n"));assert(patch.content.includes("ownedNeighbor: '007' # KEEP_READMD_NEIGHBOR\r\n"));assert.equal(patch.content.slice(patch.content.indexOf('# SETTINGS_ALPHA')),source.slice(source.indexOf('# SETTINGS_ALPHA')));
    const stale=await call('readmd_publication_profile',{action:'patch',content:patch.content,base_revision:get.revision,patch:{}});assert.equal(stale.isError,true);assert.equal(toolValue(stale).error_code,'transaction_conflict');
    const inverse=payload(await call('readmd_content_transaction',{content:patch.content,transaction:{base_revision:patch.revision,origin:'editor',edits:patch.inverse}}));assert.equal(inverse.result.content,source);assert.equal(inverse.result.revision,get.revision);
    const noChange=payload(await call('readmd_publication_profile',{action:'patch',content:patch.content,base_revision:patch.revision,patch:{}}));assertProfileTransaction(noChange,patch.content,{changed:false});
    const removed=payload(await call('readmd_publication_profile',{action:'patch',content:patch.content,base_revision:patch.revision,patch:{footnotes:{kinds:{footnote:{size:null}}}}}));assertProfileTransaction(removed,patch.content);assert(!Object.hasOwn(removed.sourceValues.footnotes.kinds.footnote,'size'));assert.equal(removed.sourceValues.footnotes.kinds.endnote.size,11);
    const explicit=payload(await call('readmd_publication_profile',{action:'patch',content:removed.content,base_revision:removed.revision,patch:{footnotes:{kinds:{footnote:{size:9,start:1}}}}}));assertProfileTransaction(explicit,removed.content);assert.equal(explicit.sourceValues.footnotes.kinds.footnote.start,1);assert.equal(explicit.sourceValues.footnotes.kinds.footnote.size,9);
    const future=source.replace('"version":1','"version":1,"future":"007"'),futureRead=await profile(future);assert.equal(futureRead.sourceValues.future,'007');assert(futureRead.diagnostics.some(row=>row.path==='source.readmd.publication.future'&&row.from>=0&&row.to>row.from));
    check('Source profile transaction preserves neighboring YAML, BOM/CRLF and body; null deletes, explicit defaults persist, inverse is exact and stale revision fails');
    const original=await exportDocx('source-original',source);assertSourceNative(original.value);assert(original.value.notes.footnote.definitions.some(row=>row.type==='normal'&&row.text.includes('SOURCE_GENERATED_CITATION')),'Generated citation note was lost');
    await importAndSave('source-unchanged',original.file,{markers:['SOURCE_FOOT_BODY','SOURCE_END_BODY'],verifyImported:text=>{assertSourceCaptionIdentity(text);assert(text.includes('[@work, p. 12]'),'Unchanged generated note did not restore editable citation');},verifyOutput:value=>assertSourceNative(value)});
    const defaultsDoc=await exportDocx('source-explicit-defaults',explicit.content);assertSourceNative(defaultsDoc.value,{footStart:1,footSize:9});
    await importAndSave('source-explicit-defaults',defaultsDoc.file,{markers:['SOURCE_FOOT_BODY','SOURCE_END_BODY'],verifyImported:(text,current)=>{const policy=current.sourceValues.footnotes.kinds.footnote;assert(Object.hasOwn(policy,'start'));assert.equal(policy.start,1);assert(Object.hasOwn(policy,'size'));assert.equal(policy.size,9);},verifyOutput:value=>assertSourceNative(value,{footStart:1,footSize:9})});
    const alteredFile=path.join(evidence,'fixtures/source-native.docx'),mutation=oracle('source-native-mutation',['mutate','--mode','source-native','--input',original.file,'--output',alteredFile]);assertMutation(mutation,original.value,mutation.inspection);
    assert.throws(()=>assertSourceNative(mutation.inspection,{mutated:true}),'Parameter-only mutation must not masquerade as refreshed fields');assert.equal(mutation.inspection.paragraphs.find(row=>row.text==='SETTINGS_ALPHA').rule.start,'5');assert(mutation.inspection.paragraphs.some(row=>row.fields.some(field=>/^\s*STYLEREF\s+1\s/.test(field.instruction)&&field.cached==='III')));
    const conflicting=await convertOnly('source-native-cache-conflict',alteredFile);assertCaptionConflict(conflicting);const reviewed=payload(await call('readmd_academic_document',{content:conflicting,baseDir:documents}));assertCaptionConflict(reviewed.content);check('Parameter-only native mutation preserves current caption/cache and literal REF with diagnostics; prepare never reactivates it');
    const refreshedFile=path.join(evidence,'fixtures/source-native-refreshed.docx'),refreshed=oracle('source-native-refreshed-mutation',['mutate','--mode','source-native-refreshed','--input',original.file,'--output',refreshedFile]);assertMutation(refreshed,original.value,refreshed.inspection);assert.equal(refreshed.fieldCacheRefresh.engineRefreshCertified,false);assertSourceNative(refreshed.inspection,{mutated:true});
    await importAndSave('source-native-changed',refreshedFile,{markers:['SOURCE_FOOT_BODY','SOURCE_END_BODY'],verifyImported:assertSourceCaptionIdentity,verifyOutput:value=>assertSourceNative(value,{mutated:true})});
    assert(original.value.auxiliaryParts['customXml/readmd-numbering-source.xml'],'Clear cases require an actual exported source binding, not a fabricated manifest');
    for(const mode of ['numbering-clear','notes-clear']){
      const file=path.join(evidence,'fixtures/source-native-'+mode+'.docx'),cleared=oracle(mode+'-mutation',['mutate','--mode',mode,'--input',original.file,'--output',file]);assertMutation(cleared,original.value,cleared.inspection);assert.equal(cleared.nativeClear.applicationEditingCertified,false);
      if(mode==='numbering-clear'){
        assertNumberingCleared(cleared.inspection);assert(cleared.nativeClear.headingsDisabled>=4&&cleared.nativeClear.fieldsFlattened>=3);
        await importAndSave('source-native-numbering-clear',file,{markers:['SOURCE_FOOT_BODY','SOURCE_END_BODY'],verifyImported:assertNumberingClearedImport,verifyOutput:assertNumberingCleared});
      }else{
        assertNotesCleared(cleared.inspection,{input:true});assert(cleared.nativeClear.notePropertiesRemoved>=2);
        await importAndSave('source-native-notes-clear',file,{markers:['SOURCE_FOOT_BODY','SOURCE_END_BODY'],verifyImported:(text,current)=>{assertNotesClearedProfile(current);assertSourceCaptionIdentity(text);assert(text.includes('[@work, p. 12]'),'Clearing native note settings lost an unchanged editable citation');},verifyOutput:assertNotesCleared});
      }
    }
    for(const fixture of fixtures)await importAndSave(fixture.kind,fixture.file,{markers:fixture.kind==='native-caption-numbering'?undefined:['NATIVE_FOOT_BODY','NATIVE_END_BODY'],verifyImported:(text,current)=>{assert(text.includes('NATIVE_ALPHA')&&text.includes('NATIVE_METHOD'));if(fixture.kind==='native-caption-numbering')assert.deepEqual(current.sourceValues,{});},verifyOutput:value=>assertNativeFixture(value,{kind:fixture.kind})});
    const legacyHeadingFile=path.join(evidence,'fixtures/native-heading-ref-legacy-hyphen.docx'),legacyHeading=oracle('fixture-native-heading-ref-legacy-hyphen',['fixture','--kind','native-numbering','--output',legacyHeadingFile]);assert(legacyHeading.inspection.paragraphs.some(row=>row.fields.some(field=>/^\s*REF\s+native-alpha\s/.test(field.instruction))));
    report.openCases.push({name:'legacy-hyphen-heading-ref-material',input:path.relative(evidence,legacyHeadingFile),nativeAutomaticReturnCertified:false,productRoundtripExecuted:false,reason:'Original native-alpha material retained; its target is outside the current REF grammar and cannot exercise the current-link preservation boundary'});
    const headingFile=path.join(evidence,'fixtures/native-heading-ref-open.docx'),heading=oracle('fixture-native-heading-ref-open',['fixture','--kind','native-heading-ref-safe','--output',headingFile]);assert(heading.inspection.paragraphs.some(row=>row.bookmarks.some(mark=>mark.name==='native_alpha')));assert(heading.inspection.paragraphs.some(row=>row.fields.some(field=>/^\s*REF\s+native_alpha\s/.test(field.instruction))));
    const headingHash=fileHash(headingFile),headingNames=fs.readdirSync(path.dirname(headingFile)).sort(),headingText=plainText(await call('readmd_convert_to_markdown',{file_path:headingFile}));retain('inspection/native-heading-ref-open.md',headingText);assert.equal(fileHash(headingFile),headingHash);assert.deepEqual(fs.readdirSync(path.dirname(headingFile)).sort(),headingNames);
    const headingDiagnostics=assertHeadingReferenceOpen(headingText);
    report.openCases.push({name:'native-heading-ref',input:path.relative(evidence,headingFile),nativeAutomaticReturnCertified:false,currentLiteralLinkPreserved:true,diagnostics:headingDiagnostics});check('Heading REF retains current visible content/link and explicit diagnostic; native automatic heading REF remains open');
    const extendedFile=path.join(evidence,'fixtures/extended-format.docx'),extended=oracle('extended-format-mutation',['mutate','--mode','extended-format','--input',original.file,'--output',extendedFile]);assertMutation(extended,original.value,extended.inspection);
    const styleOnlyNative=assertStyleOnlyMixedNoteFixture(extended.inspection);check('Actual style-only mutation retains30pt template/gap and10pt manual/generated authors before import',styleOnlyNative);
    await importAndSave('extended-format-style-only',extendedFile,{markers:['SOURCE_FOOT_BODY','SOURCE_END_BODY'],verifyImported:async(text,current)=>{assertStyleOnlyMixedNoteImport(text,current);const prepared=payload(await call('readmd_academic_document',{content:text,baseDir:documents}));assert.equal(prepared.projection.citations.length,0,'A nonreproducible current note was silently restored into active CSL');assert(prepared.warnings.includes('docx_citation_conflict'));noteLabel(prepared.content,'SOURCE_GENERATED_CITATION','SOURCE_GENERATED_CITATION p. 12');assert.equal(markdownStructure(prepared.content).paragraphs.filter(row=>row.text==='SOURCE_GENERATED_CITATION').length,1,'Prepare removed the current literal bibliography');},verifyOutput:assertStyleOnlyMixedNoteFixture});
    const uniformFile=path.join(evidence,'fixtures/extended-format-uniform.docx'),uniform=oracle('extended-format-uniform-mutation',['mutate','--mode','extended-format-uniform','--input',original.file,'--output',uniformFile]);assertMutation(uniform,original.value,uniform.inspection);assertUniformNormalFootnoteRuns(uniform.inspection);assertSourceNative(uniform.inspection,{format:'chicago',footStart:12000,footSize:30,footSizeCs:30});assert.equal(uniform.uniformFootnoteText.metadataResigned,false);
    await importAndSave('extended-format',uniformFile,{markers:['SOURCE_FOOT_BODY','SOURCE_END_BODY'],verifyImported:async text=>{assertChangedGeneratedNote(text);const prepared=payload(await call('readmd_academic_document',{content:text,baseDir:documents}));assert.equal(prepared.projection.citations.length,0,'A modified generated note was silently restored as an unchanged citation');assert(prepared.warnings.includes('docx_citation_conflict'));assertChangedGeneratedNote(prepared.content);},verifyOutput:value=>{assertSourceNative(value,{format:'chicago',footStart:12000,footSize:30,footSizeCs:30});assertUniformNormalFootnoteRuns(value);assert.equal(normalNote(value,'footnote','SOURCE_GENERATED_CITATION').text.trim(),'SOURCE_GENERATED_CITATION p. 12','Current generated-note text or locator changed');}});
    for(const mode of ['unknown-format','foreign-format']){
      const file=path.join(evidence,'fixtures',mode+'.docx'),changed=oracle(mode+'-mutation',['mutate','--mode',mode,'--input',original.file,'--output',file]);assertMutation(changed,original.value,changed.inspection);
      const before=fs.readdirSync(path.dirname(file)).sort(),digest=fileHash(file);const failure=assertConversionError(await call('readmd_convert_to_markdown',{file_path:file}),mode==='unknown-format'?'docx_note_settings_invalid':'docx_note_settings_unsupported');assert.equal(fileHash(file),digest);assert.deepEqual(fs.readdirSync(path.dirname(file)).sort(),before);check(mode+': typed rejection keeps original native document and creates no Markdown/asset sidecars',failure);
    }
    const legacySectionsFile=path.join(evidence,'fixtures/native-multi-section.docx'),legacySections=oracle('fixture-native-multi-section-legacy-ref',['fixture','--kind','native-multi-section','--output',legacySectionsFile]);assert.equal(legacySections.nativeOnly,true);assert.equal(legacySections.sourcePayloadPresent,false);assert.deepEqual(legacySections.inspection.auxiliaryParts,{});assertNativeMultiSection(legacySections.inspection);
    const legacyDigest=fileHash(legacySectionsFile),legacyDirectory=fs.readdirSync(path.dirname(legacySectionsFile)).sort(),legacyFailure=assertLegacyMultiSectionReferenceRejection(await call('readmd_convert_to_markdown',{file_path:legacySectionsFile}));assert.equal(fileHash(legacySectionsFile),legacyDigest);assert.deepEqual(fs.readdirSync(path.dirname(legacySectionsFile)).sort(),legacyDirectory);check('Legacy multi-section hyphen REF is rejected with its exact typed field location; original native document and directory stay unchanged',legacyFailure);
    report.openCases.push({name:'native-multi-section-legacy-heading-ref',input:path.relative(evidence,legacySectionsFile),nativeAutomaticReturnCertified:false,typedRejectionExecuted:true,errorCode:legacyFailure.code,reason:'Legacy native-alpha target remains outside the current editable REF subset; the field and original file are retained.'});
    const sectionsFile=path.join(evidence,'fixtures/native-multi-section-safe-ref.docx'),sections=oracle('fixture-native-multi-section-safe-ref',['fixture','--kind','native-multi-section-safe-ref','--output',sectionsFile]);assert.equal(sections.nativeOnly,true);assert.equal(sections.sourcePayloadPresent,false);assert.deepEqual(sections.inspection.auxiliaryParts,{});assertSafeMultiSectionFixture(legacySections.inspection,sections.inspection);
    await importAndSave('native-multi-section',sectionsFile,{markers:['NATIVE_FOOT_BODY','NATIVE_END_BODY'],verifyImported:(text,current)=>check('Native varying positions become two readable source scopes with current body/reference identities and a preserved current heading link',assertSafeMultiSectionImport(text,current)),verifyOutput:assertNativeMultiSection});
    const chosen=await exportDocx('explicit-preset',source,{preset:PRESET});assertSourceNative(chosen.value,{overrideStart:23,overrideSize:12});
    const overridden=await exportDocx('explicit-options',source,{preset:PRESET,options:{footnotes:{start:31,size:13}}});assertSourceNative(overridden.value,{overrideStart:31,overrideSize:13});
    const inherited=await exportDocx('null-inherits-source',source,{options:{footnotes:{start:null,size:null}}});assertSourceNative(inherited.value);
    check('Implicit preset cannot overwrite source; explicitly selected preset and API options win; null inherits without source mutation');
    for(const points of [10.5,11,14]){
      const label='global-body-'+String(points).replace('.','_'),native=await exportDocx(label,source,{options:{typography:{size:points}}});assertNativeGlobalSize(native.value,points);
      await importAndSave(label,native.file,{markers:['SOURCE_FOOT_BODY','SOURCE_END_BODY'],verifyImported:(text,current)=>{assert.equal(current.sourceValues.typography.size,points);assertSourceCaptionIdentity(text);assert(text.includes('[@work, p. 12]'));},verifyOutput:value=>{assertSourceNative(value);assertNativeGlobalSize(value,points);}});
    }
    check('Actual shared 10.5/11/14pt body, REF and independent caption CS survive import and explicit save; reopened exports must omit old options');
    const plain='# SETTINGS_PLAIN\n\nNo notes, automatic numbering or document settings.\n';const plainDoc=await exportDocx('plain-original',plain);
    await importAndSave('plain-no-pollution',plainDoc.file,{verifyImported:(text,current)=>{assert.deepEqual(current.sourceValues,{version:1,typography:{size:10.5}});assert(!text.includes('readmd-numbering:'),'Unused numbering metadata polluted plain import');assert(!Object.hasOwn(current.sourceValues,'footnotes'),'Unused note settings polluted plain import');},verifyOutput:value=>{assert.equal(value.notes.footnote.references.length+value.notes.endnote.references.length,0);assert(value.paragraphs.every(row=>!row.rule));}});
    sectionState=await sectionNoteCases.prepare(sectionContext);
    assert.equal(fileHash(presetFile),presetHash,'Source inheritance polluted owned user preset or Last');
    const first={passed:true};await client.stop(first);report.sessions.push(first);assert.equal(first.passed,true);assert.equal(first.session.hostExit.code,0);assert.equal(first.session.forcedStop,false);client=null;
    await start('wire-reopened');assert.notEqual(client.info.pid,first.session.pid,'Reopen must use another process');
    for(const row of queue){
      const chunks=[];for(const args of sourceReadChunks(row.file,row.markdown)){const chunk=payload(await call('readmd_read_document',args));assert.equal(chunk.revision,row.sha256);chunks.push(chunk.content);}const reopened={content:chunks.join(''),revision:row.sha256};const expected=row.markdown.startsWith('\uFEFF')?row.markdown.slice(1):row.markdown;assert.equal(reopened.content,expected);assert.equal(fileHash(row.file),row.sha256);row.reopenedMarkdown=reopened.content;
      const output=await exportDocx('reopened-'+row.label,reopened.content);assert.deepEqual(output.args.options,{});assert(!Object.hasOwn(output.args,'style_preset'));if(row.verifyOutput)await row.verifyOutput(output.value,output);
      const evidenceRow=report.roundtrips.find(value=>value.label===row.label);evidenceRow.reopenedInNewProcess=true;evidenceRow.secondExportWithoutOldOptions=true;evidenceRow.secondOutput=path.relative(evidence,output.file);evidenceRow.nativeVerified=true;check(row.label+': saved bytes reopen in a different process and re-export native settings without prior options');
    }
    await sectionNoteCases.reopened(sectionContext,sectionState);
    assert.equal(fileHash(presetFile),presetHash,'Restarted source export polluted presets/Last');assert.equal(fileHash(path.join(documents,'authored-input.md')),hash(source));check('Owned preset/Last and authored input remain byte-identical across both real sessions');report.passed=true;
  }catch(error){report.error=String(error.stack||error);}
  finally{
    clearTimeout(timer);if(client){const session={passed:true};await client.stop(session);report.sessions.push(session);if(!session.passed)report.passed=false;}
    try{verifyBinding(binding);assert.equal(fileHash(options.python),report.python.sha256,'QA Python changed');report.finalCandidateBindingVerified=true;}catch(error){report.passed=false;report.bindingError=String(error.stack||error);}
    report.finishedUtc=new Date().toISOString();retain('report.json',report);
  }
  console.log(JSON.stringify({evidence:path.relative(ROOT,evidence),passed:report.passed,checks:report.checks.length,outputs:report.outputs.length,sessions:report.sessions.length,error:report.error,bindingError:report.bindingError}));if(!report.passed)process.exitCode=1;
}
if(require.main===module)main().catch(error=>{console.error(error.stack||error);process.exitCode=1;});
module.exports={ROOT,BASE,QA_FILES,PRODUCT_FILES,PROFILE,NUMBERING,PRESET,PRESET_DATA,NATIVE_POSITIVES,argumentsFor,inside,planFor,bindingFor,plainText,toolValue,sourceReadArguments,sourceFor,noteStyle,applyEdits,assertProfileTransaction,assertSavedEdit,kindProperties,normalNote,assertNote,assertSourceNative,assertNativeFixture,assertNativeMultiSection,assertMultiSectionImport,assertSafeMultiSectionFixture,assertSafeMultiSectionImport,assertLegacyMultiSectionReferenceRejection,noteLabel,assertProfileMatchesNotes,assertConversionError,assertMutation,numberingImportDiagnostics,assertHeadingReferenceOpen,assertCaptionConflict,assertSourceCaptionIdentity,assertNumberingCleared,assertNumberingClearedImport,assertNotesCleared,assertNotesClearedProfile,markdownStructure,markdownTokens,inlineStructure,controlledSpanTag,assertUniformNormalFootnoteRuns,assertStyleOnlyMixedNoteFixture,assertStyleOnlyMixedNoteImport,assertChangedGeneratedNote};
module.exports.assertNativeGlobalSize=assertNativeGlobalSize;
module.exports.SECTION_BASE=SECTION_BASE;
module.exports.sourceReadChunks=sourceReadChunks;
