// Opt-in translation of public UI strings through an already-configured local
// ReadMD instance. No API key is read, printed or written by this tool.
// READMD_I18N_ORIGIN, READMD_I18N_MODEL, READMD_I18N_OUTPUT are required.
// Output is a resumable i18n-add specification; review before applying it.
import fs from 'node:fs';
import path from 'node:path';
const origin = new URL(process.env.READMD_I18N_ORIGIN || '');
if (!['127.0.0.1', 'localhost', '[::1]'].includes(origin.hostname)) throw Error('Use a local ReadMD origin');
const model = process.env.READMD_I18N_MODEL;
const output = process.env.READMD_I18N_OUTPUT;
if (!model || !output) throw Error('READMD_I18N_MODEL and READMD_I18N_OUTPUT are required');
const dir = path.resolve('assets/i18n');
const en = JSON.parse(fs.readFileSync(path.join(dir, 'en.json'), 'utf8'));
const zh = JSON.parse(fs.readFileSync(path.join(dir, 'zh-CN.json'), 'utf8'));
const html = await (await fetch(origin, { signal: AbortSignal.timeout(10000) })).text();
const token = html.match(/name="readmd-app-token" content="([^"]+)"/)?.[1];
if (!token) throw Error('The local application session is not available');
const spec = fs.existsSync(output) ? JSON.parse(fs.readFileSync(output, 'utf8')) : {};
const placeholders = value => [...value.matchAll(/\{([\w.-]+)\}/g)].map(match => match[1]).sort().join(',');
const technical = /^(?:ReadMD|Live2D|BongoCat|Hermes|Markdown|HTML|CSS|JSON|XML|YAML|LaTeX|PDF|DOCX|EPUB|OCR|AI|MCP|API|URL|UTF-?8|ASCII|RGB|BPM|px|pt|em|rem|auto|[.\d\s×/%+−-]+)$/i;
const locales = (process.env.READMD_I18N_LOCALES || fs.readdirSync(dir).filter(file => file.endsWith('.json') && file !== 'meta.json').map(file => file.slice(0, -5)).join(',')).split(',').filter(lang => lang && lang !== 'en' && !lang.startsWith('zh'));
let calls = 0; const budget = Number(process.env.READMD_I18N_CALL_BUDGET || 80);
const batchSize = Number(process.env.READMD_I18N_BATCH_SIZE || 160);
const tasks = locales.flatMap(lang => {
  const dict = JSON.parse(fs.readFileSync(path.join(dir, lang + '.json'), 'utf8'));
  const keys = Object.keys(en).filter(key => dict[key] === en[key] && /[a-z]{2}/i.test(en[key]) && !technical.test(en[key]) && !spec[key]?.[lang]);
  return Array.from({ length: Math.ceil(keys.length / batchSize) }, (_, index) => ({ lang, keys: keys.slice(index * batchSize, (index + 1) * batchSize) }));
}).filter(task => task.keys.length);
let next = 0, failures = 0;
await Promise.all(Array.from({ length: Number(process.env.READMD_I18N_CONCURRENCY || 2) }, async () => {
  while (next < tasks.length) {
    const task = tasks[next++];
    if (calls++ >= budget) { failures++; console.log(JSON.stringify({ locale: task.lang, pending: task.keys.length, reason: 'call_budget' })); continue; }
    const source = task.keys.map(key => en[key]);
    try {
      const response = await fetch(new URL('/api/ai/chat', origin), { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-ReadMD-App-Token': token },
        body: JSON.stringify({ model, temperature: 0.1, stream: true, session: 'public-i18n-' + task.lang,
          messages: [{ role: 'system', content: `Translate this ordered array of public ReadMD software UI strings into locale ${task.lang}. Use natural native-language UI wording. Preserve meaning, numbers, brands, shortcuts, HTML markup and every {placeholder} exactly. Do not copy English sentences as a fallback. Return ONLY a JSON ARRAY of translated strings in exactly the same order, with exactly ${task.keys.length} elements. No explanations or markdown fences.` }, { role: 'user', content: JSON.stringify(source) }] }), signal: AbortSignal.timeout(270000) });
      const result = await response.json();
      if (!response.ok || !result.ok) throw Error(result.error_code || 'provider_request_failed');
      const raw = result.content.replace(/^\s*```(?:json)?\s*\n?/, '').replace(/\n?```\s*$/, '').trim();
      const translated = JSON.parse(raw);
      if (!Array.isArray(translated) || translated.length !== task.keys.length) throw Error('translation_key_count');
      for (const [index, key] of task.keys.entries()) {
        const value = translated[index];
        if (typeof value !== 'string' || !value.trim() || placeholders(value) !== placeholders(en[key])) throw Error('translation_placeholders');
      }
      for (const [index, key] of task.keys.entries()) spec[key] = { ...(spec[key] || { en: en[key], 'zh-CN': zh[key], 'zh-TW': JSON.parse(fs.readFileSync(path.join(dir, 'zh-TW.json'), 'utf8'))[key] }), [task.lang]: translated[index] };
      fs.mkdirSync(path.dirname(output), { recursive: true });
      fs.writeFileSync(output, JSON.stringify(spec, null, 2) + '\n');
      console.log(JSON.stringify({ locale: task.lang, translatedKeys: task.keys.length, passed: true }));
    } catch (error) { failures++; console.log(JSON.stringify({ locale: task.lang, pending: task.keys.length, reason: error.name === 'SyntaxError' ? 'invalid_translation_json' : error.message.slice(0, 100) })); }
  }
}));
if (failures) process.exitCode = 1;
