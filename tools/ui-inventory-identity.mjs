// Historical inventory line numbers are citations, not stable identities.
// Validate the actual element/route/command so insertions cannot disable the
// gate and a nonempty but unrelated line cannot hide a removed feature.
export function locateControl(text, control) {
  const tags = new RegExp(`<${control.tag}\\b(?:"[^"]*"|'[^']*'|[^'">])*>`, 'g');
  for (const match of text.matchAll(tags)) {
    const attrs = Object.fromEntries([...match[0].matchAll(/([\w:-]+)\s*=\s*(?:"([^"]*)"|'([^']*)')/g)].map(m => [m[1], m[2] ?? m[3]]));
    if (control.id ? attrs.id === control.id : Object.entries(control.attrs || {}).every(([k, v]) =>
      k === 'class' ? String(v).split(/\s+/).every(token => (attrs.class || '').split(/\s+/).includes(token)) : attrs[k] === v)) {
      return text.slice(0, match.index).split('\n').length;
    }
  }
  return 0;
}

export function hasDeclaration(text, value, field) {
  const escaped = String(value).replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  return new RegExp(field ? `\\b${field}\\s*:\\s*['"]${escaped}['"]` : `['"]${escaped}['"]`).test(text);
}
