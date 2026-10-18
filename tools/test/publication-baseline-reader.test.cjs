'use strict';
// Uses the real Reader consumer. Pixel/glyph origins belong to native QA.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const root=path.resolve(__dirname,'../..');
class Style{constructor(){this.values=new Map();}setProperty(k,v,p=''){this.values.set(k,[String(v),p]);}getPropertyValue(k){return this.values.get(k)?.[0]||'';}getPropertyPriority(k){return this.values.get(k)?.[1]||'';}removeProperty(k){this.values.delete(k);}}
class Node{
  constructor(tag,text){this.tagName=tag?.toUpperCase();this.nodeType=tag?1:3;this.nodeValue=tag?null:text;this.childNodes=[];this.dataset={};this.style=new Style();this.parentNode=null;this.protected=false;this.isConnected=true;}
  appendChild(node){if(node.parentNode){const list=node.parentNode.childNodes;list.splice(list.indexOf(node),1);}this.childNodes.push(node);node.parentNode=this;return node;}
  replaceWith(...nodes){const parent=this.parentNode;if(!parent)return;const list=parent.childNodes,at=list.indexOf(this);for(const node of nodes)if(node.parentNode){const old=node.parentNode.childNodes;old.splice(old.indexOf(node),1);}list.splice(at,1,...nodes);for(const node of nodes)node.parentNode=parent;this.parentNode=null;}
  get textContent(){return this.nodeType===3?this.nodeValue:this.childNodes.map(n=>n.textContent).join('');}
  matches(selector){if(selector==='[data-readmd-baseline]')return Object.hasOwn(this.dataset,'readmdBaseline');if(selector==='[data-readmd-baseline-run]')return Object.hasOwn(this.dataset,'readmdBaselineRun');return selector.includes('code,pre')&&this.protected;}
  querySelectorAll(selector){const found=[];const walk=node=>{for(const child of node.childNodes){if(child.matches(selector))found.push(child);walk(child);}};walk(this);return found;}
}
function env(value='3pt'){
  const source='<!-- readmd:layout v1 {"id":"baseline","text":{"baselineShiftPt":3}} -->\r\n\r\nOriginal 中文 e\u0301 👩\u200d💻.\r\n',key='baseline-source-digest';
  const target=new Node('p'),plain=new Node(null,'Original 中文 e\u0301 👩\u200d💻. '),link=new Node('a'),linkText=new Node(null,'LINK'),sup=new Node('sup'),supText=new Node(null,'2');
  link.appendChild(linkText);sup.appendChild(supText);target.appendChild(plain);target.appendChild(link);target.appendChild(sup);
  const specials=['code','span','mjx-container','a'].map((tag,index)=>{const element=new Node(tag);element.protected=true;element.appendChild(new Node(null,['CODE','CSL','MATH','NOTE'][index]));target.appendChild(element);return element;});
  const originalNodes=[...target.childNodes],document={createElement:tag=>new Node(tag)};target.ownerDocument=document;
  const marker={dataset:{readmdLayout:key},nextElementSibling:target,remove(){this.removed=true;}},tab={},container={markers:[marker],querySelectorAll(selector){if(selector==='[data-readmd-layout]')return this.markers;if(selector==='[data-readmd-baseline]')return target.dataset.readmdBaseline?[target]:[];return[];}};
  let rules={[key]:{'--readmd-baseline-shift':value}},current=source;
  const context={window:{},document,state:{editing:true,activeTabId:'owned',file:'Z:/qa.md'},cmView:{},getActiveTab:()=>tab,getEditContent:()=>current,AbortSignal,Date,setTimeout,clearTimeout,showToast:()=>{},apiMessage:()=>'',i18n:{t:k=>k},apiFetch:async()=>({ok:true,json:async()=>({ok:true,content:current,layout:rules})})};
  vm.runInNewContext(fs.readFileSync(path.join(root,'assets/js/reader/publication.js'),'utf8'),context);
  const reader=context.window.ReadMDPublication;return{reader,target,plain,link,linkText,sup,supText,specials,originalNodes,container,marker,key,context,prepare:()=>reader.prepare(current,'Z:/qa.md'),setRules(next,suffix){rules=next;current=source+suffix;}};
}
test('physical baseline wraps whole ordinary text nodes and preserves code CSL math notes and link identities',async()=>{
  const e=env(),text=e.target.textContent;await e.prepare();e.reader.applyLayout(e.container);
  assert.equal(e.target.textContent,text);assert.equal(e.target.dataset.readmdBaseline,e.key);assert.equal(e.plain.parentNode.style.getPropertyValue('top'),'-3pt');
  assert.equal(e.plain.parentNode.style.getPropertyValue('position'),'relative');assert.equal(e.link.parentNode,e.target);assert.equal(e.linkText.parentNode.parentNode,e.link);assert.equal(e.supText.parentNode.parentNode,e.sup);
  assert.equal(e.linkText.parentNode.style.getPropertyValue('top'),'-3pt');assert.equal(e.supText.parentNode.style.getPropertyValue('top'),'-3pt');
  for(const element of e.specials){assert.equal(element.parentNode,e.target);assert.equal(element.childNodes[0].nodeType,3);assert.equal(element.style.getPropertyValue('top'),'');}
  for(const key of ['top','position','transform','font-size','line-height'])assert.equal(e.target.style.getPropertyValue(key),'');
});
test('reapplying unchanged source is idempotent and removing the override restores original node identity and order',async()=>{
  const e=env();await e.prepare();e.reader.applyLayout(e.container);const wrapper=e.plain.parentNode;e.reader.applyLayout(e.container);assert.equal(e.plain.parentNode,wrapper);
  e.container.markers=[];e.setRules({},'Cleared.');await e.prepare();e.reader.applyLayout(e.container);
  assert.equal(e.target.dataset.readmdBaseline,undefined);assert.deepEqual(e.target.childNodes,e.originalNodes);assert.equal(e.link.childNodes[0],e.linkText);assert.equal(e.sup.childNodes[0],e.supText);
});
test('explicit zero resets inherited offset rather than retaining the previous physical placement',async()=>{
  const e=env();await e.prepare();e.reader.applyLayout(e.container);const zero=e.key+'-zero';e.setRules({[zero]:{'--readmd-baseline-shift':'0pt'}},'Zero.');e.marker.dataset.readmdLayout=zero;await e.prepare();e.reader.applyLayout(e.container);
  assert.equal(e.plain.parentNode.style.getPropertyValue('top'),'0pt');assert.equal(e.plain.parentNode.parentNode,e.target);assert.equal(e.target.dataset.readmdBaseline,zero);
});
test('stale layouts and tab changes restore previous text placement',async()=>{
  for(const mutate of [e=>e.reader.clear(),e=>{e.context.state.activeTabId='different';}]){const e=env();await e.prepare();e.reader.applyLayout(e.container);mutate(e);e.reader.applyLayout(e.container);assert.deepEqual(e.target.childNodes,e.originalNodes);assert.equal(e.target.dataset.readmdBaseline,undefined);}
});
test('forged CSS, bad units, nonfinite and out of range offsets never touch the DOM',async()=>{
  for(const value of ['72.001pt','-72.001pt','NaNpt','Infinitypt','3px','3pt;color:red','var(--evil)','1e999pt']){const e=env(value);await e.prepare();e.reader.applyLayout(e.container);assert.deepEqual(e.target.childNodes,e.originalNodes,value);assert.equal(e.target.dataset.readmdBaseline,undefined,value);}
});
test('container targets do not acquire a paragraph capability',async()=>{const e=env();e.target.tagName='BLOCKQUOTE';await e.prepare();e.reader.applyLayout(e.container);assert.deepEqual(e.target.childNodes,e.originalNodes);assert.equal(e.target.dataset.readmdBaseline,undefined);});
test('a locally opted-in heading receives the same text-only offset',async()=>{const e=env('-2.5pt');e.target.tagName='H2';await e.prepare();e.reader.applyLayout(e.container);assert.equal(e.plain.parentNode.style.getPropertyValue('top'),'2.5pt');for(const special of e.specials)assert.equal(special.childNodes[0].nodeType,3);});

