'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

// Exceed the image exporter's page budget AFTER a valid PDF has been laid out.
// This exercises cleanup of the private layout PDF as well as the final target.
module.exports = async function overBudget({host, docs, options}) {
  const before = fs.readdirSync(docs).sort();
  const source = Array.from({length:129}, (_, i) => 'Measurement ' + (i + 1))
    .join('\n\n<!-- pagebreak -->\n\n');
  const report = {};
  for (const [format, ext] of [['png','png'], ['png-pages','zip']]) {
    const target = path.join(docs, 'ordinary.' + ext);
    const expected = fs.readFileSync(target);
    const response = await host.api('/api/export', {
      method:'POST', headers:{'Content-Type':'application/json'},
      body:JSON.stringify({format, content:source, options, baseDir:docs,
        suggestedName:path.basename(target), out_path:target}),
    });
    const result = await response.json();
    assert.equal(result.ok,false);
    assert.equal(result.error_code,'export_snapshot_budget',JSON.stringify(result));
    assert.deepEqual(fs.readFileSync(target),expected);
    assert.deepEqual(fs.readdirSync(docs).sort(),before);
    report[format] = {inputPages:129, errorCode:result.error_code, priorBytesPreserved:expected.length};
  }
  return report;
};
