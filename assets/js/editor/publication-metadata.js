'use strict';
/* Publication properties share the revision checked source service. This
   component never saves files, writes source while opening, or parses YAML. */
(() => {
  const fields = ['title', 'subtitle', 'authors', 'affiliations', 'translators', 'abstracts', 'keywords', 'funding', 'publisher', 'date', 'rights', 'edition', 'series', 'identifiers', 'language', 'subject'];
  const listFields = new Set(['authors', 'affiliations', 'translators', 'abstracts', 'keywords', 'funding', 'identifiers']);
  const aliases = {author: 'authors', institution: 'affiliations', institutions: 'affiliations', affiliation: 'affiliations', translator: 'translators', abstract: 'abstracts', isbn: 'identifiers', identifier: 'identifiers', lang: 'language'};
  const limits = {authors: 32, affiliations: 32, translators: 32, abstracts: 8, keywords: 64, funding: 16, identifiers: 16};
  const personKeys = ['name', 'id', 'given', 'family', 'fileAs', 'orcid', 'corresponding', 'affiliationIds'];
  const rows = {authors: personKeys, translators: personKeys, affiliations: ['name', 'id', 'department', 'address'], abstracts: ['language', 'text'], keywords: ['value'], funding: ['funder', 'award', 'recipientIds'], identifiers: ['scheme', 'value', 'primary']};
  const optionalKeys = new Set(['given', 'family', 'fileAs', 'orcid', 'department', 'address', 'language', 'award', 'number']);
  const english = {metadata: 'Publication metadata', sourceHint: 'Edit document properties. Unchanged fields, comments and body text are retained.', exportHint: 'Overrides apply to this export or preset. Inherited values remain in the source document.', title: 'Title', subtitle: 'Subtitle', authors: 'Authors', affiliations: 'Institutions', translators: 'Translators', abstracts: 'Abstracts', keywords: 'Keywords', funding: 'Funding', publisher: 'Publisher', date: 'Publication date', rights: 'Rights', edition: 'Edition', series: 'Series', identifiers: 'Book identifiers', language: 'Content language', subject: 'Subject', inherit: 'Inherit', value: 'Override', clear: 'Clear', add: 'Add', remove: 'Remove', up: 'Move up', down: 'Move down', id: 'Stable ID', name: 'Full name', given: 'Given name', family: 'Family name', fileAs: 'Sort name', orcid: 'ORCID', corresponding: 'Corresponding author', affiliationIds: 'Institutions', department: 'Department', address: 'Address', text: 'Text', funder: 'Funder', award: 'Award or grant number', recipientIds: 'Recipients', scheme: 'Scheme', identifierValue: 'Identifier', primary: 'Primary identifier', number: 'Series number', details: 'Details', frontMatter: 'Front matter', none: 'None', paper: 'Paper', book: 'Book', labels: 'Content labels', abstractLabel: 'Abstract', keywordsLabel: 'Keywords', fundingLabel: 'Funding', translatorLabel: 'Translator', dateLabel: 'Date', emptyInstitutions: 'Add an institution to make it available here.', emptyAuthors: 'Add an author to select funding recipients.', loading: 'Loading document properties…', invalid: 'Check the publication metadata.', conflict: 'The document changed. Reopen its properties before applying changes.'};
  const clone = value => value === undefined ? undefined : JSON.parse(JSON.stringify(value));
  const equal = (a, b) => JSON.stringify(a) === JSON.stringify(b);
  const t = key => {const value = window.i18n?.t('pm.' + key); return value && value !== 'pm.' + key ? value : english[key] || key;};
  const el = (tag, className, text) => {const node = document.createElement(tag); if (className) node.className = className; if (text !== undefined) node.textContent = text; return node;};
  const empty = key => listFields.has(key) ? [] : null;
  let sequence = 0, epoch = 0, editorSession = null, exportForm = null;

  function stableRowId(role, name = '') {
    if (window.crypto?.randomUUID) return role + '-' + window.crypto.randomUUID();
    sequence++; return role + '-' + Date.now().toString(36) + '-' + sequence.toString(36) + '-' + name.length;
  }
  function normalized(value, key) {
    if (key === 'series' && typeof value === 'string') return {title: value, number: null};
    if (key === 'abstracts' && value && !Array.isArray(value) && typeof value === 'object' && !Object.hasOwn(value, 'text')) return Object.entries(value).map(([language, text]) => ({language, text}));
    if (!listFields.has(key)) return value ?? null;
    if (value == null) return [];
    const values = Array.isArray(value) ? value : [value];
    return values.map(item => {
      if (key === 'keywords') return typeof item === 'string' ? item : '';
      if (typeof item === 'object' && item) return clone(item);
      if (key === 'authors' || key === 'translators') return {id: stableRowId('person'), name: String(item), affiliationIds: []};
      if (key === 'affiliations') return {id: stableRowId('institution'), name: String(item)};
      if (key === 'abstracts') return {text: String(item), language: null};
      if (key === 'funding') return {funder: String(item), award: null, recipientIds: []};
      return {scheme: 'uri', value: String(item), primary: false};
    });
  }
  function layer(meta) {
    const values = {}, clears = new Set(Array.isArray(meta?.clear) ? meta.clear : []);
    for (const [raw, value] of Object.entries(meta || {})) {
      const key = aliases[raw] || raw;
      if (!fields.includes(key) || value == null || ['title', 'author', 'subject', 'language', 'lang'].includes(raw) && value === '') continue;
      if (raw === 'isbn') values.identifiers = [{scheme: 'isbn', value, primary: false}];
      else values[key] = clone(value);
    }
    return {values, clears};
  }
  function newRow(field) {
    if (field === 'keywords') return '';
    if (field === 'authors' || field === 'translators') return {id: stableRowId('person'), name: '', given: null, family: null, fileAs: null, orcid: null, corresponding: false, affiliationIds: []};
    if (field === 'affiliations') return {id: stableRowId('institution'), name: '', department: null, address: null};
    if (field === 'abstracts') return {language: null, text: ''};
    if (field === 'funding') return {funder: '', award: null, recipientIds: []};
    return {scheme: 'isbn', value: '', primary: false};
  }
  function fieldLabel(field, key) {return t(key === 'value' ? field === 'keywords' ? 'text' : 'identifierValue' : key);}
  function input(label, value, options = {}) {
    const group = el('div', 'modal-form-group');
    const id = 'pm-input-' + (++sequence);
    const title = el('label', '', label); title.htmlFor = id;
    const control = el(options.multiline ? 'textarea' : 'input', 'modal-input'); control.id = id;
    if (control.tagName === 'INPUT') control.type = options.checkbox ? 'checkbox' : 'text';
    if (options.checkbox) control.checked = Boolean(value); else control.value = value ?? '';
    if (options.multiline) control.rows = 3;
    if (!options.checkbox) control.maxLength = options.large ? 16384 : 4096;
    if (options.key) control.dataset.pmKey = options.key;
    group.append(title, control); return {group, control};
  }
  function action(name, fn) {
    const button = el('button', 'tb-btn', t(name)); button.type = 'button'; button.dataset.pmAction = name;
    button.addEventListener('click', fn); return button;
  }
  function referenceOptions(form, key) {
    const target = key === 'affiliationIds' ? 'affiliations' : 'authors';
    const value = form.kind === 'export' && form.modes[target] === 'clear' ? [] : form.values[target];
    return Array.isArray(value) ? value.filter(row => row.id).map(row => ({id: row.id, name: row.name || row.id})) : [];
  }
  function references(form, container, field, index, key) {
    const options = referenceOptions(form, key), selected = form.values[field][index]?.[key] || [];
    container.textContent = ''; container.dataset.pmKey = key;
    const title = el('div', '', t(key)); container.append(title);
    if (!options.length) container.append(el('p', 'exp-note', t(key === 'affiliationIds' ? 'emptyInstitutions' : 'emptyAuthors')));
    for (const id of selected) if (!options.some(option => option.id === id)) options.push({id, name: id});
    for (const option of options) {
      const label = el('label', 'exp-check'), control = el('input'); control.type = 'checkbox'; control.value = option.id;
      control.checked = selected.includes(option.id); control.dataset.pmRef = option.id;
      control.disabled = form.kind === 'export' && form.modes[field] !== 'value';
      control.addEventListener('change', () => {form.values[field][index][key] = [...container.querySelectorAll('input:checked')].map(item => item.value); notify(form, field);});
      label.append(control, document.createTextNode(option.name)); container.append(label);
    }
  }
  function refreshReferences(form) {
    for (const field of ['authors', 'translators', 'funding']) {
      const group = form.groups[field]; if (!group) continue;
      group.querySelectorAll('[data-pm-row]').forEach((row, index) => row.querySelectorAll('[data-pm-key="affiliationIds"], [data-pm-key="recipientIds"]').forEach(container => references(form, container, field, index, container.dataset.pmKey)));
    }
  }
  function notify(form, field) {
    form.touched.add(field);
    if (field === 'language' && form.kind === 'source') {const old = document.getElementById('fm-input-lang'); if (old) old.value = form.values.language || '';}
    form.onChange?.();
  }
  function list(form, field, host) {
    host.textContent = '';
    form.values[field].forEach((value, index) => {
      const row = el('fieldset', 'modal-form-group'); row.dataset.pmRow = String(index); row.style.minWidth = '0';
      const legend = el('legend', '', t(field) + ' ' + (index + 1)); row.append(legend);
      const grid = el('div', 'panel-form-grid'); grid.style.gridTemplateColumns = 'repeat(auto-fit, minmax(min(100%, 190px), 1fr))';
      const details = el('details', 'metadata-bibliography'), summary = el('summary', '', t('details')), more = el('div', 'panel-form-grid');
      more.style.gridTemplateColumns = 'repeat(auto-fit, minmax(min(100%, 190px), 1fr))'; details.append(summary, more);
      for (const key of rows[field]) {
        if (key === 'affiliationIds' || key === 'recipientIds') {const choices = el('div', 'modal-form-group'); choices.dataset.pmKey = key; (key === 'recipientIds' ? grid : more).append(choices); references(form, choices, field, index, key); continue;}
        const item = field === 'keywords' ? value : value[key];
        const {group, control} = input(fieldLabel(field, key), item, {key, checkbox: key === 'corresponding' || key === 'primary', multiline: key === 'text' || key === 'address', large: key === 'text'});
        if (key === 'id') control.maxLength = 128;
        control.addEventListener(control.type === 'checkbox' ? 'change' : 'input', () => {
          const next = control.type === 'checkbox' ? control.checked : optionalKeys.has(key) && control.value === '' ? null : control.value;
          if (field === 'keywords') form.values[field][index] = next; else form.values[field][index][key] = next;
          notify(form, field);
        });
        if (key === 'name' || key === 'id') control.addEventListener('change', () => refreshReferences(form));
        (['given', 'family', 'fileAs', 'orcid', 'corresponding', 'id'].includes(key) ? more : grid).append(group);
      }
      row.append(grid); if (more.children.length) row.append(details);
      const controls = el('div', 'modal-footer'); controls.style.flexWrap = 'wrap';
      const remove = action('remove', () => {form.values[field].splice(index, 1); list(form, field, host); refreshReferences(form); notify(form, field);});
      const up = action('up', () => {const values = form.values[field]; [values[index - 1], values[index]] = [values[index], values[index - 1]]; list(form, field, host); refreshReferences(form); notify(form, field);});
      const down = action('down', () => {const values = form.values[field]; [values[index + 1], values[index]] = [values[index], values[index + 1]]; list(form, field, host); refreshReferences(form); notify(form, field);});
      up.disabled = index === 0; down.disabled = index === form.values[field].length - 1;
      controls.append(up, down, remove); row.append(controls); host.append(row);
    });
    const add = action('add', () => {form.values[field].push(newRow(field)); list(form, field, host); refreshReferences(form); notify(form, field);});
    add.disabled = form.values[field].length >= limits[field]; host.append(add);
    disable(form, field);
  }
  function disable(form, field) {
    if (form.kind !== 'export') return;
    const enabled = form.modes[field] === 'value';
    form.groups[field]?.querySelectorAll('[data-pm-values] input, [data-pm-values] textarea, [data-pm-values] button, [data-pm-values] select').forEach(control => {control.disabled = !enabled || control.dataset.pmAction === 'up' && control.closest('[data-pm-row]')?.dataset.pmRow === '0' || control.dataset.pmAction === 'down' && Number(control.closest('[data-pm-row]')?.dataset.pmRow) === form.values[field].length - 1 || control.dataset.pmAction === 'add' && form.values[field].length >= limits[field];});
  }
  function renderValue(form, field) {
    const host = form.groups[field].querySelector('[data-pm-values]'); host.textContent = '';
    if (listFields.has(field)) {list(form, field, host); return;}
    if (field === 'series') {
      const grid = el('div', 'panel-form-grid');
      for (const key of ['title', 'number']) {
        const {group, control} = input(t(key), form.values.series?.[key], {key});
        control.addEventListener('input', () => {const next = {...(form.values.series || {title: '', number: null}), [key]: control.value || null}; form.values.series = !next.title && !next.number ? null : next; notify(form, field);}); grid.append(group);
      }host.append(grid);
    } else if (field === 'title' && form.kind === 'source' && document.getElementById('fm-input-title')) {
      const control = document.getElementById('fm-input-title'), group = control.closest('.modal-form-group');
      control.dataset.pmKey = 'value'; control.value = form.values.title || ''; control.maxLength = 4096;
      control.oninput = () => {form.values.title = control.value || null; notify(form, field);}; host.append(group);
    } else {
      const {group, control} = input(t(field), form.values[field], {key: 'value', multiline: field === 'rights'});
      control.addEventListener('input', () => {form.values[field] = control.value || null; notify(form, field);}); host.append(group);
    }disable(form, field);
  }
  function createForm(host, kind, inherited, meta, onChange) {
    const over = layer(meta), form = {host, kind, inherited: clone(inherited || {}), values: {}, modes: {}, initial: {}, raw: over.values, touched: new Set(), groups: {}, onChange};
    host.textContent = ''; host.append(el('p', 'exp-note', t(kind === 'source' ? 'sourceHint' : 'exportHint')));
    for (const field of fields) {
      const mode = kind === 'export' ? over.clears.has(field) ? 'clear' : Object.hasOwn(over.values, field) ? 'value' : 'inherit' : 'value'; form.modes[field] = mode;
      form.values[field] = normalized(mode === 'value' && kind === 'export' ? over.values[field] : inherited?.[field] ?? empty(field), field);
      if (mode === 'clear') form.values[field] = empty(field);
      form.initial[field] = clone(form.values[field]);
      const plain = ['title', 'subtitle', 'language'].includes(field), group = el(plain ? 'div' : 'details', plain ? 'modal-form-group' : 'metadata-bibliography');
      group.dataset.pmField = field; if (!plain) {group.append(el('summary', '', t(field))); if (field === 'authors') group.open = true;}
      if (kind === 'export') {
        const control = el('select', 'modal-select'); control.dataset.pmState = field; control.setAttribute('aria-label', t(field));
        for (const name of ['inherit', 'value', 'clear']) control.add(new Option(t(name), name)); control.value = mode;
        control.addEventListener('change', () => {form.modes[field] = control.value; form.values[field] = control.value === 'clear' ? empty(field) : control.value === 'inherit' ? normalized(form.inherited[field] ?? empty(field), field) : clone(form.values[field] ?? empty(field)); if (control.value === 'value' && listFields.has(field) && !Array.isArray(form.values[field])) form.values[field] = []; renderValue(form, field); refreshReferences(form); notify(form, field);});
        group.append(control);
      }
      const value = el('div'); value.dataset.pmValues = field; group.append(value); form.groups[field] = group; host.append(group); renderValue(form, field);
    }
    refreshReferences(form); return form;
  }
  async function request(body) {
    const response = await apiFetch('/api/publication/metadata', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify(body)});
    const data = await response.json(); if (!response.ok || data.ok === false) {const error = Error(data.error_code || data.error || 'publication_metadata_invalid'); error.data = data; throw error;} return data;
  }
  function showError(host, error) {
    let box = host.querySelector('[data-pm-error]'); if (!box) {box = el('p', 'exp-note'); box.dataset.pmError = ''; box.setAttribute('role', 'alert'); host.prepend(box);}
    const path = error?.data?.path || error?.data?.field || '', field = String(path).replace(/^meta\./, '').split(/[.[]/)[0];
    box.dataset.errorCode = error?.data?.error_code || error?.message || ''; box.textContent = t(box.dataset.errorCode === 'transaction_conflict' ? 'conflict' : 'invalid') + (fields.includes(field) ? ' ' + t(field) + '.' : '');
    const group = host.querySelector('[data-pm-field="' + (fields.includes(field) ? field : 'title') + '"]'); if (group?.tagName === 'DETAILS') group.open = true;
    group?.querySelector('input, textarea, select')?.focus();
  }
  function selectValue(id, value) {
    const select = document.getElementById(id); if (!select) return;
    if (![...select.options].some(option => option.value === value)) select.add(new Option(value, value)); select.value = value;
  }
  async function open() {
    if (!state.editing) return false;
    closeMdPopups(); const modal = document.getElementById('frontmatter-modal'), scroll = modal?.querySelector('.panel-scroll-body'); if (!scroll) return false;
    const token = ++epoch, content = getEditContent(), cmText = cmView?.state.doc.toString();
    let host = document.getElementById('publication-metadata-editor');
    if (!host) {host = el('div'); host.id = 'publication-metadata-editor'; scroll.prepend(host);}
    // A previous session may have moved the stable title input into this host.
    const titleGroup = document.getElementById('fm-input-title')?.closest('.modal-form-group'); if (titleGroup && host.contains(titleGroup)) scroll.insertBefore(titleGroup, host.nextSibling);
    host.textContent = ''; host.append(el('p', 'exp-note', t('loading')));
    document.getElementById('fm-input-author')?.closest('.modal-form-group')?.setAttribute('hidden', '');
    const insert = document.getElementById('fm-modal-insert'); if (insert) insert.disabled = true;
    editorSession = {token, content, cmText, host, loading: true}; modal.classList.remove('hidden');
    try {
      const data = await request({action: 'get', content, source_name: state.sourceName || state.file || ''});
      if (token !== epoch || getEditContent() !== content) return false;
      const form = createForm(host, 'source', data.metadata, null); const legacy = data.legacyValues || {};
      const bibliography = Array.isArray(legacy.bibliography) ? JSON.stringify(legacy.bibliography) : legacy.bibliography ?? '';
      const legacyInitial = {bibliography, csl: legacy.csl ?? 'apa', lang: data.metadata.language ?? '', theme: legacy.presentation?.theme ?? 'black', transition: legacy.presentation?.transition ?? 'slide'};
      for (const key of ['bibliography', 'csl', 'lang']) {const input = document.getElementById('fm-input-' + key); if (input) input.value = legacyInitial[key];}
      for (const key of ['theme', 'transition']) selectValue('fm-select-' + key, String(legacyInitial[key]));
      const lang = document.getElementById('fm-input-lang'); if (lang) lang.oninput = () => {form.values.language = lang.value || null; form.groups.language.querySelector('input').value = lang.value; notify(form, 'language');};
      editorSession = {token, content, cmText, host, revision: data.revision, form, legacyInitial, loading: false};
      if (insert) insert.disabled = false; document.getElementById('fm-input-title')?.focus(); return true;
    } catch (error) {if (token === epoch) {editorSession.loading = false; showError(host, error);} return false;}
  }
  function cancel() {epoch++; editorSession = null;}
  function toEditorChanges(source, edits) {
    const changes = [], pairs = source.matchAll(/\r\n/g); let next = pairs.next(), removed = 0;
    const position = at => {
      if (!Number.isInteger(at) || at < 0 || at > source.length || at > 0 && source[at - 1] === '\r' && source[at] === '\n') throw Error('transaction_invalid_range');
      if (at > 0 && /[\uD800-\uDBFF]/.test(source[at - 1]) && /[\uDC00-\uDFFF]/.test(source[at] || '')) throw Error('transaction_invalid_range');
      while (!next.done && next.value.index + 2 <= at) {removed++; next = pairs.next();} return at - removed;
    };
    let previous = -1;
    for (const edit of edits) {if (edit.from < previous || edit.to < edit.from || typeof edit.insert !== 'string') throw Error('transaction_invalid_range'); changes.push({from: position(edit.from), to: position(edit.to), insert: edit.insert.replace(/\r\n?/g, '\n')}); previous = edit.to;}
    return changes;
  }
  async function submit() {
    const session = editorSession; if (!session?.form || session.loading) return false;
    if (getEditContent() !== session.content || cmView?.state.doc.toString() !== session.cmText) {showError(session.host, {message: 'transaction_conflict'}); return false;}
    const patch = {};
    for (const field of fields) if (!equal(session.form.values[field], session.form.initial[field])) patch[field] = clone(session.form.values[field]);
    for (const key of ['bibliography', 'csl']) {
      const value = document.getElementById('fm-input-' + key)?.value ?? '';
      if (value !== session.legacyInitial[key]) {if (key === 'bibliography' && value.trimStart().startsWith('[')) {try {const parsed = JSON.parse(value); if (!Array.isArray(parsed) || parsed.some(item => typeof item !== 'string')) throw Error('bibliography'); patch[key] = parsed;} catch (error) {showError(session.host, error); return false;}} else patch[key] = value;}
    }
    const presentation = {}; for (const key of ['theme', 'transition']) {const value = document.getElementById('fm-select-' + key)?.value; if (value !== undefined && value !== session.legacyInitial[key]) presentation[key] = value;}
    if (Object.keys(presentation).length) patch.presentation = presentation;
    const insert = document.getElementById('fm-modal-insert'); if (insert) insert.disabled = true;
    try {
      const data = await request({action: 'patch', content: session.content, source_name: state.sourceName || state.file || '', base_revision: session.revision, patch});
      if (session.token !== epoch) return false;
      if (getEditContent() !== session.content || cmView?.state.doc.toString() !== session.cmText) throw Error('transaction_conflict');
      if (data.changed) {
        if (!cmView || session.cmText !== session.content.replace(/\r\n?/g, '\n')) throw Error('transaction_conflict');
        let actual = '', cursor = 0;
        for (const edit of data.transaction.edits) {actual += session.content.slice(cursor, edit.from) + edit.insert; cursor = edit.to;} actual += session.content.slice(cursor);
        if (actual !== data.content) throw Error('transaction_invalid_range');
        const changes = toEditorChanges(session.content, data.transaction.edits), CM = window.ReadMDCodeMirror;
        cmView.dispatch({changes, ...(CM?.Transaction ? {annotations: CM.Transaction.userEvent.of('input.publication')} : {})});
      }
      closeFrontmatterModal(); return true;
    } catch (error) {if (session.token === epoch) showError(session.host, error); return false;}
    finally {if (insert) insert.disabled = false;}
  }
  function frontMatterControls(host, options, form) {
    const group = el('details', 'metadata-bibliography'); group.dataset.pmFrontMatter = ''; group.append(el('summary', '', t('frontMatter')));
    const select = el('select', 'modal-select'); select.dataset.pmMode = ''; select.setAttribute('aria-label', t('frontMatter'));
    for (const name of ['none', 'paper', 'book']) select.add(new Option(t(name), name)); select.value = options.frontMatter?.mode || 'none'; form.frontMode = select.value; form.initialFrontMode = select.value;
    select.addEventListener('change', () => {form.frontMode = select.value; form.onChange?.();}); group.append(select);
    const labels = el('details', 'metadata-bibliography'); labels.append(el('summary', '', t('labels'))); form.labels = clone(options.frontMatter?.labels || {}); form.initialLabels = clone(form.labels);
    for (const key of ['abstract', 'keywords', 'funding', 'corresponding', 'translator', 'publisher', 'date', 'edition', 'rights', 'series']) {
      const labelKey = {abstract: 'abstractLabel', keywords: 'keywordsLabel', funding: 'fundingLabel', translator: 'translatorLabel', date: 'dateLabel'}[key] || key;
      const {group, control} = input(t(labelKey), form.labels[key]); control.dataset.pmLabel = key; control.placeholder = t('inherit');
      control.addEventListener('input', () => {if (control.value) form.labels[key] = control.value; else delete form.labels[key]; form.onChange?.();}); labels.append(group);
    }
    group.append(labels); host.append(group);
  }
  function mountExport(host, options, onChange) {
    host.id = 'publication-metadata-export'; const content = currentExportContent();
    const form = createForm(host, 'export', {}, options.meta, onChange); exportForm = form; frontMatterControls(host, options, form);
    // Resolve inheritance for display and reference choices, then leave it in
    // the source; collectExport never serializes these display-only defaults.
    request({action: 'get', content, source_name: state.sourceName || state.file || ''}).then(data => {
      if (exportForm !== form || !host.isConnected || currentExportContent() !== content) return;
      form.inherited = clone(data.metadata); for (const field of fields) if (form.modes[field] === 'inherit') {form.values[field] = normalized(data.metadata[field] ?? empty(field), field); renderValue(form, field);} refreshReferences(form);
    }).catch(error => {if (exportForm === form && host.isConnected) showError(host, error);});
    return form;
  }
  function collectExport(options) {
    const form = exportForm; if (!form?.host?.isConnected) return options;
    const meta = clone(options.meta || {}); for (const key of Object.keys(meta)) if (fields.includes(aliases[key] || key) || key === 'clear') delete meta[key];
    const clear = [];
    for (const field of fields) {
      if (form.modes[field] === 'inherit') continue;
      const value = form.values[field];
      if (form.modes[field] === 'clear' || !listFields.has(field) && (value == null || value === '')) {if (listFields.has(field)) meta[field] = []; else clear.push(field);}
      else meta[field] = clone(!form.touched.has(field) && Object.hasOwn(form.raw, field) ? form.raw[field] : value);
    }
    if (clear.length) meta.clear = clear;
    options.meta = meta;
    if (options.frontMatter || form.frontMode !== 'none' || !equal(form.labels, {})) {
      const front = clone(options.frontMatter || {}); if (Object.hasOwn(front, 'mode') || form.frontMode !== 'none' || form.frontMode !== form.initialFrontMode) front.mode = form.frontMode;
      if (Object.hasOwn(front, 'labels') || Object.keys(form.labels).length || !equal(form.labels, form.initialLabels)) front.labels = clone(form.labels); options.frontMatter = front;
    }
    return options;
  }
  window.ReadMDPublicationMetadata = Object.freeze({open, cancel, submit, mountExport, collectExport, toEditorChanges, schemaFields: Object.freeze(fields.slice())});
})();
