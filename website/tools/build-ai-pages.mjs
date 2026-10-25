import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const site = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const origin = 'https://rust.readmd.asia';
export const aiCopies = [
  {
    prefix: '', lang: 'en', label: 'AI autocomplete', title: 'Your next thought. One Tab away.',
    lead: 'A suggestion beside your words. Accept it with Tab, dismiss it with Esc, or undo the insertion.',
    illustration: 'Interactive illustration',
    disclosure: 'This is an interaction illustration, not a recording of ReadMD. It uses fixed example text, sends no model requests and makes no response-time claim.',
    contextLabel: 'Nearby context', context: 'Field notes · A quiet walk after the rain.',
    document: 'Example note', editor: 'Example document: try Tab, Esc or undo',
    draft: 'The pavement reflected the lights. ', suggestion: 'We walked by the river and wrote the moment down.',
    legend: 'The gray words are a suggestion, not part of the draft yet.',
    hint: 'Focus the example. Tab accepts once; the next Tab moves to the next control. Esc dismisses. Ctrl+Z or ⌘Z undoes an accepted insertion.',
    accept: 'Accept', dismiss: 'Dismiss', undo: 'Undo', reset: 'Try again',
    pending: 'Suggestion ready. Your draft is unchanged.', accepted: 'Suggestion inserted. You can undo this insertion.',
    dismissed: 'Suggestion dismissed. Your draft is unchanged.', restored: 'Insertion undone. The original draft is restored.',
    noScript: 'The example remains readable without JavaScript. Enable JavaScript to try its controls.',
    privacy: 'In ReadMD, autocomplete is opt-in. Choose your saved connection and model in AI settings. This illustration does not connect to either.',
    shared: 'One Rust engine, two editors', sharedCopy: 'The desktop app and VS Code share completion context limits and filtering. MCP exposes the same completion engine.',
    platform: 'Platform support, with evidence', platformCopy: 'Builds and native acceptance are separate. Windows is tested locally; macOS, Linux and domestic Linux distributions still require their own native acceptance. HarmonyOS remains a source-integrated candidate.',
    back: 'Back to ReadMD',
  },
  {
    prefix: 'zh-cn/', lang: 'zh-CN', label: 'AI 自动补全', title: '下一句灵感，按 Tab 接上。',
    lead: '候选紧邻正在写的文字。Tab 接受，Esc 忽略；不满意，撤销这次插入。',
    illustration: '交互示意',
    disclosure: '这是交互示意，并非 ReadMD 真实录屏。示例使用固定文字，不调用模型，也不承诺响应时延。',
    contextLabel: '附近上下文', context: '散步札记 · 雨后的一段安静路程。',
    document: '示例笔记', editor: '示例文档：试用 Tab、Esc 或撤销',
    draft: '雨停了，街道映着灯光。', suggestion: '我们沿河走了一会儿，把这一刻写进笔记。',
    legend: '灰字是候选，尚未写入原稿。',
    hint: '聚焦示例。首次 Tab 接受候选，再按 Tab 正常移至下一控件；Esc 忽略。Ctrl+Z 或 ⌘Z 撤销已接受的插入。',
    accept: '接受', dismiss: '忽略', undo: '撤销', reset: '再试一次',
    pending: '候选已就绪，原稿未变。', accepted: '候选已插入，可以撤销这次插入。',
    dismissed: '已忽略候选，原稿未变。', restored: '已撤销插入，恢复原稿。',
    noScript: '关闭 JavaScript 后仍可阅读示例；启用后可试用这些控件。',
    privacy: 'ReadMD 自动补全需主动开启。连接与模型沿用 AI 设置中的选择；本示意不会连接它们。',
    shared: '同一个 Rust 核心，两种编辑环境', sharedCopy: '桌面与 VS Code 共用补全上下文限制及输出过滤；MCP 也可调用相同补全引擎。',
    platform: '平台支持，以验证为准', platformCopy: '构建产物与原生验收分开记录。Windows 已在本机实测；macOS、Linux 及国产 Linux 发行版仍需各自的原生验收。鸿蒙保持源码集成候选状态。',
    back: '回到 ReadMD',
  },
  {
    prefix: 'zh-tw/', lang: 'zh-TW', label: 'AI 自動補全', title: '下一句靈感，按 Tab 接上。',
    lead: '候選緊鄰正在寫的文字。Tab 接受，Esc 忽略；不滿意，復原這次插入。',
    illustration: '互動示意',
    disclosure: '這是互動示意，並非 ReadMD 的實際錄影。範例使用固定文字，不呼叫模型，也不承諾回應延遲。',
    contextLabel: '附近上下文', context: '散步札記 · 雨後的一段安靜路程。',
    document: '範例筆記', editor: '範例文件：試用 Tab、Esc 或復原',
    draft: '雨停了，街道映著燈光。', suggestion: '我們沿河走了一會兒，把這一刻寫進筆記。',
    legend: '灰字是候選，尚未寫入原稿。',
    hint: '將焦點移到範例。首次 Tab 接受候選，再按 Tab 正常移至下一個控制項；Esc 忽略。Ctrl+Z 或 ⌘Z 復原已接受的插入。',
    accept: '接受', dismiss: '忽略', undo: '復原', reset: '再試一次',
    pending: '候選已就緒，原稿未變。', accepted: '候選已插入，可以復原這次插入。',
    dismissed: '已忽略候選，原稿未變。', restored: '已復原插入，恢復原稿。',
    noScript: '關閉 JavaScript 後仍可閱讀範例；啟用後可試用這些控制項。',
    privacy: 'ReadMD 自動補全需主動開啟。連線與模型沿用 AI 設定中的選擇；本示意不會連線至它們。',
    shared: '同一個 Rust 核心，兩種編輯環境', sharedCopy: '桌面與 VS Code 共用補全上下文限制及輸出過濾；MCP 也能呼叫相同補全引擎。',
    platform: '平台支援，以驗證為準', platformCopy: '建置產物與原生驗收分開記錄。Windows 已在本機實測；macOS、Linux 與中國 Linux 發行版仍需各自的原生驗收。鴻蒙維持原始碼整合候選狀態。',
    back: '回到 ReadMD',
  },
  {
    prefix: 'ja/', lang: 'ja', label: 'AI 自動補完', title: '次のひらめきを、Tab でつなぐ。',
    lead: '書いている文章のすぐ横に候補を表示。Tab で確定、Esc で破棄。挿入した文は元に戻せます。',
    illustration: '操作イメージ',
    disclosure: 'これは操作イメージであり、ReadMD の実際の録画ではありません。固定の例文を使い、モデルへのリクエストは送信せず、応答時間も保証しません。',
    contextLabel: '周辺の文脈', context: '散歩のメモ · 雨上がりの静かな道。',
    document: 'サンプルのメモ', editor: 'サンプル文書：Tab、Esc、元に戻す操作を試す',
    draft: '雨が上がり、道に明かりが映っていた。', suggestion: '川沿いを少し歩き、このひとときをノートに残した。',
    legend: '灰色の文字は候補です。まだ原稿には挿入されていません。',
    hint: 'サンプルにフォーカスしてください。最初の Tab で候補を確定し、次の Tab で次の操作項目へ移動します。Esc で破棄。Ctrl+Z または ⌘Z で挿入を元に戻せます。',
    accept: '確定', dismiss: '破棄', undo: '元に戻す', reset: 'もう一度試す',
    pending: '候補を表示しています。原稿は変更されていません。', accepted: '候補を挿入しました。この挿入を元に戻せます。',
    dismissed: '候補を破棄しました。原稿は変更されていません。', restored: '挿入を取り消し、元の原稿に戻しました。',
    noScript: 'JavaScript が無効でも例文は読めます。操作を試すには JavaScript を有効にしてください。',
    privacy: 'ReadMD の自動補完は任意で有効にします。接続とモデルは AI 設定で選択します。この操作イメージはそれらに接続しません。',
    shared: '同じ Rust エンジンを二つのエディターで', sharedCopy: 'デスクトップと VS Code は補完のコンテキスト制限と出力フィルターを共有します。MCP も同じ補完エンジンを呼び出せます。',
    platform: '検証に基づくプラットフォーム対応', platformCopy: 'ビルドとネイティブ環境での受け入れ確認は別です。Windows はローカルで確認済みですが、macOS、Linux、中国の Linux 環境はそれぞれ検証が必要です。HarmonyOS はソース統合済みの候補です。',
    back: 'ReadMD に戻る',
  },
];

