`html5-character-references.json` is the HTML5 named character reference data
from CPython 3.11.15's standard-library `html.entities.html5` (2,231 names, including
the legacy forms without semicolons), sorted by name and encoded as UTF-8 JSON.
The upstream data is the [WHATWG character reference table](https://html.spec.whatwg.org/entities.json).
The CPython distribution's license and historical notices are preserved in
[`html5-character-references-PYTHON-LICENSE.txt`](../licenses/html5-character-references-PYTHON-LICENSE.txt).
Only the data representation changes: all names and replacement strings are
copied unchanged from `html.entities.html5`; no CPython implementation code is
included.

Only the CommonJS heading parser loads this file. Browser builds use the
browser's HTML parser and do not add this table to `readmd.boot.js`.

Regenerate using Python's standard library, with no installed packages:

```python
import html.entities
import json
from pathlib import Path

Path('assets/vendor/html5-character-references.json').write_text(
    json.dumps(html.entities.html5, ensure_ascii=False, sort_keys=True, indent=2) + '\n',
    encoding='utf-8',
)
```
