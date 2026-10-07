import * as vscode from 'vscode';
import * as fs from 'fs';
import * as path from 'path';

// These stable keys predate VS Code's message-based l10n API. Read our own
// bundled dictionaries rather than assuming an unsupported `key` overload.
const bundles: Record<string, Record<string, string>> = {};
export function l10n(key: string, fallback: string, args?: Record<string, unknown>): string {
  const locale = (vscode.env?.language || 'en').toLowerCase().startsWith('zh') ? 'zh-cn' : 'en';
  if (!bundles[locale]) {
    const file = locale === 'en' ? 'bundle.l10n.json' : `bundle.l10n.${locale}.json`;
    try { bundles[locale] = JSON.parse(fs.readFileSync(path.join(__dirname, '..', 'l10n', file), 'utf8')); }
    catch { bundles[locale] = {}; }
  }
  const message = bundles[locale][key] || fallback;
  return message.replace(/\{([^{}]+)\}/g, (match, name) => args && name in args ? String(args[name]) : match);
}
