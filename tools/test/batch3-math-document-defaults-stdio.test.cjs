'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),path=require('node:path');
const qa=require('./batch3-math-document-defaults-stdio.cjs');
test('document-defaults plan keeps native/application certification separate',()=>{
  const plan=qa.plan();assert.equal(plan.nativeInputs,1);assert.equal(plan.noGui,true);assert.equal(plan.candidateRequired,true);
  for(const key of ['fullBatch3Certified','wordApplicationCertified','readerApplicationCertified'])assert.equal(plan[key],false);
  assert.deepEqual(plan.sourceOrders,['ordinary-first','native-first']);assert.equal(plan.ordinaryOccurrences,4);assert.equal(plan.nativeOccurrences,3);
  assert(plan.positiveOutputLimit<=16);assert.equal(plan.limits.workflowMs,900000);
});
test('context selectors preserve exact source range and never use expression text as identity',()=>{
  const context={id:'one',revision:'revision-a',fingerprint:'policy-a'};
  const a={from:3,to:8,source:'x^2',dialect:'latex',display:false};
  const b={...a,from:24,to:29};
  const one=qa.parseRequest(context,a),two=qa.parseRequest(context,b);
  assert.equal(one.source,two.source);assert.notDeepEqual(one.document_context,two.document_context);
  assert.deepEqual(one.document_context,{...context,from:3,to:8});assert(!Object.hasOwn(one,'metadata'));
  const changed=qa.parseRequest({...context,revision:'revision-b',fingerprint:'policy-b'},a);
  assert.notDeepEqual(changed.document_context,one.document_context);
});
test('context requests retain metadata, mode and font metric contract',()=>{
  const request=qa.parseRequest({id:'i',revision:'r',fingerprint:'f'},{from:4,to:40,source:'x',dialect:'dsl',display:true,metadata:'carrier'});
  assert.equal(request.metadata,'carrier');assert.equal(request.font_size,12);assert.equal(request.layout_width,300);assert.equal(request.dialect,'dsl');
});
test('native lane uses owned high-density CRLF material with duplicate formulas and astral source',()=>{
  assert(qa.PLAIN.includes('😀'));assert(!/(^|[^\r])\n/.test(qa.PLAIN));assert.equal(qa.PLAIN.match(/\$x\^2\$/g).length,2);
  assert.equal(qa.PLAIN.match(/\$\$\\frac\{x\}\{y\}\$\$/g).length,2);assert(qa.PLAIN.includes('PLAIN_CONTROL'));
  const args=qa.args(['--plan','--output','owned']);assert.equal(args.plan,true);assert.equal(args.output,path.resolve('owned'));
  assert.throws(()=>qa.args(['--unknown','value']));
});

const ink='<svg xmlns="http://www.w3.org/2000/svg"><g data-math-ink="true"><path d="M0 0L1 1"/></g></svg>';
function htmlCase(nativeFirst=false){
  const ordinary=[false,false,true,true].map(display=>({dialect:'latex',display})),native=Array.from({length:3},()=>({dialect:'dsl',display:true}));
  const previews=(nativeFirst?[...native,...ordinary]:[...ordinary,...native]).map((occurrence,ordinal)=>({occurrence:{...occurrence,ordinal}}));
  const source=previews.map(({occurrence:o})=>{const ordinary=o.dialect==='latex',tag=o.display?'div':'span',label=ordinary?(o.display?'\\frac{x}{y}':'{x}^{2}'):'\\frac{a}{b}';return '<'+tag+' class="readmd-export-math'+(o.display?' readmd-export-math-block':'')+'"'+(!ordinary?' style="text-align:left"':'')+' role="math" aria-label="'+label+'">'+ink+'</'+tag+'>';}).join('\r\n');
  return {previews,source};
}
test('HTML selected-source vectors require the complete ordered mixed corpus in both orders',()=>{
  for(const nativeFirst of [false,true]){const c=htmlCase(nativeFirst),rows=qa.htmlVectorCases(c.source,c.previews);assert.deepEqual(rows.map(row=>row.ordinal),[0,1,2,3,4,5,6]);assert(rows.every(row=>row.svg===ink));}
});
test('HTML checks reject wrappers with omitted or empty SVG even when seven labels remain',()=>{
  const c=htmlCase();for(const replacement of ['','<svg xmlns="http://www.w3.org/2000/svg"></svg>'])assert.throws(()=>qa.htmlVectorCases(c.source.replace(ink,replacement),c.previews));
});
test('HTML checks reject a hidden wrapper or changed source label',()=>{
  const c=htmlCase();for(const damaged of [c.source.replace(' role="math"',' hidden role="math"'),c.source.replace(' role="math"',' style="opacity:0" role="math"'),c.source.replace('{x}^{2}','z^2')])assert.throws(()=>qa.htmlVectorCases(damaged,c.previews));
});
test('HTML checks reject reordered dialects, missing wrappers and extra SVG copies',()=>{
  const c=htmlCase();assert.throws(()=>qa.htmlVectorCases(c.source,htmlCase(true).previews));assert.throws(()=>qa.htmlVectorCases(c.source.replace(/<span[\s\S]*?<\/span>/,''),c.previews));assert.throws(()=>qa.htmlVectorCases(c.source+ink,c.previews));
});
test('HTML extracted ink remains byte-exact for the independent path/style comparison',()=>{
  const c=htmlCase();const painted=c.source.replace('d="M0 0L1 1"','opacity="0" d="M0 0L1 1"');const rows=qa.htmlVectorCases(painted,c.previews);assert(rows[0].svg.includes('opacity="0"'));assert.notEqual(rows[0].svg,ink);
});
