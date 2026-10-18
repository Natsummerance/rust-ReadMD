'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const root=path.resolve(__dirname,'../..');
const state=fs.readFileSync(path.join(root,'assets/js/core/state.js'),'utf8');
const start=state.indexOf('function apiMessage('),end=state.indexOf('function warnMessages(',start);
assert(start>=0&&end>start);
const source=state.slice(start,end);
const locales=fs.readdirSync(path.join(root,'assets/i18n')).filter(f=>f.endsWith('.json')&&f!=='meta.json');
function api(locale){
  const strings=JSON.parse(fs.readFileSync(path.join(root,'assets/i18n',locale),'utf8'));
  const context=vm.createContext({window:{i18n:{t:key=>strings[key]??key}}});
  vm.runInContext(source,context);
  return {message:context.apiMessage,strings};
}
test('export and preview expose actionable math failures in every shipped locale',()=>{
  assert.equal(locales.length,46);
  for(const locale of locales){
    const {message,strings}=api(locale);
    for(const [code,key] of [
      ['math_budget','error.math_budget'],['math_unsupported_extension','error.math_unsupported_extension'],
      ['math_invalid_xml','formula.invalid'],['math_unsupported_version','formula.unsupportedVersion']
    ]){
      assert(typeof strings[key]==='string'&&strings[key].length>0,`${locale}: missing ${key}`);
      for(const envelope of [{ok:false,error_code:code,details:'owned test diagnostic'},{ok:false,error:code}]){
        for(const fallback of ['common.error','export.previewFailed']){
          const actual=message(envelope,fallback);
          assert.equal(actual,strings[key],`${locale}: ${code}`);
          assert.notEqual(actual,code);
          if(code==='math_budget')assert.notEqual(actual,strings['formula.tooLarge'],'Export must not recommend only the source editor');
        }
      }
    }
  }
});
test('the error mapping uses exact codes and keeps unrelated service errors',()=>{
  const {message}=api('en.json');
  for(const error of ['math_budgetary','math_budget/private.tex','Unrelated engine error']){
    assert.equal(message({error},'export.previewFailed'),error);
  }
  assert.match(message({error_code:'math_budget'}),/size or complexity/i);
  assert.match(message({error_code:'math_unsupported_extension'}),/original source is preserved/i);
});
