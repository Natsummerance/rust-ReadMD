'use strict';
/* ============================================================
   ReadMD Features - In-App Auto Update System
   ============================================================ */

let updateInfo = null;
let updateTimer = null;
let isUpdating = false;
let upgradeUrl = null;
let updatePollPending = false;
let updateStatusFailures = 0;
let updateReadyFile = null;
let updateJobEpoch = 0;

let updateCheckPromise = null;
let updateRetryTimer = null;
let updateCheckFailures = 0;
function scheduleUpdateCheck(success) {
  clearTimeout(updateRetryTimer);
  updateCheckFailures = success ? 0 : Math.min(3, updateCheckFailures + 1);
  const delay = success ? 6 * 60 * 60 * 1000 : [30000, 120000, 600000][updateCheckFailures - 1];
  updateRetryTimer = setTimeout(() => checkUpdate(true), delay);
}
function checkUpdate(silent = true) {
  if (window.__STARTUP_PROBE__ || isUpdating) return Promise.resolve();
  if (updateCheckPromise) return updateCheckPromise.then(result => {
    if (!silent) {
      if (result?.has_update) openUpdateModal();
      else if (!result?.ok) showToast(window.i18n.t(result?.error_code && result.error_code !== 'update_network_error' ? 'update.checkFail' : 'update.networkRetry'));
      else showToast(window.i18n.t('update.latest', { ver:result.current_version, version:result.current_version }));
    }
    return result;
  });
  const button = $('btn-check-update');
  if (button) { button.disabled = true; button.setAttribute('aria-busy', 'true'); }
  updateCheckPromise = checkUpdateOnce(silent).finally(() => {
    updateCheckPromise = null;
    if (button) { button.disabled = false; button.removeAttribute('aria-busy'); }
  });
  return updateCheckPromise;
}
window.addEventListener('online', () => { clearTimeout(updateRetryTimer); updateRetryTimer = setTimeout(() => checkUpdate(true), 500); });
async function checkUpdateOnce(silent = true) {
  if (window.__STARTUP_PROBE__) return;
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  try {
    if (navigator.onLine === false) { scheduleUpdateCheck(false); if (!silent) showToast(_t('update.networkRetry')); return {ok:false,error_code:'update_network_error'}; }
    let res = null;
    if (hasPy && py.check_update) {
      res = await py.check_update();
    } else {
      const resp = await fetch('/api/update/check');
      res = await resp.json().catch(() => null);
    }
    if (!res || !res.ok) {
      scheduleUpdateCheck(false);
      if (!silent) {
        let msg = _t('update.checkFail');
        if (res && res.error) {
          msg = _t('update.checkFail') + '：' + res.error;
        } else if (res && res.error_code === 'update_network_error') {
          msg = _t('update.networkRetry');
        }
        showToast(msg);
      }
      return res;
    }
    scheduleUpdateCheck(true);
    if (res && res.current_version) {
      if ($('status-version')) $('status-version').textContent = 'v' + res.current_version;
      if ($('menu-version-label')) $('menu-version-label').textContent = _t('update.currentVer', { ver: res.current_version }) || ('当前版本 v' + res.current_version);
    }
    if (res.has_update) {
      updateInfo = res;
      upgradeUrl = res.html_url;
      if ($('status-update-badge')) {
        $('status-update-badge').classList.remove('hidden');
        if ($('update-badge-ver')) $('update-badge-ver').textContent = res.latest_version;
      }
      if ($('update-menu-dot')) $('update-menu-dot').classList.remove('hidden');
      if (!silent) {
        openUpdateModal();
      } else {
        showToast(_t('update.foundNew', { ver: res.latest_version }) || ('发现新版本 ' + res.latest_version), 5000);
      }
    } else {
      updateInfo = null; upgradeUrl = null;
      $('status-update-badge')?.classList.add('hidden');
      $('update-menu-dot')?.classList.add('hidden');
      const curVer = res.current_version || (typeof VERSION !== 'undefined' ? VERSION : (document.documentElement.getAttribute('data-version') || ''));
      if (!silent) showToast(_t('update.latest', { ver: curVer, version: curVer }) || ('当前已是最新版本 (v' + curVer + ')'));
    }
    return res;
  } catch (e) {
    scheduleUpdateCheck(false);
    if (!silent) showToast(_t('update.networkRetry'));
    return {ok:false,error_code:'update_network_error'};
  }
}


