'use strict';
// DOM-event unit contracts only. No native/CSP/backend/undo acceptance claim.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const root=path.resolve(process.env.READMD_QA_REPO_ROOT||path.join(__dirname,'../..'));
const source=fs.readFileSync(process.env.READMD_MSPACE_PROPERTIES_SOURCE||path.join(root,'assets/js/editor/formula-properties.js'),'utf8');
const NS='http://www.w3.org/1998/Math/MathML',OFFICE='http://schemas.openxmlformats.org/officeDocument/2006/math';
const node=(id,name,namespace,attributes={},children=[])=>({id,name,namespace,attributes,attribute_namespaces:{},content:children.map(value=>({type:'element',value}))});
const clone=value=>JSON.parse(JSON.stringify(value));
class Element{
  constructor(tag,document){this.tagName=tag.toUpperCase();this.document=document;this.children=[];this.dataset={};this.attributes={};this.className='';this.listeners={};this.open=false;this.scrollTop=0;this.textContent='';}
  get value(){return this._value??(this.tagName==='SELECT'?this.children[0]?.value:'')??'';}set value(value){this._value=String(value);}
  append(...nodes){for(const n of nodes){n.parentElement=this;this.children.push(n);}}setAttribute(k,v){this.attributes[k]=String(v);}
  replaceChildren(...nodes){for(const n of this.children)n.parentElement=null;this.children=[];this.append(...nodes);}
  replaceWith(node){const parent=this.parentElement,index=parent.children.indexOf(this);parent.children[index]=node;node.parentElement=parent;this.parentElement=null;}
  contains(node){for(let at=node;at;at=at.parentElement)if(at===this)return true;return false;}
  focus(){this.document.activeElement=this;}addEventListener(k,fn){(this.listeners[k]||=[]).push(fn);}async emit(k){for(const fn of this.listeners[k]||[])await fn({target:this});}
  querySelectorAll(selector){const matches=n=>{if(selector.startsWith('.'))return n.className.split(' ').includes(selector.slice(1));if(selector.startsWith('[data-')){const m=/^\[data-([\w-]+)(?:="([^"]*)")?\](\[open\])?$/u.exec(selector);if(!m)return false;const key=m[1].replace(/-([a-z])/gu,(_,c)=>c.toUpperCase());return Object.hasOwn(n.dataset,key)&&(m[2]===undefined||n.dataset[key]===m[2])&&(!m[3]||n.open);}return n.tagName===selector.toUpperCase();};const result=[];function walk(n){for(const c of n.children){if(matches(c))result.push(c);walk(c);}}walk(this);return result;}
  querySelector(selector){return this.querySelectorAll(selector)[0]||null;}
}
async function environment(nodes,change){
  const document={activeElement:null,createElement(tag){return new Element(tag,document);}},calls=[],requests=[];
  const schema={coreObjects:[{element:'f'}],elements:[{element:'f',children:[]}],enumerations:[]};
  const context={window:{},document,i18n:{t:key=>'translated:'+key},apiMessage:(error,fallback)=>'localized:'+(error.error_code||fallback),apiFetch:async route=>{requests.push(route);assert.equal(route,'/api/math/schema');return{ok:true,json:async()=>({schema})};}};
  vm.createContext(context);vm.runInContext(source,context);
  const d={host:new Element('span',document),message:new Element('span',document),rich:{root:node('root','m:oMath',OFFICE,{},nodes)}};
  let activations=0;const ui=await context.window.ReadMDMathProperties.mount(d,async()=>{activations++;},async operation=>{calls.push(clone(operation));if(change)await change(operation,d);else{const next=clone(d.rich),target=context.window.ReadMDMathProperties.flatten(next.root).find(n=>n.id===operation.node_id);if(operation.value===null)delete target.attributes[operation.name];else target.attributes[operation.name]=operation.value;d.rich=next;}});
  ui.details.open=true;await ui.details.emit('toggle');
  const fields=()=>d.host.querySelectorAll('[data-math-space]').map(row=>({name:row.dataset.mathSpace,input:row.querySelector('input'),label:row.children[0]}));
  const control=name=>fields().find(f=>f.name===name)?.input;
  const select=()=>d.host.querySelector('select');
  return{d,ui,calls,requests,context,fields,control,select,activations:()=>activations};
}
const space=(id='s1',attributes={width:' 1.25EM ',height:'2Pt',depth:'0pX'})=>node(id,'mm:mspace',NS,attributes);
test('strict MathML mspace exposes three accessible text controls without admitting foreign XML',async()=>{
  const e=await environment([space(),node('foreign','x:mspace','urn:other',{width:'9em'}),node('fake','m:mspace',OFFICE,{width:'9em'})]);
  assert.equal(e.select().value,'s1');assert.deepEqual(e.fields().map(f=>f.name),['width','height','depth']);
  assert.deepEqual(e.select().children.map(n=>n.value),['s1']);
  for(const field of e.fields()){assert.equal(field.input.type,'text');assert.equal(field.input.maxLength,128);assert.match(field.input.attributes['aria-label'],/^translated:formula\.mathml\.mspace translated:/u);}
  assert.deepEqual(e.fields().map(f=>f.label.textContent),['translated:img.width','translated:img.height','translated:formula.mathml.depth']);
  assert.equal(e.calls.length,0);assert.deepEqual(e.requests,['/api/math/schema']);
});
test('opening advanced properties does not materialize omitted dimensions or replace explicit zero',async()=>{
  for(const attributes of [{},{width:'0',depth:'0PX'}]){const e=await environment([space('s',attributes)]);assert.equal(e.control('height').value,'');assert.equal(e.control('width').value,attributes.width||'');assert.equal(e.control('depth').value,attributes.depth||'');assert.equal(e.calls.length,0);assert.deepEqual(e.d.rich.root.content[0].value.attributes,attributes);}
});
test('each change sends a source-owned set_attribute and preserves lexical units without client parsing',async()=>{
  const e=await environment([space()]);e.control('width').focus();e.control('width').value=' 16PX ';await e.control('width').onchange();
  assert.deepEqual(e.calls,[{operation:'set_attribute',node_id:'s1',name:'width',value:' 16PX '}]);
  assert.equal(e.control('width').value,' 16PX ');assert.equal(e.control('height').value,'2Pt');assert.equal(e.control('depth').value,'0pX');assert.equal(e.context.document.activeElement.dataset.mathFocus,'mspace.width');
});
test('clearing one dimension removes that attribute while explicit 0 remains authored',async()=>{
  const e=await environment([space()]);e.control('height').value='';await e.control('height').onchange();e.control('depth').value='0';await e.control('depth').onchange();
  assert.deepEqual(e.calls.map(p=>[p.name,p.value]),[['height',null],['depth','0']]);const value=e.d.rich.root.content[0].value.attributes;
  assert(!Object.hasOwn(value,'height'));assert.equal(value.depth,'0');assert.equal(value.width,' 1.25EM ');
});
test('identical spacing nodes use the selected identity and do not edit the first occurrence',async()=>{
  const e=await environment([space('first'),space('second')]);e.select().value='second';e.select().onchange();e.control('width').value='2Em';await e.control('width').onchange();
  assert.equal(e.calls[0].node_id,'second');assert.equal(e.d.rich.root.content[0].value.attributes.width,' 1.25EM ');assert.equal(e.d.rich.root.content[1].value.attributes.width,'2Em');assert.equal(e.select().value,'second');
});
test('rejected units stay visible and localized until a valid change refreshes the control',async()=>{
  const e=await environment([space()],async(op,d)=>{if(op.value==='-1em'||op.value==='50%')throw Error('math_unsupported_extension');const next=clone(d.rich);next.root.content[0].value.attributes[op.name]=op.value;d.rich=next;});
  for(const value of ['-1em','50%']){e.control('width').value=value;await e.control('width').onchange();assert.equal(e.control('width').value,value);assert.equal(e.control('width').attributes['aria-invalid'],'true');assert.equal(e.d.message.textContent,'localized:math_unsupported_extension');assert.equal(e.d.rich.root.content[0].value.attributes.width,' 1.25EM ');}
  e.control('width').value='.50Em';await e.control('width').onchange();assert.equal(e.control('width').value,'.50Em');assert(!Object.hasOwn(e.control('width').attributes,'aria-invalid'));
});
test('a whitespace-only value reaches the shared validator instead of becoming an omitted attribute',async()=>{
  const e=await environment([space()],async()=>{throw Error('math_invalid_property');});e.control('width').value=' ';await e.control('width').onchange();assert.equal(e.calls[0].value,' ');assert.equal(e.d.message.textContent,'localized:math_invalid_property');
});
test('fraction selection and Office-only behavior remain available around spacing descendants',async()=>{
  const fraction=node('fraction','m:f',OFFICE,{},[space()]);const e=await environment([fraction]);assert.equal(e.select().value,'fraction');assert.equal(e.fields().length,0);e.select().value='s1';e.select().onchange();assert.equal(e.fields().length,3);
  assert.equal(e.context.window.ReadMDMathProperties.isMath(space()),false);assert.equal(e.context.window.ReadMDMathProperties.isSpacing(fraction),false);
});
