'use strict';
/* ============================================================
   ReadMD Features - Document Export Console & High-Fidelity Preview
   ============================================================ */

/* ---------------- 导出面板（PDF / DOCX / HTML + 样式定制） ---------------- */

const EXPORT_FONTS = ['MicrosoftYaHei', 'SimHei', 'SimSun', 'KaiTi', 'DengXian', 'Arial'];
const EXPORT_MONO = ['Consolas', 'Courier New', 'SimHei'];
const EXPORT_ALIGNS = ['left', 'center', 'right', 'justify'];
const EXPORT_PAGES = ['A3', 'A4', 'A5', 'B5', 'Letter', 'Legal', 'Custom'];

function getExportPresetNames() {
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  return {
    minimal: _t('export.presetMinimal') || '',
    classic: _t('export.presetClassic') || '',
    business: _t('export.presetBusiness') || '',
    ...Object.fromEntries((state.export.catalog || []).map(p=>[p.id,_t(p.label)]))
  };
}

function getExportSections() {
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  const sections = [
    /* --- EPUB 电子书专属参数 --- */
    { title: _t('export.secEpubMeta') || '', fmts: ['epub'], fields: [
      { k: 'epub.title', label: _t('export.epubTitle') || '', type: 'text', full: true },
      { k: 'epub.author', label: _t('export.epubAuthor') || '', type: 'text' },
      { k: 'epub.publisher', label: _t('export.epubPublisher') || '', type: 'text' },
      { k: 'epub.isbn', label: _t('export.epubIsbn') || '', type: 'text' },
      { k: 'epub.cover', label: _t('export.coverPage') + ' · ' + _t('editor.image'), type: 'asset', full: true },
      { k: 'epub.language', label: _t('export.epubLanguage') || '', type: 'select', default: '', opts: [
        ['', _t('pm.inherit')],
        ['zh-CN', '简体中文 (zh-CN)'], ['en', 'English (en)'], ['ja', '日本語 (ja)'], ['zh-TW', '繁體中文 (zh-TW)'], ['fr', 'Français (fr)'], ['de', 'Deutsch (de)'], ['es', 'Español (es)']
      ]},
      { k: 'epub.splitLevel', label: _t('export.epubSplitLevel') || '', type: 'select', opts: [
        ['h1', _t('export.epubSplitH1') || ''],
        ['h2', _t('export.epubSplitH2') || ''],
        ['none', _t('export.epubSplitNone') || '']
      ], full: true },
    ]},
    { title: _t('export.secEpubStyle') || '', fmts: ['epub'], fields: [
      { k: 'epub.fontSize', label: _t('export.bodySize') || '', type: 'number', min: 6, max: 72, step: 'any' },
      { k: 'epub.lineHeight', label: _t('export.lineHeight') || '', type: 'number', min: 1, max: 5, step: 'any' },
      { k: 'epub.marginV', label: _t('export.epubMarginV') || '', type: 'number', min: 0, max: 40, step: 'any' },
      { k: 'epub.marginH', label: _t('export.epubMarginH') || '', type: 'number', min: 0, max: 40, step: 'any' },
    ]},

    /* --- LaTeX 学术源码专属参数 --- */
    { title: _t('export.secLatexDoc') || '', fmts: ['tex'], fields: [
      { k: 'tex.docClass', label: _t('export.latexDocClass') || '', type: 'select', opts: [
        ['ctexart', 'ctexart'],
        ['article', 'article'],
        ['ctexrep', 'ctexrep'],
        ['report', 'report'],
        ['book', 'book'],
        ['beamer', 'beamer']
      ], full: true },
      { k: 'tex.fontSize', label: _t('export.latexFontSize') || '', type: 'select', opts: [
        ['10pt', '10pt'], ['11pt', '11pt'], ['12pt', '12pt']
      ]},
      { k: 'tex.paperSize', label: _t('export.pageSize') || '', type: 'select', opts: [
        ['a4paper', 'A4'], ['letterpaper', 'US Letter']
      ]},
      { k: 'tex.margin', label: _t('export.latexMargin') || '', type: 'select', opts: [
        ['2.5cm', '2.5 cm'], ['1in', '1 in'], ['2cm', '2.0 cm'], ['3cm', '3.0 cm']
      ]},
      { k: 'tex.bibEngine', label: _t('export.latexBibEngine') || '', type: 'select', opts: [
        ['biblatex', 'BibLaTeX'], ['natbib', 'Natbib'], ['bibtex', 'BibTeX']
      ]},
      { k: 'tex.useCtex', label: _t('export.latexUseCtex') || '', type: 'checkbox' },
    ]},

    /* --- PDF / DOCX 页面与版式 --- */
    { title: _t('export.secPage') || '', fmts: ['pdf', 'docx'], fields: [
      { k: 'page.size', label: _t('export.pageSize') || '', type: 'select', opts: EXPORT_PAGES },
      { k: 'page.width', label: _t('export.customWidth'), type: 'number', min: 80, max: 600 },
      { k: 'page.height', label: _t('export.customHeight'), type: 'number', min: 80, max: 600 },
      { k: 'page.orientation', label: _t('export.pageOrientation') || '', type: 'select', opts: [['portrait', _t('export.portrait') || ''], ['landscape', _t('export.landscape') || '']] },
      { k: 'page.marginTop', label: _t('export.marginTop') || '', type: 'number', min: 0, max: 60 },
      { k: 'page.marginRight', label: _t('export.marginRight') || '', type: 'number', min: 0, max: 60 },
      { k: 'page.marginBottom', label: _t('export.marginBottom') || '', type: 'number', min: 0, max: 60 },
      { k: 'page.marginLeft', label: _t('export.marginLeft') || '', type: 'number', min: 0, max: 60 },
      { k: 'page.columns', label: _t('export.columns'), type:'number', min:1, max:3, step:1, engines:['typst'], default:1 },
      { k: 'page.columnGap', label: _t('export.columnGap'), type:'number', min:1, max:30, engines:['typst'], default:8 },
      { k: 'page.gutter', label: _t('export.bindingGutter'), type:'number', min:0, max:40, engines:['typst'], default:0 },
      { k: 'page.mirrorMargins', label: _t('export.mirrorMargins'), type:'checkbox', engines:['typst'], default:false },
    ]},
    { title: _t('export.secCoverToc') || '', fmts: ['pdf', 'docx', 'epub'], fields: [
      { k: 'cover.enabled', label: _t('export.enableCover') || '', type: 'checkbox' },
      { k: 'cover.title', label: _t('export.coverTitle') || '', type: 'text', full: true },
      { k: 'cover.subtitle', label: _t('export.coverSubtitle') || '', type: 'text', full: true },
      { k: 'cover.date', label: _t('export.coverDate') || '', type: 'text' },
      { k: 'cover.align', label: _t('export.coverAlign') || '', type: 'select', opts: [['center', _t('export.alignCenter') || ''], ['left', _t('export.alignLeft') || ''], ['right', _t('export.alignRight') || '']] },
      { k: 'toc.enabled', label: _t('export.toc'), type: 'checkbox', fmts: ['pdf', 'docx'] },
      { k: 'toc.depth', label: _t('export.tocDepth'), type:'number', min:1, max:6, step:1, fmts:['pdf','docx','epub'], engines:['typst'], default:3 },
      { k: 'toc.leader', label: _t('export.tocLeader'), type:'select', opts:[['dot','···'],['hyphen','– – –'],['underscore','___'],['none',_t('presentation.transitionNone')]], fmts:['pdf','docx'], engines:['typst'], default:'dot' },
    ]},
    { title: _t('export.secTypography') || '', fmts: ['pdf', 'docx', 'html', 'epub', 'tex', 'presentation'], fields: [
      { k: 'typography.font', label: _t('export.bodyFont') || '', type: 'select', opts: EXPORT_FONTS.map(f => [f, f]) },
      { k: 'typography.size', label: _t('export.bodySize') || '', type: 'number', min: 8, max: 20, fmts: ['pdf','docx','html'] },
      { k: 'typography.lineSpacing', label: _t('export.lineHeight') || '', type: 'lineSpacing', full: true },
      { k: 'typography.trackingPt', label: _t('export.trackingPt') || '', type: 'tracking' },
      { k: 'typography.baselineShiftPt', label: _t('export.baselineShiftPt') || '', type: 'baseline' },
      { k: 'typography.spacing', label: _t('export.paragraphSpacing') || '', type: 'number', min: 0, max: 30 },
      { k: 'typography.firstLineIndent', label: _t('export.firstLineIndent'), type: 'number', min: 0, max: 30 },
      { k: 'typography.color', label: _t('export.bodyColor') || '', type: 'color' },
      { k: 'typography.align', label: _t('export.align') || '', type: 'select', opts: [['left', _t('export.alignLeft') || ''], ['center', _t('export.alignCenter') || ''], ['right', _t('export.alignRight') || ''], ['justify', _t('export.alignJustify') || '']] },
      { k: 'typography.widowControl', label: _t('export.widowControl'), type:'checkbox', fmts:['pdf','docx','html','epub'], engines:['typst'], default:true },
      { k: 'typography.keepLines', label: _t('export.keepLines'), type:'checkbox', fmts:['pdf','docx','html','epub'], engines:['typst'], default:false },
      { k: 'footnotes.position', label: _t('export.notePlacement'), type:'select', opts:[['page',_t('export.noteFootnotes')],['document',_t('export.noteEndnotes')]], fmts:['pdf','docx','epub'], engines:['typst'], default:'page' },
      { k: 'footnotes.title', label: _t('export.noteTitle'), type:'text', fmts:['pdf','epub'], engines:['typst'], default:_t('export.noteEndnotes') },
      { k: 'footnotes.size', label: _t('export.footnoteSize'), type:'number', min:6, max:24, step:.5, fmts:['pdf','docx','epub'], engines:['typst'], default:9 },
      { k: 'footnotes.lineHeight', label: _t('export.footnoteLineHeight'), type:'number', min:1, max:3, step:.1, fmts:['pdf','docx','epub'], engines:['typst'], default:1 },
      { k: 'footnotes.numberFormat', label: _t('export.footnoteNumberFormat'), type:'select', opts:[['decimal','1, 2, 3'],['lowerRoman','i, ii, iii'],['upperRoman','I, II, III'],['lowerLetter','a, b, c'],['upperLetter','A, B, C']], fmts:['pdf','docx','epub'], engines:['typst'], default:'decimal' },
      { k: 'footnotes.start', label: _t('export.noteStart'), type:'number', min:1, max:9999, step:1, fmts:['pdf','docx','epub'], engines:['typst'], default:1 },
    ]},
    { title: _t('export.secHeadings') || '', fmts: ['pdf', 'docx', 'html', 'epub'], headingRows: true },
    { title: _t('export.secTable') || '', fmts: ['pdf', 'docx', 'html', 'epub'], fields: [
      { k: 'table.style', label: _t('export.tableStyle'), type: 'select', opts: [['grid', _t('export.tableGrid')], ['three-line', _t('export.tableThreeLine')]] },
      { k: 'table.headerBg', label: _t('export.tableHeaderBg') || '', type: 'color' },
      { k: 'table.headerColor', label: _t('export.tableHeaderColor') || '', type: 'color' },
      { k: 'table.headerBold', label: _t('export.tableHeaderBold') || '', type: 'checkbox' },
      { k: 'table.borderColor', label: _t('export.tableBorderColor') || '', type: 'color' },
      { k: 'table.borderWidth', label: _t('export.tableBorderWidth') || '', type: 'number', min: 0, max: 3, step: 0.25 },
      { k: 'table.banded', label: _t('export.tableBanded') || '', type: 'checkbox' },
      { k: 'table.bandColor', label: _t('export.tableBandColor') || '', type: 'color' },
      { k: 'table.cellSize', label: _t('export.tableCellSize') || '', type: 'number', min: 7, max: 16 },
      { k: 'table.cellPadding', label: _t('export.tableCellPadding') || '', type: 'number', min: 0, max: 20 },
      { k: 'table.align', label: _t('export.align') || '', type: 'select', opts: [['left', _t('export.alignLeft') || ''], ['center', _t('export.alignCenter') || ''], ['right', _t('export.alignRight') || ''], ['justify', _t('export.alignJustify') || '']] },
      { k: 'table.widthPct', label: _t('export.tableWidthPct') || '', type: 'number', min: 50, max: 100 },
    ]},
    { title: _t('export.secCode') || '', fmts: ['pdf', 'docx', 'html', 'epub'], fields: [
      { k: 'code.bg', label: _t('export.codeBg') || '', type: 'color' },
      { k: 'code.color', label: _t('export.codeColor') || '', type: 'color' },
      { k: 'code.font', label: _t('export.codeFont') || '', type: 'select', opts: EXPORT_MONO.map(f => [f, f]) },
      { k: 'code.size', label: _t('export.codeSize') || '', type: 'number', min: 6, max: 16 },
      { k: 'code.borderColor', label: _t('export.codeBorderColor') || '', type: 'color' },
      { k: 'code.borderWidth', label: _t('export.codeBorderWidth') || '', type: 'number', min: 0, max: 3, step: 0.25 },
      { k: 'code.rounded', label: _t('export.codeRounded') || '', type: 'checkbox', fmts: ['html','epub'] },
    ]},
    { title: _t('export.secQuoteLink') || '', fmts: ['pdf', 'docx', 'html', 'epub'], fields: [
      { k: 'quote.barColor', label: _t('export.quoteBarColor') || '', type: 'color' },
      { k: 'quote.bg', label: _t('export.quoteBg') || '', type: 'color' },
      { k: 'quote.color', label: _t('export.quoteColor') || '', type: 'color' },
      { k: 'link.color', label: _t('export.linkColor') || '', type: 'color' },
      { k: 'hr.color', label: _t('export.hrColor') || '', type: 'color' },
    ]},
    { title: _t('export.secFooterMeta') || '', fmts: ['pdf', 'docx'], fields: [
      { k: 'header.text', label: _t('export.headerText'), type: 'text', full: true },
      { k: 'page.differentOddEven', label: _t('export.differentOddEven'), type:'checkbox', engines:['typst'], default:false },
      { k: 'header.evenText', label: _t('export.evenHeaderText'), type:'text', full:true, engines:['typst'], default:'' },
      { k: 'header.align', label: _t('export.align'), type: 'select', opts: EXPORT_ALIGNS.slice(0, 3).map(a=>[a,_t({left:'export.alignLeft',center:'export.alignCenter',right:'export.alignRight'}[a])]) },
      { k: 'header.size', label: _t('export.headerSize'), type:'number', min:6, max:24, step:.5, engines:['typst'], default:9 },
      { k: 'header.distance', label: _t('export.headerDistance'), type:'number', min:0, max:50, engines:['typst'], default:10 },
      { k: 'images.widthPct', label: _t('export.imageWidth'), type: 'number', min: 10, max: 100 },
      { k: 'images.maxHeightPct', label: _t('export.imageHeight'), type: 'number', min: 10, max: 100 },
      { k: 'footer.pageNumbers', label: _t('export.showPageNumbers') || '', type: 'checkbox' },
      { k: 'footer.text', label: _t('export.footerTextLabel') || '', type: 'text', full: true },
      { k: 'footer.evenText', label: _t('export.evenFooterText'), type:'text', full:true, engines:['typst'], default:'' },
      { k: 'footer.align', label: _t('export.footerAlign'), type:'select', opts:EXPORT_ALIGNS.slice(0,3).map(a=>[a,_t({left:'export.alignLeft',center:'export.alignCenter',right:'export.alignRight'}[a])]), engines:['typst'], default:'center' },
      { k: 'footer.size', label: _t('export.footerSize'), type:'number', min:6, max:24, step:.5, engines:['typst'], default:9 },
      { k: 'footer.distance', label: _t('export.footerDistance'), type:'number', min:0, max:50, engines:['typst'], default:10 },
      { k: 'footer.numberFormat', label: _t('export.pageNumberFormat'), type:'select', opts:[['decimal','1, 2, 3'],['lowerRoman','i, ii, iii'],['upperRoman','I, II, III'],['lowerLetter','a, b, c'],['upperLetter','A, B, C']], engines:['typst'], default:'decimal' },
      { k: 'footer.start', label: _t('export.pageNumberStart'), type:'number', min:1, max:9999, engines:['typst'], default:1 },
      { k: 'footer.showTotal', label: _t('export.pageNumberTotal'), type:'checkbox', engines:['typst'], default:false },
      { k: 'page.differentFirstPage', label: _t('export.firstPageRunning'), type:'checkbox', engines:['typst'], default:!!state.export.options?.cover?.enabled },
    ]},
    { title: _t('pm.metadata'), fmts: ['pdf', 'docx', 'html', 'epub', 'tex'], publicationMetadata: true },
    { title: _t('export.secMath') || '', fmts: ['pdf', 'docx'], fields: [
      { k: 'math.dpi', label: _t('export.mathDpi') || '', type: 'number', min: 100, max: 500, step: 10 },
    ]},
    { title: _t('export.secHtmlTheme') || '', fmts: ['html'], fields: [
      { k: 'htmlTheme', label: _t('export.htmlThemeLabel') || '', type: 'select', opts: [['light', _t('export.themeLight') || ''], ['dark', _t('export.themeDark') || ''], ['sepia', _t('export.themeSepia') || '']] },
    ]},
    { title: _t('export.pdfEngine'), engine:true, fmts: ['pdf'], fields: [
      { k:'pdf.engine',label:_t('export.pdfEngine'),type:'select',opts:[['rust',_t('export.pdfEngineNative')],['typst','Typst'],['xelatex','XeLaTeX'],['pdflatex','pdfLaTeX']],full:true },
      { k:'pdf.engineHint',label:_t('export.pdfEngineHint'),type:'note',full:true },
    ]},
    { title: _t('export.secFigures'), fmts: ['pdf','docx','html','epub'], fields: [
      { k:'figures.style',label:_t('export.figureStyle'),type:'select',opts:[['original',_t('export.figureOriginal')],['research',_t('export.figureResearch')]] },
      { k:'figures.widthMm',label:_t('export.figureWidth'),type:'number',min:45,max:300 },
      { k:'figures.fontSizePt',label:_t('export.figureFontSize'),type:'number',min:5,max:18,step:.5 },
    ]},
    { title: _t('export.secSlides') || '', fmts: ['presentation'], fields: [
      { k: 'theme', label: _t('export.slidesTheme') || '', type: 'select', opts: ['black', 'white', 'league', 'beige', 'night', 'serif', 'simple', 'solarized', 'blood', 'moon', 'sky'] },
      { k: 'transition', label: _t('export.slidesTransition') || '', type: 'select', opts: ['slide', 'fade', 'zoom', 'convex', 'concave', 'none'] },
      { k: 'slidesHint', label: _t('export.slidesHint') || '', type: 'note', full: true },
    ]},
  ];
  const texPdf = state.export.fmt === 'pdf' && ['xelatex','pdflatex'].includes(state.export.options?.pdf?.engine);
  if (!texPdf) return sections;
  // Show only controls actually supported by the selected output engine.
  return [sections.find(s=>s.engine),...sections.filter(s=>s.fmts?.includes('tex')).map(s=>({...s,fmts:['pdf'],fields:s.fields.filter(f=>f.k!=='tex.bibEngine')})),
    ...sections.filter(s=>s.publicationMetadata).map(s=>({...s,fmts:['pdf']}))];
}


