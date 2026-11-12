'use strict';

const TREE_FOLDER_ICON = '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 20h16a1 1 0 0 0 1-1V8a1 1 0 0 0-1-1h-8L9 5H4a1 1 0 0 0-1 1v13a1 1 0 0 0 1 1Z"/></svg>';
/* ============================================================
   ReadMD Reader - Folder Tree Browser
   ============================================================ */

/* ---------------- 文件夹浏览 ---------------- */

async function openFolder() {
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  if (!hasPy) { showToast(_t('toast.openFolderBrowserNotice') || '浏览器模式下请使用“打开文件”'); return; }
  let dir;
  try { dir = await py.choose_folder(state.folder || state.dir || ''); } catch (e) { dir = null; }
  if (!dir) return;
  await listFolder(dir);
}

let folderRequestEpoch = 0;
async function listFolder(dir, { reveal = true } = {}) {
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  const epoch = ++folderRequestEpoch;
  try {
    const r = await apiFetch('/api/list?p=' + encodeURIComponent(dir));
    const d = await r.json();
    if (epoch !== folderRequestEpoch) return false;
    if (!r.ok || d.ok === false || typeof d.dir !== 'string' || !Array.isArray(d.files) || d.files.some(p => typeof p !== 'string')) {
      throw new Error(d.error || 'HTTP ' + r.status);
    }
    state.folder = d.dir;
    state.folderFiles = d.files || [];
    if (reveal) showSide('files', { loadFiles: false });
    else renderFolderList();
    return true;
  } catch (e) {
    if (epoch === folderRequestEpoch) showToast((_t('toast.readFolderFail') || '读取文件夹失败') + ': ' + e.message);
    return false;
  }
}


function renderFolderList() {
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  const box = $('file-list');
  box.replaceChildren();
  if (!state.folder) {
    syncFileSidebar(true);
    return;
  }

  const folderName = state.folder.replace(/\\/g, '/').split('/').filter(Boolean).pop() || state.folder;
  const header = document.createElement('div');
  header.className = 'dir-header';
  header.innerHTML = TREE_FOLDER_ICON + '<span></span>';
  header.querySelector('span').textContent = folderName;
  header.title = state.folder;
  box.appendChild(header);

  if (!state.folderFiles || !state.folderFiles.length) {
    const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
    const empty = document.createElement('div');
    empty.className = 'dir-label';
    empty.textContent = _t('sidebar.emptyFiles') || '（未找到 Markdown 文件）';
    box.appendChild(empty);
    syncFileSidebar(true);
    return;
  }


  const normRoot = state.folder.replace(/\\/g, '/').replace(/\/$/, '');
  const rootNode = { name: folderName, type: 'dir', path: state.folder, children: Object.create(null) };

  state.folderFiles.forEach(fullPath => {
    const normPath = fullPath.replace(/\\/g, '/');
    let rel = normPath;
    if (normPath.toLowerCase().startsWith(normRoot.toLowerCase() + '/')) {
      rel = normPath.slice(normRoot.length + 1);
    }
    const parts = rel.split('/').filter(Boolean);
    let curr = rootNode;
    parts.forEach((part, idx) => {
      const isFile = (idx === parts.length - 1);
      if (!curr.children[part]) {
        curr.children[part] = {
          name: part,
          type: isFile ? 'file' : 'dir',
          path: isFile ? fullPath : (normRoot + '/' + parts.slice(0, idx + 1).join('/')),
          children: isFile ? null : Object.create(null)
        };
      }
      if (!isFile) {
        curr = curr.children[part];
      }
    });
  });

  const treeContainer = document.createElement('div');
  treeContainer.className = 'tree-children';
  treeContainer.setAttribute('role', 'tree');
  treeContainer.setAttribute('aria-label', _t('sidebar.files') || '文件');
  renderTreeNodes(treeContainer, rootNode.children, 0);
  box.appendChild(treeContainer);
  syncFileSidebar(true);
}

function renderTreeNodes(container, childrenObj, depth) {
  const keys = Object.keys(childrenObj || {}).sort((a, b) => {
    const itemA = childrenObj[a];
    const itemB = childrenObj[b];
    if (itemA.type !== itemB.type) {
      return itemA.type === 'dir' ? -1 : 1;
    }
    return itemA.name.localeCompare(itemB.name, undefined, { numeric: true, sensitivity: 'base' });
  });

  keys.forEach(key => {
    const item = childrenObj[key];
    const nodeEl = document.createElement('div');
    nodeEl.className = 'tree-node';
    nodeEl.setAttribute('role', 'none');

    const row = document.createElement('button');
    row.type = 'button';
    row.className = 'tree-row';
    row.setAttribute('role', 'treeitem');
    row.setAttribute('aria-level', String(depth + 1));
    row.tabIndex = -1;
    row.setAttribute('aria-selected', 'false');

    const toggle = document.createElement('span');
    toggle.className = 'tree-toggle';
    toggle.setAttribute('aria-hidden', 'true');

    if (item.type === 'dir') {
      const childrenCount = Object.keys(item.children || {}).length;
      if (childrenCount > 0) {
        toggle.textContent = '▶';
        toggle.classList.add('open');
      } else {
        toggle.classList.add('empty');
      }

      const nameEl = document.createElement('span');
      nameEl.className = 'tree-name';
      nameEl.textContent = item.name;

      row.appendChild(toggle);
      const icon = document.createElement('span');
      icon.className = 'tree-icon';
      icon.innerHTML = TREE_FOLDER_ICON;
      row.appendChild(icon);
      row.appendChild(nameEl);
      nodeEl.appendChild(row);

      const childrenContainer = document.createElement('div');
      childrenContainer.className = 'tree-children';
      childrenContainer.setAttribute('role', 'group');
      childrenContainer.setAttribute('aria-label', item.name);
      renderTreeNodes(childrenContainer, item.children, depth + 1);
      nodeEl.appendChild(childrenContainer);
      row.setAttribute('aria-expanded', 'true');

      row.addEventListener('click', e => {
        e.stopPropagation();
        const collapsed = childrenContainer.classList.toggle('collapsed');
        row.setAttribute('aria-expanded', collapsed ? 'false' : 'true');
        if (collapsed) {
          toggle.classList.remove('open');
        } else {
          toggle.classList.add('open');
        }
        updateTreeTabIndex();
      });
    } else {
      toggle.classList.add('empty');

      row.dataset.filePath = normalizePath(item.path);
      const nameEl = document.createElement('span');
      nameEl.className = 'tree-name';
      nameEl.textContent = item.name;

      row.appendChild(toggle);
      row.appendChild(nameEl);
      row.title = item.path;
      nodeEl.appendChild(row);

      row.addEventListener('click', e => {
        e.stopPropagation();
        loadFile(item.path);
      });
    }

    container.appendChild(nodeEl);
  });
}

