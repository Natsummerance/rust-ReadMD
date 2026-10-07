# Offline renderer notices

These assets are copied from the repository's existing `assets/vendor` tree;
no dependency is downloaded while compiling or packaging the extension.
Original runtime headers are retained. The adjacent files contain upstream
license texts retrieved while preparing this distribution.

| Component | Upstream/source | License/notice |
| --- | --- | --- |
| Marked 15.0.12 | https://github.com/markedjs/marked/tree/v15.0.12 | marked.txt |
| KaTeX | https://github.com/KaTeX/KaTeX | ../vendor/katex/LICENSE |
| Mermaid | https://github.com/mermaid-js/mermaid | mermaid.txt |
| Reveal.js | https://github.com/hakimel/reveal.js | ../vendor/reveal/LICENSE |
| Source Sans Pro | https://github.com/adobe-fonts/source-sans | source-sans-pro.txt |
| WaveDrom 3.3.0 | https://github.com/wavedrom/wavedrom/tree/v3.3.0 | wavedrom.txt |
| Bit-field | https://github.com/wavedrom/bitfield | ../vendor/diagrams/bitfield/LICENSE |
| Vega 5.25.0 | https://github.com/vega/vega/tree/v5.25.0 | vega.txt |
| Vega-Lite 5.16.1 | https://github.com/vega/vega-lite/tree/v5.16.1 | vega-lite.txt |
| Chart.js 4.5.1 | https://github.com/chartjs/Chart.js/tree/v4.5.1 | chart.txt |
| Viz.js (asset header: 3.14.0) | https://github.com/mdaines/viz-js/tree/v3 | viz.txt; graphviz.txt and expat.txt for included object code |
| TikZJax v1 | https://github.com/kisonecat/tikzjax | tikzjax.txt (LPPL 1.3c); upstream TeX/WASM sources: https://github.com/kisonecat/web2js and https://github.com/kisonecat/dvi2html |

TikZJax's script, WASM and compressed TeX data remain unmodified. The extension
redirects its fixed resource requests to packaged copies in an isolated frame.
The extension's own code and ReadMD's root MIT license do not replace the
licenses of these separately distributed components.
