/* Document inheritance stays out of Last and user presets. One session owns
   sparse explicit edits; only requests receive the effective source values. */
(() => {
  'use strict';
  const clone = value => JSON.parse(JSON.stringify(value || {}));
  const common = ['size','sizeCs','lineHeight','lineSpacing','numberFormat','numberPattern','start','restart','placement','separator','continuationSeparator','continuationNotice','longNote'];
  const object = value => value && typeof value === 'object' && !Array.isArray(value);
  const get = (value, key) => key.split('.').reduce((node, k) => node != null && Object.hasOwn(node,k) ? node[k] : undefined, value);
  function overlay(base, over) {
    const result = clone(base);
    for (const [key, value] of Object.entries(over || {})) {
      if (value == null) continue;
      const next = object(value) && Object.hasOwn(result,key) && object(result[key]) ? overlay(result[key], value) : cloneScalar(value);
      Object.defineProperty(result,key,{value:next,enumerable:true,writable:true,configurable:true});
    }
    return result;
  }
  const cloneScalar = value => JSON.parse(JSON.stringify(value));
  function patchPolicy(base, over) {
    const out = overlay(base, over);
    if (over?.lineSpacing != null) {out.lineSpacing = cloneScalar(over.lineSpacing); delete out.lineHeight;}
    else if (over?.lineHeight != null) delete out.lineSpacing;
    return out;
  }
  function mergeNotes(...layers) {
    let result = {}, kinds = {};
    const expanded = layers.some(layer => object(layer?.kinds));
    for (const layer of layers) {
      if (!object(layer)) continue;
      const flat = Object.fromEntries(common.filter(key => layer[key] != null).map(key => [key, layer[key]]));
      result = patchPolicy(result, layer);
      if (expanded) for (const kind of ['footnote','endnote']) {
        kinds[kind] = patchPolicy(patchPolicy(kinds[kind] || {}, flat), layer.kinds?.[kind]);
      }
    }
    if (expanded) result.kinds = Object.fromEntries(Object.entries(kinds).filter(([, value]) => Object.keys(value).length));
    return result;
  }
  function create(sourceOptions, rememberedOptions) {
    const source = clone(sourceOptions?.footnotes);
    let fallback = clone(rememberedOptions?.footnotes);
    let explicit = {}, selectedKind;
    const effective = () => mergeNotes(fallback, source, explicit);
    const remembered = () => mergeNotes(fallback, explicit);
    const preferredKind = notes => Object.hasOwn(notes.kinds || {},'footnote') ? 'footnote'
      : Object.hasOwn(notes.kinds || {},'endnote') || notes.position === 'document' ? 'endnote' : 'footnote';
    selectedKind = preferredKind(effective());
    const policy = notes => patchPolicy(Object.fromEntries(common.filter(key => notes[key] != null).map(key => [key, notes[key]])), notes.kinds?.[selectedKind]);
    return Object.freeze({
      hasSource: Object.keys(source).length > 0,
      get hasKinds() {return !!object(effective().kinds);},
      hasBindings: Object.keys(source.kindByLabel || {}).length > 0,
      effective, remembered,
      kind: () => selectedKind,
      selectKind(kind) {if (!['footnote','endnote'].includes(kind)) throw Error('publication_note_policy_invalid'); selectedKind = kind;},
      selectPreset(options) {
        fallback = {}; explicit = clone(options?.footnotes);
        const notes = effective();
        if (!Object.hasOwn(notes.kinds || {},selectedKind)) selectedKind = preferredKind(notes);
      },
      controlKey(key) {
        const name = key.slice('footnotes.'.length);
        return object(effective().kinds) && common.includes(name) ? `footnotes.kinds.${selectedKind}.${name}` : key;
      },
      display(key) {
        const notes = effective(), name = key.slice('footnotes.'.length);
        return common.includes(name) && object(notes.kinds) ? get(policy(notes), name) : get(notes, name);
      },
      change(key, value) {
        const path = this.controlKey(key).split('.').slice(1); let current = explicit;
        for (const name of path.slice(0,-1)) {if (!object(current[name])) current[name] = {}; current = current[name];}
        const name = path.at(-1); current[name] = cloneScalar(value);
        if (name === 'lineHeight') delete current.lineSpacing;
        if (name === 'lineSpacing') delete current.lineHeight;
      },
      collect(options) {
        const result = clone(options), notes = remembered(); delete result.footnotes;
        if (Object.keys(notes).length) result.footnotes = notes;
        return result;
      },
      request(options) {
        const result = clone(options), notes = effective(); delete result.footnotes;
        if (Object.keys(notes).length) result.footnotes = notes;
        return result;
      },
    });
  }
  window.ReadMDPublicationProfile = Object.freeze({create, mergeNotes});
})();
