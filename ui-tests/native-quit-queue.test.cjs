'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const {test} = require('node:test');

test('native quit waits for restore and save, and a repeated request shares the recovery decision', async () => {
  const source = fs.readFileSync(path.join(__dirname,'../assets/js/core/lifecycle.js'),'utf8');
  const env = vm.createContext({});
  vm.runInContext(`
    let completeRestore, completeSave, restoring = true, closes = 0;
    const noop = () => {};
    const state = {};
    let commitDocumentSave = noop, saveEdit = noop, confirmExitEdit = noop, closeTab = noop, closeOtherTabs = noop,
        closeAllTabs = noop, switchTab = noop, goHome = noop, newDocument = noop;
    const window = {ReadMDReview:{onEditorUpdate:noop},ReadMDSession:{restoring:()=>restoring,retryRecovery:noop},
      ReadMDRecovery:{prepareClose:()=>{closes++;return true;},prepareInstall:noop}};
    window.__readmdSessionReady = new Promise(resolve=>{completeRestore=()=>{restoring=false;resolve();}});
  ` + source + '\nwindow.ReadMDLifecycle.install();',env);
  const beforeReady = vm.runInContext('window.ReadMDRecovery.prepareClose()',env);
  assert.equal(vm.runInContext('closes',env),0);
  vm.runInContext('completeRestore()',env);
  assert.equal(await beforeReady,true);
  assert.equal(vm.runInContext('closes',env),1);
  const pending = vm.runInContext(`
    window.ReadMDLifecycle.guard('save',()=>new Promise(resolve=>{completeSave=resolve;}))();
    window.ReadMDRecovery.prepareClose();
  `,env);
  assert.equal(vm.runInContext('window.ReadMDRecovery.prepareClose()',env),pending);
  await Promise.resolve();
  assert.equal(vm.runInContext('closes',env),1);
  vm.runInContext('completeSave()',env);
  assert.equal(await pending,true);
  assert.equal(vm.runInContext('closes',env),2);
});
