'use strict';
/* ============================================================
   ReadMD Features - Unified Batch Workbench (convert + OCR)
   ============================================================ */

/* ---------------- 批量工作台：文档批量转换 + 图片逐张 OCR ---------------- */

let batchJobTimer = null;
let batchJobId = null;
let batchGeneration = 0;
let batchPollAbort = null;
let batchCancelPending = false;
let batchCancelAcknowledged = false;
let batchCancelRequested = false;
let batchOcrCanceled = false;
let batchDocsDone = false;
let batchOcrDone = false;
let batchFinished = false;
let batchCount = { ok: 0, skipped: 0, error: 0, canceled: 0 };
let batchAttempt = 0;
let batchOutputDir = '';
let batchOutputParent = '';
const batchRowsBySrc = Object.create(null);

function batchT(k, p) {
  return window.i18n ? window.i18n.t(k, p) : k;
}

function batchFailureMessage(item) {
  const batchKey = {
    output_exists: 'convert.existsTitle',
    empty_output: 'note.convert_no_text',
    batch_interrupted: 'batch.incompleteResult',
    batch_running: 'batch.alreadyRunning',
    batch_attempt_changed: 'audit.retryLoad',
    batch_no_retry_items: 'ai.taskCompleted',
    batch_start_failed: 'error.conversion_failed',
  }[item?.error_code];
  if (batchKey) return batchT(batchKey);
  const message = apiMessage(item, 'error.conversion_failed');
  const details = item?.error_details;
  if (!details || typeof details !== 'object' || Array.isArray(details)) return message;
  const code = item.error_code;
  const keys = Object.keys(details).sort().join(',');
  if (['docx_note_settings_invalid', 'docx_note_settings_unsupported', 'author_inline_text_invalid', 'author_inline_text_unsupported', 'author_inline_text_budget'].includes(code)
      && keys === 'detail,part,path') {
    const bounded = (value, bytes) => typeof value === 'string' && value.length > 0
      && value.length <= bytes && !/[\u0000-\u001f\u007f-\u009f]/u.test(value)
      && new TextEncoder().encode(value).length <= bytes;
    if (bounded(details.part, 512) && bounded(details.path, 2048) && bounded(details.detail, 2048)) {
      return [message, details.part, details.path, details.detail].join('\n');
    }
  } else if (['docx_numbering_invalid', 'docx_numbering_unrepresentable'].includes(code)
      && keys === 'reason' && typeof details.reason === 'string'
      && /^[a-z0-9_]{1,128}$/.test(details.reason)) {
    return message + '\n' + details.reason;
  }
  return message;
}

function openBatchModal() {
  $('convert-modal')?.classList.remove('hidden');
  if (typeof initSpeechLanguage === 'function') initSpeechLanguage();
  updateBatchOutputChoice();
}

function resetBatchRun() {
  stopBatchPoll();
  batchGeneration++;
  batchJobId = null;
  batchAttempt = 0;
  batchOutputDir = '';
  batchCancelRequested = false;
  batchCancelPending = false;
  batchCancelAcknowledged = false;
  batchOcrCanceled = false;
  batchFinished = false;
  batchCount = { ok: 0, skipped: 0, error: 0, canceled: 0 };
  for (const k in batchRowsBySrc) delete batchRowsBySrc[k];
  $('convert-open-dir')?.classList.add('hidden');
  $('batch-retry')?.classList.add('hidden');
  $('convert-list').innerHTML = '';
  $('convert-status').textContent = '';
  $('batch-cancel').classList.add('hidden');
  $('batch-cancel').disabled = false;
}

function updateBatchOutputChoice() {
  const output = $('convert-output-parent');
  if (!output) return;
  output.textContent = batchOutputParent || batchT('batch.defaultOutput');
  if (batchOutputParent) delete output.dataset.i18n;
  else output.dataset.i18n = 'batch.defaultOutput';
  output.title = batchOutputParent;
  $('convert-output-settings')?.classList.toggle('hidden', !hasPy || typeof py.choose_folder !== 'function');
  $('convert-output-reset')?.classList.toggle('hidden', !batchOutputParent);
}

