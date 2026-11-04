'use strict';
// --source-test is a UI-only VM projection check. Browser mode requires root's
// actual newly bundled boot, production CSP/token and an explicit frozen core.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm'), assert = require('node:assert/strict');
const { ROOT, sha, until, openOwned } = require('../tools/test/owned-browser-host.cjs');
const argv = process.argv.slice(2), arg = name => { const at = argv.indexOf(name); return at < 0 ? undefined : argv[at + 1]; };
const read = name => fs.readFileSync(path.join(ROOT, name));
const IDS = ['btn-open', 'btn-palette', 'btn-search', 'btn-ai', 'btn-graph'];
const KEYS = ['toolbar.open', 'editor.command', 'toolbar.search', 'toolbar.ai', 'graph.title'];
const sourceFiles = ['assets/index.html', 'assets/style.css', 'assets/css/shell.css', 'assets/js/shell/shell.js', 'assets/readmd.boot.js',
  'assets/app.js', 'assets/js/core/tabs.js', 'assets/js/core/modal.js', 'assets/js/editor/review.js',
  'assets/js/reader/enhance.js', 'assets/css/panels.css'];
const binding = () => Object.fromEntries(sourceFiles.map(file => [file, sha(read(file))]));

function sourceChecks() {
  const checks = [], source = read('assets/js/shell/shell.js').toString(), html = read('assets/index.html').toString();
  function pass(name, fn) { fn(); checks.push(name); }
  class Node {
    constructor(tag = 'div') { this.tagName = tag; this.children = []; this.parentNode = null; this.attrs = {}; this.dataset = {}; this.events = {}; this.hidden = false; this.disabled = false; this.textContent = ''; this.classes = new Set(); this.isConnected = true;
      this.classList = { contains: name => this.classes.has(name), toggle: (name, enabled) => { if (enabled ?? !this.classes.has(name)) this.classes.add(name); else this.classes.delete(name); }, add: name => this.classes.add(name), remove: name => this.classes.delete(name) };
    }
    set className(value) { this.classes = new Set(value.split(/\s+/).filter(Boolean)); }
    get className() { return [...this.classes].join(' '); }
    setAttribute(name, value) { this.attrs[name] = String(value); }
    getAttribute(name) { return this.attrs[name] ?? null; }
    hasAttribute(name) { return name in this.attrs; }
    removeAttribute(name) { delete this.attrs[name]; }
    remove() { if (this.parentNode) { this.parentNode.children = this.parentNode.children.filter(node => node !== this); this.parentNode = null; } }
    insertAt(node, index) { node.remove(); node.parentNode = this; this.children.splice(index, 0, node); }
    append(...nodes) { for (const node of nodes) this.insertAt(node, this.children.length); }
    prepend(node) { this.insertAt(node, 0); }
    before(node) { this.parentNode.insertAt(node, this.parentNode.children.indexOf(this)); }
    after(node) { this.parentNode.insertAt(node, this.parentNode.children.indexOf(this) + 1); }
    contains(node) { return node === this || this.children.some(child => child.contains(node)); }
    closest(selector) { for (let node = this; node; node = node.parentNode) if (selector.split(',').some(part => part === '.hidden' && node.classList.contains('hidden') || part === '[hidden]' && node.hidden || part === '[inert]' && node.hasAttribute('inert'))) return node; return null; }
    getClientRects() { return this.isConnected && !this.closest('.hidden,[hidden]') ? [{}] : []; }
    querySelector(selector) { return this.children.flatMap(node => [node, ...node.all()]).find(node => selector === '[data-i18n="shell.group.general"]' && node.dataset.i18n === 'shell.group.general') || null; }
    all() { return this.children.flatMap(node => [node, ...node.all()]); }
    focus() { document.activeElement = this; }
    addEventListener(type, listener) { (this.events[type] ??= []).push(listener); }
    click() { for (const listener of this.events.click || []) listener({ target: this }); }
  }
  const document = { activeElement: null, body: new Node('body'), createComment: () => new Node('#comment'), createElement: tag => new Node(tag) };
  const nodes = new Map(), node = id => { const value = new Node(); value.id = id; nodes.set(id, value); return value; };
  const toolbar = node('toolbar'), left = new Node(), right = new Node(), menu = node('more-menu');
  const group = node('shell-compact-group'), actions = node('shell-compact-actions'), heading = new Node('span');
  heading.dataset.i18n = 'shell.group.general'; group.className = 'more-group open'; group.append(heading, actions); menu.append(group); right.append(menu); toolbar.append(left, right);
  for (const id of ['btn-toc', 'btn-open', 'btn-palette']) { const button = node(id); button.className = 'tb-btn'; left.append(button); }
  for (const id of ['btn-graph', 'btn-search', 'btn-edit', 'rd-tools', 'btn-more', 'btn-ai']) { const button = node(id); button.className = 'tb-btn'; right.insertAt(button, right.children.length - 1); }
  nodes.get('btn-graph').classList.add('hidden');
  const originals = IDS.map(id => nodes.get(id)), listenerCalls = {};
  for (const button of originals) button.addEventListener('click', () => { listenerCalls[button.id] = (listenerCalls[button.id] || 0) + 1; });
  const buttonOrder = parent => parent.children.filter(node => node.id).map(node => node.id);
  const originalOrder = [buttonOrder(left), buttonOrder(right)];
  const observers = [], media = { matches: false, addEventListener(_type, listener) { this.change = listener; } }, events = {}, frames = new Map();
  let language = 'zh-CN', activeModal = null, frameId = 0;
  const context = { document, $: id => nodes.get(id), getComputedStyle: node => ({ display: node.closest('.hidden,[hidden]') ? 'none' : 'block', visibility: node.visibility || 'visible' }), requestAnimationFrame(fn) { frames.set(++frameId, fn); return frameId; }, cancelAnimationFrame(id) { frames.delete(id); },
    MutationObserver: class { constructor(fn) { this.fn = fn; this.records = []; } observe(target) { const record = { target, fn: this.fn, active: true }; this.records.push(record); observers.push(record); } disconnect() { this.records.forEach(record => { record.active = false; }); } },
    closeMoreMenu() { menu.classList.remove('open'); }, window: { ReadMDKeys: { isMac: true }, matchMedia: query => { assert.equal(query, '(max-width: 640px)'); return media; },
      ReadMDModal: { top: () => activeModal }, i18n: { t: key => JSON.parse(read('assets/i18n/' + language + '.json'))[key] }, addEventListener(type, listener) { (events[type] ??= []).push(listener); } } };
  const start = source.indexOf('  const compactGroup ='), end = source.indexOf('  const shortcuts =', start);
  assert(start >= 0 && end > start); vm.runInNewContext(source.slice(start, end), context);
  const resize = narrow => { media.matches = narrow; media.change(); };
  const flush = button => observers.filter(item => item.active && item.target === button).forEach(item => item.fn());
  const paint = () => { const work = [...frames.values()]; frames.clear(); work.forEach(fn => fn()); };
  pass('one HTML container; original buttons remain unique', () => { for (const id of [...IDS, 'shell-compact-group', 'shell-compact-actions']) assert.equal([...html.matchAll(new RegExp('\\bid="' + id + '"', 'g'))].length, 1); });
  pass('desktop excludes hidden compact group from original menu keyboard items', () => { assert.equal(group.parentNode, null); assert.deepEqual([buttonOrder(left), buttonOrder(right)], originalOrder); });
  nodes.get('btn-search').focus(); resize(true);
  pass('compact moves the same five original nodes and focuses visible More', () => { originals.forEach((button, index) => { assert.equal(button.parentNode.dataset.shellCompact, IDS[index]); assert.equal(button.getAttribute('role'), 'menuitem'); assert(button.hasAttribute('data-shell-menu-action')); }); assert.equal(document.activeElement, nodes.get('btn-more')); assert.equal(group.parentNode, menu); });
  pass('hidden graph produces no blank wrapper or keyboard menu item', () => { const button = nodes.get('btn-graph'); assert(button.parentNode.hidden); assert(!button.classList.contains('more-item')); button.classList.remove('hidden'); flush(button); assert(!button.parentNode.hidden); assert(button.classList.contains('more-item')); });
  pass('graph hidden attribute removes focus eligibility without changing service flags', () => { const button = nodes.get('btn-graph'); button.focus(); button.hidden = true; flush(button); assert(button.hidden); assert(button.parentNode.hidden); assert(!button.classList.contains('more-item')); assert.equal(document.activeElement, nodes.get('btn-more')); button.hidden = false; flush(button); });
  pass('live locale applies real packaged labels and platform shortcut notation', () => { language = 'ar'; events['readmd:language-changed'].forEach(fn => fn()); const strings = JSON.parse(read('assets/i18n/ar.json')); originals.forEach((button, index) => { const label = button.parentNode.children.find(child => child.dataset.i18n === KEYS[index]); assert.equal(label.textContent, strings[KEYS[index]].replace(/\bCtrl\b/g, 'Cmd')); }); assert.equal(heading.textContent, strings['shell.group.general']); });
  originals[0].click(); originals[3].click();
  pass('relocation preserves each original single business listener', () => { assert.deepEqual(listenerCalls, { 'btn-open': 1, 'btn-ai': 1 }); originals.forEach(button => assert.equal(button.events.click.length, 1)); });
  nodes.get('btn-search').focus(); menu.classList.add('open'); resize(false);
  pass('desktop restores exact original order, roles, flags and focused node', () => { assert.deepEqual([buttonOrder(left), buttonOrder(right)], originalOrder); originals.forEach(button => { assert.equal(button.getAttribute('role'), null); assert(!button.hasAttribute('data-shell-menu-action')); assert(!button.classList.contains('more-item')); }); assert.equal(document.activeElement, nodes.get('btn-search')); assert.equal(group.parentNode, null); assert(!menu.classList.contains('open')); assert.equal(nodes.get('rd-tools').parentNode, right); });
  pass('repeated resize keeps one node and one original listener', () => { for (let i = 0; i < 8; i++) { resize(true); resize(false); } originals.forEach(button => assert.equal(button.events.click.length, 1)); assert.deepEqual([buttonOrder(left), buttonOrder(right)], originalOrder); });
  const modal = new Node(), field = new Node('input'), editor = new Node('textarea'); modal.append(field); document.body.append(toolbar, modal, editor);
  const opened = () => { activeModal = modal; modal.classList.remove('hidden'); nodes.get('btn-palette').focus(); events['readmd:modal-opened'].forEach(fn => fn()); field.focus(); };
  const closed = focus => { activeModal = null; modal.classList.add('hidden'); focus.focus(); flush(modal); paint(); };
  pass('moved modal opener repairs a hidden result after original close', () => { opened(); resize(true); closed(document.body); assert.equal(document.activeElement, nodes.get('btn-more')); assert.equal(observers.filter(item => item.active && item.target === modal).length, 0); });
  pass('explicit return to a visible editor wins over compact focus fallback', () => { resize(false); opened(); resize(true); closed(editor); assert.equal(document.activeElement, editor); });
  pass('inverse resize keeps the original visible opener and repeated resize has one temporary observer', () => { resize(false); opened(); for (let i = 0; i < 8; i++) { resize(i % 2 === 0); assert.equal(observers.filter(item => item.active && item.target === modal).length, 1); } closed(nodes.get('btn-palette')); assert.equal(document.activeElement, nodes.get('btn-palette')); assert.equal(observers.filter(item => item.active && item.target === modal).length, 0); assert.equal(frames.size, 0); });
  pass('a modal with an unrelated opener never acquires a compact close observer', () => { activeModal = modal; modal.classList.remove('hidden'); editor.focus(); events['readmd:modal-opened'].forEach(fn => fn()); field.focus(); resize(true); assert.equal(observers.filter(item => item.active && item.target === modal).length, 0); activeModal = null; modal.classList.add('hidden'); resize(false); });
  pass('all reused compact keys exist in all46 packaged locales', () => { const locales = fs.readdirSync(ROOT + '/assets/i18n').filter(name => name.endsWith('.json') && name !== 'meta.json'); assert.equal(locales.length, 46); for (const locale of locales) { const strings = JSON.parse(read('assets/i18n/' + locale)); for (const key of [...KEYS, 'shell.group.general']) assert.equal(typeof strings[key], 'string', locale + ':' + key); } });
  return { passed: true, sourceOnly: true, checks, browserCertified: false, source: binding() };
}

