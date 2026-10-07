'use strict';
/* ============================================================
   ReadMD Reader - Document Fixes Modal
   ============================================================ */

/* ---------------- 修正详情 ---------------- */

let documentInspectionGeneration = 0;
function showFixModal() {
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  const list = $('fix-list');
  if (list) list.innerHTML = '';
  const fixes = state.fixes || [];
  const fixCount = $('fix-count');
  if (fixCount) {
    fixCount.textContent = fixes.length ? (_t('fixes.countTotal', { count: fixes.length }) || ('（共 ' + fixes.length + ' 处）')) : '';
  }
  if (!fixes.length) {
    const li = document.createElement('li');
    li.className = 'empty';
    li.textContent = _t('fixes.noFixes') || '本篇文档未发现需要修正的内容';
    if (list) list.appendChild(li);
  } else {
    fixes.forEach(f => {
      const li = document.createElement('li');
      li.textContent = f;
      if (list) list.appendChild(li);
    });
  }
  const modal = $('fix-modal');
  if (modal) modal.classList.remove('hidden');
  void inspectCurrentDocument(++documentInspectionGeneration);
}

async function inspectCurrentDocument(generation) {
  const text = state.editing ? getEditContent() : (state.original ?? state.fixed ?? '');
  const tabId = state.activeTabId;
  const file = state.file && !state.virtualSource ? state.file : '';
  const list = $('fix-list');
  const status = document.createElement('li');
  status.className = 'document-inspection-summary';
  status.textContent = i18n.t('inspection.loading');
  list?.appendChild(status);
  try {
    const response = await apiFetch('/api/document/analyze', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ content: text, ...(file && /^(?:[A-Za-z]:[\\/]|\/)/.test(file) ? { file_path: file } : {}) })
    });
    const result = await response.json();
    if (generation !== documentInspectionGeneration || tabId !== state.activeTabId || $('fix-modal').classList.contains('hidden')) return;
    if (text !== (state.editing ? getEditContent() : (state.original ?? state.fixed ?? ''))) {
      status.textContent = i18n.t('inspection.changed'); return;
    }
    if (!response.ok || !result.ok) throw new Error('inspection_failed');
    status.textContent = i18n.t('inspection.summary', { count: result.diagnostics.length });
    for (const issue of result.diagnostics) {
      const row = document.createElement('li');
      row.className = 'document-inspection-issue';
      const key = 'inspection.' + issue.code;
      row.textContent = i18n.t('inspection.atLine', { line: issue.position.line }) + ' · ' + i18n.t(key) + ' · ' + issue.target;
      list?.appendChild(row);
    }
    if (result.truncated) {
      const row = document.createElement('li'); row.textContent = i18n.t('inspection.limited'); list?.appendChild(row);
    }
  } catch (_) {
    if (generation === documentInspectionGeneration && tabId === state.activeTabId && status.isConnected) status.textContent = i18n.t('inspection.failed');
  }
}

async function handleAiDocumentFix() {
  return window.ReadMDTask.run('document-ai-fix', runAiDocumentFix, { trigger: 'fix-ai-btn' });
}

async function runAiDocumentFix() {
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  const rawContent = state.editing ? getEditContent() : (state.fixed ?? state.original ?? '');
  const origin = { tabId: state.activeTabId, editor: window.cmView, name: state.sourceName || state.file, path: state.file, dir: state.dir };
  if (!rawContent || !rawContent.trim()) {
    showToast(_t('fixes.noDocContent') || '当前没有可修复的文档内容');
    return;
  }

  try {
    const connection = typeof ensureAiConfigured === 'function'
      ? await ensureAiConfigured()
      : (typeof resolveSharedAiConnection === 'function' ? await resolveSharedAiConnection() : null);
    if (!connection) return;
    const fixModal = $('fix-modal');
    if (fixModal) fixModal.classList.add('hidden');
    showToast(_t('fixes.aiFixing') || '正在进行 AI 深度格式排版自愈...', 2500);
    const resp = await apiFetch('/api/ai/chat', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        provider: connection.provider,
        credential_id: connection.credential_id,
        model: connection.model,
        base_url: connection.base_url,
        mode: connection.mode,
        endpoint_mode: connection.endpoint_mode,
        headers: connection.headers,
        skill_id: 'readmd-format-fix',
        skill_variables: {
          document: rawContent,
          selection: rawContent,
          request: '',
          language: (window.i18n && window.i18n.locale) || document.documentElement.lang || 'en',
          context: '',
          output_format: 'Markdown'
        },
        messages: [{ role: 'user', content: rawContent }],
        stream: false
      })
    });

    const data = await resp.json();
    if (!resp.ok || !data || !data.ok || typeof data.content !== 'string' || !data.content.trim()) {
      showToast((_t('fixes.aiFixFail') || 'AI 修复失败：') + ((data && data.error) || '未返回有效内容'));
      return;
    }

    let fixedMd = data.content.trim();
    // 剥离可能存在的 markdown 代码块包裹
    if (fixedMd.startsWith('```markdown') && fixedMd.endsWith('```')) {
      fixedMd = fixedMd.slice(11, -3).trim();
    } else if (fixedMd.startsWith('```md') && fixedMd.endsWith('```')) {
      fixedMd = fixedMd.slice(5, -3).trim();
    } else if (fixedMd.startsWith('```') && fixedMd.endsWith('```') && !rawContent.startsWith('```')) {
      fixedMd = fixedMd.slice(3, -3).trim();
    }

    if (!fixedMd) throw new Error(_t('audit.emptyAiResult'));
    const unchangedEditor = origin.editor && state.activeTabId === origin.tabId && state.editing && window.cmView === origin.editor &&
      window.cmView.state.doc.toString() === rawContent;
    if (unchangedEditor) {
      if (!await window.ReadMDRecovery?.checkpoint('ai_repair')) return;
      if (state.activeTabId !== origin.tabId || window.cmView !== origin.editor || getEditContent() !== rawContent) {
        await renderVirtual('ai', getNextAiCopyTabName(origin.name), origin.dir || '', fixedMd, [], { originPath: origin.path });
        return;
      }
      window.cmView.dispatch({
        annotations: window.ReadMDCodeMirror.Transaction.userEvent.of('ai.repair'),
        changes: { from: 0, to: window.cmView.state.doc.length, insert: fixedMd }
      });
      if (typeof syncActiveTabDirty === 'function') syncActiveTabDirty();
      if (typeof updateEditorPreview === 'function') updateEditorPreview();
    } else {
      await renderVirtual('ai', getNextAiCopyTabName(origin.name), origin.dir || '', fixedMd, [], { originPath: origin.path });
      showToast(_t('toast.appliedVirtualNotice') || '已创建副本标签页');
      return;
    }

    showToast(_t('fixes.aiFixed') || 'AI 深度排版修复完成', 1800);
  } catch (err) {
    showToast((_t('fixes.aiFixFail') || 'AI 修复失败：') + err.message);
  }
}

document.addEventListener('DOMContentLoaded', () => {
  const aiFixBtn = $('fix-ai-btn');
  if (aiFixBtn) {
    aiFixBtn.addEventListener('click', handleAiDocumentFix);
  }
});
