/** Keep the operation index and saving UX map tied to the recording manifest. */
import fs from 'node:fs';import path from 'node:path';import{fileURLToPath}from'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const manifest=JSON.parse(fs.readFileSync(path.join(root,'showcase/manifest.json')));
const verificationPath=path.join(root,'showcase/checks/verification.json');
const v=fs.existsSync(verificationPath)?JSON.parse(fs.readFileSync(verificationPath)):null;
if(!v||v.capture_profile!==manifest.capture_profile||v.ui_source_sha256!==manifest.ui_source_sha256||v.features!==manifest.features.length)throw Error('Verify the current recordings before generating the final report');
const privacy=JSON.parse(fs.readFileSync(path.join(root,'showcase/checks/privacy.json')));
const cleanupPath=path.join(root,'showcase/checks/cleanup.json');
const cleanup=fs.existsSync(cleanupPath)?JSON.parse(fs.readFileSync(cleanupPath)):null;
const escape=s=>String(s).replaceAll('|','\\|').replaceAll('\n',' ');
const film=id=>`[${id}](../../showcase/videos/${id}.mp4)`;
const map={
 D01:'F001 F012',D02:'F003 F021',D03:'F021 F009',D04:'F021 F102',D05:'F021',D06:'F021',D07:'F003 F012 F044',D08:'F012',D09:'F012 F097',D10:'F021',D11:'F009 F021',D12:'F021 F012',D13:'F097',D14:'F097 F103',D15:'F021 F012',D16:'F021',
 D17:'F102',D18:'F102',D19:'F012',D20:'F012',D21:'F012 F044',D22:'F028 F030',D23:'F028 F030 F012',D24:'F028 F030 F012',D25:'F012 F102',
 D26:'F042 F044',D27:'F045',D28:'F045',D29:'F044 F045',D30:'F082',D31:'F044 F045 F082',D32:'F082',D33:'F044 F051',D34:'F044 F012',
 D35:'F060',D36:'F058 F060',D37:'F058 F059',D38:'F058 F059',D39:'F060',D40:'F058 F066',D41:'F061 F028',
 D42:'F023 F045 F103',D43:'F009 F023',D44:'F103',D45:'F006 F009 F097',D46:'F010',D47:'F104',D48:'F103 F104',D49:'F103',D50:'F103',D51:'F103',D52:'F103',D53:'F103 F097',D54:'F102 F103',D55:'F103',D56:'F096 F102 F103'
};
const lifecycle=fs.readFileSync(path.join(root,'docs/reviews/readmd-document-lifecycle-ux-2026-10-03.md'),'utf8');
let body=`# ReadMD 全功能操作演示与验收\n\n日期：${manifest.date}。按[最新功能清单](readmd-ui-function-inventory-2026-10-02.md)组织 **104 个功能流程、21 个分类**，每项对应一段纯软件画面 MP4、真实画面封面及独立可选字幕。全部按最新 UI 重录。\n\n`;
if(v)body+=`媒体共 ${(v.total_bytes/1048576).toFixed(1)} MiB；时长中位数 ${v.median_seconds.toFixed(1)} 秒，最长 ${v.max_seconds.toFixed(1)} 秒。104 段媒体均通过哈希、格式与网站一致性检查，并完成浏览器解码、定位和字幕加载；四语言首页及功能页共 ${v.layouts.length} 个窗口/主题组合通过。\n\n`;
body+='公开视频使用原创哲学资料。AI 为真实 Antigravity Manager 反代与 `gemini-3.8-flash-high` 请求；长等待仅加速时间，独立字幕和 manifest 保留剪辑记录，视频不烧入说明、编号或加速标签。Windows 文件夹、导出、打印、网页授权和独立桌宠均由真实系统操作驱动。原生视频只采集自身 WebView：系统文件夹、选择器、资源管理器、打印窗口及其他前台应用不录入；角色画面按真实窗口位置合成。完整源码、接口和条件在总清单中，实际录制步骤及结果断言在 [manifest](../../showcase/manifest.json)。\n\n';
body+=`全部 ${privacy.clips} 段视频及 ${privacy.posters} 张封面通过本机 OCR 隐私复核，共检查 ${privacy.frames_inspected+privacy.verification_frames} 帧（视频按每秒 4 帧抽检，并检查封面）；${privacy.redacted_clips.length} 段的个人路径已遮挡并复核。隐私批准绑定媒体哈希，变更后需重新核验；摘要见 [privacy.json](../../showcase/checks/privacy.json)。新增录制逐帧在自身 WebView 中加入路径遮挡层，不修改应用业务数据。\n\n## 按层级逐项播放\n\n`;
for(const section of [...new Set(manifest.features.map(f=>f.section))]){
 body+=`### ${section}\n\n| 功能与视频 | 时长 | 实际演示操作 |\n| --- | --- | --- |\n`;
 for(const f of manifest.features.filter(f=>f.section===section))body+=`| ${film(f.id)} ${escape(f.title)} | ${f.recording.status==='recorded'?f.recording.duration.toFixed(1)+' 秒':'待录制'} | ${escape((f.recording.steps||[]).filter(s=>!s.text.includes('等待过程已加速')).map(s=>s.text).join(' → '))} |\n`;
 body+='\n';
}
body+='## 保存、副本、转换和 AI 修改的体验对照\n\n下表为[文档生命周期清单](readmd-document-lifecycle-ux-2026-10-03.md) D01–D56 的相关功能片段与验证依据。D 编号属于保护细则，不再重复计算为新功能。磁盘失败、过期响应、外部写入冲突、容量上限等通过行为测试或源码审计验证；未在正常操作视频中伪造故障。\n\n| 体验细则 | 相关功能视频 | 已有验证依据 |\n| --- | --- | --- |\n';
for(const line of lifecycle.split('\n')){const match=line.match(/^\| (D\d{2}) ([^|]+)\|/);if(!match)continue;const cols=line.split('|');const id=match[1];if(!map[id])throw Error('Missing lifecycle recording map '+id);body+=`| ${id} ${escape(match[2].trim())} | ${map[id].split(' ').map(film).join('、')} | ${escape(cols.at(-2).trim())} |\n`;}
body+='\n## 既有问题修复与本次录制升级\n\n- 中文 Windows CSV 的 CRLF 拆行越界导致转换崩溃，已修复并实测转换结果。\n- AI 文内编辑未完成时可应用、空指令被模型拒绝，已加等待和非空请求保护；应用为可撤销事务。\n- YAML 元数据首次阅读显示、书目引用接口、反向链接索引及图谱请求接线已修正。\n- Skill 的保存、生成、评估、发布、启停、原始来源导入与会话历史均使用真实持久化；导入预览入口已修复。\n- 原生重命名重复扩展名、文件夹选择起始目录和网页授权后会话过早关闭已修正。\n- 打印只打印正文；空编辑器导出保留空内容；预览与导出 PDF 字节复用通过验证。\n- Windows 快速点击允许已排队的抬起事件完成，松开后移动不会启动拖动；3 条输入时序回归用例通过。\n- 桌宠支持辅助输入/远程桌面注入的真实键鼠事件，原生拖动结束后恢复透明区域穿透；原创精灵和 Live2D 保留各自互动。\n- CRLF 文件进入编辑器不再被误标修改，取消无改动编辑不弹保存框；保存保留换行，代码输出写入使用编辑器坐标。\n- 首页沿用原 Apple 风格的系统字体、白灰配色、蓝色圆角按钮、玻璃导航、居中标题和产品大图，只精修排版及演示入口；分类页采用同一视觉风格，保留搜索、字幕、步骤、下载、主题和手机布局。\n- 原生录制改为隔离 WebView 画面，排除系统文件窗口和其他前台程序；全部短片与封面加入隐私复核，替换原生录屏及个人路径。\n\n## 有条件的操作与范围\n\n104 是功能流程数量，并不表示 399 个静态控件、143 行格式矩阵或全部故障分支都分别录了独立视频。片段展示的具体步骤以上表和字幕为准。\n\n- F085 真实更新服务当前没有更高正式版本，录制检查及实际状态；没有构造假的更新成功或安装旧版。下载、取消、校验失败和安装重试保留在功能清单，通过相应回归测试验证。\n- F084 演示真实语言切换和开机自启切换并恢复原注册值；没有把临时录制程序设为用户默认文件关联。系统关联实现由原生注册表方案测试覆盖。\n- F057 展示带原始许可的 ZIP Skill 预览/选择/导入；GitHub、文件夹来源和各导出格式仍列在总清单，片段没有宣称逐来源和逐字段完整录制。\n- Hermes 私人哲学 EPUB/PDF 实测转换成功且原件哈希未变，仅保留 [兼容性元数据](../../showcase/checks/private-compatibility.json)，私人正文和原始书籍不进入公开资料。\n\n## 验收证据与复现\n\n软件行为测试沿用此前已完成的回归证据；本次完整重录的媒体、播放、隐私和 UI 快照核验以当前 manifest 和 verification.json 为准。\n\n- 应用业务代码沿用已完成的回归证据，详见 [窗口与系统升级验收](readmd-window-system-upgrade-2026-10-04.md) 和 [文档生命周期验收](readmd-document-lifecycle-ux-2026-10-03.md)。本次录制没有修改应用业务代码。\n- 104 段录制均带实际功能结果断言；全部核对当前 UI 源码哈希与 immersive-v2 模式，12 段采集原生窗口或独立桌宠。每段均验证录制用临时样式已恢复。\n- 录制模式检查通过：隐藏通知时仍可操作功能按钮，隐私遮挡不修改业务数据，结束后恢复通知。证据见 [capture-privacy.json](../../showcase/checks/capture-privacy.json)。\n- 前端 assets/wiring/styles/i18n 检查通过；267 份上游资源、1,148 条 provider、46 语言/2,034 文案键；没有新增依赖，使用此前离线打包的当前 boot。\n- 网站静态验收：45 个规范页面及全 104 项真实短片索引；全部媒体与 30 个布局组合的证据见 [verification.json](../../showcase/checks/verification.json)。\n\n构建、重录、原生要求、凭据隔离与清理命令见 [showcase README](../../showcase/README.md)。最终网站保留在 `website/dist`，本轮结果未提交、推送或部署。\n';
if(cleanup?.completed) body+=`
本轮缓存清理完成：删除隔离凭据、私人测试副本、旧录制和失败帧、WebView 缓存及任务临时文件，释放约 ${(cleanup.runtime_unique_bytes_reclaimed/1073741824).toFixed(2)} GiB；保留最终媒体、原创资料和验收证据。摘要见 [cleanup.json](../../showcase/checks/cleanup.json)。
`;
fs.writeFileSync(path.join(root,'docs/reviews/readmd-showcase-2026-10-03.md'),body);
console.log('Built 104-operation index and 56-item document UX map');
