'use strict';
// Structural edits consume the rows validated against CodeMirror's syntax tree.
// Only the review service commits the resulting UTF-16 replacement.
(function (root, factory) {
  if (typeof module !== 'undefined' && module.exports) {
    module.exports = factory(require('../reader/heading-identity.js'), require('../../vendor/marked.min.js'), require('./table-attributes.js'));
  } else root.ReadMDTableStructure = factory(root.ReadMDHeadings, root.marked, root.ReadMDTableAttributes);
})(typeof window !== 'undefined' ? window : null, (headings, marked, tableAttributes) => {
  function columnEdit(context, column, insert) {
    if (context.layout?.hasAttributes) return null;
    const width = context.rows[0].length;
    if (insert ? width >= tableAttributes.maxColumns || (width + 1) * context.rows.length > 256 : width === 1) return null;
    const output = context.lines.map(({ line, segments }, index) => {
      const cells = segments.map(segment => line.text.slice(segment.from, segment.to));
      let prefix = line.text.slice(0, segments[0].from);
      const suffix = line.text.slice(segments[width - 1].to);
      // Frame bare rows so an empty edge cell or the last surviving column
      // still parses as a table. Keep existing indentation outside the frame.
      if (!prefix) {
        const indent = cells[0].match(/^ {0,3}/)[0];
        cells[0] = cells[0].slice(indent.length);
        prefix = indent + '|';
      }
      if (!suffix && /(?:^|[^\\])(?:\\\\)*\\$/.test(cells[width - 1])) cells[width - 1] += ' ';
      if (insert) cells.splice(column + 1, 0, index === 1 ? ' --- ' : ' ');
      else cells.splice(column, 1);
      return prefix + cells.join('|') + (suffix || '|');
    });
    const replacement = output.join('\n');
    if (replacement.length > 65536) return null;
    return { from: context.lines[0].line.from, to: context.lines.at(-1).line.to, insert: replacement };
  }

  // Compare signed decimal significands and powers without rounding long IDs,
  // converting currencies or expanding exponent notation into huge strings.
  function decimal(text) {
    const match = /^([+-]?)(\d*)(?:\.(\d*))?(?:[eE]([+-]?\d+))?$/.exec(text);
    if (!match || !(match[2] || match[3])) return null;
    const raw = match[2] + (match[3] || '');
    const first = raw.search(/[1-9]/);
    if (first < 0) return { sign: 0, power: 0n, digits: '' };
    return { sign: match[1] === '-' ? -1 : 1,
      power: BigInt(match[2].length - first) + BigInt(match[4] || 0),
      digits: raw.slice(first).replace(/0+$/, '') };
  }
  const compare = (a, b) => a < b ? -1 : a > b ? 1 : 0;
  function sortRows(context, column, ascending, language) {
    if (context.layout?.hasAttributes) return null;
    const data = context.rows.slice(1);
    if (data.length < 2) return null;
    const collator = new Intl.Collator(language, {numeric: true});
    const keyed = data.map((row, index) => {
      const cell = row[column];
      const text = (cell.value ?? headings.inlineText(marked.Lexer.lexInline(cell.original))).trim();
      const number = decimal(text);
      return { index, text, number, group: !text ? 2 : number ? 0 : 1 };
    });
    keyed.sort((a, b) => {
      if (a.group !== b.group) return a.group - b.group;
      let order;
      if (a.number && b.number) {
        const x = a.number, y = b.number;
        order = x.sign - y.sign || x.sign * (compare(x.power, y.power) || compare(x.digits, y.digits));
      } else order = collator.compare(a.text, b.text);
      return (ascending ? order : -order) || a.index - b.index;
    });
    const first = keyed.findIndex((key, index) => key.index !== index);
    if (first < 0) return null;
    let last = keyed.length - 1;
    while (keyed[last].index === last) last--;
    const line = index => context.lines[index + 2].line;
    return { from: line(first).from, to: line(last).to,
      insert: keyed.slice(first, last + 1).map(key => line(key.index).text).join('\n') };
  }
  function attributeEdit(context, cell, attributes) {
    const current = context.layout.cells[cell.row][cell.column];
    const previous = current?.attributes ?? {}, names = Object.keys(attributes);
    if (names.length === Object.keys(previous).length && names.every(name => attributes[name] === previous[name])) return null;
    const from = current?.from ?? cell.original.length;
    const to = current?.to ?? from;
    const insert = tableAttributes.suffix(attributes, from > 0);
    if (insert === null || cell.original.slice(from, to) === insert ||
        context.to - context.from - (to - from) + insert.length > 65536) return null;
    const updated = cell.original.slice(0, from) + insert + cell.original.slice(to);
    const parsed = tableAttributes.parseCell(updated);
    // Removing metadata must not activate an earlier, literal brace suffix.
    if (!insert && parsed) return null;
    const rows = context.rows.map(row => row.map(slot => slot === cell ? updated : slot.original));
    if (!tableAttributes.layout(rows)) return null;
    return { from: cell.from + from, to: cell.from + to, insert };
  }

  function rectangle(context, row, column, rowspan, colspan) {
    return context.rows.slice(row, row + rowspan)
      .flatMap(cells => cells.slice(column, column + colspan))
      .map(({ segmentFrom: from, segmentTo: to }) => ({ from, to }));
  }

  function mergeCells(context, cell, rowspan, colspan) {
    const { row, column } = cell;
    if (context.layout.owners[row][column] || rowspan === 1 && colspan === 1) return null;
    const attrs = { ...context.layout.cells[row][column]?.attributes };
    if (colspan !== 1) attrs.colspan = colspan;
    if (rowspan !== 1) attrs.rowspan = rowspan;
    const edit = attributeEdit(context, cell, attrs);
    return edit && { ...edit, affected: rectangle(context, row, column, rowspan, colspan) };
  }

  function splitCell(context, cell) {
    const owner = context.layout.owners[cell.row][cell.column];
    if (!owner) return null;
    const { row, column, rowspan, colspan } = owner;
    const { align, valign } = context.layout.cells[row][column].attributes;
    const attrs = {};
    if (align) attrs.align = align;
    if (valign) attrs.valign = valign;
    const edit = attributeEdit(context, context.rows[row][column], attrs);
    return edit && { ...edit, affected: rectangle(context, row, column, rowspan, colspan) };
  }

  function alignCell(context, cell, align, valign) {
    const attrs = { ...context.layout.cells[cell.row][cell.column]?.attributes };
    if (align) attrs.align = align;
    else delete attrs.align;
    if (valign) attrs.valign = valign;
    else delete attrs.valign;
    const edit = attributeEdit(context, cell, attrs);
    return edit && { ...edit, affected: rectangle(context, cell.row, cell.column, 1, 1) };
  }

  return { columnEdit, sortRows, mergeCells, splitCell, alignCell };
});
