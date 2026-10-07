/* Offline preview and presentation. No document-supplied scripts or styles. */
(() => {
  'use strict';
  const api = typeof acquireVsCodeApi === 'function' ? acquireVsCodeApi() : null;
  const initial = JSON.parse(document.getElementById('readmd-initial').textContent);
  const root = document.getElementById('content');
  const allowed = new Set('p div span br hr h1 h2 h3 h4 h5 h6 a img pre code blockquote ul ol li table thead tbody tfoot tr th td del s strong b em i u sub sup kbd samp details summary input dl dt dd abbr mark section'.split(' '));
  const removed = new Set('script style iframe frame object embed link meta base form textarea select button svg math template noscript'.split(' '));
  let generation = 0;
  let scrollLock = 0;
  let reveal;
  let diagramCounter = 0;
  const diagramCache = new Map();
  const nativeDiagrams = new Map();
  const diagramEngines = new Set(['wavedrom','bitfield','graphviz','dot','viz','vega','vega-lite','tikz','chart','chartjs','chart.js','plantuml','puml','wsd','d2','ditaa']);
  const zh = /^zh/i.test(initial.language);

  function renderDiagram(engine, code) {
    const key = engine + '\n' + code;
    if (diagramCache.has(key)) return Promise.resolve(diagramCache.get(key));
    return new Promise((resolve, reject) => {
      const token = 'diagram-' + (++diagramCounter);
      const native = ['plantuml','puml','wsd','d2','ditaa'].includes(engine);
      const frame = native ? null : document.createElement('iframe');
      let settled = false;
      const finish = data => {
        if (settled) return; settled = true;
        clearTimeout(timer); window.removeEventListener('message', listener); frame?.remove(); nativeDiagrams.delete(token);
        if (data?.error || (!data?.svg && !data?.png)) { reject(new Error('diagram')); return; }
        const src = data.png || 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(data.svg);
        diagramCache.set(key, src); if (diagramCache.size > 32) diagramCache.delete(diagramCache.keys().next().value);
        resolve(src);
      };
      const listener = event => { if (event.source === frame?.contentWindow && event.data?.type === 'diagramResult' && event.data.token === token) finish(event.data); };
      const timer = setTimeout(() => finish({ error: true }), 50000);
      if (native) {
        if (!api) return finish({ error: true });
        nativeDiagrams.set(token, finish); api.postMessage({ type: 'nativeDiagram', token, engine, code });
      } else {
        window.addEventListener('message', listener);
        frame.setAttribute('sandbox', 'allow-scripts'); frame.title = engine;
        frame.style.cssText = 'position:absolute;left:-10000px;top:0;width:1000px;height:700px;border:0';
        frame.onload = () => frame.contentWindow.postMessage({ type: 'renderDiagram', token, engine, code }, '*');
        frame.src = initial.assetBase + '/diagram-frame.html'; document.body.append(frame);
      }
    });
  }

  function clean(html) {
    const template = document.createElement('template');
    template.innerHTML = html;
    for (const node of [...template.content.querySelectorAll('*')]) {
      const tag = node.localName;
      if (removed.has(tag)) { node.remove(); continue; }
      if (!allowed.has(tag)) { node.replaceWith(...node.childNodes); continue; }
      for (const attr of [...node.attributes]) {
        const name = attr.name.toLowerCase();
        let value = attr.value.trim();
        // Resolve only parsed image elements, never examples inside code fences.
        const localImage = name === 'src' && tag === 'img' && initial.documentBase && !/^[a-z][a-z\d+.-]*:|^\/\//i.test(value);
        if (localImage) {
          try { value = new URL(value.replace(/\\/g, '/'), initial.documentBase).href; node.setAttribute(name, value); } catch {}
        }
        let safe = ['title','alt','colspan','rowspan','lang','dir'].includes(name);
        if (name === 'class' && tag === 'code') safe = /^language-[\w-]+$/.test(value);
        if (name === 'id') safe = /^heading-[\w-]+$/.test(value);
        if (name === 'align' && ['td','th'].includes(tag)) safe = /^(left|right|center)$/.test(value);
        if (name === 'start' && tag === 'ol') safe = /^\d+$/.test(value);
        if (name === 'open' && tag === 'details') safe = true;
        if (name === 'href' && tag === 'a') safe = /^(https?:\/\/|mailto:|#)/i.test(value) || (!/^[a-z][a-z\d+.-]*:/i.test(value) && !value.startsWith('//'));
        if (name === 'src' && tag === 'img') safe = (localImage && value.startsWith(initial.documentBase)) || /^(vscode-webview-resource:|https:\/\/[^/]+\.vscode-cdn\.net\/|vscode-webview:|data:image\/(png|jpe?g|webp|gif);base64,)/i.test(value);
        if (tag === 'input' && ['type','checked','disabled'].includes(name)) safe = name !== 'type' || value === 'checkbox';
        if (!safe) node.removeAttribute(attr.name);
      }
      if (tag === 'input') { node.type = 'checkbox'; node.disabled = true; }
      if (tag === 'a') { node.rel = 'noopener noreferrer'; }
      if (tag === 'img' && !node.hasAttribute('src')) {
        const replacement = document.createElement('span');
        replacement.className = 'remote-image';
        replacement.textContent = node.alt || (/^zh/i.test(initial.language) ? '图片' : 'Image');
        node.replaceWith(replacement);
      }
    }
    return template.content;
  }

  function renderPart(markdown) {
    const part = document.createElement('section');
    part.append(clean(marked.parse(markdown, { gfm: true, breaks: false })));
    return part;
  }

  function documentMetadata(markdown) {
    const match = /^---[ \t]*\r?\n([\s\S]*?)\r?\n(?:---|\.\.\.)[ \t]*(?:\r?\n|$)/.exec(markdown);
    const metadata = {};
    if (!match) return { markdown, metadata };
    // Same small scalar subset as the native presentation exporter.
    for (const line of match[1].split(/\r?\n/)) {
      const field = /^\s*(theme|transition|title|author|slideNumber|width|height):\s*(.*?)\s*$/i.exec(line);
      if (!field) continue;
      let value = field[2].replace(/\s+#.*$/, '');
      try { if (value.startsWith('"')) value = JSON.parse(value); else value = value.replace(/^'|'$/g, ''); } catch { continue; }
      metadata[field[1].toLowerCase()] = String(value);
    }
    return { markdown: markdown.slice(match[0].length), metadata };
  }

  async function render(markdown) {
    const current = ++generation;
    const ratio = window.scrollY / Math.max(1, document.documentElement.scrollHeight - innerHeight);
    const fragment = document.createDocumentFragment();
    // YAML metadata is document configuration, not a slide or a heading.
    const parsed = documentMetadata(String(markdown).replace(/^\uFEFF/, ''));
    markdown = parsed.markdown;
    const metadata = parsed.metadata;
    if (initial.presentation) {
      const slides = document.createElement('div'); slides.className = 'slides';
      // Delimit slides only outside fenced code; keep thematic breaks in code.
      const parts = []; let lines = []; let fence = '';
      for (const line of String(markdown).split('\n')) {
        const match = /^\s{0,3}(`{3,}|~{3,})/.exec(line);
        if (match) { if (!fence) fence = match[1]; else if (match[1][0] === fence[0] && match[1].length >= fence.length) fence = ''; }
        if (!fence && /^(---\s*|<!--\s*slide\s*-->\s*)$/.test(line)) { parts.push(lines.join('\n')); lines = []; }
        else lines.push(line);
      }
      parts.push(lines.join('\n'));
      for (const part of parts) slides.append(renderPart(part));
      fragment.append(slides);
    } else fragment.append(renderPart(String(markdown)));
    root.replaceChildren(fragment);
    const slugs = new Map();
    for (const heading of root.querySelectorAll('h1,h2,h3,h4,h5,h6')) {
      const slug = heading.textContent.trim().toLowerCase().replace(/[^\p{L}\p{N}\s_-]/gu, '').replace(/\s/g, '-');
      const count = slugs.get(slug) || 0; slugs.set(slug, count + 1);
      heading.id = slug + (count ? '-' + count : '');
    }
    for (const marker of root.querySelectorAll('p')) {
      if (!/^\[toc\]$/i.test(marker.textContent.trim())) continue;
      const nav = document.createElement('nav'); nav.setAttribute('aria-label', zh ? '目录' : 'Table of contents');
      const list = document.createElement('ul');
      for (const heading of root.querySelectorAll('h1,h2,h3,h4,h5,h6')) {
        const item = document.createElement('li'), link = document.createElement('a');
        item.style.marginLeft = (Number(heading.tagName.slice(1)) - 1) * 14 + 'px';
        link.href = '#' + encodeURIComponent(heading.id); link.textContent = heading.textContent;
        item.append(link); list.append(item);
      }
      nav.append(list); marker.replaceWith(nav);
    }
    // Math is rendered after sanitizing Markdown; KaTeX trust stays disabled.
    renderMathInElement(root, { delimiters: [
      { left: '$$', right: '$$', display: true }, { left: '$', right: '$', display: false },
      { left: '\\[', right: '\\]', display: true }, { left: '\\(', right: '\\)', display: false },
    ], throwOnError: false, trust: false });
    for (const code of root.querySelectorAll('code.language-mermaid')) {
      const diagram = document.createElement('div'); diagram.className = 'mermaid';
      diagram.textContent = code.textContent; code.parentElement.replaceWith(diagram);
    }
    try { await mermaid.run({ nodes: root.querySelectorAll('.mermaid') }); }
    catch { if (current === generation) for (const item of root.querySelectorAll('.mermaid:not([data-processed])')) item.classList.add('diagram-error'); }
    if (current !== generation) return;
    for (const code of root.querySelectorAll('pre > code')) {
      const engine = (code.className.match(/language-([\w.-]+)/) || [])[1];
      if (!diagramEngines.has(engine)) continue;
      if (current !== generation) return;
      const pre = code.parentElement;
      const holder = document.createElement('figure'); holder.className = 'diagram';
      const source = code.textContent;
      const status = document.createElement('figcaption'); status.textContent = zh ? '正在渲染图表…' : 'Rendering diagram…';
      pre.replaceWith(holder); holder.append(status);
      try {
        const src = await renderDiagram(engine, source);
        if (current !== generation) return;
        const image = document.createElement('img'); image.src = src; image.alt = engine;
        holder.replaceChildren(image);
      } catch {
        if (current !== generation) return;
        status.textContent = zh ? '图表渲染失败。请检查语法；PlantUML 需要本地 Java 和 PlantUML。源码已保留。' : 'Cannot render this diagram. Check its syntax; PlantUML needs local Java and PlantUML. Source is preserved.';
        holder.append(pre);
      }
    }
    if (initial.presentation) {
      const themes = ['black','white','league','beige','sky','night','serif','simple','solarized','blood','moon'];
      const theme = (metadata.theme || 'black').replace(/\.css$/, '');
      document.getElementById('readmd-slide-theme').href = initial.assetBase + '/vendor/reveal/theme/' + (themes.includes(theme) ? theme : 'black') + '.css';
      const transitions = ['none','fade','slide','convex','concave','zoom'];
      const options = { embedded: true, hash: false, controls: true, progress: true, transition: transitions.includes(metadata.transition) ? metadata.transition : 'slide', slideNumber: metadata.slidenumber === 'true' };
      for (const dimension of ['width','height']) if (/^\d+$/.test(metadata[dimension] || '') && Number(metadata[dimension]) >= 240 && Number(metadata[dimension]) <= 7680) options[dimension] = Number(metadata[dimension]);
      if (reveal) { reveal.configure(options); reveal.sync(); }
      else { reveal = new Reveal(root, options); await reveal.initialize(); }
    } else { scrollLock = Date.now() + 200; window.scrollTo(0, ratio * Math.max(0, document.documentElement.scrollHeight - innerHeight)); }
  }
  mermaid.initialize({ startOnLoad: false, securityLevel: 'strict', theme: document.body.classList.contains('vscode-dark') ? 'dark' : 'default' });
  window.addEventListener('message', event => {
    if (event.data?.type === 'nativeDiagramResult') nativeDiagrams.get(event.data.token)?.(event.data);
    if (event.data?.type === 'updateContent') void render(event.data.markdown);
    if (event.data?.type === 'scrollToRatio' && Number.isFinite(event.data.ratio)) {
      scrollLock = Date.now() + 250;
      window.scrollTo(0, Math.max(0, Math.min(1, event.data.ratio)) * Math.max(0, document.documentElement.scrollHeight - innerHeight));
    }
  });
  window.addEventListener('scroll', () => {
    if (Date.now() < scrollLock || initial.presentation) return;
    api?.postMessage({ type: 'previewScroll', ratio: window.scrollY / Math.max(1, document.documentElement.scrollHeight - innerHeight) });
  }, { passive: true });
  document.addEventListener('click', event => {
    const link = event.target.closest('a[href]');
    if (!link) return;
    const href = link.getAttribute('href');
    if (/^(https?:|mailto:)/i.test(href)) { event.preventDefault(); api?.postMessage({ type: 'openLink', href }); }
    else if (href.startsWith('#')) {
      event.preventDefault();
      try { document.getElementById(decodeURIComponent(href.slice(1)))?.scrollIntoView({ block: 'start' }); } catch {}
    } else { event.preventDefault(); api?.postMessage({ type: 'openDocument', href }); }
  });
  void render(initial.markdown).then(() => api?.postMessage({ type: 'webviewReady' }));
})();