let sidebarActivePath = '';

function syncFileSidebar(rebuilt = false) {
  const box = $('file-list');
  if (!box) return;
  const listed = new Set(state.folderFiles.map(normalizePath));
  const opened = new Map();
  for (const tab of state.tabs) {
    if (!tab.path || tab.isVirtual || tab._restoreUnavailable) continue;
    const key = normalizePath(tab.path);
    if (!listed.has(key)) opened.set(key, { name: tab.name || tab.path.split(/[\\/]/).pop(), path: tab.path, type: 'file' });
  }
  const existing = $('file-open-group');
  const signature = JSON.stringify([...opened.values()]);
  const refreshed = existing?.dataset.files !== signature && (opened.size > 0 || existing);
  if (refreshed) {
    existing?.remove();
    if (opened.size) {
      const group = document.createElement('div');
      group.id = 'file-open-group';
      group.dataset.files = signature;
      const header = document.createElement('div');
      header.id = 'file-open-label';
      header.className = 'dir-header';
      header.dataset.i18n = 'tabs.openDocs';
      header.textContent = window.i18n.t('tabs.openDocs');
      const tree = document.createElement('div');
      tree.className = 'tree-children';
      tree.setAttribute('role', 'tree');
      tree.setAttribute('aria-labelledby', header.id);
      renderTreeNodes(tree, Object.fromEntries(opened), 0);
      group.append(header, tree);
      box.appendChild(group);
    }
  }

  const path = getActiveTab()?.path;
  const active = path ? normalizePath(path) : '';
  const changed = active !== sidebarActivePath;
  if (rebuilt || refreshed || changed) {
    sidebarActivePath = active;
    let activeRow = null;
    for (const row of box.querySelectorAll('.tree-row[data-file-path]')) {
      const selected = row.dataset.filePath === active;
      row.classList.toggle('active', selected);
      row.setAttribute('aria-selected', String(selected));
      if (selected) {
        row.setAttribute('aria-current', 'true');
        activeRow = row;
      } else row.removeAttribute('aria-current');
    }
    if (activeRow && (changed || rebuilt)) {
      for (let group = activeRow.closest('.tree-children.collapsed'); group; group = group.parentElement.closest('.tree-children.collapsed')) {
        group.classList.remove('collapsed');
        const parent = group.parentElement.querySelector(':scope > .tree-row');
        parent?.setAttribute('aria-expanded', 'true');
        parent?.querySelector('.tree-toggle')?.classList.add('open');
      }
      if (activeRow.offsetParent) activeRow.scrollIntoView({ block: 'nearest' });
    }
  }
  if (rebuilt || refreshed || changed) updateTreeTabIndex();
}

function visibleTreeRows() {
  return Array.from(document.querySelectorAll('#file-list .tree-row'))
    .filter(row => row.offsetParent !== null);
}

function updateTreeTabIndex() {
  const rows = visibleTreeRows();
  const entry = rows.find(row => row === document.activeElement) || rows.find(row => row.classList.contains('active')) || rows[0];
  rows.forEach(row => { row.tabIndex = row === entry ? 0 : -1; });
}

function handleTreeKeydown(event, row) {
  const rows = visibleTreeRows();
  const index = rows.indexOf(row);
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault();
    const next = rows[(index + (event.key === 'ArrowDown' ? 1 : rows.length - 1)) % rows.length];
    next.focus();
    return;
  }
  if (event.key !== 'ArrowRight' && event.key !== 'ArrowLeft') return;
  event.preventDefault();
  const expanded = row.getAttribute('aria-expanded');
  if (event.key === 'ArrowRight') {
    if (expanded === 'false') {
      row.click();
    } else {
      const childContainer = row.closest('.tree-node')?.querySelector(':scope > .tree-children');
      const childRow = childContainer?.querySelector(':scope > .tree-node > .tree-row');
      if (childRow && childRow.offsetParent) childRow.focus();
    }
    return;
  }
  if (expanded === 'true') {
    row.click();
    return;
  }
  row.closest('.tree-node')?.parentElement?.closest('.tree-node')?.querySelector(':scope > .tree-row')?.focus();
}

document.addEventListener('keydown', event => {
  const row = event.target instanceof Element ? event.target.closest('#file-list .tree-row') : null;
  if (row) handleTreeKeydown(event, row);
});
