/* A disposable opaque-origin sandbox. Document scripts never enter this frame.
 * Vega's expression compiler and WASM stay isolated from the parent webview. */
(() => {
  'use strict';
  const base = new URL('./vendor/diagrams/', document.baseURI);
  const output = document.getElementById('output');
  let started = false;
  const load = file => new Promise((resolve, reject) => {
    const script = document.createElement('script'); script.src = new URL(file, base).href;
    script.onload = resolve; script.onerror = () => reject(new Error('load')); document.head.append(script);
  });
  function json(source) {
    try { return JSON.parse(source); } catch {}
    return JSON.parse(source.replace(/([{,]\s*)([A-Za-z_$][\w$-]*)\s*:/g, '$1"$2":')
      .replace(/'([^'\\]*(?:\\.[^'\\]*)*)'/g, (_, value) => '"' + value.replace(/"/g, '\\"') + '"')
      .replace(/,\s*([}\]])/g, '$1'));
  }
  function tree(node) {
    if (node == null || node === false) return document.createTextNode('');
    if (!Array.isArray(node)) return document.createTextNode(String(node));
    const element = document.createElementNS('http://www.w3.org/2000/svg', node[0]);
    let offset = 1;
    if (node[1] && typeof node[1] === 'object' && !Array.isArray(node[1])) {
      for (const [key, value] of Object.entries(node[1])) element.setAttribute(key, String(value)); offset = 2;
    }
    for (const child of node.slice(offset)) element.append(tree(child));
    return element;
  }
  function offlineSpec(value) {
    if (!value || typeof value !== 'object') return;
    for (const [key, child] of Object.entries(value)) {
      if (key === 'url' || key === 'href') throw new Error('external-data');
      offlineSpec(child);
    }
  }
  async function render(engine, code) {
    if (engine === 'wavedrom') {
      await load('wavedrom/skins/default.js'); await load('wavedrom/skins/narrow.js'); await load('wavedrom/wavedrom.min.js');
      output.id = 'wave-output-0'; WaveDrom.RenderWaveForm(0, json(code), 'wave-output-', false);
    } else if (engine === 'bitfield') {
      await load('bitfield/bitfield.min.js'); let spec = json(code); spec = spec.reg || spec;
      if (!Array.isArray(spec)) throw new Error('input');
      const rendered = bitfield.render(spec, {}); output.append(rendered?.outerHTML ? rendered : tree(rendered));
    } else if (['graphviz', 'dot', 'viz'].includes(engine)) {
      await load('viz/viz-standalone.js'); const instance = await Viz.instance();
      output.innerHTML = await instance.renderString(code, { engine: 'dot', format: 'svg' });
    } else if (['vega', 'vega-lite'].includes(engine)) {
      await load('vega/vega.min.js'); let spec = json(code); offlineSpec(spec);
      if (engine === 'vega-lite') { await load('vega-lite/vega-lite.min.js'); spec = vegaLite.compile(spec).spec; }
      const view = new vega.View(vega.parse(spec), { renderer: 'none' });
      try { output.innerHTML = await view.toSVG(); } finally { view.finalize(); }
    } else if (['chart', 'chartjs', 'chart.js'].includes(engine)) {
      await load('chart/chart.umd.js'); const spec = json(code); offlineSpec(spec);
      const canvas = document.createElement('canvas'); canvas.width = 800; canvas.height = 450; output.append(canvas);
      spec.options = { ...(spec.options || {}), responsive: false, animation: false };
      const chart = new Chart(canvas, spec); const png = canvas.toDataURL('image/png'); chart.destroy(); return { png };
    } else if (engine === 'tikz') {
      const originalFetch = window.fetch;
      window.fetch = (input, init) => {
        const url = typeof input === 'string' ? input : input.url;
        if (!url.startsWith('https://s3.us-east-2.amazonaws.com/tikzjax.com/')) throw new Error('external-data');
        return originalFetch(new URL('tikzjax/' + url.slice(url.lastIndexOf('/') + 1), base), { ...init, credentials: 'omit' });
      };
      await load('tikzjax/tikzjax.js');
      const source = document.createElement('script'); source.type = 'text/tikz';
      source.textContent = /\\begin\{(?:document|tikzpicture)\}/.test(code) ? code : '\\begin{tikzpicture}\n' + code + '\n\\end{tikzpicture}';
      output.append(source); window.onload?.call(window);
      const deadline = Date.now() + 45000;
      while (!output.querySelector('svg') && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 100));
    } else throw new Error('engine');
    const svg = output.querySelector('svg'); if (!svg) throw new Error('input');
    svg.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
    return { svg: new XMLSerializer().serializeToString(svg) };
  }
  window.addEventListener('message', async event => {
    if (started || event.source !== parent || event.data?.type !== 'renderDiagram') return;
    started = true;
    const { token, engine, code } = event.data;
    try { parent.postMessage({ type: 'diagramResult', token, ...(await render(engine, String(code))) }, '*'); }
    catch (error) { parent.postMessage({ type: 'diagramResult', token, error: true }, '*'); }
  });
})();
