import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import { CURRENT_INVENTORY, RECORDED_INVENTORY, digest, validateShowcaseHistory } from './showcase-history-contract.mjs';

const bytes = value => Buffer.from(JSON.stringify(value));
function fixture() {
  const date = '2026-10-04', ui = digest(Buffer.from('recorded UI'));
  const features = ['F001', 'F002'].map(id => ({ id, section: '01 Reading', title: id, controls: ['open'],
    flow: 'Open a document', frontend: [['assets/js/core/history.js', 'open']], backend: 'Local file', state: 'wired' }));
  const inventory = { date, features }, media = new Map();
  const manifest = { date, capture_profile: 'immersive-v2', ui_source_sha256: ui, features: structuredClone(features).map(item => {
    const video = Buffer.from('video ' + item.id), poster = Buffer.from('poster ' + item.id), captions = Buffer.from('WEBVTT\n');
    const r = { status: 'recorded', capture_profile: 'immersive-v2', ui_source_sha256: ui, recorded_at: date + 'T10:00:00Z',
      video: `videos/${item.id}.mp4`, poster: `posters/${item.id}.webp`, captions: `captions/${item.id}.vtt`,
      duration: 2, bytes: video.length, sha256: digest(video), steps: [{ text: 'Open a document' }], evidence: [{ assertion: 'document_visible' }],
      privacy: { approved: true, sha256: digest(video), poster_sha256: digest(poster) } };
    for (const [name, data] of [['video', video], ['poster', poster], ['captions', captions]]) {
      media.set('showcase/' + r[name], data); media.set('website/public/showcase/' + r[name], data);
    }
    return { ...item, recording: r };
  }) };
  const catalog = { features: manifest.features.map(item => ({ id: item.id, section: item.section, title: item.title, controls: item.controls,
    flow: item.recording.steps.map(step => step.text).join(' → '), recording: structuredClone(item.recording) })) };
  const verification = { ui_source_sha256: ui, recordings: manifest.features.map(item => ({ id: item.id, sha256: item.recording.sha256, bytes: item.recording.bytes, seconds: item.recording.duration })) };
  const current = structuredClone(inventory); current.features[0].flow = 'Open and restore a document';
  const history = { schema: 1, kind: 'historical-recordings', recorded_date: date, software_version: { status: 'unverified', value: null },
    claims: { current_ui_recorded: false, current_semantics_recorded: false }, recorded_inventory: { path: RECORDED_INVENTORY },
    current_inventory: { path: CURRENT_INVENTORY }, ui_source_sha256: ui, unrecorded_feature_ids: ['F001'],
    provenance: { inventory_commit: 'a'.repeat(40), inventory_git_blob: 'b'.repeat(40), manifest_commit: 'c'.repeat(40), inventory_path: CURRENT_INVENTORY } };
  return pack({ inventory, manifest, catalog, verification, current, history, media });
}
function pack(data) {
  data.inventoryBytes = bytes(data.inventory); data.currentBytes = bytes(data.current);
  data.manifest.inventory_sha256 = digest(data.inventoryBytes); data.verification.inventory_sha256 = digest(data.inventoryBytes);
  data.manifestBytes = bytes(data.manifest); data.catalogBytes = bytes(data.catalog); data.verificationBytes = bytes(data.verification);
  data.history.recorded_inventory.sha256 = digest(data.inventoryBytes); data.history.current_inventory.sha256 = digest(data.currentBytes);
  data.history.manifest_sha256 = digest(data.manifestBytes); data.history.catalog_sha256 = digest(data.catalogBytes); data.history.verification_sha256 = digest(data.verificationBytes);
  data.readMedia = path => data.media.get(path) || null;
  return data;
}
const errors = data => validateShowcaseHistory(data);

test('historical inventory remains complete while changed current semantics remain unrecorded', () => {
  assert.deepEqual(errors(fixture()), []);
  const data = fixture(); data.current.features.push({ ...structuredClone(data.current.features[1]), id: 'F003' }); pack(data);
  assert.ok(errors(data).includes('new or changed feature semantics are not disclosed as unrecorded'));
  data.history.unrecorded_feature_ids.push('F003'); assert.deepEqual(errors(data), []);
});

test('byte drift in any bound source requires review, even when its JSON is unchanged', () => {
  for (const field of ['inventoryBytes', 'manifestBytes', 'catalogBytes', 'verificationBytes', 'currentBytes']) {
    const data = fixture(); data[field] = Buffer.concat([data[field], Buffer.from('\n')]);
    assert.ok(errors(data).some(error => /checksum mismatch|inventory changed/.test(error)), field);
  }
});

