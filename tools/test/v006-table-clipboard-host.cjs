'use strict';
// Production editor functions and the checked-in CM/marked bundles. The small
// view below substitutes DOM geometry only; documents, syntax and history are CM.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ROOT = path.resolve(__dirname, '../..');
const read = file => fs.readFileSync(path.join(ROOT, file), 'utf8');

function host({ source = '', selection, file = 'owned-table.md', apiFetch } = {}) {
  const calls = [], toasts = [], listeners = new Map();
  const tab = { id: 'table-qa', path: file, name: 'owned-table.md', original: source, content: source, encoding: 'utf-8', revision: '', mtime: 1 };
  const state = { editing: true, original: source, file, tabs: [tab], encoding: 'utf-8' };
  const area = { value: source, selectionStart: 0, selectionEnd: 0, focus() {} };
  const context = { console, setTimeout, clearTimeout, setInterval, clearInterval,
    document: { documentElement: { style: {} }, addEventListener(name, fn) { const list = listeners.get(name) || []; list.push(fn); listeners.set(name, list); },
      getElementById: id => id === 'edit-area' ? area : null, querySelectorAll: () => [] },
    localStorage: { getItem: () => null }, addEventListener() {},
    state, getActiveTab: () => tab, $: id => id === 'edit-area' ? area : null,
    i18n: { t: (key, params) => params?.count === undefined ? key : `${key}:${params.count}` },
    showToast: message => toasts.push(message), ReadMDInlineCompletion: { accept: () => false },
    ReadMDTask: { run: (_, fn) => fn() }, renderActiveTab: async () => {},
    renderTabsBar() {}, updateUnloadGuard() {}, updateStatus() {},
    apiFetch: async (route, request) => {
      calls.push({ route, body: request?.body ? JSON.parse(request.body) : undefined });
      if (apiFetch) return apiFetch(route, request);
      return { ok: true, json: async () => ({ ok: true, mtime: 2, revision: 'saved-table-qa' }) };
    }
  };
  context.window = context;
  vm.createContext(context);
  for (const file of ['assets/vendor/codemirror.bundle.js', 'assets/vendor/marked.min.js',
    'assets/js/editor/md-transforms.js', 'assets/js/editor/editor.js', 'assets/js/editor/preview.js']) {
    vm.runInContext(read(file), context, { filename: file });
  }
  const CM = context.ReadMDCodeMirror;
  const view = { transactions: [], focused: 0, focus() { view.focused++; },
    dispatch(spec) {
      const transaction = spec instanceof CM.Transaction ? spec : view.state.update(spec);
      view.transactions.push(transaction);
      view.state = transaction.state;
    }
  };
  view.state = CM.EditorState.create({ doc: source, selection: selection || { anchor: source.replace(/\r\n?/g, '\n').length },
    extensions: [CM.history(), CM.markdown(), CM.EditorState.allowMultipleSelections.of(true)] });
  context.__tableView = view;
  vm.runInContext('cmView = __tableView; window.cmView = cmView;', context);

  function paste(text, types = {}) {
    const data = { 'text/plain': text, ...types };
    const event = { clipboardData: { items: [], files: [], types: Object.keys(data), getData: type => data[type] || '' },
      target: view, defaultPrevented: false, preventDefault() { this.defaultPrevented = true; } };
    const handled = context.cmHandlePaste(event, view);
    return { event, handled };
  }
  const key = name => context.cmPriorityKeymap().find(binding => binding.key === name).run(view);
  return { context, CM, view, tab, state, area, listeners, calls, toasts, paste, key,
    text: () => view.state.doc.toString(), html: () => context.marked.parse(view.state.doc.toString(), { gfm: true }),
    setSelection(anchor, head = anchor) { view.dispatch({ selection: { anchor, head } }); },
    undo: () => context.cmUndo(), redo: () => context.cmRedo(), save: () => context.saveEdit(),
    fallback() { vm.runInContext('cmView = null; window.cmView = null;', context); }
  };
}

function decodeEntities(text) {
  return text.replace(/&(#(?:x[0-9a-f]+|\d+)|amp|lt|gt|quot|apos);/gi, (_, entity) => {
    if (entity[0] === '#') return String.fromCodePoint(entity[1].toLowerCase() === 'x' ? parseInt(entity.slice(2), 16) : Number(entity.slice(1)));
    return { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'" }[entity.toLowerCase()];
  });
}
function tableCells(html) {
  return [...html.matchAll(/<tr[^>]*>([\s\S]*?)<\/tr>/g)].map(row => [...row[1].matchAll(/<t[hd][^>]*>([\s\S]*?)<\/t[hd]>/g)]
    .map(cell => decodeEntities(cell[1].replace(/<br\s*\/?>/g, '\n').replace(/<[^>]*>/g, ''))));
}

module.exports = { ROOT, host, tableCells };