async function chooseBatchOutputParent() {
  if (isBatchRunning() || !hasPy) return;
  try {
    const path = await py.choose_folder(batchOutputParent || state.folder || state.dir || '');
    if (typeof path === 'string' && path && !isBatchRunning()) {
      batchOutputParent = path;
      updateBatchOutputChoice();
    }
  } catch (_) { showToast(batchT('toast.readFolderFail')); }
}

function resetBatchOutputParent() {
  batchOutputParent = '';
  updateBatchOutputChoice();
}

function closeBatchModal() {
  $('convert-modal').classList.add('hidden');
}

function stopBatchPoll() {
  if (batchJobTimer) { clearTimeout(batchJobTimer); batchJobTimer = null; }
  batchPollAbort?.abort();
  batchPollAbort = null;
}

function makeBatchRow(path) {
  const row = document.createElement('div');
  row.className = 'batch-item queued';
  const nm = document.createElement('span');
  nm.className = 'batch-name';
  nm.textContent = path.split(/[\\/]/).pop();
  nm.title = path;
  const output = document.createElement('span');
  output.className = 'batch-output-name';
  nm.appendChild(output);
  const st = document.createElement('span');
  st.className = 'batch-state';
  st.textContent = batchT('batch.statusQueued') || '';
  row.appendChild(nm); row.appendChild(st);
  const open = async () => {
    if (!row.dataset.done || !row.dataset.out) return;
    closeBatchModal();
    await loadFile(row.dataset.out);
  };
  row.addEventListener('click', open);
  row.addEventListener('keydown', event => {
    if ((event.key === 'Enter' || event.key === ' ') && row.dataset.out) {
      event.preventDefault(); open();
    }
  });
  return row;
}

function setBatchRow(row, status, title) {
  // A completed result must never regress to an older progress snapshot.
  if (row.dataset.done) return;
  row.className = 'batch-item ' + status;
  if (title) {
    row.title = title;
    row.setAttribute('aria-description', title);
    if (status === 'error') row.tabIndex = 0;
  }
  if (row.dataset.out) {
    row.tabIndex = 0;
    row.setAttribute('role', 'button');
  }
  const st = row.querySelector('.batch-state');
  if (st) {
    const labels = {
      queued: batchT('batch.statusQueued') || '',
      running: batchT('batch.statusRunning') || '',
      ok: batchT('batch.statusOk') || '',
      skipped: batchT('batch.statusSkipped') || '',
      error: batchT('batch.statusError') || '',
      canceled: batchT('batch.statusCanceled') || '',
    };
    st.textContent = labels[status] || status;
    if (title) st.title = title;
  }
}

function countBatchRow(row, status) {
  if (row.dataset.done || !Object.hasOwn(batchCount, status)) return;
  row.dataset.done = '1';
  row.dataset.result = status;
  batchCount[status]++;
}

async function enqueueBatchFiles(paths, overwrite) {
  if (isBatchRunning()) {
    $('convert-modal')?.classList.remove('hidden');
    showToast(batchT('batch.alreadyRunning'));
    return;
  }
  const input = (paths || []).filter(p => typeof p === 'string' && p.trim());
  const extracted = [];
  // Selected/uploaded archives use the same guarded extractor as OS drops.
  // Reserve the workbench during extraction, before either conversion lane.
  setBatchTriggersBusy(true);
  try {
    for (const path of input) {
      if (/\.zip$/i.test(path)) {
        const result = await extractDroppedZip({ path, name: path.split(/[\\/]/).pop() });
        if (!result || result.ok === false) return;
        extracted.push(...(result.paths || []));
      } else extracted.push(path);
    }
  } catch (error) { showToast(error.message); return; }
  finally { setBatchTriggersBusy(false); }
  // Windows paths are case-insensitive; POSIX paths retain case. One source
  // owns one row and one output even when selected twice or present in a ZIP.
  const seen = new Set();
  const list = extracted.filter(path => {
    if (typeof path !== 'string' || !path.trim()) return false;
    let key = path.replace(/\\/g, '/');
    if (/^(?:[a-z]:\/|\/\/)/i.test(key)) key = key.toLowerCase();
    if (seen.has(key)) return false;
    seen.add(key); return true;
  });
  if (!list.length) { if (input.length) showToast(batchT('convert.noConvertibleFiles')); return; }
  if (list.some(p => IMG_RE.test(p)) && moduleBlocked('ocr')) return;
  if (list.some(p => !IMG_RE.test(p)) && moduleBlocked('convert')) return;
  resetBatchRun();
  openBatchModal();
  const docs = [], images = [];
  const listEl = $('convert-list');
  list.forEach(p => {
    const row = makeBatchRow(p);
    listEl.appendChild(row);
    if (IMG_RE.test(p)) images.push([p, row]);
    else { docs.push(p); batchRowsBySrc[p] = row; }
  });
  batchDocsDone = docs.length === 0;
  batchOcrDone = images.length === 0;
  batchFinished = false;
  setBatchTriggersBusy(true);
  $('batch-cancel').classList.remove('hidden');
  $('convert-status').textContent = batchT('batch.preparing') || '';
  const generation = batchGeneration;
  if (docs.length) runBatchDocsLane(docs, generation);
  if (images.length) runBatchOcrLane(images, overwrite, generation);
}

