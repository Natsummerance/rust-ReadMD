import test from 'node:test';
import assert from 'node:assert/strict';
import { locateControl, hasDeclaration } from '../ui-inventory-identity.mjs';

test('inventory identities survive inserted lines and quoted greater-than signs', () => {
  const control = { tag: 'button', id: 'save', line: 2 };
  assert.equal(locateControl('\n\n<button title="a > b" id="save">Save</button>', control), 3);
  assert.equal(locateControl('<input id="save">', control), 0);
  assert.equal(locateControl('<button id="save-copy">Save</button>', control), 0);
});
test('anonymous controls retain their attribute identity', () => {
  const control = { tag: 'button', attrs: { class: 'group-header', 'data-menu': 'text' } };
  assert.equal(locateControl('<button class="active group-header" data-menu="text">Text</button>', control), 1);
  assert.equal(locateControl('<button class="group-header" data-menu="other">Other</button>', control), 0);
});
test('route and command declarations are checked rather than arbitrary nonempty lines', () => {
  assert.equal(hasDeclaration('("/api/save", h_save)', '/api/save'), true);
  assert.equal(hasDeclaration('("/api/save-as", h_save)', '/api/save'), false);
  assert.equal(hasDeclaration("register({id:'file.save',run:save})", 'file.save', 'id'), true);
  assert.equal(hasDeclaration("register({label:'file.save'})", 'file.save', 'id'), false);
});
