import crypto from 'node:crypto';
import { scanBuffer } from '../../tools/privacy-scan.mjs';

export const RECORDED_INVENTORY = 'showcase/checks/recorded-inventory-2026-10-04.json';
export const CURRENT_INVENTORY = 'docs/reviews/ui-function-inventory-2026-10-02/inventory.json';
const featureKeys = ['id', 'section', 'title', 'controls', 'flow', 'frontend', 'backend', 'state'];
export const digest = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const feature = item => Object.fromEntries(featureKeys.map(key => [key, item[key]]));
const ids = features => features.map(item => item.id);

// These are source metadata and UI descriptions, never private configuration.
function privateFields(value) {
  if (typeof value === 'string') return /(?:^|[\s"'(])(?:[A-Za-z]:[\\/]|\\\\[^\\]+\\|\/(?:Users|home|tmp|var|private|mnt|etc)\/)/.test(value);
  if (!value || typeof value !== 'object') return false;
  return Object.entries(value).some(([key, item]) => /^(?:api_key|access_token|auth_token|password|secret|private_config|user_config|personal_path)$/i.test(key) || privateFields(item));
}

/** Validate a historical gallery, without claiming that it covers current UI. */
export function validateShowcaseHistory({ history, inventoryBytes, manifestBytes, catalogBytes, verificationBytes, currentBytes, readMedia }) {
  const errors = [];
  let inventory, manifest, catalog, verification, current;
  try {
    inventory = JSON.parse(inventoryBytes); manifest = JSON.parse(manifestBytes);
    catalog = JSON.parse(catalogBytes); verification = JSON.parse(verificationBytes); current = JSON.parse(currentBytes);
  } catch { return ['historical showcase metadata is missing or invalid JSON']; }
  if (!inventory || !manifest || !catalog || !verification || !current) return ['historical showcase metadata is missing or invalid JSON'];
  const check = (condition, message) => { if (!condition) errors.push(message); };
  const hashMatches = (bytes, hash) => typeof hash === 'string' && /^[a-f0-9]{64}$/.test(hash) && digest(bytes) === hash;
  check(history?.schema === 1 && history.kind === 'historical-recordings', 'historical showcase contract is missing or unsupported');
  if (!history || !Array.isArray(inventory.features) || !Array.isArray(manifest.features) || !Array.isArray(catalog.features) || !Array.isArray(current.features)) return [...errors, 'historical showcase feature lists are missing'];
  check(history.software_version?.status === 'unverified' && history.software_version.value === null, 'historical software version has no verified source binding');
  check(history.claims?.current_ui_recorded === false && history.claims?.current_semantics_recorded === false, 'historical recordings must not claim current UI or semantic coverage');
  check(history.recorded_inventory?.path === RECORDED_INVENTORY, 'historical inventory path is not the fixed snapshot');
  check(history.current_inventory?.path === CURRENT_INVENTORY, 'current inventory path is not the documented inventory');
  check(hashMatches(inventoryBytes, history.recorded_inventory?.sha256), 'historical inventory snapshot checksum mismatch');
  check(hashMatches(manifestBytes, history.manifest_sha256), 'historical recording manifest checksum mismatch');
  check(hashMatches(catalogBytes, history.catalog_sha256), 'historical public catalog checksum mismatch');
  check(hashMatches(verificationBytes, history.verification_sha256), 'historical verification checksum mismatch');
  check(hashMatches(currentBytes, history.current_inventory?.sha256), 'current inventory changed: historical disclosure must be reviewed');
  check(history.recorded_inventory?.sha256 === manifest.inventory_sha256 && manifest.inventory_sha256 === verification.inventory_sha256,
    'recording and verification must bind the original inventory bytes');
  check(/^\d{4}-\d{2}-\d{2}$/.test(history.recorded_date || '') && history.recorded_date === inventory.date && history.recorded_date === manifest.date,
    'historical recording date does not match its snapshot');
  check(history.ui_source_sha256 === manifest.ui_source_sha256 && manifest.ui_source_sha256 === verification.ui_source_sha256,
    'historical UI source evidence differs');
  check(/^[a-f0-9]{40}$/.test(history.provenance?.inventory_commit || '') && /^[a-f0-9]{40}$/.test(history.provenance?.inventory_git_blob || '') &&
    /^[a-f0-9]{40}$/.test(history.provenance?.manifest_commit || '') && history.provenance.inventory_path === CURRENT_INVENTORY,
    'historical inventory provenance is incomplete');
  for (const [label, bytes, value] of [['historical-inventory.json', inventoryBytes, inventory], ['historical-contract.json', Buffer.from(JSON.stringify(history)), history]]) {
    check(!scanBuffer(label, bytes).length && !privateFields(value), 'historical metadata contains private fields or absolute filesystem paths');
  }
  const recordedIds = ids(inventory.features), currentIds = ids(current.features);
  for (const [name, list] of [['historical', recordedIds], ['current', currentIds]]) check(new Set(list).size === list.length && list.every(id => /^F\d{3}$/.test(id)), name + ' inventory has invalid or duplicate feature IDs');
  check(same(ids(manifest.features), recordedIds), 'showcase does not completely cover the historical inventory');
  check(same(ids(catalog.features), recordedIds), 'public gallery does not completely cover the historical inventory');
  check(recordedIds.every(id => currentIds.includes(id)), 'historical and current feature IDs are incompatible');
  const recorded = new Map(inventory.features.map(item => [item.id, item]));
  const unrecorded = current.features.filter(item => !recorded.has(item.id) || !same(feature(item), feature(recorded.get(item.id)))).map(item => item.id);
  check(same(history.unrecorded_feature_ids, unrecorded), 'new or changed feature semantics are not disclosed as unrecorded');
  const verified = new Map((verification.recordings || []).map(item => [item.id, item]));
  check(verified.size === recordedIds.length && recordedIds.every(id => verified.has(id)), 'historical verification does not cover every recording');
  for (const item of manifest.features) {
    const r = item.recording, old = recorded.get(item.id), c = catalog.features.find(value => value.id === item.id), proof = verified.get(item.id);
    check(old && same(feature(item), feature(old)), item.id + ': recording descriptions differ from the historical inventory');
    if (r?.status !== 'recorded') { errors.push(item.id + ': genuine recording is missing'); continue; }
    check(r.evidence?.length && r.steps?.length && r.duration > 0 && r.bytes > 0, item.id + ': missing operation evidence');
    check(r.capture_profile === manifest.capture_profile && r.ui_source_sha256 === history.ui_source_sha256 && r.recorded_at?.startsWith(history.recorded_date), item.id + ': recording source or date is not bound to the historical snapshot');
    check(r.privacy?.approved === true && r.privacy.sha256 === r.sha256, item.id + ': privacy approval is missing or stale');
    check(proof?.sha256 === r.sha256 && proof.bytes === r.bytes && proof.seconds === r.duration, item.id + ': verification evidence differs from the recording');
    check(c && ['id', 'section', 'title', 'controls'].every(key => same(c[key], item[key])), item.id + ': public historical description is stale');
    check(c?.recording.sha256 === r.sha256 && same(c?.recording.steps, r.steps) && c?.flow === (r.steps?.map(step => step.text).join(' → ') || item.title), item.id + ': public catalog is stale');
    for (const name of ['video', 'poster', 'captions']) {
      const expected = `${name === 'video' ? 'videos' : name === 'poster' ? 'posters' : 'captions'}/${item.id}.${name === 'video' ? 'mp4' : name === 'poster' ? 'webp' : 'vtt'}`;
      if (r[name] !== expected) { errors.push(item.id + ': invalid historical ' + name + ' path'); continue; }
      const local = readMedia?.('showcase/' + expected), published = readMedia?.('website/public/showcase/' + expected);
      if (!local || !published) { errors.push(item.id + ': missing ' + name); continue; }
      const hash = digest(local);
      check(hash === digest(published), item.id + ': published ' + name + ' differs from recording');
      if (name === 'video') check(hash === r.sha256 && local.length === r.bytes, item.id + ': video checksum or size mismatch');
      if (name === 'poster') check(hash === r.privacy?.poster_sha256, item.id + ': poster privacy approval is stale');
      if (name === 'captions') check(local.toString('utf8').startsWith('WEBVTT\n'), item.id + ': missing captions');
    }
  }
  return errors;
}
