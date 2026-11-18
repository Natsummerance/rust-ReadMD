'use strict';
// Exercise the same frozen-source preview and commit used by the export dialog.
const assert = require('node:assert/strict');
const {randomUUID, createHash} = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const zip = require('../../ui-tests/zip-parts.cjs');

module.exports = async function snapshotChecks({host, docs, options}) {
  const results = {};
  async function post(route, body) {
    const response = await host.api(route, {
      method: 'POST', headers: {'Content-Type':'application/json'}, body: JSON.stringify(body),
    });
    return response.json();
  }
  const review = body => post('/api/review', body);
  const snapshot = body => post('/api/export/snapshot', body);
  const source = [
    '| Group | Trial | Reading |', '| --- | --- | --- |',
    '| **North** {rowspan=2 align=right valign=bottom} | One | 3.4 |',
    '| | Two | 5.6 |', '', 'Result', '',
  ].join('\n');
  for (const [format, ext] of [['pdf','pdf'],['png','png'],['png-pages','zip']]) {
    const base = 'review-geometry-' + format;
    const file = path.join(docs, base + '.md');
    const output = path.join(docs, base + '.' + ext);
    fs.writeFileSync(file,source,{flag:'wx'});
    await host.api('/api/file?p=' + encodeURIComponent(file));
    const initial = await review({action:'open',path:file});
    assert.equal(initial.content,source);
    const pending = await review({
      action:'propose', document_id:initial.document_id, generation:initial.generation,
      review_revision:initial.review_revision, call_id:randomUUID(),origin:'ai',
      edits:[{from:source.length,to:source.length,insert:'\nReviewed result\n'}],
    });
    assert.equal(pending.changes.filter(c=>c.decision==='pending').length,1);
    const prepare = () => snapshot({
      action:'prepare',document_id:pending.document_id,generation:pending.generation,
      review_revision:pending.review_revision,call_id:randomUUID(),format,options,
    });
    const cancelled = await prepare();
    assert(cancelled.snapshot_id, 'prepare '+format+': '+cancelled.error_code);
    const original = Buffer.from('ORIGINAL TARGET');
    fs.writeFileSync(output,original,{flag:'wx'});
    const discarded = await snapshot({action:'discard',snapshot_id:cancelled.snapshot_id});
    assert.equal(discarded.discarded,true);
    const stale = await snapshot({action:'commit',snapshot_id:cancelled.snapshot_id,out_path:output,
      accept_pending:true,overwrite:true});
    assert.equal(stale.ok,false);
    assert.deepEqual(fs.readFileSync(output),original);
    const afterCancel = await review({action:'read',document_id:pending.document_id});
    assert.deepEqual(afterCancel.changes,pending.changes);
    const prepared = await prepare();
    assert(prepared.snapshot_id, 'prepare '+format+': '+prepared.error_code);
    const action = (name, fields={}) => snapshot({action:name,snapshot_id:prepared.snapshot_id,...fields});
    const preview = await post('/api/export/preview',{snapshot_id:prepared.snapshot_id});
    assert(preview.ok, 'preview '+format+': '+preview.error_code);
    assert.equal(preview.previewEngine,'rust');
    const layout = Buffer.from(preview.pdf, 'base64');
    assert.equal(layout.subarray(0,5).toString(),'%PDF-');
    assert.equal(preview.pages,1);
    // A protected target must survive and review decisions must remain pending.
    const refused = await action('commit',{out_path:output,accept_pending:true,overwrite:false});
    assert.equal(refused.error_code,'export_snapshot_commit_conflict');
    assert.deepEqual(fs.readFileSync(output),original);
    const after = await review({action:'read',document_id:pending.document_id});
    assert.equal(after.review_revision,pending.review_revision);
    assert.deepEqual(after.changes,pending.changes);
    const committed = await action('commit',{out_path:output,accept_pending:true,overwrite:true});
    assert(committed.ok,'commit '+format+': '+committed.error_code);
    assert.equal(committed.snapshot_state,'finished');
    assert.equal(committed.review.changes.find(c=>c.id===pending.changes[0].id)?.decision,'accepted');
    const bytes = fs.readFileSync(output);
    if (format === 'pdf') assert.deepEqual(bytes,layout,'PDF commit uses the actual preview artifact');
    else {
      // A cached PDF layout cannot stand in for either type of PNG artifact.
      const image = format === 'png' ? bytes : zip(bytes,null).get('page-001.png');
      assert(image && image.subarray(0,8).equals(Buffer.from([137,80,78,71,13,10,26,10])));
    }
    assert.equal(fs.readFileSync(file,'utf8'),source,'export never writes the source');
    results[format]={previewPages:preview.pages,preservedOnCancel:true,preservedOnConflict:true,reviewAcceptedAfterWrite:true,
      bytes:bytes.length,sha256:createHash('sha256').update(bytes).digest('hex')};
  }
  return results;
};
