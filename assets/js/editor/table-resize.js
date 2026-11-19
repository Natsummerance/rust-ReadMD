'use strict';

(function (root) {
  const bounds = root.ReadMDTableAttributes.dimensions;
  const clamp = (axis, value) => Math.min(bounds[axis].max, Math.max(bounds[axis].min, Math.round(value)));

  function mount(grid, context, canResize, commit, translate) {
    const document = grid.ownerDocument;
    const columns = [], rows = [], colgroup = document.createElement('colgroup');
    grid.append(colgroup);
    grid.classList.add('has-table-resize');
    let drag = null;

    function authored(track) {
      const sizes = track.axis === 'width' ? context.layout.widths : context.layout.heights;
      return sizes[track.index];
    }

    function dimension(track) {
      return clamp(track.axis, authored(track) ?? track.cell.getBoundingClientRect()[track.axis]);
    }

    function paint(widthDraft, rowDraft) {
      const widths = widthDraft ?? context.layout.widths;
      const fixed = widths.reduce((total, width) => total + (width ?? 0), 0);
      const auto = widths.filter(width => width == null).length;
      grid.style.setProperty('--table-fixed-width', fixed + 'px');
      grid.style.setProperty('--table-auto-columns', String(auto));
      grid.style.width = auto ? '100%' : 'var(--table-grid-min-width)';
      columns.forEach((track, index) => { track.column.style.width = widths[index] == null ? '' : widths[index] + 'px'; });
      rows.forEach(track => {
        const height = track === rowDraft?.track ? rowDraft.value : authored(track);
        if (height == null) track.row.style.removeProperty('--table-row-height');
        else track.row.style.setProperty('--table-row-height', height + 'px');
      });
    }

    function update() {
      const enabled = canResize();
      for (const track of [...columns, ...rows]) {
        track.handle.setAttribute('aria-disabled', String(!enabled));
        if (enabled && !drag) track.handle.setAttribute('aria-valuenow', String(dimension(track)));
      }
    }
    const resizeObserver = typeof root.ResizeObserver === 'function' ? new root.ResizeObserver(() => {
      if (!grid.isConnected) resizeObserver.disconnect();
      else if (!drag) update();
    }) : null;

    function release() {
      const gesture = drag;
      drag = null;
      if (!gesture) return null;
      const { handle } = gesture.track;
      delete handle.dataset.tableDragging;
      if (handle.hasPointerCapture(gesture.pointerId)) handle.releasePointerCapture(gesture.pointerId);
      return gesture;
    }

    function cancel() {
      if (!release()) return false;
      paint();
      update();
      return true;
    }

    async function save(track, value) {
      try {
        if (canResize()) await commit(track.axis, track.index, value);
      } finally {
        paint();
        update();
      }
    }

    function coordinate(track, event) {
      return track.axis === 'width' ? event.clientX : event.clientY;
    }

    function preview(event) {
      if (!drag || event.pointerId !== drag.pointerId) return;
      const { track, initial, origin, direction, widths } = drag;
      drag.value = clamp(track.axis, initial + direction * (coordinate(track, event) - origin));
      if (widths) widths[track.index] = drag.value;
      paint(widths, drag);
      track.handle.setAttribute('aria-valuenow', String(drag.value));
    }

    function register(axis, index, cell, row) {
      const handle = document.createElement('span');
      const track = { axis, index, cell, row, handle };
      if (axis === 'width') {
        track.column = document.createElement('col');
        colgroup.append(track.column);
        columns.push(track);
      } else rows.push(track);
      const key = axis === 'width' ? 'editor.tableResizeColumn' : 'editor.tableResizeRow';
      const label = translate(key, { index: index + 1 });
      handle.className = 'table-resize-handle';
      handle.dataset.tableResize = axis;
      handle.dataset.tableTrack = String(index);
      handle.tabIndex = 0;
      handle.title = label;
      handle.setAttribute('role', 'separator');
      handle.setAttribute('aria-label', label);
      handle.setAttribute('aria-orientation', axis === 'width' ? 'vertical' : 'horizontal');
      handle.setAttribute('aria-valuemin', String(bounds[axis].min));
      handle.setAttribute('aria-valuemax', String(bounds[axis].max));
      handle.addEventListener('focus', () => handle.setAttribute('aria-valuenow', String(dimension(track))));
      handle.addEventListener('pointerdown', event => {
        if (event.button !== 0 || event.isPrimary === false || drag || !canResize()) return;
        try { handle.setPointerCapture(event.pointerId); } catch (_) { return; }
        event.preventDefault();
        handle.focus({ preventScroll: true });
        const initial = dimension(track);
        const widths = axis === 'width' ? columns.map(item => item.cell.getBoundingClientRect().width) : undefined;
        const direction = widths && root.getComputedStyle(grid).direction === 'rtl' ? -1 : 1;
        drag = { track, initial, value: initial, widths, origin: coordinate(track, event), direction, pointerId: event.pointerId };
        handle.dataset.tableDragging = '';
      });
      handle.addEventListener('pointermove', preview);
      handle.addEventListener('pointerup', event => {
        if (!drag || drag.pointerId !== event.pointerId) return;
        preview(event);
        const { initial, value } = release();
        if (value === initial) { paint(); update(); return; }
        void save(track, value);
      });
      for (const eventName of ['pointercancel', 'lostpointercapture']) {
        handle.addEventListener(eventName, event => {
          if (drag?.pointerId === event.pointerId) cancel();
        });
      }
      handle.addEventListener('keydown', event => {
        const [less, more] = axis === 'width' ? ['ArrowLeft', 'ArrowRight'] : ['ArrowUp', 'ArrowDown'];
        if (event.altKey || event.ctrlKey || event.metaKey || ![less, more, 'Home', 'End'].includes(event.key)) return;
        event.preventDefault();
        if (drag || !canResize()) return;
        const initial = dimension(track);
        const direction = axis === 'width' && root.getComputedStyle(grid).direction === 'rtl' ? -1 : 1;
        const value = event.key === 'Home' ? bounds[axis].min : event.key === 'End' ? bounds[axis].max :
          clamp(axis, initial + (event.shiftKey ? 24 : 8) * (event.key === more ? direction : -direction));
        if (value !== initial) void save(track, value);
      });
      cell.append(handle);
    }

    return {
      add(cell, slot, row) {
        if (cell.row === 0) register('width', cell.column, slot, row);
        if (cell.column === 0) register('height', cell.row, slot, row);
      },
      refresh() { paint(); update(); resizeObserver?.observe(grid); },
      update,
      cancel,
    };
  }

  root.ReadMDTableResize = { mount };
})(window);
