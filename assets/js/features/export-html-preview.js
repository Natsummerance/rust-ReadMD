'use strict';

// A preview document uses the writer's CSS at its actual reading width.
// Only the containing frame is scaled for the thumbnail.
function renderHtmlArticlePreview(data, presetName) {
  const generation = nativeExportPreview.generation;
  const miniPage = $('export-preview-mini-page');
  miniPage?.classList.add('is-native-preview');
  if (miniPage?.parentElement) {
    const viewport = miniPage.parentElement;
    miniPage.style.transform = 'scale(' + Math.min(2.5, (viewport.clientWidth - 32) / 110, (viewport.clientHeight - 32) / 148) + ')';
  }
  const meta = $('export-preview-paper-meta');
  if (meta) meta.textContent = 'HTML · ' + presetName;
  const mini = $('export-preview-mini-content');
  if (mini) mount(mini, true);
  const modal = $('export-preview-modal');
  if (modal && !modal.classList.contains('hidden')) {
    const wrapper = modal.querySelector('.export-preview-paper-wrapper');
    if (wrapper) mount(wrapper, false);
    const count = $('export-preview-pages-meta');
    if (count) count.textContent = 'HTML';
    for (const id of ['export-preview-prev-btn', 'export-preview-next-btn']) $(id)?.classList.remove('is-visible');
  }

  function mount(host, small) {
    if (host.querySelector('.export-html-frame')?.dataset.generation === String(generation)) return;
    const frame = document.createElement('iframe');
    frame.className = 'export-html-frame';
    frame.title = 'HTML · ' + presetName;
    frame.dataset.generation = String(generation);
    frame.setAttribute('sandbox', 'allow-same-origin');
    if (small) { frame.tabIndex = -1; frame.setAttribute('aria-hidden', 'true'); }
    frame.onload = () => {
      if (!frame.isConnected || nativeExportPreview.generation !== generation) return;
      const doc = frame.contentDocument;
      const root = doc?.getElementById('content');
      if (!root) return;
      doc.documentElement.lang = data.language || document.documentElement.lang;
      const opts = data.options || {};
      if (opts.htmlLayout) doc.body.dataset.readmdHtmlLayout = opts.htmlLayout;
      doc.body.dataset.readmdHtmlTone = opts.htmlTheme || 'light';
      const style = doc.createElement('style');
      style.textContent = data.styleSheet;
      doc.head.append(style);
      // Parse inside an inert template. Keep native Math SVG and table spans.
      // The sandbox has no script permission or network access.
      const fragment = doc.createElement('template');
      const math = protectMath(data.content);
      fragment.innerHTML = data.footnotesHtml ?? restoreMath(marked.parse(math.src, {gfm: true, breaks: true}), math.saved);
      fragment.content.querySelectorAll('script,style,iframe,frame,link,base,meta,embed,object,form,foreignObject,template').forEach(node => node.remove());
      for (const node of fragment.content.querySelectorAll('*')) {
        for (const attr of [...node.attributes]) {
          if (/^on/i.test(attr.name) || ['autofocus', 'srcdoc', 'ping', 'target', 'download'].includes(attr.name)) node.removeAttribute(attr.name);
        }
      }
      root.replaceChildren(fragment.content);
      doc.addEventListener('click', event => {
        const link = event.target.closest?.('a');
        if (!link) return;
        event.preventDefault();
        const href = link.getAttribute('href') || '';
        if (!small && href.startsWith('#')) {
          try { doc.getElementById(decodeURIComponent(href.slice(1)))?.scrollIntoView(); } catch (_) { /* Invalid authored anchor. */ }
        }
      });
      for (const link of root.querySelectorAll('a')) {
        if (small || !link.getAttribute('href')?.startsWith('#')) link.removeAttribute('href');
      }
      if (data.authorText?.runs.length) window.ReadMDAuthorText?.apply(root, data.authorText);
      ReadMDExportHtmlLayout.mount(root, opts.htmlLayout, {links: !small, contentsLabel: i18n.t('tabs.toc'), tone: opts.htmlTheme});
      if (data.tableTracks) window.ReadMDTableView?.observe(root, root, (opts.table?.widthPct || 100) / 100);
      renderMath(root);
      awaitHtmlPreviewSurface(host, root, generation);
    };
    frame.srcdoc = '<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src \'none\'; style-src \'unsafe-inline\'; img-src data:; font-src data:; base-uri \'none\'; form-action \'none\'"><meta name="viewport" content="width=device-width,initial-scale=1"></head><body><article id="content" class="readmd-export"></article></body></html>';
    host.setAttribute('aria-busy', 'true');
    host.replaceChildren(frame);
  }
}
