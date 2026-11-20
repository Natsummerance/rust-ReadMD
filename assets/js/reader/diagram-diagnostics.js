'use strict';
// Reader-only checks. Never execute object literals or change document source.
(function(root, factory) {
  if (typeof module !== 'undefined' && module.exports) module.exports = factory();
  else root.ReadMDDiagramDiagnostics = factory();
})(typeof window !== 'undefined' ? window : globalThis, () => {
  const MAX_SOURCE = 2 * 1024 * 1024;
  const aliases = { chartjs: 'chart', 'chart.js': 'chart', viz: 'graphviz', dot: 'graphviz', puml: 'plantuml' };
  const engineName = name => aliases[String(name || '').toLowerCase()] || String(name || '').toLowerCase();
  const jsonEngines = new Set(['chart', 'vega', 'vega-lite', 'wavedrom', 'bitfield']);
  const errorKeys = {
    diagram_engine_unavailable: 'reader.diagramEngineUnavailable',
    diagram_engine_load_failed: 'reader.diagramEngineLoadFailed',
    diagram_engine_timeout: 'reader.diagramTimeout',
    diagram_engine_unavailable_handler: 'reader.diagramEngineUnavailable',
    diagram_engine_unavailable_rendered: 'reader.diagramEngineUnavailable',
    diagram_invalid_input: 'reader.diagramInvalidInput',
    diagram_dependency_missing: 'reader.diagramDependencyMissing',
    diagram_client_renderer_required: 'reader.diagramClientRendererRequired',
    diagram_network_unavailable: 'reader.diagramNetworkUnavailable',
    diagram_input_too_large: 'reader.diagramInputTooLarge',
    diagram_render_failed: 'reader.diagramRenderFailed',
  };
  function errorKey(code) {
    return Object.prototype.hasOwnProperty.call(errorKeys, code) ? errorKeys[code] : 'toast.unknownError';
  }
  function location(source, offset) {
    const before = source.slice(0, Math.max(0, Math.min(source.length, offset)));
    const lines = before.split(/\r\n|\r|\n/);
    return { line: lines.length, column: lines[lines.length - 1].length + 1 };
  }
  function fieldPath(path, key) {
    return /^[A-Za-z_$][\w$-]*$/.test(key) ? path + '.' + key : path + '[' + JSON.stringify(key) + ']';
  }
  // This small JSON scanner supplies positions even in WebKit, whose
  // JSON.parse errors do not consistently contain an offset. Loose mode is
  // restricted to punctuation common in WaveDrom examples; no eval/Function.
  function scanJson(source, loose = false, targetPath = null) {
    source = String(source ?? '');
    let offset = 0, depth = 0;
    const edits = [], positions = new Map();
    const keyPattern = /[A-Za-z_$][\w$-]*/y;
    const valuePattern = /(?:true|false|null|-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?)/y;
    function fail(expected, at = offset, key = 'reader.diagramExpectedField') {
      const error = new SyntaxError('diagram_invalid_input');
      error.diagramDiagnostic = { code: 'diagram_invalid_input', category: 'syntax',
        messageKey: key, messageParams: { field: 'JSON', expected },
        offset: at, ...location(source, at) };
      throw error;
    }
    function skip() {
      for (;;) {
        while (/[\x20\t\r\n]/.test(source[offset] || '\0')) offset++;
        if (loose && offset === 0 && source.charCodeAt(offset) === 0xfeff) { edits.push({ from: offset, to: ++offset, text: '' }); continue; }
        if (!loose || source[offset] !== '/') return;
        const start = offset;
        if (source[offset + 1] === '/') {
          offset += 2;
          while (offset < source.length && !/[\r\n]/.test(source[offset])) offset++;
        } else if (source[offset + 1] === '*') {
          const end = source.indexOf('*/', offset + 2);
          if (end < 0) fail('"*/"', source.length);
          offset = end + 2;
        } else return;
        // Whitespace preserves token boundaries and all line positions.
        edits.push({ from: start, to: offset, text: source.slice(start, offset).replace(/[^\r\n]/g, ' ') });
      }
    }
    function string() {
      const start = offset, quote = source[offset++];
      if (quote !== '"' && (!loose || quote !== "'")) fail('JSON string', start);
      let value = '';
      while (offset < source.length) {
        const char = source[offset++];
        if (char === quote) {
          if (quote !== '"') edits.push({ from: start, to: offset, text: JSON.stringify(value) });
          return value;
        }
        if (char.charCodeAt(0) < 0x20) fail('JSON string escape', offset - 1);
        if (char !== '\\') { value += char; continue; }
        const escapeAt = offset - 1, escaped = source[offset++];
        const escapes = { '"': '"', '\\': '\\', '/': '/', b: '\b', f: '\f', n: '\n', r: '\r', t: '\t' };
        if (quote === "'" && escaped === "'") { value += "'"; continue; }
        if (Object.prototype.hasOwnProperty.call(escapes, escaped)) { value += escapes[escaped]; continue; }
        if (escaped === 'u' && /^[0-9a-f]{4}$/i.test(source.slice(offset, offset + 4))) {
          value += String.fromCharCode(parseInt(source.slice(offset, offset + 4), 16)); offset += 4; continue;
        }
        fail('JSON string escape', escapeAt);
      }
      fail(quote, source.length);
    }
    function value(path) {
      skip(); if (path === '$' || path === targetPath) positions.set(path, offset);
      if (++depth > 128) fail('JSON depth ≤ 128');
      const char = source[offset];
      if (char === '{' || char === '[') {
        const object = char === '{', closing = object ? '}' : ']';
        offset++; skip();
        let index = 0;
        if (source[offset] === closing) offset++;
        else for (;;) {
          let key = index;
          if (object) {
            const keyAt = offset;
            if (source[offset] === '"' || loose && source[offset] === "'") key = string();
            else if (loose) {
              keyPattern.lastIndex = offset;
              const match = keyPattern.exec(source);
              if (!match) fail('JSON string');
              key = match[0]; offset += key.length; edits.push({ from: keyAt, to: offset, text: JSON.stringify(key) });
            } else fail('JSON string');
            skip(); if (source[offset] !== ':') fail('":"'); offset++;
          }
          value(object ? fieldPath(path, key) : path + '[' + index + ']');
          index++; skip();
          if (source[offset] === closing) { offset++; break; }
          if (source[offset] !== ',') fail('"," / "' + closing + '"');
          const comma = offset++; skip();
          if (source[offset] === closing) {
            if (!loose) fail(closing, comma, 'reader.diagramTrailingComma');
            edits.push({ from: comma, to: comma + 1, text: '' }); offset++; break;
          }
        }
      } else if (char === '"' || loose && char === "'") string();
      else {
        valuePattern.lastIndex = offset;
        const match = valuePattern.exec(source);
        if (!match) fail('JSON value');
        offset += match[0].length;
      }
      depth--;
    }
    value('$'); skip();
    if (offset !== source.length) fail('JSON EOF');
    const parts = []; let cursor = 0;
    edits.sort((a, b) => a.from - b.from).forEach(edit => { parts.push(source.slice(cursor, edit.from), edit.text); cursor = edit.to; });
    parts.push(source.slice(cursor));
    const normalized = edits.length ? parts.join('') : source;
    return { value: JSON.parse(normalized), normalized, changed: edits.length > 0, positions };
  }
  function parseJson(source, loose = false) {
    try { return JSON.parse(String(source ?? '')); }
    catch (error) { if (loose) return scanJson(source, true).value; throw error; }
  }
  function safeDetail(value) {
    let text = typeof value === 'string' ? value.slice(0, 4096) : '';
    // Select explanatory lines, never parser source excerpts or stack frames.
    text = text.split(/\r?\n/).filter(line => line.trim() && !/^\s*(?:at\s|\.\.\.|[-~]*\^|>\s*\d|\d+\s*[|:])/.test(line))
      .filter((line, index) => index === 0 || /^(?:expecting\b|expected\b|got\b|error\b|syntax\b|!\s)/i.test(line.trim())).slice(0, 3).join('\n');
    return text.replace(/\b(?:https?|file|wss?):\/\/[^\s<>"']+/gi, '[URL]')
      .replace(/(["'])(?:[A-Za-z]:[\\/]|\/)[^\r\n]*?\1/g, '"[path]"')
      .replace(/(?:\b[A-Za-z]:[\\/]|\/(?:Users|home|private|tmp|var|Volumes|opt|etc|workspace|app|root)\/)[^\s<>"']*/g, '[path]')
      .replace(/(?:\\\\[^\s\\]+\\[^\s<>"']+|\/(?:[^/\s<>"'():,;=]+\/)+[^\s<>"'(),;]*)/g, '[path]')
      .replace(/(?:Bearer\s+)[^\s,;"']+/gi, 'Bearer [redacted]')
      .replace(/((?:api[_-]?key|access[_-]?token|refresh[_-]?token|token|authorization|password|secret)["']?\s*[=:]\s*)(?:"[^"\n]*"|'[^'\n]*'|[^\s,;\]}]+)/gi, '$1[redacted]')
      .replace(/\b(?:sk-|gh[pousr]_|github_pat_|xox[baprs]-)[A-Za-z0-9_-]{8,}\b/g, '[redacted]')
      .replace(/\beyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\b/g, '[redacted]')
      .replace(/[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]/g, '').slice(0, 520);
  }
  function failure(engine, code, key, params = {}, extra = {}) {
    return { engine, code, category: code === 'diagram_invalid_input' ? 'syntax' : /network/.test(code) ? 'network' : /timeout/.test(code) ? 'timeout' : /engine|dependency|renderer/.test(code) ? 'environment' : 'render',
      messageKey: key || errorKeys[code] || errorKeys.diagram_render_failed, messageParams: params,
      line: null, column: null, detail: '', fix: null, ...extra };
  }
  function liteTransforms(spec, path = '$', result = [], depth = 0) {
    if (!spec || typeof spec !== 'object' || Array.isArray(spec) || depth > 128) return result;
    if (Array.isArray(spec.transform)) spec.transform.forEach((value, index) => result.push({ value, path: path + '.transform[' + index + ']' }));
    for (const key of ['layer', 'concat', 'hconcat', 'vconcat']) {
      if (Array.isArray(spec[key])) spec[key].forEach((child, index) => liteTransforms(child, path + '.' + key + '[' + index + ']', result, depth + 1));
    }
    if (spec.spec) liteTransforms(spec.spec, path + '.spec', result, depth + 1);
    return result;
  }
  function invalidTransform(source, warning) {
    let path = '$.transform';
    try {
      const message = String(warning || '');
      const match = message.length < 32768 && /^Ignoring an invalid transform:\s*([\s\S]+)\.$/.exec(message);
      if (match) {
        const canonical = value => JSON.stringify(value, (_key, item) => item && typeof item === 'object' && !Array.isArray(item)
          ? Object.keys(item).sort().reduce((row, key) => { Object.defineProperty(row, key, { value: item[key], enumerable: true }); return row; }, {}) : item);
        const invalid = canonical(JSON.parse(match[1]));
        const row = liteTransforms(JSON.parse(source)).find(row => canonical(row.value) === invalid);
        if (row) path = row.path;
      }
    } catch (_) { /* Compiler wording is not a stable source locator. */ }
    let position = { line: null, column: null };
    try { const at = scanJson(source, false, path).positions.get(path); if (at != null) position = location(source, at); } catch (_) { /* no invented location */ }
    return failure('vega-lite', 'diagram_invalid_input', 'reader.diagramExpectedField', { field: path, expected: 'Vega-Lite transform' },
      { field: path, detail: 'Ignoring an invalid transform.', ...position });
  }
  function shapeDiagnostic(engine, source, parsed) {
    const spec = parsed.value, object = value => value && typeof value === 'object' && !Array.isArray(value);
    const at = path => {
      // A valid chart normally needs no scanner or source-position index.
      // Locate just the failing structural field when there is an error.
      try { return location(source, scanJson(source, engine === 'wavedrom' || engine === 'bitfield', path).positions.get(path) ?? 0); }
      catch (_) { return { line: null, column: null }; }
    };
    const expected = (path, type) => failure(engine, 'diagram_invalid_input', 'reader.diagramExpectedField', { field: path, expected: type }, { field: path, ...at(path) });
    const missing = (path, owner = '$') => failure(engine, 'diagram_invalid_input', 'reader.diagramMissingField', { field: path }, { field: path, ...at(owner) });
    if (engine === 'bitfield') {
      const fields = Array.isArray(spec) ? spec : spec?.reg;
      if (!Array.isArray(fields)) return expected('$ / $.reg', 'array');
      for (let i = 0; i < fields.length; i++) {
        const path = (Array.isArray(spec) ? '$' : '$.reg') + '[' + i + ']';
        if (!object(fields[i])) return expected(path, 'object');
        if (!Number.isInteger(fields[i].bits) || fields[i].bits < 1) return expected(path + '.bits', 'integer ≥ 1');
      }
      return null;
    }
    if (!object(spec)) return expected('$', 'object');
    if (engine === 'wavedrom') {
      if (spec.signal != null && !Array.isArray(spec.signal)) return expected('$.signal', 'array');
      // WaveDrom also accepts assign diagrams; the renderer owns that grammar.
      if (spec.signal == null && spec.assign == null) return missing('$.signal / $.assign');
    }
    if (engine === 'chart') {
      if (spec.type != null && typeof spec.type !== 'string') return expected('$.type', 'string');
      if (spec.data != null && !object(spec.data)) return expected('$.data', 'object');
      if (spec.data?.datasets != null && !Array.isArray(spec.data.datasets)) return expected('$.data.datasets', 'array');
      if (spec.options != null && !object(spec.options)) return expected('$.options', 'object');
      const sets = spec.data?.datasets || [];
      for (let i = 0; i < sets.length; i++) {
        const path = '$.data.datasets[' + i + ']';
        if (!object(sets[i])) return expected(path, 'object');
        if (!spec.type && !sets[i].type) return missing('$.type / ' + path + '.type', path);
        if (sets[i].data != null && !Array.isArray(sets[i].data) && !object(sets[i].data)) return expected(path + '.data', 'array / object');
      }
    }
    if (engine === 'vega' || engine === 'vega-lite') {
      if (engine === 'vega-lite') {
        const transforms = liteTransforms(spec);
        const broken = transforms.filter(row => row.value && row.value.type === 'regression' && row.value.regression == null);
        if (broken.length) {
          const diagnostic = missing(broken[0].path + '.regression', broken[0].path + '.type');
          diagnostic.detail = 'Vega-Lite: {"regression":"Y","on":"X"}';
          if (broken.every(row => typeof row.value.on === 'string' && row.value.on && typeof row.value.y === 'string' && row.value.y)) {
            // The Vega transform spelling is a common copy/paste mistake.
            // The author supplied both field roles, so the migration is exact.
            const copy = JSON.parse(source);
            for (const row of liteTransforms(copy)) if (row.value?.type === 'regression' && row.value.regression == null) {
              row.value.regression = row.value.y; delete row.value.type; delete row.value.y;
            }
            diagnostic.fix = { code: JSON.stringify(copy, null, 2), kind: 'vega-lite-regression', labelKey: 'reader.diagramLocalFix' };
          }
          return diagnostic;
        }
      }
      // Vega's loader is intentionally offline. Report the actual field before
      // the engine emits a generic failed-to-load/empty-data error.
      function remote(value, path, depth = 0) {
        if (!value || typeof value !== 'object' || depth > 128) return null;
        if (object(value.data) && typeof value.data.url === 'string') return path + '.data.url';
        if (engine === 'vega' && Array.isArray(value.data)) {
          const index = value.data.findIndex(row => row && row.url != null);
          if (index >= 0) return path + '.data[' + index + '].url';
        }
        for (const [key, child] of Object.entries(value)) {
          if (['values', 'datasets', 'usermeta'].includes(key)) continue;
          if (Array.isArray(child)) { for (let i = 0; i < child.length; i++) { const result = remote(child[i], fieldPath(path, key) + '[' + i + ']', depth + 1); if (result) return result; } }
          else if (object(child)) { const result = remote(child, fieldPath(path, key), depth + 1); if (result) return result; }
        }
        return null;
      }
      const path = remote(spec, '$');
      if (path) return failure(engine, 'diagram_invalid_input', 'reader.diagramExternalData', { field: path }, { category: 'capability', field: path, ...at(path) });
    }
    return null;
  }
  function inspect(engine, source, error) {
    engine = engineName(engine); source = String(source ?? '');
    if (error?.diagramDiagnostic) return failure(engine, error.diagramDiagnostic.code || 'diagram_render_failed', null, {}, error.diagramDiagnostic);
    if (!source.trim()) return failure(engine, 'diagram_invalid_input', 'audit.codeRequired', {}, { line: 1, column: 1 });
    if (source.length > MAX_SOURCE || typeof TextEncoder !== 'undefined' && new TextEncoder().encode(source).length > MAX_SOURCE) return failure(engine, 'diagram_input_too_large');
    if (jsonEngines.has(engine)) {
      let parsed;
      try { parsed = { value: parseJson(source, engine === 'wavedrom' || engine === 'bitfield') }; }
      catch (parseError) {
        // Native errors differ across WebKit/Chromium. Scan only failures to
        // supply a consistent exact location, without copying data-array paths.
        if (!parseError.diagramDiagnostic) { try { scanJson(source); } catch (precise) { parseError = precise; } }
        const diagnostic = failure(engine, 'diagram_invalid_input', null, {}, parseError.diagramDiagnostic || {});
        if (engine !== 'wavedrom' && engine !== 'bitfield') {
          try {
            const corrected = scanJson(source, true);
            if (corrected.changed && !shapeDiagnostic(engine, corrected.normalized, corrected)) diagnostic.fix = { code: corrected.normalized, kind: 'json-punctuation', labelKey: 'reader.diagramLocalFix' };
          } catch (_) { /* A repair is offered only when it fully parses. */ }
        }
        return diagnostic;
      }
      const invalid = shapeDiagnostic(engine, source, parsed);
      if (invalid) return invalid;
    }
    if (!error) return null;
    const rawCode = error.error_code || error.code || (typeof error === 'string' ? error : error.message);
    const code = Object.prototype.hasOwnProperty.call(errorKeys, rawCode) ? rawCode : 'diagram_render_failed';
    const diagnostic = failure(engine, code);
    const raw = error.diagramDetail || error.detail || error.message || (typeof error === 'string' ? error : '');
    if (raw && raw !== code && !Object.prototype.hasOwnProperty.call(errorKeys, raw)) diagnostic.detail = safeDetail(raw);
    const messageLine = /(?:\bon line\s+|\bin line\s+|\bline\s*[:=]?\s*)(\d+)(?:\s*[,;:]?\s*(?:column|col)\s*[:=]?\s*(\d+))?/i.exec(raw || '');
    const loc = error.hash?.loc || error.loc || error.location;
    const suppliedLine = Number(error.line ?? error.lineNumber ?? loc?.line ?? loc?.start?.line ?? loc?.first_line);
    const suppliedColumn = Number(error.column ?? error.columnNumber ?? loc?.column ?? loc?.start?.column ?? (Number.isInteger(loc?.first_column) ? loc.first_column + 1 : undefined));
    if (messageLine) {
      diagnostic.line = Number(messageLine[1]);
      diagnostic.column = messageLine[2] ? Number(messageLine[2]) : suppliedLine === diagnostic.line && suppliedColumn > 0 ? suppliedColumn : null;
    } else if (suppliedLine > 0) { diagnostic.line = suppliedLine; diagnostic.column = suppliedColumn > 0 ? suppliedColumn : null; }
    if (diagnostic.line > source.split(/\r\n|\r|\n/).length) { diagnostic.line = null; diagnostic.column = null; }
    if (diagnostic.line && diagnostic.detail) diagnostic.category = 'syntax';
    return diagnostic;
  }
  function asError(diagnostic) {
    return Object.assign(new Error(diagnostic.code || 'diagram_render_failed'), { diagramDiagnostic: diagnostic });
  }
  return { inspect, errorKey, parseJson, safeDetail, asError, location, invalidTransform };
});
