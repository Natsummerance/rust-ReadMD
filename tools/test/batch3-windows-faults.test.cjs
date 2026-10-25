'use strict';
const {test}=require('node:test'),assert=require('node:assert/strict'),os=require('node:os'),path=require('node:path');
const qa=require('./batch3-windows-faults.cjs');
const upgrade=path.resolve(__dirname,'../..','.cache/capability-upgrade');

test('QA Python uses separate scratch without changing the product environment or PATH',()=>{
 const engine={PATH:'C:\\Windows\\System32',TEMP:'long-product-temp',TMP:'long-product-temp',TMPDIR:'long-product-temp',READMD_DATA_DIR:'long-product-profile'};
 const before={...engine},temporary=path.join(upgrade,'batch3-faults-native-test','oracle-temp');
 const oracle=qa.oracleEnvironment(engine,temporary);
 assert.deepEqual(engine,before);
 assert.deepEqual(oracle,{...before,TEMP:temporary,TMP:temporary,TMPDIR:temporary});
 assert.throws(()=>qa.oracleEnvironment(engine,'relative'));
});

test('plan is detached, bounded and advertises the exact four serial phases',()=>{
 const p=qa.planFor();
 assert.equal(p.noProcessesStarted,true);assert.equal(p.noFilesWritten,true);
 assert.deepEqual(p.phases.map(row=>row.label),['typst-normal','typst-owner-crash','ocr-normal','ocr-owner-crash']);
 for(const row of p.phases){assert.equal(row.engine,row.label.startsWith('typst')?'typst':'ocr');}
 assert.deepEqual(p.crashDirection,{typst:'mcp',ocr:'http',configurable:false});
 assert.equal(p.limits.totalMs,480000);assert.equal(p.limits.phaseMs,90000);assert.equal(p.limits.prepareMs,90000);
 assert.equal(p.limits.cleanupMs,30000);assert.equal(p.limits.barrierMs,20000);
 assert.equal(p.limits.busyMs,200);assert.equal(p.limits.cancelAckMs,200);assert.equal(p.limits.pingMs,200);
 assert.equal(p.limits.retirementMs,2000);assert.equal(p.limits.stoppedMs,2000);assert.equal(p.limits.suspendMaxMs,60000);
 assert.equal(p.limits.installedBytes,128*1024*1024);assert.equal(p.limits.scopeBytes,256*1024*1024);assert.equal(p.limits.smallEvidenceBytes,32*1024*1024);
});

test('every phase carries the fixed transport assignment table',()=>{
 const p=qa.planFor();
 for(const row of p.phases){const t=row.transports;
  assert.deepEqual(t.twoOwners,['mcp','http']);assert.deepEqual(t.thirdRequest,['mcp','http']);assert.equal(t.newOwner,'mcp');}
 for(const label of ['typst-normal','typst-owner-crash','ocr-normal'])assert.equal(qa.caseFor(label).phase.transports.siblingClose,'http','Phases 1-3 must close the surviving HTTP sibling via task cancel');
 assert.equal(qa.caseFor('ocr-owner-crash').phase.transports.siblingClose,'mcp','Phase 4 has no surviving HTTP owner');
 assert.equal(qa.transportsFor(qa.PHASES[3]).phase4HasSurvivingHttpOwner,false);
});

test('plan is a pure function: mutating a returned plan cannot poison later plans',()=>{
 const first=qa.planFor();first.phases[0].label='changed';first.packs[0].id='changed';first.limits.busyMs=99999;first.lockedFiles.length=0;
 const second=qa.planFor();
 assert.equal(second.phases[0].label,'typst-normal');assert.equal(second.packs[0].id,'fonts');assert.equal(second.limits.busyMs,200);
 assert.equal(second.lockedFiles.length,qa.FILES.length,'Frozen list is rebuilt, not reused');
});

