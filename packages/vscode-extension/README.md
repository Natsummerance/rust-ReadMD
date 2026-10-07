# ReadMD for VS Code · V0.0.4

在 VS Code、Cursor 等兼容编辑器中阅读、修复和转换 Markdown，使用 ReadMD 的 AI Skills，并导出文档与演讲。保留编辑器原生的保存、撤销和未保存提示。

## 安装和连接

安装 ReadMD 桌面应用，再用 **Extensions: Install from VSIX…** 安装 `readmd-vscode-0.0.4.vsix`。打开 Markdown，执行 ReadMD 命令即可。

扩展按需启动一个持久的 `readmd --mcp` 子进程，通过标准输入输出通信；没有 HTTP 服务地址设置，不需要打开桌面窗口，也不需要 Python。探测顺序：`readmd.executablePath` → 随 VSIX 附带的内核 → 桌面应用默认安装目录 → PATH。Windows 包括用户的 `Applications/ReadMD`、`LocalAppData/Programs` 和 Program Files。

安装在其他目录时，设置 `readmd.executablePath` 为可执行文件的完整路径。修改后连接会重建；进程异常退出时，下次操作重新连接。卸载扩展不删除源文档、桌面设置或凭据。

## 工具箱与 22 个命令

标题栏保留预览入口，低频操作收进菜单和分组工具箱。所有命令均可在命令面板搜索。

| 命令 | 实际行为与交互 |
| --- | --- |
| `readmd.preview` | 每个源文件复用一个实时预览；双向滚动同步；公式、目录、标题链接、本地图片和工程图表离线渲染。 |
| `readmd.fixCurrentDocument` | 修复当前草稿，作为一次可撤销编辑应用；等待期间文档改变则停止替换。 |
| `readmd.openAiWorkbench` | 选择 Skill、已保存的连接和模型；生成后选择替换选区/全文、插入末尾或另开结果。 |
| `readmd.openSkills` | 从实际内核列出 Skills，选择后打开内容。 |
| `readmd.openSkillByUri` | 通过工具箱指定 URI 打开 Skill。 |
| `readmd.insertCodeChunk` | 插入 Python、JavaScript、Bash、R 或 Go 模板。 |
| `readmd.runCodeChunk` | 选择代码或将光标放入围栏；确认后调用本地运行时；结果面板展示输出、标准错误和生成图片。支持反引号和波浪号围栏。 |
| `readmd.insertDiagram` | 插入 PlantUML、TikZ、WaveDrom、Vega-Lite、Graphviz、Bitfield 模板。 |
| `readmd.insertDocImport` | 插入 `@import` 引用。 |
| `readmd.processImports` | 从当前文件目录解析引用，在新文档中展示展开结果。 |
| `readmd.insertFrontmatter` | 插入文档和演讲元数据，已有元数据时提示。 |
| `readmd.insertToc` | 插入 `[TOC]`，实时预览根据标题生成可点击目录。 |
| `readmd.insertSlide` | 插入 `<!-- slide -->`。 |
| `readmd.openPresentation` | 使用随包 Reveal.js 播放；支持分页分隔符和 Frontmatter 主题、转场，忽略元数据内的分隔线。 |
| `readmd.exportPresentation` | 选择路径，导出含脚本及源目录图片的离线 HTML；公式、Mermaid 生效，主题字体使用系统回退；保存框确认替换后可覆盖。 |
| `readmd.exportDocument` | 选择 PDF、Word、HTML、LaTeX 或 EPUB；预设从实际桌面内核读取，包括自定义预设；相对图片以源目录解析。 |
| `readmd.convertFileToMarkdown` | 从资源管理器或文件选择器转换；保留源文件，结果在未保存的新文档中打开。 |
| `readmd.convertAnyFilePrompt` | 打开同一转换选择器，含“所有文件”，不会把内核支持范围限制为少量后缀。 |
| `readmd.fetchWebToMarkdown` | 输入有效 HTTP(S) 网址，提取后打开新文档；操作会联网。 |
| `readmd.convertToLatex` | 将当前草稿转换为 LaTeX，在新文档中打开。 |
| `readmd.parseBibtex` | 读取工作区或所选 BibTeX 数据库并报告条目数。 |
| `readmd.setupMcpServer` | 写入选定工作区的 VS Code/Cursor MCP 配置，或复制 Claude Desktop 模板。保留其他服务及 JSONC 注释；无效配置不覆盖。 |

