'use strict';
/* Drawing drafts never write Markdown until the captured image target is checked
 * again after staging. Reopening a draft restores ink, not insertion authority. */
window.ReadMDInk = (() => {
  const LIMIT = { edge: 8192, layers: 32, strokes: 4096, points: 48000, svg: 4 * 1024 * 1024 };
  const defaults = {
    title: '压感画布', layerName: '图层 {index}', layers: '图层', addLayer: '添加图层', name: '图层名称',
    visible: '显示图层', opacity: '不透明度', color: '画笔颜色', penWidth: '笔宽', pressure: '使用触控笔压感',
    pen: '画笔', mask: '蒙版', clearMask: '清除蒙版', import: '导入 ReadMD SVG', export: '下载 SVG',
    keepClose: '关闭并保留草稿', retarget: '使用当前选区', stale: '插入位置已变化。请回到原文档并重新选择插入位置。',
    restored: '已恢复草稿。请在原文档重新选择插入位置。', draftKept: '绘画草稿已保留',
    draftError: '恢复草稿保存失败。请先下载 SVG，再关闭画布。', closeConfirm: '恢复草稿保存失败。仍要关闭并放弃本次绘画吗？',
    importConfirm: '导入将替换当前画布，可撤销恢复。是否继续？', invalid: '无法打开此画布，请检查尺寸、图层与笔画。',
    limit: '已达到画布点数、笔画或图层上限。', hint: '拖动绘画；方向键移动画笔，空格点绘，Ctrl/⌘+Z 撤销。', canvas: '绘画区域'
  };
  const tr = (key, args = {}) => {
    const id = key.includes('.') ? key : 'ink.' + key;
    let value = window.i18n?.t(id, args);
    if (!value || value === id) value = defaults[key] || ({ 'img.width': '宽度', 'img.height': '高度', 'editor.undo': '撤销', 'editor.redo': '重做', 'dialog.imgInsert': '插入文档' })[key] || key;
    return String(value).replace(/\{(\w+)\}/g, (all, name) => args[name] ?? all);
  };
  let ui, session = null, stroke = null, frame = 0, timer = null;
  const drafts = new Map();
  const copy = value => JSON.parse(JSON.stringify(value));
  const activeTab = () => typeof getActiveTab === 'function' ? getActiveTab() : null;
  const keyOf = tab => tab ? 'readmd.ink.draft.v1.' + encodeURIComponent(tab.path || 'tab:' + tab.id) : '';
  const originalTab = s => !!s && keyOf(activeTab()) === s.key && activeTab() === s.tab;
  const targetNow = () => typeof imageEditTarget === 'function' ? imageEditTarget() : null;
  const visible = () => ui && !ui.modal.classList.contains('hidden');
  function checked(value) {
    if (value?.schema !== 1 || !Number.isInteger(value.width) || !Number.isInteger(value.height) || value.width < 1 || value.height < 1 || value.width > LIMIT.edge || value.height > LIMIT.edge || !Array.isArray(value.layers) || !value.layers.length || value.layers.length > LIMIT.layers) throw Error('invalid');
    let points = 0, strokes = 0;
    const document = { schema: 1, width: value.width, height: value.height, layers: value.layers.map(layer => {
      if (typeof layer?.name !== 'string' || Array.from(layer.name).length > 100 || typeof layer.visible !== 'boolean' || !Number.isInteger(layer.opacity) || layer.opacity < 0 || layer.opacity > 100) throw Error('invalid');
      const readStrokes = list => {
        if (!Array.isArray(list) || (strokes += list.length) > LIMIT.strokes) throw Error('limit');
        return list.map(mark => {
          if (!/^#[0-9a-f]{6}$/i.test(mark?.color) || !Number.isFinite(mark.width) || mark.width < 0.25 || mark.width > 128 || !Array.isArray(mark.points) || !mark.points.length || (points += mark.points.length) > LIMIT.points) throw Error('invalid');
          return { color: mark.color, width: mark.width, points: mark.points.map(p => {
            if (!Array.isArray(p) || p.length !== 3 || !p.every(Number.isFinite) || p[0] < 0 || p[0] > value.width || p[1] < 0 || p[1] > value.height || p[2] < 0 || p[2] > 1) throw Error('invalid');
            return p.slice();
          }) };
        });
      };
      const out = { name: layer.name, visible: layer.visible, opacity: layer.opacity, strokes: readStrokes(layer.strokes) };
      if (layer.mask !== undefined) out.mask = readStrokes(layer.mask);
      return out;
    }) };
    return document;
  }
  const empty = () => ({ schema: 1, width: 1024, height: 640, layers: [{ name: tr('layerName', { index: 1 }), visible: true, opacity: 100, strokes: [] }] });
  function counts(document) {
    let points = 0, strokes = 0;
    for (const layer of document.layers) for (const marks of [layer.strokes, layer.mask || []]) for (const mark of marks) { strokes++; points += mark.points.length; }
    return { points, strokes };
  }
  function status(key) { if (ui) ui.status.textContent = key ? tr(key) : ''; }
  function persist(s = session) {
    if (!s) return true;
    drafts.set(s.key, { document: copy(s.document), layer: s.layer });
    try {
      localStorage.setItem(s.key, JSON.stringify({ schema: 1, document: s.document, layer: s.layer }));
      s.durable = true; return true;
    } catch (_) { s.durable = false; if (s === session) status('draftError'); return false; }
  }
  function checkpoint(before) {
    session.undo.push(before); session.redo = [];
    let bytes = session.undo.reduce((sum, entry) => sum + entry.length * 2, 0);
    while (session.undo.length > 30 || bytes > 8 * 1024 * 1024) bytes -= session.undo.shift().length * 2;
  }
  function change(action) {
    if (!session || session.pending || stroke) return;
    const before = JSON.stringify(session.document), document = copy(session.document);
    try { action(document); const next = checked(document); if (JSON.stringify(next) === before) return; checkpoint(before); session.document = next; persist(); refresh(); }
    catch (error) { status(error.message === 'limit' ? 'limit' : 'invalid'); refreshControls(true); }
  }
  function history(redo) {
    if (!session || session.pending || stroke) return;
    const from = redo ? session.redo : session.undo, to = redo ? session.undo : session.redo;
    if (!from.length) return;
    to.push(JSON.stringify(session.document)); session.document = JSON.parse(from.pop());
    session.layer = Math.min(session.layer, session.document.layers.length - 1); persist(); refresh();
  }
  function label(key, control) {
    const item = document.createElement('label'); item.className = 'ink-field';
    const title = document.createElement('span'); title.dataset.inkLabel = key; title.textContent = tr(key); item.append(title, control); return item;
  }
  function button(id, key, action) {
    const control = document.createElement('button'); control.type = 'button'; control.id = id; control.dataset.inkLabel = key; control.textContent = tr(key); control.addEventListener('click', action); return control;
  }
  function input(id, type, value) { const control = document.createElement('input'); control.id = id; control.type = type; control.value = value; return control; }
  function mount() {
    if (ui) return;
    const modal = document.createElement('div'); modal.id = 'ink-modal'; modal.className = 'modal hidden'; modal.setAttribute('role', 'dialog'); modal.setAttribute('aria-modal', 'true'); modal.setAttribute('aria-labelledby', 'ink-title'); modal.dataset.backdrop = 'static';
    const box = document.createElement('section'); box.className = 'modal-dialog ink-box';
    const head = document.createElement('header'); head.className = 'modal-header';
    const title = document.createElement('h3'); title.id = 'ink-title'; title.dataset.inkLabel = 'title'; title.textContent = tr('title');
    const keep = button('ink-close', 'keepClose', close); keep.dataset.modalClose = ''; head.append(title, keep);
    const toolbar = document.createElement('div'); toolbar.className = 'ink-toolbar';
    const pen = button('ink-pen', 'pen', () => { if (session) { session.mask = false; refreshControls(); } });
    const mask = button('ink-mask', 'mask', () => { if (session) { session.mask = true; refreshControls(); } });
    const color = input('ink-color', 'color', '#222222'); color.addEventListener('change', () => { if (session) session.color = color.value; });
    const width = input('ink-width', 'number', '6'); width.min = '0.25'; width.max = '128'; width.step = '0.25'; width.addEventListener('change', () => { if (!session) return; const n = Number(width.value); if (Number.isFinite(n) && n >= 0.25 && n <= 128) session.width = n; else width.value = session.width; });
    const pressure = input('ink-pressure', 'checkbox', ''); pressure.checked = true; pressure.addEventListener('change', () => { if (session) session.pressure = pressure.checked; });
    const undo = button('ink-undo', 'editor.undo', () => history(false)), redo = button('ink-redo', 'editor.redo', () => history(true));
    const size = ['width', 'height'].map(axis => { const field = input('ink-document-' + axis, 'number', axis === 'width' ? 1024 : 640); field.min = '1'; field.max = String(LIMIT.edge); field.step = '1'; field.addEventListener('change', () => change(document => { document[axis] = Number(field.value); })); return field; });
    toolbar.append(pen, mask, label('color', color), label('penWidth', width), label('pressure', pressure), undo, redo, label('img.width', size[0]), label('img.height', size[1]));
    const workspace = document.createElement('div'); workspace.className = 'ink-workspace';
    const side = document.createElement('aside'); side.className = 'ink-layers';
    const layers = document.createElement('select'); layers.id = 'ink-layers'; layers.size = 6; layers.addEventListener('change', () => { if (!session || stroke) return; session.layer = Number(layers.value); persist(); refresh(); });
    const add = button('ink-add-layer', 'addLayer', () => { change(document => { if (document.layers.length >= LIMIT.layers) throw Error('limit'); document.layers.push({ name: tr('layerName', { index: document.layers.length + 1 }), visible: true, opacity: 100, strokes: [] }); }); if (session) { session.layer = session.document.layers.length - 1; persist(); refresh(); } });
    const name = input('ink-layer-name', 'text', ''); name.maxLength = 200; name.addEventListener('change', () => change(document => { document.layers[session.layer].name = name.value; }));
    const show = input('ink-layer-visible', 'checkbox', ''); show.addEventListener('change', () => change(document => { document.layers[session.layer].visible = show.checked; }));
    const opacity = input('ink-opacity', 'range', '100'); opacity.min = '0'; opacity.max = '100'; opacity.step = '1'; opacity.addEventListener('change', () => change(document => { document.layers[session.layer].opacity = Number(opacity.value); }));
    const clearMask = button('ink-clear-mask', 'clearMask', () => change(document => { delete document.layers[session.layer].mask; }));
    side.append(label('layers', layers), add, label('name', name), label('visible', show), label('opacity', opacity), clearMask);
    const area = document.createElement('div'); area.className = 'ink-surface';
    const canvas = document.createElement('canvas'); canvas.id = 'ink-canvas'; canvas.tabIndex = 0; canvas.setAttribute('aria-label', tr('canvas')); canvas.setAttribute('aria-describedby', 'ink-hint'); area.append(canvas); workspace.append(side, area);
    const hint = document.createElement('p'); hint.id = 'ink-hint'; hint.dataset.inkLabel = 'hint'; hint.textContent = tr('hint');
    const message = document.createElement('p'); message.id = 'ink-status'; message.setAttribute('role', 'status'); message.setAttribute('aria-live', 'polite');
    const footer = document.createElement('footer'); footer.className = 'modal-footer ink-actions';
    const file = input('ink-file', 'file', ''); file.accept = '.svg,image/svg+xml'; file.hidden = true; file.addEventListener('change', importFile);
    const load = button('ink-import', 'import', () => file.click()), download = button('ink-export', 'export', exportFile);
    const retarget = button('ink-retarget', 'retarget', () => { if (!session || session.pending || !originalTab(session)) return; const target = targetNow(); if (target?.current()) { session.target = target; status(''); refreshControls(); } else status('stale'); });
    const insert = button('ink-insert', 'dialog.imgInsert', insertDrawing); insert.classList.add('primary');
    footer.append(file, load, download, retarget, insert); box.append(head, toolbar, workspace, hint, message, footer); modal.append(box); document.body.append(modal);
    ui = { modal, box, canvas, scratch: document.createElement('canvas'), layers, add, name, show, opacity, clearMask, pen, mask, color, width, pressure, undo, redo, size, file, load, download, retarget, insert, keep, status: message };
    window.ReadMDModal?.setGuard(modal.id, () => !session?.pending);
    canvas.addEventListener('pointerdown', pointerStart); canvas.addEventListener('pointermove', pointerMove); canvas.addEventListener('pointerup', pointerEnd);
    canvas.addEventListener('pointercancel', pointerCancel); canvas.addEventListener('lostpointercapture', pointerCancel);
    canvas.addEventListener('keydown', keyboardInk); canvas.addEventListener('focus', drawSoon); canvas.addEventListener('blur', drawSoon);
    modal.addEventListener('keydown', event => {
      if (event.isComposing || event.keyCode === 229 || /^(INPUT|TEXTAREA|SELECT)$/.test(event.target.tagName)) return;
      if ((event.ctrlKey || event.metaKey) && ['z', 'y'].includes(event.key.toLowerCase())) { event.preventDefault(); event.stopPropagation(); history(event.shiftKey || event.key.toLowerCase() === 'y'); }
    });
    window.addEventListener('beforeunload', event => { if (session && !persist()) { event.preventDefault(); event.returnValue = ''; } });
  }
  function refreshControls(force = false) {
    if (!session || !ui) return;
    const layer = session.document.layers[session.layer];
    ui.modal.querySelectorAll('button,input,select').forEach(control => { control.disabled = session.pending || !!stroke; });
    ui.pen.setAttribute('aria-pressed', String(!session.mask)); ui.mask.setAttribute('aria-pressed', String(session.mask));
    const value = (control, text) => { if (force || document.activeElement !== control) control.value = text; };
    value(ui.color, session.color); value(ui.width, session.width); ui.pressure.checked = session.pressure;
    value(ui.size[0], session.document.width); value(ui.size[1], session.document.height);
    value(ui.name, layer.name); ui.show.checked = layer.visible; value(ui.opacity, layer.opacity);
    ui.undo.disabled ||= !session.undo.length; ui.redo.disabled ||= !session.redo.length; ui.add.disabled ||= session.document.layers.length >= LIMIT.layers;
    ui.keep.disabled = session.pending;
    ui.clearMask.disabled ||= !layer.mask?.length; ui.retarget.disabled ||= !originalTab(session);
    ui.insert.disabled ||= !session.target?.current(); ui.canvas.setAttribute('aria-disabled', String(session.pending || !layer.visible));
    ui.box.setAttribute('aria-busy', String(session.pending));
  }
  function refresh() {
    if (!session) return;
    ui.layers.replaceChildren(...session.document.layers.map((layer, index) => { const option = document.createElement('option'); option.value = index; option.textContent = layer.name || tr('layerName', { index: index + 1 }); option.selected = index === session.layer; return option; }));
    refreshControls(true); drawSoon();
  }
  function drawSoon() { if (!frame) frame = requestAnimationFrame(() => { frame = 0; paint(); }); }
  function paintMark(ctx, mark) {
    // Match the Rust SVG renderer, including pressure at the two segment ends.
    const radius = p => mark.width * (0.15 + 0.85 * p[2]) * 0.5;
    ctx.fillStyle = mark.color;
    for (let i = 0; i < mark.points.length; i++) {
      const p = mark.points[i], r = radius(p); ctx.beginPath(); ctx.arc(p[0], p[1], r, 0, Math.PI * 2); ctx.fill();
      if (!i) continue;
      const a = mark.points[i - 1], ar = radius(a), dx = p[0] - a[0], dy = p[1] - a[1], length = Math.hypot(dx, dy); if (!length) continue;
      const nx = -dy / length, ny = dx / length; ctx.beginPath(); ctx.moveTo(a[0] + nx * ar, a[1] + ny * ar); ctx.lineTo(p[0] + nx * r, p[1] + ny * r); ctx.lineTo(p[0] - nx * r, p[1] - ny * r); ctx.lineTo(a[0] - nx * ar, a[1] - ny * ar); ctx.closePath(); ctx.fill();
    }
  }
  function paint() {
    if (!session || !visible()) return;
    const doc = session.document, scale = Math.min(1.5, 1600 / Math.max(doc.width, doc.height)), width = Math.max(1, Math.round(doc.width * scale)), height = Math.max(1, Math.round(doc.height * scale));
    const canvas = ui.canvas, scratch = ui.scratch; if (canvas.width !== width || canvas.height !== height) { canvas.width = width; canvas.height = height; }
    if (scratch.width !== width || scratch.height !== height) { scratch.width = width; scratch.height = height; }
    canvas.style.aspectRatio = doc.width + ' / ' + doc.height;
    const ctx = canvas.getContext('2d'), layerCtx = scratch.getContext('2d'); ctx.setTransform(1, 0, 0, 1, 0, 0); ctx.clearRect(0, 0, width, height);
    doc.layers.forEach((layer, index) => {
      if (!layer.visible || !layer.opacity) return;
      layerCtx.setTransform(1, 0, 0, 1, 0, 0); layerCtx.clearRect(0, 0, width, height); layerCtx.setTransform(width / doc.width, 0, 0, height / doc.height, 0, 0); layerCtx.globalCompositeOperation = 'source-over';
      layer.strokes.forEach(mark => paintMark(layerCtx, mark)); if (stroke?.layer === index && !stroke.mask) paintMark(layerCtx, stroke.mark);
      layerCtx.globalCompositeOperation = 'destination-out'; (layer.mask || []).forEach(mark => paintMark(layerCtx, mark)); if (stroke?.layer === index && stroke.mask) paintMark(layerCtx, stroke.mark);
      ctx.globalAlpha = layer.opacity / 100; ctx.drawImage(scratch, 0, 0);
    });
    ctx.globalAlpha = 1;
    if (document.activeElement === canvas && session.cursor) { const [x, y] = session.cursor.map((v, i) => v * (i ? height / doc.height : width / doc.width)); ctx.strokeStyle = '#1686ff'; ctx.lineWidth = 1; ctx.beginPath(); ctx.moveTo(x - 7, y); ctx.lineTo(x + 7, y); ctx.moveTo(x, y - 7); ctx.lineTo(x, y + 7); ctx.stroke(); }
  }
  function sample(event, previous) {
    const rect = ui.canvas.getBoundingClientRect(), doc = session.document;
    const pressure = session.pressure && event.pointerType === 'pen' ? Math.min(1, Math.max(0, event.pressure > 0 ? event.pressure : previous?.[2] ?? 0.5)) : 1;
    return [Math.round(Math.min(doc.width, Math.max(0, (event.clientX - rect.left) / rect.width * doc.width)) * 100) / 100, Math.round(Math.min(doc.height, Math.max(0, (event.clientY - rect.top) / rect.height * doc.height)) * 100) / 100, Math.round(pressure * 1000) / 1000];
  }
  function appendSamples(event) {
    if (!stroke || event.pointerId !== stroke.id) return;
    const events = event.getCoalescedEvents?.() || [event];
    for (const pointEvent of events.length ? events : [event]) {
      if (stroke.mark.points.length + stroke.used >= LIMIT.points) { status('limit'); break; }
      const last = stroke.mark.points.at(-1), point = sample(pointEvent, last);
      if (!last || Math.abs(point[0] - last[0]) + Math.abs(point[1] - last[1]) > 0.15 || Math.abs(point[2] - last[2]) > 0.02) stroke.mark.points.push(point);
    }
    drawSoon();
  }
  function pointerStart(event) {
    if (!session || session.pending || stroke || event.button !== 0 || event.isPrimary === false || !session.document.layers[session.layer].visible) return;
    const used = counts(session.document); if (used.points >= LIMIT.points || used.strokes >= LIMIT.strokes) { status('limit'); return; }
    event.preventDefault(); ui.canvas.focus({ preventScroll: true });
    stroke = { id: event.pointerId, layer: session.layer, mask: session.mask, used: used.points, mark: { color: session.color, width: session.width, points: [sample(event)] } };
    try { ui.canvas.setPointerCapture(event.pointerId); } catch (_) { stroke = null; return; }
    refreshControls(); drawSoon();
  }
  function pointerMove(event) { if (stroke?.id === event.pointerId) { event.preventDefault(); appendSamples(event); } }
  function finishStroke(cancelled) {
    if (!stroke) return;
    const completed = stroke; stroke = null;
    if (!cancelled) change(document => { const layer = document.layers[completed.layer], key = completed.mask ? 'mask' : 'strokes'; (layer[key] ||= []).push(completed.mark); });
    refreshControls(); drawSoon();
  }
  function pointerEnd(event) { if (stroke?.id === event.pointerId) { appendSamples(event); finishStroke(false); try { ui.canvas.releasePointerCapture(event.pointerId); } catch (_) {} } }
  function pointerCancel(event) { if (stroke?.id === event.pointerId) finishStroke(true); }
  function keyboardInk(event) {
    if (!session || session.pending || stroke || event.isComposing) return;
    const amount = event.shiftKey ? 10 : 1, cursor = session.cursor || [session.document.width / 2, session.document.height / 2];
    if (event.key.startsWith('Arrow')) { event.preventDefault(); if (event.key === 'ArrowLeft') cursor[0] -= amount; if (event.key === 'ArrowRight') cursor[0] += amount; if (event.key === 'ArrowUp') cursor[1] -= amount; if (event.key === 'ArrowDown') cursor[1] += amount; session.cursor = [Math.max(0, Math.min(session.document.width, cursor[0])), Math.max(0, Math.min(session.document.height, cursor[1]))]; drawSoon(); }
    else if (event.code === 'Space' && !event.repeat) { event.preventDefault(); const used = counts(session.document); if (used.points >= LIMIT.points || used.strokes >= LIMIT.strokes) { status('limit'); return; } if (!session.document.layers[session.layer].visible) return; change(document => { const layer = document.layers[session.layer], key = session.mask ? 'mask' : 'strokes'; (layer[key] ||= []).push({ color: session.color, width: session.width, points: [[...cursor, 1]] }); }); }
  }
  async function request(action, payload) {
    const response = await apiFetch('/api/drawing', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ action, ...payload }) });
    const value = await response.json(); if (!response.ok || value.ok !== true) throw Error('invalid'); return value;
  }
  async function perform(action) {
    if (!session || session.pending || stroke) return;
    const current = session; current.pending = true; refreshControls();
    try { await action(current); } catch (_) { if (session === current) status('invalid'); }
    finally { if (session === current) { current.pending = false; refreshControls(); } }
  }
  async function importFile() {
    const file = ui.file.files?.[0]; ui.file.value = ''; if (!file || !session) return;
    if (file.size > LIMIT.svg) { status('invalid'); return; }
    if (counts(session.document).strokes && !window.confirm(tr('importConfirm'))) return;
    await perform(async current => {
      const opened = await request('open', { svg: await file.text() }); if (session !== current) return;
      const document = checked(opened.document); checkpoint(JSON.stringify(current.document)); current.document = document; current.layer = 0; current.cursor = null; persist(); refresh();
    });
  }
  async function exportFile() {
    await perform(async current => {
      const rendered = await request('render', { document: current.document }); if (session !== current || typeof rendered.svg !== 'string') return;
      const blob = new Blob([rendered.svg], { type: 'image/svg+xml' }); if (blob.size > LIMIT.svg) throw Error('invalid');
      const url = URL.createObjectURL(blob), link = document.createElement('a'); link.href = url; link.download = 'ReadMD-drawing.svg'; document.body.append(link); link.click(); link.remove(); setTimeout(() => URL.revokeObjectURL(url), 30000);
    });
  }
  async function insertDrawing() {
    if (!session?.target?.current()) { status('stale'); return; }
    await perform(async current => {
      const target = current.target, result = await request('stage', { document: current.document });
      if (session !== current || current.target !== target || !target.current() || !visible()) { status('stale'); return; }
      if (typeof result.rel !== 'string' || !result.asset || typeof registerStagedImage !== 'function' || typeof cmInsertImage !== 'function') throw Error('invalid');
      registerStagedImage(result.asset, target.tab); cmInsertImage(result.rel, target);
      drafts.delete(current.key); try { localStorage.removeItem(current.key); } catch (_) {}
      session = null; clearInterval(timer); timer = null; window.ReadMDModal.close(ui.modal);
    });
  }
  function open() {
    const tab = activeTab(); if (!tab) return false;
    if (visible()) { ui.canvas.focus(); return true; }
    mount(); const key = keyOf(tab); let saved = drafts.get(key), stored;
    if (!saved) {
      try { stored = localStorage.getItem(key); } catch (_) { /* Drawing can continue with an explicit unsaved-close guard. */ }
      if (stored) try {
        if (stored.length > LIMIT.svg) throw Error('invalid');
        const value = JSON.parse(stored); if (value.schema !== 1) throw Error('invalid');
        saved = { document: checked(value.document), layer: value.layer };
      } catch (_) { if (typeof showToast === 'function') showToast(tr('invalid')); return false; }
    }
    session = { key, tab, document: saved ? copy(saved.document) : empty(), layer: Math.max(0, Math.min(Number.isInteger(saved?.layer) ? saved.layer : 0, (saved?.document.layers.length || 1) - 1)), target: saved ? null : targetNow(), color: '#222222', width: 6, pressure: true, mask: false, pending: false, undo: [], redo: [], durable: false, cursor: null };
    ui.modal.querySelectorAll('[data-ink-label]').forEach(node => { node.textContent = tr(node.dataset.inkLabel); }); ui.canvas.setAttribute('aria-label', tr('canvas'));
    refresh(); status(saved ? 'restored' : ''); persist();
    window.ReadMDModal.open(ui.modal); drawSoon(); clearInterval(timer); timer = setInterval(() => { if (visible()) refreshControls(); }, 400); return true;
  }
  function close() {
    if (!session || session.pending) return false;
    if (stroke) finishStroke(false);
    const kept = persist(); if (!kept && !window.confirm(tr('closeConfirm'))) return false;
    if (!kept) { drafts.delete(session.key); session = null; }
    clearInterval(timer); timer = null; window.ReadMDModal.close(ui.modal); if (kept && typeof showToast === 'function') showToast(tr('draftKept')); return true;
  }
  return { open, close };
})();
