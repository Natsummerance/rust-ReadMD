'use strict';
// Reuse Marked's resolved inline tokens, including document-level reference
// links. Rendering must not reparse a cell as an independent document.
(function (root) {
  const attributes = root.ReadMDTableAttributes;
  const measured = new WeakMap();
  const sizedColumns = 'table.readmd-table-geometry--columns';

  function fitColumns(widths, available) {
    const minimum = Math.min(attributes.dimensions.width.min, available / widths.length);
    const fixed = widths.filter(width => width !== null);
    const flexible = widths.length - fixed.length;
    let room = available - flexible * minimum;
    let total = fixed.reduce((sum, width) => sum + width, 0);
    let scale = Math.min(1, room / total);
    if (scale < 1 && fixed.length > 1) {
      fixed.sort((a, b) => a - b);
      for (const width of fixed.slice(0, -1)) {
        if (width * scale >= minimum) break;
        room -= minimum;
        total -= width;
        scale = room / total;
      }
    }
    const fixedWidth = width => Math.max(minimum, width * scale);
    const remaining = available - fixed.reduce((sum, width) => sum + fixedWidth(width), 0);
    const autoWidth = flexible ? Math.max(minimum, remaining / flexible) : 0;
    return widths.map(width => width === null ? autoWidth : fixedWidth(width));
  }

  function observe(scope, owner = scope, widthFraction = 1) {
    if (!owner) return;
    measured.get(owner)?.disconnect();
    measured.delete(owner);
    const byContainer = new Map();
    for (const table of scope?.querySelectorAll?.(sizedColumns) || []) {
      const cols = Array.from(table.querySelectorAll(':scope > colgroup > col'));
      const widths = cols.map(col => {
        const raw = col.dataset.tableWidth;
        const value = Number(raw);
        return /^[1-9]\d*$/.test(raw) && attributes.validDimension('width', value) ? value : null;
      });
      if (!widths.some(width => width !== null) || !table.parentElement) continue;
      const parent = table.parentElement;
      if (!byContainer.has(parent)) byContainer.set(parent, []);
      byContainer.get(parent).push({ table, cols, widths });
    }
    if (!byContainer.size) return;

    function measure(parent, width) {
      if (width <= 0) return;
      for (const { table, cols, widths } of byContainer.get(parent)) {
        if (table.parentElement !== parent) continue;
        const style = root.getComputedStyle(table);
        const border = (parseFloat(style.borderLeftWidth) || 0) + (parseFloat(style.borderRightWidth) || 0);
        const gap = style.borderCollapse === 'separate' ? (parseFloat(style.borderSpacing) || 0) * (cols.length + 1) : 0;
        const sizes = fitColumns(widths, Math.max(1, width * widthFraction - border - gap));
        table.style.width = (sizes.reduce((sum, size) => sum + size, 0) + border + gap) + 'px';
        cols.forEach((col, index) => { col.style.width = sizes[index] + 'px'; });
      }
    }
    for (const parent of byContainer.keys()) {
      const style = root.getComputedStyle(parent);
      measure(parent, parent.clientWidth - (parseFloat(style.paddingLeft) || 0) - (parseFloat(style.paddingRight) || 0));
    }
    if (typeof root.ResizeObserver === 'function') {
      const observer = new root.ResizeObserver(entries => {
        for (const entry of entries) measure(entry.target, entry.contentRect.width);
      });
      for (const parent of byContainer.keys()) observer.observe(parent);
      measured.set(owner, observer);
    }
  }

  function render(token, renderer) {
    const rows = [token.header, ...token.rows];
    if (!rows.some(row => row.some(cell => cell.text.endsWith('}')))) return false;
    const layout = attributes.layout(rows.map(row => row.map(cell => cell.text)));
    if (!layout?.hasAttributes) return false;
    function content(row, column) {
      const cell = rows[row][column];
      const parsed = layout.cells[row][column];
      if (!parsed) return renderer.parser.parseInline(cell.tokens);
      const tokens = cell.tokens.slice();
      const tail = tokens.at(-1);
      const suffix = cell.text.slice(parsed.from, parsed.to);
      // The suffix is ordinary trailing text. Never replace the first matching
      // string in a rich cell or discard a token belonging to an inline object.
      if (!tail || tail.type !== 'text' || !tail.text.endsWith(suffix)) return renderer.parser.parseInline(cell.tokens);
      const text = tail.text.slice(0, -suffix.length);
      if (text) tokens[tokens.length - 1] = { ...tail, text, raw: tail.raw.slice(0, -suffix.length) };
      else tokens.pop();
      return renderer.parser.parseInline(tokens);
    }
    function renderRow(row, index) {
      const tag = index ? 'td' : 'th';
      const height = layout.heights[index];
      let result = height ? '<tr height="' + height + '" data-table-height="' + height + '">' : '<tr>';
      for (let column = 0; column < row.length; column++) {
        const owner = layout.owners[index][column];
        if (owner && (owner.row !== index || owner.column !== column)) continue;
        const spanRows = owner?.rowspan || 1, spanCols = owner?.colspan || 1;
        const props = layout.cells[index][column]?.attributes || {};
        const align = props.align || row[column].align;
        const fragments = [];
        for (let y = index; y < index + spanRows; y++) {
          for (let x = column; x < column + spanCols; x++) {
            const html = content(y, x);
            if (html) fragments.push(html);
          }
        }
        let attrs = spanRows > 1 ? ' rowspan="' + spanRows + '"' : '';
        if (spanCols > 1) attrs += ' colspan="' + spanCols + '"';
        if (align) attrs += ' align="' + align + '"';
        if (props.valign) attrs += ' valign="' + props.valign + '"';
        result += '<' + tag + attrs + '>' + fragments.join('<br>') + '</' + tag + '>';
      }
      return result + '</tr>';
    }
    const sized = layout.widths.some(Boolean);
    const colgroup = sized ? '<colgroup>' + layout.widths.map(width =>
      width ? '<col width="' + width + '" data-table-width="' + width + '">' : '<col>').join('') + '</colgroup>' : '';
    return '<table class="readmd-table-geometry' + (sized ? ' readmd-table-geometry--columns' : '') + '">' +
      colgroup + '<thead>' + renderRow(rows[0], 0) + '</thead><tbody>' +
      rows.slice(1).map((row, i) => renderRow(row, i + 1)).join('') + '</tbody></table>';
  }
  root.ReadMDTableView = { render, observe };
  root.marked.use({ renderer: { table(token) { return render(token, this); } } });
})(window);
