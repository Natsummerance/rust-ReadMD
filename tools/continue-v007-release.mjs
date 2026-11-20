// Bound, unattended continuation of an explicitly dispatched V0.0.7 refresh.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const base = 'repos/Natsummerance/rust-ReadMD';
const start = '<!-- readmd-autopublish:start -->', end = '<!-- readmd-autopublish:end -->';
export function publishedCopy(text, {source, run, petRun, locale = 'zh-CN'}) {
  const tree = 'https://github.com/Natsummerance/rust-ReadMD/tree/' + source;
  const release = 'https://github.com/Natsummerance/rust-ReadMD/actions/runs/' + run;
  const pet = 'https://github.com/Natsummerance/rust-ReadMD/actions/runs/' + petRun;
  const copy = {
    'zh-CN': `本轮图表阅读界面与 AI 修复已自动发布。全部十份桌面／集成包来自同一完整源码 [\`${source}\`](${tree})；[发行 CI](${release}) 与 [PET 三系统检查](${pet}) 均通过。应用保持 0.0.7，随包伴读组件为 1.0.2。`,
    en: `The inline chart UI and AI repair refresh is published. All ten desktop/integration assets come from [\`${source}\`](${tree}); the [release CI](${release}) and [three-platform PET checks](${pet}) passed. The app remains 0.0.7 and bundles companion 1.0.2.`,
    'zh-TW': `本輪圖表閱讀介面與 AI 修復已自動發行。全部十份桌面／整合套件來自同一完整原始碼 [\`${source}\`](${tree})；[發行 CI](${release}) 與 [PET 三系統檢查](${pet}) 均通過。應用程式維持 0.0.7，隨包伴讀元件為 1.0.2。`,
    ja: `グラフ閲覧 UI と AI 修正の更新を公開しました。全10ファイルは同じソース [\`${source}\`](${tree}) から生成し、[リリース CI](${release}) と [PET の3システム検査](${pet}) が通過しました。アプリは 0.0.7、同梱コンパニオンは 1.0.2 です。`,
  };
  return text.replace(/<!-- readmd-autopublish:start -->[\s\S]*?<!-- readmd-autopublish:end -->/, start + '\n' + (copy[locale] || copy['zh-CN']) + '\n' + end)
    .replace('图表阅读与 AI 修复（自动发布中）', '图表阅读与 AI 修复（已自动发布）');
}
async function main() {
  const source = process.env.CANDIDATE_COMMIT, run = Number(process.env.CANDIDATE_RUN), petRun = Number(process.env.CANDIDATE_PET_RUN), token = process.env.GH_TOKEN;
  if (!process.env.GITHUB_ACTIONS || !token || !/^[a-f\d]{40}$/.test(source) || ![run, petRun].every(id => Number.isSafeInteger(id) && id > 0)) throw Error('Exact source and both dispatched CI IDs are required');
  const own = path.join(root, '.cache/v007-refresh'); fs.mkdirSync(own, {recursive: true});
  const save = (name, value) => fs.writeFileSync(path.join(own, name), JSON.stringify(value, null, 2) + '\n');
  async function api(method, route, data) {
    for (let attempt = 0; ; attempt++) {
      try {
        const response = await fetch('https://api.github.com/' + base + route, {method, headers: {Authorization: 'Bearer ' + token, Accept: 'application/vnd.github+json', 'X-GitHub-Api-Version': '2022-11-28', 'Content-Type': 'application/json'}, body: data === undefined ? undefined : JSON.stringify(data), signal: AbortSignal.timeout(60000)});
        if (!response.ok) throw Error('Continuation ' + method + ' HTTP ' + response.status);
        return response.status === 204 ? null : await response.json();
      } catch (error) {
        // Only reads are retried; ambiguous writes retain their normal receipts.
        if (method !== 'GET' || attempt >= 5) throw error;
        await new Promise(resolve => setTimeout(resolve, 1000 * (attempt + 1)));
      }
    }
  }
  if (process.argv[2] === 'wait') {
    const deadline = Date.now() + 150 * 60000; let previous;
    save('ci-wait.json', {sourceCommit: source, releaseRun: run, petRun, status: 'waiting', checkedAt: new Date().toISOString()});
    while (Date.now() < deadline) {
      const states = [];
      for (const [id, expectedPath] of [[run, '.github/workflows/release.yml'], [petRun, '.github/workflows/pet-rust-quality.yml']]) {
        const value = await api('GET', '/actions/runs/' + id);
        if (value.head_sha !== source || value.path !== expectedPath || value.event !== 'workflow_dispatch') throw Error('The bound CI run/source identity differs');
        if (value.status === 'completed' && value.conclusion !== 'success') throw Error('Bound CI failed or was cancelled: ' + id);
        states.push({id, status: value.status, conclusion: value.conclusion, sourceCommit: value.head_sha});
      }
      save('ci-wait.json', {sourceCommit: source, runs: states, checkedAt: new Date().toISOString()});
      const current = JSON.stringify(states); if (current !== previous) {console.log(current); previous = current;}
      if (states.every(value => value.status === 'completed' && value.conclusion === 'success')) return;
      await new Promise(resolve => setTimeout(resolve, 30000));
    }
    throw Error('Bound CI wait expired; release downloads remain unchanged');
  }
  if (process.argv[2] !== 'finish') throw Error('Expected wait or finish');
  const transaction = JSON.parse(fs.readFileSync(path.join(own, 'transaction.json')));
  if (transaction.status !== 'complete' || transaction.sourceCommit !== source || transaction.runId !== run) throw Error('Existing publication transaction has not completed');
  const release = await api('GET', '/releases/451629011');
  if (release.assets.length !== 11 || !release.body.includes(source)) throw Error('The actual formal release differs from the completed transaction');
  const docs = [['README.md', 'zh-CN'], ['README.en.md', 'en'], ['README.zh-TW.md', 'zh-TW'], ['README.ja.md', 'ja'], ['RELEASE_NOTES.md', 'zh-CN'], ['release/release_notes.md', 'zh-CN']];
  for (const [name, locale] of docs) fs.writeFileSync(path.join(root, name), publishedCopy(fs.readFileSync(path.join(root, name), 'utf8'), {source, run, petRun, locale}));
  process.env.GITHUB_TOKEN = token;
  const {refreshRelease, syncRelease} = await import('../website/tools/published-release.mjs');
  await refreshRelease(); await syncRelease({folders: ['public']});
  const git = (...args) => execFileSync('git', args, {cwd: root, encoding: 'utf8'}).trim();
  const parent = git('rev-parse', 'HEAD');
  if ((await api('GET', '/git/ref/heads/main')).object.sha !== parent) throw Error('Main advanced; refusing to overwrite other work');
  const files = [...docs.map(([name]) => name), 'website/published-release.json', 'website/public/version.json', 'website/release.json'];
  const tree = [];
  for (const name of files) {
    const bytes = fs.readFileSync(path.join(root, name));
    const old = execFileSync('git', ['show', 'HEAD:' + name], {cwd: root});
    if (bytes.equals(old)) continue;
    const expected = crypto.createHash('sha1').update('blob ' + bytes.length + '\0').update(bytes).digest('hex');
    const blob = await api('POST', '/git/blobs', {encoding: 'base64', content: bytes.toString('base64')});
    if (blob.sha !== expected) throw Error('Documentation blob differs');
    tree.push({path: name, mode: '100644', type: 'blob', sha: blob.sha});
  }
  let documentationCommit = parent;
  if (tree.length) {
    const commit = await api('GET', '/git/commits/' + parent);
    const updatedTree = await api('POST', '/git/trees', {base_tree: commit.tree.sha, tree});
    const updated = await api('POST', '/git/commits', {message: 'Record completed V0.0.7 inline chart and AI refresh [skip ci]', tree: updatedTree.sha, parents: [parent], author: {name: 'github-actions[bot]', email: '41898282+github-actions[bot]@users.noreply.github.com'}});
    await api('PATCH', '/git/refs/heads/main', {sha: updated.sha, force: false});
    documentationCommit = updated.sha;
  }
  for (const workflow of ['website-github-pages.yml', 'website-cloudflare.yml']) await api('POST', '/actions/workflows/' + workflow + '/dispatches', {ref: 'main'});
  save('continuation-complete.json', {sourceCommit: source, releaseRun: run, petRun, documentationCommit, websiteWorkflowsDispatched: ['website-github-pages.yml', 'website-cloudflare.yml']});
  console.log('Verified release, actual download snapshot and existing website deployments continued automatically.');
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