function coldFormula(e){
  const source=new Node(null,' before $x^2+1$ after ');e.target.appendChild(source);
  const math=new Node('mjx-container'),svg=new Node('svg'),path=new Node('path');math.protected=true;math.style.setProperty('vertical-align','-0.2ex');svg.appendChild(path);math.appendChild(svg);
  const before=new Node(null,' before '),after=new Node(null,' after ');
  return{source,math,svg,path,before,after,typeset(){source.replaceWith(before,math,after);}};
}
function assertProtected(formula,target){
  assert.equal(formula.math.parentNode,target);assert.equal(formula.math.childNodes[0],formula.svg);assert.equal(formula.svg.childNodes[0],formula.path);assert.equal(formula.math.style.getPropertyValue('vertical-align'),'-0.2ex');
  for(let node=formula.math;node;node=node.parentNode)assert.equal(Object.hasOwn(node.dataset,'readmdBaselineRun'),false,'Formula must not have a shifted ancestor');
}
function mathQueue(e,typeset){
  const file=fs.readFileSync(path.join(root,'assets/js/reader/formula.js'),'utf8'),from=file.indexOf('function queueMathOperation('),to=file.indexOf('// Incremental typesetting',from);assert(from>=0&&to>from);
  const context={window:e.context.window,mathTypesetQueue:Promise.resolve(),filteredMathInputs:new WeakSet(),MathJax:{startup:{document:{inputJax:[]}},typesetPromise:typeset}};
  vm.runInNewContext(file.slice(from,to),context);return context;
}
test('cold MathJax completion unwraps shifted formula ancestors before metadata without replacing generated nodes',async()=>{
  const e=env('-2.5pt'),formula=coldFormula(e);await e.prepare();e.reader.applyLayout(e.container);
  const coldSpan=formula.source.parentNode;assert.equal(coldSpan.style.getPropertyValue('top'),'2.5pt');
  e.target.style.setProperty('line-height','2.25');e.target.style.setProperty('letter-spacing','.3pt');
  const events=[];let complete;e.context.window.ReadMDMathMetadata={schedule(){assertProtected(formula,e.target);events.push('metadata');}};
  const queue=mathQueue(e,()=>new Promise(resolve=>{complete=()=>{formula.typeset();events.push('typeset');resolve();};}));
  const pending=queue.queueMathTypeset([e.target]);await Promise.resolve();assert.equal(formula.source.parentNode,coldSpan);assert.deepEqual(events,[]);
  complete();await pending;assert.deepEqual(events,['typeset','metadata']);assertProtected(formula,e.target);
  assert.equal(formula.before.parentNode.style.getPropertyValue('top'),'2.5pt');assert.equal(formula.after.parentNode.style.getPropertyValue('top'),'2.5pt');assert.equal(e.plain.parentNode.style.getPropertyValue('top'),'2.5pt');
  assert.equal(e.target.style.getPropertyValue('line-height'),'2.25');assert.equal(e.target.style.getPropertyValue('letter-spacing'),'.3pt');assert.equal(e.specials[0].parentNode,e.target);assert.equal(e.link.childNodes[0].childNodes[0],e.linkText);
  const text=e.target.textContent,spans=e.target.querySelectorAll('[data-readmd-baseline-run]').length;
  queue.MathJax.typesetPromise=async()=>events.push('repeat');await queue.queueMathTypeset([e.target]);
  assertProtected(formula,e.target);assert.equal(e.target.textContent,text);assert.equal(e.target.querySelectorAll('[data-readmd-baseline-run]').length,spans);
  for(const span of e.target.querySelectorAll('[data-readmd-baseline-run]'))assert.equal(span.querySelectorAll('[data-readmd-baseline-run]').length,0);
});
test('baseline refresh includes the passed target but never rebuilds a sibling binding',async()=>{
  const e=env(),other=new Node('p'),otherText=new Node(null,'Unrelated paragraph');other.ownerDocument=e.target.ownerDocument;other.appendChild(otherText);
  e.container.markers.push({dataset:{readmdLayout:e.key},nextElementSibling:other,remove(){}});
  await e.prepare();e.reader.applyLayout(e.container);const siblingSpan=otherText.parentNode;e.reader.refreshBaseline(e.target);
  assert.equal(otherText.parentNode,siblingSpan);assert.equal(otherText.parentNode.style.getPropertyValue('top'),'-3pt');assert.equal(e.plain.parentNode.childNodes[0],e.plain);
});
test('refresh preserves explicit zero while keeping new MathJax and pending DSL stories unshifted',async()=>{
  const e=env('0pt'),formula=coldFormula(e),dsl=new Node('span'),pending=new Node(null,'pending DSL');dsl.protected=true;dsl.dataset.readmdDsl='opaque';dsl.dataset.readmdBusy='true';dsl.appendChild(pending);e.target.appendChild(dsl);
  await e.prepare();e.reader.applyLayout(e.container);formula.typeset();e.reader.refreshBaseline(e.container);assertProtected(formula,e.target);
  assert.equal(formula.before.parentNode.style.getPropertyValue('top'),'0pt');assert.equal(e.target.dataset.readmdBaseline,e.key);assert.equal(dsl.parentNode,e.target);assert.equal(pending.parentNode,dsl);assert.equal(dsl.dataset.readmdBusy,'true');assert.equal(dsl.dataset.readmdReady,undefined);
  const rendered=new Node('span');rendered.protected=true;pending.replaceWith(rendered);e.reader.refreshBaseline(e.container);assert.equal(rendered.parentNode,dsl);assert.equal(dsl.parentNode,e.target);assert.equal(dsl.style.getPropertyValue('top'),'');
});
test('clear or stale source refresh removes old spans without resurrecting the saved shift',async()=>{
  for(const invalidate of [async e=>{e.setRules({},'Cleared source.');await e.prepare();},async e=>e.reader.clear(),async e=>{e.context.state.activeTabId='different';}]){
    const e=env(),formula=coldFormula(e);await e.prepare();e.reader.applyLayout(e.container);formula.typeset();e.target.style.setProperty('color','#123456');await invalidate(e);e.reader.refreshBaseline(e.target);
    assertProtected(formula,e.target);assert.equal(e.target.dataset.readmdBaseline,undefined);assert.equal(e.target.querySelectorAll('[data-readmd-baseline-run]').length,0);assert.equal(formula.before.parentNode,e.target);assert.equal(e.target.style.getPropertyValue('color'),'#123456');
    e.reader.refreshBaseline(e.target);assert.equal(e.target.querySelectorAll('[data-readmd-baseline-run]').length,0);
  }
});
test('refresh neither creates an unbound capability nor touches a detached subtree',async()=>{
  const e=env(),formula=coldFormula(e);await e.prepare();const original=[...e.target.childNodes];e.reader.refreshBaseline(e.target);assert.deepEqual(e.target.childNodes,original);
  e.target.dataset.readmdBaseline=e.key;e.reader.refreshBaseline(e.target);assert.deepEqual(e.target.childNodes,original);delete e.target.dataset.readmdBaseline;
  e.reader.applyLayout(e.container);const span=formula.source.parentNode;e.target.isConnected=false;e.reader.refreshBaseline(e.target);assert.equal(formula.source.parentNode,span);
});
test('failed MathJax work does not run baseline completion or metadata scheduling',async()=>{
  const e=env(),formula=coldFormula(e);await e.prepare();e.reader.applyLayout(e.container);const span=formula.source.parentNode,events=[];
  const queue=mathQueue(e,async()=>{throw Error('synthetic typeset failure');});queue.window={ReadMDPublication:{refreshBaseline(){events.push('refresh');}},ReadMDMathMetadata:{schedule(){events.push('metadata');}}};
  await queue.queueMathTypeset([e.target]);assert.deepEqual(events,[]);assert.equal(formula.source.parentNode,span);
});
