#!/usr/bin/env node
// Reusable evidence index. Associated passing suites are not promoted to
// per-branch/manual verification; keep original preconditions visible.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const args = Object.fromEntries(process.argv.slice(2).map(a => { const i = a.indexOf('='); return [a.slice(0, i), a.slice(i + 1)]; }));
const date = args['--date'] || new Date().toISOString().slice(0, 10);
if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) throw Error('Invalid report date');
execFileSync(process.execPath, [path.join(root, 'tools/audit-ui-features.mjs')], { cwd: root, stdio: 'pipe' });
const read = file => JSON.parse(fs.readFileSync(path.resolve(root, file), 'utf8'));
if (!args['--desktop'] || !args['--native'] || !args['--stress']) throw Error('Provide --desktop=<Playwright JSON> --native=<native report> --stress=<stress report>');
const inventory = read('docs/reviews/ui-function-inventory-2026-10-02/inventory.json');
const verification = read('docs/reviews/ui-function-inventory-2026-10-02/verification.json');
const desktop = read(args['--desktop']), native = read(args['--native']), stress = read(args['--stress']);
const finalVerification = args['--final'] ? read(args['--final']) : { pending: true };
const suites = new Map();
function walk(suite, parent) {
  const file = path.basename(suite.file || parent || 'unknown');
  const counts = suites.get(file) || { passed: 0, failed: 0, skipped: 0, flaky: 0 };
  for (const spec of suite.specs || []) for (const test of spec.tests || []) {
    counts[test.status === 'expected' ? 'passed' : test.status === 'skipped' ? 'skipped' : test.status === 'flaky' ? 'flaky' : 'failed']++;
  }
  suites.set(file, counts);
  for (const child of suite.suites || []) walk(child, file);
}
for (const suite of desktop.suites) walk(suite);
if (finalVerification.protocolRegression?.stats) {
  const stats = finalVerification.protocolRegression.stats;
  suites.set('ai-protocols.spec.js', { passed: stats.expected, failed: stats.unexpected, skipped: stats.skipped, flaky: stats.flaky });
}
const changes = {
  F001: '原生文件选择失败可见反馈，正常取消安静', F002: '原生文件夹选择失败可见反馈',
  F008: '按内容版本识别同修改时间变化；串行轮询；迟到刷新保护当前草稿',
  F042: '流式请求兼容 JSON；Responses 文本块/字符串增量与标准 completions 文本可见；不重复请求模型',
  F045: '与聊天共享协议兼容修复，避免普通 JSON 回包被误判空补全',
  F049: 'Anthropic 代理地址保留前缀，避免重复拼接 /v1；完整 URL 模式保留原意',
  F057: 'Skill 来源选择器失效不再静默', F058: '重复来源去重，OCR 覆盖策略在启动时固定',
  F059: '进行中不计完成；终态不倒退；断线重连；取消确认/重试；键盘打开结果',
  F061: 'ZIP 提取后的来源也参与去重', F062: '刷新合并、关闭中止请求和保留空状态',
  F063: '安装进度串行轮询、超时与重连', F064: '丢弃开关保存前的旧列表',
  F071: '只记住本次实际提交的预设；记忆失败保留真实导出成功',
  F072: '预览翻页串行、失败留在原页并可重试', F074: '选择位置前注册取消；确认取消才显示状态；失败可重试；打开结果错误被捕获',
  F088: '桌宠导入共用原生选择器失败反馈',
};
const extra = new Set(['F008', 'F058', 'F059', 'F061', 'F062', 'F063', 'F064', 'F071', 'F072', 'F074']);
const features = inventory.features.map(feature => {
  const reviewed = verification.features.find(row => row.id === feature.id);
  const related = [...new Set(reviewed.evidence.filter(e => e.file.startsWith('ui-tests/') && e.file.endsWith('.spec.js')).map(e => path.basename(e.file)))];
  if (extra.has(feature.id)) related.push('experience-depth.spec.js');
  if (['F042', 'F045', 'F049'].includes(feature.id) && finalVerification.protocolRegression) related.push('ai-protocols.spec.js');
  return {
    id: feature.id, section: feature.section, title: feature.title, controls: feature.controls,
    frontend: feature.frontend, backend: feature.backend, operation: feature.flow,
    change: changes[feature.id] || '保留既有设计，运行所属回归；本轮无新增业务行为',
    sourceIdentityChecked: true,
    suiteEvidence: [...new Set(related)].map(file => ({ file, ...(suites.get(file) || { notExecuted: true }) })),
    originalValidationScope: reviewed.validation, conditions: reviewed.conditions,
    limits: '套件关联是覆盖索引，不等于本条每个输入、服务、设备、网络或异常分支均已实机执行。',
  };
});
const output = {
  date, version: fs.readFileSync(path.join(root, 'VERSION'), 'utf8').trim(),
  baseline: { sections: 21, flows: 104, historicalControls: 399, currentAssignedControls: 398, retired: [{ id: 'S396', control: 'btn-app-exit', reason: '用户要求移除冗余设置退出入口' }], dynamicFamilies: 80 },
  method: '源码身份检查 + 无重试桌面回归 + 原生 WebView/CSP + release 内核实际磁盘压力；输入文档仅使用隔离副本或合成材料。',
  criteria: ['前置条件和不可用状态', '正常操作的前后端闭环', '慢响应/并发/迟到结果', '取消确认与失败重试', '原件/冲突/原子提交', '撤销/恢复/目录和空间', '键盘/IME/多语言/紧凑布局', '长文档/批量/性能与平台边界'],
  features,
  desktop: { stats: desktop.stats, suites: Object.fromEntries(suites) }, native, stress, finalVerification,
  v005Supplement: [
    { id: 'V005-AI', title: '流式自动补全和独立模型策略', evidence: 'readmd-v005-streaming-completion-2026-10-07.md', suites: ['ai-inline-completion.spec.js', 'ai-inline-stream.spec.js'] },
    { id: 'V005-FIND', title: '当前草稿查找替换与一步撤销', suites: ['community-upgrade.spec.js'] },
    { id: 'V005-CHECK', title: '当前草稿结构与断链诊断', suites: ['document-intelligence.spec.js'] },
    { id: 'V005-WORKSPACE', title: '工作区检索、双链及文件监听', evidence: 'readmd-community-upgrade-2026-10-07.md', scope: 'Rust 检索/索引与 VS Code 单元、前轮真实宿主验收分别记录' },
    { id: 'V005-MCP', title: 'MCP 预览/提交、恢复和背压', evidence: 'readmd-v005-acceptance-2026-10-07.md', scope: '本轮 stdio 使用合成服务；本轮未重测所有外部模型' },
    { id: 'V005-PLATFORM', title: 'Linux 打包与 HarmonyOS 原生桥接', evidence: 'readmd-v005-acceptance-2026-10-07.md', scope: 'Windows 已实机；其它系统/CPU/鸿蒙 SDK 与真机仍有未验收项，不能宣称全部支持完成' },
  ],
  allChecksPassed: desktop.stats.unexpected === 0 && desktop.stats.flaky === 0 && native.passed === true && stress.passed === true && (!args['--final'] || finalVerification.passed === true),
};
const directory = path.join(root, 'docs/reviews'); fs.mkdirSync(path.join(directory, 'evidence'), { recursive: true });
fs.writeFileSync(path.join(directory, 'evidence', `v005-experience-depth-${date}.json`), JSON.stringify(output, null, 2) + '\n');
const clean = text => String(text).replaceAll('|', '／').replaceAll('\n', ' ');
let markdown = `# V0.0.5 逐功能体验验收索引\n\n日期：${date}。原清单的 21 个层级、104 条流程全部保留。入口、操作、前端处理、后端实现和限制的完整数据见 [JSON 证据](evidence/v005-experience-depth-${date}.json)。本表仅关联所属套件；通过的套件不能证明每一条分支或所有第三方服务均已执行。\n\n`;
for (const section of [...new Set(features.map(f => f.section))]) {
  markdown += `## ${section}\n\n| 编号 | 功能与本轮处理 | 所属套件本轮结果 | 条件 |\n| --- | --- | --- | --- |\n`;
  for (const f of features.filter(f => f.section === section)) {
    const count = f.suiteEvidence.reduce((out, row) => { for (const key of ['passed', 'failed', 'skipped', 'flaky']) out[key] += row[key] || 0; return out; }, { passed: 0, failed: 0, skipped: 0, flaky: 0 });
    markdown += `| ${f.id} | ${clean(f.title)}；${clean(f.change)} | ${f.suiteEvidence.length} 套件：${count.passed} 通过 / ${count.failed} 失败 / ${count.skipped} 跳过 / ${count.flaky} 不稳定 | ${clean(f.conditions)} |\n`;
  }
  markdown += '\n';
}
markdown += '## V0.0.5 补充能力\n\n';
for (const f of output.v005Supplement) markdown += `- **${f.id} ${f.title}**：${f.scope || '所属桌面套件见完整证据。'}${f.evidence ? ` [独立验收记录](${f.evidence})。` : ''}\n`;
fs.writeFileSync(path.join(directory, `readmd-v005-experience-matrix-${date}.md`), markdown);
console.log(JSON.stringify({ flows: features.length, sections: 21, desktop: desktop.stats, native: native.passed, stress: stress.passed, passed: output.allChecksPassed }));
if (!output.allChecksPassed) process.exitCode = 1;
