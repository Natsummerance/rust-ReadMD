'use strict';
// Deterministic VM/IO controls only. No App, native input, Swift, port or model.
const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),zlib=require('node:zlib');
const repo=path.resolve(__dirname,'..'),workspace=path.dirname(repo),harness=path.join(__dirname,'macos-shell-system-native.cjs');
const source=fs.readFileSync(harness,'utf8'),csp="default-src 'self'; script-src 'self'; object-src 'none'";
const evidence=process.env.READMD_SHELL_VM_EVIDENCE||path.join(workspace,'.readmd-local','shell-native-vm');
assert(path.isAbsolute(evidence)&&path.resolve(evidence).startsWith(workspace+path.sep),'Owned workspace VM evidence required');
fs.mkdirSync(evidence,{recursive:true,mode:0o700});
function fresh(label){const dir=fs.mkdtempSync(path.join(evidence,label+'-'));fs.writeFileSync(path.join(dir,'VM_ONLY_NOT_NATIVE.txt'),'Only controlled VM/IO doubles; no real application or service.\n');return dir;}
function load(host,clock){
 const module={exports:{}},req=name=>name==='./macos-native-system-host.cjs'?host:/^(?:node:)?child_process$/.test(name)?new Proxy({},{get(){return()=>{throw Error('Native subprocess forbidden in VM');};}}):require(name);
 req.main={};req.resolve=name=>name==='./macos-native-system-host.cjs'?require.resolve('./macos-native-system-host.cjs'):require.resolve(name);
 const fakeProcess={platform:'darwin'};
 vm.runInNewContext(source,{require:req,module,exports:module.exports,__dirname,__filename:harness,process:fakeProcess,console:{log(){},error(){}},Buffer,Date:clock?class extends Date{static now(){return clock.now;}}:Date,setTimeout:clock?(fn,ms)=>{clock.now+=Math.max(1000,ms);return setImmediate(fn);}:setTimeout,clearTimeout},{filename:harness});
 return module.exports;
}
const shell=load({prepare(){throw Error('Native preparation forbidden');}});
function predicate(script,label,call){const at=script.indexOf("'"+label+"'");assert(at>=0);const start=script.lastIndexOf(call+'(',at);assert(start>=0);return script.slice(start+call.length+1,at).replace(/,\s*$/,'').trim();}
function editorContext(){
 let body=shell.B;
 const context=vm.createContext({state:{original:'\ufeff'+shell.B.replaceAll('\n','\r\n')},cmView:{state:{doc:{toString:()=>body}}},config:{b:shell.B,addition:shell.ADDITION},getActiveTab:()=>({isDirty:false}),text:value=>String(value??'').replace(/^\uFEFF/,'').replace(/\r\n?/g,'\n')});
 const preview=fs.readFileSync(path.join(repo,'assets/js/editor/preview.js'),'utf8'),begin=preview.indexOf('function getEditContent()'),end=preview.indexOf('\nfunction setPvLayout(',begin);assert(begin>=0&&end>begin);vm.runInContext(preview.slice(begin,end),context);
 return{context,append:()=>{body+=shell.ADDITION;}};
}
function png(){
 const crc=bytes=>{let c=0xffffffff;for(const byte of bytes){c^=byte;for(let bit=0;bit<8;bit++)c=c&1?0xedb88320^(c>>>1):c>>>1;}return(c^0xffffffff)>>>0;};
 const chunk=(name,bytes)=>{const type=Buffer.from(name),head=Buffer.alloc(4),tail=Buffer.alloc(4);head.writeUInt32BE(bytes.length);tail.writeUInt32BE(crc(Buffer.concat([type,bytes])));return Buffer.concat([head,type,bytes,tail]);};
 const header=Buffer.alloc(13);header.writeUInt32BE(3840);header.writeUInt32BE(1,4);header[8]=8;header[9]=6;
 return Buffer.concat([Buffer.from('89504e470d0a1a0a','hex'),chunk('IHDR',header),chunk('IDAT',zlib.deflateSync(Buffer.alloc(3840*4+1))),chunk('IEND',Buffer.alloc(0))]);
}
async function closedDriver(quit,{script=shell.driver('closed_stays_closed',{a:'/owned/a.md',b:'/owned/b.md',fix:'/owned/fix.md',ack:'/owned/ack.txt'},{csp,cases:shell.cases,a:shell.A,b:shell.B,fix:shell.FIX,addition:shell.ADDITION}),profile=fresh('terminal'),onRaw=()=>{},dropTerminal=false}={}){
 const b={id:'b',path:'/owned/b.md',content:shell.B+shell.ADDITION,isDirty:false},fix={id:'fix',path:'/owned/fix.md'},messages=[];
 const context={__readmdSessionReady:Promise.resolve(),hasPy:true,URL,Date,setTimeout,clearTimeout,editorPrefs:{aiCompletion:false},state:{tabs:[b,fix],activeTabId:'b'},getActiveTab:()=>b,
  document:{body:{classList:{contains:key=>['native-macos','shell-single-chrome'].includes(key)}},querySelector:key=>key==='.titlebar-mark'?{complete:true,naturalWidth:1,src:'http://owned.invalid/assets/ReadMD-macos.svg'}:{content:csp}},
  ReadMDSession:{whenIdle:async()=>{},flush:async()=>true},requestAnimationFrame:fn=>fn(),fetch:async()=>({status:403}),apiFetch:async()=>({ok:true,json:async()=>({modules:{ocr:'idle',transcribe:'idle'}})}),
  py:{native_titlebar:'macos',request_quit:quit},window:{ReadMDLifecycle:{busy:()=>false},ipc:{postMessage:message=>{
   messages.push(message);if(message==='readmd:quit'){onRaw();return;}
   const parsed=JSON.parse(message),value=parsed.readmd_native_qa_progress||parsed.readmd_native_qa_result;if(!value||dropTerminal&&parsed.readmd_native_qa_result)return;
   const file=path.join(profile,parsed.readmd_native_qa_progress?'native-qa-progress.json':'native-qa-result.json');try{fs.writeFileSync(file,JSON.stringify(value),parsed.readmd_native_qa_progress?{}:{flag:'wx'});}catch(error){if(error.code!=='EEXIST')throw error;}
  }}}};
 if(script.includes('const phase="closed_stays_closed"')){
  const files=JSON.parse(script.match(/files=(\{[^;]*?\}),config=/)[1]);b.path=files.b;fix.path=files.fix;
 }
 const running=new vm.Script(script).runInNewContext(context);
 return{running,messages,profile,terminals:()=>messages.filter(message=>message.startsWith('{')).map(JSON.parse).map(value=>value.readmd_native_qa_result).filter(Boolean)};
}

