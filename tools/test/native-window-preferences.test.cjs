'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const root=path.resolve(__dirname,'../..'),source=fs.readFileSync(path.join(root,'assets/js/shell/shell.js'),'utf8');
const body=source.slice(source.indexOf('function syncWindowPreferences()'),source.indexOf('function initWindowChrome()'));
const dictionary = locale => JSON.parse(fs.readFileSync(path.join(root, 'assets/i18n', locale + '.json'), 'utf8'));
function run(py,closeToTray,dict=dictionary('en')){
 const calls=[];
 const context=vm.createContext({hasPy:true,py:{...py,window_control:(action,options)=>calls.push({action,options:JSON.parse(JSON.stringify(options))})},state:{closeToTray},window:{i18n:{t:key=>dict[key]??key}},$:()=>null});
 vm.runInContext(body+'\nsyncWindowPreferences();',context);return calls;
}
test('native Mac titlebars still apply close-to-menu-bar and translated menu preferences',()=>{
 const [call]=run({custom_titlebar:false,native_titlebar:'macos'},false);
 assert.equal(call.action,'preferences');assert.equal(call.options.closeToTray,false);
 const labels=call.options.menuLabels;
 assert.deepEqual(['file','save','save-as'].map(id=>labels[id]),['File','Save','Save As']);
 assert.deepEqual(['edit','undo','redo','cut','copy','paste'].map(id=>labels[id]),['Edit','Undo','Redo','Cut','Copy','Paste']);
 assert.equal(labels.open,dictionary('en')['menu.open']);
 assert.equal(labels.hide,'Hide ReadMD');
 assert.equal(labels['select-all'],'Select All');
 assert.equal(labels.fullscreen,'Fullscreen');
 assert.equal(run({custom_titlebar:false,native_titlebar:'macos'},true)[0].options.closeToTray,true);
});
test('the tray keeps readable native labels while the locale is loading',()=>{
 const mac={native_titlebar:'macos'};
 assert.deepEqual(run(mac,false,{})[0].options,{closeToTray:false,menuLabels:{}});
 assert.deepEqual(run(mac,true,{'window.show':'Show ReadMD','window.exit':'window.exit'})[0].options.menuLabels,{show:'Show ReadMD'});
});
test('native labels resolve in every shipped locale and leave shortcuts to AppKit',()=>{
 const locales=Object.keys(dictionary('meta'));
 for(const locale of locales){
  const labels=run({native_titlebar:'macos'},true,dictionary(locale))[0].options.menuLabels;
  assert.equal(Object.keys(labels).length,24,locale);
  for(const [action,value] of Object.entries(labels)){
   assert.ok(value.trim()&&!/^[\w-]+(?:\.[\w-]+)+$/.test(value),`${locale} ${action}`);
   assert.doesNotMatch(value,/[（(](?:[^()（）]*\+[^()（）]+|F11)[)）]$/iu,`${locale} ${action}`);
  }
 }
});
test('the native Mac path subscribes to the same locale events as a custom titlebar',()=>{
 const listeners={}, calls=[];
 const actions={dataset:{},addEventListener(){}};
 const title={textContent:''};
 let strings={};
 const context=vm.createContext({hasPy:true,state:{},$:id=>({'titlebar-actions':actions,'window-title':title}[id]),
  py:{native_titlebar:'macos',custom_titlebar:false,window_control:(action,options)=>calls.push({action,options})},
  document:{title:'ReadMD',querySelector:()=>({})},MutationObserver:class{observe(){}},
  syncChromeDocumentDisclosure(){},initSingleChromeNavigation(){},initMacWindowChrome(){},
  window:{i18n:{t:key=>strings[key]??key},addEventListener:(type,fn)=>{listeners[type]=fn;}}});
 const begin=source.indexOf('function initWindowChrome()'),end=source.indexOf('\nfunction ',begin+1);
 assert(begin>=0&&end>begin);
 const chrome=source.slice(begin,end);
 vm.runInContext(body+chrome+'\ninitWindowChrome();',context);
 assert.equal(Object.keys(calls.find(call=>call.action==='preferences').options.menuLabels).length,0);
 strings=dictionary('ja');
 listeners['readmd:language-changed']();
 assert.equal(calls.at(-1).options.menuLabels.open,strings['menu.open']);
 strings=dictionary('ar');
 listeners['readmd:language-changed']();
 assert.equal(calls.at(-1).options.menuLabels['select-all'],strings['mac.selectAll']);
});
test('native Close dismisses the guarded modal before touching the document',async()=>{
 const closed=[],modal={id:'plugin-modal'};
 const context=vm.createContext({window:{ReadMDModal:{top:()=>modal,closeTop:()=>{closed.push('modal');return true;}}},state:{activeTabId:'owned-document'},closeTab:async id=>closed.push(id)});
 vm.runInContext(body,context);
 assert.equal(await context.window.__readmdNativeMenu('close-document'),true);
 assert.deepEqual(closed,['modal']);
 assert.equal(await context.window.__readmdNativeMenu('new'),false);
});
test('native document Close uses the active tab service and preserves IME composition',async()=>{
 const closed=[],context=vm.createContext({window:{},cmView:{composing:true},state:{activeTabId:'owned-document'},closeTab:async id=>closed.push(id)});
 vm.runInContext(body,context);
 assert.equal(await context.window.__readmdNativeMenu('close-document'),false);assert.deepEqual(closed,[]);
 context.cmView.composing=false;
 assert.equal(await context.window.__readmdNativeMenu('close-document'),true);assert.deepEqual(closed,['owned-document']);
 assert.equal(await context.window.__readmdNativeMenu('unregistered-command'),false);
});
test('rapid native New then Close waits for the document transition',async()=>{
 let complete;const order=[],context=vm.createContext({window:{},state:{activeTabId:'original'},newDocument:async()=>{order.push('new-start');await new Promise(r=>{complete=r;});context.state.activeTabId='new';order.push('new-ready');},closeTab:async id=>order.push('close-'+id)});
 vm.runInContext(body,context);
 const creating=context.window.__readmdNativeMenu('new'),closing=context.window.__readmdNativeMenu('close-document');
 for(let i=0;i<4;i++)await Promise.resolve();
 assert.deepEqual(order,['new-start']);complete();
 await Promise.all([creating,closing]);assert.deepEqual(order,['new-start','new-ready','close-new']);
});
test('custom titlebars retain preferences and a plain browser has no native call',()=>{
 assert.equal(run({custom_titlebar:true},false)[0].options.closeToTray,false);
 assert.deepEqual(run({custom_titlebar:false},false),[]);
});
