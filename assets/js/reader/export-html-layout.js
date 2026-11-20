'use strict';

globalThis.ReadMDExportHtmlLayout = (() => {
  const layouts = new Set(['reading', 'docs', 'magazine']);

  function mount(root, layout, { links = true, contentsLabel = 'Contents', tableLabel = 'Table', tone = 'light' } = {}) {
    if (!root || !layouts.has(layout)) return;
    const doc = root.ownerDocument;
    // A preview can reuse its host. Keep the source DOM while changing layout.
    const previous = root.firstElementChild;
    if (previous?.classList.contains('readmd-html-layout') && root.children.length === 1) {
      const article = previous.querySelector('.readmd-layout-article');
      if (article) root.replaceChildren(...article.childNodes);
    }
    const wrap = doc.createElement('div');
    wrap.className = 'readmd-html-layout';
    wrap.dataset.layout = layout;
    wrap.dataset.tone = ['dark', 'sepia'].includes(tone) ? tone : 'light';
    const flow = doc.createElement('div');
    flow.className = 'readmd-layout-flow';
    const article = doc.createElement('div');
    article.className = 'readmd-layout-article';
    while (root.firstChild) article.appendChild(root.firstChild);

    for (const table of article.querySelectorAll('table')) {
      if (table.closest('.readmd-layout-table') || table.parentElement?.closest('table')) continue;
      const scroll = doc.createElement('div');
      scroll.className = 'readmd-layout-table';
      scroll.setAttribute('role', 'region');
      scroll.setAttribute('aria-label', table.caption?.textContent?.trim() || tableLabel);
      scroll.tabIndex = 0;
      table.before(scroll);
      scroll.appendChild(table);
    }

    if (layout === 'docs') {
      const headings = [...article.querySelectorAll('h2,h3,h4')].filter(heading => !heading.closest('table,blockquote')).slice(0, 200);
      if (headings.length) {
        const navigation = doc.createElement('nav');
        navigation.className = 'readmd-layout-nav';
        navigation.setAttribute('aria-label', contentsLabel);
        const title = doc.createElement('strong');
        title.textContent = contentsLabel;
        const list = doc.createElement('ul');
        // Reserve authored IDs in later headings and body content too.
        const owners = new Map();
        for (const node of article.querySelectorAll('[id]')) if (!owners.has(node.id)) owners.set(node.id, node);
        const used = new Set(owners.keys());
        const assigned = new Set();
        let nextAnchor = 0;
        for (const heading of headings) {
          const item = doc.createElement('li');
          item.dataset.level = heading.localName;
          const link = doc.createElement(links ? 'a' : 'span');
          if (links) {
            if (!heading.id || owners.get(heading.id) !== heading || assigned.has(heading.id) || doc.getElementById(heading.id)) {
              let id;
              do { id = 'readmd-section-' + ++nextAnchor; }
              while (used.has(id) || doc.getElementById(id));
              heading.id = id;
              used.add(id);
            }
            assigned.add(heading.id);
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
        flow.appendChild(navigation);
      }
    }
    flow.appendChild(article);
    wrap.appendChild(flow);
    root.replaceChildren(wrap);
  }
  return Object.freeze({ mount });
})();
