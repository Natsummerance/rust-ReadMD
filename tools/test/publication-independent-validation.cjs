'use strict';
// Continue only explicit assertions about an already generated artifact. API,
// timeout, source-integrity and setup failures must still abort the native run.
const assert = require('node:assert/strict');
const marked = new WeakSet();
function artifactAssertion(check) {
  try { return check(); }
  catch (error) {
    if (error instanceof assert.AssertionError) marked.add(error);
    throw error;
  }
}
function transferAssertion(original, replacement) {
  if (marked.has(original)) marked.add(replacement);
  return replacement;
}
async function independentStep(step, label, work, retain) {
  try { return await step(label, work); }
  catch (error) {
    if (!marked.has(error)) throw error;
    // Retention/integrity failures are never swallowed. The caller must fail
    // the whole suite after its independent cases, using the retained list.
    await retain(label, error);
    return undefined;
  }
}
function requireNoFailures(failures) {
  assert.equal(failures.length, 0, 'Independent artifact validation failures remain; see preserved first failures');
}
module.exports = {artifactAssertion, transferAssertion, independentStep, requireNoFailures};
