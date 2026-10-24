import {test} from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

// Exercise the production async clipboard handler without starting a browser.
const source=fs.readFileSync('assets/js/editor/formula-studio.js','utf8');
const start=source.indexOf('  async function pasteExternalFormula(event) {');
const end=source.indexOf('  function apply() {',start);
assert(start>=0&&end>start);
function fixture(text,{before='',selected=true,response,fetch}={}){
  const field={value:before,selectionStart:selected?0:1,selectionEnd:selected?before.length:2,setSelectionRange(a,b){this.selectionStart=a;this.selectionEnd=b;}};
  const elements={'formula-source':field,'formula-apply':{disabled:false},'formula-preview':{textContent:'',replaceChildren(){this.textContent='';}},'formula-error':{textContent:''},'formula-mode':{value:'block'}};
  const calls=[],context={window:{},draft:{dialect:'dsl',tex:before,valid:true},renderEpoch:0,renders:0,
    byId:id=>elements[id],tr:key=>'localized:'+key,
    apiFetch:async(url,options)=>{calls.push({url,body:JSON.parse(options.body)});return fetch?await fetch():{ok:response?.ok??true,json:async()=>response?.body??{ok:true,output:'function(sin, x)'}};}};
  context.renderDraft=async()=>{context.renders++;};vm.createContext(context);
  vm.runInContext(source.slice(start,end)+'\nwindow.paste=pasteExternalFormula;',context);
  const event={prevented:false,clipboardData:{getData:type=>type==='text/plain'?text:''},preventDefault(){this.prevented=true;}};
  return {context,elements,field,calls,event,run:()=>context.window.paste(event)};
}
test('explicit UnicodeMath function clipboard input reaches the shared compiler and editable DSL',async()=>{
  for(const text of ['sin⁡(x+1)','sin\\funcapply^2 x','𝛼+sin⁡(x+1)+sin⁡(x+1)']){
    const f=fixture(text);await f.run();assert.equal(f.event.prevented,true,text);assert.equal(f.calls.length,1);
    assert.deepEqual(f.calls[0],{url:'/api/math/serialize',body:{source:text,dialect:'unicodemath',target:'dsl',display:true}});
    assert.equal(f.context.draft.dialect,'dsl');assert.equal(f.context.draft.tex,'function(sin, x)');assert.equal(f.field.value,'function(sin, x)');assert.equal(f.context.renders,1);
  }
});
test('ordinary prose, unrelated macros and partial selections keep native clipboard behavior',async()=>{
  for(const text of ['sin x','sin\\funcapplyExtra x','plain words']){
    const f=fixture(text);await f.run();assert.equal(f.event.prevented,false);assert.equal(f.calls.length,0);
  }
  const f=fixture('sin⁡x',{before:'frac(a, b)',selected:false});await f.run();assert.equal(f.event.prevented,false);assert.equal(f.calls.length,0);assert.equal(f.field.value,'frac(a, b)');
});
test('compiler rejection retains the pasted draft and localized error without applying content',async()=>{
  const f=fixture('sin⁡()',{response:{ok:false,body:{ok:false,error_code:'math_missing_argument'}}});await f.run();
  assert.equal(f.event.prevented,true);assert.equal(f.field.value,'sin⁡()');assert.equal(f.context.draft.tex,'sin⁡()');assert.equal(f.context.draft.valid,false);assert.equal(f.context.renders,0);assert.equal(f.elements['formula-error'].textContent,'localized:formula.invalid');assert.equal(f.elements['formula-apply'].disabled,true);
});
test('late clipboard compilation cannot overwrite typing, a replacement draft or a closed formula',async()=>{
  for(const change of ['typing','replacement','closed']){
    let resolve;const pending=new Promise(done=>{resolve=done;});const f=fixture('sin⁡x',{fetch:()=>pending});const original=f.context.draft,run=f.run();
    assert.equal(f.calls.length,1);
    if(change==='typing'){f.field.value='new text';original.tex='new text';}
    if(change==='replacement')f.context.draft={dialect:'dsl',tex:'new formula',valid:false};
    if(change==='closed')f.context.draft=null;
    resolve({ok:true,json:async()=>({ok:true,output:'stale result'})});await run;
    assert.notEqual(f.field.value,'stale result');assert.equal(f.context.renders,0);
    if(change==='typing')assert.equal(original.tex,'new text');
    if(change==='replacement')assert.equal(f.context.draft.tex,'new formula');
    if(change==='closed')assert.equal(f.context.draft,null);
  }
});
