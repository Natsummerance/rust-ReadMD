import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
test('partial locale updates preserve existing translations and populate new keys', () => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), 'readmd-i18n-add-'));
  const script = fileURLToPath(new URL('../i18n-add.mjs', import.meta.url));
  try {
    const locales = path.join(fixture, 'assets/i18n'); fs.mkdirSync(locales, { recursive: true });
    fs.writeFileSync(path.join(locales, 'meta.json'), JSON.stringify({ fr: { native: 'Français' } }));
    for (const [lang, text] of [['en', 'Save'], ['fr', 'Enregistrer'], ['de', 'Speichern'], ['zh-HK', '儲存（香港）']]) fs.writeFileSync(path.join(locales, lang + '.json'), JSON.stringify({ 'editor.save': text }));
    const spec = path.join(fixture, 'keys.json');
    fs.writeFileSync(spec, JSON.stringify({ 'editor.save': { en: 'Save file', de: 'Datei speichern', 'zh-TW': '儲存' }, 'editor.new': { en: 'New file', fr: 'Nouveau fichier', 'zh-TW': '新增檔案' } }));
    const result = spawnSync(process.execPath, [script, spec], { cwd: fixture, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    const read = lang => JSON.parse(fs.readFileSync(path.join(locales, lang + '.json')));
    assert.equal(read('fr')['editor.save'], 'Enregistrer');
    assert.equal(read('de')['editor.save'], 'Datei speichern');
    assert.equal(read('de')['editor.new'], 'New file');
    assert.equal(read('fr')['editor.new'], 'Nouveau fichier');
    assert.equal(read('zh-HK')['editor.save'], '儲存（香港）');
    assert.equal(read('zh-HK')['editor.new'], '新增檔案');
    assert.deepEqual(read('meta'), { fr: { native: 'Français' } });
  } finally { fs.rmSync(fixture, { recursive: true, force: true }); }
});
