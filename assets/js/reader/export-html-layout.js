'use strict';

globalThis.ReadMDExportHtmlLayout = (() => {
  const layouts = new Set(['reading', 'docs', 'magazine']);
  let nextAnchor = 0;
  function mount(root, layout, { links = true, contentsLabel, tone = "light" } = {}) {
    if (!root || !layouts.has(layout)) return;
    const doc = root.ownerDocument;
    const wrap = doc.createElement('div');
    wrap.className = 'readmd-html-layout';
    wrap.dataset.layout = layout;
    wrap.dataset.tone = tone;
    const article = doc.createElement('div');
    article.className = 'readmd-layout-article';
    while (root.firstChild) article.appendChild(root.firstChild);
    if (layout === 'docs') {
      const headings = [...article.querySelectorAll('h2,h3,h4')].slice(0, 200);
      if (headings.length) {
        const navigation = doc.createElement('nav');
        navigation.className = 'readmd-layout-nav';
        const label = contentsLabel || (/^zh/i.test(doc.documentElement.lang) ? '目录' : 'Contents');
        navigation.setAttribute('aria-label', label);
        const title = doc.createElement('strong');
        title.textContent = label;
        const list = doc.createElement('ul');
        const ids = new Set();
        for (const heading of headings) {
          const item = doc.createElement('li');
          item.dataset.level = heading.localName;
          const link = doc.createElement(links ? 'a' : 'span');
          if (links) {
            if (!heading.id || ids.has(heading.id) || doc.getElementById(heading.id)) {
              do { heading.id = 'readmd-section-' + ++nextAnchor; }
              while (ids.has(heading.id) || doc.getElementById(heading.id));
            }
            ids.add(heading.id);
            link.href = '#' + encodeURIComponent(heading.id);
            if (!doc.body.dataset.readmdHtmlLayout) link.addEventListener('click', event => {
              event.preventDefault();
              heading.scrollIntoView({ block: 'start' });
            });
          }
          link.textContent = heading.textContent;
          item.appendChild(link);
          list.appendChild(item);
        }
        navigation.append(title, list);
        wrap.appendChild(navigation);
      }
    }
    wrap.appendChild(article);
    root.replaceChildren(wrap);
  }
  return Object.freeze({ mount });
})();