function expGet(obj, path) {
  return path.split('.').reduce((o, k) => (o == null ? undefined : o[k]), obj);
}
function expSet(obj, path, val) {
  const ks = path.split('.');
  let o = obj;
  for (let i = 0; i < ks.length - 1; i++) {
    if (typeof o[ks[i]] !== 'object' || o[ks[i]] === null) o[ks[i]] = {};
    o = o[ks[i]];
  }
  o[ks[ks.length - 1]] = val;
}
const EXPORT_EPUB_NUMERIC = {
  'epub.fontSize': { min: 6, max: 72, inherit: 'typography.size', fallback: 11, unit: 'pt' },
  'epub.lineHeight': { min: 1, max: 5, inherit: 'typography.lineHeight', fallback: 1.6, unit: '' },
  'epub.marginV': { min: 0, max: 40, fallback: 5, unit: '%' },
  'epub.marginH': { min: 0, max: 40, fallback: 8, unit: '%' }
};
function expEpubNumericValid(value, rule) {
  return typeof value === 'number' && Number.isFinite(value) && value >= rule.min && value <= rule.max;
}
function expEpubNumericIssue(options) {
  if (options.epub != null && (typeof options.epub !== 'object' || Array.isArray(options.epub))) return 'error.epub_invalid_options';
  return Object.entries(EXPORT_EPUB_NUMERIC).some(([key, rule]) => {
    const value = expGet(options, key); return value != null && !expEpubNumericValid(value, rule);
  }) ? 'error.epub_invalid_options' : '';
}
function validateEpubNumericControls(report = false) {
  if (state.export.fmt !== 'epub') return true;
  let first;
  document.querySelectorAll('#export-opts [data-k]').forEach(el => {
    const rule = EXPORT_EPUB_NUMERIC[el.dataset.k]; if (!rule) return;
    const raw = el.dataset.epubNumericInvalid;
    const valid = raw === undefined && !el.validity?.badInput && (el.value.trim() === '' || expEpubNumericValid(Number(el.value), rule));
    el.setCustomValidity?.(valid ? '' : i18n.t('error.epub_invalid_options'));
    el.setAttribute('aria-invalid', String(!valid));
    if (!valid && !first) first = el;
  });
  if (first && report) {
    first.closest?.('.exp-sec')?.classList.add('open');
    for (let node = first.parentElement; node; node = node.parentElement) if (node.tagName === 'DETAILS') node.open = true;
    first.focus?.(); first.reportValidity?.(); showToast(i18n.t('error.epub_invalid_options'));
  }
  return !first;
}
function expDeepMerge(base, over) {
  const out = JSON.parse(JSON.stringify(base || {}));
  if (!over || typeof over !== 'object') return out;
  Object.keys(over).forEach(k => {
    const v = over[k];
    if (v && typeof v === 'object' && !Array.isArray(v) && out[k] && typeof out[k] === 'object') {
      out[k] = expDeepMerge(out[k], v);
    } else if (v !== undefined) out[k] = JSON.parse(JSON.stringify(v));
  });
  const merged=window.ReadMDPublication?.lineSpacing.inheritGlobal(base,over,out)||out;
  const physical=window.ReadMDPublication?.tracking.inheritGlobal(base,over,merged)||merged;
  return window.ReadMDPublication?.baseline.inheritGlobal(base,over,physical)||physical;
}

async function loadExportPresets() {
  if (state.export.defaults) return true;
  if (!bindPy() && window.READMD_ENGINE !== 'rust') return false;
  try {
    const d = (hasPy && py && typeof py.get_export_presets === 'function')
      ? await py.get_export_presets()
      : await apiFetch('/api/export/presets').then(r => {
        if (!r.ok) throw new Error('HTTP ' + r.status);
        return r.json();
      });
    if (!d || d.ok === false || !d.defaults || !Object.keys(d.defaults).length) return false;
    state.export.defaults = d.defaults || {};
    state.export.htmlDarkPalette = d.htmlDarkPalette || [];
    state.export.presets = d.presets || {};
    state.export.catalog = d.catalog || [];
    state.export.custom = d.custom || {};
    state.export.last = d.last || null;
    if (state.export.last && state.export.last.fmt) state.export.fmt = state.export.last.fmt;
    if (state.export.last && state.export.last.options) {
      state.export.options = expDeepMerge(state.export.defaults, state.export.last.options);
    } else {
      state.export.options = expDeepMerge(state.export.defaults, {});
    }
    return true;
  } catch (e) {
    console.error(e);
    return false;
  }
}

let exportOpenEpoch = 0;
async function openExportModal() {
  const epoch = ++exportOpenEpoch;
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  if (state.mode === 'welcome') {
    showToast(_t('toast.openDocumentToUse') || '');
    return;
  }
  if (!bindPy() && window.READMD_ENGINE !== 'rust') { showToast(_t('toast.exportBrowserNotice') || ''); return; }
  if (!state.export.ready) {
    const ok = await loadExportPresets();
    if (ok) state.export.ready = true;
    if (epoch !== exportOpenEpoch || state.mode === 'welcome') return;
    if (ok) renderExportModal();
    else showToast(_t('toast.exportModuleLoadFail') || '');
    return;
  }
  renderExportModal();
}

function closeExportModal() { exportOpenEpoch++; $('export-modal').classList.add('hidden'); cancelNativeExportPreview(); }

function currentExportContent() {
  if (state.editing) {
    if (typeof getEditContent === 'function') {
      const txt = getEditContent();
      if (typeof txt === 'string') return txt;
    }
    return $('edit-area')?.value ?? '';
  }
  if (state.mode === 'file') return state.original ?? state.fixed ?? '';
  return state.fixed ?? state.original ?? '';
}
function currentExportName() {
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  let n = '';
  if (state.mode === 'file' && state.file) n = state.file.split(/[\\/]/).pop();
  else n = (state.sourceName || (_t('export.defaultExportName') || '')).split(/[\\/]/).pop();
  n = n.replace(/\.[^.]+$/, '');
  return n || (_t('export.defaultExportName') || '');
}


function renderExportModal() {
  $('export-modal').classList.remove('hidden');
  document.querySelectorAll('#export-box .exp-fmt').forEach(button => {
    const selected = button.dataset.fmt === state.export.fmt;
    button.classList.toggle('active', selected);
    button.setAttribute('aria-selected', String(selected));
    if (selected) $('export-opts').setAttribute('aria-labelledby', button.id);
  });
  renderExportPresetSelect();
  renderExportSections();
  initExportAiDesigner();
  updateExportLivePreview();
  const r = $('export-result');
  r.textContent = ''; r.className = 'export-result'; r.title = '';
  if ($('export-warns')) $('export-warns').classList.add('hidden');
  $('export-open').classList.add('hidden');
  $('export-reveal').classList.add('hidden');
}