async function postBatch(payload) {
  const response = await apiFetch('/api/convert/batch', {
    method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ ...payload, confirm: true, workbench: true }),
  });
  return { response, data: await response.json().catch(() => ({})) };
}

async function runBatchDocsLane(paths, generation) {
  const job = window.crypto?.randomUUID?.().replaceAll('-', '');
  const requestedJob = job ? 'b-' + job : '';
  let submitted = false;
  try {
    if (!(await ensureModule('convert'))) throw new Error(batchT('convert.moduleUnavailable'));
    const payload = { paths, language: typeof currentSpeechLanguage === 'function' ? currentSpeechLanguage() : 'auto' };
    if (requestedJob) payload.job = requestedJob;
    if (batchOutputParent) payload.output_parent = batchOutputParent;
    submitted = true;
    const { response, data } = await postBatch(payload);
    if (generation !== batchGeneration) return;
    if (!response.ok && response.status < 500 && data.error_code !== 'batch_job_exists') {
      failBatchDocsLane(batchFailureMessage(data));
      return;
    }
    if (!response.ok || typeof data.job !== 'string' || !data.job) throw new Error(batchFailureMessage(data));
    batchJobId = data.job;
    if (batchCancelRequested) sendBatchCancel();
    pollBatchJob(data.job);
  } catch (e) {
    if (generation !== batchGeneration) return;
    // The POST response can be lost after the worker persisted its receipt.
    // Look up that precise job before releasing the workbench.
    if (submitted && requestedJob) {
      batchJobId = requestedJob;
      pollBatchJob(requestedJob, e?.message);
    } else failBatchDocsLane(e?.message);
  }
}

async function refreshBatchHistory() {
  const select = $('batch-history');
  if (!select) return;
  const generation = batchGeneration;
  const controller = new AbortController();
  const deadline = setTimeout(() => controller.abort(), 15000);
  try {
    const response = await apiFetch('/api/convert/progress?history=1', { signal: controller.signal });
    const history = response.ok ? (await response.json()).batches : null;
    if (generation !== batchGeneration || !Array.isArray(history)) return;
    const options = history.map(batch => {
      const option = document.createElement('option');
      option.value = batch.job;
      const timestamp = new Date(batch.modified).toLocaleString(window.i18n?.currentLang);
      const name = batch.name || batch.job.slice(0, 10);
      option.textContent = `${timestamp} · ${name}${batch.total > 1 ? ` (${batch.total})` : ''}`;
      return option;
    });
    const blank = document.createElement('option');
    blank.hidden = true;
    blank.value = '';
    select.replaceChildren(blank, ...options);
    select.value = batchJobId || '';
    $('batch-history-control')?.classList.toggle('hidden', options.length === 0);
  } catch (_) { /* Keep the existing history choices during a transport failure. */ }
  finally { clearTimeout(deadline); }
}

