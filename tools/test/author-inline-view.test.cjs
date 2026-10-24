'use strict';
// DOM interaction tests only; production layout is checked by the native lanes.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const nonce='a'.repeat(32);
class Style{
  constructor(){this.values=new Map();}getPropertyValue(k){return this.values.get(k)?.[0]||'';}getPropertyPriority(k){return this.values.get(k)?.[1]||'';}
  setProperty(k,v,p=''){this.values.set(k,[v,p]);}removeProperty(k){this.values.delete(k);}
}
class Element{
  constructor(tag='span'){this.tagName=tag.toUpperCase();this.children=[];this.dataset={};this.style=new Style();this.attributes={};this.fontSize='16px';this.textContent='';}
  append(...nodes){for(const n of nodes){n.parent=this;this.children.push(n);}}remove(){if(this.parent)this.parent.children=this.parent.children.filter(n=>n!==this);}
  setAttribute(k,v){this.attributes[k]=v;}get firstElementChild(){return this.children[0];}
  contains(n){return n===this||this.children.some(c=>c.contains(n));}closest(){return this.block||this.parent?.closest();}
  querySelectorAll(){return this.children.flatMap(child=>[...(Object.hasOwn(child.dataset,'readmdAuthorRun')||Object.hasOwn(child.dataset,'readmdAuthorActive')?[child]:[]),...child.querySelectorAll()]);}
  reportValidity(){return this.valid!==false;}
}
function reader(){const context=vm.createContext({window:{},getComputedStyle:node=>({fontSize:node.fontSize})});vm.runInContext(fs.readFileSync('assets/js/reader/author-text.js','utf8'),context);return context.window.ReadMDAuthorText;}
function run(index=0,more={}){return{key:nonce+'-'+index,size:30,base:null,complex:false,blockBase:false,...more};}
function surface(){const root=new Element('article'),paragraph=new Element('p'),span=new Element('span');paragraph.block=paragraph;span.block=paragraph;span.dataset.readmdAuthorRun=nonce+'-0';root.append(paragraph);paragraph.append(span);return{root,paragraph,span};}
test('trusted sizes restore original span and inline code values including priority',()=>{
  const api=reader(),{root,span}=surface(),code=new Element('code');span.style.setProperty('font-size','17px','important');code.style.setProperty('font-size','.92em','important');span.append(code);
  api.apply(root,{nonce,runs:[run()]});assert.equal(span.style.getPropertyValue('font-size'),'30pt');assert.equal(code.style.getPropertyValue('font-size'),'inherit');
  api.apply(root,null);assert.equal(span.style.getPropertyValue('font-size'),'17px');assert.equal(span.style.getPropertyPriority('font-size'),'important');assert.equal(code.style.getPropertyValue('font-size'),'.92em');assert.equal(code.style.getPropertyPriority('font-size'),'important');
});
test('block reset recomputes actual paragraph size and never borrows an external surface',()=>{
  const api=reader(),{root,span,paragraph}=surface(),model={nonce,runs:[run(0,{size:null,blockBase:true})]};paragraph.fontSize='32px';api.apply(root,model);assert.equal(span.style.getPropertyValue('font-size'),'32px');paragraph.fontSize='24px';api.apply(root,model);assert.equal(span.style.getPropertyValue('font-size'),'24px');
  span.block=new Element('p');api.apply(root,model);assert.equal(span.style.getPropertyValue('font-size'),'');
});
test('invalid values, forged keys and duplicated live keys cannot become CSS',()=>{
  for(const bad of [run(0,{size:'30;position:fixed'}),run(0,{size:Infinity}),run(0,{size:-1}),run(0,{base:1e10}),run(0,{key:'b'.repeat(32)+'-0'})]){
    const api=reader(),{root,span}=surface();api.apply(root,{nonce,runs:[bad]});assert.equal(span.style.getPropertyValue('font-size'),'');
  }
  const api=reader(),{root,paragraph,span}=surface(),duplicate=new Element('span');duplicate.dataset.readmdAuthorRun=span.dataset.readmdAuthorRun;paragraph.append(duplicate);api.apply(root,{nonce,runs:[run()]});assert.equal(span.style.getPropertyValue('font-size'),'');assert.equal(duplicate.style.getPropertyValue('font-size'),'');
});
test('removing a marker or invalidating the model restores sparse surviving nodes',()=>{
  const api=reader(),{root,span}=surface();api.apply(root,{nonce,runs:[run()]});delete span.dataset.readmdAuthorRun;api.apply(root,null);assert.equal(span.style.getPropertyValue('font-size'),'');assert(!Object.hasOwn(span.dataset,'readmdAuthorActive'));
});
function inlineEditor({selected=true}={}){
  const parent=new Element('div'),requests=[],dispatched=[],controller=new AbortController();let current=true,resolve;
  const source='Original text',view={state:{selection:{main:{from:selected?0:4,to:selected?8:4}}},dispatch:edit=>dispatched.push(edit)};
  const context=vm.createContext({window:{},document:{createElement:tag=>new Element(tag)},apiFetch:async(route,init)=>{const body=JSON.parse(init.body);requests.push({route,body});if(body.action==='get')return{ok:true,json:async()=>({ok:true,revision:'r',nodes:[]})};return new Promise(done=>{resolve=()=>done({ok:true,json:async()=>({ok:true,transaction:{edits:[{from:0,to:8,insert:'safe'}]},result:{changed:true}})});});}});
  vm.runInContext(fs.readFileSync('assets/js/editor/publication-inline-text.js','utf8'),context);
  let closed=0;context.window.ReadMDPublicationInlineText.mount(parent,{source,view,signal:controller.signal,isCurrent:()=>current,onApplied:()=>closed++,t:key=>key});
  const details=parent.children[0];return{requests,dispatched,controller,details,get closed(){return closed;},stale(){current=false;},resolve(){resolve();},async open(){details.open=true;await details.ontoggle();}};
}
test('selection formatting sends only the edited axis and applies one editor transaction',async()=>{
  const e=inlineEditor();await e.open();const fields=e.details.children[1],western=fields.children[0],cs=fields.children[1];
  assert.equal(cs.children[1].value,'');western.children[1].value='points';western.children[1].onchange();western.children[2].value='30';
  const pending=e.details.children[3].children[1].onclick();await Promise.resolve();assert.deepEqual(e.requests[1].body.patch,{size:30});e.resolve();await pending;await new Promise(done=>setImmediate(done));assert.equal(e.dispatched.length,1);assert.equal(e.closed,1);
});
test('late results and closed dialogs cannot write to the current editor',async()=>{
  const e=inlineEditor();await e.open();const w=e.details.children[1].children[0];w.children[1].value='base';w.children[1].onchange();e.details.children[3].children[1].onclick();await Promise.resolve();e.stale();e.resolve();await new Promise(done=>setImmediate(done));assert.equal(e.dispatched.length,0);assert.equal(e.closed,0);
});
test('a caret without a styled scope cannot silently format an entire paragraph',async()=>{
  const e=inlineEditor({selected:false});await e.open();assert.equal(e.details.children[3].children[1].disabled,true);assert.equal(e.requests.length,1);
});
test('new errors and controls have explicit localized wording in all shipped locales',()=>{
  const files=fs.readdirSync('assets/i18n').filter(f=>f.endsWith('.json')&&f!=='meta.json');assert.equal(files.length,46);
  const keys=['publication.inline.complexSize','publication.inline.blockBase',...['invalid','unsupported','target','budget','pdf_shaping_unsupported'].map(s=>'error.author_inline_text_'+s)];
  for(const file of files){const data=JSON.parse(fs.readFileSync('assets/i18n/'+file,'utf8'));for(const key of keys)assert(typeof data[key]==='string'&&data[key].length>0,file+':'+key);}
});
