'use strict';
// Pure ownership/restoration controls; actual CSS geometry belongs to native QA.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const root=path.resolve(process.env.READMD_QA_REPO_ROOT||path.join(__dirname,'../..'));
const draft=process.env.READMD_TRACKING_READER_DRAFT;
const load=file=>fs.readFileSync(draft?path.join(draft,file):path.join(root,file),'utf8');
class Style {
  constructor(){this.values=new Map();}
  setProperty(k,v,p=''){this.values.set(k,[String(v),p]);}
  getPropertyValue(k){return this.values.get(k)?.[0]||'';}
  getPropertyPriority(k){return this.values.get(k)?.[1]||'';}
  removeProperty(k){this.values.delete(k);}
}
function environment(value='1.25pt') {
  const source='<!-- readmd:layout v1 {"id":"one","text":{"trackingPt":1.25}} -->\r\n\r\nOriginal e\u0301 👩\u200d💻.\r\n',key='one-bound-revision';
  const code={tagName:'CODE',style:new Style(),computed:'3px'},citation={tagName:'SPAN',style:new Style(),computed:'0.5px'},math={tagName:'MJX-CONTAINER',style:new Style(),computed:'normal'};
  code.style.setProperty('letter-spacing','3px','important');citation.style.setProperty('--readmd-untracked-letter-spacing','0.5px','important');
  const children=[code,citation,math],target={tagName:'P',dataset:{},style:new Style(),textContent:'Original e\u0301 👩\u200d💻.',children,computed:'2px',querySelectorAll:()=>children};
  target.style.setProperty('letter-spacing','2px','important');
  const marker={dataset:{readmdLayout:key},nextElementSibling:target,remove(){this.removed=true;}},tab={id:'a'},toasts=[];
  const container={markers:[marker],querySelectorAll(selector){if(selector==='[data-readmd-layout]')return this.markers;if(selector==='[data-readmd-tracking]')return target.dataset.readmdTracking?[target]:[];return[];}};
  let rules={[key]:{'letter-spacing':value}},replySource=source;
  const context={window:{},state:{editing:true,activeTabId:'a',file:'Z:/owned.md'},cmView:{},getActiveTab:()=>tab,getEditContent:()=>replySource,AbortSignal,Date,setTimeout,clearTimeout,showToast:v=>toasts.push(v),apiMessage:()=> 'unsupported',i18n:{t:k=>k},getComputedStyle:el=>({letterSpacing:el.computed}),apiFetch:async()=>({ok:true,json:async()=>({ok:true,content:replySource,layout:rules})})};
  vm.runInNewContext(load('assets/js/reader/publication.js'),context);
  return{context,publication:context.window.ReadMDPublication,target,marker,container,children,code,citation,math,source,key,toasts,prepare:()=>context.window.ReadMDPublication.prepare(replySource,'Z:/owned.md'),rules(next,sourceSuffix=''){rules=next;replySource=source+sourceSuffix;}};
}
test('only the current source-bound Reader map applies pt without rewriting Unicode or special children',async()=>{
  const e=environment(),original=e.target.textContent,children=[...e.children];await e.prepare();e.publication.applyLayout(e.container);
  assert.equal(e.target.style.getPropertyValue('letter-spacing'),'1.25pt');assert.equal(e.target.dataset.readmdTracking,e.key);
  assert.equal(e.target.textContent,original);assert.deepEqual(e.target.children,children);assert.equal(e.code.style.getPropertyValue('letter-spacing'),'3px');assert.equal(e.code.style.getPropertyPriority('letter-spacing'),'important');
  for(const [element,wanted]of [[e.target,'2px'],[e.code,'3px'],[e.citation,'0.5px'],[e.math,'normal']])assert.equal(element.style.getPropertyValue('--readmd-untracked-letter-spacing'),wanted);
  assert.deepEqual(e.toasts,[]);
});
test('removing the sparse source declaration restores the exact original inline CSS and priority',async()=>{
  const e=environment();await e.prepare();e.publication.applyLayout(e.container);e.container.markers=[];e.rules({},'Override cleared.');await e.prepare();e.publication.applyLayout(e.container);
  assert.equal(e.target.dataset.readmdTracking,undefined);assert.equal(e.target.style.getPropertyValue('letter-spacing'),'2px');assert.equal(e.target.style.getPropertyPriority('letter-spacing'),'important');
  assert.equal(e.target.style.getPropertyValue('--readmd-untracked-letter-spacing'),'');assert.equal(e.code.style.getPropertyValue('--readmd-untracked-letter-spacing'),'');assert.equal(e.code.style.getPropertyValue('letter-spacing'),'3px');
  assert.equal(e.citation.style.getPropertyValue('--readmd-untracked-letter-spacing'),'0.5px');assert.equal(e.citation.style.getPropertyPriority('--readmd-untracked-letter-spacing'),'important');
});
test('a refreshed zero value captures the original baseline rather than the previous local tracking',async()=>{
  const e=environment();await e.prepare();e.publication.applyLayout(e.container);e.rules({[e.key+'-zero']:{'letter-spacing':'0pt'}},'Zero override.');e.marker.dataset.readmdLayout=e.key+'-zero';await e.prepare();e.publication.applyLayout(e.container);
  assert.equal(e.target.style.getPropertyValue('letter-spacing'),'0pt');assert.equal(e.target.style.getPropertyValue('--readmd-untracked-letter-spacing'),'2px');assert.equal(e.code.style.getPropertyValue('letter-spacing'),'3px');
});
test('reapplying an unchanged binding never records its own tracking as the protected baseline',async()=>{
  const e=environment();await e.prepare();e.publication.applyLayout(e.container);e.target.computed='1.66667px';e.publication.applyLayout(e.container);assert.equal(e.target.style.getPropertyValue('--readmd-untracked-letter-spacing'),'2px');
});
test('clearing the cache or changing tabs cannot reuse a previous Reader layout as authority',async()=>{
  for(const mutate of [e=>e.publication.clear(),e=>e.context.state.activeTabId='b']){const e=environment();await e.prepare();e.publication.applyLayout(e.container);mutate(e);e.publication.applyLayout(e.container);assert.equal(e.target.style.getPropertyValue('letter-spacing'),'2px');assert.equal(e.target.dataset.readmdTracking,undefined);}
});
test('style strings, wrong units, nonfinite values and values outside the typed range are rejected',async()=>{
  for(const value of ['21pt','-5.01pt','NaNpt','Infinitypt','1px','1pt;color:red','var(--evil)','1e999pt']){const e=environment(value);await e.prepare();e.publication.applyLayout(e.container);assert.equal(e.target.style.getPropertyValue('letter-spacing'),'2px',value);assert.equal(e.target.dataset.readmdTracking,undefined);}
});
test('a forged target tag does not turn the ordinary paragraph allowance into a container allowance',async()=>{
  const e=environment();e.target.tagName='BLOCKQUOTE';await e.prepare();e.publication.applyLayout(e.container);assert.equal(e.target.style.getPropertyValue('letter-spacing'),'2px');assert.equal(e.target.dataset.readmdTracking,undefined);
});
test('naturally hidden toast is awaited using the existing real element, never a nonmatching selector',async()=>{
  const native=path.join(draft||root,'ui-tests/publication-tracking-ui-native.cjs'),{waitForToastHidden}=require(native);let ticks=0;
  const toast={count:async()=>1,evaluate:async fn=>fn({classList:{contains:()=>ticks>=2}}),isVisible:async()=>ticks<2};
  const page={locator(selector){assert.equal(selector,'#toast');return toast;}},until=async check=>{for(ticks=0;ticks<4;ticks++)if(await check())return;throw Error('Bounded wait expired');};
  assert.deepEqual(await waitForToastHidden(page,until),{classHidden:true,visible:false,naturallyDismissed:true});assert(ticks>=2);
  await assert.rejects(()=>waitForToastHidden({locator:()=>({...toast,count:async()=>0})},until));
  await assert.rejects(()=>waitForToastHidden({locator:()=>({...toast,isVisible:async()=>true})},until),/Bounded wait expired/);
});
test('native geometry controls reject source, code, citation and math leakage instead of weakening the comparison',()=>{
  const {checkReaderGeometry}=require(path.join(draft||root,'ui-tests/publication-tracking-ui-native.cjs'));
  const crypto=require('node:crypto'),source='Repeated source [@study].\r\nRepeated source [@study].',revision=crypto.createHash('sha256').update(source).digest('hex');
  const probe='CSL-TRACKING-GUARD',font={fontFamily:'QA sans',fontSize:'16px',fontWeight:'400',fontStyle:'italic',fontVariant:'small-caps',fontVariantCaps:'small-caps',lineHeight:'28px',letterSpacing:'normal',whiteSpace:'normal',wordBreak:'normal',overflowWrap:'break-word',display:'inline'};
  const flow=()=>({font:{...font}, parentBounds:{left:0,top:0,right:400,bottom:100,width:400,height:100},characters:[...probe].map((text,i)=>({text,unicode:text.charCodeAt(0),font:{...font},rects:[{left:i*7,top:0,right:(i+1)*7,bottom:16,width:7,height:16}]})),elementRects:[{left:0,top:0,right:probe.length*7,bottom:16,width:probe.length*7,height:16}],contentRangeRects:[{left:0,top:0,right:probe.length*7,bottom:16,width:probe.length*7,height:16}]});
  const row=(from,tracked,width,spacing)=>({prefix:'Repeated source',prefixWidth:width,spacing,tracked,code:{text:'TRACKCODE',width:44,height:16,spacing:'normal'},citation:{text:'CSL-TRACKING-GUARD',flow:flow(),width:120,height:16,spacing:'normal',from,to:from+8,revision,format:'readmd-csl-italic|readmd-csl-smallcaps'},math:{text:'x2+1',width:24,height:20,spacing:'normal',shapes:[['path','M1 2L3 4','']]}});
  const first=source.indexOf('[@study]'),second=source.indexOf('[@study]',first+1),proof={source,revision,rows:[row(first,false,100,'normal'),row(second,true,90,'-0.666667px')]};
  assert.equal(checkReaderGeometry(proof,-.5).protectedCodeCslMath,true);
  const cleared=structuredClone(proof);Object.assign(cleared.rows[1],{tracked:false,prefixWidth:100,spacing:'normal'});assert.equal(checkReaderGeometry(cleared,null).localPt,null);
  for(const mutate of [p=>p.rows[1].code.width+=1,p=>p.rows[1].citation.spacing='-.66px',p=>p.rows[1].math.height+=1,p=>p.rows[1].math.shapes[0][1]='wrong outline',p=>p.rows[1].prefixWidth=100,p=>p.rows[1].citation.from=first,p=>p.revision='old',p=>p.rows[1].code.text='TRACCODE',p=>p.rows[1].citation.format='normal',p=>p.rows[1].spacing='0px']){const bad=structuredClone(proof);mutate(bad);assert.throws(()=>checkReaderGeometry(bad,-.5));}
});