const esc = value => String(value).replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;');

export function renderAiPage(copy, version = '0.0.5') {
  if (!/^\d+\.\d+\.\d+$/.test(version)) throw Error('Invalid AI introduction version');
  const c = Object.fromEntries(Object.entries(copy).map(([key, value]) => [key, esc(value)]));
  const url = origin + '/' + c.prefix + 'ai-autocomplete/';
  const label = c.label + ' · V' + version;
  const alternates = aiCopies.map(item => `<link rel="alternate" hreflang="${item.lang}" href="${origin}/${item.prefix}ai-autocomplete/">`).join('\n');
  const control = (action, label, key, primary = false) => `<button type="button" data-ai-action="${action}" class="${primary ? 'apple-pill-primary' : 'apple-pill-secondary'} inline-flex items-center gap-2" disabled>${label}${key ? `<kbd class="text-xs">${key}</kbd>` : ''}</button>`;
  return `<!doctype html>
<html lang="${c.lang}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${label} — ReadMD</title>
<meta name="description" content="${c.lead}">
<link rel="canonical" href="${url}">
${alternates}
<link rel="alternate" hreflang="x-default" href="${origin}/ai-autocomplete/">
<link rel="stylesheet" href="/assets/site.css">
<link rel="stylesheet" href="/assets/motion.css">
<meta property="og:type" content="website">
<meta property="og:title" content="${label}">
<meta property="og:description" content="${c.lead}">
<meta property="og:url" content="${url}">
<meta property="og:image" content="${origin}/media/overview-reader.png">
</head>
<body class="text-ink">
<header class="border-b border-line"><nav class="mx-auto max-w-6xl px-5 py-5"><a class="flex items-center gap-3 font-semibold" href="/${c.prefix}"><img src="/assets/icon-256.png" width="32" height="32" alt="">ReadMD</a></nav></header>
<main class="mx-auto max-w-6xl px-5 py-16 md:py-24">
<p class="text-sm text-muted">${label} · ${c.illustration}</p>
<h1 class="mt-6 max-w-4xl text-4xl md:text-6xl font-bold tracking-tight">${c.title}</h1>
<p class="mt-6 text-xl text-muted max-w-3xl">${c.lead}</p>
<figure class="mt-10 max-w-4xl" data-ai-illustration="offline" aria-labelledby="ai-demo-title" aria-describedby="ai-demo-disclosure">
<div class="overflow-hidden rounded-3xl border border-line bg-card">
<div class="flex flex-wrap items-center justify-between gap-3 border-b border-line px-6 py-4"><span class="font-semibold" id="ai-demo-title">ReadMD · ${c.document}</span><span class="text-sm text-muted">${c.illustration}</span></div>
<div class="p-6 md:p-8">
<div class="rounded-2xl bg-surface px-5 py-4" data-ai-context><p class="text-xs text-muted">${c.contextLabel}</p><p class="mt-2 text-sm">${c.context}</p></div>
<p class="mt-5 text-sm text-muted" id="ai-demo-legend">${c.legend}</p>
<div class="mt-3 rounded-2xl border border-line p-5 text-lg leading-relaxed break-words focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-action" data-ai-editor role="group" tabindex="0" aria-label="${c.editor}" aria-describedby="ai-demo-legend ai-demo-hint"><span data-ai-draft>${c.draft}</span><span data-ai-inserted hidden></span><span data-ai-suggestion class="text-muted">${c.suggestion}</span></div>
<p class="mt-4 text-sm text-muted" id="ai-demo-hint">${c.hint}</p>
<div class="mt-5 flex flex-wrap gap-3">${control('accept', c.accept, 'Tab', true)}${control('dismiss', c.dismiss, 'Esc')}${control('undo', c.undo, 'Ctrl / ⌘ Z')}${control('reset', c.reset, '')}</div>
<p class="mt-4 min-h-12 text-sm text-muted" data-ai-status role="status" aria-live="polite" aria-atomic="true" data-pending="${c.pending}" data-accepted="${c.accepted}" data-dismissed="${c.dismissed}" data-restored="${c.restored}">${c.pending}</p>
<noscript><p class="mt-4 text-sm text-muted">${c.noScript}</p></noscript>
</div>
</div>
<figcaption class="mt-3 text-sm text-muted" id="ai-demo-disclosure" data-ai-disclosure>${c.disclosure}</figcaption>
</figure>
<p class="mt-6 text-muted max-w-3xl">${c.privacy}</p>
<section class="grid md:grid-cols-2 gap-6 mt-12"><article class="rounded-3xl border border-line bg-card p-8"><h2 class="text-xl font-semibold">${c.shared}</h2><p class="mt-4 text-muted leading-relaxed">${c.sharedCopy}</p></article><article class="rounded-3xl border border-line bg-card p-8"><h2 class="text-xl font-semibold">${c.platform}</h2><p class="mt-4 text-muted leading-relaxed">${c.platformCopy}</p></article></section>
</main>
<footer class="mx-auto max-w-6xl px-5 py-8 border-t border-line"><a href="/${c.prefix}">${c.back}</a></footer>
<script src="/assets/site.js" defer></script>
<script src="/assets/motion.js" defer></script>
<script src="/assets/ai-illustration.js" defer></script>
<script type="module" src="/assets/release.js"></script>
</body>
</html>
`;
}