async function restoreBatchWorkbench(selected = '') {
  if (isBatchRunning() || (batchFinished && (!selected || selected === batchJobId))) return;
  let generation = batchGeneration;
  let following = false;
  setBatchTriggersBusy(true);
  const controller = new AbortController();
  const deadline = setTimeout(() => controller.abort(), 15000);
  try {
    const query = selected ? 'job=' + encodeURIComponent(selected) : 'latest=1';
    const r = await apiFetch('/api/convert/progress?' + query, { signal: controller.signal });
    if (generation !== batchGeneration || (!selected && r.status === 404)) return;
    if (!r.ok) throw new Error(batchFailureMessage(await r.json().catch(() => ({}))));
    const d = await r.json();
    if (generation !== batchGeneration || !d.job || !Array.isArray(d.items)
      || !Number.isSafeInteger(d.attempt) || typeof d.finished !== 'boolean') return;
    resetBatchRun();
    generation = batchGeneration;
    batchJobId = d.job;
    batchOcrDone = true;
    batchDocsDone = d.finished;
    renderBatchProgress(d);
    if (d.finished) maybeFinishBatch();
    else {
      $('batch-cancel').classList.remove('hidden');
      pollBatchJob(d.job);
      following = true;
    }
  } catch (e) {
    if (generation === batchGeneration) $('convert-status').textContent = e?.message || batchT('batch.reconnecting');
  } finally {
    clearTimeout(deadline);
    if (generation === batchGeneration && !following) setBatchTriggersBusy(false);
    if ($('batch-history')) $('batch-history').value = batchJobId || '';
  }
}

async function retryBatchDocs() {
  if (!batchJobId || !batchAttempt || isBatchRunning()) return;
  const job = batchJobId;
  const attempt = batchAttempt;
  batchGeneration++;
  batchFinished = false;
  batchDocsDone = false;
  batchCancelRequested = false;
  batchCancelPending = false;
  batchCancelAcknowledged = false;
  setBatchTriggersBusy(true);
  $('batch-retry')?.classList.add('hidden');
  $('batch-cancel').disabled = false;
  $('convert-status').textContent = batchT('batch.preparing');
  try {
    const { response, data } = await postBatch({ retry: job, attempt });
    if (!response.ok && response.status !== 409 && response.status < 500) {
      batchDocsDone = true;
      maybeFinishBatch();
      showToast(batchFailureMessage(data));
      return;
    }
  } catch (_) {
    // Polling will show the current attempt even when the reply was lost.
  }
  $('batch-cancel').classList.remove('hidden');
  pollBatchJob(job);
}

async function sendBatchCancel() {
  if (!batchJobId || !batchCancelRequested || batchCancelPending || batchCancelAcknowledged || batchDocsDone) return;
  const generation = batchGeneration;
  batchCancelPending = true;
  $('batch-cancel').disabled = true;
  try {
    const response = await apiFetch('/api/convert/cancel', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ job: batchJobId }),
    });
    const data = await response.json();
    if (!response.ok || data.ok !== true) throw new Error('cancel_failed');
    if (generation !== batchGeneration || batchFinished) return;
    batchCancelAcknowledged = true;
    $('convert-status').textContent = batchT('task.cancelling');
  } catch (_) {
    if (generation !== batchGeneration || batchFinished) return;
    $('convert-status').textContent = batchT('task.cancelRetry');
    showToast(batchT('task.cancelRetry'));
  } finally {
    if (generation === batchGeneration) {
      batchCancelPending = false;
      $('batch-cancel').disabled = batchCancelAcknowledged;
    }
  }
}

function pollBatchJob(jid, submissionError) {
  stopBatchPoll();
  const generation = batchGeneration;
  let failures = 0;
  let missingRetries = submissionError ? 2 : 0;
  const current = () => generation === batchGeneration && jid === batchJobId && !batchDocsDone;
  const poll = async () => {
    if (!current()) return;
    const controller = new AbortController();
    batchPollAbort = controller;
    const deadline = setTimeout(() => controller.abort(), 15000);
    try {
      const r = await apiFetch('/api/convert/progress?job=' + encodeURIComponent(jid), { signal: controller.signal });
      if (!current()) return;
      if (r.status === 404) {
        if (missingRetries > 0) { missingRetries--; throw new Error(); }
        failBatchDocsLane(submissionError || batchT('batch.jobMissing'));
        return;
      }
      if (!r.ok) {
        const fault = await r.json().catch(() => ({}));
        throw new Error(apiMessage(fault));
      }
      const d = await r.json();
      if (!current()) return;
      if (!Array.isArray(d.items) || typeof d.finished !== 'boolean') throw new Error('invalid_progress');
      failures = 0;
      missingRetries = 0;
      renderBatchProgress(d);
      if (batchCancelRequested) sendBatchCancel();
      if (d.finished) {
        // A malformed final response cannot leave rows indefinitely running.
        for (const row of Object.values(batchRowsBySrc)) {
          if (!row.dataset.done) { setBatchRow(row, 'error', batchT('batch.incompleteResult')); countBatchRow(row, 'error'); }
        }
        batchDocsDone = true;
        maybeFinishBatch();
      }
    } catch (error) {
      if (!current()) return;
      failures++;
      // A transport failure says nothing about files still being written by
      // the kernel. Keep ownership of the job and reconnect with backoff.
      $('convert-status').textContent = error?.message === 'invalid_progress'
        ? batchT('batch.reconnecting') : (error?.message || batchT('batch.reconnecting'));
    } finally {
      clearTimeout(deadline);
      if (batchPollAbort === controller) batchPollAbort = null;
      if (current()) batchJobTimer = setTimeout(poll, Math.min(10000, 600 * (2 ** Math.min(failures, 5))));
    }
  };
  batchJobTimer = setTimeout(poll, 0);
}