async function browserCheck() {
  const binary = arg('--bin'), output = arg('--evidence'), playwright = arg('--playwright');
  assert(binary && path.isAbsolute(binary) && output && path.isAbsolute(output), 'Explicit absolute --bin and --evidence required');
  const report = { passed: false, driverSha256: sha(fs.readFileSync(__filename)), binarySha256: sha(fs.readFileSync(binary)),
    sourceBefore: binding(), cases: [], checks: [], realCspAndToken: true, actualHeadlessBrowser: true,
    modelsRequested: false, nativeCertified: false, forbiddenRequests: [] };
  let test, phase = 'bootstrap';
  try {
    test = await openOwned(output, async (_api, paths) => {
      fs.writeFileSync(path.join(paths.profile, 'settings.json'), JSON.stringify({ lang: 'zh-CN', autoCheckUpdate: false }));
      fs.writeFileSync(path.join(paths.documents, 'mobile-owned.md'), '# Owned mobile document\n\nSynthetic search target.\n');
      fs.writeFileSync(path.join(paths.documents, 'graph-owned.md'), '# Owned graph document\n\n[[mobile-owned]]\n');
    }, { binary, playwright });
    const { page } = test, cdp = await test.context.newCDPSession(page);
    await cdp.send('Emulation.setTouchEmulationEnabled', { enabled: true, maxTouchPoints: 1 });
    page.on('request', request => { if (request.method() !== 'POST') return; const route = new URL(request.url()).pathname;
      if (/^\/api\/ai\/(chat|completion)$/.test(route) || route === '/api/ai/tasks' && ['start', 'resume'].includes(request.postDataJSON()?.action)) report.forbiddenRequests.push(route); });
    assert.equal(await page.evaluate(() => $('btn-open').previousSibling?.nodeType === Node.COMMENT_NODE && $('btn-open').previousSibling.data === 'btn-open desktop position'), true, 'A new root-bundled compact boot is required; do not certify old boot');
    await page.evaluate(async ids => { setEditorPref('aiCompletion', false); await ReadMDShellLoader.load(); window.__compactOriginalNodes = Object.fromEntries(ids.map(id => [id, $(id)])); }, IDS);
    await page.evaluate(() => { window.__compactMoreFocus = []; document.addEventListener('focusin', event => { if (event.target.id === 'btn-more') window.__compactMoreFocus.push(performance.now()); }); });
    report.isMacShortcut = await page.evaluate(() => Boolean(ReadMDKeys.isMac));
    const strings = lang => JSON.parse(read('assets/i18n/' + lang + '.json'));
    async function stable() { await page.evaluate(async () => Promise.all(document.getAnimations().filter(a => Number.isFinite(a.effect?.getTiming().iterations)).map(a => a.finished.catch(() => {})))); }
    async function more() { if (await page.locator('#btn-more').getAttribute('aria-expanded') !== 'true') await page.locator('#btn-more').click(); if (!await page.locator('#shell-compact-group').evaluate(node => node.classList.contains('open'))) await page.locator('#shell-compact-group > .more-group-header').click(); }
    async function modalProjection(width) {
      await until(() => page.evaluate(({ ids, width }) => {
        const compact = width <= 640, group = $('shell-compact-group');
        return innerWidth === width && (compact ? group && !group.hidden && $('more-menu').contains(group) : !group) && ids.every(id => {
          const button = $(id);
          return compact ? button.closest('#shell-compact-actions') && button.getAttribute('role') === 'menuitem'
            : button.parentElement.classList.contains(['btn-open', 'btn-palette'].includes(id) ? 'tb-left' : 'tb-right') && button.getAttribute('role') === null;
        });
      }, { ids: IDS, width }), 'actual compact/desktop parent and ARIA projection at ' + width);
      const value = await page.evaluate(width => {
        const header = $('app-titlebar'), palette = $('btn-palette'), r = palette.getBoundingClientRect(), modal = ReadMDModal.top();
        return { width: innerWidth, expectedWidth: width, compact: matchMedia('(max-width:640px)').matches,
          headerOverflow: header.scrollWidth - header.clientWidth, modalOpen: ReadMDPalette.isOpen(),
          focusInModal: !!modal && modal.contains(document.activeElement), active: document.activeElement.id || document.activeElement.tagName,
          palette: { parent: palette.parentElement.className, visible: !!palette.getClientRects().length, x: r.x, right: r.right },
          groupConnected: !!$('shell-compact-group'), menuOpen: $('more-menu').classList.contains('open') };
      }, width);
      (report.focusProjections ??= []).push({ phase, ...value });
      assert.equal(value.width, width); assert(value.headerOverflow <= 1); assert(value.modalOpen && value.focusInModal, 'Resize preserves the open original modal and its focused field');
      assert.equal(value.compact, width <= 640); assert.equal(value.menuOpen, false);
      if (width > 640) assert(value.palette.visible && value.palette.parent === 'tb-group tb-left' && value.palette.x >= 0 && value.palette.right <= width, 'Stable desktop original palette is restored and in viewport before closing');
    }
    async function geometry(label, width, height, { lang = 'zh-CN', theme = 'light', mac = false } = {}) {
      phase = label;
      await page.setViewportSize({ width, height });
      await page.evaluate(async ({ lang, theme, mac }) => { await i18n.setLanguage(lang, false); state.theme = theme; applySettings(); document.body.classList.toggle('native-macos', mac); }, { lang, theme, mac });
      await stable();
      const value = await page.evaluate(() => {
        const rect = node => { const r = node.getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height, right: r.right, bottom: r.bottom }; };
        const shown = node => node.getClientRects().length && getComputedStyle(node).visibility !== 'hidden' && !node.closest('.hidden,[hidden]');
        const header = $('app-titlebar'), buttons = [...header.querySelectorAll('button')].filter(shown).map(node => {
          const r = rect(node), hit = document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2);
          return { id: node.id, ...r, disabled: node.disabled, hit: !!hit && (node === hit || node.contains(hit)), name: node.getAttribute('aria-label') || node.title || node.textContent.trim() };
        });
        return { width: innerWidth, height: innerHeight, header: { ...rect(header), overflow: header.scrollWidth - header.clientWidth },
          overflow: document.documentElement.scrollWidth - innerWidth, buttons, compact: matchMedia('(max-width:640px)').matches,
          identity: rect(document.querySelector('.titlebar-identity')), actions: rect($('titlebar-actions')) };
      });
      report.cases.push({ label, ...value });
      await page.screenshot({ path: path.join(output, label + '.png') });
      assert(value.overflow <= 0 && value.header.overflow <= 1, label + ': no horizontal page/header overflow');
      assert.equal(value.header.height, 52);
      for (const button of value.buttons) { assert(button.width >= 44 && button.height >= 44 && button.name, label + ':44px and named ' + button.id); assert(button.x >= 0 && button.right <= width && button.y >= 0 && button.bottom <= height, label + ':viewport ' + button.id); if (!button.disabled) assert(button.hit, label + ':hit ' + button.id); }
      for (let i = 0; i < value.buttons.length; i++) for (let j = i + 1; j < value.buttons.length; j++) { const a = value.buttons[i], b = value.buttons[j]; assert(a.right <= b.x + .01 || b.right <= a.x + .01 || a.bottom <= b.y + .01 || b.bottom <= a.y + .01, label + ':nonoverlap ' + a.id + '/' + b.id); }
      if (mac) assert(value.actions.x >= 88 && value.actions.right <= value.identity.x, label + ':desktop Mac safe inset and right brand');
      if (value.compact) {
        await more(); await stable();
        const menu = await page.locator('#more-menu').boundingBox(); assert(menu.x >= 0 && menu.x + menu.width <= width && menu.y + menu.height <= height);
        const items = await page.locator('#shell-compact-actions > .shell-compact-action').evaluateAll(nodes => nodes.map(node => {
          const button = node.querySelector('button'), label = node.querySelector('.shell-menu-label'), r = button.getBoundingClientRect();
          return { id: button.id, hidden: node.hidden, label: label.textContent, key: label.dataset.i18n, width: r.width, height: r.height, role: button.getAttribute('role') };
        }));
        assert.deepEqual(items.map(item => item.id), IDS);
        for (const item of items) if (!item.hidden) { assert(item.width >= 44 && item.height >= 44); assert.equal(item.role, 'menuitem'); const expected = strings(lang)[item.key]; assert.equal(item.label, report.isMacShortcut ? expected.replace(/\bCtrl\b/g, 'Cmd') : expected); }
        assert.equal(await page.evaluate(ids => ids.every(id => $(id) === window.__compactOriginalNodes[id] && document.querySelectorAll('#' + id).length === 1), IDS), true);
        await page.screenshot({ path: path.join(output, label + '-more.png') });
        await page.locator('#btn-more').focus(); await page.keyboard.press('Enter'); await page.keyboard.press('Enter');
        assert.equal(await page.locator('#more-menu').evaluate(node => node.classList.contains('open')), true, 'More keyboard toggle remains usable');
        await page.keyboard.press('Escape');
      } else assert.equal(await page.locator('#more-menu #shell-compact-group').count(), 0, 'Desktop keyboard list excludes compact header');
      report.checks.push(label + ':real layout/targets/menu/identity');
    }
    await geometry('390-welcome', 390, 844);
    await page.evaluate(file => loadFile(file), path.join(test.documents, 'mobile-owned.md'));
    await until(() => page.locator('#toast').evaluate(node => node.classList.contains('hidden')), 'file toast expiry');
    for (const [width, height] of [[360, 740], [390, 844], [412, 844], [600, 820], [640, 820]]) await geometry(width + '-reading', width, height);
    await geometry('390-reading-ar-dark', 390, 844, { lang: 'ar', theme: 'dark' });
    phase = 'original search menu handler'; await more(); await page.locator('#btn-search').click(); assert(await page.locator('#search-bar').isVisible()); assert.equal(await page.locator('#more-menu').evaluate(node => node.classList.contains('open')), false); await page.locator('#search-input').fill('target'); await page.keyboard.press('Escape'); report.checks.push('Original search handler opens its real field from compact More');
    phase = 'original command menu handler'; await more(); await page.locator('#btn-palette').click(); await until(() => page.evaluate(() => ReadMDPalette.isOpen()), 'real command palette'); await page.keyboard.press('Escape'); assert.equal(await page.evaluate(() => document.activeElement.id), 'btn-more'); report.checks.push('Original command handler and visible More opener focus return');
    phase = 'fresh profile AI safety preconditions'; const config = await test.api('/api/ai/config');
    assert.deepEqual(config.current, {}, 'Fresh owned profile has no selected model/connection'); assert(Array.isArray(config.custom)); assert.equal(config.custom.length, 0, 'No owned profile AI connection');
    assert.equal(await page.evaluate(() => editorPrefs.aiCompletion), false, 'Automatic completion remains disabled before AI panel entry'); report.aiPreconditions = { emptyCurrent: true, noConnections: true, automaticCompletion: false };
    phase = 'original AI menu handler'; await more(); await page.locator('#btn-ai').click(); assert.equal(await page.locator('#more-menu').evaluate(node => node.classList.contains('open')), false); await until(() => page.locator('#ai-panel').isVisible(), 'real AI panel opens without sending'); await page.locator('#ai-close').click(); report.checks.push('Original AI panel entry only; no inference or replacement handler');
    phase = 'reader prefs stays by Edit'; await page.locator('#rd-prefs-btn').click(); assert(await page.locator('#rd-prefs').isVisible()); await page.keyboard.press('Escape'); assert.equal(await page.evaluate(() => $('rd-tools').closest('#shell-compact-actions') === null), true); report.checks.push('Original reader preferences remain outside hidden menu and open normally');
    phase = 'real graph visibility'; await page.evaluate(file => loadFile(file), path.join(test.documents, 'graph-owned.md')); await until(() => page.locator('#btn-graph').evaluate(node => !node.classList.contains('hidden')), 'real wikilink graph entry');
    await geometry('641-reading-graph', 641, 820);
    await page.setViewportSize({ width: 390, height: 844 });
    await until(() => page.evaluate(() => $('shell-compact-actions')?.contains($('btn-graph'))), 'original graph returns to compact menu');
    await more(); assert.equal(await page.locator('[data-shell-compact="btn-graph"]').evaluate(node => node.hidden), false); await page.locator('#btn-graph').click(); await until(() => page.locator('#graph-modal').isVisible(), 'original graph entry opens'); await page.locator('#graph-btn-close').click(); await until(() => page.locator('#graph-modal').isHidden(), 'original graph closes');
    await page.evaluate(file => loadFile(file), path.join(test.documents, 'mobile-owned.md')); await until(() => page.locator('[data-shell-compact="btn-graph"]').evaluate(node => node.hidden), 'real graph entry hidden again'); report.checks.push('Real document graph visibility tracks wrapper with no blank menu item');
    await page.evaluate(async () => { await toggleEdit(); setPvLayout('none'); });
    for (const [width, height] of [[360, 740], [390, 820], [412, 844]]) await geometry(width + '-editing', width, height);
    phase = 'desktop and compact resize restoration'; await page.setViewportSize({ width: 390, height: 844 }); await more(); await page.locator('#btn-search').focus(); await page.setViewportSize({ width: 1024, height: 680 }); await until(() => page.evaluate(() => $('btn-search').parentElement.classList.contains('tb-right')), 'desktop restore'); assert.equal(await page.evaluate(() => document.activeElement.id), 'btn-search');
    await geometry('1024-editing', 1024, 680); await geometry('1160-editing-mac', 1160, 820, { mac: true });
    await page.evaluate(() => document.body.classList.remove('native-macos'));
    phase = 'command modal across compact breakpoint'; await page.locator('#btn-palette').click(); await until(() => page.evaluate(() => ReadMDPalette.isOpen()), 'desktop command open before resize');
    await page.evaluate(() => { window.__compactMoreFocus.length = 0; });
    await page.setViewportSize({ width: 390, height: 844 }); await modalProjection(390);
    await page.keyboard.press('Escape'); await until(() => page.evaluate(() => document.activeElement.id === 'btn-more'), 'visible compact focus after modal resize'); assert.equal(await page.evaluate(() => window.__compactMoreFocus.length), 1); report.checks.push('Original command modal stays open across resize and closes once to visible compact chrome');
    phase = 'explicit editor return is preserved'; await page.setViewportSize({ width: 1024, height: 680 }); await page.locator('#btn-palette').click(); await until(() => page.evaluate(() => ReadMDPalette.isOpen()), 'negative-control command opens');
    await page.evaluate(() => { window.__compactMoreFocus.length = 0; }); await page.setViewportSize({ width: 390, height: 844 }); await modalProjection(390);
    await page.evaluate(() => { ReadMDPalette.close(); cmView.focus(); }); await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    assert.equal(await page.evaluate(() => cmView.hasFocus), true, 'Explicit original caller editor focus remains'); assert.equal(await page.evaluate(() => window.__compactMoreFocus.length), 0); report.checks.push('Original modal close with explicit visible editor focus is not overridden');
    phase = 'inverse and repeated modal resize'; await page.setViewportSize({ width: 1024, height: 680 }); await page.locator('#btn-palette').click(); await until(() => page.evaluate(() => ReadMDPalette.isOpen()), 'inverse command opens'); await page.evaluate(() => { window.__compactMoreFocus.length = 0; });
    for (let i = 0; i < 8; i++) { const width = i % 2 === 0 ? 390 : 1024; await page.setViewportSize({ width, height: width === 390 ? 844 : 680 }); await modalProjection(width); }
    await page.keyboard.press('Escape'); await until(() => page.evaluate(() => document.activeElement.id === 'btn-palette'), 'original visible desktop opener after repeated resize'); assert.equal(await page.evaluate(() => window.__compactMoreFocus.length), 0); report.checks.push('Repeated and inverse resize retains original visible opener; VM separately counts one active temporary observer');
    await page.setViewportSize({ width: 1024, height: 680 }); await page.locator('#btn-search').focus();
    await page.setViewportSize({ width: 390, height: 844 }); await until(() => page.evaluate(() => $('shell-compact-actions')?.contains($('btn-search'))), 'compact return'); assert.equal(await page.evaluate(() => document.activeElement.id), 'btn-more');
    await geometry('390-roundtrip', 390, 844); assert.deepEqual(report.forbiddenRequests, []); assert.deepEqual(test.errors, []); report.passed = true;
  } catch (error) { report.failure = { phase, message: error.message, stack: error.stack }; if (test) { report.failure.focus = await test.page.evaluate(() => ({ active: document.activeElement.id || document.activeElement.tagName, activeVisible: !!document.activeElement.getClientRects().length, modal: ReadMDModal.top()?.id || null, menuOpen: $('more-menu').classList.contains('open') })).catch(() => null); await test.page.screenshot({ path: path.join(output, 'first-failure.png') }).catch(() => {}); } process.exitCode = 1; }
  finally { if (test) { report.pageErrors = test.errors; await test.close(); } report.sourceAfter = binding(); report.sourceUnchanged = JSON.stringify(report.sourceBefore) === JSON.stringify(report.sourceAfter); report.ownedScratchRemoved = ['tmp', 'home', 'browser-profile'].every(name => !fs.existsSync(path.join(output, name))); fs.writeFileSync(path.join(output, 'report.json'), JSON.stringify(report, null, 2) + '\n'); console.log(JSON.stringify({ passed: report.passed, cases: report.cases.length, failure: report.failure, sourceUnchanged: report.sourceUnchanged, scratchRemoved: report.ownedScratchRemoved })); }
}
if (argv.includes('--source-test')) { const result = sourceChecks(); const output = arg('--report'); if (output && output !== '--source-test') fs.writeFileSync(output, JSON.stringify(result, null, 2) + '\n', { flag: 'wx' }); console.log(JSON.stringify(result)); }
else browserCheck().catch(error => { console.error(error); process.exitCode = 1; });
