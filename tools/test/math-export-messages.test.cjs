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
      ['math_invalid_xml','formula.invalid'],['math_unsupported_version','formula.unsupportedVersion'],
      ['math_document_defaults_conflict','error.math_document_defaults_conflict']
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

test('native paragraph inheritance loss uses the typed warning in every shipped locale',()=>{
  const begin=state.indexOf('function warnMessages('),finish=state.indexOf('function setProgress(',begin);
  assert(begin>=0&&finish>begin);const warningSource=state.slice(begin,finish),code='math_paragraph_style_materialized';
  for(const locale of locales){
    const {strings}=api(locale),key='warn.'+code;
    assert(typeof strings[key]==='string'&&strings[key].length>0,`${locale}: missing ${key}`);
    const context=vm.createContext({window:{i18n:{t:key=>strings[key]??key}}});vm.runInContext(warningSource,context);
    assert.deepEqual(Array.from(context.warnMessages({warn_items:[{code,params:{},text:code}]})),[strings[key]],locale);
    assert.deepEqual(Array.from(context.warnMessages({warn_items:[{code:code+'_lookalike',params:{},text:'owned fallback'}]})),['owned fallback'],locale);
  }
});

test('native equation character inheritance warnings have explicit native translations and exact lookup',()=>{
  const begin=state.indexOf('function warnMessages('),finish=state.indexOf('function setProgress(',begin);
  assert(begin>=0&&finish>begin);const warningSource=state.slice(begin,finish),code='math_run_style_materialized',key='warn.'+code;
  const spec=JSON.parse(fs.readFileSync(path.join(__dirname,'i18n-math-native-runs.json'),'utf8'))[key];
  assert.deepEqual(Object.keys(spec).sort(),locales.map(file=>file.replace(/\.json$/,'')).sort(),'No locale may be supplied by the i18n insertion fallback');
  for(const locale of locales){
    const {strings}=api(locale),language=locale.replace(/\.json$/,'');
    assert.equal(strings[key],spec[language],`${locale}: explicit translation changed or missing`);
    if(language!=='en')assert.notEqual(strings[key],spec.en,`${locale}: English placeholder`);
    const context=vm.createContext({window:{i18n:{t:key=>strings[key]??key}}});vm.runInContext(warningSource,context);
    assert.deepEqual(Array.from(context.warnMessages({warn_items:[{code,params:{},text:code}]})),[strings[key]],locale);
    for(const invalid of [code+'_lookalike',code+'/private.docx',code+': source']){
      assert.deepEqual(Array.from(context.warnMessages({warn_items:[{code:invalid,params:{},text:'owned fallback'}]})),['owned fallback'],locale);
    }
  }
});
