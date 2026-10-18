# MathJax local components

ReadMD uses MathJax 3.2.2 (Apache-2.0; copyright MathJax contributors).
The `html` and `ui/safe` components match the existing `tex-svg.js` runtime.
They load locally; the equation editor does not fetch extensions from a CDN.

- [HTML component source](https://github.com/mathjax/MathJax/blob/3.2.2/es5/input/tex/extensions/html.js)
- [Safe component source](https://github.com/mathjax/MathJax/blob/3.2.2/es5/ui/safe.js)
- [License](LICENSE)

`assets/math-catalog.json` derives command spellings from the existing runtime's
character maps. `tools/build-math-catalog.mjs` regenerates it offline and records
the source hash and Unicode version. The remaining symbol repertoire comes from
the JavaScript runtime's Unicode Math property and Greek letters.
`input/mml.js` is also the official MathJax 3.2.2 component, Apache-2.0.
Source: https://github.com/mathjax/MathJax/blob/3.2.2/es5/input/mml.js
SHA-256: 65c93277d980860f34341112c59520b9d2745cd46280d9c2fe036718b826624d.
It is packaged locally; formula rendering never fetches this component from a CDN.

The `boldsymbol` component is also MathJax 3.2.2, Apache-2.0, from
commit `600692ad9d3552cc25f85510d5797bc942ecc9f7`:
https://github.com/mathjax/MathJax/blob/600692ad9d3552cc25f85510d5797bc942ecc9f7/es5/input/tex/extensions/boldsymbol.js
It adds the actual bold-symbol parser and token rewriting, including Greek and
nested mathematics. It is loaded explicitly from the local package. No build
step downloads components. `components.lock.json` pins runtime resources and
their license; the asset check rejects missing, changed or additional scripts.