function openUpdateModal() {
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  if (!updateInfo) {
    checkUpdate(false);
    return;
  }
  $('update-modal').classList.remove('hidden');
  $('update-new-ver').textContent = updateInfo.latest_version || '';
  $('update-pub-time').textContent = updateInfo.published_at ? new Date(updateInfo.published_at).toLocaleDateString() : '';

  const notesEl = $('update-notes-content');
  if (updateInfo.release_notes) {
    // Release notes cross a trust boundary: fail closed if the shared renderer is unavailable.
    notesEl.innerHTML = typeof window.renderSafeMarkdown === 'function'
      ? window.renderSafeMarkdown(updateInfo.release_notes)
      : '';
  } else {
    notesEl.textContent = _t('update.noNotes') || '暂无详细更新说明。';
  }

  if (updateInfo.asset) {
    $('update-asset-name').textContent = updateInfo.asset.name || (_t('update.package') || '安装包');
    const mb = updateInfo.asset.size ? (updateInfo.asset.size / (1024 * 1024)).toFixed(1) + ' MB' : '';

    $('update-asset-size').textContent = mb;
    const verifiable = !!updateInfo.asset.expected_sha;
    $('btn-update-start').disabled = !verifiable || isUpdating;
    $('btn-update-start').textContent = updateReadyFile ? _t('audit.updateInstallRetry') : verifiable
      ? (_t('update.installNow') || '立即下载并更新')
      : (_t('update.unverifiedPackage') || '无法验证更新包');
  } else {
    $('update-asset-name').textContent = _t('update.noAsset') || '未找到匹配当前系统的二进制资产';
    $('update-asset-size').textContent = '';
    $('btn-update-start').disabled = true;
    $('btn-update-start').textContent = _t('update.noAsset') || '暂无对应安装包';
  }
}


function isUpdateDownloading() {
  return isUpdating === true;
}

// Esc must not dismiss the dialog while a download is running.
if (window.ReadMDModal) window.ReadMDModal.setGuard('update-modal', () => { if (isUpdating && updateStatusFailures < 3) { closeUpdateModal(); return false; } return true; });

function closeUpdateModal() {
  if (isUpdating && updateStatusFailures < 3) {
    const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
    showToast(_t('update.closeBlockedDownloading') || '更新正在下载，请先取消下载或等待完成');
    return;
  }
  $('update-modal').classList.add('hidden');
}

async function startUpdateDownload() {
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  if (!updateInfo || !updateInfo.asset || isUpdating) return;
  if (updateReadyFile) return applyReadyUpdate();
  const epoch = ++updateJobEpoch;
  updateStatusFailures = 0;
  updatePollPending = false;
  const asset = updateInfo.asset;
  const useMirror = $('update-use-mirror') && $('update-use-mirror').checked;
  if (!asset.expected_sha) {
    showToast(_t('update.unverifiedPackage') || '无法验证更新包');
    return;
  }

  $('btn-update-start').disabled = true;
  $('btn-update-cancel').classList.remove('hidden');
  $('update-progress-wrap').classList.remove('hidden');
  $('update-progress-fill').style.width = '0%';
  $('update-progress-text').textContent = _t('update.prepDownload') || '准备下载…';
  $('update-progress-speed').textContent = '';
  isUpdating = true;

  try {
    let started = false;
    let startResult = null;
    if (hasPy && py.start_download_update) {
      startResult = await py.start_download_update(asset.download_url, asset.name, asset.expected_sha, useMirror);
      started = startResult && startResult.ok;
    } else {
      const resp = await fetch('/api/update/download', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          download_url: asset.download_url,
          target_filename: asset.name,
          expected_sha: asset.expected_sha,
          use_mirror: useMirror,
        }),
      });
      startResult = await resp.json();
      if (!resp.ok) throw new Error(startResult?.error || 'HTTP ' + resp.status);
      started = startResult && startResult.ok;
    }

    if (!started) {
      const reason = (startResult && startResult.error) || (_t('toast.unknownNetworkErr') || '未知网络错误');
      showToast((_t('toast.updateStartFail') || '启动下载失败') + '：' + reason);
      isUpdating = false;
      $('update-progress-wrap').classList.add('hidden');
      $('btn-update-cancel').classList.add('hidden');
      $('btn-update-start').disabled = false;
      return;
    }

    if (updateTimer) clearInterval(updateTimer);
    updateTimer = setInterval(async () => {
      if (updatePollPending || epoch !== updateJobEpoch) return;
      updatePollPending = true;
      try {
      let st = null;
      if (hasPy && py.get_download_status) {
        st = await py.get_download_status();
      } else {
        const resp = await fetch('/api/update/status');
        st = await resp.json();
        if (!resp.ok) throw new Error(st?.error || 'HTTP ' + resp.status);
      }
      if (epoch !== updateJobEpoch) return;
      if (!st || st.ok === false || !['downloading', 'verifying', 'ready', 'error', 'cancelled', 'idle'].includes(st.status)) throw new Error(st?.error || _t('audit.invalidResponse'));
      updateStatusFailures = 0;

      if (st.status === 'downloading') {
        const pct = st.percent || 0;
        $('update-progress-bar').setAttribute('aria-valuenow', String(pct));
        $('update-progress-fill').style.width = pct + '%';
        const speedMb = ((st.speed_bps || 0) / (1024 * 1024)).toFixed(1);
        const curMb = ((st.downloaded_bytes || 0) / (1024 * 1024)).toFixed(1);
        const totMb = ((st.total_bytes || 0) / (1024 * 1024)).toFixed(1);
        const dlLabel = _t('update.downloading') || '正在下载…';
        $('update-progress-text').textContent = `${dlLabel} ${pct}% (${curMb}MB / ${totMb}MB)`;
        $('update-progress-speed').textContent = `${speedMb} MB/s`;
      } else if (st.status === 'verifying') {
        $('update-progress-bar').setAttribute('aria-valuenow', '100');
        $('update-progress-fill').style.width = '100%';
        $('update-progress-text').textContent = _t('update.verifyingChecksum') || '正在校验文件完整性 (SHA256)…';
      } else if (st.status === 'ready') {
        if (typeof st.target_file !== 'string' || !st.target_file.trim()) throw new Error(_t('audit.invalidResponse'));
        clearInterval(updateTimer);
        updateTimer = null;
        isUpdating = false;
        updateReadyFile = st.target_file;
        $('update-progress-text').textContent = _t('update.downloadDone') || '下载校验完成！正在准备安装…';
        $('btn-update-start').textContent = _t('update.restarting') || '正在重启并安装…';
        setTimeout(() => { if (epoch === updateJobEpoch) applyReadyUpdate(); }, 800);
      } else if (st.status === 'error') {
        clearInterval(updateTimer);
        updateTimer = null;
        isUpdating = false;
        $('update-progress-text').textContent = (_t('update.downloadFailPrefix') || '下载失败：') + (st.error || _t('update.networkRetry'));
        $('btn-update-start').disabled = false;
        $('btn-update-start').textContent = _t('update.retryDownload') || '重试下载';
        $('btn-update-cancel').classList.add('hidden');
      } else if (st.status === 'cancelled') {
        clearInterval(updateTimer);
        updateTimer = null;
        isUpdating = false;
        $('update-progress-text').textContent = _t('update.downloadCancelled') || '下载已取消';
        $('btn-update-start').disabled = false;
        $('btn-update-start').textContent = _t('update.redownload') || '重新下载';
        $('btn-update-cancel').classList.add('hidden');
      }
      } catch (error) {
        if (epoch !== updateJobEpoch) return;
        updateStatusFailures++;
        $('update-progress-text').textContent = _t('audit.updateStatusFailed', { error: error.message });
      } finally {
        if (epoch === updateJobEpoch) updatePollPending = false;
      }
    }, 400);

  } catch (e) {
    showToast((_t('toast.downloadError') || '下载出错：') + e.message);
    isUpdating = false;
    $('update-progress-wrap').classList.add('hidden');
    $('btn-update-cancel').classList.add('hidden');
    $('btn-update-start').disabled = false;
  }
}

