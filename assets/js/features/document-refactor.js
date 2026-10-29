'use strict';
/* One confirmed plan owns the rename, reference edits, and local adoption.
   Preview/receipts are data, never authority to open an enclosing directory. */
window.ReadMDRefactor = (() => {
  const MAX_FILES = 512, MAX_EDITS = 8192, MAX_TEXT = 16 * 1024 * 1024, MAX_DIFF = 2 * 1024 * 1024;
  const hash = value => typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);
  const lf = value => window.ReadMDReview?.lf(value) ?? String(value ?? '').replace(/^\uFEFF/, '').replace(/\r\n?/g, '\n');
  const key = value => typeof normalizePath === 'function' ? normalizePath(value) : String(value || '').replace(/\\/g, '/');
  const tabs = () => typeof state !== 'undefined' && Array.isArray(state.tabs) ? state.tabs : [];
  const t = (name, params) => window.i18n?.t(name, params) || name;
  const waiters = new Set();
  let current = null;
  function failure(code) { const error = new Error(code); error.code = code; return error; }
  function notify(code = 'conflict') {
    const unsupported = /unsupported|read_?only|metadata|permission|access_denied/.test(code);
    const specificKey = 'error.' + code, specific = t(specificKey);
    const name = unsupported ? 'rename.safeUnsupported' : /recovery|unknown/.test(code) ? 'error.review_needs_recovery' : /persist/.test(code) ? 'review.persistenceFailed'
      : /busy/.test(code) ? 'review.busy' : /target/.test(code) ? 'storage.targetOpen' : /limit|budget/.test(code) ? 'review.limit' : 'review.conflict';
    if (typeof showToast === 'function') showToast(!unsupported && specific !== specificKey ? specific : t(name));
  }
  async function request(ctx, action, extra = {}) {
    const body = { action, operation_id: ctx.operation_id, ...(ctx.plan_hash ? { plan_hash: ctx.plan_hash } : {}), ...extra };
    let response, data;
    try {
      response = await apiFetch('/api/rename/refactor', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
      data = await response.json();
    } catch (_) { throw failure('unknown'); }
    const rejectedCode = typeof data?.error_code === 'string' && /^[a-z][a-z0-9_]{0,95}$/.test(data.error_code) ? data.error_code : null;
    // Standard ApiError rejections need not contain an operation receipt.
    // Preview cannot mutate files. Commit still reconciles its exact durable
    // outcome before a rejected code may be reported as a settled failure.
    if (action === 'preview' && (!response.ok || data?.ok === false)) throw failure(rejectedCode || 'conflict');
    if (action === 'commit' && !response.ok && rejectedCode) ctx.failureCode = rejectedCode;
    // Non-2xx terminal receipts still carry the durable operation outcome.
    if (!data || data.operation_id !== ctx.operation_id || (ctx.plan_hash && action !== 'preview' && data.plan_hash !== ctx.plan_hash)) throw failure('unknown');
    if (action === 'preview' && (!response.ok || data.ok !== true)) throw failure(data.error_code || 'conflict');
    return data;
  }
  function applyEdits(before, edits) {
    let position = 0, after = '';
    for (const edit of edits) {
      if (!Number.isSafeInteger(edit.from) || !Number.isSafeInteger(edit.to) || edit.from < position || edit.to < edit.from || edit.to > before.length ||
          typeof edit.insert !== 'string' || lf(edit.insert) !== edit.insert) throw failure('conflict');
      after += before.slice(position, edit.from) + edit.insert; position = edit.to;
    }
    return after + before.slice(position);
  }
  function planData(ctx, value) {
    if (value?.ok !== true || value.operation_id !== ctx.operation_id || !hash(value.plan_hash) || key(value.source_path) !== key(ctx.old_path) ||
        typeof value.target_path !== 'string' || !value.target_path || value.target_path.length > 32768 || !Array.isArray(value.files) || !value.files.length || value.files.length > MAX_FILES ||
        (value.diagnostics !== undefined && (!Array.isArray(value.diagnostics) || value.diagnostics.length))) throw failure('conflict');
    let bytes = 0, editCount = 0, diffBytes = 0;
    const sources = new Set(), destinations = new Set(), encoder = new TextEncoder();
    for (const file of value.files) {
      if (!file || typeof file.source_path !== 'string' || !file.source_path || file.source_path.length > 32768 ||
          typeof file.target_path !== 'string' || !file.target_path || file.target_path.length > 32768 ||
          typeof file.before !== 'string' || typeof file.after !== 'string' || !hash(file.before_disk_revision) || !hash(file.after_disk_revision) || !Array.isArray(file.edits) ||
          sources.has(key(file.source_path)) || destinations.has(key(file.target_path))) throw failure('conflict');
      sources.add(key(file.source_path)); destinations.add(key(file.target_path));
      bytes += encoder.encode(file.before).length + encoder.encode(file.after).length;
      editCount += file.edits.length;
      if (bytes > MAX_TEXT || editCount > MAX_EDITS || applyEdits(lf(file.before), file.edits) !== lf(file.after)) throw failure('limit');
      for (const edit of file.edits) diffBytes += encoder.encode(lf(file.before).slice(edit.from, edit.to)).length + encoder.encode(edit.insert).length;
      if (diffBytes > MAX_DIFF) throw failure('limit');
    }
    const source = value.files.find(file => key(file.source_path) === key(ctx.old_path));
    if (!source || key(source.target_path) !== key(value.target_path)) throw failure('conflict');
    return value;
  }
  const roster = plan => plan.files.map(file => [key(file.source_path), key(file.target_path)]).sort((a, b) => a[0].localeCompare(b[0]));
  function openOwners(files, source) {
    const owners = [];
    for (const tab of tabs()) {
      const path = tab.path || (tab === source ? tab._renamePending?.old_path : '');
      if (!path) continue;
      const file = files.find(file => key(file.source_path || file.old_path) === key(path));
      const destination = files.find(file => key(file.target_path || file.path) === key(path));
      if (!file && destination) {
        // A previously adopted owner is legitimate on an idempotent retry.
        if (tab === source && source?._renamePending) owners.push({ tab, file: destination });
        else throw failure('target');
      } else if (file) owners.push({ tab, file });
    }
    if (source && !owners.some(owner => owner.tab === source)) {
      const file = files.find(file => key(file.source_path || file.old_path) === key(source._renamePending?.old_path));
      if (!file) throw failure('conflict');
      owners.push({ tab: source, file });
    }
    return owners;
  }
  function clean(tab, ctx) {
    const snapshot = window.ReadMDReview?.snapshot(tab);
    const recoverable = tab === ctx.source && tab._renamePending?.operation_id === ctx.operation_id && tab.isVirtual && tab._recoveryContent === undefined;
    return tabs().includes(tab) && (tab.mode === 'file' || recoverable) && !tab.isDirty && !tab.unsavedCreation && !tab.externalChanged && !tab._reviewRestorePending &&
      (!tab._renamePending || tab === ctx.source && tab._renamePending.operation_id === ctx.operation_id) &&
      !(typeof state !== 'undefined' && state.editing && state.activeTabId === tab.id) && !snapshot?.recovery &&
      !snapshot?.changes?.some(change => change.decision === 'conflict' || change.taken_over);
  }
  function freeze(ctx, tab) {
    if (!ctx.flags.has(tab)) { ctx.flags.set(tab, tab._renameInFlight); tab._renameInFlight = true; }
  }
  async function reserve(ctx, files) {
    const owners = openOwners(files, ctx.source);
    if (owners.some(owner => !clean(owner.tab, ctx))) throw failure('busy');
    ctx.owners = owners;
    for (const { tab } of owners) freeze(ctx, tab);
    for (const { tab } of owners) {
      if (window.ReadMDAiHarness) {
        if (!await window.ReadMDAiHarness.reserveTabForRename(tab)) throw failure('busy');
        ctx.ai.push(tab);
      }
      const mark = await window.ReadMDReview?.beginRename(tab);
      if (!mark) throw failure('busy');
      ctx.review.push(tab);
      const before = window.ReadMDReview.refactorState(tab);
      if (!before || !clean(tab, ctx)) throw failure('conflict');
      ctx.before.set(tab, before);
      if (tab === ctx.source) ctx.review_session = mark.review_session;
    }
    return owners;
  }
  function stableOwners(ctx, files) {
    const latest = openOwners(files, ctx.source);
    if (latest.length !== ctx.owners.length || latest.some(owner => !ctx.before.has(owner.tab))) return false;
    return ctx.owners.every(({ tab }) => {
      if (!clean(tab, ctx)) return false;
      const before = ctx.before.get(tab), now = window.ReadMDReview?.refactorState(tab);
      return now && JSON.stringify(now) === JSON.stringify(before);
    });
  }
  function release(ctx) {
    if (current !== ctx) return;
    for (const tab of ctx.review) window.ReadMDReview?.endRename(tab);
    for (const tab of ctx.ai) window.ReadMDAiHarness?.releaseTab(tab);
    for (const [tab, previous] of ctx.flags) {
      if (previous === undefined) delete tab._renameInFlight; else tab._renameInFlight = previous;
    }
    current = null;
    for (const resolve of waiters) resolve();
    waiters.clear();
    window.ReadMDSession?.persist();
  }
  function context(source, pending = {}) {
    return { source, operation_id: pending.operation_id || 'refactor-' + window.crypto.randomUUID(), old_path: pending.old_path || source?.path,
      workspace_root: pending.workspace_root, plan_hash: pending.plan_hash || '', pending: source?._renamePending || null,
      owners: [], flags: new Map(), before: new Map(), ai: [], review: [], sent: false, applied: false, restoring: false };
  }
  async function preview(ctx, newStem) {
    return planData(ctx, await request(ctx, 'preview', { path: ctx.old_path, new_stem: newStem, workspace_root: ctx.workspace_root }));
  }
  async function confirmPlan(plan) {
    if (!window.ReadMDModal || !document.body) return false;
    const modal = document.createElement('div');
    modal.id = 'document-refactor-modal'; modal.className = 'modal hidden'; modal.setAttribute('role', 'dialog'); modal.setAttribute('aria-modal', 'true');
    modal.setAttribute('aria-labelledby', 'document-refactor-title'); modal.dataset.backdrop = 'static';
    const box = document.createElement('section'); box.className = 'document-refactor-box';
    const title = document.createElement('h2'); title.id = 'document-refactor-title'; title.textContent = t('cmd.rename');
    const source = document.createElement('p'); source.className = 'document-refactor-source'; source.textContent = plan.source_path + ' → ' + plan.target_path;
    const list = document.createElement('div'); list.className = 'document-refactor-files';
    for (const file of plan.files) {
      const details = document.createElement('details'); details.open = true;
      const summary = document.createElement('summary'); summary.textContent = file.source_path + (key(file.source_path) === key(file.target_path) ? '' : ' → ' + file.target_path);
      details.appendChild(summary);
      const before = lf(file.before);
      for (const edit of file.edits) {
        const row = document.createElement('div'); row.className = 'document-refactor-edit';
        const line = document.createElement('span'); line.textContent = t('inspection.atLine', { line: before.slice(0, edit.from).split('\n').length });
        const original = document.createElement('code'), arrow = document.createElement('span'), replacement = document.createElement('code');
        original.textContent = before.slice(edit.from, edit.to); arrow.textContent = ' → '; replacement.textContent = edit.insert;
        row.append(line, original, arrow, replacement); details.appendChild(row);
      }
      list.appendChild(details);
    }
    const actions = document.createElement('div'); actions.className = 'document-refactor-actions';
    const cancel = document.createElement('button'), confirm = document.createElement('button');
    cancel.type = confirm.type = 'button'; cancel.className = 'tb-btn'; confirm.className = 'tb-btn accent';
    cancel.textContent = t('common.cancel'); cancel.setAttribute('data-modal-initial', ''); cancel.setAttribute('data-modal-close', ''); confirm.textContent = t('dialog.confirm');
    actions.append(cancel, confirm); box.append(title, source, list, actions); modal.appendChild(box); document.body.appendChild(modal);
    return new Promise(resolve => {
      let finished = false;
      const finish = value => {
        if (finished) return; finished = true;
        window.ReadMDModal.setGuard(modal.id, null); window.ReadMDModal.close(modal); modal.remove(); resolve(value);
      };
      cancel.addEventListener('click', () => finish(false)); confirm.addEventListener('click', () => finish(true));
      window.ReadMDModal.setGuard(modal.id, () => { finish(false); return false; });
      window.ReadMDModal.open(modal);
    });
  }
  async function openFile(path) {
    const response = await apiFetch('/api/file?p=' + encodeURIComponent(path));
    if (!response.ok) throw failure('recovery');
    const file = await response.json();
    if (key(file?.path) !== key(path) || !hash(file.revision) || !hash(file.disk_revision) || typeof file.content !== 'string' ||
        typeof (file.original ?? file.content) !== 'string' || typeof file.name !== 'string' || !file.name || typeof file.dir !== 'string') throw failure('recovery');
    return file;
  }
  function receiptFiles(ctx, receipt) {
    if (receipt?.operation_id !== ctx.operation_id || receipt.plan_hash !== ctx.plan_hash || !['committed', 'rolled_back'].includes(receipt.state) ||
        !Array.isArray(receipt.files) || !receipt.files.length || receipt.files.length > MAX_FILES) throw failure('recovery');
    const seen = new Set(), paths = new Set();
    for (const row of receipt.files) {
      const review = row?.review;
      if (typeof row?.old_path !== 'string' || !row.old_path || typeof row.path !== 'string' || !row.path || !hash(row.before_disk_revision) || !hash(row.disk_revision) ||
          seen.has(key(row.old_path)) || paths.has(key(row.path)) ||
          (row.identity !== undefined && (typeof row.identity !== 'string' || !row.identity || row.identity.length > 512)) ||
          !review?.document_id || key(review.path) !== key(row.path) ||
          typeof review.content !== 'string' || lf(review.content) !== review.content || !hash(review.revision) || review.disk_revision !== row.disk_revision ||
          !Number.isSafeInteger(review.generation) || !Number.isSafeInteger(review.review_revision) || !Array.isArray(review.changes)) throw failure('recovery');
      seen.add(key(row.old_path)); paths.add(key(row.path));
    }
    const source = receipt.files.find(row => key(row.old_path) === key(ctx.old_path));
    if (!source || source.before_disk_revision !== ctx.pending?.expected_disk_revision ||
        (ctx.pending?.review_session && source.review.document_id !== ctx.pending.review_session.document_id) ||
        (receipt.path && key(receipt.path) !== key(source.path))) throw failure('recovery');
    if (ctx.plan && (receipt.files.length !== ctx.plan.files.length || receipt.files.some(row => {
      const planned = ctx.plan.files.find(file => key(file.source_path) === key(row.old_path));
      return !planned || planned.before_disk_revision !== row.before_disk_revision || key(row.path) !== key(receipt.state === 'committed' ? planned.target_path : planned.source_path) ||
        row.disk_revision !== (receipt.state === 'committed' ? planned.after_disk_revision : planned.before_disk_revision);
    }))) throw failure('recovery');
    return receipt.files;
  }
  async function verifiedFiles(ctx, receipt) {
    const rows = receiptFiles(ctx, receipt), files = [];
    for (const row of rows) {
      const file = await openFile(row.path);
      // File revisions include decoded original line endings; review revisions
      // hash LF text. Raw-byte SHA and exact normalized text bind the two.
      const digest = await window.crypto.subtle.digest('SHA-256', new TextEncoder().encode(row.review.content));
      const normalizedRevision = Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, '0')).join('');
      if (file.disk_revision !== row.disk_revision || (row.identity !== undefined && file.identity !== row.identity) ||
          normalizedRevision !== row.review.revision || lf(file.original ?? file.content) !== row.review.content) throw failure('recovery');
      files.push({ row, file });
    }
    return files;
  }
  function updateTabs(ctx, items) {
    for (const { tab, row, file } of items) {
      const oldPath = tab.path || row.old_path;
      Object.assign(tab, { mode: 'file', source: 'file', isVirtual: false, path: file.path, dir: file.dir,
        name: file.name, title: file.name, original: file.original ?? file.content, content: file.content, fixed: file.content,
        revision: file.revision, mtime: file.mtime || 0, size: file.size || 0, encoding: file.encoding || tab.encoding,
        fixes: file.fixes || [], stats: file.stats || {}, isDirty: false, unsavedCreation: false, externalChanged: false });
      if (tab.originPath && key(tab.originPath) === key(oldPath)) tab.originPath = file.path;
    }
    if (typeof state !== 'undefined' && Array.isArray(state.folderFiles)) {
      // Map only verified receipt paths. Do not reopen the folder or steal the
      // user's current sidebar selection while reconciling a background task.
      const paths = new Map(items.map(({ row, file }) => [key(row.old_path), file.path]));
      state.folderFiles = state.folderFiles.map(path => paths.get(key(path)) || path);
    }
    const active = typeof getActiveTab === 'function' ? getActiveTab() : null;
    if (active && items.some(item => item.tab === active)) {
      if (typeof syncStateFromActiveTab === 'function') syncStateFromActiveTab();
      document.title = active.name + ' - ReadMD';
      if (typeof setFileTitle === 'function') setFileTitle(active.name, true, active.path);
    }
  }
  async function publishSettlement(ctx) {
    delete ctx.source._renamePending;
    let saved = false;
    try { saved = await window.ReadMDSession?.flush() === true; } catch (_) { /* Keep the same durable intent for reconciliation. */ }
    if (!saved) {
      ctx.source._renamePending = ctx.pending; window.ReadMDSession?.persist(); throw failure('persistence');
    }
    if (ctx.owners.some(owner => typeof getActiveTab === 'function' && owner.tab === getActiveTab()) && typeof state !== 'undefined' && !state.editing && typeof renderActiveTab === 'function') {
      try { await renderActiveTab({ restoreScroll: true }); } catch (_) { if (typeof renderTabsBar === 'function') renderTabsBar(); notify('recovery'); }
    } else if (typeof renderTabsBar === 'function') renderTabsBar();
    if (typeof renderFolderList === 'function') {
      try { renderFolderList(); } catch (_) { notify('recovery'); }
    }
    release(ctx);
    return true;
  }
  async function adoptReceipt(ctx, receipt) {
    const verified = await verifiedFiles(ctx, receipt);
    if (!ctx.owners.length) await reserve(ctx, receipt.files);
    const currentOwners = openOwners(receipt.files, ctx.source);
    if (currentOwners.length !== ctx.owners.length || currentOwners.some(owner => !ctx.before.has(owner.tab))) throw failure('recovery');
    const items = ctx.owners.map(({ tab, file: original }) => {
      const found = verified.find(item => key(item.row.old_path) === key(original.source_path || original.old_path));
      if (!found || !clean(tab, ctx)) throw failure('recovery');
      const before = ctx.before.get(tab), planned = ctx.plan?.files.find(file => key(file.source_path) === key(found.row.old_path));
      const edits = receipt.state === 'committed' && planned ? planned.edits : undefined;
      return { tab, before, snapshot: found.row.review, file_revision: found.file.revision, edits, row: found.row, file: found.file };
    });
    if (!window.ReadMDReview?.validateRefactorGroup(items)) throw failure('recovery');
    if (!window.ReadMDReview.applyRefactorGroup(items, () => updateTabs(ctx, items))) throw failure('recovery');
    ctx.applied = true;
    return publishSettlement(ctx);
  }
  async function settle(ctx, receipt) {
    if (receipt?.operation_id !== ctx.operation_id || receipt.plan_hash !== ctx.plan_hash) throw failure('unknown');
    if (receipt.state === 'not_started' || receipt.state === 'not_found') receipt = await request(ctx, 'abandon');
    if (receipt?.operation_id !== ctx.operation_id || receipt.plan_hash !== ctx.plan_hash) throw failure('unknown');
    if (receipt.state === 'abandoned') {
      const source = await openFile(ctx.old_path);
      if (source.revision !== ctx.pending.expected_revision || source.disk_revision !== ctx.pending.expected_disk_revision ||
          !stableOwners(ctx, ctx.plan?.files || ctx.owners.map(owner => owner.file))) throw failure('recovery');
      if (ctx.source.isVirtual && ctx.source._recoveryContent === undefined) updateTabs(ctx, [{ tab: ctx.source, row: { old_path: ctx.old_path }, file: source }]);
      ctx.outcome = receipt.state;
      const saved = await publishSettlement(ctx);
      if (saved) notify(receipt.error_code || ctx.failureCode || 'conflict');
      return saved;
    }
    if (receipt.state === 'committed' || receipt.state === 'rolled_back') {
      ctx.outcome = receipt.state;
      const saved = await adoptReceipt(ctx, receipt);
      if (saved && receipt.state === 'rolled_back') notify(receipt.error_code || ctx.failureCode || 'conflict');
      return saved;
    }
    throw failure('recovery');
  }
  async function rename(tab, newTitle) {
    if (current) { notify('busy'); return false; }
    const root = typeof state !== 'undefined' ? state.folder : '';
    if (!tab?.path || tab.mode !== 'file' || typeof root !== 'string' || !root || !window.ReadMDSession?.prepareRename) { notify('conflict'); return false; }
    const ctx = context(tab, { workspace_root: root }); current = ctx; freeze(ctx, tab);
    const ext = (tab.path.match(/\.[^\\/.]+$/) || [''])[0];
    const stem = ext && String(newTitle).toLowerCase().endsWith(ext.toLowerCase()) ? String(newTitle).slice(0, -ext.length) : String(newTitle || '');
    let keep = false;
    try {
      if (!stem || /[\\/]/.test(stem) || !clean(tab, ctx)) throw failure('busy');
      const first = await preview(ctx, stem);
      await reserve(ctx, first.files);
      const second = await preview(ctx, stem);
      if (JSON.stringify(roster(first)) !== JSON.stringify(roster(second)) || !stableOwners(ctx, second.files)) throw failure('conflict');
      for (const { tab: owner } of ctx.owners) {
        const planned = second.files.find(file => key(file.source_path) === key(owner.path)), before = ctx.before.get(owner);
        if (!planned || lf(planned.before) !== before.content || (before.document_id && (!planned.review_head ||
            planned.review_head.document_id !== before.document_id || planned.review_head.generation !== before.generation || planned.review_head.review_revision !== before.review_revision))) throw failure('conflict');
      }
      const response = await apiFetch('/api/file?meta=1&p=' + encodeURIComponent(ctx.old_path));
      const metadata = response.ok ? await response.json() : null, source = second.files.find(file => key(file.source_path) === key(ctx.old_path));
      if (!metadata || metadata.revision !== tab.revision || metadata.disk_revision !== source.before_disk_revision || state.folder !== root) throw failure('conflict');
      ctx.plan = second; ctx.plan_hash = second.plan_hash;
      if (!await confirmPlan(second)) return false;
      if (state.folder !== root || !stableOwners(ctx, second.files)) throw failure('conflict');
      const pending = { operation_id: ctx.operation_id, old_path: ctx.old_path, new_stem: stem,
        expected_revision: metadata.revision, expected_disk_revision: metadata.disk_revision,
        refactor: true, plan_hash: ctx.plan_hash, workspace_root: root, ...(ctx.review_session ? { review_session: ctx.review_session } : {}) };
      ctx.pending = pending;
      const prepared = await window.ReadMDSession.prepareRename(tab, pending);
      keep = prepared || window.ReadMDSession.uncertainRename?.(pending.operation_id) === true;
      if (!prepared) throw failure('persistence');
      ctx.sent = true;
      let receipt;
      try { receipt = await request(ctx, 'commit', { path: ctx.old_path, new_stem: stem, workspace_root: root, confirmed: true }); }
      catch (_) { receipt = await request(ctx, 'status'); }
      if (!['committed', 'rolled_back', 'not_started', 'abandoned'].includes(receipt.state)) receipt = await request(ctx, 'status');
      const saved = await settle(ctx, receipt);
      if (saved && ctx.outcome === 'committed' && typeof showToast === 'function') showToast(t('toast.renamedTo', { name: tab.name }));
      return saved && ctx.outcome === 'committed';
    } catch (error) {
      if (ctx.pending && tab._renamePending?.operation_id === ctx.operation_id && window.ReadMDSession?.uncertainRename?.(ctx.operation_id) === true) keep = true;
      if (ctx.sent || keep) { keep = true; if (ctx.pending) tab._renamePending = ctx.pending; }
      notify(ctx.sent && error.code !== 'persistence' ? 'recovery' : error.code || 'conflict');
      return false;
    } finally {
      if (!keep && current === ctx) { if (tab._renamePending?.operation_id === ctx.operation_id) delete tab._renamePending; release(ctx); }
    }
  }
  async function recover(tab) {
    const pending = tab?._renamePending;
    if (!pending?.refactor || !hash(pending.plan_hash)) return false;
    if (current && current.operation_id !== pending.operation_id) { notify('busy'); return false; }
    const ctx = current || context(tab, pending); current = ctx; ctx.source = tab; ctx.pending = pending; freeze(ctx, tab);
    try {
      if (window.ReadMDSession?.uncertainRename?.(pending.operation_id) && !await window.ReadMDSession.flush()) throw failure('persistence');
      if (!ctx.owners.length && ctx.plan) await reserve(ctx, ctx.plan.files);
      let receipt = await request(ctx, 'status');
      if (receipt.state === 'not_started' || receipt.state === 'not_found') {
        if (!ctx.owners.length) await reserve(ctx, [{ source_path: pending.old_path, target_path: pending.old_path }]);
      }
      return await settle(ctx, receipt);
    } catch (error) { notify(error.code === 'persistence' ? 'persistence' : 'recovery'); return false; }
  }
  async function restoreDescriptor(item) {
    const pending = item?.renamePending;
    if (!pending?.refactor || !hash(pending.plan_hash)) return { descriptor: item, file: null, settled: false };
    if (current && current.operation_id !== pending.operation_id) return { descriptor: item, file: null, settled: false };
    const ctx = current || context(null, pending); current = ctx; ctx.pending = pending; ctx.restoring = true;
    ctx.restoreReads = (ctx.restoreReads || 0) + 1;
    let receipt;
    try {
      receipt = await request(ctx, 'status');
      if (receipt.state === 'not_started' || receipt.state === 'not_found') receipt = await request(ctx, 'abandon');
      let file;
      if (receipt.state === 'abandoned') {
        file = await openFile(pending.old_path);
        if (file.revision !== pending.expected_revision || file.disk_revision !== pending.expected_disk_revision) throw failure('recovery');
      } else {
        const verified = await verifiedFiles(ctx, receipt);
        file = verified.find(item => key(item.row.old_path) === key(pending.old_path))?.file;
      }
      if (!file) throw failure('recovery');
      ctx.receipt = receipt;
      return { descriptor: { ...item, path: file.path, name: file.name, dir: file.dir, revision: file.revision, renamePending: null }, file, receipt, settled: true };
    } catch (_) { return { descriptor: item, file: null, receipt, settled: false }; }
    finally { ctx.restoreReads--; }
  }
  function finishRestore(saved) {
    if (!current?.restoring) return;
    if (saved === true) {
      const ctx = current; release(ctx);
      if (ctx.receipt?.state === 'rolled_back') notify(ctx.receipt.error_code || 'conflict');
    }
  }
  function suspendRestore() {
    if (!current?.restoring || current.source || current.owners.length || current.restoreReads) return false;
    release(current); return true;
  }
  return { rename, recover, restoreDescriptor, finishRestore, suspendRestore, busy: () => Boolean(current),
    whenIdle: () => current ? new Promise(resolve => waiters.add(resolve)) : Promise.resolve() };
})();
