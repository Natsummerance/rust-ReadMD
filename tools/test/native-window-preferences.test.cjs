'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const root=path.resolve(__dirname,'../..'),source=fs.readFileSync(path.join(root,'assets/js/shell/shell.js'),'utf8');
const body=source.slice(source.indexOf('function syncWindowPreferences()'),source.indexOf('function initWindowChrome()'));
function run(py,closeToTray){
 const calls=[],en=JSON.parse(fs.readFileSync(path.join(root,'assets/i18n/en.json'),'utf8'));
 const context=vm.createContext({hasPy:true,py:{...py,window_control:(action,options)=>calls.push({action,options:JSON.parse(JSON.stringify(options))})},state:{closeToTray},window:{i18n:{t:key=>en[key]}},$:()=>null});
 vm.runInContext(body+'\nsyncWindowPreferences();',context);return calls;
}
test('native Mac titlebars still apply close-to-menu-bar and translated menu preferences',()=>{
 const [call]=run({custom_titlebar:false,native_titlebar:'macos'},false);
 assert.equal(call.action,'preferences');assert.equal(call.options.closeToTray,false);
 assert.deepEqual(call.options.fileLabels,['File','Save','Save As']);
 assert.deepEqual(call.options.editLabels,['Editor','Undo','Redo','Cut','Copy','Paste']);
 assert.equal(call.options.menuLabels.hide,'Hide ReadMD');
 assert.equal(call.options.menuLabels['select-all'],'Select All');
 assert.equal(call.options.menuLabels.fullscreen,'Fullscreen');
 assert.equal(run({custom_titlebar:false,native_titlebar:'macos'},true)[0].options.closeToTray,true);
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
test('custom titlebars retain preferences and a plain browser has no native call',()=>{
 assert.equal(run({custom_titlebar:true},false)[0].options.closeToTray,false);
 assert.deepEqual(run({custom_titlebar:false},false),[]);
});
