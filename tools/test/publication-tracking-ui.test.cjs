'use strict';
// Pure interaction contracts. This does not certify native geometry/CSP/Word.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const root=path.resolve(process.env.READMD_QA_REPO_ROOT||path.join(__dirname,'../..')),draft=process.env.READMD_P04_UI_DRAFT;
const load=p=>fs.readFileSync(draft?path.join(draft,path.basename(p)):path.join(root,p),'utf8'),plain=v=>JSON.parse(JSON.stringify(v));
class Element{
 constructor(tag){this.tagName=tag.toUpperCase();this.children=[];this.dataset={};this.attributes={};this.style={};this.className='';this.value='';this.listeners={};this.disabled=false;this.classList={add:n=>{if(!this.className.split(' ').includes(n))this.className+=' '+n;},remove:n=>{this.className=this.className.split(' ').filter(x=>x!==n).join(' ');},contains:n=>this.className.split(' ').includes(n)};}
 append(...nodes){for(const node of nodes){this.children.push(node);node.parentElement=this;}}appendChild(node){this.append(node);return node;}
 replaceChildren(...nodes){for(const node of this.children)node.parentElement=null;this.children=[];this.append(...nodes);}
 setAttribute(k,v){this.attributes[k]=String(v);}addEventListener(k,fn){(this.listeners[k]||=[]).push(fn);}emit(name){for(const fn of this.listeners[name]||[])fn({target:this});}
 setCustomValidity(message){this.validationMessage=message;}reportValidity(){this.reported=true;return this.disabled||!this.validationMessage;}focus(){this.focused=true;}
 querySelectorAll(selector){const all=[];const visit=node=>{for(const child of node.children){all.push(child);visit(child);}};visit(this);return all.filter(n=>selector==='[data-tracking-control]'?n.dataset.trackingControl!==undefined:selector==='[data-baseline-control]'?n.dataset.baselineControl!==undefined:selector==='[data-line-spacing-control]'?n.dataset.lineSpacingControl!==undefined:selector==='[data-k]'?n.dataset.k!==undefined:selector==='input, select'?['INPUT','SELECT'].includes(n.tagName):false);}
 closest(selector){for(let node=this;node;node=node.parentElement)if(selector==='.exp-sec'&&node.classList.contains('exp-sec'))return node;return null;}
}
function environment(){const ids=new Map(),host=new Element('div');ids.set('export-opts',host);for(const id of ['export-preview-mini-content','export-preview-paper-meta','exp-save-name','exp-save-input','exp-save-ok','exp-save-cancel'])ids.set(id,new Element(id.includes('input')?'input':'div'));
 const document={createElement:t=>new Element(t),querySelectorAll:s=>s==='#export-opts [data-k]'?host.querySelectorAll('[data-k]'):[],getElementById:id=>ids.get(id)||null};
 const context={window:{addEventListener(){}},document,state:{export:{defaults:{typography:{lineHeight:1.6}},options:{typography:{lineHeight:1.6}},fmt:'docx',custom:{},presets:{},catalog:[]},editing:false},i18n:{t:key=>key},AbortController,AbortSignal,setTimeout,clearTimeout,console,showToast:()=>{},apiFetch:()=>{throw Error('No API call allowed by this control test');},$:id=>ids.get(id)||null};
 context.window.i18n=context.i18n;vm.createContext(context);vm.runInContext(load('assets/js/reader/publication.js'),context);vm.runInContext(load('assets/js/features/export.js'),context);
 return{context,host,ids,helper:context.window.ReadMDPublication.lineSpacing,mount(style={},scope='global',options={}){return this.helper.mount(host,{scope,style,t:k=>k,...options});}};
}
function mode(c,value){c.mode.value=value;c.mode.emit('change');}function number(c,value){c.value.value=String(value);c.value.emit('input');}
test('optional tracking remains absent when an old preset is merely opened',()=>{const e=environment();e.helper=e.context.window.ReadMDPublication.tracking;const c=e.helper.mount(e.host,{style:{},getFormat:()=> 'docx'});assert.equal(c.input.value,'');assert.equal(c.validate(),true);assert(!Object.hasOwn(plain(e.context.collectExportOptions()).typography,'trackingPt'));});
test('null overlay inherits without losing explicit zero or replacing the parent value',()=>{const e=environment();for(const value of [0,1.25,-.5])assert.equal(e.context.expDeepMerge({typography:{trackingPt:value}},{typography:{trackingPt:null}}).typography.trackingPt,value);assert(!Object.hasOwn(e.context.expDeepMerge({typography:{}},{typography:{trackingPt:null}}).typography,'trackingPt'));});
test('explicit zero survives collection while empty removes the override',()=>{const e=environment(),h=e.context.window.ReadMDPublication.tracking,c=h.mount(e.host,{style:{trackingPt:2},getFormat:()=> 'docx'});e.context.state.export.options.typography.trackingPt=2;c.input.value='0';c.input.emit('input');assert.equal(e.context.collectExportOptions().typography.trackingPt,0);c.input.value='';c.input.emit('input');assert(!Object.hasOwn(e.context.collectExportOptions().typography,'trackingPt'));});
test('fractional tracking is not quantized or clamped by the control',()=>{const e=environment(),h=e.context.window.ReadMDPublication.tracking,c=h.mount(e.host,{getFormat:()=> 'docx'});for(const value of [-5,-.025,0,.025,20]){c.input.value=String(value);c.input.emit('input');assert.equal(c.validate(),true);assert.equal(e.context.collectExportOptions().typography.trackingPt,value);}});
for(const value of ['21','-5.01','0x10','NaN','Infinity','-','1,5'])test('invalid tracking draft stays visible and blocks export: '+value,()=>{const e=environment(),h=e.context.window.ReadMDPublication.tracking,c=h.mount(e.host,{getFormat:()=> 'docx'});c.input.value=value;c.input.emit('input');assert.equal(c.validate(),false);const result=e.context.collectExportOptions();h.refreshExport(e.host,result.typography);assert.equal(c.input.value,value);assert.equal(c.validate(true),false);assert.equal(e.context.runExport(),false);});
test('malformed persisted tracking cannot silently become a valid number',()=>{const e=environment(),h=e.context.window.ReadMDPublication.tracking;for(const value of ['1',false,{},'']){const c=h.mount(e.host,{style:{trackingPt:value},getFormat:()=> 'docx'});assert.equal(c.validate(),false);}});
test('all unsupported previews clear stale content and block before content/API preparation',async()=>{for(const format of ['pdf','html','epub','tex','presentation']){const e=environment(),h=e.context.window.ReadMDPublication.tracking;e.context.state.export.fmt=format;e.context.state.export.options.typography.trackingPt=0;h.mount(e.host,{style:{trackingPt:0},getFormat:()=>format});e.context.cancelNativeExportPreview=()=>{};e.context.currentExportContent=()=>{throw Error('Must reject before content preparation');};e.ids.get('export-preview-mini-content').textContent='stale';e.context.updateExportLivePreview();assert.equal(e.ids.get('export-preview-mini-content').textContent,'error.publication_tracking_unsupported');assert.equal(await e.context.runExportOnce(),false);assert(e.context.expFieldApplicable({type:'tracking'},[],format));}});
test('new tracking control stays hidden on unconfigured unsupported formats',()=>{const e=environment();for(const format of ['pdf','html','epub','tex','presentation'])assert.equal(e.context.expFieldApplicable({type:'tracking'},[],format),false);assert.equal(e.context.expFieldApplicable({type:'tracking'},[],'docx'),true);});
test('valid shared tracking preset remains savable in unsupported HTML without exporting',async()=>{const e=environment(),h=e.context.window.ReadMDPublication.tracking,calls=[];e.context.state.export.fmt='html';e.context.state.export.options.typography.trackingPt=.025;h.mount(e.host,{style:{trackingPt:.025},getFormat:()=> 'html'});e.context.renderExportPresetCards=()=>{};e.context.hasPy=false;e.context.py=null;e.context.renderExportPresetSelect=()=>{};e.context.getExportPresetNames=()=>({});e.context.getExportLast=()=>({});e.context.cancelNativeExportPreview=()=>{};e.context.apiFetch=async(route,options)=>{calls.push({route,body:JSON.parse(options.body)});return{ok:true,json:async()=>({ok:true})};};e.context.expSavePreset();e.ids.get('exp-save-input').value='Tracking shared';await e.ids.get('exp-save-ok').onclick();assert.equal(calls.length,1);assert.equal(calls[0].route,'/api/export/presets');assert.equal(calls[0].body.custom['Tracking shared'].typography.trackingPt,.025);});

