// Offline schema inventory. Presence in source is deliberately NOT labelled
// complete support: property and round-trip certification need real fixtures.
// Usage: node tools/audit-office-math.mjs --schema FILE --commit COMMIT
import fs from 'node:fs';
import crypto from 'node:crypto';
const arg = name => process.argv[process.argv.indexOf(name) + 1];
if (!process.argv.includes('--schema') || !process.argv.includes('--commit')) throw Error('Supply --schema FILE --commit COMMIT');
const bytes = fs.readFileSync(arg('--schema')), schema = JSON.parse(bytes);
const commit = arg('--commit').trim(); if (!/^[a-f\d]{40}$/.test(commit)) throw Error('A pinned schema commit is required');
const local = value => value.split('/').at(-1).replace(/^m:/, '');
const parser = fs.readFileSync('rust/readmd-kernel/src/latex2omml.rs', 'utf8').split('fn parse_latex_to_omml_inner_recursive')[0];
const mathml = fs.readFileSync('rust/readmd-kernel/src/omml.rs', 'utf8');
const pdf = fs.readFileSync('rust/readmd-kernel/src/math_layout.rs', 'utf8');
const objects = ['acc','bar','box','borderBox','d','eqArr','f','func','sPre','limLow','m','nary','phant','rad','groupChr','sSub','sSubSup','sSup','limUpp'];
function attributes(type,seen=new Set()){
  if(seen.has(type.ClassName))throw Error('Schema inheritance cycle');seen.add(type.ClassName);
  const parent=schema.Types.find(t=>t.ClassName===type.BaseClass);
  return [...(parent?attributes(parent,seen):[]),...(type.Attributes||[])].filter((a,i,all)=>!all.slice(i+1).some(b=>b.QName===a.QName));
}
const types = schema.Types.map(type => ({
  element: local(type.Name), className: type.ClassName,
  abstract:!!type.IsAbstract,baseClass:type.BaseClass,particle:type.Particle||null,
  children: (type.Children || []).map(c => local(c.Name)),
  attributes: attributes(type).map(a => ({ name: a.QName, type: a.Type, required: (a.Validators || []).some(v => v.Name === 'RequiredValidator'), validators:a.Validators||[] })),
  sourcePresence: { latexWriter: parser.includes('<m:' + local(type.Name) + '>') || parser.includes('<m:' + local(type.Name) + ' '),
    mathmlBranch: mathml.includes('"' + local(type.Name) + '" =>'), pdfBranch: pdf.includes('"' + local(type.Name) + '" =>') },
  certifiedFullPropertyRoundTrip: false,
}));
const result = { schema: 1, standard: 'Office Open XML Math namespace; WordprocessingML embedded children and vendor extensions require separate fixtures',
  source: `https://github.com/dotnet/Open-XML-SDK/blob/${commit}/data/schemas/schemas_openxmlformats_org_officeDocument_2006_math.json`,
  sourceLicense: 'MIT', sourceSha256: crypto.createHash('sha256').update(bytes).digest('hex'),
  counts: { elements: types.length, enums: schema.Enums.length, coreObjects: objects.length },
  methodology: 'Source presence is a navigation aid, not a compatibility test. No element is marked fully property/round-trip certified without Office-generated fixtures and comparison of semantic values.',
  coreObjects: objects.map(name => types.find(t => t.element === name)),
  elements: types,
  enumerations: schema.Enums.map(e => ({ name: e.Name, type: e.Type, facets: e.Facets })),
};
fs.mkdirSync('docs/design',{recursive:true});
fs.writeFileSync('docs/design/office-math-coverage.json',JSON.stringify(result,null,2)+'\n');
console.log(JSON.stringify(result.counts));