test('three pinned packs and the frozen file list are declared completely',()=>{
 const p=qa.planFor();
 assert.equal(p.packs.length,3);
 for(const pack of p.packs){assert(/^[0-9a-f]{64}$/.test(pack.sha256));assert(pack.name.endsWith('.readmd-pack'));assert(Number.isInteger(pack.bytes)&&pack.bytes>0);}
 assert.equal(new Set(p.packs.map(row=>row.id)).size,3);
 assert(p.packs.reduce((n,row)=>n+row.bytes,0)<p.limits.installedBytes,'Compressed packs already exceed the install cap');
 assert.equal(p.installedBytesLimit,128*1024*1024);assert.equal(p.installedBytesMeasuredAtInstall,true);
 for(const name of ['tools/test/batch3-windows-faults.cjs','tools/test/inspect-windows-faults.py','rust/readmd-kernel/src/capability_process.rs'])assert(p.lockedFiles.includes(name),'Frozen list missing '+name);
 // Both observers are frozen inputs as well as individually recorded digests.
 assert.deepEqual(p.lockedFiles.filter(name=>name.startsWith('ui-tests/')),
  ['ui-tests/helpers/windows-worker-observer.cs','ui-tests/helpers/windows-worker-observer.ps1']);
 assert(p.lockedFiles.includes('rust/readmd-kernel/src/capability_packs/space.rs'));
 assert.deepEqual(qa.OBSERVER_FILES,['ui-tests/helpers/windows-worker-observer.cs','ui-tests/helpers/windows-worker-observer.ps1']);
 assert.equal(qa.planFor().observer.qaRootPrefix,'batch3-faults-native-');
});

test('arguments reject unknown flags, unknown cases and out-of-scope outputs',()=>{
 assert.throws(()=>qa.argumentsFor(['--profile','n2e7Ug'],{}),/Unknown/);
 assert.throws(()=>qa.argumentsFor(['--case','bogus'],{}),/Unknown case bogus/);
 assert.throws(()=>qa.argumentsFor([],{}),/Required/);
 assert.throws(()=>qa.argumentsFor(['--plan','--output',os.tmpdir()],{}),/Outside/);
 const insideUpgrade=path.join(upgrade,'batch3-faults-native-demo');
 const ok=qa.argumentsFor(['--plan','--output',insideUpgrade],{});
 assert.equal(ok.output,insideUpgrade);
 assert.throws(()=>qa.argumentsFor(['--plan','--output',path.join(upgrade,'nested','batch3-faults-native-demo')],{}),/direct child/);
 assert.throws(()=>qa.argumentsFor(['--plan','--output',path.join(upgrade,'resource-wait-demo')],{}),/batch3-faults-native-/);
 assert.deepEqual(qa.argumentsFor(['--case','ocr-normal'],{}),{candidate:undefined,python:undefined,case:'ocr-normal'});
 // A case-only invocation is planning: it must not require execution inputs.
 const caseOnly=qa.argumentsFor(['--case','typst-normal'],{});
 assert(!caseOnly.plan&&caseOnly.case==='typst-normal');
});

test('--case only plans one phase and never claims execution',()=>{
 const value=qa.caseFor('typst-owner-crash');
 assert.equal(value.noProcessesStarted,true);assert.equal(value.noFilesWritten,true);
 assert.equal(value.label,'typst-owner-crash');assert.equal(value.phase.ending,'crash');
 assert.equal(value.phase.crashTransport,'mcp');assert.equal(value.crashDirection,'mcp');
 assert.deepEqual(value.phase.leases,['typst','fonts']);
 assert.equal(value.limits.phaseMs,90000);assert.equal(value.packs.length,3);
 assert.throws(()=>qa.caseFor('no-such-phase'),/Unknown case/);
});

test('lease validation accepts only winerror 32 on the zero-byte lock identity',()=>{
 const good={resource:'typst',held:true,exclusiveWinError:32,identityBytes:0,identity:{volume:'12345678',index:'0011223344556677',created:'0123456789abcdef',bytes:0}};
 assert(qa.validateLease(good,'typst'));
 for(const patch of [{resource:'fonts'},{held:false},{exclusiveWinError:5},{exclusiveWinError:0},{identityBytes:1},{identity:{volume:'1',index:'',created:'2',bytes:0}},{identity:{volume:'1',index:'a',created:'b',bytes:1}}])
  assert.throws(()=>qa.validateLease({...good,...patch},good.resource),JSON.stringify(patch));
 assert.throws(()=>qa.validateLease(good,'ocr'),'Lease resource must match');
});

test('per-engine lease sets: typst holds two shared leases, ocr exactly one',()=>{
 assert.deepEqual(qa.leaseResourcesFor('typst'),['typst','fonts']);
 assert.deepEqual(qa.leaseResourcesFor('ocr'),['ocr']);
 assert(!qa.leaseResourcesFor('ocr').includes('fonts'),'The OCR path never takes the fonts lease');
});

