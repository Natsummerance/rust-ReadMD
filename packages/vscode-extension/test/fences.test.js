const test = require('node:test');
const assert = require('node:assert/strict');
const { fencedCodeAt } = require('../out/fences');
test('long and tilde fences keep shorter fence text inside the code', () => {
  const source = 'Intro\r\n  ````javascript {cmd=true}\r\nconsole.log("```");\r\n```\r\n  ````\r\n';
  const result = fencedCodeAt(source, source.indexOf('console'));
  assert.equal(result.language, 'javascript'); assert.equal(result.code, 'console.log("```");\r\n```\r\n');
  assert.equal(fencedCodeAt('~~~sh\necho ok\n~~~\n', 8).language, 'sh');
});
test('a shorter closing fence and an indented block cannot execute the wrong code', () => {
  assert.equal(fencedCodeAt('````js\nconsole.log(1)\n```\n', 10), undefined);
  assert.equal(fencedCodeAt('    ```js\nconsole.log(1)\n    ```\n', 20), undefined);
  assert.equal(fencedCodeAt('```js\nconsole.log(1)\n```\n\nOutside', 30), undefined);
});
