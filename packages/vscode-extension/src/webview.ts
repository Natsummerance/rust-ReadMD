import { randomBytes } from 'crypto';

export interface WebviewAssets {
  base: string;
  cspSource: string;
  language?: string;
  customStyle?: string;
  documentBase?: string;
}

function escapeHtml(text: string): string {
  return text.replace(/[&<>"']/g, ch => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[ch]!));
}

/** All executable resources are shipped in the VSIX. Markdown is inert data. */
export function getWebviewContent(markdown: string, title: string, assets: WebviewAssets,
    presentation = false): string {
  const nonce = randomBytes(18).toString('base64');
  const base = escapeHtml(assets.base.replace(/\/$/, ''));
  const script = (file: string) => `<script nonce="${nonce}" src="${base}/${file}"></script>`;
  const data = JSON.stringify({ markdown, presentation, language: assets.language || 'en', assetBase: assets.base, documentBase: assets.documentBase }).replace(/</g, '\\u003c').replace(/\u2028/g, '\\u2028').replace(/\u2029/g, '\\u2029');
  return `<!DOCTYPE html>
<html lang="${escapeHtml(assets.language || 'en')}"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src ${escapeHtml(assets.cspSource)} data:; font-src ${escapeHtml(assets.cspSource)}; style-src ${escapeHtml(assets.cspSource)} 'unsafe-inline'; script-src 'nonce-${nonce}'; frame-src ${escapeHtml(assets.cspSource)}; connect-src 'none'; form-action 'none'; base-uri 'none';">
<title>ReadMD: ${escapeHtml(title)}</title>
<link rel="stylesheet" href="${base}/vendor/katex/katex.min.css">
${presentation ? `<link rel="stylesheet" href="${base}/vendor/reveal/reveal.css"><link id="readmd-slide-theme" rel="stylesheet" href="${base}/vendor/reveal/theme/black.css">` : ''}
<link rel="stylesheet" href="${base}/preview.css">${assets.customStyle ? `<link rel="stylesheet" href="${escapeHtml(assets.customStyle)}">` : ''}</head>
<body${presentation ? ' class="presentation"' : ''}><main id="content"${presentation ? ' class="reveal"' : ''}></main>
<script id="readmd-initial" type="application/json" nonce="${nonce}">${data}</script>
${script('vendor/marked.min.js')}${script('vendor/katex/katex.min.js')}${script('vendor/katex/auto-render.min.js')}
${script('vendor/mermaid.min.js')}${presentation ? script('vendor/reveal/reveal.min.js') : ''}${script('preview.js')}
</body></html>`;
}