async function cancelUpdateDownload() {
  return window.ReadMDTask.run('update-cancel', async () => {
  try {
  let result;
  if (hasPy && py.cancel_download) {
    result = await py.cancel_download();
  } else {
    const response = await apiFetch('/api/update/cancel', { method: 'POST' });
    result = await response.json();
    if (!response.ok) throw new Error(result?.error || 'HTTP ' + response.status);
  }
  if (result === false || result?.ok === false) throw new Error(result?.error || window.i18n.t('audit.invalidResponse'));
  return true;
  } catch (error) { showToast(window.i18n.t('audit.updateCancelFailed', { error: error.message })); return false; }
  }, { trigger: 'btn-update-cancel' });
}

async function applyReadyUpdate() {
  if (!updateReadyFile || isUpdating) return;
  isUpdating = true;
  const _t = (key, params) => window.i18n.t(key, params);
  $('btn-update-start').disabled = true;
  $('btn-update-cancel').classList.add('hidden');
  try {
    let result;
    if (window.ReadMDRecovery && !await window.ReadMDRecovery.prepareInstall()) {
      $('update-progress-text').textContent = _t('update.keepDrafts');
      $('btn-update-start').disabled = false;
      $('btn-update-start').textContent = _t('audit.updateInstallRetry');
      return;
    }
    if (hasPy && py.apply_update) result = await py.apply_update(updateReadyFile, updateInfo.flavor);
    else {
      const response = await apiFetch('/api/update/apply', { method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ file_path: updateReadyFile, flavor: updateInfo.flavor }) });
      result = await response.json();
      if (!response.ok) throw new Error(result?.error || 'HTTP ' + response.status);
    }
    if (result !== true && result?.ok !== true) throw new Error(result?.error || _t('audit.invalidResponse'));
    updateReadyFile = null;
    if (result?.quit_required && hasPy) py.request_quit();
  } catch (error) {
    $('update-progress-text').textContent = _t('audit.updateApplyFailed', { error: error.message });
    $('btn-update-start').textContent = _t('audit.updateInstallRetry');
    $('btn-update-start').disabled = false;
    showToast(_t('audit.updateApplyFailed', { error: error.message }));
  } finally { isUpdating = false; }
}

