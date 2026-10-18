'use strict';
const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const source=fs.readFileSync(path.join(__dirname,'../../assets/js/reader/render.js'),'utf8');
const controller=source.slice(source.indexOf('window.ReadMDViewMode = (() => {'),source.indexOf('\nlet presentationOpener'));
function host(bridge){
  let zen=false;const events={},documentEvents={},calls=[],toasts=[],classes=new Set();
  const document={body:{classList:{toggle:(name,on)=>on?classes.add(name):classes.delete(name)}},documentElement:{},getElementById:()=>null,addEventListener:(name,fn)=>documentEvents[name]=fn};
  const window={addEventListener:(name,fn)=>events[name]=fn};
  if(bridge)window.pywebview={api:{toggle_native_fullscreen:async force=>{calls.push(force);return bridge(force);}}};
  const context={window,document,i18n:{t:k=>k},showToast:text=>toasts.push(text),toggleZenMode:on=>zen=on};
  vm.runInNewContext(controller,context);
  const key=(key,options={})=>{const e={key,preventDefault(){this.prevented=true;},stopImmediatePropagation(){this.stopped=true;},...options};events.keydown(e);return e;};
  return {mode:window.ReadMDViewMode,window,document,calls,toasts,classes,key,setZen:on=>zen=on,zen:()=>zen,change:()=>documentEvents.fullscreenchange()};
}
const native=force=>({ok:true,supported:true,fullscreen:force});
test('native fullscreen enables Zen, manual Zen cancellation survives repeated native state reports',async()=>{
  const h=host(native);assert.equal(await h.mode.toggleFullscreen(),true);assert.equal(h.zen(),true);
  h.setZen(false);h.mode.syncNative(true);h.mode.syncNative(true);assert.equal(h.zen(),false);assert.equal(h.mode.isFullscreen(),true);
  await h.mode.toggleFullscreen();assert.equal(h.zen(),false);assert.equal(h.mode.isFullscreen(),false);assert.deepEqual(h.calls,[true,false]);
});
test('right-click exit cancels both modes even when Zen was enabled independently',async()=>{
  const h=host(native);h.setZen(true);await h.mode.toggleFullscreen();await h.mode.toggleFullscreen();assert.equal(h.zen(),false);
  h.setZen(true);await h.mode.toggleFullscreen(false);assert.equal(h.zen(),false);assert.deepEqual(h.calls,[true,false]);
});
test('F11 is consumed once and key repeat does not toggle back; Esc leaves pure fullscreen',async()=>{
  const h=host(native);const first=h.key('F11');await new Promise(setImmediate);
  assert(first.prevented&&first.stopped);assert.equal(h.zen(),true);h.key('F11',{repeat:true});assert.deepEqual(h.calls,[true]);
  h.setZen(false);const escape=h.key('Escape');await new Promise(setImmediate);assert(escape.prevented&&escape.stopped);assert.deepEqual(h.calls,[true,false]);assert.equal(h.mode.isFullscreen(),false);
});
test('Escape during an outstanding enter request applies an explicit exit afterwards',async()=>{
  let finish;const h=host(force=>force?new Promise(resolve=>finish=()=>resolve(native(true))):native(false));
  const pending=h.mode.toggleFullscreen(true);h.key('Escape');finish();await pending;
  assert.deepEqual(h.calls,[true,false]);assert.equal(h.zen(),false);assert.equal(h.mode.isFullscreen(),false);
});
test('native or browser refusal does not fake fullscreen or remove pre-existing Zen',async()=>{
  for(const bridge of [()=>({ok:false,supported:true}),()=>({ok:false,supported:false})]){
    const h=host(bridge);h.setZen(true);assert.equal(await h.mode.toggleFullscreen(),false);assert.equal(h.mode.isFullscreen(),false);assert.equal(h.zen(),true);assert.deepEqual(h.toasts,['toast.fullscreenFailed']);
  }
});
test('browser fullscreen events synchronize external exit and keep manual Zen cancellation',async()=>{
  const h=host();h.document.documentElement.requestFullscreen=async()=>{h.document.fullscreenElement=h.document.documentElement;h.change();};
  h.document.exitFullscreen=async()=>{h.document.fullscreenElement=null;h.change();};
  await h.mode.toggleFullscreen();assert.equal(h.zen(),true);h.setZen(false);h.change();assert.equal(h.zen(),false);
  h.setZen(true);h.document.fullscreenElement=null;h.change();assert.equal(h.zen(),false);assert.equal(h.mode.isFullscreen(),false);
});
test('IME Escape is left to composition and ordinary Escape is left to existing layers',()=>{
  const h=host(native);assert(!h.key('Escape').stopped);assert(!h.key('F11',{isComposing:true}).stopped);assert.deepEqual(h.calls,[]);
});

// Exercise the native bridge actually injected by the Rust host. A successful
// postMessage is not a successful fullscreen transition.
function nativeBridge(throwOnPost=false){
  const rust=fs.readFileSync(path.join(__dirname,'../../rust/readmd-kernel/src/main.rs'),'utf8');
  const sync=rust.slice(rust.indexOf('var nativeFullscreen = false'),rust.indexOf('var webSequence = 0'));
  const method=rust.slice(rust.indexOf('toggle_native_fullscreen: function(force)'),rust.indexOf('request_quit: function()'));
  const sent=[],timers=new Map();let sequence=0;
  const window={ipc:{postMessage:message=>{if(throwOnPost)throw Error('closed');sent.push(message);}}};
  vm.runInNewContext(sync+'window.pywebview={api:{'+method+'}};',{
    window,setTimeout:fn=>{timers.set(++sequence,fn);return sequence;},clearTimeout:id=>timers.delete(id)
  });
  return {window,sent,timers,toggle:window.pywebview.api.toggle_native_fullscreen,state:window.__readmdNativeFullscreenState};
}
test('native bridge waits for the host and coalesces identical requests',async()=>{
  const h=nativeBridge();let done=false;const pending=h.toggle(true);pending.then(()=>{done=true;});
  assert.equal(h.toggle(true),pending);assert.deepEqual(h.sent,['readmd:fullscreen-on']);assert(!h.window.__readmdNativeFullscreen);
  h.state(false);await new Promise(setImmediate);assert.equal(done,false);
  h.state(true);const result=await pending;assert.equal(result.ok,true);assert.equal(result.fullscreen,true);assert.equal(h.timers.size,0);
});
test('native bridge applies a reverse request after the first host acknowledgement',async()=>{
  const h=nativeBridge();const enter=h.toggle(true),exit=h.toggle(false);
  h.state(true);await enter;await new Promise(setImmediate);assert.deepEqual(h.sent,['readmd:fullscreen-on','readmd:fullscreen-off']);
  h.state(false);assert.equal((await exit).fullscreen,false);assert.equal(h.timers.size,0);
});
test('native bridge IPC failure and missing acknowledgement never invent fullscreen state',async()=>{
  const failed=nativeBridge(true);assert.equal((await failed.toggle(true)).ok,false);assert(!failed.window.__readmdNativeFullscreen);assert.equal(failed.timers.size,0);
  const stalled=nativeBridge();const pending=stalled.toggle(true);for(const expire of stalled.timers.values())expire();
  assert.equal((await pending).ok,false);assert(!stalled.window.__readmdNativeFullscreen);assert.equal(stalled.timers.size,0);
});