test('DOCX automatic preview reports unsupported inline while an explicit unsupported export still reports a toast',async()=>{
  const e=environment(),toasts=[];let timer;e.ids.set('export-preview-modal',{classList:{contains:()=>true},querySelector:()=>null});e.context.showToast=message=>toasts.push(message);e.context.window.READMD_ENGINE='rust';e.context.state.export.options.typography.trackingPt=1;
  e.context.window.ReadMDPublication.tracking.mount(e.host,{style:{trackingPt:1},getFormat:()=>e.context.state.export.fmt});
  e.context._t=(key,params)=>e.context.i18n.t(key,params);e.context.window.ReadMDTask={loading:host=>{host.textContent='loading';}};e.context.crypto={randomUUID:()=> 'owned-preview-test'};
  e.context.setTimeout=callback=>{timer=callback();return 1;};e.context.clearTimeout=()=>{};e.context.apiMessage=data=>'error.'+data.error_code;
  e.context.apiFetch=async()=>({ok:false,json:async()=>({ok:false,error_code:'publication_tracking_unsupported'})});
  e.context.currentExportContent=()=> 'Original source';e.context.currentExportName=()=> 'Original';e.context.renderExportPresetCards=()=>{};
  e.context.updateExportLivePreview();assert(timer);await timer;
  assert(e.ids.get('export-preview-mini-content').textContent.includes('error.publication_tracking_unsupported'));assert.deepEqual(toasts,[]);
  e.context.state.export.fmt='html';assert.equal(await e.context.runExportOnce(),false);assert.deepEqual(toasts,['error.publication_tracking_unsupported']);
});