function failBatchDocsLane(message) {
  stopBatchPoll();
  for (const src in batchRowsBySrc) {
    const row = batchRowsBySrc[src];
    if (!row.dataset.done) {
      setBatchRow(row, 'error', message || '');
      countBatchRow(row, 'error');
    }
  }
  batchDocsDone = true;
  maybeFinishBatch();
}

function renderBatchProgress(d) {
  if (Number.isSafeInteger(d.attempt) && d.attempt > batchAttempt && typeof d.output_dir === 'string') {
    batchAttempt = d.attempt;
    batchOutputDir = d.output_dir;
    const sources = new Set(d.items.map(it => it.src));
    for (const [source, row] of Object.entries(batchRowsBySrc)) {
      if (!sources.has(source)) {
        row.remove();
        if (row.dataset.result) batchCount[row.dataset.result]--;
        delete batchRowsBySrc[source];
      }
    }
    for (const it of d.items) {
      const previous = batchRowsBySrc[it.src];
      if (previous?.dataset.result === 'ok') continue;
      const row = makeBatchRow(it.src);
      if (previous) {
        previous.replaceWith(row);
        if (previous.dataset.result) batchCount[previous.dataset.result]--;
      } else $('convert-list').appendChild(row);
      batchRowsBySrc[it.src] = row;
    }
  }
  (d.items || []).forEach(it => {
    const row = batchRowsBySrc[it.src];
    if (!row || row.dataset.done) return;
    const status = it.status || 'queued';
    if (it.planned_out) {
      const output = row.querySelector('.batch-output-name');
      if (output) {
        output.textContent = it.planned_out.split(/[\\/]/).pop();
        output.title = it.planned_out;
      }
    }
    if (status !== 'queued') {
      if ((status === 'ok' || status === 'skipped') && it.out) row.dataset.out = it.out;
      setBatchRow(row, status, status === 'error' ? batchFailureMessage(it) : it.error || '');
      if (status === 'ok') window.ReadMDConversionReport?.attach(row, it.fidelity_report);
      countBatchRow(row, status);
    }
  });
  if (d.running) {
    $('convert-status').textContent = batchCancelAcknowledged ? batchT('task.cancelling') : batchT('batch.converting', { done: d.done, total: d.total })
      || '';
  }
}

async function runBatchOcrLane(items, overwrite, generation) {
  if (!(await ensureModule('ocr'))) {
    items.forEach(([, row]) => { setBatchRow(row, 'error'); countBatchRow(row, 'error'); });
    batchOcrDone = true;
    maybeFinishBatch();
    return;
  }
  for (let i = 0; i < items.length; i++) {
    if (generation !== batchGeneration) return;
    const [path, row] = items[i];
    if (batchOcrCanceled && !row.dataset.done) {
      setBatchRow(row, 'canceled');
      countBatchRow(row, 'canceled');
      continue;
    }
    setBatchRow(row, 'running');
    try {
      const r = await apiFetch('/api/ocr?p=' + encodeURIComponent(path) + '&save=1&on_exists=' + (overwrite ? 'overwrite' : 'skip'));
      const d = await r.json().catch(() => ({}));
      if (generation !== batchGeneration) return;
      if (r.ok && d.empty) {
        // 识别成功但没有文字：不写文件，标注“无文字”。
        setBatchRow(row, 'skipped', batchT('batch.ocrNoText') || '');
        const st = row.querySelector('.batch-state');
        if (st) st.textContent = batchT('batch.ocrNoText') || '无文字';
        countBatchRow(row, 'skipped');
      } else if (r.ok && d.content) {
        if (d.out && (d.saved || d.skipped)) row.dataset.out = d.out;
        const status = d.skipped ? 'skipped' : 'ok';
        setBatchRow(row, status);
        if (status === 'ok') window.ReadMDConversionReport?.attach(row, d.fidelity_report);
        countBatchRow(row, status);
      } else {
        const code = d.error_code || 'ocr_failed';
        const msg = batchT('batch.err.' + code);
        setBatchRow(row, 'error', msg && msg !== 'batch.err.' + code ? msg : (d.error || code));
        countBatchRow(row, 'error');
      }
    } catch (e) {
      setBatchRow(row, 'error', e && e.message);
      countBatchRow(row, 'error');
    }
  }
  batchOcrDone = true;
  maybeFinishBatch();
}