test('missing media, altered public copies and stale privacy approvals fail', () => {
  for (const name of ['videos/F001.mp4', 'posters/F001.webp', 'captions/F001.vtt']) {
    const missing = fixture(); missing.media.delete('showcase/' + name);
    assert.ok(errors(missing).some(error => /missing video|missing poster|missing captions/.test(error)));
    const changed = fixture(); changed.media.set('website/public/showcase/' + name, Buffer.from('changed'));
    assert.ok(errors(changed).some(error => error.includes('differs from recording')));
  }
  const privacy = fixture(); privacy.manifest.features[0].recording.privacy.approved = false; pack(privacy);
  assert.ok(errors(privacy).some(error => error.includes('privacy approval')));
  const evidence = fixture(); evidence.manifest.features[0].recording.evidence = []; pack(evidence);
  assert.ok(errors(evidence).some(error => error.includes('missing operation evidence')));
});

test('omitted historical items, incompatible IDs and rewritten descriptions fail', () => {
  for (const field of ['manifest', 'catalog']) {
    const data = fixture(); data[field].features.pop(); pack(data);
    assert.ok(errors(data).some(error => error.includes('completely cover the historical inventory')));
  }
  const proof = fixture(); proof.verification.recordings.pop(); pack(proof);
  assert.ok(errors(proof).some(error => error.includes('verification does not cover')));
  const ids = fixture(); ids.current.features.pop(); pack(ids);
  assert.ok(errors(ids).some(error => error.includes('IDs are incompatible')));
  const rewritten = fixture(); rewritten.manifest.features[0].flow = 'New UI'; pack(rewritten);
  assert.ok(errors(rewritten).some(error => error.includes('descriptions differ from the historical inventory')));
});

test('omitted semantic disclosures and claims of current coverage fail', () => {
  const omitted = fixture(); omitted.history.unrecorded_feature_ids = [];
  assert.ok(errors(omitted).some(error => error.includes('semantics are not disclosed')));
  for (const key of ['current_ui_recorded', 'current_semantics_recorded']) {
    const data = fixture(); data.history.claims[key] = true;
    assert.ok(errors(data).some(error => error.includes('must not claim current UI or semantic coverage')));
  }
});

test('private fields and absolute filesystem paths cannot enter historical metadata', () => {
  for (const target of ['inventory', 'history']) {
    const data = fixture(); data[target].private_config = { secret: 'synthetic fixture' }; pack(data);
    assert.ok(errors(data).some(error => error.includes('private fields or absolute filesystem paths')));
  }
  const data = fixture(); data.inventory.features[0].frontend[0][0] = ['C:', 'Users', 'fixture', 'document.md'].join('/'); pack(data);
  assert.ok(errors(data).some(error => error.includes('private fields or absolute filesystem paths')));
});

test('a date snapshot cannot be presented as a verified software version', () => {
  const data = fixture(); data.history.software_version = { status: 'verified', value: '0.0.4' };
  assert.ok(errors(data).some(error => error.includes('software version has no verified source binding')));
  const wrongDate = fixture(); wrongDate.history.recorded_date = '2026-10-05';
  assert.ok(errors(wrongDate).some(error => error.includes('date does not match')));
  assert.ok(errors({ ...fixture(), inventoryBytes: null }).some(error => error.includes('missing or invalid JSON')));
});

const script = fs.readFileSync(new URL('../public/assets/showcase.js', import.meta.url), 'utf8');
const copySource = script.slice(script.indexOf(' const HISTORY_COPY='), script.indexOf(' const $='));
for (const [lang, unverified, changed] of [['en', 'software version not verified', 'Later changes'], ['zh-CN', '软件版本未验证', '后续变更'], ['zh-Hant-TW', '軟體版本未驗證', '後續變更'], ['ja', 'バージョンは未確認', '後の変更']]) {
  test('actual gallery history hints preserve the honest boundary: ' + lang, () => {
    const context = {}; vm.runInNewContext(copySource + '\nglobalThis.message = historyMessage;', context);
    const history = fixture().history, global = context.message(history, lang), clip = context.message(history, lang, 'F001');
    assert.ok(global.includes('2026-10-04') && global.includes(unverified)); assert.ok(clip.includes(changed));
    assert.doesNotMatch(global, /V0\.0\.4|V0\.0\.5/); assert.equal(context.message(history, lang, 'F002'), global);
  });
}

test('gallery waits for the historical contract before opening recordings', () => {
  assert.match(script, /Promise\.all\(\[load\('\/showcase\/catalog\.json'\),load\('\/showcase\/history\.json'\)\]\)/);
  assert.match(script, /historyNote\.textContent=historyMessage/); assert.match(script, /playerHistory\.textContent=historyMessage/);
  assert.doesNotMatch(script, /\.innerHTML\s*=/);
});