test('baseline shift is sparse, preserves inherited zero and clears without materializing a default',()=>{
  const e=environment(),h=e.context.window.ReadMDPublication.baseline,c=h.mount(e.host,{style:{},getFormat:()=>({format:'docx'})});
  assert.equal(c.input.value,'');assert.equal(c.validate(),true);
  assert(!Object.hasOwn(e.context.collectExportOptions().typography,'baselineShiftPt'));
  for(const value of [-2.125,0,2.125])assert.equal(e.context.expDeepMerge({typography:{baselineShiftPt:value}},{typography:{baselineShiftPt:null}}).typography.baselineShiftPt,value);
  c.input.value='0';c.input.emit('input');assert.equal(e.context.collectExportOptions().typography.baselineShiftPt,0);
  e.context.state.export.options.typography.baselineShiftPt=3;
  c.input.value='';c.input.emit('input');assert(!Object.hasOwn(e.context.collectExportOptions().typography,'baselineShiftPt'));
});
test('baseline and tracking controls preserve independent physical values and validation',()=>{
  const e=environment(),h=e.context.window.ReadMDPublication,baseline=h.baseline.mount(e.host,{getFormat:()=> 'docx'}),tracking=h.tracking.mount(e.host,{getFormat:()=> 'docx'});
  tracking.input.value='0.025';tracking.input.emit('input');
  for(const value of [-72,-.125,0,.125,72]){baseline.input.value=String(value);baseline.input.emit('input');assert.equal(baseline.validate(),true);const options=e.context.collectExportOptions();assert.equal(options.typography.baselineShiftPt,value);assert.equal(options.typography.trackingPt,.025);}
  baseline.input.value='73';baseline.input.emit('input');assert.equal(h.baseline.validateExport(e.host),false);assert.equal(h.tracking.validateExport(e.host),true);assert.equal(e.context.runExport(),false);
});
for(const value of ['-72.01','72.01','NaN','0x10','1,5','Infinity','-'])test('invalid baseline draft remains recoverable and prevents output: '+value,()=>{
  const e=environment(),h=e.context.window.ReadMDPublication.baseline,c=h.mount(e.host,{getFormat:()=> 'docx'});
  c.input.value=value;c.input.emit('input');assert.equal(c.validate(),false);
  const options=e.context.collectExportOptions();h.refreshExport(e.host,options.typography);
  assert.equal(c.input.value,value);assert.equal(e.context.runExport(),false);
});
test('malformed persisted baseline shift is rejected rather than coerced',()=>{
  for(const value of ['2',false,{},[]]){const e=environment(),h=e.context.window.ReadMDPublication.baseline,c=h.mount(e.host,{style:{baselineShiftPt:value},getFormat:()=> 'docx'});assert.equal(c.validate(),false);}
});
test('baseline format gating admits verified physical PDF engines and clears unsupported stale previews',async()=>{
  for(const [format,engine]of [['html',undefined],['epub',undefined],['tex',undefined],['pdf','xelatex'],['pdf','pdflatex']]){
    const e=environment(),h=e.context.window.ReadMDPublication.baseline;e.context.state.export.fmt=format;e.context.state.export.options.pdf={engine};e.context.state.export.options.typography.baselineShiftPt=0;
    h.mount(e.host,{style:{baselineShiftPt:0},getFormat:()=>({format,engine})});e.context.cancelNativeExportPreview=()=>{};
    e.context.currentExportContent=()=>{throw Error('Unsupported baseline must fail before content preparation');};e.ids.get('export-preview-mini-content').textContent='stale';
    e.context.updateExportLivePreview();assert.equal(e.ids.get('export-preview-mini-content').textContent,'error.publication_baseline_unsupported');assert.equal(await e.context.runExportOnce(),false);
    assert.equal(e.context.expFieldApplicable({type:'baseline'},[],format),true,'Existing override must remain removable');
    delete e.context.state.export.options.typography.baselineShiftPt;assert.equal(e.context.expFieldApplicable({type:'baseline'},[],format),false);
  }
  const e=environment(),h=e.context.window.ReadMDPublication.baseline;
  for(const [format,engine]of [['docx',undefined],['pdf',undefined],['pdf','rust'],['pdf','typst']])assert.equal(h.issue({typography:{baselineShiftPt:1}},format,engine),'');
});