/** 最长公共父目录；没有公共部分时取第一个输出所在目录。 */
function commonOutputDir(paths) {
  const dirs = paths.map(p => p.replace(/\\/g, '/').replace(/\/[^/]*$/, ''));
  if (!dirs.length) return '';
  const split = dirs.map(d => d.split('/'));
  const first = split[0];
  let n = first.length;
  for (const s of split.slice(1)) {
    let i = 0;
    while (i < n && i < s.length && s[i].toLowerCase() === first[i].toLowerCase()) i++;
    n = i;
  }
  const common = first.slice(0, n).join('/');
  return common && !/^[A-Za-z]:$/.test(common) ? common : dirs[0];
}

function showBatchOpenDir() {
  const btn = $('convert-open-dir');
  if (!btn) return;
  const outs = Array.from($('convert-list').querySelectorAll('.batch-item'))
    .map(r => r.dataset.out).filter(Boolean);
  if (!batchOutputDir && !outs.length) { btn.classList.add('hidden'); return; }
  const dir = batchOutputDir || commonOutputDir(outs);
  btn.classList.remove('hidden');
  btn.title = dir;
  btn.onclick = async () => {
    try {
      const r = await apiFetch('/api/system/open-path', {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: dir }),
      });
      const d = await r.json().catch(() => ({}));
      if (!r.ok || d.ok === false) {
        showToast(d.error_code === 'path_not_found'
          ? (batchT('toast.pathNotFound') || '路径不存在')
          : (batchT('toast.openFailed') || '无法打开：') + dir);
      }
    } catch (e) { showToast((batchT('toast.openFailed') || '无法打开：') + dir); }
  };
}

/** True from enqueue until both lanes have finished. */
let batchActive = false;
function isBatchRunning() {
  return batchActive;
}

/** The pickers stay disabled while a batch is running (no parallel jobs). */
function setBatchTriggersBusy(on) {
  batchActive = on;
  ['convert-files', 'convert-folder', 'convert-speech-language', 'convert-overwrite', 'convert-output-choose', 'convert-output-reset', 'batch-history'].forEach(id => {
    const el = $(id);
    if (!el) return;
    el.disabled = on;
    if (on) el.setAttribute('aria-busy', 'true'); else el.removeAttribute('aria-busy');
  });
}

function maybeFinishBatch() {
  if (batchFinished || !batchDocsDone || !batchOcrDone) return;
  batchFinished = true;
  setBatchTriggersBusy(false);
  showBatchOpenDir();
  const c = batchCount;
  let text = batchT('batch.summary', { ok: c.ok, skipped: c.skipped, failed: c.error })
    || '';
  if (c.canceled) {
    text += ' · ' + (batchT('batch.summaryCanceled', { canceled: c.canceled }) || '');
  }
  $('convert-status').textContent = text;
  $('batch-cancel').classList.add('hidden');
  const retry = batchAttempt > 0 && Object.values(batchRowsBySrc)
    .some(row => ['error', 'canceled'].includes(row.dataset.result));
  $('batch-retry')?.classList.toggle('hidden', !retry);
  if (batchAttempt) void refreshBatchHistory();
}

function onBatchCancel() {
  batchOcrCanceled = true;
  batchCancelRequested = true;
  sendBatchCancel();
}
