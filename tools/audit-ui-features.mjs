#!/usr/bin/env node
// Verify that the reviewed inventory still points to real controls and code.
// This complements the UI/Rust tests; source presence alone is not a behavior test.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { locateControl, hasDeclaration } from './ui-inventory-identity.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const directory = path.join(root, 'docs/reviews/ui-function-inventory-2026-10-02');
const inventory = JSON.parse(fs.readFileSync(path.join(directory, 'inventory.json'), 'utf8'));
const verification = JSON.parse(fs.readFileSync(path.join(directory, 'verification.json'), 'utf8'));
const errors = [];
// Requested removal of the redundant Settings → Exit item. Native/titlebar
// exit remains covered by F084/F104; historical inventory rows stay intact.
const retiredControls = new Map([['S396', 'btn-app-exit']]);
const sourceCache = new Map();
function source(relative) {
  const absolute = path.resolve(root, relative);
  if (!absolute.startsWith(root + path.sep)) throw new Error(`Outside repository: ${relative}`);
  if (!sourceCache.has(relative)) sourceCache.set(relative, fs.readFileSync(absolute, 'utf8'));
  return sourceCache.get(relative);
}
const escape = value => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const featureIds = new Set(inventory.features.map(feature => feature.id));
if (featureIds.size !== inventory.features.length) errors.push('Duplicate feature IDs');
for (const [collection, count] of [
  ['features', 'feature_flows'], ['controls', 'static_controls'], ['dynamic', 'dynamic_families'],
  ['formats', 'format_matrix_rows'], ['commands', 'commands'], ['api', 'api_routes'],
]) {
  if (inventory[collection].length !== inventory.counts[count]) errors.push(`Wrong count: ${collection}`);
}
if (new Set(inventory.features.map(feature => feature.section)).size !== inventory.counts.sections) errors.push('Wrong section count');
const mathCatalog = JSON.parse(source('assets/math-catalog.json'));
if (inventory.formulas.length !== mathCatalog.templates.length + mathCatalog.symbols.length
    || inventory.counts.formulas !== inventory.formulas.length) errors.push('Formula inventory is stale');
for (const feature of inventory.features) {
  if (!feature.controls.length || !feature.flow || !feature.backend || !feature.frontend.length) errors.push(`Incomplete feature: ${feature.id}`);
  for (const [file, symbol] of feature.frontend) {
    const text = source(file);
    if (symbol && !new RegExp(`\\b(?:function|fn)\\s+${escape(symbol)}\\b`).test(text)) errors.push(`Missing implementation: ${feature.id} ${file} ${symbol}`);
  }
}
for (const control of inventory.controls) {
  if (!featureIds.has(control.feature_id)) errors.push(`Unassigned control: ${control.inventory_id}`);
  if (retiredControls.get(control.inventory_id) === control.id) {
    if (locateControl(source(control.source), control)) errors.push(`Retired control returned: ${control.id}`);
    continue;
  }
  if (!locateControl(source(control.source), control)) errors.push(`Missing control: ${control.inventory_id} ${control.id}`);
}
for (const row of [...inventory.api, ...inventory.commands]) {
  const text = source(row.source);
  const generated = !row.route && /^(?:export|theme)\./.test(row.id) && (() => {
    const [prefix, item] = row.id.split('.');
    const variable = prefix === 'export' ? 'fmt' : 'choice';
    return new RegExp(`\\bid\\s*:\\s*['"]${prefix}\\.['"]\\s*\\+\\s*${variable}\\b`).test(text)
      && hasDeclaration(text, item);
  })();
  const registered = row.route
    ? hasDeclaration(source('rust/readmd-kernel/src/server.rs'), row.route) && new RegExp(`\\bfn\\s+${escape(row.handler.split('::').at(-1))}\\b`).test(text)
    : hasDeclaration(text, row.id, 'id') || generated;
  if (!registered) errors.push(`Missing declaration: ${row.route || row.id}`);
}
const reviewed = new Set();
for (const row of verification.features) {
  if (!featureIds.has(row.id) || reviewed.has(row.id)) errors.push(`Invalid verification row: ${row.id}`);
  reviewed.add(row.id);
  if (row.source_audited !== true || !row.evidence.length || !row.validation || !row.conditions) errors.push(`Missing verification evidence: ${row.id}`);
  for (const evidence of row.evidence) source(evidence.file);
}
for (const id of featureIds) if (!reviewed.has(id)) errors.push(`Not reviewed: ${id}`);
if (errors.length) {
  for (const error of errors) process.stderr.write(`${error}\n`);
  process.exitCode = 1;
} else {
  process.stdout.write(`OK: ${featureIds.size} reviewed flows, ${inventory.controls.length - retiredControls.size} current/assigned controls + ${retiredControls.size} requested retirement, ${inventory.dynamic.length} dynamic families, ${inventory.api.length} routes; source identities and evidence links exist.\n`);
  process.stdout.write('Run the referenced UI and Rust tests to verify behavior. This gate checks inventory coverage and implementation entry points.\n');
}
