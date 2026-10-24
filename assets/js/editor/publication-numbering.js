'use strict';
/* Shared source-owned numbering controls. Hosts supply the existing editor;
   this component never opens a new panel, writes files or stores export styles. */
(() => {
  const FORMATS = Object.freeze([
    ['decimal', '1, 2, 3'], ['lowerRoman', 'i, ii, iii'], ['upperRoman', 'I, II, III'],
    ['lowerLetter', 'a, b, c'], ['upperLetter', 'A, B, C']
  ]);
  const KINDS = Object.freeze(['equation', 'figure', 'table']);
  const KEYS = Object.freeze(['title', 'hint', 'levels', 'level', 'format', 'start', 'restartAfter', 'never',
    'separator', 'suffix', 'space', 'tab', 'legacy', 'document', 'chapter', 'chapterLevel', 'customPrefix',
    'prefix', 'labelSeparator', 'captionSeparator', 'apply', 'clear', 'loading', 'requiresEditor',
    'customLevelText', 'levelText', 'customLevelSuffix', 'levelSuffix', 'legalNumbering', 'sequence']);
  const mounts = new Set();
  let serial = 0;
  const clone = value => value == null ? null : JSON.parse(JSON.stringify(value));
  const error = code => Object.assign(new Error(code), {code});
  const invalid = () => { throw error('publication_numbering_invalid'); };
  const bounded = (value, limit) => typeof value === 'string' && [...value].length <= limit && !/[\u0000-\u001F\u007F-\u009F]/u.test(value);
  const integer = (value, min, max) => {
    if (!/^\d+$/u.test(String(value))) return invalid();
    const number = Number(value); if (!Number.isSafeInteger(number) || number < min || number > max) return invalid();
    return number;
  };
  const format = value => FORMATS.some(([name]) => name === value) ? value : invalid();
  function validateConfig(config) {
    if (config == null) return null;
    if (!config || typeof config !== 'object' || Array.isArray(config) || Object.keys(config).some(key => !['headings', 'captions'].includes(key))) return invalid();
    const headings = config.headings;
    if (headings != null) {
      if (!Array.isArray(headings.levels) || headings.levels.length < 1 || headings.levels.length > 6 || !bounded(headings.separator ?? '.', 16) || (headings.separator ?? '.').includes('%') || !['space', 'tab', 'nothing'].includes(headings.suffix ?? 'space')) return invalid();
      headings.levels.forEach((rule, index) => { format(rule.format ?? 'decimal'); integer(rule.start ?? 1, 0, 10000); if (rule.restartAfter != null) integer(rule.restartAfter, 0, index);
        if (rule.text != null && !bounded(rule.text, 256) || rule.suffix != null && !['space', 'tab', 'nothing'].includes(rule.suffix) || rule.isLegal != null && typeof rule.isLegal !== 'boolean') return invalid(); });
    }
    const captions = config.captions ?? {};
    if (!captions || typeof captions !== 'object' || Array.isArray(captions) || Object.keys(captions).some(kind => !KINDS.includes(kind)) || !headings && !Object.keys(captions).length) return invalid();
    for (const rule of Object.values(captions)) {
      if (!rule || !['document', 'chapter'].includes(rule.scope ?? 'document')) return invalid();
      format(rule.format ?? 'decimal'); integer(rule.start ?? 1, 0, 10000);
      const chapterLevel = integer(rule.chapterLevel ?? 1, 1, 6);
      if (rule.scope === 'chapter' && (!headings || headings.levels.length < chapterLevel)) return invalid();
      if (!bounded(rule.separator ?? '.', 16) || !bounded(rule.labelSeparator ?? ' ', 16) || !bounded(rule.captionSeparator ?? ' — ', 32) || rule.prefix != null && !bounded(rule.prefix, 128)) return invalid();
      if (rule.sequence != null && (typeof rule.sequence !== 'string' || rule.sequence.length > 40 || !/^\p{Alphabetic}[\p{Alphabetic}\p{Number}_]*$/u.test(rule.sequence))) return invalid();
    }
    return config;
  }
  function createDraft(config) {
    validateConfig(config);
    const heading = config?.headings;
    const levels = Array.from({length: 6}, (_, index) => {
      const rule = heading?.levels[index] || {};
      return {format: rule.format ?? 'decimal', start: String(rule.start ?? 1), restartAfter: String(rule.restartAfter ?? index),
        customText: rule.text != null, text: rule.text ?? Array.from({length: index + 1}, (_, at) => '%' + (at + 1)).join(heading?.separator ?? '.'),
        customSuffix: rule.suffix != null, suffix: rule.suffix ?? heading?.suffix ?? 'space', isLegal: rule.isLegal ?? false};
    });
    const captions = Object.fromEntries(KINDS.map(kind => {
      const rule = config?.captions?.[kind];
      return [kind, {scope: rule?.scope ?? (rule ? 'document' : ''), chapterLevel: String(rule?.chapterLevel ?? 1),
        format: rule?.format ?? 'decimal', start: String(rule?.start ?? 1), customPrefix: rule?.prefix != null,
        prefix: rule?.prefix ?? '', separator: rule?.separator ?? '.', labelSeparator: rule?.labelSeparator ?? ' ', captionSeparator: rule?.captionSeparator ?? ' — ', sequence: rule?.sequence ?? ''}];
    }));
    return {initial: clone(config), touched: false, changed: [], depth: String(heading?.levels.length ?? 0),
      separator: heading?.separator ?? '.', suffix: heading?.suffix ?? 'space', levels, captions};
  }
  function configFromDraft(draft) {
    if (!draft.touched) return clone(validateConfig(draft.initial));
    const depth = integer(draft.depth, 0, 6), config = {captions: {}};
    if (depth) config.headings = {levels: draft.levels.slice(0, depth).map((rule, index) => {
      const result = {format: format(rule.format), start: integer(rule.start, 0, 10000), restartAfter: integer(rule.restartAfter, 0, index)};
      if (rule.customText) result.text = rule.text;
      if (rule.customSuffix) result.suffix = rule.suffix;
      if (rule.isLegal || Object.hasOwn(draft.initial?.headings?.levels?.[index] ?? {}, 'isLegal')) result.isLegal = rule.isLegal;
      return result;
    }), separator: draft.separator, suffix: draft.suffix};
    for (const kind of KINDS) {
      const rule = draft.captions[kind]; if (!rule.scope) continue;
      config.captions[kind] = {scope: rule.scope, chapterLevel: integer(rule.chapterLevel, 1, 6), format: format(rule.format),
        start: integer(rule.start, 0, 10000), separator: rule.separator, labelSeparator: rule.labelSeparator, captionSeparator: rule.captionSeparator};
      if (rule.customPrefix) config.captions[kind].prefix = rule.prefix;
      if (rule.sequence !== '') config.captions[kind].sequence = rule.sequence;
    }
    if (!depth && !Object.keys(config.captions).length) return null;
    if (draft.initial && Array.isArray(draft.changed)) {
      // Editing one control does not turn every inherited/default field into
      // an explicit source choice or erase an imported null/override.
      const old = draft.initial, touched = new Set(draft.changed), result = clone(old);
      const copy = (target, candidate, key) => { if (Object.hasOwn(candidate, key)) target[key] = clone(candidate[key]); else delete target[key]; };
      if (!depth) { if (touched.has('depth')) result.headings = null; }
      else if (!old.headings) result.headings = config.headings;
      else {
        result.headings.levels = Array.from({length: depth}, (_, index) => {
          const previous = old.headings.levels[index], candidate = config.headings.levels[index];
          if (!previous) return candidate;
          const rule = clone(previous), prefix = 'levels.' + index + '.';
          for (const key of ['format', 'start', 'restartAfter', 'isLegal']) if (touched.has(prefix + key)) copy(rule, candidate, key);
          if (touched.has(prefix + 'text') || touched.has(prefix + 'customText')) copy(rule, candidate, 'text');
          if (touched.has(prefix + 'suffix') || touched.has(prefix + 'customSuffix')) copy(rule, candidate, 'suffix');
          return rule;
        });
        for (const key of ['separator', 'suffix']) if (touched.has(key)) result.headings[key] = config.headings[key];
      }
      for (const kind of KINDS) {
        const previous = old.captions?.[kind], candidate = config.captions[kind], prefix = 'captions.' + kind + '.';
        if (!candidate) { if (result.captions) delete result.captions[kind]; continue; }
        result.captions ??= {};
        if (!previous) { result.captions[kind] = candidate; continue; }
        const rule = clone(previous);
        for (const key of ['scope', 'chapterLevel', 'format', 'start', 'separator', 'labelSeparator', 'captionSeparator', 'sequence']) if (touched.has(prefix + key)) copy(rule, candidate, key);
        if (touched.has(prefix + 'prefix') || touched.has(prefix + 'customPrefix')) copy(rule, candidate, 'prefix');
        result.captions[kind] = rule;
      }
      return validateConfig(result);
    }
    return validateConfig(config);
  }
  function editorChanges(source, data, revision) {
    const transaction = data?.transaction, result = data?.result;
    if (!transaction || transaction.base_revision !== revision || !Array.isArray(transaction.edits) || transaction.edits.length > 1 || !result || typeof result.content !== 'string' || typeof result.revision !== 'string' || typeof result.changed !== 'boolean') throw error('transaction_conflict');
    const converter = window.ReadMDPublicationMetadata?.toEditorChanges;
    if (typeof converter !== 'function') throw error('transaction_invalid_range');
    // The shared converter rejects split CRLF/surrogates and invalid ranges.
    const changes = converter(source, transaction.edits);
    let content = '', at = 0;
    for (const edit of transaction.edits) { content += source.slice(at, edit.from) + edit.insert; at = edit.to; }
    content += source.slice(at);
    if (content !== result.content || result.changed !== (content !== source)) throw error('transaction_invalid_range');
    return changes;
  }
  function mount(host, options = {}) {
    for (const old of [...mounts]) if (!old.root.isConnected) old.destroy();
    const translate = (key, params) => (options.t || window.i18n.t.bind(window.i18n))(key, params);
    const nt = (key, params) => translate('publication.numbering.' + key, params);
    const root = document.createElement('details'); root.className = 'metadata-bibliography publication-numbering'; root.dataset.publicationNumbering = ''; root.style.gridColumn = '1 / -1';
    const labels = [], controls = new Map();
    const label = (node, key, params) => { labels.push({node, key, params}); node.textContent = translate(key, params); return node; };
    const numberLabel = (node, key, params) => label(node, 'publication.numbering.' + key, params);
    const summary = numberLabel(document.createElement('summary'), 'title'); root.append(summary);
    const body = document.createElement('div'); root.append(body); host.append(root);
    const abort = new AbortController();
    const session = {root, alive: true, loaded: false, busy: false, committing: false, draft: null, revision: '', source: options.source ?? options.getSource?.() ?? '', controls};
    const getSource = () => options.getSource ? options.getSource() : session.source;
    const current = () => session.alive && !options.signal?.aborted && options.isCurrent?.() !== false && getSource() === session.source;
    const message = document.createElement('p'); message.className = 'publication-style-error'; message.setAttribute('role', 'alert');
    let applyButton, clearButton, content;
    function showError(problem) {
      const code = problem?.data?.error_code || problem?.code || problem?.message;
      message.dataset.errorCode = code || 'publication_numbering_invalid';
      message.textContent = code === 'numbering_editor_required' ? nt('requiresEditor') : translate(['transaction_conflict', 'transaction_invalid_range'].includes(code) ? 'error.publication_local_style_target' : /^publication_numbering_(invalid|budget|hierarchy|chapter_missing)$/u.test(code || '') ? 'error.' + code : 'error.publication_numbering_invalid');
      root.open = true; if (!message.isConnected) body.append(message);
    }
    function sync() {
      const draft = session.draft; if (!draft) return;
      const depth = Number(draft.depth);
      for (const [path, input] of controls) {
        const pieces = path.split('.');
        input.disabled = session.busy || pieces[0] === 'levels' && Number(pieces[1]) >= depth || ['separator', 'suffix'].includes(path) && !depth;
        if (pieces[0] === 'levels' && (pieces[2] === 'text' && !draft.levels[pieces[1]].customText || pieces[2] === 'suffix' && !draft.levels[pieces[1]].customSuffix)) input.disabled = true;
        if (pieces[0] === 'captions') {
          const rule = draft.captions[pieces[1]], field = pieces[2];
          input.disabled = session.busy || field !== 'scope' && !rule.scope || field === 'chapterLevel' && rule.scope !== 'chapter' || field === 'prefix' && !rule.customPrefix;
        }
      }
      if (applyButton) applyButton.disabled = session.busy;
      if (clearButton) clearButton.disabled = session.busy || session.draft.initial == null;
    }
    const valueAt = path => path.split('.').reduce((value, key) => value[key], session.draft);
    function update(path, value) {
      const pieces = path.split('.'); let owner = session.draft;
      for (const key of pieces.slice(0, -1)) owner = owner[key];
      owner[pieces.at(-1)] = value; session.draft.touched = true; session.draft.changed.push(path); message.textContent = ''; sync();
    }
    function field(parent, path, key, choices = null, settings = {}) {
      const wrapper = document.createElement('label'), caption = settings.key ? label(document.createElement('span'), settings.key, settings.params) : numberLabel(document.createElement('span'), key, settings.params);
      const input = document.createElement(choices ? 'select' : 'input'); input.id = 'publication-numbering-' + (++serial); caption.id = input.id + '-label'; input.setAttribute('aria-labelledby', caption.id); input.dataset.numberingField = path;
      if (choices) {
        for (const choice of choices) {
          const option = document.createElement('option'); option.value = String(choice.value);
          if (choice.key) label(option, choice.key, choice.params); else option.textContent = choice.text;
          input.append(option);
        }
      } else {
        input.type = settings.checkbox ? 'checkbox' : settings.number ? 'number' : 'text';
        if (settings.number) { input.min = settings.min ?? '1'; input.max = settings.max ?? '10000'; input.step = '1'; }
        else if (!settings.checkbox) input.maxLength = settings.maxLength || 512;
      }
      if (settings.checkbox) input.checked = Boolean(valueAt(path)); else input.value = String(valueAt(path));
      input.addEventListener(choices || settings.checkbox ? 'change' : 'input', () => update(path, settings.checkbox ? input.checked : input.value));
      if (settings.checkbox) { wrapper.className = 'exp-check'; input.style.width = 'auto'; wrapper.style.flexDirection = 'row'; }
      wrapper.append(caption, input); parent.append(wrapper); controls.set(path, input); return input;
    }
    const formatChoices = FORMATS.map(([value, text]) => ({value, text}));
    const levelChoices = () => Array.from({length: 6}, (_, index) => ({value: index + 1, key: 'publication.numbering.level', params: {level: index + 1}}));
    function group(parent) { const grid = document.createElement('div'); grid.className = 'publication-style-fields'; parent.append(grid); return grid; }
    function render(config) {
      body.replaceChildren(); controls.clear(); labels.splice(1); session.draft = createDraft(config);
      const hint = numberLabel(document.createElement('p'), 'hint'); hint.className = 'exp-note'; body.append(hint);
      content = document.createElement('div'); body.append(content);
      const heading = group(content);
      field(heading, 'depth', 'levels', [{value: 0, key: 'app.disabled'}, ...Array.from({length: 6}, (_, index) => ({value: index + 1, text: String(index + 1)}))]);
      field(heading, 'separator', 'separator');
      field(heading, 'suffix', 'suffix', [{value: 'space', key: 'publication.numbering.space'}, {value: 'tab', key: 'publication.numbering.tab'}, {value: 'nothing', text: '∅'}]);
      for (let index = 0; index < 6; index++) {
        const section = document.createElement('details'); section.className = 'metadata-bibliography'; section.dataset.numberingLevel = String(index + 1);
        section.append(numberLabel(document.createElement('summary'), 'level', {level: index + 1})); const grid = group(section);
        field(grid, 'levels.' + index + '.format', 'format', formatChoices);
        field(grid, 'levels.' + index + '.start', 'start', null, {number: true, min: '0'});
        field(grid, 'levels.' + index + '.restartAfter', 'restartAfter', [{value: 0, key: 'publication.numbering.never'}, ...levelChoices().slice(0, index)]);
        field(grid, 'levels.' + index + '.customText', 'customLevelText', null, {checkbox: true});
        field(grid, 'levels.' + index + '.text', 'levelText', null, {maxLength: 512});
        field(grid, 'levels.' + index + '.customSuffix', 'customLevelSuffix', null, {checkbox: true});
        field(grid, 'levels.' + index + '.suffix', 'levelSuffix', [{value: 'space', key: 'publication.numbering.space'}, {value: 'tab', key: 'publication.numbering.tab'}, {value: 'nothing', text: '∅'}]);
        field(grid, 'levels.' + index + '.isLegal', 'legalNumbering', null, {checkbox: true});
        content.append(section);
      }
      for (const kind of KINDS) {
        const section = document.createElement('details'); section.className = 'metadata-bibliography'; section.dataset.numberingKind = kind;
        section.append(label(document.createElement('summary'), 'academic.' + kind)); const grid = group(section), path = 'captions.' + kind + '.';
        field(grid, path + 'scope', 'title', [{value: '', key: 'publication.numbering.legacy'}, {value: 'document', key: 'publication.numbering.document'}, {value: 'chapter', key: 'publication.numbering.chapter'}], {key: 'academic.' + kind});
        field(grid, path + 'chapterLevel', 'chapterLevel', levelChoices());
        field(grid, path + 'format', 'format', formatChoices); field(grid, path + 'start', 'start', null, {number: true, min: '0'});
        field(grid, path + 'customPrefix', 'customPrefix', null, {checkbox: true}); field(grid, path + 'prefix', 'prefix');
        field(grid, path + 'labelSeparator', 'labelSeparator'); field(grid, path + 'separator', 'separator'); field(grid, path + 'captionSeparator', 'captionSeparator');
        field(grid, path + 'sequence', 'sequence', null, {maxLength: 40});
        content.append(section);
      }
      const actions = document.createElement('div'); actions.className = 'publication-style-actions';
      applyButton = numberLabel(document.createElement('button'), 'apply'); applyButton.type = 'button'; applyButton.className = 'tb-btn accent'; applyButton.dataset.numberingAction = 'apply'; applyButton.addEventListener('click', () => commit(false));
      clearButton = numberLabel(document.createElement('button'), 'clear'); clearButton.type = 'button'; clearButton.className = 'tb-btn'; clearButton.dataset.numberingAction = 'clear'; clearButton.addEventListener('click', () => commit(true));
      actions.append(clearButton, applyButton); body.append(message, actions); message.textContent = ''; sync();
    }
    async function request(body) {
      const response = await apiFetch('/api/publication/numbering', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify(body), signal: abort.signal});
      const data = await response.json(); if (!response.ok || data.ok !== true) throw Object.assign(error(data.error_code || 'publication_numbering_invalid'), {data}); return data;
    }
    async function load() {
      if (session.loaded) return true; if (session.loading) return session.loading;
      if (!current()) { showError(error('transaction_conflict')); return false; }
      const loading = numberLabel(document.createElement('p'), 'loading'); loading.setAttribute('role', 'status'); body.replaceChildren(loading);
      session.loading = (async () => {
        try {
          const data = await request({action: 'get', content: session.source}); if (!current()) throw error('transaction_conflict');
          if (typeof data.revision !== 'string' || !Object.hasOwn(data, 'config')) throw error('publication_numbering_invalid');
          session.revision = data.revision; render(data.config); session.loaded = true; return true;
        } catch (problem) { if (session.alive && !options.signal?.aborted) showError(problem); return false; }
        finally { session.loading = null; }
      })(); return session.loading;
    }
    async function commit(clear = false) {
      if (session.committing || !session.alive) return false;
      session.committing = true;
      try {
        if (!await load()) return false;
        const config = clear ? null : configFromDraft(session.draft);
        if (!current()) throw error('transaction_conflict');
        session.busy = true; sync();
        const binding = options.ensureEditor ? await options.ensureEditor() : options.view ? {view: options.view} : null;
        if (!binding?.view || binding.source != null && binding.source !== session.source) throw error('numbering_editor_required');
        const view = binding.view, cmSource = session.source.replace(/\r\n?/g, '\n');
        const editorCurrent = () => current() && binding.current?.() !== false && view.state.doc.toString() === cmSource;
        if (!editorCurrent()) throw error('transaction_conflict');
        const data = await request({action: 'apply', content: session.source, base_revision: session.revision, config});
        if (!editorCurrent()) throw error('transaction_conflict');
        const changes = editorChanges(session.source, data, session.revision);
        if (data.result.changed) {
          const CM = window.ReadMDCodeMirror;
          view.dispatch({changes, ...(CM?.Transaction ? {annotations: CM.Transaction.userEvent.of('input.publication.numbering')} : {})});
        }
        session.source = data.result.content; session.revision = data.result.revision;
        if (Object.hasOwn(data, 'config')) render(data.config); else render(config);
        options.onApplied?.(data); return true;
      } catch (problem) { if (session.alive && !options.signal?.aborted) showError(problem); return false; }
      finally { session.committing = false; session.busy = false; sync(); }
    }
    const controller = {root, controls, load, apply: () => commit(false), clear: () => commit(true),
      read: () => configFromDraft(session.draft), destroy() { if (!session.alive) return; session.alive = false; abort.abort(); mounts.delete(controller); },
      refreshLabels() { for (const item of labels) item.node.textContent = translate(item.key, item.params); if (message.textContent) showError(error(message.dataset.errorCode)); }};
    root.addEventListener('toggle', () => { if (root.open) load(); });
    options.signal?.addEventListener('abort', () => controller.destroy(), {once: true}); mounts.add(controller); return controller;
  }
  window.addEventListener('readmd:language-changed', () => { for (const controller of [...mounts]) { if (controller.root.isConnected) controller.refreshLabels(); else controller.destroy(); } });
  window.ReadMDPublicationNumbering = Object.freeze({mount, createDraft, configFromDraft, validateConfig, editorChanges, keys: KEYS});
})();