test('busy validation rejects latency over 200ms and wrong codes through both transports',()=>{
 const mcp={isError:true,content:[{type:'text',text:'{"error_code":"typst_busy"}'}]};
 assert(qa.validateMcpBusy(mcp,'typst',200));
 assert.throws(()=>qa.validateMcpBusy(mcp,'typst',200.001));
 assert.throws(()=>qa.validateMcpBusy(mcp,'ocr',1),'Wrong engine code');
 assert.throws(()=>qa.validateMcpBusy({isError:false,content:[]},'typst',1),'Busy must be a typed error');
 const http={status:200,body:{ok:false,error_code:'typst_busy'},elapsedMs:199};
 assert(qa.validateHttpBusy(http,'typst'));
 assert.throws(()=>qa.validateHttpBusy({...http,elapsedMs:201},'typst'));
 const ocr={status:409,body:{ok:false,error_code:'ocr_busy'},elapsedMs:1};
 assert(qa.validateHttpBusy(ocr,'ocr'));
 assert.throws(()=>qa.validateHttpBusy({...ocr,status:200},'ocr'),'OCR busy is 409');
 assert.throws(()=>qa.validateHttpBusy({...ocr,body:{error_code:'ocr_timeout'}},'ocr'));
});

test('job accounting must be unchanged across the busy window and positively calibrated later',()=>{
 const armed={totalProcesses:1,activeProcesses:1,ownerAlive:true};
 const engine={totalProcesses:2,activeProcesses:2,ownerAlive:true};
 assert(qa.validateAccountingUnchanged(engine,{...engine}));
 assert.throws(()=>qa.validateAccountingUnchanged(engine,{...engine,totalProcesses:3}),'A short-lived version probe must be caught by the cumulative total');
 assert.throws(()=>qa.validateAccountingUnchanged(engine,{...engine,ownerAlive:false}));
 assert(qa.validateAccountingGrowth(armed,engine));
 assert.throws(()=>qa.validateAccountingGrowth(armed,{...armed}),'No real engine subprocess calibrated the counter');
 assert.throws(()=>qa.validateAccountingGrowth(armed,{...engine,ownerAlive:false}));
});

test('resume releases only its own suspension and keeps the pinned handle',()=>{
 assert(qa.validateResume({resumed:4,processHandleKept:true}));
 assert.throws(()=>qa.validateResume({resumed:4,processHandleKept:false}),'Losing the handle breaks stopped/alive');
 assert.throws(()=>qa.validateResume({resumed:0,processHandleKept:true}),'Nothing was actually suspended');
 assert.throws(()=>qa.validateResume({resumed:'4',processHandleKept:true}));
});

test('worker stop must be observed within two seconds',()=>{
 assert(qa.validateStopped({stopped:true,waitMs:2000}));
 for(const bad of [{stopped:false,waitMs:100},{stopped:true,waitMs:2001},{stopped:true,waitMs:-1}])assert.throws(()=>qa.validateStopped(bad));
});

test('observation must be a genuine frozen engine with stable bytes',()=>{
 const good={compileOrRecognition:true,physicalSuspension:true,genuineEngineSha256:'ab'.repeat(32),threads:6};
 assert(qa.validateObserve(good,'ab'.repeat(32)));
 assert.throws(()=>qa.validateObserve(good,'cd'.repeat(32)));
 assert.throws(()=>qa.validateObserve({...good,compileOrRecognition:false},'ab'.repeat(32)));
 assert.throws(()=>qa.validateObserve({...good,physicalSuspension:false},'ab'.repeat(32)));
 assert.throws(()=>qa.validateObserve({...good,threads:0},'ab'.repeat(32)));
});

test('HTTP cancel ack is the registered cancelling reply within 200ms',()=>{
 assert(qa.validateCancelAck({status:200,body:{ok:true,state:'cancelling'},elapsedMs:200}));
 for(const bad of [{status:404,body:{ok:true,state:'cancelling'},elapsedMs:1},{status:200,body:{ok:true,state:'unknown'},elapsedMs:1},{status:200,body:{ok:true,state:'finished'},elapsedMs:1},{status:200,body:{ok:false,state:'cancelling'},elapsedMs:1},{status:200,body:{ok:true,state:'cancelling'},elapsedMs:200.5}])
  assert.throws(()=>qa.validateCancelAck(bad),JSON.stringify(bad));
});