export function aiIllustrationProblems(html, lang) {
  const errors = [], copy = aiCopies.find(item => item.lang === lang);
  if (!copy) return ['Unknown AI illustration language: ' + lang];
  for (const marker of ['data-ai-illustration="offline"', 'data-ai-context', 'data-ai-editor', 'data-ai-draft', 'data-ai-inserted hidden', 'data-ai-suggestion class="text-muted"', 'data-ai-disclosure', 'role="status"', 'aria-live="polite"', 'tabindex="0"', '/assets/ai-illustration.js']) {
    if (!html.includes(marker)) errors.push('AI illustration missing ' + marker + ': ' + lang);
  }
  for (const action of ['accept', 'dismiss', 'undo', 'reset']) {
    if (!new RegExp('<button\\b[^>]*type="button"[^>]*data-ai-action="' + action + '"').test(html)) errors.push('AI illustration missing button ' + action + ': ' + lang);
  }
  for (const key of ['disclosure', 'contextLabel', 'context', 'draft', 'suggestion', 'hint', 'pending', 'accepted', 'dismissed', 'restored']) {
    if (!html.includes(esc(copy[key]))) errors.push('AI illustration missing localized ' + key + ': ' + lang);
  }
  if (/<(?:video|source|iframe)\b|\bposter\s*=|\/media\/v005-ai|\/showcase\/videos\//i.test(html)) errors.push('AI illustration must not depend on recordings: ' + lang);
  if (/<script\b(?![^>]*\bsrc=)[^>]*>|\bon(?:click|keydown|load)\s*=/i.test(html)) errors.push('AI illustration requires same-origin external scripts: ' + lang);
  return errors;
}

// Generation is deliberately limited to these four pages. Existing homepage,
// gallery, sitemap, feed and crawler content have independent owners.
export function buildAiPages(publicRoot = path.join(site, 'public'), version = '0.0.5') {
  return aiCopies.map(copy => {
    const file = path.join(publicRoot, copy.prefix, 'ai-autocomplete/index.html');
    const html = renderAiPage(copy, version);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    if (!fs.existsSync(file) || fs.readFileSync(file, 'utf8') !== html) fs.writeFileSync(file, html);
    return file;
  });
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const release = JSON.parse(fs.readFileSync(path.join(site, 'release.json'), 'utf8'));
  console.log('Generated ' + buildAiPages(undefined, release.candidate).length + ' localized offline AI illustration pages.');
}