function scientificTablePreviewCss(opts) {
  if (opts.table?.style !== 'three-line') return '';
  const width=Math.max(.25,Math.min(3,Number(opts.table.borderWidth)||.75));
  const border=/^#[0-9a-f]{6}$/i.test(opts.table.borderColor||'')?opts.table.borderColor:'#111111';
  const color=/^#[0-9a-f]{6}$/i.test(opts.typography?.color||'')?opts.typography.color:'#111111';
  return ['#export-preview-mini-content','#export-preview-full-page','.export-page-body'].map(root=>`
    ${root} table { border-collapse:collapse!important;border:${width*2}pt solid ${border}!important;border-left:0!important;border-right:0!important; }
    ${root} th,${root} td { border:0!important;background:transparent!important;color:${color}!important; }
    ${root} thead th { border-bottom:${width}pt solid ${border}!important; }
    ${root} tbody tr:nth-child(even),${root} tbody tr:nth-child(even) td { background:transparent!important; }
  `).join('\n');
}
function generateExportPreviewCss(opts, fmt) {
  opts = opts || {};
  if (fmt === 'html' && opts.htmlTheme === 'dark') {
    opts = expDeepMerge(opts, {});
    for (const [key, original, dark] of state.export.htmlDarkPalette || []) {
      const value = expGet(opts, key);
      if (typeof value !== 'string' || value.trim().toLowerCase() === original) expSet(opts, key, dark);
    }
  }
  const ty = opts.typography || {};
  const hd = opts.headings || {};
  const tb = opts.table || {};
  const code = opts.code || {};
  const quote = opts.quote || {};
  const link = opts.link || {};
  const hr = opts.hr || {};
  const page = opts.page || {};
  const htmlTheme = opts.htmlTheme || 'light';

  const fontMap = {
    'MicrosoftYaHei': '"Microsoft YaHei", "Microsoft YaHei UI", "PingFang SC", "微软雅黑", sans-serif',
    'SimHei': '"SimHei", "黑体", sans-serif',
    'SimSun': '"SimSun", "宋体", serif',
    'KaiTi': '"KaiTi", "楷体", serif',
    'DengXian': '"DengXian", "等线", sans-serif',
    'Arial': 'Arial, sans-serif',
  };
  const fontFamily = fontMap[ty.font] || fontMap['MicrosoftYaHei'];

  let pageBg = '#ffffff';
  let baseFg = ty.color || '#262626';
  let codeBg = code.bg || '#f5f6f8';
  let quoteBg = quote.bg || '#f3f6ff';

  if (fmt === 'html') {
    baseFg = '#262626';
    if (htmlTheme === 'dark') {
      pageBg = '#14161a';
      baseFg = '#d6d9de';
    } else if (htmlTheme === 'sepia') {
      pageBg = '#faf4e7';
      baseFg = '#3b2f1d';
    }
  }

  const h1 = hd.h1 || { size: 20, color: '#1a1a1a', bold: true, align: 'left', before: 18, after: 10 };
  const h2 = hd.h2 || { size: 16, color: '#1f2937', bold: true, align: 'left', before: 14, after: 8 };
  const h3 = hd.h3 || { size: 14, color: '#2d3748', bold: true, align: 'left', before: 12, after: 6 };
  const h4 = hd.h4 || { size: 12, color: '#374151', bold: true, align: 'left', before: 10, after: 6 };
  const h5 = hd.h5 || { size: 11, color: '#4a5568', bold: true, align: 'left', before: 8, after: 4 };
  const h6 = hd.h6 || { size: 10.5, color: '#4a5568', bold: true, align: 'left', before: 8, after: 4 };

  return `
    /* Mini Preview Dynamic Styling */
    #export-preview-mini-page {
      background: ${pageBg} !important;
      color: ${baseFg} !important;
      font-family: ${fontFamily} !important;
      text-align: ${ty.align || 'left'} !important;
    }
    #export-preview-mini-content {
      color: ${baseFg} !important;
      font-family: ${fontFamily} !important;
      font-size: 3.5px !important;
      line-height: ${ty.lineHeight ?? 1.6} !important;
    }
    #export-preview-mini-content p, #export-preview-mini-content li, #export-preview-mini-content span, #export-preview-mini-content div {
      color: ${baseFg} !important;
      font-size: ${(ty.size ?? 11) * 0.32}px !important;
      line-height: ${ty.lineHeight ?? 1.6} !important;
      text-align: ${ty.align || 'left'} !important;
    }
    #export-preview-mini-content p {
      margin: ${(ty.spacing ?? 6) * 0.25}px 0 !important;
    }
    #export-preview-mini-content h1 {
      color: ${h1.color || '#1a1a1a'} !important;
      font-size: ${(h1.size ?? 20) * 0.35}px !important;
      font-weight: ${h1.bold ? 'bold' : 'normal'} !important;
      text-align: ${h1.align || 'left'} !important;
      margin-top: ${(h1.before ?? 18) * 0.2}px !important;
      margin-bottom: ${(h1.after ?? 10) * 0.2}px !important;
      border-bottom: none !important;
    }
    #export-preview-mini-content h2 {
      color: ${h2.color || '#1f2937'} !important;
      font-size: ${(h2.size ?? 16) * 0.35}px !important;
      font-weight: ${h2.bold ? 'bold' : 'normal'} !important;
      text-align: ${h2.align || 'left'} !important;
      margin-top: ${(h2.before ?? 14) * 0.2}px !important;
      margin-bottom: ${(h2.after ?? 8) * 0.2}px !important;
      border-bottom: none !important;
    }
    #export-preview-mini-content h3 {
      color: ${h3.color || '#2d3748'} !important;
      font-size: ${(h3.size ?? 14) * 0.35}px !important;
      font-weight: ${h3.bold ? 'bold' : 'normal'} !important;
      text-align: ${h3.align || 'left'} !important;
      margin-top: ${(h3.before ?? 12) * 0.2}px !important;
      margin-bottom: ${(h3.after ?? 6) * 0.2}px !important;
    }
    #export-preview-mini-content h4, #export-preview-mini-content h5, #export-preview-mini-content h6 {
      color: ${h4.color || '#374151'} !important;
      font-size: ${(h4.size ?? 12) * 0.35}px !important;
      font-weight: ${h4.bold ? 'bold' : 'normal'} !important;
      text-align: ${h4.align || 'left'} !important;
    }
    #export-preview-mini-content table {
      border-collapse: collapse !important;
      width: ${tb.widthPct ?? 100}% !important;
      margin: 3px auto !important;
      font-size: ${(tb.cellSize ?? 10) * 0.32}px !important;
    }
    #export-preview-mini-content th, #export-preview-mini-content td {
      border: 0.5px solid ${tb.borderColor || '#c8cdd4'} !important;
      padding: 1px 2px !important;
      text-align: ${tb.align || 'left'} !important;
      color: ${baseFg} !important;
    }
    #export-preview-mini-content th {
      background: ${tb.headerBg || '#3b6ef5'} !important;
      color: ${tb.headerColor || '#ffffff'} !important;
      font-weight: ${tb.headerBold ? 'bold' : 'normal'} !important;
    }
    #export-preview-mini-content tbody tr:nth-child(even) td {
      background: ${tb.banded ? (tb.bandColor || '#f3f5f9') : 'transparent'} !important;
    }
    #export-preview-mini-content pre {
      background: ${codeBg} !important;
      color: ${code.color || '#2f3b4a'} !important;
      border: 0.5px solid ${code.borderColor || '#dfe3e8'} !important;
      border-radius: ${code.rounded ? '2px' : '0'} !important;
      padding: 2px 3px !important;
      font-size: ${(code.size ?? 9.5) * 0.32}px !important;
      margin: 2px 0 !important;
    }
    #export-preview-mini-content code {
      font-family: ${code.font || 'Consolas'}, Consolas, monospace !important;
    }
    #export-preview-mini-content :not(pre) > code {
      background: ${codeBg} !important;
      color: ${fmt === 'html' && htmlTheme === 'dark' ? '#ff99b0' : '#c7254e'} !important;
      padding: 0 1px !important;
    }
    #export-preview-mini-content blockquote {
      margin: 2px 0 !important;
      padding: 1px 4px !important;
      background: ${quoteBg} !important;
      color: ${quote.color || '#4a5568'} !important;
      border-left: 2px solid ${quote.barColor || '#3b6ef5'} !important;
    }
    #export-preview-mini-content blockquote p {
      color: ${quote.color || '#4a5568'} !important;
    }
    #export-preview-mini-content a {
      color: ${link.color || '#2b6cb0'} !important;
    }
    #export-preview-mini-content hr {
      border: none !important;
      border-top: 0.5px solid ${hr.color || '#d8dce2'} !important;
      margin: 3px 0 !important;
    }

    /* Full Modal Preview Dynamic Styling */
    #export-preview-full-page {
      width: 100% !important;
      height: auto !important;
      min-height: 100% !important;
      overflow: visible !important;
      display: flex !important;
      flex-direction: column !important;
      align-items: center !important;
      gap: 24px !important;
      background: transparent !important;
      box-shadow: none !important;
      padding: 0 !important;
    }
    .export-preview-page-sheet {
      background: ${pageBg} !important;
      color: ${baseFg} !important;
      font-family: ${fontFamily} !important;
      font-size: ${ty.size ?? 11}pt !important;
      line-height: ${ty.lineHeight ?? 1.6} !important;
      text-align: ${ty.align || 'left'} !important;
      padding: ${page.marginTop ?? 20}mm ${page.marginRight ?? 18}mm ${page.marginBottom ?? 20}mm ${page.marginLeft ?? 18}mm !important;
      width: ${exportPaperSize(page)[0]}mm; height: ${exportPaperSize(page)[1]}mm;
      box-sizing: border-box !important;
      overflow: hidden !important;
      display: flex !important;
      flex-direction: column !important;
      justify-content: space-between !important;
      box-shadow: 0 4px 24px rgba(0, 0, 0, 0.35) !important;
      border-radius: 2px !important;
      margin-bottom: 24px !important;
      flex-shrink: 0 !important;
    }
    .export-page-body {
      flex: 1 1 auto !important;
      min-height: 0 !important;
      overflow: hidden !important;
      word-break: break-word !important;
    }
    #export-preview-full-page p, .export-preview-page-sheet p, .export-page-body p, #export-preview-full-page li, .export-page-body li, #export-preview-full-page span, .export-page-body span, #export-preview-full-page div, .export-page-body div {
      color: ${baseFg} !important;
      font-size: ${ty.size ?? 11}pt !important;
      line-height: ${ty.lineHeight ?? 1.6} !important;
      text-align: ${ty.align || 'left'} !important;
    }
    #export-preview-full-page p, .export-page-body p {
      margin: ${ty.spacing ?? 6}pt 0 !important;
      text-indent: ${ty.firstLineIndent ?? 0}mm;
    }
    #export-preview-full-page h1, .export-page-body h1 {
      color: ${h1.color || '#1a1a1a'} !important;
      font-size: ${h1.size ?? 20}pt !important;
      font-weight: ${h1.bold ? 'bold' : 'normal'} !important;
      text-align: ${h1.align || 'left'} !important;
      margin-top: ${h1.before ?? 18}pt !important;
      margin-bottom: ${h1.after ?? 10}pt !important;
      line-height: 1.35 !important;
      border-bottom: none !important;
    }
    #export-preview-full-page h2, .export-page-body h2 {
      color: ${h2.color || '#1f2937'} !important;
      font-size: ${h2.size ?? 16}pt !important;
      font-weight: ${h2.bold ? 'bold' : 'normal'} !important;
      text-align: ${h2.align || 'left'} !important;
      margin-top: ${h2.before ?? 14}pt !important;
      margin-bottom: ${h2.after ?? 8}pt !important;
      line-height: 1.35 !important;
      border-bottom: none !important;
    }
    #export-preview-full-page h3, .export-page-body h3 {
      color: ${h3.color || '#2d3748'} !important;
      font-size: ${h3.size ?? 14}pt !important;
      font-weight: ${h3.bold ? 'bold' : 'normal'} !important;
      text-align: ${h3.align || 'left'} !important;
      margin-top: ${h3.before ?? 12}pt !important;
      margin-bottom: ${h3.after ?? 6}pt !important;
      line-height: 1.35 !important;
    }
    #export-preview-full-page h4, .export-page-body h4 {
      color: ${h4.color || '#374151'} !important;
      font-size: ${h4.size ?? 12}pt !important;
      font-weight: ${h4.bold ? 'bold' : 'normal'} !important;
      text-align: ${h4.align || 'left'} !important;
      margin-top: ${h4.before ?? 10}pt !important;
      margin-bottom: ${h4.after ?? 6}pt !important;
    }
    #export-preview-full-page h5, .export-page-body h5 {
      color: ${h5.color || '#4a5568'} !important;
      font-size: ${h5.size ?? 11}pt !important;
      font-weight: ${h5.bold ? 'bold' : 'normal'} !important;
      text-align: ${h5.align || 'left'} !important;
      margin-top: ${h5.before ?? 8}pt !important;
      margin-bottom: ${h5.after ?? 4}pt !important;
    }
    #export-preview-full-page h6, .export-page-body h6 {
      color: ${h6.color || '#4a5568'} !important;
      font-size: ${h6.size ?? 10.5}pt !important;
      font-weight: ${h6.bold ? 'bold' : 'normal'} !important;
      text-align: ${h6.align || 'left'} !important;
      margin-top: ${h6.before ?? 8}pt !important;
      margin-bottom: ${h6.after ?? 4}pt !important;
    }
    #export-preview-full-page table, .export-page-body table {
      border-collapse: collapse !important;
      width: ${tb.widthPct ?? 100}% !important;
      margin: 12pt auto !important;
      font-size: ${tb.cellSize ?? 10}pt !important;
    }
    #export-preview-full-page th, .export-page-body th, #export-preview-full-page td, .export-page-body td {
      border: ${tb.borderWidth ?? 0.75}px solid ${tb.borderColor || '#c8cdd4'} !important;
      padding: ${tb.cellPadding ?? 6}px !important;
      text-align: ${tb.align || 'left'} !important;
      color: ${baseFg} !important;
    }
    #export-preview-full-page th, .export-page-body th {
      background: ${tb.headerBg || '#3b6ef5'} !important;
      color: ${tb.headerColor || '#ffffff'} !important;
      font-weight: ${tb.headerBold ? 'bold' : 'normal'} !important;
    }
    #export-preview-full-page tbody tr:nth-child(even) td, .export-page-body tbody tr:nth-child(even) td {
      background: ${tb.banded ? (tb.bandColor || '#f3f5f9') : 'transparent'} !important;
    }
    #export-preview-full-page pre, .export-page-body pre {
      background: ${codeBg} !important;
      color: ${code.color || '#2f3b4a'} !important;
      border: ${code.borderWidth ?? 0.5}px solid ${code.borderColor || '#dfe3e8'} !important;
      border-radius: ${code.rounded ? '6px' : '0'} !important;
      padding: 10pt 12pt !important;
      overflow: auto !important;
      font-family: ${code.font || 'Consolas'}, Consolas, monospace !important;
      font-size: ${code.size ?? 9.5}pt !important;
      line-height: 1.5 !important;
      margin: 8pt 0 !important;
    }
    #export-preview-full-page code, .export-page-body code {
      font-family: ${code.font || 'Consolas'}, Consolas, monospace !important;
    }
    #export-preview-full-page :not(pre) > code, .export-page-body :not(pre) > code {
      background: ${codeBg} !important;
      color: ${fmt === 'html' && htmlTheme === 'dark' ? '#ff99b0' : '#c7254e'} !important;
      padding: 2px 5px !important;
      border-radius: 4px !important;
      font-size: 0.92em !important;
    }
    #export-preview-full-page blockquote, .export-page-body blockquote {
      margin: 8pt 0 !important;
      padding: 8pt 14pt !important;
      background: ${quoteBg} !important;
      color: ${quote.color || '#4a5568'} !important;
      border-left: 4px solid ${quote.barColor || '#3b6ef5'} !important;
    }
    #export-preview-full-page blockquote p, .export-page-body blockquote p {
      color: ${quote.color || '#4a5568'} !important;
      margin: 4pt 0 !important;
    }
    #export-preview-full-page a, .export-page-body a {
      color: ${link.color || '#2b6cb0'} !important;
      text-decoration: underline !important;
    }
    #export-preview-full-page hr, .export-page-body hr {
      border: none !important;
      border-top: 1px solid ${hr.color || '#d8dce2'} !important;
      margin: 14pt 0 !important;
    }
    #export-preview-full-page img, .export-page-body img {
      max-width: 100% !important;
      height: auto !important;
    }
    .export-page-header, .export-page-footer {
      color: ${baseFg} !important;
      opacity: 0.65;
      border-color: ${baseFg}33 !important;
    }
  ` + (fmt === 'html' ? `
    #export-preview-full-page .export-preview-html-sheet {
      width: min(852px, 100%) !important; max-width: none; height: auto;
      min-height: 0 !important; padding: 24px 16px 64px !important;
    }
    #export-preview-full-page .export-preview-html-sheet p {
      margin-top: 1em !important; margin-bottom: ${ty.spacing ?? 6}pt !important;
    }
    #export-preview-full-page .export-preview-html-sheet table { margin: 8px auto !important; }
    #export-preview-full-page .export-preview-html-sheet blockquote {
      margin: 8px 0 !important; padding: 8px 14px !important;
    }
    #export-preview-full-page .export-preview-html-sheet blockquote p { margin: 4px 0 !important; }
    #export-preview-full-page .export-preview-html-sheet hr { margin: 16px 0 !important; }
  ` : '') + scientificTablePreviewCss(opts);
}