async function outer(kind){
 const directory=fresh(kind),app=path.join(directory,'MockOnly.app'),assets=path.join(directory,'assets'),output=path.join(directory,'run');fs.mkdirSync(app);fs.mkdirSync(assets);
 for(const name of ['index.html','readmd.boot.js','style.css','css/shell.css','css/panels.css','ReadMD-macos.svg']){const file=path.join(assets,name);fs.mkdirSync(path.dirname(file),{recursive:true});fs.writeFileSync(file,name==='index.html'?'<meta http-equiv="Content-Security-Policy" content="'+csp+'">':'owned VM fixture');}
 const clock={now:100000},state={alive:false,cleanupCalls:0,rawQuit:0,polls:0,waitCalls:0,phases:0,messages:[]};
 const host={prepare:()=>({binarySha256:'vm_only',helperSha256:'vm_only',bundleIdentifier:'asia.readmd.nativeqa.vm',launch:async({profile,docs})=>{
  state.alive=true;if(kind==='launch_reject')throw Error('launch_cleanup_unproved');
  const script=fs.readFileSync(path.join(profile,'native-qa.js'),'utf8'),phase=JSON.parse(script.match(/const phase=("[^"]+")/)[1]),receipt={initialAppContext:{pid:4000+(++state.phases)},forcedCleanup:false,nativeProcessGone:false};
  let frame=0;const ids=[101,102,103,104,201];
  if(!['cleanup_failure','cleanup_success','missing_receipt'].includes(kind)){
   if(phase==='shell_edit_save'){
    fs.writeFileSync(path.join(docs,'b','相同标题.md'),Buffer.from('\ufeff'+(shell.B+shell.ADDITION).replaceAll('\n','\r\n')));
    fs.mkdirSync(path.join(profile,'native-frames'));for(const id of ids)fs.writeFileSync(path.join(profile,'native-frames',String(id).padStart(5,'0')+'.png'),png());
   }
   if(phase==='closed_stays_closed'){
    const quit=async()=>{if(kind==='quit_throw')throw Error('owned_guarded_quit_failure');if(kind==='quit_false')return false;state.alive=false;return true;};
    const closed=await closedDriver(quit,{script,profile,onRaw:()=>{state.rawQuit++;state.alive=false;},dropTerminal:kind==='quit_true_missing_receipt'});await closed.running;state.messages=closed.messages;
   }else fs.writeFileSync(path.join(profile,'native-qa-result.json'),JSON.stringify({ok:true,phase,checks:['appkit_green_enters_actual_fullscreen','native_menu_enters_actual_fullscreen','production_csp_unchanged','unauthenticated_api_refused']}),{flag:'wx'});
  }
  return{receipt,pid:receipt.initialAppContext.pid,get launcherStopped(){
   state.polls++;if(['cleanup_failure','cleanup_success','missing_receipt'].includes(kind))return true;
   if(phase==='shell_edit_save'){const ack=fs.readFileSync(path.join(docs,'native-frame-ack.txt'),'utf8').trim().split('\n');if(frame<ids.length&&ack.includes(String(ids[frame])))frame++;if(frame<ids.length){fs.writeFileSync(path.join(profile,'native-qa-progress.json'),JSON.stringify({ok:true,phase:'shell_capture_'+ids[frame],checks:[]}));return false;}state.alive=false;}
   else if(phase==='reopen_close')state.alive=false;return!state.alive;
  },wait:async()=>{state.waitCalls++;if(['cleanup_failure','cleanup_success'].includes(kind))throw Error('owned_phase_failed');if(state.alive)throw Error('owned mock process still alive');receipt.nativeProcessGone=true;receipt.launcherOutcome={code:0};return receipt;},cleanup:async()=>{state.cleanupCalls++;if(kind==='cleanup_failure')throw Error('owned process did not stop during cleanup');if(state.alive)receipt.forcedCleanup=true;state.alive=false;receipt.nativeProcessGone=true;}};
 }})};
 let error;try{await load(host,clock).run(app,output,assets);}catch(failure){error=failure;}
 const report=JSON.parse(fs.readFileSync(path.join(output,'report.json'))),summary={kind,error:error?.message,state,report,tmpExists:fs.existsSync(path.join(output,'tmp')),homeExists:fs.existsSync(path.join(output,'native-home')),elapsedClockMs:clock.now-100000};
 fs.writeFileSync(path.join(directory,'vm-case-summary.json'),JSON.stringify({scope:'VM/IO only; nativeAcceptanceExecuted in raw report is a controlled-host value, never native certification',...summary,error:error?.message},null,2)+'\n');return summary;
}

test('plan and three generated scripts stay inert and parse',()=>{
 assert.equal(shell.plan().nativeAcceptanceExecuted,false);
 for(const phase of shell.phases)new vm.Script(shell.driver(phase,{a:'/owned/a.md',b:'/owned/b.md',fix:'/owned/fix.md',ack:'/owned/ack.txt'},{csp,cases:shell.cases,a:shell.A,b:shell.B,fix:shell.FIX,addition:shell.ADDITION}));
});
test('logical editor predicates accept actual BOM/CRLF getEditContent',()=>{
 const state=editorContext(),script=shell.driver('shell_edit_save',{},{}),opening=predicate(script,'editor_opens_exact_saved_source','check'),saving=predicate(script,'visible_save_did_not_settle','until');
 assert.match(state.context.getEditContent(),/\r\n/);assert.equal(vm.runInContext('('+opening+')',state.context),true);state.append();assert.match(state.context.getEditContent(),/\r\n/);assert.equal(vm.runInContext('('+saving+')()',state.context),true);
});
test('the unchanged physical verifier rejects LF-only, missing BOM and altered original bytes',()=>{
 const directory=fresh('physical-bytes'),files={a:path.join(directory,'a.md'),b:path.join(directory,'b.md'),fix:path.join(directory,'fix.md')};
 const bytes=value=>Buffer.from('\ufeff'+value.replaceAll('\n','\r\n'));for(const[key,value]of[['a',shell.A],['b',shell.B+shell.ADDITION],['fix',shell.FIX]])fs.writeFileSync(files[key],bytes(value));
 const start=source.indexOf('const verify = () => {'),end=source.indexOf('\n  async function framesReady()',start);assert(start>=0&&end>start);
 const context={fs,assert,files,A:shell.A,B:shell.B,ADDITION:shell.ADDITION,FIX:shell.FIX,encode:shell.encode,bindings:()=>({own:'asset'}),before:{own:'asset'}};vm.runInNewContext(source.slice(start,end)+'\nverify();',context);
 for(const invalid of [Buffer.from(shell.B+shell.ADDITION),Buffer.from('\ufeff'+shell.B+shell.ADDITION)]){fs.writeFileSync(files.b,invalid);assert.throws(()=>vm.runInNewContext(source.slice(start,end)+'\nverify();',context));}
 fs.writeFileSync(files.b,bytes(shell.B+shell.ADDITION));fs.writeFileSync(files.a,bytes(shell.A+'changed'));assert.throws(()=>vm.runInNewContext(source.slice(start,end)+'\nverify();',context));
});
for(const [name,quit]of[['throws',async()=>{throw Error('owned_quit_failure');}],['false',async()=>false],['undefined',async()=>undefined]])test('guarded quit '+name+' persists one false terminal and sends no raw quit',async()=>{
 const run=await closedDriver(quit);await run.running;assert.equal(run.terminals().length,1);assert.equal(run.terminals()[0].ok,false);assert.equal(JSON.parse(fs.readFileSync(path.join(run.profile,'native-qa-result.json'))).ok,false);assert(!run.messages.includes('readmd:quit'));
});
test('only strict guard true permits one success, with no terminal before acceptance',async()=>{
 let release,called=false;const accepted=new Promise(resolve=>{release=resolve;});const run=await closedDriver(async()=>{called=true;return accepted;});
 while(!called)await new Promise(setImmediate);const before=run.terminals().length;release(true);await run.running;assert.equal(before,0);assert.equal(run.terminals().length,1);assert.equal(run.terminals()[0].ok,true);assert(!run.messages.includes('readmd:quit'));
});
for(const kind of ['quit_throw','quit_false'])test('owner aborts early on legal false terminal ('+kind+')',async()=>{
 const result=await outer(kind);assert.equal(result.report.passed,false);assert(result.error);assert.equal(result.state.rawQuit,0);assert(result.state.messages.some(message=>message.startsWith('{')&&JSON.parse(message).readmd_native_qa_result?.ok===false));assert(result.state.polls<25,'Do not wait out the 180-second budget');assert(result.report.phases.at(-1).result?.ok===false);assert.equal(result.state.waitCalls,2,'The failed final phase must not enter native wait');
});
test('cleanup failure preserves both live-owner scratch directories and a failed report',async()=>{
 const result=await outer('cleanup_failure');assert(result.error);assert.equal(result.report.passed,false);assert.equal(result.state.alive,true);assert.equal(result.state.cleanupCalls,1);assert.equal(result.tmpExists,true);assert.equal(result.homeExists,true);assert(result.report.failure);assert.equal(result.report.cleanup.allOwnedTerminated,false);assert.equal(result.report.cleanup.instances[0].cleanupConfirmed,false);assert(result.report.cleanup.instances[0].error);
});
test('confirmed owner cleanup permits removing both scratch directories',async()=>{
 const result=await outer('cleanup_success');assert(result.error);assert.equal(result.state.alive,false);assert.equal(result.state.cleanupCalls,1);assert.equal(result.tmpExists,false);assert.equal(result.homeExists,false);assert.equal(result.report.cleanup.allOwnedTerminated,true);assert.equal(result.report.cleanup.instances[0].cleanupConfirmed,true);
});
test('launch rejection cannot infer successful helper cleanup and retains scratch',async()=>{
 const result=await outer('launch_reject');assert.match(result.error,/launch_cleanup_unproved/);assert.equal(result.tmpExists,true);assert.equal(result.homeExists,true);assert.equal(result.report.cleanup.allOwnedTerminated,false);assert.equal(result.report.cleanup.instances[0].instanceReturned,false);assert.equal(result.report.cleanup.instances[0].cleanupConfirmed,false);
});
test('accepted quit with a lost terminal receipt remains a failed owner run',async()=>{
 const result=await outer('quit_true_missing_receipt');assert(result.error);assert.equal(result.report.passed,false);assert.equal(result.state.rawQuit,0);assert.equal(result.state.alive,false);assert.equal(result.report.cleanup.allOwnedTerminated,true);
});
test('accepted quit and confirmed owners complete the controlled positive run',async()=>{
 const result=await outer('quit_true');assert.equal(result.error,undefined);assert.equal(result.report.passed,true);assert.equal(result.state.rawQuit,0);assert.equal(result.state.cleanupCalls,3);assert.equal(result.report.cleanup.allOwnedTerminated,true);assert(result.report.cleanup.instances.every(instance=>instance.cleanupConfirmed));assert.equal(result.tmpExists,false);assert.equal(result.homeExists,false);
});
