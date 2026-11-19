'use strict';
// Exercise tab close with a persisted external-change marker and the real Rust CAS.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {openOwned} = require('../tools/test/owned-browser-host.cjs');

const [binary, evidence, playwright] = process.argv.slice(2);
assert(path.isAbsolute(binary) && path.isAbsolute(evidence));
fs.mkdirSync(evidence, {recursive:true});

async function run(divergent) {
  let target, expected;
  const kind = divergent ? 'divergent' : 'matching';
  const h = await openOwned(path.join(evidence, kind), async (api, profile) => {
    const rows = [];
    for (let i = 0; i < 4; i++) {
      const virtual = i === 0 || i === 3;
      const name = '恢复测试 ' + i + '.md';
      const file = virtual ? '' : path.join(profile.documents, name);
      const key = (virtual ? 'draft:' : 'file:') + 'external-close-' + i;
      const text = (i === 1 ? '#测试 ' : '# 测试 ') + i + '\n\n原件。\n';
      rows.push({key, path:file, name, dir:profile.documents, source:virtual?'recovery':'file',
        originPath:file, reviewPath:virtual?key:'', dirty:false, unsavedCreation:false});
      if (virtual) {
        const review = await api('/api/review', {action:'open', path:key, content:text});
        const mark = Object.fromEntries(['document_id','generation','review_revision','revision'].map(key => [key, review[key]]));
        await api('/api/documents/history', {op:'record', key, path:'', name, kind:'draft',
          reason:'edit', content:text, context:{dir:profile.documents, review:mark}});
        continue;
      }
      fs.writeFileSync(file, text, {flag:'wx'});
      if (i !== 1) continue;
      target = file;
      const reader = await api('/api/file?p='+encodeURIComponent(file));
      assert.equal(reader.original,text);
      assert.notEqual(reader.content,text);
      const review = await api('/api/review', {action:'open', path:file});
      const inserted = '在其他编辑器保存。\n';
      const edit = await api('/api/review', {action:'edit', document_id:review.document_id,
        generation:review.generation, review_revision:review.review_revision, origin:'editor',
        call_id:'external-close-edit', edits:[{from:text.length,to:text.length,insert:inserted}]});
      assert.equal(edit.content,text+inserted);
      expected = divergent ? edit.content+'外部内容有变化。\n' : edit.content;
      fs.writeFileSync(file,expected);
    }
    const session = await api('/api/session');
    const result = await api('/api/session', {session:{version:1,tabs:rows,active:rows.at(-1).key,folder:''},
      expected_session_version:session.session_version,call_id:'external-close-fixture'});
    assert.equal(result.persisted,true);
  }, {binary, playwright});
  try {
    const initial = await h.api('/api/review', {action:'resume',path:target});
    assert.equal(initial.snapshot.recovery.reason,'external_change');
    const counts = await h.page.evaluate(async () => {
      const sizes = [state.tabs.length];
      for (let i = 0; i < 4; i++) {
        await closeTab(state.activeTabId, false, 'button');
        sizes.push(state.tabs.length);
      }
      return sizes;
    });
    assert.deepEqual(counts,divergent?[4,4,4,4,4]:[4,3,2,1,0]);
    assert.equal((await h.api('/api/session')).session.tabs.length,counts.at(-1));
    assert.equal((await h.api('/api/documents/history')).entries.filter(entry=>entry.kind==='draft').length,2);
    const stored = await h.api('/api/review', {action:'resume',path:target});
    assert.deepEqual(stored.snapshot,initial.snapshot);
    assert.equal(fs.readFileSync(target,'utf8'),expected);
    assert.deepEqual(h.errors,[]);
    return {kind,counts,reviewUnchanged:true,fileUnchanged:true,draftsRetained:true};
  } finally { await h.close(); }
}

(async()=>{
  const result = [await run(false),await run(true)];
  fs.writeFileSync(path.join(evidence,'report.json'),JSON.stringify(result,null,2)+'\n',{flag:'wx'});
  console.log(JSON.stringify(result));
})().catch(error=>{console.error(error);process.exitCode=1});
