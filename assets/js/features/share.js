'use strict';
/* ============================================================
   ReadMD Features - Mobile LAN Sharing
   ============================================================ */

/* ---------------- 移动端共享 ---------------- */

let qrLibraryLoader;
let shareStatusEpoch = 0;
let shareCurrentUrl = '';
const shareText = (key, params) => window.i18n ? window.i18n.t(key, params) : key;

function shareBusy(busy) {
  $('share-modal').setAttribute('aria-busy', String(busy));
  $('share-copy').disabled = busy || !shareCurrentUrl;
  if (busy) ['share-start', 'share-stop', 'share-refresh'].forEach(id => { $(id).disabled = true; });
}

function shareError(error) {
  shareCurrentUrl = '';
  $('share-copy').classList.add('hidden');
  $('share-url').textContent = '';
  $('share-token').textContent = '';
  $('share-qr').textContent = shareText('audit.shareFailed', { error: error.message });
  $('share-start').disabled = true;
  $('share-stop').disabled = true;
}

async function shareResponse(response) {
  const data = await response.json();
  if (!response.ok || !data || data.ok === false || data.error) throw new Error(data?.error || data?.error_code || 'HTTP ' + response.status);
  return data;
}

function loadQrLibrary() {
  if (typeof qrcode === 'function') return Promise.resolve();
  qrLibraryLoader ||= new Promise((resolve, reject) => {
    const script = document.createElement('script');
    script.src = '/assets/vendor/qrcode.min.js';
    script.onload = resolve;
    script.onerror = () => reject(new Error('QR library failed to load'));
    document.head.appendChild(script);
  }).catch(error => {
    qrLibraryLoader = null;
    throw error;
  });
  return qrLibraryLoader;
}

async function openShareModal() {
  $('share-modal').classList.remove('hidden');
  return refreshShareStatus();
}

async function refreshShareStatus() {
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  const epoch = ++shareStatusEpoch;
  shareBusy(true);
  try {
    const r = await apiFetch('/api/share/status');
    const d = await shareResponse(r);
    if (epoch !== shareStatusEpoch) return false;
    if (typeof d.running !== 'boolean' || (d.running && (typeof d.url !== 'string' || !d.url))) throw new Error(_t('audit.invalidResponse'));
    if (d.running) {
      const authenticated = new URL(d.url);
      if (!['http:', 'https:'].includes(authenticated.protocol) || typeof d.token !== 'string' || !d.token) throw new Error(_t('audit.invalidResponse'));
      authenticated.searchParams.set('token', d.token);
      const url = authenticated.href;
      shareCurrentUrl = url;
      $('share-copy').classList.remove('hidden');
      $('share-start').disabled = true;
      $('share-stop').disabled = false;
      $('share-url').textContent = (_t('share.mobileUrlLabel') || '手机浏览器打开：') + url;
      $('share-token').textContent = (_t('share.tokenLabel') || '访问令牌：') + d.token;
      await renderQr(url, epoch);
    } else {
      shareCurrentUrl = '';
      $('share-copy').classList.add('hidden');
      $('share-start').disabled = false;
      $('share-stop').disabled = true;
      $('share-url').textContent = '';
      $('share-token').textContent = '';
      const q = $('share-qr');
      q.textContent = _t('share.notRunning') || '尚未开启共享';
    }
    return true;
  } catch (e) {
    if (epoch === shareStatusEpoch) shareError(e);
    return false;
  } finally {
    if (epoch === shareStatusEpoch) {
      shareBusy(false);
      $('share-refresh').disabled = false;
    }
  }
}

async function renderQr(text, epoch = shareStatusEpoch) {
  const box = $('share-qr');
  box.innerHTML = '';
  try {
    await loadQrLibrary();
    if (epoch !== shareStatusEpoch) return;
    const qr = qrcode(0, 'M');
    qr.addData(text);
    qr.make();
    box.innerHTML = qr.createImgTag(6, 10);
  } catch (e) {
    if (epoch === shareStatusEpoch) box.textContent = text;
  }
}

async function startShare() {
  return window.ReadMDTask.run('share-change', () => changeShare(true), { trigger: ['share-start', 'share-stop', 'share-refresh'] });
}

async function copyShareLink() {
  if (shareCurrentUrl) await copyText(shareCurrentUrl);
}

async function stopShare() {
  return window.ReadMDTask.run('share-change', () => changeShare(false), { trigger: ['share-start', 'share-stop', 'share-refresh'] });
}

async function changeShare(start) {
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  ++shareStatusEpoch;
  shareBusy(true);
  try {
    const r = await apiFetch(start ? '/api/share/start' : '/api/share/stop', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ current_file: state.file || null,
        current_content: (!state.file || (state.editing && hasUnsavedEditorChanges()))
          ? (state.editing ? getEditContent() : (state.fixed ?? state.original ?? '')) : null,
        language: window.i18n?.locale || 'en', theme: document.body.dataset.theme,
        labels: { title: _t('share.title'), up: _t('audit.shareParent'), download: _t('audit.shareDownload') } }),
    });
    const data = await shareResponse(r);
    if (data.ok !== true || data.running !== start) throw new Error(_t('audit.invalidResponse'));
    showToast(_t(start ? 'toast.shareStarted' : 'toast.shareStopped'));
    return true;
  } catch (e) {
    showToast(_t(start ? 'toast.shareStartFail' : 'toast.shareStopFail') + e.message);
    return false;
  } finally {
    // Run after the task runner restores its trigger buttons.
    setTimeout(() => refreshShareStatus(), 0);
  }
}

