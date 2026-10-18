'use strict';
/* ============================================================
   ReadMD Core - Module Management & Lifecycle
   ============================================================ */

/* ---------------- 模块懒加载 ---------------- */

function startModules() {
  if (state.modulesStarted) return;
  state.modulesStarted = true;
  if (hasPy) { try { py.start_modules(); } catch (e) { /* ignore */ } }
  pollModules();
}

async function pollModules() {
  try {
    const r = await apiFetch('/api/modules');
    const d = await r.json();
    state.modules = d.modules || {};
    state.win7 = !!d.win7;
    updateModuleUi();
    const pending = Object.values(state.modules).some(v => v === 'loading' || v === 'idle');
    if (pending && state.modulesStarted) setTimeout(pollModules, 900);
  } catch (e) {
    if (state.modulesStarted) setTimeout(pollModules, 2000);
  }
}

function moduleLabel(name) {
  const key={convert:'menu.convert',ocr:'menu.ocr',web:'menu.web',ai:'toolbar.ai',transcribe:'plugin.capability.audio',pdf_editor:'pdf.title'}[name];
  return key && window.i18n ? window.i18n.t(key) : '';
}

function updateModuleUi() {
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  const m = state.modules;
  const ready = n => m[n] === 'ready';
  const disabled = n => m[n] === 'disabled';
  [['btn-convert', 'convert'], ['btn-web', 'web'], ['btn-ocr', 'ocr'], ['btn-ai', 'ai']].forEach(([id, key]) => {
    const el = $(id);
    if (!el) return;
    el.disabled = false;
    if (disabled(key)) {
      el.title = _t('status.win7Unsupported') || 'Win7 版暂不支持该功能';
    }
  });
  ['w-convert', 'w-web', 'w-ocr', 'w-ai'].forEach(id => {
    const el = $(id);
    if (el) el.disabled = false;
  });
  if (ready('ai') && !state.ai.config) loadAiConfig();
  const parts = [];
  for (const [k, v] of Object.entries(m)) {
    const label=moduleLabel(k);if(!label)continue;
    const status=v==='ready'?'status.ready':v==='error'||v==='unavailable'?'plugin.native.unavailable':v==='disabled'?'status.win7UnsupportedShort':'status.loading';
    parts.push(label+' · '+_t(status));
  }
  const el = $('status-mods');
  if (el) {
    const details=parts.length?_t('status.modulePrefix')+'\n'+parts.join('\n'):'';
    el.textContent=details?'●':'';el.title=details;el.setAttribute('role','img');el.setAttribute('aria-label',details);
    el.dataset.moduleState=Object.values(m).some(v=>['error','unavailable','disabled'].includes(v))?'unavailable':Object.values(m).some(v=>v!=='ready')?'loading':'ready';
  }
}
window.addEventListener('readmd:language-changed',updateModuleUi);

function moduleBlocked(name) {
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  if (state.modules[name] === 'disabled') {
    showToast(_t('toast.win7Blocked') || '该功能在 Win7 版暂不支持（本版本仅保留 docx / pdf 转 MD 与导出功能）', 3400);
    return true;
  }
  return false;
}

async function ensureModule(name, timeoutMs) {
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  const t0 = Date.now();
  const limit = timeoutMs || 60000;
  if (!moduleLoadRequests[name]) {
    moduleLoadRequests[name] = apiFetch('/api/modules/load', {
      method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name, retry: true }),
    }).then(async r => {
      const d = await r.json();
      if (!r.ok || d.status === 'error' || d.status === 'unavailable') throw Error(d.error_code || 'module_unavailable');
      return d;
    });
  }
  try { await moduleLoadRequests[name]; }
  catch (_) { delete moduleLoadRequests[name]; showToast(_t('toast.moduleLoadFail', { name:moduleLabel(name) })); return false; }
  while (Date.now() - t0 < limit) {
    try {
      const r = await apiFetch('/api/modules');
      const d = await r.json();
      const st = d.modules && d.modules[name];
      if (st === 'ready') return true;
      if (st === 'error' || st === 'unavailable' || st === 'disabled') {
        delete moduleLoadRequests[name];
        showToast(_t('toast.moduleLoadFail', { name:moduleLabel(name) }));
        return false;
      }
    } catch (e) { /* ignore */ }
    await new Promise(r => setTimeout(r, 800));
  }
  delete moduleLoadRequests[name];
  showToast(_t('toast.moduleTimeout') || '模块加载超时，请重试');
  return false;
}

