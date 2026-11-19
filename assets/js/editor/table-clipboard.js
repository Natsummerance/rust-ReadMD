'use strict';
(function (root, factory) {
  if (typeof module !== 'undefined' && module.exports) {
    module.exports = factory(require('./table-attributes.js'), require('../../vendor/marked.min.js'), require('../reader/heading-identity.js'));
  } else root.ReadMDTableClipboard = factory(root.ReadMDTableAttributes, root.marked, root.ReadMDHeadings);
})(typeof window !== 'undefined' ? window : null, (attributes, marked, headings) => {
  const mime = 'application/x-readmd-table+json';
  const csvMime = 'text/csv', tsvMime = 'text/tab-separated-values';
  const maxSource = 65536, maxCells = 256, maxClipboard = 1024 * 1024;
  const invalidRange = 'editor.tableCellInvalidRange';
  const unsafeString = /[\u0000\uD800-\uDFFF]/u;

  function inBounds(grid, area) {
    return area && [area.row, area.column, area.height, area.width].every(Number.isSafeInteger) &&
      area.row >= 0 && area.column >= 0 && area.height >= 1 && area.width >= 1 &&
      area.row + area.height <= grid.length && area.column + area.width <= grid[0].length;
  }

  function containsSpans(context, area) {
    const bottom = area.row + area.height, right = area.column + area.width;
    for (let row = area.row; row < bottom; row++) for (let col = area.column; col < right; col++) {
      const owner = context.layout.owners[row][col];
      if (owner && (owner.row < area.row || owner.column < area.column ||
          owner.row + owner.rowspan > bottom || owner.column + owner.colspan > right)) return false;
    }
    return true;
  }

  function rectangle(context, anchor, end) {
    if (!anchor) return null;
    if (end && (end.row !== anchor.row || end.column !== anchor.column)) {
      return { row: Math.min(anchor.row, end.row), column: Math.min(anchor.column, end.column),
        height: Math.abs(anchor.row - end.row) + 1, width: Math.abs(anchor.column - end.column) + 1, explicit: true };
    }
    const owner = context.layout.owners[anchor.row]?.[anchor.column];
    return owner ? { row: owner.row, column: owner.column, height: owner.rowspan, width: owner.colspan } :
      { row: anchor.row, column: anchor.column, height: 1, width: 1 };
  }

  function shape(rows) {
    if (!Array.isArray(rows) || rows.length === 0 || rows.length > attributes.maxRows + 1 || !Array.isArray(rows[0])) return null;
    const columns = rows[0].length;
    if (!columns || columns > attributes.maxColumns || rows.length * columns > maxCells ||
        rows.some(row => !Array.isArray(row) || row.length !== columns ||
          row.some(cell => typeof cell !== 'string' || unsafeString.test(cell)))) return null;
    return columns;
  }

  // The clipboard parser keeps empty cells explicit. A short row in a range
  // would make it unclear whether an existing target slot should be erased.
  function parseDelimited(text, delimiter) {
    if (text.length > maxClipboard || unsafeString.test(text)) return null;
    text = text.replace(/^\uFEFF/, '').replace(/\r\n?/g, '\n');
    const rows = [];
    let row = [], cell = '', quoted = false, closed = false;
    for (let at = 0; at < text.length; at++) {
      const ch = text[at];
      if (quoted) {
        if (ch !== '"') cell += ch;
        else if (text[at + 1] === '"') { cell += '"'; at++; }
        else { quoted = false; closed = true; }
      } else if (ch === delimiter || ch === '\n') {
        row.push(cell); cell = ''; closed = false;
        if (row.length > attributes.maxColumns) return null;
        if (ch === '\n') {
          rows.push(row); row = [];
          if (rows.length > attributes.maxRows + 1) return null;
        }
      } else {
        if (closed || delimiter === ',' && ch === '"' && cell) return null;
        if (ch === '"' && !cell) quoted = true;
        else cell += ch;
      }
    }
    if (quoted) return null;
    if (!text.endsWith('\n') || row.length) { row.push(cell); rows.push(row); }
    return shape(rows) ? rows : null;
  }

  function serialize(rows, delimiter) {
    return rows.map(row => row.map(cell => {
      // Quoting the empty one-column record distinguishes it from a trailing newline.
      if (!cell && row.length === 1 || cell.includes(delimiter) || /["\r\n\t\uFEFF]/.test(cell)) {
        return '"' + cell.replace(/"/g, '""').replace(/\r\n?|\n/g, '\r\n') + '"';
      }
      return cell;
    }).join(delimiter)).join('\r\n');
  }

  function richCell(source) {
    if (/[\r\n\u2028\u2029]|^[ \t]|[ \t]$|<\/?(?:table|tr|td|th|col|caption)\b/i.test(source)) return false;
    let escaped = false;
    for (const ch of source) {
      if (ch === '\\') { escaped = !escaped; continue; }
      if (ch === '|' && !escaped) return false;
      escaped = false;
    }
    return true;
  }

  function decodeRich(value) {
    if (!value || value.length > maxClipboard) return null;
    try {
      const { version, header, firstColumn, rows } = JSON.parse(value);
      const columns = shape(rows);
      if (version !== 1 || typeof header !== 'boolean' || typeof firstColumn !== 'boolean' || !columns ||
          rows.some(row => row.some(cell => !richCell(cell))) || rows.flat().reduce((n, cell) => n + cell.length, 0) > maxSource) return null;
      // The first copied row can be a body row. Recreate only the header/first
      // column offsets needed for validating span and track suffix placement.
      const positioned = firstColumn ? [...rows] : rows.map(row => ['', ...row]);
      if (!header) positioned.unshift(Array(positioned[0].length).fill(''));
      if (!attributes.layout(positioned)) return null;
      return { rows, columns, rich: true };
    } catch (_) { return null; }
  }

  // null: let ordinary paste continue; false: recognizable but invalid range.
  function read(clipboard, allowSingleColumn = false) {
    if (!clipboard) return null;
    const types = Array.from(clipboard.types || []), get = type => clipboard.getData(type);
    const custom = get(mime);
    if (custom) return decodeRich(custom) || false;
    const html = get('text/html');
    if (html && html.length <= maxClipboard) {
      const carrier = /<table\b[^<>]*?\sdata-readmd-table\s*=\s*(['"])([^'"\s<>]*)\1[^<>]*>/i.exec(html);
      if (carrier) {
        // The HTML is an optional transport when a browser/OS filters custom
        // MIME. Never parse or insert the clipboard's HTML as DOM.
        try { return decodeRich(decodeURIComponent(carrier[2])) || false; }
        catch (_) { return false; }
      }
    }
    if (types.includes(mime)) return false;
    const csv = get(csvMime), tsv = get(tsvMime), text = get('text/plain');
    const explicitCsv = csv || types.includes(csvMime), explicitTsv = tsv || types.includes(tsvMime);
    if (!explicitCsv && !explicitTsv && !text.includes('\t')) {
      if (!allowSingleColumn || !text && !types.includes('text/plain')) return null;
      // Preserve literal quotes for normal single-cell text copied elsewhere.
      if (!/[\r\n]/.test(text)) return shape([[text]]) && text.length <= maxClipboard ? { rows: [[text]], columns: 1 } : false;
    }
    const rows = parseDelimited(explicitCsv ? csv : explicitTsv ? tsv : text, explicitCsv ? ',' : '\t');
    return rows ? { rows, columns: rows[0].length } : false;
  }

  function plainText(source) {
    if (!marked?.Lexer?.lexInline || !headings?.inlineText) return source;
    const flatten = tokens => tokens.map(token => {
      if (token.type === 'br' || token.type === 'html' && /^<br\s*\/?>$/i.test(token.raw)) return '\n';
      return token.tokens ? flatten(token.tokens) : headings.inlineText([token]);
    }).join('');
    return flatten(marked.Lexer.lexInline(source.replace(/\\\|/g, '|')));
  }

  const escapeHtml = text => text.replace(/[&<>"']/g, ch => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[ch]));
  function flavors(context, area) {
    if (!inBounds(context.rows, area) || !containsSpans(context, area)) return null;
    const rows = context.rows.slice(area.row, area.row + area.height).map(row => row.slice(area.column, area.column + area.width));
    const json = JSON.stringify({ version: 1, header: area.row === 0, firstColumn: area.column === 0,
      rows: rows.map(row => row.map(cell => cell.original)) });
    if (!decodeRich(json)) return null;
    const values = rows.map(row => row.map(cell => plainText(cell.content)));
    const carrier = encodeURIComponent(json).replace(/'/g, '%27');
    const html = '<table data-readmd-table="' + carrier + '">' + rows.map((row, y) => {
      const tag = y + area.row === 0 ? 'th' : 'td';
      return '<tr>' + row.map((cell, x) => {
        const owner = context.layout.owners[cell.row][cell.column];
        if (owner && (owner.row !== cell.row || owner.column !== cell.column)) return '';
        const height = owner?.rowspan || 1, width = owner?.colspan || 1;
        const props = context.layout.cells[cell.row][cell.column]?.attributes;
        const sizes = Object.entries({ rowspan: height > 1 ? height : null, colspan: width > 1 ? width : null,
          align: props?.align, valign: props?.valign, width: props?.width, height: props?.height })
          .filter(([, value]) => value != null).map(([key, value]) => ' ' + key + '="' + value + '"').join('');
        const items = values.slice(y, y + height).flatMap(line => line.slice(x, x + width)).filter(Boolean);
        return '<' + tag + sizes + '>' + items.map(value => escapeHtml(value).replace(/\n/g, '<br>')).join('<br>') + '</' + tag + '>';
      }).join('') + '</tr>';
    }).join('') + '</table>';
    const tsv = serialize(values, '\t');
    return { [mime]: json, 'text/html': html, [csvMime]: serialize(values, ','), [tsvMime]: tsv, 'text/plain': tsv };
  }

  function write(event, data) {
    if (!event.clipboardData || !data || event.defaultPrevented) return false;
    // Plain text is mandatory; Gecko/WebKit/Chromium differ on what other
    // clipboard flavors survive a native clipboard write.
    try { event.clipboardData.setData('text/plain', data['text/plain']); }
    catch (_) { return false; }
    for (const [type, value] of Object.entries(data)) if (type !== 'text/plain') {
      try { event.clipboardData.setData(type, value); } catch (_) { /* Keep the other formats. */ }
    }
    event.preventDefault();
    return true;
  }

  function richReplacement(source, targetAttrs) {
    const from = attributes.parseCell(source), into = targetAttrs?.attributes;
    const missingTracks = ['width', 'height'].filter(key => into?.[key] !== undefined && from?.attributes[key] === undefined);
    if (!missingTracks.length) return source;
    const tracks = missingTracks.map(key => key + '=' + into[key]).join(' ');
    return from ? source.slice(0, from.to - 1) + ' ' + tracks + source.slice(from.to - 1) :
      source + (source ? ' ' : '') + '{' + tracks + '}';
  }

  function replacements(doc, from, to, edits) {
    let start = from, result = '';
    for (const edit of edits) { result += doc.sliceString(start, edit.from) + edit.insert; start = edit.to; }
    return result + doc.sliceString(start, to);
  }

  function plan(context, target, payload, encodeCell) {
    if (!target || !payload) return null;
    const area = { row: target.row, column: target.column, height: payload.rows.length, width: payload.columns };
    if (target.explicit && (area.height !== target.height || area.width !== target.width) ||
        !inBounds(context.rows, area) || !containsSpans(context, area)) return null;
    const updatedRows = context.rows.map(row => row.map(cell => cell.original)), edits = [];
    for (let y = 0; y < area.height; y++) for (let x = 0; x < area.width; x++) {
      const row = area.row + y, column = area.column + x, source = payload.rows[y][x];
      const cell = context.rows[row][column], existing = context.layout.cells[row][column];
      let result;
      if (payload.rich) result = richReplacement(source, existing);
      else {
        if (source.length > maxSource) return null;
        const body = encodeCell(source), suffix = existing && cell.original.slice(existing.from + (existing.from > 0 ? 1 : 0));
        result = suffix ? (body ? body + ' ' : '') + suffix : body;
      }
      updatedRows[row][column] = result;
      if (result !== cell.original) edits.push({ from: cell.from, to: cell.to, insert: result });
    }
    if (!edits.length) return { edits };
    if (!attributes.layout(updatedRows)) return null;
    // An originally unframed cell may no longer parse when emptied or when
    // the pasted Markdown ends in an escape. Check the entire table without
    // reserializing its untouched rows, delimiter, padding or other cells.
    const doc = context.editorDoc, tableFrom = context.lines[0].line.from, tableTo = context.lines.at(-1).line.to;
    if (tableTo - tableFrom + edits.reduce((n, edit) => n + edit.insert.length - (edit.to - edit.from), 0) > maxSource) return null;
    const next = replacements(doc, tableFrom, tableTo, edits);
    const tokens = marked.lexer(next, { gfm: true }), token = tokens[0];
    if (tokens.length !== 1 || token?.type !== 'table') return null;
    const renderedRows = [token.header, ...token.rows];
    if (renderedRows.length !== updatedRows.length || renderedRows.some((row, y) => row.length !== updatedRows[y].length ||
        row.some((cell, x) => cell.text !== updatedRows[y][x].replace(/\\\|/g, '|')))) return null;
    const from = edits[0].from, to = edits.at(-1).to;
    return { from, to, insert: replacements(doc, from, to, edits), edits };
  }

  // CSV is explicitly available to plain-text consumers even when the OS
  // does not expose the native text/csv flavor from the standard Copy action.
  async function copyCsv(value) {
    try { await navigator.clipboard.writeText(value); return true; } catch (_) { /* Try the native copy event. */ }
    let written = false;
    const listener = event => { written = write(event, { 'text/plain': value, [csvMime]: value }); };
    document.addEventListener('copy', listener);
    try { document.execCommand('copy'); } catch (_) { /* Both transports are unavailable. */ }
    finally { document.removeEventListener('copy', listener); }
    return written;
  }

  function bindGrid({ grid, csvButton, pasteCsvButton, context, getCell, getRange, ready, encodeCell, report, commit }) {
    const gridButton = target => target?.closest?.('button[data-table-row][data-table-column]');
    function area(target) {
      const range = getRange();
      const button = gridButton(target);
      if (range && (!button || [range.anchor, range.end].some(cell => cell.row === +button.dataset.tableRow && cell.column === +button.dataset.tableColumn))) {
        return rectangle(context, range.anchor, range.end);
      }
      const cell = button ? context.rows[+button.dataset.tableRow]?.[+button.dataset.tableColumn] : getCell();
      return rectangle(context, cell);
    }
    grid.addEventListener('copy', event => {
      if (!ready() || !event.clipboardData || event.defaultPrevented) return;
      const data = flavors(context, area(event.target));
      if (!data) { event.preventDefault(); report(invalidRange); return; }
      if (write(event, data)) report('toast.copiedSelection');
    });
    function paste(event, target = event.target) {
      if (!event.clipboardData || event.defaultPrevented) return;
      const payload = read(event.clipboardData, true);
      if (payload === null) return;
      event.preventDefault();
      if (!ready()) return;
      const edit = plan(context, area(target), payload, encodeCell);
      if (!edit) { report(invalidRange); return; }
      if (edit.edits.length) void commit(edit);
    }
    grid.addEventListener('paste', paste);

    // Some native hosts only send Paste to an editable target. Momentarily
    // give the grid shortcut such a target while leaving the default command
    // intact, so it still delivers a trusted ClipboardEvent with all flavors.
    const doc = grid.ownerDocument;
    if (doc && grid.parentElement) {
      const relay = doc.createElement('textarea');
      relay.tabIndex = -1; relay.inputMode = 'none';
      relay.setAttribute('aria-label', grid.getAttribute('aria-label'));
      relay.style.cssText = 'position:absolute;width:1px;height:1px;padding:0;border:0;opacity:0;pointer-events:none';
      grid.parentElement.append(relay);
      let origin;
      function restoreFocus() {
        const button = origin; origin = null;
        if (doc.activeElement === relay && button?.isConnected) button.focus({ preventScroll: true });
      }
      relay.addEventListener('paste', event => {
        const target = origin;
        restoreFocus();
        if (target) paste(event, target);
        event.preventDefault();
      });
      grid.addEventListener('keydown', event => {
        if (!(event.metaKey || event.ctrlKey) || event.altKey || event.key.toLowerCase() !== 'v' ||
            event.defaultPrevented || event.isComposing || !ready()) return;
        const button = gridButton(event.target);
        if (!button) return;
        origin = button; relay.value = '';
        relay.focus({ preventScroll: true });
        setTimeout(() => { if (origin === button) restoreFocus(); }, 250);
      });
    }
    // Some spreadsheet apps expose CSV only as plain text. An explicit
    // paste keeps commas in ordinary prose literal and uses the same grid transaction.
    if (pasteCsvButton) {
      let fallback, request = 0;
      function dropFallback() { fallback?.remove(); fallback = null; }
      function sameTarget(target) {
        const now = area();
        return ready() && target && now && ['row','column','width','height','explicit']
          .every(key => target[key] === now[key]);
      }
      function accept(text, target) {
        if (!sameTarget(target)) { report('review.conflict'); return; }
        const rows = parseDelimited(text, ',');
        const edit = rows && plan(context, target, {rows, columns: rows[0].length}, encodeCell);
        if (!edit) { report(invalidRange); return; }
        if (edit.edits.length) void commit(edit);
      }
      pasteCsvButton.addEventListener('click', async () => {
        if (!ready()) return;
        const target = area(), token = ++request;
        dropFallback();
        if (!target) { report(invalidRange); return; }
        if (typeof navigator !== 'undefined' && navigator.clipboard?.readText) {
          try {
            const text = await navigator.clipboard.readText();
            if (token === request && text) accept(text, target);
            return;
          } catch (_) { /* WebKit can require a keyboard or native menu paste. */ }
        }
        if (token !== request || !sameTarget(target)) return;
        const input = pasteCsvButton.ownerDocument.createElement('textarea');
        input.rows = 1; input.className = 'modal-input'; input.placeholder = pasteCsvButton.textContent;
        input.setAttribute('aria-label', pasteCsvButton.textContent);
        input.addEventListener('paste', event => {
          event.preventDefault();
          const value = event.clipboardData?.getData(csvMime) || event.clipboardData?.getData('text/plain');
          dropFallback(); pasteCsvButton.focus();
          if (value && token === request) accept(value, target);
        });
        input.addEventListener('keydown', event => {
          if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); dropFallback(); pasteCsvButton.focus(); }
        });
        fallback = input; pasteCsvButton.after(input); input.focus();
      });
    }
    csvButton.addEventListener('click', async () => {
      if (!ready()) return;
      const data = flavors(context, area());
      if (!data) { report(invalidRange); return; }
      const copied = await copyCsv(data[csvMime]);
      if (ready()) report(copied ? 'toast.copiedSelection' : 'toast.copyFailed');
    });
  }

  return { mime, invalidRange, read, rectangle, flavors, plan, bindGrid };
});
