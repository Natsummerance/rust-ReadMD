'use strict';
/* Durable AI jobs belong to the document, independently of panel visibility.
   The service owns tool execution, budgets and provider/Skill resolution. */
window.ReadMDAiHarness = (() => {
  const tasks = new Map(), bindings = new Map(), timers = new Map(), polls = new Map(), actions = new Map(), waiters = new Map();
  const starting = new Set(), startedHere = new Set(), closingTabs = new WeakSet();
  const activeStates = new Set(['queued', 'running']);
  const states = new Set(['queued', 'running', 'paused', 'needs_user_resume', 'budget_exhausted', 'completed', 'failed']);
  let initialization = null;
  const t = (key, params) => window.i18n?.t(key, params) || key;
  const activeTab = () => typeof getActiveTab === 'function' ? getActiveTab() : null;
  const allTabs = () => typeof state !== 'undefined' && Array.isArray(state.tabs) ? state.tabs : [];
  const id = () => 'ai-task-' + window.crypto.randomUUID();
  function failure(code) { const error = new Error(code); error.code = code; return error; }
  function notify(error) {
    const key = 'error.' + (error?.code || ''), localized = t(key);
    if (typeof showToast === 'function') showToast(localized !== key ? localized : t(/conflict|stale/.test(error?.code || '') ? 'review.conflict' : 'review.aiFailed'));
  }
  async function request(action, payload = {}) {
    const response = await apiFetch('/api/ai/tasks', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ action, ...payload }) });
    const data = await response.json();
    if (!response.ok || data.ok === false || data.error_code || data.error) throw failure(data.error_code || 'ai_task_failed');
    return data;
  }
  function tabFor(documentId) {
    return allTabs().find(tab => (window.ReadMDReview?.snapshot(tab)?.document_id || tab.reviewSession?.document_id) === documentId) || null;
  }
  function publicTask(value) {
    if (!value || typeof value.id !== 'string' || typeof value.document_id !== 'string' || !states.has(value.state)) throw failure('ai_task_failed');
    const count = value => Number.isSafeInteger(value) && value >= 0 ? value : 0;
    return { id: value.id, document_id: value.document_id, state: value.state, rounds: count(value.rounds),
      elapsed_ms: count(value.elapsed_ms), last_event_id: count(value.last_event_id), owned_here: value.owned_here === true,
      error_code: typeof value.error_code === 'string' && /^[a-z][a-z0-9_]{0,95}$/.test(value.error_code) ? value.error_code : null };
  }
  function settle(task, snapshot) {
    if (activeStates.has(task.state)) return;
    const pending = waiters.get(task.id) || [];
    waiters.delete(task.id);
    for (const waiter of pending) {
      if (task.state === 'completed') waiter.resolve(snapshot || window.ReadMDReview?.snapshot(bindings.get(task.id)?.tab) || null);
      else waiter.reject(failure(task.state === 'failed' ? 'ai_task_failed' : 'cancelled'));
    }
  }
  function schedule(task) {
    clearTimeout(timers.get(task.id)); timers.delete(task.id);
    if (!activeStates.has(task.state)) return;
    timers.set(task.id, setTimeout(() => { timers.delete(task.id); void status(task.id).catch(() => {}); }, 1000));
  }
  async function ingest(data, tab) {
    const task = publicTask(data.task), previous = tasks.get(task.id);
    // A status poll started before Pause/Resume cannot roll back its receipt.
    if (previous && task.last_event_id < previous.last_event_id) return previous;
    tasks.set(task.id, task);
    const bound = bindings.get(task.id)?.tab;
    const owner = tab && allTabs().includes(tab) ? tab : bound && allTabs().includes(bound) ? bound : tabFor(task.document_id);
    if (owner) bindings.set(task.id, { tab: owner, document_id: task.document_id });
    const receipt = data.snapshot;
    if (receipt && receipt.document_id === task.document_id && owner && allTabs().includes(owner)) await window.ReadMDReview?.adopt(receipt, owner);
    settle(task, receipt);
    schedule(task); render();
    if (typeof window.dispatchEvent === 'function') window.dispatchEvent(new CustomEvent('readmd:ai-task-state', { detail: { task: { ...task } } }));
    return task;
  }
  function init() {
    if (window.READMD_ENGINE !== 'rust') return Promise.resolve(true);
    if (initialization) return initialization;
    initialization = request('list').then(async data => {
      if (!Array.isArray(data.tasks)) throw failure('ai_task_failed');
      for (const task of [...data.tasks].reverse()) await ingest({ task });
      return true;
    }).catch(() => { initialization = null; return false; });
    return initialization;
  }
  async function start(options, stamp, tab = activeTab()) {
    if (!tab || closingTabs.has(tab) || !allTabs().includes(tab) || !stamp) throw failure('review_stale_generation');
    if (options.signal?.aborted) throw failure('cancelled');
    const work = (async () => {
      const latest = await window.ReadMDReview?.capture(tab);
      if (!latest || latest.document_id !== stamp.document_id || latest.generation !== stamp.generation || latest.review_revision !== stamp.review_revision ||
          latest.content !== stamp.content || latest.editGeneration !== stamp.editGeneration || closingTabs.has(tab) || !allTabs().includes(tab)) throw failure('review_stale_generation');
      const data = await request('start', { doc_id: stamp.document_id, generation: stamp.generation, review_revision: stamp.review_revision,
        request: options.request || '', provider: options.provider, credential_id: options.credential_id, model: options.model, stream: options.stream,
        skill_id: options.skill_id, ranges: options.ranges, language: window.i18n?.currentLang || 'en', call_id: options.task_id || id() });
      // Deliberately do not attach the panel/inline-bar AbortSignal to a
      // durable job. Explicit Pause and document/app close own that lifecycle.
      const task = await ingest(data, tab);
      startedHere.add(task.id);
      return task;
    })();
    const pending = { tab, work }; starting.add(pending);
    try { return await work; } finally { starting.delete(pending); }
  }
  function status(taskId) {
    if (polls.has(taskId)) return polls.get(taskId);
    const task = tasks.get(taskId);
    if (!task) return Promise.reject(failure('ai_task_failed'));
    const work = request('status', { task_id: taskId, after_event_id: task.last_event_id }).then(data => ingest(data)).catch(error => {
      schedule(tasks.get(taskId) || task);
      throw error;
    }).finally(() => { if (polls.get(taskId) === work) polls.delete(taskId); });
    polls.set(taskId, work);
    return work;
  }
  function action(kind, taskId) {
    const prior = actions.get(taskId) || Promise.resolve();
    const work = prior.catch(() => {}).then(async () => {
      const task = tasks.get(taskId);
      if (!task) throw failure('ai_task_failed');
      if (kind === 'resume' && (window.ReadMDRefactor?.busy?.() || allTabs().some(tab => closingTabs.has(tab) &&
          (bindings.get(task.id)?.tab === tab || (window.ReadMDReview?.snapshot(tab)?.document_id || tab.reviewSession?.document_id) === task.document_id))))
        throw failure('review_stale_generation');
      if (kind === 'pause' && !activeStates.has(task.state)) return task;
      if (kind === 'resume' && (activeStates.has(task.state) || task.state === 'completed')) return task;
      clearTimeout(timers.get(taskId)); timers.delete(taskId);
      // A resume always comes from an explicit control. Listing and polling
      // can report a paused job but never invoke a provider to continue it.
      return ingest(await request(kind, { task_id: taskId, call_id: id() }));
    }).finally(() => { if (actions.get(taskId) === work) actions.delete(taskId); });
    actions.set(taskId, work);
    return work;
  }
  async function pauseTab(tab) {
    closingTabs.add(tab);
    const documentId = window.ReadMDReview?.snapshot(tab)?.document_id || tab.reviewSession?.document_id;
    if (!await init()) {
      const known = [...starting].some(pending => pending.tab === tab) || [...tasks.values()].some(task =>
        (bindings.get(task.id)?.tab === tab || task.document_id === documentId) && activeStates.has(task.state));
      if (!known) return true;
      closingTabs.delete(tab); notify(failure('ai_task_failed')); return false;
    }
    await Promise.allSettled([...starting].filter(pending => pending.tab === tab).map(pending => pending.work));
    try {
      for (const task of tasks.values()) if ((bindings.get(task.id)?.tab === tab || task.document_id === documentId) && activeStates.has(task.state)) await action('pause', task.id);
      return true;
    } catch (error) { closingTabs.delete(tab); notify(error); return false; }
  }
  // A rename must not silently stop a running task or allow a new one to
  // capture the old path while the durable review identity is moving.
  async function reserveTabForRename(tab) {
    if (!tab || closingTabs.has(tab)) return false;
    closingTabs.add(tab);
    let reserved = false;
    try {
      if (!await init()) return false;
      await Promise.allSettled([...starting].filter(pending => pending.tab === tab).map(pending => pending.work));
      const documentId = window.ReadMDReview?.snapshot(tab)?.document_id || tab.reviewSession?.document_id;
      if ([...tasks.values()].some(task => (bindings.get(task.id)?.tab === tab || task.document_id === documentId) && activeStates.has(task.state))) return false;
      reserved = true; return true;
    } finally {
      if (!reserved) closingTabs.delete(tab);
    }
  }
  async function pauseAll() {
    // Optional task discovery cannot block an otherwise ordinary quit. Any
    // already known jobs owned here must still persist an explicit pause.
    await init();
    await Promise.allSettled([...starting].map(pending => pending.work));
    try {
      for (const task of tasks.values()) if (activeStates.has(task.state) && (task.owned_here || startedHere.has(task.id))) await action('pause', task.id);
      return true;
    } catch (error) { notify(error); return false; }
  }
  function wait(taskId) {
    const task = tasks.get(taskId);
    if (!task) return Promise.reject(failure('ai_task_failed'));
    if (task.state === 'completed') return Promise.resolve(window.ReadMDReview?.snapshot(bindings.get(taskId)?.tab) || null);
    if (!activeStates.has(task.state)) return Promise.reject(failure(task.state === 'failed' ? 'ai_task_failed' : 'cancelled'));
    return new Promise((resolve, reject) => { if (!waiters.has(taskId)) waiters.set(taskId, []); waiters.get(taskId).push({ resolve, reject }); });
  }
  function render() {
    if (typeof document === 'undefined') return;
    let host = document.getElementById('ai-task-status');
    const output = document.getElementById('ai-output');
    if (!host && output) {
      host = document.createElement('div'); host.id = 'ai-task-status'; host.className = 'ai-task-status';
      host.setAttribute('role', 'status'); output.insertAdjacentElement('beforebegin', host);
    }
    if (!host) return;
    host.replaceChildren();
    const tab = activeTab(), documentId = window.ReadMDReview?.snapshot(tab)?.document_id || tab?.reviewSession?.document_id;
    const visible = [...tasks.values()].filter(task => task.document_id === documentId || bindings.get(task.id)?.tab === tab).reverse();
    host.hidden = !visible.length;
    const resumable = visible.filter(task => task.state !== 'completed');
    const terminal = visible.find(task => task.state === 'completed');
    function rowFor(task) {
      const row = document.createElement('div'); row.className = 'ai-task-row';
      const label = document.createElement('span');
      const key = activeStates.has(task.state) ? 'ai.taskRunning' : task.state === 'completed' ? 'ai.taskCompleted' : task.state === 'failed' ? 'ai.taskFailed' : 'ai.taskPaused';
      const parts = [t(key), t('ai.taskRounds', { count: task.rounds }), t('ai.taskElapsed', { seconds: Math.floor(task.elapsed_ms / 1000) })];
      if (!activeStates.has(task.state) && task.error_code) {
        const reasonKey = 'error.' + task.error_code, localized = t(reasonKey);
        const reason = localized !== reasonKey ? localized : /budget|limit/.test(task.error_code) ? t('review.limit') : /stale|conflict/.test(task.error_code) ? t('review.conflict') : task.error_code === 'review_ai_interrupted' ? t('ai.taskInterrupted') : '';
        if (reason) parts.push(reason);
      }
      label.textContent = parts.join(' · ');
      row.appendChild(label);
      if (task.state !== 'completed') {
        const control = document.createElement('button'); control.type = 'button'; control.className = 'tb-btn';
        const kind = activeStates.has(task.state) ? 'pause' : 'resume';
        control.textContent = t(kind === 'pause' ? 'ai.taskCancel' : 'ai.taskResume');
        control.addEventListener('click', async () => {
          control.disabled = true;
          try { await action(kind, task.id); } catch (error) { notify(error); render(); }
        });
        row.appendChild(control);
      }
      return row;
    }
    for (const task of resumable.slice(0, 3)) host.appendChild(rowFor(task));
    if (terminal) host.appendChild(rowFor(terminal));
    if (resumable.length > 3) {
      const history = document.createElement('details'); history.className = 'ai-task-history';
      const summary = document.createElement('summary'); summary.textContent = t('ai.history') + ' · ' + (resumable.length - 3);
      history.appendChild(summary);
      for (const task of resumable.slice(3)) history.appendChild(rowFor(task));
      host.appendChild(history);
    }
  }
  function attachDocument() {
    const tab = activeTab(), documentId = window.ReadMDReview?.snapshot(tab)?.document_id || tab?.reviewSession?.document_id;
    for (const task of tasks.values()) if (tab && task.document_id === documentId && bindings.get(task.id)?.tab !== tab) {
      bindings.set(task.id, { tab, document_id: documentId });
      // Restoring a document can finish after its last background poll. A
      // status read retrieves that receipt without resuming the provider.
      if (task.state === 'completed') void status(task.id).catch(() => {});
    }
    render();
  }
  document.addEventListener?.('readmd:document-loaded', attachDocument);
  window.addEventListener?.('readmd:review-state', attachDocument);
  window.addEventListener?.('readmd:language-changed', render);
  const launch = () => { if (window.READMD_ENGINE === 'rust' && !window.__STARTUP_PROBE__) void init(); };
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', launch); else setTimeout(launch, 0);
  return { init, start, status, pause: taskId => action('pause', taskId), resume: taskId => action('resume', taskId), pauseTab, reserveTabForRename, releaseTab: tab => closingTabs.delete(tab), pauseAll, wait, render,
    hasTask: tab => [...tasks.values()].some(task => bindings.get(task.id)?.tab === tab || task.document_id === (window.ReadMDReview?.snapshot(tab)?.document_id || tab?.reviewSession?.document_id)),
    list: () => [...tasks.values()].map(task => ({ ...task })) };
})();