/**
 * 真实渲染 DOM 像素高度测量与智能切分引擎
 * 按选定纸张和边距估算分页；PDF / Word 使用各自的原生排版引擎。
 */
function paginateHtmlIntoExportSheets(fullHtml, opts = {}) {
  const page = opts.page || {};
  const [PAGE_WIDTH_MM, PAGE_HEIGHT_MM] = exportPaperSize(page);
  const marginTop = Number(page.marginTop ?? 20);
  const marginBottom = Number(page.marginBottom ?? 20);
  const marginLeft = Number(page.marginLeft ?? 18);
  const marginRight = Number(page.marginRight ?? 18);

  // 转换为 px (1mm = 3.7795px at 96 DPI)
  const MM_TO_PX = 3.779527559;
  const contentWidthPx = Math.max(200, (PAGE_WIDTH_MM - marginLeft - marginRight) * MM_TO_PX);
  // 可用正文高度（扣除上下边距和页眉页脚预留）
  const usableBodyHeightPx = Math.max(200, (PAGE_HEIGHT_MM - marginTop - marginBottom - 18) * MM_TO_PX);

  // 1. 创建离屏度量容器
  const measureHost = document.createElement('div');
  measureHost.id = 'export-preview-measure-host';
  measureHost.style.cssText = `
    position: absolute !important;
    left: -9999px !important;
    top: 0 !important;
    visibility: hidden !important;
    width: ${contentWidthPx}px !important;
    box-sizing: border-box !important;
    pointer-events: none !important;
    word-break: break-word !important;
  `;
  measureHost.className = 'export-page-body';
  measureHost.innerHTML = fullHtml;
  document.body.appendChild(measureHost);
  renderMath(measureHost);

  const pages = [];
  let currentPageElements = [];
  let currentHeight = 0;

  function pushCurrentPage() {
    if (currentPageElements.length > 0) {
      const container = document.createElement('div');
      currentPageElements.forEach(el => container.appendChild(el));
      pages.push(container.innerHTML);
      currentPageElements = [];
      currentHeight = 0;
    }
  }

  const childNodes = Array.from(measureHost.children);

  if (childNodes.length === 0) {
    document.body.removeChild(measureHost);
    return [fullHtml || ''];
  }

  for (let i = 0; i < childNodes.length; i++) {
    const el = childNodes[i];
    const rect = el.getBoundingClientRect();
    const computed = window.getComputedStyle(el);
    const mTop = parseFloat(computed.marginTop) || 0;
    const mBottom = parseFloat(computed.marginBottom) || 0;
    const blockHeight = (rect.height || el.offsetHeight || 20) + mTop + mBottom;

    // 手动分页符
    if (el.tagName === 'DIV' && el.style.pageBreakAfter === 'always') {
      pushCurrentPage();
      continue;
    }

    // 判断当前页是否放得下
    if (currentHeight + blockHeight <= usableBodyHeightPx) {
      currentPageElements.push(el.cloneNode(true));
      currentHeight += blockHeight;
    } else {
      // 放不下了，如果当前页已有内容，先落页
      if (currentPageElements.length > 0) {
        pushCurrentPage();
      }

      // 如果单个元素高度本身就超过单页（例如超长段落、超长表格或超长代码块）
      if (blockHeight > usableBodyHeightPx) {
        if (el.tagName === 'TABLE') {
          // 表格按行拆分
          const rows = Array.from(el.querySelectorAll('tr'));
          const thead = el.querySelector('thead');
          let tablePart = document.createElement('table');
          tablePart.className = el.className;
          let tbody = document.createElement('tbody');
          tablePart.appendChild(tbody);
          if (thead) tablePart.appendChild(thead.cloneNode(true));

          let subHeight = thead ? 36 : 0;
          rows.forEach(tr => {
            if (tr.parentElement && tr.parentElement.tagName === 'THEAD') return;
            const rH = tr.offsetHeight || 28;
            if (subHeight + rH > usableBodyHeightPx && tbody.children.length > 0) {
              currentPageElements.push(tablePart);
              pushCurrentPage();
              tablePart = document.createElement('table');
              tablePart.className = el.className;
              if (thead) tablePart.appendChild(thead.cloneNode(true));
              tbody = document.createElement('tbody');
              tablePart.appendChild(tbody);
              subHeight = thead ? 36 : 0;
            }
            tbody.appendChild(tr.cloneNode(true));
            subHeight += rH;
          });
          if (tbody.children.length > 0) {
            currentPageElements.push(tablePart);
            currentHeight = subHeight;
          }
        } else if (el.tagName === 'P' || el.tagName === 'BLOCKQUOTE') {
          // 长段落拆分
          const text = el.innerText || el.textContent || '';
          const sentences = text.split(/(?<=[。！？\.\!\?\n])/);
          let pPart = document.createElement(el.tagName.toLowerCase());
          pPart.className = el.className;
          let subText = '';

          sentences.forEach(s => {
            pPart.textContent = subText + s;
            measureHost.appendChild(pPart);
            const pH = pPart.offsetHeight;
            measureHost.removeChild(pPart);

            if (currentHeight + pH > usableBodyHeightPx && subText.length > 0) {
              pPart.textContent = subText;
              currentPageElements.push(pPart.cloneNode(true));
              pushCurrentPage();
              pPart = document.createElement(el.tagName.toLowerCase());
              pPart.className = el.className;
              subText = s;
            } else {
              subText += s;
            }
          });
          if (subText.length > 0) {
            pPart.textContent = subText;
            currentPageElements.push(pPart);
            currentHeight = pPart.offsetHeight || 20;
          }
        } else {
          // 其他不可拆分块（如 code block, img），直接整块放入新页
          currentPageElements.push(el.cloneNode(true));
          currentHeight = blockHeight;
        }
      } else {
        currentPageElements.push(el.cloneNode(true));
        currentHeight = blockHeight;
      }
    }
  }

  pushCurrentPage();
  document.body.removeChild(measureHost);

  return pages.length > 0 ? pages : [fullHtml];
}

let nativeExportPreview = { generation: 0, signature: '', timer: null, controller: null, taskId:'', pageController: null, pagePending: false, data: null, missingCapability: null, page: 1, pageCache: new Map(), blobUrl: '' };

function cancelNativeExportPreview() {
  nativeExportPreview.generation++;
  clearTimeout(nativeExportPreview.timer);
  nativeExportPreview.controller?.abort();
  if (nativeExportPreview.taskId) apiFetch('/api/task/cancel',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({id:nativeExportPreview.taskId})}).catch(()=>{});
  nativeExportPreview.taskId='';
  nativeExportPreview.pageController?.abort();
  nativeExportPreview.pagePending = false;
  if (nativeExportPreview.blobUrl) URL.revokeObjectURL(nativeExportPreview.blobUrl);
  nativeExportPreview.signature = ''; nativeExportPreview.data = null; nativeExportPreview.missingCapability = null; nativeExportPreview.blobUrl = '';
  $('export-preview-mini-page')?.classList.remove('is-native-preview');
  $('export-preview-card')?.classList.remove('hidden');
  $('export-preview-capability')?.remove();
}

