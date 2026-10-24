'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),path=require('node:path');
const {execFileSync}=require('node:child_process');
const {within,argumentsFor,assertRecovered,assertReplacementIdentity,verificationCases,STAGES}=require('./task-workspace-retirement.cjs');

test('owned fixture scope rejects ancestors, siblings, other volumes and relative inputs',()=>{
  const base=path.resolve('retirement-owned-fixture');
  assert.equal(within(base,path.join(base,'profile','known.txt')),path.join(base,'profile','known.txt'));
  for(const file of [base,path.dirname(base),path.resolve(base,'..','retirement-owned-fixture-foreign','x')])assert.throws(()=>within(base,file),/escapes/);
  assert.throws(()=>within('relative',path.join(base,'x')),/absolute/);
  if(process.platform==='win32')assert.throws(()=>within('C:\\fixture','D:\\fixture\\x'),/escapes/);
});
test('Windows canonical namespace spellings preserve strict owned-scope boundaries',()=>{
  if(process.platform!=='win32')return;
  const base=path.resolve('retirement-namespace-owned'),child=path.join(base,'profile','unknown.txt');
  const extendedBase=path.toNamespacedPath(base),extendedChild=path.toNamespacedPath(child);
  assert.equal(within(base,extendedChild),extendedChild);
  assert.equal(within(extendedBase,child),child);
  assert.equal(within(extendedBase,extendedChild),extendedChild);
  const sibling=path.resolve(base,'..','retirement-namespace-owned-foreign','unknown.txt');
  const escaped=path.join(extendedBase,'..','outside','unknown.txt');
  for(const root of [base,extendedBase]){
    for(const target of [base,extendedBase,sibling,path.toNamespacedPath(sibling),escaped,path.dirname(base),path.toNamespacedPath(path.dirname(base))])assert.throws(()=>within(root,target),/escapes/);
    assert.throws(()=>within(root,'\\\\?\\GLOBALROOT\\Device\\HarddiskVolume1\\outside'),/escapes/);
  }
  const unc='\\\\owned-server\\owned-share\\fixture',uncChild=path.join(unc,'profile','known.txt');
  assert.equal(within(unc,path.toNamespacedPath(uncChild)),path.toNamespacedPath(uncChild));
  assert.equal(within(path.toNamespacedPath(unc),uncChild),uncChild);
  for(const target of ['\\\\owned-server\\owned-share\\foreign\\x','\\\\owned-server\\other-share\\fixture\\x'])assert.throws(()=>within(path.toNamespacedPath(unc),path.toNamespacedPath(target)),/escapes/);
});
test('CLI refuses unknown modes and missing baseline bindings',()=>{
  assert.throws(()=>argumentsFor(['--mode','pretend','--binary','x','--output','y']),/mode/);
  assert.throws(()=>argumentsFor(['--mode','baseline','--binary','x','--output','y']),/app/);
  assert.throws(()=>argumentsFor(['--mode','baseline','--binary','x','--output','y','--app','z']),/candidate/);
  assert.throws(()=>argumentsFor(['--binary']),/Missing/);
  assert.throws(()=>argumentsFor(['--force']),/Unknown/);
});
test('native recovery acceptance rejects duplicated reservations, skipped visits and failed recovery',()=>{
  const id='a'.repeat(32);
  const good={before:[id],reservedBefore:256*1024*1024,report:{visited:1,removed:1,running:0,retained:0,preserved:0,failed:0},after:[]};
  assert.doesNotThrow(()=>assertRecovered(good,id));
  for(const bad of [
    {...good,before:[id,id]},
    {...good,reservedBefore:512*1024*1024},
    {...good,report:{...good.report,visited:0}},
    {...good,report:{...good.report,failed:1}},
    {...good,report:{...good.report,preserved:1}},
    {...good,after:[id]},
  ])assert.throws(()=>assertRecovered(bad,id));
  assert.equal(STAGES.length,5);
});
test('same-byte replacement acceptance requires a distinct file identity',()=>{
  const original={dev:'1',ino:'100',created:'1000',bytes:'23',sha256:'a'.repeat(64)};
  assert.doesNotThrow(()=>assertReplacementIdentity(original,{...original,ino:'101'}));
  assert.throws(()=>assertReplacementIdentity(original,{...original}),/identity/);
  assert.throws(()=>assertReplacementIdentity(original,{...original,ino:'101',bytes:'24'}),/length/);
  assert.throws(()=>assertReplacementIdentity(original,{...original,ino:'101',sha256:'b'.repeat(64)}),/bytes/);
});
test('read-only plan lists ten real-kill scenarios including both direct identity replacements',()=>{
  assert.deepEqual(argumentsFor(['--plan']),{mode:'verify',plan:true});
  assert.throws(()=>argumentsFor(['--plan','--mode','baseline']),/verification/);
  const cases=verificationCases();assert.equal(cases.length,10);assert.equal(new Set(cases.map(item=>item.label)).size,10);
  assert.deepEqual(cases.filter(item=>item.kind==='leaf-replacement').map(item=>[item.stage,item.leaf]),[
    ['WitnessLinked','owner.lock'],['WitnessLinked','record.jsonl'],
  ]);
  const planned=JSON.parse(execFileSync(process.execPath,[path.join(__dirname,'task-workspace-retirement.cjs'),'--plan'],{encoding:'utf8',windowsHide:true}));
  assert.equal(planned.nativeVerificationRun,false);assert.equal(planned.automaticRetries,0);assert.equal(planned.computerUse,false);assert.equal(planned.caseCount,10);assert.deepEqual(planned.cases,cases);
});
