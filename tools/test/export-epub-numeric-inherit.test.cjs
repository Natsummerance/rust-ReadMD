'use strict';
// Execute the product functions with a small DOM/transport host. Native layout,
// browser number-input behavior and EPUB bytes have a separate Windows script.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),crypto=require('node:crypto');
const root=path.resolve(process.env.READMD_QA_REPO_ROOT||path.join(__dirname,'../..'));
const plain=value=>JSON.parse(JSON.stringify(value));
const keys=['fontSize','lineHeight','marginV','marginH'];
const source='---\ntitle: Numeric inheritance\nlanguage: en-US\n---\n# Opening\n\nOriginal source remains unchanged.\n';
class Option{constructor(text,value=text){this.text=String(text);this.value=String(value);}}
class Element{
  constructor(tag='div'){
    this.tagName=tag.toUpperCase();this.dataset={};this.attributes={};this.children=[];this.className='';this.value='';this.isConnected=true;this.validity={badInput:false};
    this.classList={add:name=>{if(!this.classList.contains(name))this.className+=' '+name;},remove:name=>{this.className=this.className.split(/\s+/).filter(part=>part!==name).join(' ');},contains:name=>this.className.split(/\s+/).includes(name)};
  }
  setAttribute(name,value){this.attributes[name]=String(value);if(name.startsWith('data-'))this.dataset[name.slice(5).replace(/-([a-z])/g,(_,c)=>c.toUpperCase())]=String(value);}
  setCustomValidity(message){this.validationMessage=message;}reportValidity(){this.reported=true;return !this.validationMessage;}focus(){this.focused=true;}
  appendChild(child){this.children.push(child);return child;}addEventListener(){}closest(){return null;}
  set innerHTML(html){this.children=[];const numeric=/<input\b[^>]*type="number"[^>]*data-k="([^"]+)"[^>]*>/u.exec(html),select=/<select\b[^>]*data-k="([^"]+)"[^>]*>([\s\S]*?)<\/select>/u.exec(html);
    if(numeric){const el=new Element('input');el.type='number';el.dataset.k=numeric[1];for(const name of ['min','max','step'])el[name]=new RegExp(name+'="([^"]*)"','u').exec(numeric[0])?.[1]||'';this.children.push(el);}
    if(select){const el=new Select(select[1]);for(const option of select[2].matchAll(/<option value="([^"]*)">([\s\S]*?)<\/option>/gu))el.add(new Option(option[2],option[1]));this.children.push(el);}
  }
  querySelector(selector){return selector==='[data-k]'?this.children.find(child=>child.dataset.k)||null:null;}
}
class Select extends Element{
  constructor(key){super('select');this.type='select-one';this.dataset.k=key;this.options=[];this._value='';}
  add(option){this.options.push(option);if(this.options.length===1)this._value=option.value;}
  get value(){return this._value;}set value(value){this._value=this.options?.some(option=>option.value===String(value))?String(value):'';}
}
function environment({options={},defaults={typography:{size:10.5,lineHeight:1.55}},bridge=true,language=true}={}){
  const ids=new Map(),controls=[],requests=[],calls=[],saved=[],timers=[],toasts=[];
  for(const id of ['export-opts','export-result','export-warns','exp-preset','export-preview-modal','exp-save-name','exp-save-input','exp-save-ok','exp-save-cancel'])ids.set(id,new Element());
  ids.get('exp-preset').value='minimal';ids.get('export-preview-modal').className='hidden';
  const document={createElement:tag=>new Element(tag),getElementById:id=>ids.get(id)||null,querySelectorAll:selector=>selector==='#export-opts [data-k]'?controls:[],querySelector:()=>null};
  const dictionary=JSON.parse(fs.readFileSync(path.join(root,'assets/i18n/en.json'),'utf8')),i18n={t:key=>dictionary[key]||key};
  const context={window:{i18n,addEventListener(){}},i18n,document,Option,AbortController,crypto,console,
    state:{mode:'file',editing:false,original:source,file:'numeric-inheritance.md',dir:'',export:{fmt:'epub',defaults:plain(defaults),options:plain(options),catalog:[],custom:{},presets:{}}},$:id=>ids.get(id)||null,hasPy:bridge,
    py:{export_epub:async(...args)=>{calls.push(plain(args));return{ok:true,path:''};},save_export_presets:async payload=>{saved.push(plain(payload));return true;}},
    apiFetch:async(route,request)=>{requests.push({route,body:JSON.parse(request.body)});return{ok:true,json:async()=>({ok:true,path:'',format:'epub',html:'<html></html>'})};},
    setTimeout:fn=>{timers.push(fn);return timers.length;},clearTimeout(){},busy(){},showToast:message=>toasts.push(message),warnMessages:()=>[]};
  vm.createContext(context);
  for(const file of ['assets/js/editor/publication-metadata.js','assets/js/features/export.js'])vm.runInContext(fs.readFileSync(path.join(root,file),'utf8'),context,{filename:file});
  const fields=context.getExportSections().flatMap(section=>section.fields||[]);
  for(const key of [...keys.map(key=>'epub.'+key),...(language?['epub.language']:[])])controls.push(context.expFieldEl(fields.find(field=>field.k===key)).querySelector('[data-k]'));
  return{context,ids,controls,requests,calls,saved,toasts,fields,input:key=>controls.find(el=>el.dataset.k===('epub.'+key)),apply:()=>context.applyExportOptionsToDom(),collect:()=>plain(context.collectExportOptions()),async timers(){while(timers.length)await timers.shift()();},
    async save(name='Sparse numeric layout'){context.renderExportPresetSelect=()=>{};context.updateExportLivePreview=()=>{};await context.expSavePreset();ids.get('exp-save-input').value=name;await ids.get('exp-save-ok').onclick();}};
}
function sparse(epub){for(const key of keys)assert.equal(Object.hasOwn(epub||{},key),false,key+' must remain absent');}

test('EPUB controls expose compatible API ranges and accept fractional numbers',()=>{
  const e=environment(),expected={fontSize:[6,72],lineHeight:[1,5],marginV:[0,40],marginH:[0,40]};
  for(const [key,[min,max]]of Object.entries(expected)){const el=e.input(key);assert.equal(el.min,String(min));assert.equal(el.max,String(max));assert.equal(el.step,'any');}
});
test('untouched default and null controls remain blank and sparse with inherited placeholders',()=>{
  for(const options of [{}, {epub:{fontSize:null,lineHeight:null,marginV:null,marginH:null}}]){
    const e=environment({options});e.apply();for(const key of keys)assert.equal(e.input(key).value,'');
    assert.equal(e.input('fontSize').placeholder,'Inherit · 10.5pt');assert.equal(e.input('lineHeight').placeholder,'Inherit · 1.55');
    assert.equal(e.input('marginV').placeholder,'Inherit · 5%');assert.equal(e.input('marginH').placeholder,'Inherit · 8%');sparse(e.collect().epub);
    assert.deepEqual(plain(e.context.state.export.options),options);assert.equal(e.context.state.original,source);
  }
});
test('explicit zero and fractional overrides are preserved without clamping',()=>{
  const e=environment();e.apply();for(const [key,value]of Object.entries({fontSize:14.25,lineHeight:1.75,marginV:0,marginH:0}))e.input(key).value=String(value);
  const actual=e.collect();assert.deepEqual(actual.epub,{fontSize:14.25,lineHeight:1.75,marginV:0,marginH:0});assert.equal(e.context.validateEpubNumericControls(),true);
});
test('clear removes old merged overrides and repeated collection never restores them',()=>{
  const e=environment({defaults:{typography:{size:10.5,lineHeight:1.55},epub:{fontSize:18,lineHeight:2,marginV:3,marginH:6}},options:{epub:{fontSize:14,lineHeight:1.8,marginV:0,marginH:0,language:'en-US',splitLevel:'h2'}}});e.apply();
  for(const key of keys)e.input(key).value='';
  for(let i=0;i<3;i++){const actual=e.collect();sparse(actual.epub);assert.equal(actual.epub.language,'en-US');assert.equal(actual.epub.splitLevel,'h2');e.context.state.export.options=actual;}
});
test('changing a preset resets the same controls and keeps each source preset sparse',()=>{
  const e=environment({options:{epub:{fontSize:16,lineHeight:2.2,marginV:0,marginH:9,language:'zh-CN'}}});e.apply();assert.equal(e.input('fontSize').value,'16');
  const preset={typography:{size:10.5,lineHeight:1.55}};e.context.state.export.presets.minimal=plain(preset);e.context.state.export.options=e.context.exportPresetOptions('minimal');e.apply();
  for(const key of keys)assert.equal(e.input(key).value,'');assert.equal(e.input('language').value,'');sparse(e.collect().epub);assert.deepEqual(plain(e.context.state.export.presets.minimal),preset);
});
test('controls absent in another format retain saved EPUB overrides',()=>{
  const options={epub:{fontSize:17,lineHeight:1.8,marginV:0,marginH:0,language:'en-US'}},e=environment({options});e.controls.length=0;e.context.state.export.fmt='docx';assert.deepEqual(e.collect().epub,options.epub);
});
test('invalid numeric entries remain unmodified and block both export entry paths',async()=>{
  for(const [key,value]of [['fontSize','0'],['fontSize','72.1'],['lineHeight','0.99'],['lineHeight','5.1'],['marginV','-1'],['marginH','40.1'],['fontSize','14oops']]){
    const e=environment();e.apply();e.input(key).value=value;const actual=e.collect();assert.equal(actual.epub[key],Number.isFinite(Number(value))?Number(value):value);
    assert.equal(e.context.validateEpubNumericControls(),false);assert.equal(e.context.runExport(),false);assert.equal(await e.context.runExportOnce(),false);assert.equal(e.calls.length+e.requests.length,0);assert(e.input(key).reported);
  }
});
test('number-input badInput is an invalid value instead of an inherited clear',async()=>{
  const e=environment();e.apply();e.input('fontSize').value='';e.input('fontSize').validity.badInput=true;assert.equal(e.collect().epub.fontSize,'invalid');assert.equal(await e.context.runExportOnce(),false);assert.equal(e.calls.length,0);
});
test('malformed stored JSON types survive display and collection until explicitly corrected',async()=>{
  for(const value of ['14',false,[],{},'']){const options={epub:{fontSize:value}},e=environment({options});e.apply();assert.deepEqual(e.collect().epub.fontSize,value);assert.equal(e.context.validateEpubNumericControls(),false);assert.equal(await e.context.runExportOnce(),false);assert.deepEqual(plain(e.context.state.export.options),options);}
});
test('real preview request keeps inherited fields absent and sends explicit zero',async()=>{
  for(const zero of [false,true]){const e=environment({options:{epub:{language:'en-US'}}});e.apply();if(zero){e.input('marginV').value='0';e.input('marginH').value='0';}
    e.context.requestNativeExportPreview('epub',e.context.collectExportOptions(),source,'Numeric','minimal');await e.timers();assert.equal(e.requests.length,1);const body=e.requests[0].body;assert.equal(body.content,source);assert.equal(body.options.epub.language,'en-US');
    if(zero){assert.equal(body.options.epub.marginV,0);assert.equal(body.options.epub.marginH,0);assert.equal(Object.hasOwn(body.options.epub,'fontSize'),false);}else sparse(body.options.epub);
  }
});
test('both actual export transports preserve inherited fields and zero with source language',async()=>{
  for(const bridge of [false,true]){const e=environment({bridge,options:{epub:{language:'en-US'},meta:{language:'en-GB'}}});e.apply();e.input('marginV').value='0';e.input('marginH').value='0';await e.context.runExportOnce();
    const options=bridge?e.calls[0][3]:e.requests[0].body.options;assert.equal(options.epub.marginV,0);assert.equal(options.epub.marginH,0);assert.equal(options.epub.language,'en-US');for(const key of ['fontSize','lineHeight'])assert.equal(Object.hasOwn(options.epub,key),false);
    assert.equal(options.meta.language,'en-GB');assert.equal(e.context.state.original,source);assert.equal(e.saved.length,bridge?1:0);assert.equal(e.context.state.export.last.options.epub.marginH,0);
  }
});
test('saving and reloading sparse and explicit zero presets retains regional language and source state',async()=>{
  for(const zero of [false,true]){const e=environment({options:{epub:{language:'en-US'},meta:{language:'en-GB'}}});e.apply();if(zero){e.input('marginV').value='0';e.input('marginH').value='0';}await e.save();assert.equal(e.saved.length,1);
    const stored=plain(e.saved[0].custom['Sparse numeric layout']),reopened=environment({options:stored});reopened.apply();assert.equal(reopened.input('language').value,'en-US');assert.equal(reopened.input('fontSize').value,'');assert.equal(reopened.input('lineHeight').value,'');
    if(zero){assert.equal(reopened.input('marginV').value,'0');assert.equal(reopened.collect().epub.marginH,0);}else sparse(reopened.collect().epub);
    assert.deepEqual(stored,e.saved[0].last.options);assert.equal(e.context.state.original,source);
  }
});
test('invalid EPUB values cannot be saved from an EPUB or another-format preset',async()=>{
  for(const format of ['epub','pdf']){const e=environment({options:{epub:{marginH:-1}}});e.apply();e.context.state.export.fmt=format;if(format==='pdf')e.controls.length=0;await e.save();assert.equal(e.saved.length,0);assert.deepEqual(plain(e.context.state.export.custom),{});}
});
