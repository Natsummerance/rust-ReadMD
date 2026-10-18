import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import {validate, sourceDigest} from '../check-capability-upgrade.mjs';
test('certification rejects changed evidence, changed implementations and cross-build substitutes', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'readmd-acceptance-'));
  try {
    fs.writeFileSync(path.join(root, 'implementation.rs'), 'original');
    fs.writeFileSync(path.join(root, 'proof.json'), '{"passed":true}');
    const proof = {path:'proof.json', sha256:crypto.createHash('sha256').update('{"passed":true}').digest('hex'), sourceDigest:sourceDigest(root,['implementation.rs']), kind:'native', platform:'windows-x64', outcome:'passed', recordedAt:'2026-10-08', scope:'native save and reopen'};
    const item = {id:'R001',requirement:'Save',workflow:'Edit → save → reopen',acceptance:'Real file round trip',status:'verified',implementation:['implementation.rs'],remaining:[],evidence:[proof]};
    const ledger = {schema:2,requirements:[item],platformEvidence:[{id:'macos',formal:true,status:'unavailable'}]};
    assert.deepEqual(validate(ledger,root),[]);
    assert.ok(validate(ledger,root,true).some(p=>p.includes('native platform not verified')));
    ledger.platformEvidence=[];
    assert.ok(validate(ledger,root,true).some(p=>p.includes('native platform not verified: windows-x64')));
    ledger.platformEvidence=[{id:'windows-x64',formal:false,status:'verified'}];
    assert.ok(validate(ledger,root,true).some(p=>p.includes('native platform not verified: windows-x64')));
    fs.writeFileSync(path.join(root,'proof.json'),'changed');
    assert.ok(validate(ledger,root).some(p=>p.includes('changed evidence')));
    fs.writeFileSync(path.join(root,'proof.json'),'{"passed":true}');
    fs.writeFileSync(path.join(root,'implementation.rs'),'changed');
    assert.ok(validate(ledger,root).some(p=>p.includes('stale certification')));
    item.evidence=[];
    assert.ok(validate(ledger,root).some(p=>p.includes('native workflow not verified')));
    item.evidence=[{...proof,path:'../outside.json'}];
    assert.ok(validate(ledger,root).some(p=>p.includes('missing evidence')));
  } finally { fs.rmSync(root,{recursive:true,force:true}); }
});