test('cancelled finals anchor status plus error_code, never the canceled boolean',()=>{
 const exportFinal={status:200,body:{ok:false,error_code:'cancelled',canceled:false}};
 assert(qa.validateCancelledFinal(exportFinal,'typst'),'export final is HTTP 200 + cancelled');
 assert(qa.validateCancelledFinal({...exportFinal,body:{...exportFinal.body,canceled:true}},'typst'),'canceled=true alone is not the pass basis either');
 const ocrFinal={status:422,body:{ok:false,error_code:'cancelled',canceled:false}};
 assert(qa.validateCancelledFinal(ocrFinal,'ocr'),'OCR final is HTTP 422 + cancelled');
 for(const bad of [{status:200,body:{ok:false,error_code:'cancelled'}},{status:500,body:{ok:false,error_code:'cancelled'}},{status:422,body:{ok:false,error_code:'ocr_timeout'}},{status:422,body:{ok:true,error_code:'cancelled'}}])
  assert.throws(()=>qa.validateCancelledFinal(bad,'ocr'),JSON.stringify(bad));
 assert.throws(()=>qa.validateCancelledFinal({status:200,body:{ok:false,error_code:'cancelled'}},'ocr'),'OCR must not pass with the export status');
});

test('crash shape: failed report, non-zero host exit, kept instance.json, no harness deletion',()=>{
 const good={passed:false,hostExitCode:1,instanceRemoved:false,error:'AssertionError: Owned HTTP host exited before shutdown',forcedStop:false};
 assert(qa.validateCrashReport(good));
 for(const bad of [{...good,passed:true},{...good,hostExitCode:0},{...good,hostExitCode:null},{...good,instanceRemoved:true},{...good,error:'other failure'}])
  assert.throws(()=>qa.validateCrashReport(bad),JSON.stringify(bad));
 const p=qa.planFor();
 assert(p.limitations.some(row=>row.includes('broker non-zero exit is not a harness failure')));
 assert(p.limitations.some(row=>row.includes('No instance.json is ever deleted')));
});

test('materials carry unique visible markers and the oracle-recognized link',()=>{
 const a=qa.sourceFor('typst-normal'),b=qa.sourceFor('ocr-owner-crash'),l=qa.longSource('typst-normal');
 assert.notEqual(a.content,b.content);assert(a.content.includes(a.marker));assert(b.content.includes(b.marker));
 assert(a.content.includes('https://example.invalid/resource-wait'));assert(l.content.includes('https://example.invalid/resource-wait'));
 assert.equal((l.content.match(/## 故障研究 /g)||[]).length,1200,'Barrier document must stay in the 1200-paragraph range');
 const args=qa.exportArgs(a.content,'C:/owned/out.pdf','C:/owned/documents');
 assert.equal(args.output_format,'pdf');assert.equal(args.options.pdf.engine,'typst');assert.equal(args.overwrite,true);assert.equal(args.confirm,true);
 const body=qa.httpBody(a.content,'C:/owned/out.pdf','C:/owned/documents','owned-id');
 assert.equal(body.task_id,'owned-id');assert.equal(body.out_path,'C:/owned/out.pdf');assert.equal(body.format,'pdf');
});

test('cancellation adapter sends the exact original id and invents no acknowledgement',async()=>{
 const sent=[],pending=new Map();
 const fake={pending,transport:{fail:error=>{throw error;}},info:{requests:0},send:message=>sent.push(message)};
 const running=qa.CancellableClient.prototype.begin.call(fake,'readmd_export_document',{confirm:true},'owned-fault-id',1000);
 qa.CancellableClient.prototype.cancel.call(fake,'owned-fault-id');
 await assert.rejects(running,{name:'AbortError'});
 assert.equal(pending.size,0);
 assert.deepEqual(sent.map(row=>row.id??row.params.requestId),['owned-fault-id','owned-fault-id']);
 assert.equal(sent[1].method,'notifications/cancelled');
 assert(!Object.hasOwn(sent[1],'id'),'Notifications never carry a request id');
 assert.throws(()=>qa.CancellableClient.prototype.cancel.call(fake,'foreign-id'));
});

test('inside() rejects traversal and absolute escapes',()=>{
 assert.throws(()=>qa.inside('C:/owned','C:/owned/../foreign'));
 assert.throws(()=>qa.inside('C:/owned','D:/foreign'));
 assert.equal(qa.inside('C:/owned','C:/owned/child/file.txt'),path.resolve('C:/owned/child/file.txt'));
});

test('until() reports its deadline label and stops in time',async()=>{
 const t0=Date.now();
 await assert.rejects(qa.until(async()=>false,'owned-label',60),/owned-label/);
 assert(Date.now()-t0<2000);
 let attempts=0;const value=await qa.until(async()=>++attempts>=3?42:null,'poll-label',5000);
 assert.equal(value,42);assert.equal(attempts,3);
});