## 预览、样式与执行

预览脚本、公式字体和图表资产随 VSIX 打包，不从 CDN 下载。文档脚本、事件属性及外部追踪图片不执行/加载。外链点击交给编辑器打开，本地文档链接在编辑器中打开。远程图片以替代文字显示。

Mermaid、WaveDrom、Bitfield、Graphviz、Vega/Vega-Lite、Chart.js、TikZ 使用离线渲染器。需要表达式编译或 WASM 的引擎运行在可销毁的隔离 iframe，文档无法执行脚本。图表外部数据 URL 禁用。PlantUML 使用本机 Java/PlantUML，缺少依赖时保留源码并提示；不会自动把源码上传到代理。WSD/D2/Ditaa 使用内核的基础语法 SVG 渲染器。

`readmd.customCssPath` 指向本地 CSS 文件（最多 256 KB；相对路径以第一个工作区目录为基准）。修改设置会刷新已有预览。限制模式下忽略该设置，预览仍可用；原生工具、AI、运行代码和 MCP 配置写入要求工作区信任。执行代码还需逐次确认，取消不会运行。

演讲元数据支持 `theme`、`transition`、`title`、`author`、`slideNumber`、`width`、`height` 的简单标量；主题和转场也可放在 `presentation:` 下。主题使用随包资源和系统字体回退，不请求在线字体。这不是完整 YAML 配置解析器；文档内的任意 `custom_css` 不执行，预览样式通过上述本地设置管理。

代码执行使用本机运行时，不能把它当作安全沙箱。Python/Matplotlib 等可选运行环境由用户自行提供；依赖不足会反馈错误。

## AI 与文件保护

先在桌面 ReadMD 配置连接；扩展读取连接和模型，模型为空时向该连接获取列表，也可刷新。只传凭据引用，不在扩展设置中保存 API Key。选中文本时仅发送选区；否则发送全文。只有主动运行 AI 操作才发送文档。

内核进度通知是状态，不能拼接为 AI 正文。结果以最终响应为准，默认等待用户选择应用方式。生成期间文件版本改变时，禁止直接替换，仍可另开结果或插入末尾。应用结果使用原生编辑事务，可撤销；不自动保存、不在源目录生成备份。

取消会通知内核并停止应用结果。已发出的第三方网络请求可能仍在服务端执行；不承诺撤销已发生的远程费用或已提交的文件写入。扩展目前没有桌面聊天会话管理器或逐 token 输出界面。

## 维护与离线打包

已准备现有锁定开发工具后，在仓库内运行：

```sh
cd packages/vscode-extension
npm run compile
node --test test/*.test.js
node scripts/package-vsix.mjs
```

编译先从仓库 `assets/vendor` 暂存渲染资产，不下载新依赖。VSIX 打包只使用 Node 内置模块，包含 `out`、`media`、文案和 snippets，排除源码、测试、node_modules 与 source maps。默认使用已安装的桌面内核；`scripts/stage-core.mjs` 的可选平台内核打包仍可使用。

额外验收：`ui-tests/vscode-preview.config.cjs` 验证真实浏览器 CSP/公式/图表；`test/host.cjs` 可通过 VS Code `--extensionTestsPath` 验证实际宿主和 Rust stdio，需要独立测试目录。

## English

ReadMD starts a persistent local `readmd --mcp` process on demand. Install the desktop app and this VSIX, or set `readmd.executablePath`. Preview/presentation assets are bundled for offline use. The toolbox groups editing, conversion, export, Skills and integrations; all 22 commands are available from the command palette.

Conversion and generated results open as unsaved documents. Repairs and AI replacements use undoable editor transactions and guard against intervening edits. AI sends only selected text when there is a selection; otherwise the document. Network operations occur only when invoked. Code execution requires workspace trust and explicit confirmation. Cancellation discards late results but cannot reverse a remote operation already submitted. Local preview CSS is optional; document scripts and remote images stay blocked.
