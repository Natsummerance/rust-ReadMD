'use strict';
(function (root, factory) {
  if (typeof module !== 'undefined' && module.exports) module.exports = factory();
  else root.ReadMDTableAttributes = factory();
})(typeof window !== 'undefined' ? window : null, () => {
  const maxColumns = 16, maxRows = 32;
  const dimensions = Object.freeze({
    width: Object.freeze({ min: 32, max: 1200 }),
    height: Object.freeze({ min: 20, max: 600 }),
  });
  const keys = ['colspan', 'rowspan', 'align', 'valign', 'width', 'height'];
  const recognized = /(?:^|[ \t])(?:colspan|rowspan|align|valign|width|height)=/;
  const validDimension = (axis, value) => {
    const bounds = dimensions[axis];
    return Boolean(bounds && Number.isInteger(value) && value >= bounds.min && value <= bounds.max);
  };
  const valid = {
    colspan: value => Number.isInteger(value) && value >= 1 && value <= maxColumns,
    rowspan: value => Number.isInteger(value) && value >= 1 && value <= maxRows,
    align: value => ['left', 'center', 'right'].includes(value),
    valign: value => ['top', 'middle', 'bottom'].includes(value),
    width: value => validDimension('width', value),
    height: value => validDimension('height', value),
  };

  // Cell text is Markdown source. Remove precisely the one space that belongs
  // to the suffix, even if more spaces precede it in the original cell.
  function parseCell(source) {
    const match = /\{([^{}\r\n]*)\}([ \t]*)$/.exec(source);
    if (!match || !recognized.test(match[1])) return null;
    const prefix = source.slice(0, match.index);
    const hasText = !/^[ \t]*$/.test(prefix);
    if (hasText && !prefix.endsWith(' ')) return null;
    const beforeBrace = match[1].match(/\\*$/)[0].length;
    if (beforeBrace % 2) return null;
    const attributes = {};
    for (const assignment of match[1].split(/ +/)) {
      const pair = /^(colspan|rowspan|align|valign|width|height)=([a-z0-9]+)$/.exec(assignment);
      if (!pair || Object.hasOwn(attributes, pair[1])) return { invalid: true };
      const [, name, raw] = pair;
      const numeric = name.endsWith('span') || Object.hasOwn(dimensions, name);
      const value = numeric && /^[1-9][0-9]*$/.test(raw) ? Number(raw) : raw;
      if (!valid[name](value)) return { invalid: true };
      attributes[name] = value;
    }
    const from = match.index - (hasText ? 1 : 0);
    return { from, to: source.length - match[2].length, text: source.slice(0, from) + match[2], attributes };
  }

  // Owners are shared by all slots of a rectangle, including its origin.
  // Alignment alone leaves a slot free for a span and survives a later split.
  function layout(rows) {
    const width = rows[0]?.length;
    if (!width || rows.some(row => row.length !== width)) return null;
    const cells = rows.map(row => row.map(parseCell));
    const hasAttributes = cells.some(row => row.some(Boolean));
    const owners = cells.map(row => row.map(() => null));
    let hasSpans = false;
    for (let row = 0; row < cells.length; row++) {
      for (let column = 0; column < width; column++) {
        const cell = cells[row][column];
        if (!cell) continue;
        if (cell.invalid) return null;
        const { rowspan = 1, colspan = 1, width: authoredWidth, height } = cell.attributes;
        if (row > 0 && authoredWidth !== undefined || column > 0 && height !== undefined) return null;
        if (rowspan === 1 && colspan === 1) continue;
        if (row === 0 && rowspan > 1 || row + rowspan > rows.length || column + colspan > width) return null;
        hasSpans = true;
        const owner = { row, column, rowspan, colspan };
        for (let r = row; r < row + rowspan; r++) {
          for (let c = column; c < column + colspan; c++) {
            if (owners[r][c]) return null;
            owners[r][c] = owner;
          }
        }
      }
    }
    const widths = cells[0].map(cell => cell?.attributes.width ?? null);
    const heights = cells.map(row => row[0]?.attributes.height ?? null);
    return { cells, owners, hasAttributes, hasSpans, widths, heights };
  }

  function suffix(attributes, afterText) {
    const names = Object.keys(attributes);
    if (names.some(name => !keys.includes(name) || !valid[name](attributes[name]))) return null;
    if (!names.length) return '';
    return (afterText ? ' ' : '') + '{' + keys.filter(name => Object.hasOwn(attributes, name))
      .map(name => name + '=' + attributes[name]).join(' ') + '}';
  }

  return { parseCell, layout, suffix, maxColumns, maxRows, dimensions, validDimension };
});
