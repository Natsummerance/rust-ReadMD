'use strict';
// Source-level DOM/VM checks only. Actual boot/CSP/service checks use the
// separately owned browser driver after root creates the integrated bundle.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ROOT=path.resolve(__dirname,'..');
const read=name=>fs.readFileSync(path.join(ROOT,name),'utf8');
const html=read('assets/index.html');
function containingTag(markup,id){
  const tokens=[...markup.matchAll(/<\/?([a-z][\w-]*)\b[^>]*>/gi)],stack=[];
  for(const match of tokens){const value=match[0],tag=match[1].toLowerCase();if(value.startsWith('</')){if(stack.at(-1)?.tag===tag)stack.pop();continue;}if(new RegExp('\\bid="'+id+'"').test(value))return stack.map(x=>x.id).filter(Boolean);if(!['area','base','br','col','embed','hr','img','input','link','meta','param','source','track','wbr'].includes(tag)&&!value.endsWith('/>'))stack.push({tag,id:value.match(/\bid="([^"]+)"/)?.[1]});}
  throw Error('Missing element '+id);
}
test('one global chrome contains the action toolbar; service-owned tabs live in the sidebar',()=>{
  assert(containingTag(html,'toolbar').includes('app-titlebar'),'A second toolbar row must not remain a body sibling');
  assert(containingTag(html,'doc-tabs-bar').includes('side'),'The existing tablist belongs to the collapsible sidebar');
  assert(containingTag(html,'doc-tabs-secondary-bar').includes('side'),'Narrow tab fallback must share the sidebar');
  for(const id of ['btn-toc','btn-open','btn-folder','btn-recent','btn-reload','btn-palette','btn-theme','btn-zen','btn-print','btn-edit','btn-more','btn-ai','doc-tabs-bar','file-title'])assert.equal([...html.matchAll(new RegExp('\\bid="'+id+'"','g'))].length,1,id+' remains a single service-owned entry');
});
test('command is icon-only, named and reachable without taking the editor link shortcut',()=>{
  const button=html.match(/<button\b[^>]*id="btn-palette"[\s\S]*?<\/button>/)[0];
  assert.match(button,/aria-label="[^"]+"/);assert.match(button,/aria-haspopup="dialog"/);
  assert.doesNotMatch(button,/tb-palette-label/);assert.match(button,/Meta\+Shift\+P/);
  const shell=read('assets/js/shell/shell.js');assert.match(shell,/key === 'k' && !inCodeMirror\(target\)/);
});
test('new shell labels reuse existing keys in every packaged UI language',()=>{
  const keys=['shell.group.view','menu.file','toolbar.export','toolbar.theme','menu.zenMode','tabs.openDocs'];
  for(const name of fs.readdirSync(path.join(ROOT,'assets/i18n')).filter(name=>name.endsWith('.json')&&name!=='meta.json')){
    const strings=JSON.parse(read('assets/i18n/'+name));for(const key of keys)assert.equal(typeof strings[key],'string',name+': '+key);
  }
  assert.match(html,/id="side-file-actions"[^>]+data-i18n-aria="menu.file"/);
});
test('platform shortcut labels recognize macOS UA data and legacy MacIntel without changing Windows/Linux',()=>{
  const source=read('assets/js/shell/command-palette.js');
  for(const input of [{ua:'macOS',legacy:'MacIntel',mac:true},{legacy:'MacIntel',mac:true},{ua:'Windows',legacy:'Win32',mac:false},{ua:'Linux',legacy:'Linux x86_64',mac:false}]){
    const window={addEventListener(){}},document={documentElement:{dataset:{}},readyState:'loading',addEventListener(){}};
    vm.runInNewContext(source,{window,document,navigator:{userAgentData:input.ua?{platform:input.ua}:undefined,platform:input.legacy},setTimeout(){}});
    assert.equal(window.ReadMDKeys.isMac,input.mac,JSON.stringify(input));
    assert.equal(window.ReadMDKeys.formatShortcut('Mod+Shift+P').join('+'),input.mac?'⌘+⇧+P':'Ctrl+Shift+P');
  }
});

class Element {
  constructor(tag='div'){this.tagName=tag.toUpperCase();this.children=[];this.parentElement=null;this.attrs={};this.events={};this.dataset={};this.style={};this.className='';this.rect={left:12,top:200,width:240,height:44,right:252,bottom:244};this.isConnected=true;this.clientWidth=240;this.scrollWidth=240;this.scrollLeft=0;this.scrollTop=0;this.clientHeight=300;this.offsetHeight=44;this.offsetWidth=240;const el=this;this.classList={contains:name=>el.className.split(/\s+/).includes(name),add:(...names)=>{el.className=[...new Set([...el.className.split(/\s+/).filter(Boolean),...names])].join(' ');},remove:(...names)=>{el.className=el.className.split(/\s+/).filter(name=>!names.includes(name)).join(' ');},toggle:(name,force)=>{const yes=force??!this.classList.contains(name);this.classList[yes?'add':'remove'](name);return yes;}};}
  set innerHTML(value){this.children=[];this.textContent=value;}
  appendChild(el){if(el.parentElement)el.parentElement.children=el.parentElement.children.filter(x=>x!==el);el.parentElement=this;this.children.push(el);return el;}
  append(...els){els.forEach(el=>this.appendChild(el));}
  setAttribute(name,value){this.attrs[name]=String(value);}
  getAttribute(name){return this.attrs[name]??null;}
  addEventListener(name,fn){(this.events[name]??=[]).push(fn);}
  getBoundingClientRect(){return this.rect;}
  focus(){this.focused=true;}
  querySelector(selector){return this.querySelectorAll(selector)[0]||null;}
  querySelectorAll(selector){const match=el=>selector==='.tab-item.active'?el.classList.contains('tab-item')&&el.classList.contains('active'):selector==='.tab-item'?el.classList.contains('tab-item'):selector.startsWith('[data-tab-id=')?selector.includes('"'+el.dataset.tabId+'"'):false;return this.children.flatMap(el=>[...(match(el)?[el]:[]),...el.querySelectorAll(selector)]);}
  fire(name,fields={}){const event={key:'',clientX:24,clientY:204,altKey:false,ctrlKey:false,metaKey:false,shiftKey:false,dataTransfer:{setData(){},getData:()=>fields.sourceId||'source',effectAllowed:'',dropEffect:''},preventDefault(){this.defaultPrevented=true;},stopPropagation(){this.stopped=true;},...fields};for(const fn of this.events[name]||[])fn(event);return event;}
}
function tabs(orientation){
  const nodes=new Map(['doc-tabs-bar','doc-tabs-secondary-bar','doc-tabs-dropdown','doc-tabs-overflow-wrap','btn-home'].map(id=>[id,new Element()]));
  for(const id of ['doc-tabs-bar','doc-tabs-secondary-bar']){nodes.get(id).setAttribute('aria-orientation',orientation);new Element().appendChild(nodes.get(id));}
  const state={tabs:[{id:'source',title:'相同标题.md',path:'/owned/a/相同标题.md'},{id:'target',title:'相同标题.md',path:'/owned/b/相同标题.md',isDirty:true}],activeTabId:'source',mode:'file'};
  const calls={reorder:[],switch:[],close:[]},frames=[];const context={state,window:{innerWidth:1160,ReadMDSession:{persist(){}}},document:{createElement:tag=>new Element(tag),querySelectorAll:selector=>nodes.get('doc-tabs-bar').querySelectorAll(selector)},$:id=>nodes.get(id),requestAnimationFrame(fn){frames.push(fn);},CSS:{escape:x=>x},switchTab:id=>{calls.switch.push(id);return Promise.resolve();},reorderTabs:(...args)=>calls.reorder.push(args),closeTab:(...args)=>calls.close.push(args)};
  vm.createContext(context);const source=read('assets/js/core/tabs.js');vm.runInContext(source.slice(0,source.indexOf('\nfunction startTabInlineRename')),context);context.renderTabsBar();return{nodes,state,calls,context,frames,rows:nodes.get('doc-tabs-bar').children};
}
test('vertical dragging chooses before/after by Y and keeps repeated titles tied to IDs',()=>{
  const a=tabs('vertical'),target=a.rows[1];target.fire('drop',{clientY:240,clientX:24});target.fire('drop',{clientY:204,clientX:248});
  assert.deepEqual(a.calls.reorder,[['source','target',true],['source','target',false]]);
  assert.equal(a.rows[0].children.find(el=>el.className==='tab-title').textContent,'相同标题.md');assert.equal(a.rows[1].dataset.tabId,'target');
  target.children.find(el=>el.className==='tab-close').fire('click');assert.deepEqual(a.calls.close,[['target',false,'button']]);
});
test('vertical drag hints follow the same Y projection as drop',()=>{
  const a=tabs('vertical'),target=a.rows[1];target.fire('dragover',{clientY:240,clientX:24});assert.equal(target.classList.contains('tab-drag-over-right'),true);target.fire('dragover',{clientY:204,clientX:248});assert.equal(target.classList.contains('tab-drag-over-left'),true);
});
test('horizontal consumers retain X drag projection',()=>{
  const a=tabs('horizontal'),target=a.rows[1];target.fire('drop',{clientY:240,clientX:24});target.fire('drop',{clientY:204,clientX:248});assert.deepEqual(a.calls.reorder,[['source','target',false],['source','target',true]]);
});
test('Up/Down project to existing tab switching only for unmodified vertical navigation',()=>{
  const a=tabs('vertical');assert.equal(a.rows[0].fire('keydown',{key:'ArrowDown'}).defaultPrevented,true);assert.equal(a.rows[1].fire('keydown',{key:'ArrowUp'}).defaultPrevented,true);assert.deepEqual(a.calls.switch,['target','source']);
  const untouched=a.rows[0].fire('keydown',{key:'ArrowDown',ctrlKey:true});assert.equal(untouched.defaultPrevented,undefined);assert.equal(a.calls.switch.length,2);
  const horizontal=tabs('horizontal');assert.equal(horizontal.rows[0].fire('keydown',{key:'ArrowDown'}).defaultPrevented,undefined);assert.equal(horizontal.calls.switch.length,0);
});

test('opening a vertical track reveals the focused durable tab using its visible Y range',()=>{
 const a=tabs('vertical'),track=a.nodes.get('doc-tabs-bar');track.rect={left:0,top:100,width:240,height:100,right:240,bottom:200};track.clientHeight=100;track.scrollTop=0;a.rows[1].rect={left:0,top:300,width:240,height:44,right:240,bottom:344};a.context.focusVisibleTab('target');while(a.frames.length)a.frames.shift()();assert.equal(track.scrollTop,152);assert.equal(a.rows[1].focused,true);
});
