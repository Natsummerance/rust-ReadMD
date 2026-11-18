'use strict';
// Reuse Marked's resolved inline tokens, including document-level reference
// links. Rendering must not reparse a cell as an independent document.
(function (root) {
  const attributes = root.ReadMDTableAttributes;
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
      let result = '<tr>';
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
    return '<table class="readmd-table-geometry"><thead>' + renderRow(rows[0], 0) + '</thead><tbody>' +
      rows.slice(1).map((row, i) => renderRow(row, i + 1)).join('') + '</tbody></table>';
  }
  root.ReadMDTableView = { render };
  root.marked.use({ renderer: { table(token) { return render(token, this); } } });
})(window);