test('Typst baseline control is available before configuration, keeps exact signed points and clears to inheritance',()=>{
  const e=environment(),helper=e.context.window.ReadMDPublication.baseline;
  e.context.state.export.fmt='pdf';e.context.state.export.options.pdf={engine:'typst'};
  assert.equal(e.context.expFieldApplicable({type:'baseline'},[],'pdf'),true);
  const control=helper.mount(e.host,{getFormat:()=>({format:'pdf',engine:e.context.state.export.options.pdf.engine})});
  assert(!Object.hasOwn(e.context.collectExportOptions().typography,'baselineShiftPt'));
  for(const value of [3,-2.5,3.25,0]){control.input.value=String(value);control.input.emit('input');assert.equal(control.validate(),true);assert.equal(helper.issue(e.context.collectExportOptions(),'pdf','typst'),'');assert.equal(e.context.collectExportOptions().typography.baselineShiftPt,value);}
  e.context.state.export.options.typography.baselineShiftPt=3.25;
  control.input.value='';control.input.emit('input');assert(!Object.hasOwn(e.context.collectExportOptions().typography,'baselineShiftPt'));
  assert.equal(e.context.expFieldApplicable({type:'baseline'},[],'pdf'),true);
});
test('a shared baseline preset remains savable without claiming the active output supports it',async()=>{
  const e=environment(),h=e.context.window.ReadMDPublication.baseline,calls=[];e.context.state.export.fmt='epub';e.context.state.export.options.typography.baselineShiftPt=-.125;
  h.mount(e.host,{style:{baselineShiftPt:-.125},getFormat:()=> 'epub'});
  e.context.renderExportPresetCards=()=>{};e.context.hasPy=false;e.context.py=null;e.context.renderExportPresetSelect=()=>{};e.context.getExportPresetNames=()=>({});e.context.getExportLast=()=>({});e.context.cancelNativeExportPreview=()=>{};
  e.context.apiFetch=async(route,options)=>{calls.push({route,body:JSON.parse(options.body)});return{ok:true,json:async()=>({ok:true})};};
  e.context.expSavePreset();e.ids.get('exp-save-input').value='Baseline shared';await e.ids.get('exp-save-ok').onclick();
  assert.equal(calls.length,1);assert.equal(calls[0].route,'/api/export/presets');assert.equal(calls[0].body.custom['Baseline shared'].typography.baselineShiftPt,-.125);
});