const exportFigureCache=new Map();
async function prepareExportFigures(content,options,signal) {
  if(!/(?:`{3,}|~{3,})\s*(?:vega(?:-lite)?|mermaid|chart(?:js|\.js)?|graphviz|viz|dot|tikz|wavedrom|bitfield)\b/i.test(content))return options;
  const figures={...(options.figures||{})};delete figures.rendered;
  const request=await apiFetch('/api/diagram/render',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({action:'fences',content,figures}),signal});
  const parsed=await request.json();if(!parsed.ok)throw new Error(apiMessage(parsed,'reader.diagramRenderFailed'));
  const snapshots=[];let bytes=0;
  for(const item of parsed.fences||[]){
    if(!['vega','vega-lite','mermaid','chart','chartjs','chart.js','graphviz','viz','dot','tikz','wavedrom','bitfield'].includes(item.engine))continue;
    if(signal?.aborted)throw new DOMException('Cancelled','AbortError');
    let svg=exportFigureCache.get(item.key);
    if(!svg){
      const holder=document.createElement('div');holder.style.cssText='position:fixed;left:-10000px;top:0;width:960px;pointer-events:none;visibility:hidden';document.body.append(holder);
      let canvas;
      try{
        const rendered=await renderLocalDiagram(item.engine,item.code,holder,{...figures,export:true});
        if(rendered instanceof Element)holder.replaceChildren(rendered);else holder.innerHTML=rendered;
        canvas=holder.querySelector('canvas');
        if(canvas){
          const chart=canvas._readmdChart;if(chart){chart.options.animation=false;chart.update('none');}
          const width=Math.max(45,Math.min(300,Number(figures.widthMm)||89)),height=width*canvas.height/canvas.width;
          svg=`<svg xmlns="http://www.w3.org/2000/svg" width="${width}mm" height="${height}mm" viewBox="0 0 ${canvas.width} ${canvas.height}" data-readmd-raster="true"><rect width="100%" height="100%" fill="white"/><image width="${canvas.width}" height="${canvas.height}" href="${canvas.toDataURL('image/png')}"/></svg>`;
        }else{
          const element=holder.querySelector('svg');if(!element)throw new Error('diagram_render_failed');
          // Publication output is static. Mermaid ships unused @keyframes in
          // every SVG; retain ordinary style rules, remove animation and
          // resource-loading rules rather than weakening the SVG validator.
          element.querySelectorAll('style').forEach(style=>{
            const sheet=new CSSStyleSheet();sheet.replaceSync(style.textContent);
            const rules=[];for(const rule of sheet.cssRules){if(rule.type!==CSSRule.STYLE_RULE)continue;for(const name of [...rule.style])if(/^(?:animation|transition)(?:-|$)/.test(name))rule.style.removeProperty(name);rules.push(rule.cssText);}style.textContent=rules.join('\n');
          });
          element.setAttribute('xmlns','http://www.w3.org/2000/svg');svg=new XMLSerializer().serializeToString(element);
        }
        if(svg.length>4*1024*1024)throw new Error('diagram_input_too_large');
        while(exportFigureCache.size && (exportFigureCache.size>=16 || [...exportFigureCache.values()].reduce((sum,s)=>sum+s.length,0)+svg.length>16*1024*1024))exportFigureCache.delete(exportFigureCache.keys().next().value);exportFigureCache.set(item.key,svg);
      } finally {(canvas||holder.querySelector('canvas'))?._readmdChart?.destroy();holder.remove();}
    }
    bytes+=svg.length;if(bytes>16*1024*1024||snapshots.length>=128)throw new Error('diagram_input_too_large');
    snapshots.push({key:item.key,svg});
  }
  return {...options,figures:{...figures,rendered:snapshots}};
}
function renderNativeExportCapabilityRequired(generation) {
  const missing = nativeExportPreview.missingCapability;
  if (!missing) return;
  const card = $('export-preview-card');
  let status = $('export-preview-capability');
  if (card && !status) {
    status = document.createElement('div'); status.id = 'export-preview-capability';
    status.className = 'export-preview-capability'; status.setAttribute('role', 'group');
    card.insertAdjacentElement('afterend', status);
  }
  card?.classList.add('hidden');
  for (const host of [status, $('export-preview-modal')?.querySelector('.export-preview-paper-wrapper')]) {
    if (!host) continue;
    host.replaceChildren(); host.setAttribute('aria-busy', 'false');
    const message = document.createElement('p'); message.textContent = _t('error.' + missing.code);
    const install = document.createElement('button'); install.type = 'button'; install.className = 'tb-btn'; install.textContent = _t('capability.installContinue');
    install.onclick = async event => {
      event.stopPropagation();
      if (await ReadMDCapabilities.ensure(missing.id) && generation === nativeExportPreview.generation) {
        nativeExportPreview.signature = ''; updateExportLivePreview();
      }
    };
    host.append(message, install);
  }
  const meta = $('export-preview-paper-meta'); if (meta) meta.textContent = _t('error.' + missing.code);
  for (const id of ['export-preview-prev-btn', 'export-preview-next-btn']) {
    const button = $(id); if (button) { button.disabled = true; button.classList.remove('is-visible'); }
  }
}
function requestNativeExportPreview(fmt, options, content, suggestedName, presetName) {
  const payload = { format: fmt, content, baseDir: state.dir || '', suggestedName, options };
  const signature = JSON.stringify(payload);
  if (nativeExportPreview.signature === signature) {
    if (nativeExportPreview.data) renderNativeExportPreview(presetName, options);
    else if (nativeExportPreview.missingCapability) renderNativeExportCapabilityRequired(nativeExportPreview.generation);
    else if (!$('export-preview-modal')?.classList.contains('hidden')) {
      const wrapper = $('export-preview-modal')?.querySelector('.export-preview-paper-wrapper');
      if (wrapper) window.ReadMDTask.loading(wrapper, 'export.previewGenerating');
    }
    return;
  }
  cancelNativeExportPreview();
  const generation = nativeExportPreview.generation;
  nativeExportPreview.signature = signature; nativeExportPreview.page = 1; nativeExportPreview.pageCache.clear();
  const mini = $('export-preview-mini-content');
  if (mini) window.ReadMDTask.loading(mini, 'export.previewGenerating');
  $('export-preview-mini-page')?.classList.add('is-native-preview');
  const meta = $('export-preview-paper-meta');
  if (meta) meta.textContent = `${fmt.toUpperCase()} · ${presetName} · ${_t('export.previewGenerating')}`;
  if (!$('export-preview-modal')?.classList.contains('hidden')) {
    const wrapper = $('export-preview-modal').querySelector('.export-preview-paper-wrapper');
    if (wrapper) window.ReadMDTask.loading(wrapper, 'export.previewGenerating');
  }
  nativeExportPreview.timer = setTimeout(async () => {
    const controller = new AbortController(); nativeExportPreview.controller = controller;
    const taskId=crypto.randomUUID();nativeExportPreview.taskId=taskId;
    try {
      payload.options=await prepareExportFigures(content,options,controller.signal);
      if(generation!==nativeExportPreview.generation)return;
      const response = await apiFetch('/api/export/preview', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({...payload,task_id:taskId}), signal: controller.signal });
      const data = await response.json();
      if (generation !== nativeExportPreview.generation) return;
      if (!response.ok || !data.ok) {
        const capability = {publication_fonts_unavailable:'fonts', typst_engine_unavailable:'typst', latex_engine_unavailable:'latex'}[data.error_code];
        if(capability){
          nativeExportPreview.missingCapability = {id:capability, code:data.error_code}; renderNativeExportCapabilityRequired(generation); return;
        }
        throw new Error(apiMessage(data,'export.previewFailed'));
      }
      nativeExportPreview.data = data;
      if (data.firstPage) nativeExportPreview.pageCache.set(1, data.firstPage);
      if (data.format !== 'epub' && !data.firstPage) {
        const bytes = Uint8Array.from(atob(data.pdf), c => c.charCodeAt(0));
        nativeExportPreview.blobUrl = URL.createObjectURL(new Blob([bytes], { type: 'application/pdf' }));
      }
      renderNativeExportPreview(presetName, options);
    } catch (error) {
      if (generation !== nativeExportPreview.generation || error.name === 'AbortError') return;
      if (mini) { mini.textContent = _t('export.previewFailed') + ': ' + error.message; mini.setAttribute('aria-busy', 'false'); }
      const wrapper = $('export-preview-modal')?.querySelector('.export-preview-paper-wrapper');
      if (wrapper) { wrapper.textContent = _t('export.previewFailed'); wrapper.setAttribute('aria-busy', 'false'); }
      nativeExportPreview.signature = '';
    } finally {
      if (nativeExportPreview.taskId === taskId) nativeExportPreview.taskId='';
    }
  }, 300);
}

function renderNativeExportPreview(presetName, options) {
  const data = nativeExportPreview.data;
  if (!data) return;
  renderExportWarns(data.warnings || []);
  if (data.format === 'epub') { renderEpubExportPreview(data, presetName); return; }
  const mini = $('export-preview-mini-content');
  const miniPage = $('export-preview-mini-page');
  if (miniPage) {
    miniPage.classList.add('is-native-preview');
    const viewport = miniPage.parentElement;
    if (viewport) miniPage.style.transform = 'scale(' + Math.min(2.5, (viewport.clientWidth - 32) / 110, (viewport.clientHeight - 32) / 148) + ')';
  }
  const image = (png, className) => {
    const img = document.createElement('img'); img.className = className;
    img.src = 'data:image/png;base64,' + png; img.alt = _t('export.actualPreview'); return img;
  };
  const accessible = () => { const p = document.createElement('p'); p.className = 'export-preview-accessible'; p.textContent = data.text || ''; return p; };
  if (mini) {
    mini.replaceChildren(); mini.setAttribute('aria-busy', 'false');
    if (data.firstPage) mini.appendChild(image(data.firstPage, 'export-native-page-image'));
    else mini.appendChild(document.createTextNode(_t('export.actualPreview')));
    mini.appendChild(accessible());
  }
  const pageText = _t('export.previewPagesMeta', { total: data.pages });
  const meta = $('export-preview-paper-meta');
  if (meta) meta.textContent = `${data.format.toUpperCase()} · ${options.page?.size || 'A4'} · ${pageText} · ${presetName}${data.mode === 'shared-layout' ? ' · ' + _t('export.docxLayoutReference') + ' (' + (data.previewEngine === 'typst' ? 'Typst' : _t('export.pdfEngineNative')) + ')' : ''}`;
  let style = $('export-preview-dynamic-style');
  if (!style) { style = document.createElement('style'); style.id = 'export-preview-dynamic-style'; document.head.appendChild(style); }
  style.textContent = '';
  const modal = $('export-preview-modal');
  if (!modal || modal.classList.contains('hidden')) return;
  const wrapper = modal.querySelector('.export-preview-paper-wrapper');
  if (!wrapper) return;
  wrapper.setAttribute('aria-busy', 'false'); wrapper.replaceChildren();
  const host = document.createElement('div'); host.id = 'export-preview-full-page';
  const sheet = document.createElement('div'); sheet.className = 'export-preview-page-sheet export-native-sheet'; sheet.dataset.page = String(nativeExportPreview.page);
  const dims = data.dimensions || [];
  if (dims.length === 4) { sheet.style.width = (dims[2] - dims[0]) * 96 / 72 + 'px'; sheet.style.height = (dims[3] - dims[1]) * 96 / 72 + 'px'; }
  const png = nativeExportPreview.pageCache.get(nativeExportPreview.page);
  if (png) sheet.appendChild(image(png, 'export-native-page-image'));
  else {
    const frame = document.createElement('iframe'); frame.className = 'export-native-pdf-frame'; frame.title = _t('export.actualPreview');
    frame.src = nativeExportPreview.blobUrl + '#page=' + nativeExportPreview.page + '&zoom=page-fit'; sheet.appendChild(frame);
  }
  sheet.appendChild(accessible()); host.appendChild(sheet); wrapper.appendChild(host);
  const pagesMeta = $('export-preview-pages-meta');
  if (pagesMeta) pagesMeta.textContent = `${nativeExportPreview.page} / ${data.pages}${data.mode === 'shared-layout' ? ' · ' + _t('export.docxLayoutReference') + ' (' + (data.previewEngine === 'typst' ? 'Typst' : _t('export.pdfEngineNative')) + ')' : ''}`;
  const changePage = async delta => {
    if (nativeExportPreview.pagePending) return;
    const page = Math.max(1, Math.min(data.pages, nativeExportPreview.page + delta));
    if (page === nativeExportPreview.page) return;
    const generation = nativeExportPreview.generation;
    const controller = new AbortController();
    nativeExportPreview.pageController = controller;
    nativeExportPreview.pagePending = true;
    const deadline = setTimeout(() => controller.abort(), 15000);
    ['export-preview-prev-btn', 'export-preview-next-btn'].forEach(id => { if ($(id)) $(id).disabled = true; });
    let failure = '';
    try {
      if (data.firstPage && !nativeExportPreview.pageCache.has(page)) {
        window.ReadMDTask.loading(wrapper, 'export.previewGenerating');
        const response = await apiFetch('/api/export/preview', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ key: data.key, page }), signal: controller.signal });
        const result = await response.json();
        if (generation !== nativeExportPreview.generation) return;
        if (!response.ok || !result.ok || typeof result.png !== 'string' || !result.png) throw new Error(result.error || _t('export.previewFailed'));
        nativeExportPreview.pageCache.set(page, result.png);
      }
      if (generation !== nativeExportPreview.generation) return;
      nativeExportPreview.page = page;
    } catch (error) { failure = error.message; }
    finally {
      clearTimeout(deadline);
      if (generation === nativeExportPreview.generation) {
        nativeExportPreview.pagePending = false;
        nativeExportPreview.pageController = null;
        renderNativeExportPreview(presetName, options);
        if (failure) {
          if (pagesMeta) pagesMeta.textContent = `${nativeExportPreview.page} / ${data.pages} · ${_t('export.previewFailed')}`;
          showToast(_t('export.previewFailed'));
        }
      }
    }
  };
  [['export-preview-prev-btn', -1], ['export-preview-next-btn', 1]].forEach(([id, delta]) => {
    const button = $(id); if (!button) return;
    button.classList.toggle('is-visible', data.pages > 1);
    button.disabled = nativeExportPreview.pagePending || (delta < 0 ? nativeExportPreview.page <= 1 : nativeExportPreview.page >= data.pages);
    button.onclick = () => changePage(delta);
  });
}

function renderEpubExportPreview(data, presetName) {
  const frame=()=>{
    const el=document.createElement('iframe');el.className='export-epub-frame';el.title=_t('export.actualPreview');
    el.addEventListener('load',()=>{
      // srcdoc inherits the host URL as its base. A bare #anchor would load
      // that URL inside the frame (blocked by the native CSP), so keep EPUB
      // chapter/footnote navigation in the already loaded viewing document.
      el.contentDocument?.addEventListener('click',event=>{
        const link=event.target.closest('a[href]'),href=link?.getAttribute('href');
        if(!href?.startsWith('#'))return;
        event.preventDefault();
        let id;try{id=decodeURIComponent(href.slice(1));}catch(_){return;}
        const target=el.contentDocument.getElementById(id);
        if(!target)return;
        target.scrollIntoView({block:'start'});
        // Fractional device scaling can round the anchor above the viewport.
        // Leave a small reading margin without changing the exported EPUB.
        el.contentWindow.scrollBy(0,-8);
      });
    });
    el.setAttribute('sandbox','allow-same-origin');el.srcdoc=data.html;return el;
  };
  const mini=$('export-preview-mini-content'),miniPage=$('export-preview-mini-page');
  miniPage?.classList.add('is-native-preview');
  if(miniPage?.parentElement)miniPage.style.transform='scale('+Math.min(2.5,(miniPage.parentElement.clientWidth-32)/110,(miniPage.parentElement.clientHeight-32)/148)+')';
  if(mini){mini.replaceChildren(frame());mini.setAttribute('aria-busy','false');}
  const meta=$('export-preview-paper-meta');if(meta)meta.textContent='EPUB · '+_t('export.actualPreview')+' · '+presetName;
  let style=$('export-preview-dynamic-style');if(style)style.textContent='';
  const modal=$('export-preview-modal');if(!modal||modal.classList.contains('hidden'))return;
  const wrapper=modal.querySelector('.export-preview-paper-wrapper');if(!wrapper)return;
  wrapper.setAttribute('aria-busy','false');wrapper.replaceChildren();
  const host=document.createElement('div');host.id='export-preview-full-page';host.className='export-epub-preview';host.append(frame());wrapper.append(host);
  const count=$('export-preview-pages-meta');if(count)count.textContent='';
  ['export-preview-prev-btn','export-preview-next-btn'].forEach(id=>{const button=$(id);if(button){button.disabled=true;button.classList.remove('is-visible');button.onclick=null;}});
}

const htmlFigurePreview={key:'',source:null,error:'',controller:null};
function htmlExportFigureContent(content,options) {
  const key=JSON.stringify([content,options.figures||{}]);
  if(htmlFigurePreview.key!==key){
    htmlFigurePreview.controller?.abort();const controller=new AbortController();
    Object.assign(htmlFigurePreview,{key,source:null,error:'',controller});
    (async()=>{
      try{
        const prepared=await prepareExportFigures(content,options,controller.signal);
        const r=await apiFetch('/api/diagram/render',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({action:'materialize',content,figures:prepared.figures}),signal:controller.signal});
        const data=await r.json();if(!data.ok)throw new Error(apiMessage(data,'reader.diagramRenderFailed'));
        if(htmlFigurePreview.key===key)htmlFigurePreview.source=data.content;
      }catch(e){if(!controller.signal.aborted&&htmlFigurePreview.key===key)htmlFigurePreview.error=e.message;}
      finally{if(!controller.signal.aborted&&htmlFigurePreview.key===key&&state.export.fmt==='html')updateExportLivePreview();}
    })();
  }
  if(htmlFigurePreview.source!==null)return htmlFigurePreview.source;
  for(const host of [$('export-preview-mini-content'),$('export-preview-modal')?.querySelector('.export-preview-paper-wrapper')]){
    if(!host)continue;
    if(htmlFigurePreview.error){host.replaceChildren(document.createTextNode(htmlFigurePreview.error));host.setAttribute('aria-busy','false');}
    else window.ReadMDTask.loading(host,'export.previewGenerating');
  }
  return null;
}
function updateExportLivePreview() {
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  const fmt = state.export.fmt || 'pdf';
  const opts = collectExportOptions();
  // TeX source export supports typed spacing; its browser-only mock page is
  // not a TeX renderer. Do not present that approximation as a verified layout.
  const spacing=window.ReadMDPublication?.lineSpacing,tracking=window.ReadMDPublication?.tracking,baseline=window.ReadMDPublication?.baseline,spacingError=spacing?.validateExport($('export-opts'))===false||tracking?.validateExport($('export-opts'))===false||baseline?.validateExport($('export-opts'))===false?'error.publication_local_style_invalid':spacing?.issue(opts,fmt==='tex'?'html':fmt,opts.pdf?.engine)||tracking?.issue(opts,fmt)||baseline?.issue(opts,fmt,opts.pdf?.engine)||(fmt==='epub'&&(!validateEpubNumericControls()?'error.epub_invalid_options':expEpubNumericIssue(opts)));
  if(spacingError){cancelNativeExportPreview();const message=_t(spacingError);for(const id of ['export-preview-mini-content','export-preview-paper-meta']){const target=$(id);if(target){target.textContent=message;target.setAttribute('aria-busy','false');}}const wrapper=$('export-preview-modal')?.querySelector('.export-preview-paper-wrapper');if(wrapper){wrapper.textContent=message;wrapper.setAttribute('aria-busy','false');}return;}
  if(state.editing&&state.pvPublication)schedulePreview();
  const badge = $('export-preview-badge');
  const sel = $('exp-preset');
  const presetName = (sel && sel.selectedIndex >= 0) ? sel.options[sel.selectedIndex].text : (_t('export.presetDefault') || '');
  const fmtLabel = fmt === 'presentation' ? (_t('export.fmtSlides') || 'Slides') : fmt.toUpperCase();
  if (badge) badge.textContent = fmtLabel + ' · ' + presetName;
  renderExportPresetCards();

  let content = currentExportContent();
  const docTitle = currentExportName();
  if (window.READMD_ENGINE === 'rust' && (fmt === 'pdf' || fmt === 'docx' || fmt === 'epub')) {
    requestNativeExportPreview(fmt, opts, content, docTitle, presetName);
    return;
  }
  cancelNativeExportPreview();
  if(fmt==='html'&&/(?:`{3,}|~{3,})\s*(?:vega(?:-lite)?|mermaid|chart(?:js|\.js)?|graphviz|viz|dot|tikz|plantuml|puml|wavedrom|bitfield|wsd|d2|ditaa)\b/i.test(content)){
    content=htmlExportFigureContent(content,opts);if(content===null)return;
  }else{htmlFigurePreview.controller?.abort();Object.assign(htmlFigurePreview,{key:'',source:null,error:'',controller:null});}

  // 注入或更新动态样式表
  let styleEl = $('export-preview-dynamic-style');
  if (!styleEl) {
    styleEl = document.createElement('style');
    styleEl.id = 'export-preview-dynamic-style';
    document.head.appendChild(styleEl);
  }
  styleEl.textContent = generateExportPreviewCss(opts, fmt);

  const isHtmlMode = fmt === 'html';
  const fullProt = protectMath(content || '');
  const fullParsedHtml = marked.parse(fullProt.src, { gfm: true, breaks: isHtmlMode });
  const restoredFullHtml = restoreMath(fullParsedHtml, fullProt.saved);

  const pageHtmlList = isHtmlMode ? [restoredFullHtml] : paginateHtmlIntoExportSheets(restoredFullHtml, opts);
  const totalPages = pageHtmlList.length;

  const paperMeta = $('export-preview-paper-meta');
  if (paperMeta) {
    const page = opts.page || {};
    const sz = page.size || 'A4';
    const ori = (page.orientation === 'landscape') ? (_t('export.orientationLandscape') || '') : (_t('export.orientationPortrait') || '');
    const pageText = isHtmlMode ? 'HTML Web' : (_t('reader.totalPage', { total: totalPages }) || `共 ${totalPages} 页`);
    paperMeta.textContent = `${sz} · ${ori} · ${pageText} · ${presetName}`;
  }

  // Mini Preview 侧边栏预览
  const miniHost = $('export-preview-mini-content');
  if (miniHost) {
    miniHost.setAttribute('aria-busy','false');
    miniHost.innerHTML = pageHtmlList[0] || restoredFullHtml;
    renderMath(miniHost);
    const miniPage = $('export-preview-mini-page');
    const viewport = miniPage && miniPage.parentElement;
    if (viewport) miniPage.style.transform = 'scale(' + Math.min(2.5, (viewport.clientWidth - 32) / 110, (viewport.clientHeight - 32) / 148) + ')';
  }

  // Full Modal Preview 真实多页排版渲染
  const fullModal = $('export-preview-modal');
  if (fullModal && !fullModal.classList.contains('hidden')) {
    const wrapper = fullModal.querySelector('.export-preview-paper-wrapper');
    if (wrapper) {
      wrapper.setAttribute('aria-busy','false');
      // Keep the stable full-page host in the DOM.  Besides preserving CSS
      // hooks, this makes the preview accessible to keyboard/screen-reader
      // clients and avoids tests or extensions losing their target after a
      // style refresh.
      wrapper.innerHTML = '<div id="export-preview-full-page" class="export-preview-full-page"></div>';
      const fullPageHost = wrapper.querySelector('#export-preview-full-page');

      const esc = s => String(s || '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

      pageHtmlList.forEach((pageHtml, index) => {
        const sheet = document.createElement('div');
        sheet.className = isHtmlMode ? 'export-preview-page-sheet export-preview-html-sheet' : 'export-preview-page-sheet';
        sheet.dataset.page = (index + 1).toString();

        if (!isHtmlMode) {
          const headerEl = document.createElement('div');
          headerEl.className = 'export-page-header';
          headerEl.textContent = (opts.header || {}).text || '';
          headerEl.style.justifyContent = ({ left: 'flex-start', center: 'center', right: 'flex-end' })[(opts.header || {}).align] || 'flex-start';
          if (headerEl.textContent) sheet.appendChild(headerEl);
        }

        const bodyEl = document.createElement('div');
        bodyEl.className = 'export-page-body';
        bodyEl.innerHTML = pageHtml;
        sheet.appendChild(bodyEl);

        if (!isHtmlMode) {
          const footerEl = document.createElement('div');
          footerEl.className = 'export-page-footer';
          const footer = opts.footer || {};
          footerEl.innerHTML = `<span>${esc(footer.text)}</span><span>${footer.pageNumbers !== false ? index + 1 : ''}</span>`;
          if (footer.text || footer.pageNumbers !== false) sheet.appendChild(footerEl);
        }

        fullPageHost.appendChild(sheet);
        renderMath(bodyEl);
      });

      const pagesMeta = $('export-preview-pages-meta');
      if (pagesMeta) {
        pagesMeta.textContent = (window.i18n ? window.i18n.t('export.previewPagesMeta', { total: totalPages }) : '') || `共 ${totalPages} 页`;
      }
      const prevBtn = $('export-preview-prev-btn');
      const nextBtn = $('export-preview-next-btn');
      if (prevBtn && nextBtn) {
        if (totalPages > 1) {
          prevBtn.classList.add('is-visible');
          nextBtn.classList.add('is-visible');
          let curIdx = 0;
          prevBtn.onclick = () => {
            const sheets = fullPageHost.querySelectorAll('.export-preview-page-sheet');
            if (curIdx > 0) {
              curIdx--;
              sheets[curIdx]?.scrollIntoView({ behavior: 'smooth', block: 'start' });
            }
          };
          nextBtn.onclick = () => {
            const sheets = fullPageHost.querySelectorAll('.export-preview-page-sheet');
            if (curIdx < sheets.length - 1) {
              curIdx++;
              sheets[curIdx]?.scrollIntoView({ behavior: 'smooth', block: 'start' });
            }
          };
        } else {
          prevBtn.classList.remove('is-visible');
          nextBtn.classList.remove('is-visible');
        }
      }
    }
  }
}

function expFieldApplicable(f, secFmts, fmt) {
  if(f.type==='tracking')return fmt==='docx'||state.export.options?.typography?.trackingPt!=null;
  if(f.type==='baseline')return fmt==='docx'||fmt==='pdf'&&(!state.export.options?.pdf?.engine||state.export.options.pdf.engine==='rust')||state.export.options?.typography?.baselineShiftPt!=null;
  if(f.type==='lineSpacing')return ['pdf','docx','html'].includes(fmt)||['epub','tex','presentation'].includes(fmt)&&state.export.options?.typography?.lineSpacing!=null;
  if(['tex','presentation'].includes(fmt)&&/^(?:typography|footnotes)\./.test(f.k))return false;
  if(fmt==='pdf'&&f.engines&&!f.engines.includes(state.export.options?.pdf?.engine||'rust'))return false;
  if(f.k==='footnotes.title'&&state.export.options?.footnotes?.position!=='document')return false;
  if((f.k==='toc.depth'||f.k==='toc.leader')&&fmt!=='epub'&&!state.export.options?.toc?.enabled)return false;
  const fmts = f.fmts || secFmts || ['pdf', 'docx', 'html'];
  return fmts.indexOf(fmt) >= 0;
}

function renderExportSections(preserveSections = false) {
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  const fmt = state.export.fmt || 'pdf';
  const host = $('export-opts');
  const sectionState = preserveSections ? new Map([...host.querySelectorAll('.exp-sec')].map(section => [section.dataset.section, section.classList.contains('open')])) : null;
  const focusedKey = preserveSections && host.contains(document.activeElement) ? document.activeElement.dataset.k : null;
  const texPdf=fmt==='pdf'&&['xelatex','pdflatex'].includes(state.export.options?.pdf?.engine);
  $('export-modal')?.querySelector('.exp-ai-style-card')?.classList.toggle('hidden',texPdf);
  host.textContent = '';
  const sections = getExportSections();
  let renderedCount = 0;

  sections.forEach(sec => {
    const secFmts = sec.fmts || ['pdf', 'docx', 'html'];
    if (secFmts.indexOf(fmt) < 0) return;
    const fields = sec.fields || [];
    const applicable = sec.headingRows || sec.publicationMetadata ? (secFmts.indexOf(fmt) >= 0)
      : fields.some(f => expFieldApplicable(f, secFmts, fmt));
    if (!applicable) return;

    const wrap = document.createElement('div');
    wrap.className = 'exp-sec'; // 默认折叠，点击标题展开
    wrap.dataset.section = sec.publicationMetadata ? 'publication-metadata' : sec.headingRows ? 'headings' : fields[0]?.k || sec.title;
    if (sectionState?.has(wrap.dataset.section) ? sectionState.get(wrap.dataset.section) : renderedCount === 0) wrap.classList.add('open');
    renderedCount++;

    const head = document.createElement('button');
    head.type = 'button';
    head.className = 'exp-sec-head';
    head.innerHTML = '<span class="exp-arrow">&#9654;</span>' + sec.title;
    const body = document.createElement('div');
    body.className = 'exp-sec-body';
    if (sec.publicationMetadata) {
      window.ReadMDPublicationMetadata?.mountExport(body, state.export.options || {}, () => {
        state.export.options = collectExportOptions();
        const select = $('exp-preset'); if (select) select.value = '__custom__';
        state.export.selectedPreset = '__custom__'; updateExportLivePreview();
      });
    } else if (sec.headingRows) {
      for (let i = 1; i <= 6; i++) {
        const row = document.createElement('div');
        row.className = 'exp-field full exp-h-row';
        row.innerHTML =
          '<label>H' + i + '</label>' +
          '<input type="number" data-k="headings.h' + i + '.size" min="8" max="40" title="' + (_t('export.bodySize') || '') + '">' +
          '<input type="color" data-k="headings.h' + i + '.color" title="' + (_t('export.bodyColor') || '') + '">' +
          '<label class="exp-check">' + (_t('export.bold') || '') + '<input type="checkbox" data-k="headings.h' + i + '.bold"></label>' +
          '<select data-k="headings.h' + i + '.align"></select>';
        EXPORT_ALIGNS.forEach(a => row.querySelector('select').add(new Option(_t({left:'export.alignLeft',center:'export.alignCenter',right:'export.alignRight',justify:'export.alignJustify'}[a]), a)));
        body.appendChild(row);
        if(!texPdf) {
          const extra=document.createElement('details');extra.className='exp-heading-extra';
          const summary=document.createElement('summary');summary.textContent=_t('export.headingPagination',{level:i});extra.append(summary);
          const group=document.createElement('div');group.className='exp-heading-fields';
          for(const field of [
            {k:`headings.h${i}.before`,label:_t('export.headingBefore'),type:'number',min:0,max:60,default:12},
            {k:`headings.h${i}.after`,label:_t('export.headingAfter'),type:'number',min:0,max:40,default:6},
            {k:`headings.h${i}.pageBreakBefore`,label:_t('export.chapterPageBreak'),type:'checkbox',default:false},
            ...((fmt==='docx'||fmt==='html'||fmt==='epub'||fmt==='pdf'&&state.export.options?.pdf?.engine==='typst')?[
              {k:`headings.h${i}.keepNext`,label:_t('export.keepNext'),type:'checkbox',default:true},
              {k:`headings.h${i}.keepLines`,label:_t('export.keepLines'),type:'checkbox',default:true},
            ]:[]),
          ])group.append(expFieldEl(field));
          extra.append(group);body.append(extra);
        }
      }
    } else {
      fields.forEach(f => {
        if (!expFieldApplicable(f, secFmts, fmt)) return;
        body.appendChild(expFieldEl(f));
      });
    }
    head.addEventListener('click', () => wrap.classList.toggle('open'));
    wrap.appendChild(head);
    wrap.appendChild(body);
    host.appendChild(wrap);
  });
  applyExportOptionsToDom();

  // 绑定配置项实时变动事件
  host.querySelectorAll('input, select').forEach(el => {
    if (el.closest('#publication-metadata-export')||el.closest('[data-line-spacing-control]')) return;
    const onValChange = () => {
      if (EXPORT_EPUB_NUMERIC[el.dataset.k]) delete el.dataset.epubNumericInvalid;
      state.export.options = collectExportOptions();
      if(el.dataset.k==='cover.enabled'||el.dataset.k==='page.differentOddEven')applyExportOptionsToDom();
      const sel = $('exp-preset');
      if (sel) sel.value = '__custom__';
      state.export.selectedPreset = '__custom__';
      if (el.dataset.k === 'pdf.engine'||el.dataset.k==='footnotes.position'||el.dataset.k==='toc.enabled') renderExportSections(true);
      else updateExportLivePreview();
    };
    el.addEventListener('input', onValChange);
    el.addEventListener('change', onValChange);
  });

  if (focusedKey) [...host.querySelectorAll('[data-k]')].find(el => el.dataset.k === focusedKey)?.focus({preventScroll:true});
  updateExportLivePreview();
}



function expFieldEl(f) {
  const box = document.createElement('div');
  box.className = 'exp-field' + (f.full ? ' full' : '');
  if(f.type==='tracking'||f.type==='baseline'){
    window.ReadMDPublication[f.type].mount(box,{style:state.export.options?.typography||{},getFormat:()=>({format:state.export.fmt,engine:state.export.options?.pdf?.engine}),onChange:()=>{
      state.export.options=collectExportOptions();const select=$('exp-preset');if(select)select.value='__custom__';state.export.selectedPreset='__custom__';updateExportLivePreview();
    }});return box;
  }
  if(f.type==='lineSpacing'){
    window.ReadMDPublication.lineSpacing.mount(box,{scope:'global',style:state.export.options?.typography||{},getFormat:()=>({format:state.export.fmt,engine:state.export.options?.pdf?.engine}),onChange:()=>{
      state.export.options=collectExportOptions();const select=$('exp-preset');if(select)select.value='__custom__';state.export.selectedPreset='__custom__';
      updateExportLivePreview();
    }});return box;
  }
  const fieldId = 'exp-field-' + f.k.replace(/[^a-z0-9_-]+/ig, '-') + '-' + Math.random().toString(36).slice(2, 7);
  let inner = '<label for="' + fieldId + '">' + f.label + '</label>';
  if (f.type === 'select') {
    inner += '<select id="' + fieldId + '" data-k="' + f.k + '">' + (f.opts || []).map(o =>
      '<option value="' + (Array.isArray(o) ? o[0] : o) + '">' + (Array.isArray(o) ? o[1] : o) + '</option>'
    ).join('') + '</select>';
  } else if (f.type === 'note') {
    const p = document.createElement('p');
    p.className = 'exp-note';
    p.textContent = f.label;
    box.appendChild(p);
    return box;
  } else if (f.type === 'checkbox') {
    inner = '<label class="exp-check"><input id="' + fieldId + '" type="checkbox" data-k="' + f.k + '"> ' + f.label + '</label>';
  } else if (f.type === 'color') {
    inner += '<input id="' + fieldId + '" type="color" data-k="' + f.k + '">';
  } else if (f.type === 'number') {
    inner += '<input id="' + fieldId + '" type="number" data-k="' + f.k + '" min="' + (f.min != null ? f.min : '') + '" max="' + (f.max != null ? f.max : '') + '" step="' + (f.step != null ? f.step : '1') + '">';
  } else {
    inner += '<input id="' + fieldId + '" type="text" data-k="' + f.k + '">';
  }
  box.innerHTML = inner;
  if (f.default !== undefined) box.querySelector('[data-k]')?.setAttribute('data-default', JSON.stringify(f.default));
  if (f.type === 'asset') {
    const input=box.querySelector('input');input.title=i18n.t('slash.imageDesc');
    const row=document.createElement('div');row.className='exp-asset';input.replaceWith(row);row.append(input);
    const button=document.createElement('button');button.type='button';button.className='btn icon-btn';button.textContent='…';button.title=i18n.t('editor.image');button.setAttribute('aria-label',i18n.t('editor.image'));
    button.onclick=async()=>{
      try {
      if (hasPy && typeof py.choose_any_file==='function') {
        const file=await py.choose_any_file();if(file){input.value=file;input.dispatchEvent(new Event('change',{bubbles:true}));}
      } else {
        const picker=document.createElement('input');picker.type='file';picker.accept='image/png,image/jpeg,image/gif,image/svg+xml';
        picker.onchange=()=>{const file=picker.files?.[0];if(!file)return;if(file.size>25*1024*1024){showToast(i18n.t('error.epub_cover'));return;}const reader=new FileReader();reader.onload=()=>{if(!input.isConnected)return;input.value=String(reader.result);input.dispatchEvent(new Event('change',{bubbles:true}));};reader.readAsDataURL(file);};picker.click();
      }
      } catch (_) { showToast(i18n.t('error.epub_cover')); }
    };row.append(button);
  }
  return box;
}

function applyExportOptionsToDom() {
  const opts = state.export.options || {};
  document.querySelectorAll('#export-opts [data-k]').forEach(el => {
    if(el.dataset.k==='page.differentFirstPage')el.dataset.default=JSON.stringify(!!opts.cover?.enabled);
    const v = expGet(opts, el.dataset.k) ?? (el.dataset.default !== undefined ? JSON.parse(el.dataset.default) : {'figures.style':'original','figures.widthMm':89,'figures.fontSizePt':7}[el.dataset.k]);
    const numeric = EXPORT_EPUB_NUMERIC[el.dataset.k];
    if (numeric) {
      // An empty field stays sparse; the placeholder explains its effective
      // value without converting the inherited value into an author override.
      const inherited = (numeric.inherit && (expGet(opts, numeric.inherit) ?? expGet(state.export.defaults, numeric.inherit))) ?? numeric.fallback;
      el.placeholder = i18n.t('pm.inherit') + ' · ' + inherited + numeric.unit;
      el.title = el.placeholder;
      el.value = v == null ? '' : String(v);
      if (v != null && (typeof v !== 'number' || !Number.isFinite(v))) el.dataset.epubNumericInvalid = JSON.stringify(v);
      else delete el.dataset.epubNumericInvalid;
      return;
    }
    if (v === undefined || v === null) return;
    // Preserve an explicit stored tag even when it is outside the short list.
    if (el.dataset.k === 'epub.language' && typeof v === 'string' && v && !Array.from(el.options).some(o => o.value === v)) el.add(new Option(v, v));
    if (el.type === 'checkbox') el.checked = !!v;
    else el.value = v;
  });
  window.ReadMDPublication?.lineSpacing.refreshExport($('export-opts'),opts.typography);
  window.ReadMDPublication?.tracking.refreshExport($('export-opts'),opts.typography);
  window.ReadMDPublication?.baseline.refreshExport($('export-opts'),opts.typography);
  validateEpubNumericControls();
  for(const key of ['header.evenText','footer.evenText']){
    const el=document.querySelector('#export-opts [data-k="'+key+'"]');
    if(el){el.disabled=!opts.page?.differentOddEven;el.closest('.exp-field').hidden=el.disabled;}
  }
}

function collectExportOptions() {
  // Fields absent from the current format still belong to the selected preset.
  const opts = expDeepMerge(state.export.defaults, state.export.options);
  document.querySelectorAll('#export-opts [data-k]').forEach(el => {
    let v;
    if (EXPORT_EPUB_NUMERIC[el.dataset.k]) {
      if (el.dataset.epubNumericInvalid !== undefined) v = JSON.parse(el.dataset.epubNumericInvalid);
      else if (el.validity?.badInput) v = el.value || 'invalid';
      else if (el.value.trim() === '') {
        if (opts.epub) delete opts.epub[el.dataset.k.slice(5)];
        return;
      } else {
        v = Number(el.value);
        if (!Number.isFinite(v)) v = el.value;
      }
      expSet(opts, el.dataset.k, v); return;
    }
    if (el.type === 'checkbox') v = el.checked;
    else if (el.type === 'number') {
      v = parseFloat(el.value);
      if (!Number.isFinite(v)) v = expGet(opts, el.dataset.k) ?? (Number(el.min) || 0);
      if (el.min !== '') v = Math.max(Number(el.min), v);
      if (el.max !== '') v = Math.min(Number(el.max), v);
    }
    else v = el.value;
    if (el.dataset.k === 'epub.language' && v === '') {
      if (opts.epub) delete opts.epub.language;
      return;
    }
    // Displaying shared defaults must not materialize optional overrides in
    // old presets. Explicit changes, including a return to a default, persist.
    if(expGet(opts,el.dataset.k)===undefined&&el.dataset.default!==undefined&&v===JSON.parse(el.dataset.default))return;
    expSet(opts, el.dataset.k, v);
  });
  if (opts.footnotes?.position === 'document' && !opts.footnotes.title?.trim()) {
    opts.footnotes.title = i18n.t('export.noteEndnotes');
  }
  opts.calloutTitles = Object.fromEntries(['note', 'tip', 'important', 'warning', 'caution'].map(kind =>
    [kind, i18n.t('reader.callout' + kind[0].toUpperCase() + kind.slice(1))]));
  window.ReadMDPublication?.lineSpacing.collectExport(opts,$('export-opts'));
  window.ReadMDPublication?.tracking.collectExport(opts,$('export-opts'));
  window.ReadMDPublication?.baseline.collectExport(opts,$('export-opts'));
  return window.ReadMDPublicationMetadata?.collectExport(opts) || opts;
}

// AI providers sometimes return a flat dotted object while the exporter uses
// nested options. Normalize both forms and drop unknown keys before merging so
// a model cannot mutate unrelated export state.
function normalizeExportAiPayload(value) {
  const allowed = new Set(['typography', 'headings', 'table', 'page', 'epub']);
  const ALLOWED_EPUB_KEYS = new Set([
    'title', 'author', 'publisher', 'isbn', 'language', 'cover',
    'splitLevel', 'fontSize', 'lineHeight', 'marginV', 'marginH',
    'css', 'toc', 'generateToc'
  ]);
  const out = {};
  const visit = (obj, prefix = '') => {
    if (!obj || typeof obj !== 'object' || Array.isArray(obj)) return;
    Object.entries(obj).forEach(([key, val]) => {
      const full = prefix ? prefix + '.' + key : key;
      if (full.includes('.')) {
        const parts = full.split('.');
        if (!allowed.has(parts[0]) || parts.length > 4) return;
        if (parts[0] === 'epub' && !ALLOWED_EPUB_KEYS.has(parts[1])) return;
        expSet(out, full, val);
      } else if (allowed.has(key) && val && typeof val === 'object' && !Array.isArray(val)) {
        visit(val, full);
      }
    });
  };
  visit(value);
  return out;
}

function exportPresetOptions(name) {
  if (name === '__custom__') return state.export.options || {};
  const preset = name === '__default__' ? {} : (state.export.presets[name] || state.export.custom[name] || {});
  return expDeepMerge(state.export.defaults, preset);
}

function renderExportPresetSelect() {
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  const sel = $('exp-preset');
  sel.textContent = '';
  const presetNames = getExportPresetNames();
  const catalog=state.export.catalog||[],fmt=state.export.fmt||'pdf';
  const names = Object.keys(state.export.presets || {}).filter(n=>{const p=catalog.find(p=>p.id===n);return !p||p.formats?.includes(fmt);});
  sel.appendChild(new Option(_t('export.presetDefault') || 'Default', '__default__'));
  const categories=[['common','export.presetsCommon'],['book','export.presetsBook'],['academic','export.presetsAcademic']];
  const added=new Set();
  categories.forEach(([category,key])=>{
    const members=names.filter(n=>catalog.find(p=>p.id===n)?.category===category);
    if(!members.length)return;
    const group=document.createElement('optgroup');group.label=_t(key);
    members.forEach(n=>{group.append(new Option(presetNames[n]||n,n));added.add(n);});sel.append(group);
  });
  names.filter(n=>!added.has(n)).forEach(n=>sel.append(new Option(presetNames[n]||n,n)));
  const customNames=Object.keys(state.export.custom||{});
  if(customNames.length){const group=document.createElement('optgroup');group.label=_t('export.presetCustom');customNames.forEach(n=>group.append(new Option(n,n)));sel.append(group);}
  sel.appendChild(new Option(_t('export.presetCustom') || '', '__custom__'));
  const last = state.export.last;
  const remembered = state.export.selectedPreset || (last && last.preset);
  sel.value = remembered && [...sel.options].some(o => o.value === remembered)
    ? remembered
    : (last ? '__custom__' : '__default__');
  if(remembered&&sel.value!==remembered){sel.value='__custom__';state.export.selectedPreset='__custom__';}
  sel.onchange = () => {
    const v = sel.value;
    state.export.selectedPreset = v;
    if (v !== '__custom__') {
      state.export.options = exportPresetOptions(v);
      renderExportSections();
    }
    renderExportPresetCards();
  };
  renderExportPresetCards();
}

/* Visual preset gallery: one card per preset with its signature colours, so a
   style can be picked by look instead of by name. Mirrors #exp-preset. */
function renderExportPresetCards() {
  const host = $('exp-preset-cards');
  const sel = $('exp-preset');
  if (!host || !sel || !state.export.defaults) return;
  host.textContent = '';
  const recommended=state.export.fmt==='epub'?['book_novel','book_nonfiction','book_textbook']:['minimal','classic','business'];
  [...sel.options].forEach(o => {
    const v = o.value;
    if(v!=='__default__'&&v!==sel.value&&!recommended.includes(v))return;
    if (v === '__custom__' && sel.value !== '__custom__') return;
    const opts = exportPresetOptions(v);
    const card = document.createElement('button');
    card.type = 'button';
    card.className = 'exp-preset-card';
    card.dataset.preset = v;
    card.title = o.text;
    card.setAttribute('aria-pressed', String(sel.value === v));
    const swatch = document.createElement('span');
    swatch.className = 'exp-preset-swatch';
    swatch.setAttribute('aria-hidden', 'true');
    const h1 = (opts.headings && opts.headings.h1) || {};
    [h1.color, (opts.table || {}).headerBg, (opts.link || {}).color, (opts.quote || {}).barColor].forEach(c => {
      const dot = document.createElement('i');
      if (c) dot.style.background = c;
      swatch.appendChild(dot);
    });
    const name = document.createElement('span');
    name.className = 'exp-preset-name';
    name.textContent = o.text;
    const ty = opts.typography || {};
    const meta = document.createElement('span');
    meta.className = 'exp-preset-meta';
    meta.textContent = [ty.size ? ty.size + 'pt' : '', ty.lineHeight || ''].filter(Boolean).join(' · ');
    card.append(swatch, name, meta);
    card.addEventListener('click', () => { sel.value = v; sel.onchange(); });
    host.appendChild(card);
  });
}


/* Single flight: a double click (or Enter + click) never sends two exports. */
function runExport() {
  if (!validateEpubNumericControls(true)) return false;
  if(window.ReadMDPublication?.lineSpacing.validateExport($('export-opts'),true)===false||window.ReadMDPublication?.tracking.validateExport($('export-opts'),true)===false||window.ReadMDPublication?.baseline.validateExport($('export-opts'),true)===false)return false;
  if (!window.ReadMDTask) return runExportOnce();
  if (window.ReadMDTask.isRunning('export')) return;
  cancelNativeExportPreview();
  const res = $('export-result');
  if (res) { res.className = 'export-result'; res.title = ''; }
  $('export-open')?.classList.add('hidden');
  $('export-reveal')?.classList.add('hidden');
  renderExportWarns([]);
  // EPUB and the browser-only presentation preview have their own routes and
  // are not cancellable; everything through /api/export is.
  const cancellable = state.export.fmt !== 'epub';
  const taskId = cancellable ? window.ReadMDTask.newTaskId('export') : '';
  return window.ReadMDTask.run('export', () => runExportOnce(taskId), {
    trigger: ['export-run'],
    status: res,
    label: (window.i18n ? window.i18n.t('task.exporting') : '') || '',
    cancel: cancellable ? { id: taskId, button: 'export-cancel' } : null,
  });
}

async function runExportOnce(taskId) {
  if (!validateEpubNumericControls(true)) return false;
  if(window.ReadMDPublication?.lineSpacing.validateExport($('export-opts'),true)===false||window.ReadMDPublication?.tracking.validateExport($('export-opts'),true)===false||window.ReadMDPublication?.baseline.validateExport($('export-opts'),true)===false)return false;
  const fmt = state.export.fmt;
  let options = collectExportOptions();
  if (fmt === 'epub' && expEpubNumericIssue(options)) { showToast(i18n.t('error.epub_invalid_options')); return false; }
  const spacingError=window.ReadMDPublication?.lineSpacing.issue(options,fmt,options.pdf?.engine)||window.ReadMDPublication?.tracking.issue(options,fmt)||window.ReadMDPublication?.baseline.issue(options,fmt,options.pdf?.engine);if(spacingError){showToast(i18n.t(spacingError));return false;}
  const content = currentExportContent();
  const baseDir = state.dir || '';
  const suggestedName = currentExportName();
  const preset = ($('exp-preset') || {}).value || '__custom__';
  const engineCapability={typst:'typst',xelatex:'latex',pdflatex:'latex'}[options.pdf?.engine];
  if(fmt==='pdf'&&engineCapability){
    const epoch=exportOpenEpoch;
    if(!await window.ReadMDCapabilities.ensure(engineCapability)||epoch!==exportOpenEpoch)return false;
  }
  if(fmt==='pdf'&&!await ReadMDCapabilities.ensurePublication(content,options))return false;
  busy(true);
  let r = null;
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  const failedResult = reason => {
    const failed = $('export-result');
    if (failed) { failed.textContent = _t('toast.exportFailed') + reason; failed.className = 'export-result err'; failed.title = reason; }
    showToast(_t('toast.exportFailed') + reason);
  };

  try {
    options=await prepareExportFigures(content,options);
    if (fmt === 'epub') {
      const epubOpts = options.epub || options.meta || {};
      const epubPayload = {
        title: epubOpts.title || '',
        author: epubOpts.author || '',
        publisher: epubOpts.publisher || '',
        isbn: epubOpts.isbn || '',
        splitLevel: epubOpts.splitLevel || 'h1',
        fontSize: epubOpts.fontSize,
        lineHeight: epubOpts.lineHeight,
        marginV: epubOpts.marginV,
        marginH: epubOpts.marginH,
        ...epubOpts
      };
      // Automatic language stays absent; source/common metadata resolves it.
      // Keep the backend's legacy fallback for documents without a declaration.
      const fullPayload = { ...options, epub: epubPayload };
      if (hasPy && py.export_epub) {
        r = await py.export_epub(content, '', epubPayload, fullPayload, baseDir);
      } else {
        const resp = await apiFetch('/api/export/epub', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ content: content, meta: epubPayload, epub: epubPayload, options: fullPayload, baseDir: baseDir, confirm: true })
        });
        r = await resp.json();
      }
    } else if (fmt === 'presentation' && !(window.READMD_ENGINE === 'rust' || (hasPy && py && typeof py.export_doc === 'function'))) {
      // Browser-only fallback: the in-app preview endpoint returns the HTML.
      const resp = await apiFetch('/api/export/presentation', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ content: content, theme: options.theme || 'black', transition: options.transition || 'slide' })
      });
      r = await resp.json();
    } else {
      const payload = {
        content: content,
        baseDir: baseDir,
        suggestedName: suggestedName,
        options: options,
      };
      if (taskId) payload.task_id = taskId;
      if (hasPy && py && typeof py.export_doc === 'function') {
        r = await py.export_doc(fmt, payload);
      } else {
        const resp = await apiFetch('/api/export', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(Object.assign({ format: fmt }, payload))
        });
        r = await resp.json();
      }
    }
  } catch (e) {
    failedResult(e.message || _t('toast.unknownError'));
    busy(false);
    return;
  }
  busy(false);
  if (!r) { failedResult(_t('toast.unknownError')); return; }
  if ((!r.ok && r.error_code === 'cancelled') || (r.canceled && !r.error_code)) {
    const res = $('export-result');
    if (res) { res.textContent = _t('task.cancelled') || ''; res.className = 'export-result'; res.title = ''; }
    return;
  }
  if (!r.ok) {
    const reason = apiMessage(r, 'toast.unknownError') || '';
    failedResult(reason);
    if (r.details) renderExportWarns([r.details]);
    return;
  }
  const res = $('export-result');
  res.textContent = (_t('toast.exportedPrefix') || '') + (r.path || _t('toast.exportSuccess') || '');
  res.className = 'export-result ok';
  const warnList = warnMessages(r);
  if(options.figures?.rendered){options={...options,figures:{...options.figures}};delete options.figures.rendered;}
  state.export.last = { fmt, options, preset };
  try {
    if (hasPy && py.save_export_presets && !(await py.save_export_presets({ last: state.export.last }))) throw new Error('preset_save_failed');
  } catch (_) { warnList.push(_t('export.lastPresetSaveFailed')); }
  res.title = warnList.join('\n');
  renderExportWarns(warnList, r);
  if (r.path && hasPy && py) {
    const report = (out) => {
      if (out && out.ok === false) {
        showToast(out.error_code === 'path_not_found'
          ? (_t('toast.pathNotFound') || '文件不存在或已被移动')
          : (_t('toast.openFailed') || '无法打开'));
      }
    };
    $('export-open').classList.remove('hidden');
    $('export-reveal').classList.remove('hidden');
    const openResult = async (id, action) => {
      const button = $(id);
      if (button.disabled) return;
      button.disabled = true;
      try { report(await action(r.path)); }
      catch (_) { showToast(_t('toast.openFailed')); }
      finally { button.disabled = false; }
    };
    $('export-open').onclick = () => openResult('export-open', path => py.open_path(path));
    $('export-reveal').onclick = () => openResult('export-reveal', path => py.reveal_path(path));
  }
  if (warnList.length) showToast(_t('toast.exportCompleteWarns', { count: warnList.length }) || ('导出完成，' + warnList.length + ' 条提示'), 3600);
  else showToast(_t('toast.exportSuccess') || '');
}

