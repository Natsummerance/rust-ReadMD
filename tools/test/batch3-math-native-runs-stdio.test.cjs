'use strict';
const {test}=require('node:test'),assert=require('node:assert/strict'),path=require('node:path');
const lane=require('./batch3-math-native-runs-stdio.cjs');
const W='http://schemas.openxmlformats.org/wordprocessingml/2006/main';

test('native run lane keeps process-free planning and explicit bounded evidence destinations',()=>{
  const plan=lane.planFor();assert.equal(plan.noProcessesStarted,true);assert.equal(plan.guiStarted,false);assert.equal(plan.externalApplicationRoundtripCertified,false);assert.equal(plan.fullBatch3Certification,false);
  assert.deepEqual(plan.axes,['b','i','sz','color']);assert.equal(plan.nativeInputFormulas,3);assert.equal(plan.runSitesPerFormula,3);assert.deepEqual(plan.fiveOutputs,['docx','html','epub','rust-pdf','typst-pdf']);
  assert(lane.argumentsFor(['--plan'],{}).plan);assert.throws(()=>lane.argumentsFor(['--plan','--output',path.join(lane.BASE,'../unowned')],{}),/outside/);
  assert.throws(()=>lane.argumentsFor(['--candidate','x'],{}),/Required/);assert.throws(()=>lane.argumentsFor(['--plan','--gui'],{}),/Unknown/);
});

test('run fallback warnings are exact, deduplicated typed envelopes',()=>{
  assert(lane.assertWarning({warnings:[],warn_items:[]},false));
  assert(lane.assertWarning({warnings:[lane.MATERIALIZED],warn_items:[{code:lane.MATERIALIZED}]},true));
  assert.throws(()=>lane.assertWarning({warnings:[lane.MATERIALIZED+'_lookalike'],warn_items:[{code:lane.MATERIALIZED+'_lookalike'}]},true));
  assert.throws(()=>lane.assertWarning({warnings:[lane.MATERIALIZED,lane.MATERIALIZED],warn_items:[{code:lane.MATERIALIZED}]},true));
});

test('current run patch controls reject collateral declaration and operand changes',()=>{
  const direct={id:'b',namespace:W,name:'w:b',attributes:{'w:val':'off'},attribute_namespaces:{'w:val':W},content:[]};
  const before={source:'owned source',revision:lane.sha('owned source'),root:{namespace:'',name:'root',attributes:{},content:[{type:'element',value:direct},{type:'text',value:'a'}]}};
  const after=structuredClone(before);after.root.content[0].value.attributes['w:val']='on';after.source='owned changed source';after.revision=lane.sha(after.source);
  assert(lane.assertOnlyB(before,after,{node:direct,key:'w:val'}));
  const bad=structuredClone(after);bad.root.content[1].value='b';assert.throws(()=>lane.assertOnlyB(before,bad,{node:direct,key:'w:val'}));
  const stale=structuredClone(after);stale.revision=before.revision;assert.throws(()=>lane.assertOnlyB(before,stale,{node:direct,key:'w:val'}));
});

test('HTML selected branch requires exactly three real vector wrappers',()=>{
  const svg='<svg xmlns="http://www.w3.org/2000/svg"><g data-math-ink="true"><path d="M 1 2"/></g></svg>',wrapper='<div class="readmd-export-math readmd-export-math-block" role="math" aria-label="\\frac{a}{b}">'+svg+'</div>',valid=wrapper.repeat(3);
  assert.deepEqual(lane.htmlVectors(valid),[svg,svg,svg]);
  for(const bad of [wrapper.repeat(2),valid+svg,valid.replace(svg,'<img src="placeholder"/>'),valid.replace('<div','<span'),valid.replace(' role="math"',' style="display:none" role="math"'),valid.replace('role="math"','role="presentation"'),valid.replace('aria-label="\\frac{a}{b}"','aria-label="other"')])assert.throws(()=>lane.htmlVectors(bad));
});

test('owned preset overlay preserves unrelated settings and distinguishes actual engines',()=>{
  const raw=JSON.stringify({custom:{MY_STYLE:{meta:{title:'keep me'}}},last:{format:'docx',style:'MY_STYLE'}}),original=JSON.parse(raw),overlay=lane.presetOverlay(raw);
  assert.deepEqual(overlay.custom.MY_STYLE,original.custom.MY_STYLE);assert.deepEqual(overlay.last,original.last);
  for(const [engine,key]of Object.entries(lane.PRESETS)){assert.equal(overlay.custom[key].pdf.engine,engine);assert(!Object.hasOwn(original.custom,key));}
  assert.throws(()=>lane.presetOverlay(JSON.stringify(overlay)),/assert|collision/i);
  for(const format of ['docx','html','pdf','epub']){const request=lane.exportRequestFor('content',format,'typst','documents','output.'+format);assert.deepEqual(request.arguments.options,{});assert.equal(request.arguments.confirm,true);if(format!=='epub')assert.equal(request.arguments.style_preset,lane.PRESETS.typst);}
});

test('malformed ancestry cases are standalone sources and do not depend on serializer layout',()=>{
  const cases=lane.negativeCases();assert.deepEqual(cases.map(row=>row.variant),['wrong-type','missing-parent','cycle','duplicate-id']);assert.equal(new Set(cases.map(row=>row.source)).size,4);
  for(const row of cases){assert(row.source.includes('word.rStyle(val: "Accent")'));assert(!row.source.includes('readmd:math:v1'));}
});
