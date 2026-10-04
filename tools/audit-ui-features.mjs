#!/usr/bin/env node
// Verify that the reviewed inventory still points to real controls and code.
// This complements the UI/Rust tests; source presence alone is not a behavior test.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const directory = path.join(root, 'docs/reviews/ui-function-inventory-2026-10-02');
const inventory = JSON.parse(fs.readFileSync(path.join(directory, 'inventory.json'), 'utf8'));
const verification = JSON.parse(fs.readFileSync(path.join(directory, 'verification.json'), 'utf8'));
const errors = [];
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
for (const feature of inventory.features) {
  if (!feature.controls.length || !feature.flow || !feature.backend || !feature.frontend.length) errors.push(`Incomplete feature: ${feature.id}`);
  for (const [file, symbol] of feature.frontend) {
    const text = source(file);
    if (symbol && !new RegExp(`\\b(?:function|fn)\\s+${escape(symbol)}\\b`).test(text)) errors.push(`Missing implementation: ${feature.id} ${file} ${symbol}`);
  }
}
for (const control of inventory.controls) {
  if (!featureIds.has(control.feature_id)) errors.push(`Unassigned control: ${control.inventory_id}`);
  const line = source(control.source).split(/\r?\n/)[control.line - 1];
  if (!line || !line.includes(`<${control.tag}`) || (control.id && !line.includes(`id="${control.id}"`))) errors.push(`Stale control location: ${control.inventory_id}`);
}
for (const row of [...inventory.api, ...inventory.commands]) {
  if (!source(row.source).split(/\r?\n/)[row.line - 1]) errors.push(`Invalid source line: ${row.route || row.id}`);
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
  process.stdout.write(`OK: ${featureIds.size} reviewed flows, ${inventory.controls.length} assigned controls, ${inventory.dynamic.length} dynamic families, ${inventory.api.length} routes; source and evidence links exist.\n`);
  process.stdout.write('Run the referenced UI and Rust tests to verify behavior. This gate checks inventory coverage and implementation entry points.\n');
}
