'use strict';
const {test}=require('node:test'),assert=require('node:assert/strict'),path=require('node:path');
const lane=require('./batch3-math-native-defaults-stdio.cjs');

test('explicit-default lane plans without processes and never certifies whole batch/application interoperability',()=>{
 const plan=lane.planFor();assert(plan.noProcessesStarted&&plan.noFilesWritten);assert.equal(plan.guiStarted,false);
 for(const field of ['fullBatch3Certification','wordApplicationCertified','allOfficeGlobalPropertiesCertified'])assert.equal(plan[field],false);
 assert.deepEqual(plan.nonFourAxes,['rFonts','lang','szCs']);assert.equal(plan.cases.length,6);
 assert.deepEqual(plan.cases.filter(row=>row.native).map(row=>row.label),['original-a','original-b','inherited','no-character']);
 assert.deepEqual(plan.cases.filter(row=>!row.native).map(row=>row.label),['lang-mismatch','szcs-mismatch']);
 assert(lane.argumentsFor(['--plan'],{}).plan);
 for(const args of [['--plan','--gui'],['--candidate','x'],['--plan','--output',path.join(lane.BASE,'../unowned')]])assert.throws(()=>lane.argumentsFor(args,{}));
});

test('native-default presets use a supported actual target font and preserve unrelated user configuration',()=>{
 const original={custom:{PERSONAL:{typography:{font:'Keep font'},meta:{title:'Keep title'}}},last:{format:'docx',style:'PERSONAL'}},out=lane.presetOverlay(JSON.stringify(original));
 assert.deepEqual(out.custom.PERSONAL,original.custom.PERSONAL);assert.deepEqual(out.last,original.last);
 const keys=[];for(const [font,engines]of Object.entries(lane.PRESETS))for(const [engine,key]of Object.entries(engines)){keys.push(key);assert.equal(out.custom[key].typography.font,font);assert.equal(out.custom[key].pdf.engine,engine);assert(!Object.hasOwn(original.custom,key));}
 assert.equal(new Set(keys).size,2);assert.throws(()=>lane.presetOverlay(JSON.stringify(out)),/collision/);
});

test('positive inheritance retains the actual Arial body defaults instead of trusting an unsanitized STIX preset echo',()=>{
 const plan=lane.planFor();assert.deepEqual(Object.keys(lane.PRESETS),['Arial']);
 assert(plan.cases.every(row=>row.font==='Arial'));
 assert.equal(plan.fixtureAuthorFontChoice.globalAction,'retain-actual-global');
 assert.equal(plan.fixtureAuthorFontChoice.globalFont,'Arial');
 assert.equal(plan.fixtureAuthorFontChoice.otherFormulasDirectFont,'STIX Two Math');
 assert.equal(plan.fixtureAuthorFontChoice.presetCatalogueEchoProvesConsumerSupport,false);
 assert.deepEqual(plan.cases.filter(row=>row.native).map(row=>row.label),['original-a','original-b','inherited','no-character']);
 for(const format of ['docx','pdf'])assert.throws(()=>lane.requestFor('body',format,'rust','STIX Two Math','documents','output.'+format));
});

test('default-native request uses saved actual preset and empty options without format coercion',()=>{
 for(const font of Object.keys(lane.PRESETS))for(const engine of ['rust','typst'])for(const format of ['docx','pdf']){
  const request=lane.requestFor('body',format,engine,font,'documents','output.'+format);
  assert.equal(request.name,'readmd_export_document');assert.equal(request.arguments.markdown_content,'body');assert.equal(request.arguments.output_format,format);
  assert.equal(request.arguments.style_preset,lane.PRESETS[font][engine]);assert.deepEqual(request.arguments.options,{});assert.equal(request.arguments.confirm,true);
 }
 assert.throws(()=>lane.requestFor('body','epub','rust','Arial','documents','x.epub'));
 assert.throws(()=>lane.requestFor('body','pdf','rust','unknown','documents','x.pdf'));
});
