# ReadMD V0.0.5 迭代与验收清单

目标：从已有社区调研吸收适合本地文档工作流的能力，复用原入口，完成可验证的发布候选。范围并非声称覆盖所有 Markdown 项目，也不承诺任意输入永不出错。

## 取舍依据

沿用 [社区调研矩阵](readmd-community-research-2026-10-07.md)，覆盖阅读、即时预览、结构编辑、文档转换、学术写作、知识链接、演讲、编辑器与 AI 协作。既有公式、图表、目录、插件、托盘、桌宠、转换及导出入口继续使用，不创建平行的功能面板。

| 参考能力 | 本轮落地 | 入口 |
| --- | --- | --- |
| [Obsidian 本地链接](https://obsidian.md/help/links)、[属性](https://obsidian.md/help/properties) | AST 双链解析、同名歧义检测、本地目标及锚点检查、常用 YAML 标签检索 | 桌面修复报告、VS Code、MCP |
| [VS Code Markdown](https://code.visualstudio.com/docs/languages/markdown) | 原生 Problems 诊断与 Wiki DocumentLink，版本过期结果丢弃 | VS Code 原生面板与命令 |
| 常见笔记库检索 | 当前文件直接检索，组合短语、路径、标题、标签与排除词；不依赖过期索引 | VS Code Quick Pick、MCP、共用 HTTP 接口 |
| 文字处理软件的保存与审阅原则 | AI 工具先预览、后确认；字节版本冲突检测、原子写入、编码及未改部分换行保留、集中恢复历史 | MCP；复用桌面保存与恢复底层 |
| [Google 搜索入门](https://developers.google.com/search/docs/fundamentals/seo-starter-guide) | 四语言集成页、准确版本和下载入口、canonical/hreflang、站点地图与 Atom/crawler 文本同步 | 现有网站 |

不增加独立知识库数据库、同步服务、第二套编辑器或新的顶部工具条。搜索不执行正则表达式、不联网抓取链接、不扫描隐藏或生成目录。

## 增量操作清单

1. **桌面文档检查**：打开文档或编辑草稿 → 更多／互动／修复详情 → 查看链接检查。只检查当前内容；异步返回后若切换标签或修改文本，结果作废。不自动改写链接。
2. **VS Code 文档检查**：打开已信任工作区的 Markdown 文件 → 命令面板“ReadMD: 检查文档链接与标题” → 原生“问题”面板定位。活动草稿修改后防抖更新，保存、文件创建/删除/重命名后失效旧缓存；外部程序修改工作区文件也会触发刷新，忽略生成目录的噪声。
3. **VS Code Wiki 跳转**：打开包含双链的文档 → 原生链接操作跳转；明确工作区范围，重名目标不猜测。复用 VS Code 的标准 Markdown 链接能力。
4. **工作区检索**：命令面板“ReadMD: 搜索本地 Markdown 工作区” → 输入标签、短语与排除词 → Quick Pick 显示标题、相对路径和摘要 → 打开结果行。多工作区先选择范围，取消请求不应用迟到结果。
5. **MCP 读取**：readmd_read_document 返回指定行段、原始字节 SHA-256、编码及换行信息。
6. **MCP 修改预览**：readmd_edit_document 携带文件版本及文字替换，默认只预览。多处匹配须明确 replace_all，不执行模糊替换。
7. **MCP 提交**：相同版本 + dry_run:false, confirm:true → 校验内容大小与编码 → 集中恢复记录 → 重新检查版本 → 原子写入。文件已变化则停止，不生成原目录副本。
8. **MCP 历史**：readmd_document_history 列表或读取同一文档记录；原文件删除后仍可读取历史。恢复需先审阅并获取当前版本，不自动覆盖未保存的编辑器草稿。

MCP 从 22 项增加到 27 项；VS Code 从 22 条贡献命令增加到 24 条。新增桌面文案经 i18n-add.mjs 同步 46 个语言包，简中、繁中、英文提供原生文案，其余新增项使用英文回退。

## 文件安全与上限

- 单次检查/编辑文件最多 2 MiB；读取最多 1,000 行。拒绝不能无损重新编码的输入。
- 搜索最多 5,000 个 Markdown 文件、累计 64 MiB；返回最多 100 项。处理上限及跳过数明确返回，不把未检查内容当作通过。
- 标题与链接最多各 2,000 项、诊断最多 200 项、跨文档锚点最多检查 32 个目标。大目标无法检查时报告截断，避免误报缺失。
- 恢复沿用应用数据区：每文档最多 10 项、总计 64 MiB、最长 30 天。原目录不会为 MCP 编辑增加 .bak。
- MCP 最多并行 8 个工具，超额明确 server_busy。取消不会撤回已提交写入；跨程序写入仍可能在最终版本检查与原子替换之间竞争，不能把它描述成操作系统事务锁。
- MCP 文件工具处理磁盘文件；未知其他编辑器的未保存缓冲区，不应绕过宿主编辑器直接替换其草稿。

## 网站与后续复用

网站沿用现有 Apple 风格。website/release.json 区分公开正式版与候选版，离线构建使用它生成 version.json。正式发布并验证完整资产后再将 stable 改为新版本；同步工具不会把未发布候选写进公开下载链接。

四语言首页恢复六段滚动视频：滚动决定视频帧，支持倒退及章节切换，窗口随滚动缩放、轻微透视和位移。全站 50 个 HTML 页面共用组件入场、卡片悬停、指针光边、按钮按压及键盘焦点反馈；保留原版排版和明暗主题，视频不叠加提亮滤镜。最多两个视频节点，后台标签暂停动效，停止滚动后停止逐帧计算。减少动态效果或禁用 JavaScript 时所有章节仍可阅读，视频失败保留海报与真实操作链接。离线媒体派生与复验方法见 [网站动效说明](../../website/MOTION.md)。历史六段源录屏为 1280×800，转码不提升真实细节，不标成 2K；新增四条功能录屏为原生 2560×1600。

node website/tools/build-integrations.mjs 生成四语言集成说明。所有正式安装、VSIX、MCP 连接包和校验文件均指向对应发布资产；MCP 连接包不被描述成独立服务端二进制。

增量演示位于本机 showcase/updates/v0.0.5/，使用合成哲学阅读材料、真实桌面应用、真实 Extension Host 与 Rust MCP。新管线不改写旧 104 项录制清单。画面不包含系统文件夹、私人路径、录制编号或提示叠层；媒体和制作脚本不进入软件提交。

## 验收记录

### 本机完成的检查

| 验收层 | 结果与实际边界 |
| --- | --- |
| Rust 完整回归 | 本机完整运行 1,875 项通过、3 项保留原有忽略；116 项系统/原生测试通过，xtask 12 项通过。随后新增的编码与宿主锚点用例纳入 9 项文档智能定向测试，全部通过。最终完整数量以 PR 的 Windows/Linux CI 为准。 |
| JavaScript / TypeScript | 离线 TypeScript 严格检查 8 个源文件；扩展 46 项及前端/工具 49 项，共 95 项通过。 |
| 桌面 UI 全量回归 | 298 项通过，31 项按既有环境条件跳过；不能将跳过项目算作实测通过。新增用例覆盖编辑草稿、过期响应和请求边界。 |
| 解压后的 Windows 候选包 | 真实 WebView2 与原有 CSP，7 个操作流程通过；46 种语言切换保持草稿及单行工具栏，3 个主题、插入/撤销、图像保存、局域网分享均验证，零页面脚本错误。 |
| 安装后的 VSIX | VS Code 真实 Extension Host，14 个流程通过；含原生 Problems、外部重命名自动刷新、跨文档/同文档 Wiki 链接、检索、预览、确认写入、历史读取、演讲及 DOCX 导出。使用隔离配置，不覆盖日常扩展设置。 |
| MCP 真实 stdio | 27 工具发现，15 类验收检查通过，含新工具完整流程、版本冲突、原文件删除后的恢复、AI 模拟服务协议、取消、重复请求 ID 与超大输入后连接恢复。这里的 AI 服务是本机合成测试服务，不代表公网模型成功率。 |
| 打包后二进制压力 | 700 份合成文档；128 个瞬时请求中 8 个执行、120 个明确返回忙碌。客户端限制并发为 8 后，128 个请求全部完成。16 个竞争写入只有 1 次提交，其余为版本冲突或忙碌；默认预览不改文件，删除原文件后历史仍可读取。 |
| 真实文件 | 使用两份本机真实 Markdown 的隔离副本，大小 1,836,325 与 7,807 字节；读取版本、AST 检查、检索和修改预览通过，原件与副本的 SHA-256 均未变化。未把私人内容上传给 AI 或纳入视频。 |
| 网站 | 离线构建与发布校验通过，49 个 canonical 页面及既有 104 项演示清单检查通过；四语言集成页在两种宽度、明暗主题共 16 组浏览器检查通过。新增滚动动效在真实 Edge 完成 44 组检查，含六段定位、倒退、章节边界、320/390/1024 窄屏、暗色、MP4 回退、媒体失败、减少动态效果及禁用 JavaScript；89 个连续帧间隔的 p95 为 12.6ms，页面脚本错误为零。Windows 测试版 WebKit 未能解码该 WebM，不将它标为视频通过，也不能据此代替 macOS Safari 实测。 |
| 增量视频 | 4 条，原生 2560×1600、30 fps；分别展示当前草稿检查、Wiki/Problems、工作区检索、MCP 预览/提交/恢复。94 个抽样帧经过 OCR 隐私检查，全部通过；不宣称逐帧 OCR。 |
| 静态门禁 | i18n、样式、控件接线、资源、无 Python 构建链、隐私扫描和 Git 空白检查通过。媒体及制作文件均未进入软件提交。 |

验收日志保留在本机 .cache/v005/，候选包、校验和及源提交记录在 dist/candidate-v0.0.5/，VSIX 与 MCP 连接包位于 .cache/v005/integrations/。增量素材、录像脚本、清单、字幕和本地查看页位于 showcase/updates/v0.0.5/；这部分已被 Git 忽略，可在后续版本复制同一管线并替换合成材料。

### CI 与正式发布关卡

[V0.0.5 草稿 PR](https://github.com/Natsummerance/rust-ReadMD/pull/1) 的最新检查是跨平台结果的依据。CI 构建 Windows x64、Linux x64、macOS Intel 和 ARM64，并分别执行 Windows/Linux 内核测试、前端门禁、浏览器测试及 VS Code/MCP 集成验收。构建成功代表这些环境完成编译与对应自动检查，不代表已经在每个系统的实体机器上交互验收。

首轮 CI 揭示 MCP 验收脚本仍硬编码旧版 22 工具。已更新到 27 工具，并加入新工具实际读取、检查、检索、预览、确认、冲突与历史恢复操作；新增桌面检查用例也补入 CI。此前本机压力脚本与已安装扩展测到了新工具，但未运行这条独立的旧 stdio 入口，因此没有及时发现过期断言。后续发布须同时运行协议入口、宿主扩展和安装包验收，不能用源文件存在或单元测试代替可用性证明。

本轮只准备发布候选，没有合并 main、创建或移动 V0.0.5 标签，也没有替换已安装的正式 V0.0.4。正式发布前必须确认 PR 最新检查全部通过，复核平台资产和 SHA256SUMS.txt，再以同一提交创建正式发布。网站的 stable 和 published 只能在正式发布资产可下载后更新；candidate_ref 指向候选文档分支，正式版文档随合并进入 main。

不能从以上结果推断公网 AI 服务、全部操作系统/驱动、所有损坏文件或任意网络永不失败。受限输入、忙碌、取消、冲突和不可无损编码会明确拒绝；未运行的第三方环境须保留为边界，不标成通过。

### 离线复验入口

以下命令沿用已准备的工具链与依赖，不执行安装或下载：

    cargo test --offline --locked --manifest-path rust/Cargo.toml -p readmd-kernel -p xtask
    node --test packages/vscode-extension/test/*.test.js
    node packages/mcp-server/test/stdio.mjs
    node tools/package-integrations.mjs dist/integrations
    node tools/package-windows-candidate.mjs <已构建的ReadMD.exe> dist/candidate-v0.0.5 dist/ReadMD-Pet-Rust.zip
    node website/tools/build-integrations.mjs
    node website/tools/build-motion-pages.mjs
    node website/tools/sync-version.mjs
    node website/tools/validate-website.mjs --release
    node website/tools/motion-smoke.cjs

stdio 验收要求 READMD_BIN、READMD_ASSETS_DIR、隔离的 READMD_MCP_TEST_ROOT；原生验收使用 ui-tests/panel-native-smoke.cjs，按脚本要求指定独立资料目录。Playwright 使用本机已安装 CLI，运行桌面项目及 CI 列出的移动项目；不使用会隐式联网安装的命令。