/* 导出结果下方的可展开警告列表（相同提示合并计数） */
let exportWarningState = {list:[],payload:null};
function renderExportWarns(list, payload = null) {
  exportWarningState={list:[...(list||[])],payload};
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  let box = $('export-warns');
  if (!box) {
    const anchor = $('export-result');
    if (!anchor || !anchor.parentElement) return;
    box = document.createElement('details');
    box.id = 'export-warns';
    box.className = 'export-warns';
    anchor.parentElement.insertAdjacentElement('afterend', box);
  }
  box.innerHTML = '';
  if (!list || !list.length) { box.classList.add('hidden'); return; }
  const counts = new Map();
  list.forEach(w => counts.set(w, (counts.get(w) || 0) + 1));
  const sum = document.createElement('summary');
  sum.textContent = _t('export.warnsTitle', { count: list.length });
  box.appendChild(sum);
  const ul = document.createElement('ul');
  counts.forEach((n, w) => {
    const li = document.createElement('li');
    const key='warn.'+w, localized=_t(key), message=localized&&localized!==key?localized:w;
    li.textContent = n > 1 ? message + ' ×' + n : message;
    ul.appendChild(li);
  });
  box.appendChild(ul);
  box.classList.remove('hidden');
}

window.addEventListener('readmd:language-changed',()=>{
  if($('export-modal')?.classList.contains('hidden')!==false)return;
  const host=$('export-opts'),scroll=host.scrollTop,open=[...host.querySelectorAll('.exp-sec')].map(e=>e.classList.contains('open'));
  const warning=exportWarningState;
  renderExportPresetSelect();renderExportSections();
  host.querySelectorAll('.exp-sec').forEach((e,i)=>e.classList.toggle('open',!!open[i]));host.scrollTop=scroll;
  renderExportWarns(warning.payload?warnMessages(warning.payload):warning.list,warning.payload);
});

