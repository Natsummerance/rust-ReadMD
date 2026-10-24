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
/** 文件夹批量时所选的根目录；“打开结果目录”优先用它。 */
let batchFolderRoot = '';
const batchRowsBySrc = Object.create(null);

function batchT(k, p) {
  return window.i18n ? window.i18n.t(k, p) : k;
}

function batchFailureMessage(item) {
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
  if (isBatchRunning()) {
    $('convert-modal')?.classList.remove('hidden');
    return;
  }
  if (typeof initSpeechLanguage === 'function') initSpeechLanguage();
  stopBatchPoll();
  batchGeneration++;
  setBatchTriggersBusy(false);
  batchJobId = null;
  batchCancelRequested = false;
  batchCancelPending = false;
  batchCancelAcknowledged = false;
  batchOcrCanceled = false;
  batchFinished = false;
  batchCount = { ok: 0, skipped: 0, error: 0, canceled: 0 };
  batchFolderRoot = '';
  for (const k in batchRowsBySrc) delete batchRowsBySrc[k];
  const modal = $('convert-modal');
  if (modal) modal.classList.remove('hidden');
  // Batch and single-file conversion share one surface; never leak the
  // single-file "open output" action into a fresh batch run.
  $('convert-open-dir')?.classList.add('hidden');
  $('convert-list').innerHTML = '';
  $('convert-status').textContent = '';
  const note = $('convert-note');
  if (note) note.textContent = batchT('batch.note') || '';
  $('batch-cancel').classList.add('hidden');
  $('batch-cancel').disabled = false;
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
  if (status in batchCount) batchCount[status]++;
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
  if (docs.length) runBatchDocsLane(docs, overwrite, generation);
  if (images.length) runBatchOcrLane(images, overwrite, generation);
}

async function runBatchDocsLane(paths, overwrite, generation) {
  try {
    if (!(await ensureModule('convert'))) throw new Error('convert_module_unavailable');
    const r = await apiFetch('/api/convert/batch', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ paths, overwrite: !!overwrite, confirm: true, language: typeof currentSpeechLanguage === 'function' ? currentSpeechLanguage() : 'auto' }),
    });
    const d = await r.json().catch(() => ({}));
    if (generation !== batchGeneration) return;
    if (!r.ok || typeof d.job !== 'string' || !d.job) throw new Error(d.error_code || ('http_' + r.status));
    batchJobId = d.job;
    if (batchCancelRequested) sendBatchCancel();
    pollBatchJob(d.job);
  } catch (e) {
    if (generation !== batchGeneration) return;
    failBatchDocsLane(e && e.message);
  }
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

function pollBatchJob(jid) {
  stopBatchPoll();
  const generation = batchGeneration;
  let failures = 0;
  const current = () => generation === batchGeneration && jid === batchJobId && !batchDocsDone;
  const poll = async () => {
    if (!current()) return;
    const controller = new AbortController();
    batchPollAbort = controller;
    const deadline = setTimeout(() => controller.abort(), 15000);
    try {
      const r = await apiFetch('/api/convert/progress?job=' + encodeURIComponent(jid), { signal: controller.signal });
      if (!current()) return;
      if (r.status === 404) { failBatchDocsLane(batchT('batch.jobMissing')); return; }
      if (!r.ok) throw new Error('HTTP ' + r.status);
      const d = await r.json();
      if (!current()) return;
      if (!Array.isArray(d.items) || typeof d.finished !== 'boolean') throw new Error('invalid_progress');
      failures = 0;
      renderBatchProgress(d);
      if (d.finished) {
        // A malformed final response cannot leave rows indefinitely running.
        for (const row of Object.values(batchRowsBySrc)) {
          if (!row.dataset.done) { setBatchRow(row, 'error', batchT('batch.incompleteResult')); countBatchRow(row, 'error'); }
        }
        batchDocsDone = true;
        maybeFinishBatch();
      }
    } catch (_) {
      if (!current()) return;
      failures++;
      // A transport failure says nothing about files still being written by
      // the kernel. Keep ownership of the job and reconnect with backoff.
      $('convert-status').textContent = batchT('batch.reconnecting');
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
  (d.items || []).forEach(it => {
    const row = batchRowsBySrc[it.src];
    if (!row || row.dataset.done) return;
    const status = it.status || 'queued';
    if (status !== 'queued') {
      if ((status === 'ok' || status === 'skipped') && it.out) row.dataset.out = it.out;
      setBatchRow(row, status, status === 'error' ? batchFailureMessage(it) : it.error || '');
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
  if (!outs.length) { btn.classList.add('hidden'); return; }
  const dir = (typeof batchFolderRoot === 'string' && batchFolderRoot) || commonOutputDir(outs);
  btn.classList.remove('hidden');
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
  ['convert-files', 'convert-folder', 'convert-speech-language', 'convert-overwrite'].forEach(id => {
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
}

function onBatchCancel() {
  batchOcrCanceled = true;
  batchCancelRequested = true;
  sendBatchCancel();
}