async function expSavePreset() {
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  const box = $('exp-save-name');
  box.classList.remove('hidden');
  const input = $('exp-save-input');
  input.value = '';
  input.focus();
  $('exp-save-ok').onclick = async () => {
    const button = $('exp-save-ok');
    if (button.disabled) return;
    const name = input.value.trim();
    if (!name) { showToast(_t('toast.enterPresetName') || ''); return; }
    const presetNames = getExportPresetNames();
    if (name === '__default__' || name === '__custom__' || presetNames[name] || (state.export.presets && state.export.presets[name])) {
      showToast(_t('toast.presetNameConflict') || '');
      return;
    }
    if(window.ReadMDPublication?.lineSpacing.validateExport($('export-opts'),true)===false||window.ReadMDPublication?.tracking.validateExport($('export-opts'),true)===false||window.ReadMDPublication?.baseline.validateExport($('export-opts'),true)===false)return;
    if (!validateEpubNumericControls(true)) return;
    const options = collectExportOptions();
    if (expEpubNumericIssue(options)) { showToast(_t('error.epub_invalid_options')); return; }
    const custom = Object.assign({}, state.export.custom, { [name]: options });
    const last = { fmt: state.export.fmt, options, preset: name };
    button.disabled = true;
    try {
      const patch = { custom, last };
      if (hasPy && py && typeof py.save_export_presets === 'function') {
        if (!(await py.save_export_presets(patch))) throw new Error(_t('export.presetSaveFailed'));
      } else {
        const response = await apiFetch('/api/export/presets', {
          method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(patch),
        });
        const data = await response.json();
        if (!response.ok || !data.ok) throw new Error(_t('export.presetSaveFailed'));
      }
      state.export.custom = custom;
      state.export.last = last;
      state.export.options = options;
      state.export.selectedPreset = name;
      renderExportPresetSelect();
      updateExportLivePreview();
      box.classList.add('hidden');
      showToast(_t('toast.presetSaved', { name }) || ('预设已保存：' + name));
    } catch (e) {
      $('export-result').textContent = _t('export.presetSaveFailed');
      $('export-result').className = 'export-result err';
      showToast(_t('export.presetSaveFailed'));
    } finally { button.disabled = false; }
  };
  $('exp-save-cancel').onclick = () => box.classList.add('hidden');
}

/* ---------------- AI 风格排版设计师 ---------------- */

function initExportAiDesigner() {
  const genBtn = $('exp-ai-gen-btn');
  const promptInput = $('exp-ai-prompt');
  if (genBtn && promptInput) {
    genBtn.onclick = () => generateExportStyleWithAi(promptInput.value.trim());
    promptInput.onkeydown = e => {
      if (e.key === 'Enter') {
        e.preventDefault();
        generateExportStyleWithAi(promptInput.value.trim());
      }
    };
  }
}

async function generateExportStyleWithAi(stylePrompt) {
  const genButton = $('exp-ai-gen-btn');
  if (genButton?.disabled) return;
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  if (!stylePrompt) {
    showToast(_t('exportai.placeholder') || '');
    return;
  }
  if (genButton) genButton.disabled = true;
  const statusEl = $('exp-ai-status');
  if (statusEl) {
    statusEl.classList.remove('hidden');
    statusEl.textContent = _t('exportai.generating') || '';
  }

  try {
    const connection = typeof ensureAiConfigured === 'function'
      ? await ensureAiConfigured()
      : (typeof resolveSharedAiConnection === 'function' ? await resolveSharedAiConnection() : null);
    if (!connection) {
      if (statusEl) statusEl.textContent = _t('toast.noApiKeyNotice');
      return;
    }
    const res = await apiFetch('/api/ai/chat', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        provider: connection.provider,
        credential_id: connection.credential_id,
        model: connection.model,
        base_url: connection.base_url,
        mode: connection.mode,
        endpoint_mode: connection.endpoint_mode,
        headers: connection.headers,
        skill_id: 'readmd-export-style',
        skill_variables: {
          request: stylePrompt,
          context: '',
          document: stylePrompt,
          language: (window.i18n && window.i18n.locale) || document.documentElement.lang || 'en',
          output_format: 'JSON'
        },
        messages: [{ role: 'user', content: stylePrompt }],
        stream: false
      })
    });

    if (!res.ok) {
      const errText = await res.text().catch(() => '');
      let errMsg = 'HTTP ' + res.status;
      try {
        const errJson = JSON.parse(errText);
        if (errJson.error) errMsg = errJson.error;
      } catch (e) {
        if (errText) errMsg = errText.slice(0, 100);
      }
      throw new Error(errMsg);
    }

    let text = '';
    const contentType = res.headers.get('content-type') || '';
    if (contentType.includes('application/json')) {
      const data = await res.json();
      text = data.content || (data.choices && data.choices[0] && data.choices[0].message && data.choices[0].message.content) || '';
    } else {
      const rawText = await res.text();
      const lines = rawText.split('\n');
      const chunks = [];
      for (const line of lines) {
        if (line.startsWith('data:')) {
          const jsonStr = line.slice(5).trim();
          if (jsonStr && jsonStr !== '[DONE]') {
            try {
              const d = JSON.parse(jsonStr);
              if (d.type === 'delta' && d.delta) chunks.push(d.delta);
              else if (d.d) chunks.push(d.d);
              else if (d.content) chunks.push(d.content);
            } catch (e) {}
          }
        }
      }
      text = chunks.length ? chunks.join('') : rawText;
    }

    text = text.replace(/^```json\s*/i, '').replace(/^```\s*/, '').replace(/\s*```$/, '').trim();
    const parsed = normalizeExportAiPayload(JSON.parse(text));

    // Apply options to export state and DOM
    state.export.options = expDeepMerge(state.export.options || state.export.defaults, parsed);
    const presetSelect = $('exp-preset');
    if (presetSelect) presetSelect.value = '__custom__';
    state.export.selectedPreset = '__custom__';
    applyExportOptionsToDom();
    updateExportLivePreview();

    if (statusEl) {
      statusEl.textContent = _t('exportai.applied') || '';
      setTimeout(() => statusEl.classList.add('hidden'), 3000);
    }
    showToast(_t('exportai.applied') || '');
  } catch (e) {
    if (statusEl) {
      statusEl.textContent = (_t('ai.reqFailMsg') || '') + e.message;
    }
    showToast((_t('toast.unknownError') || '') + e.message);
  } finally {
    if (genButton) genButton.disabled = false;
  }
}

function exportPaperSize(page) {
  const dimensions = {A3:[297,420],A4:[210,297],A5:[148,210],B5:[176,250],Letter:[215.9,279.4],Legal:[215.9,355.6]};
  const pair = page.size === 'Custom' ? [Number(page.width)||210,Number(page.height)||297] : dimensions[page.size] || dimensions.A4;
  return page.orientation === 'landscape' ? [Math.max(...pair),Math.min(...pair)] : pair;
}
