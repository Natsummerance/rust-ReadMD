# ReadMD V0.0.4

本次版本整合桌宠、编辑保存、功能面板、原生窗口与完整演示网站的升级。程序、安装包和更新检查统一使用 `0.0.4`，更新源切换到 `Natsummerance/rust-ReadMD`。

- **全模块 UI/UX 精修**：表单使用单滚动主体、固定标题及操作区，低频参数折叠；完善转换与图片空状态、搜索导航和共享链接复制。修复重复插入、取消表格仍修改文档、自定义内容被覆盖、OCR 失败计数、网页取消反馈和原生焦点丢失；图片保存防止重复提交，失败可重试，异步结果不会改写已变化的文档。全部 29 个对话框经过三主题、五语言及两种窗口尺寸检查，详情见 `docs/reviews/readmd-uiux-2026-10-06.md`。

- **桌宠**：Rust 原生拖动与输入响应；BongoCat 的键盘、鼠标和敲击交互；原有精灵图与 Live2D 保留各自的互动机制。缩小窗口和尺寸范围，恢复角色预览、点击互动、文件拖入及右键菜单，支持选择是否置顶。
- **编辑与保存**：只在内容实际变化时提醒保存。编辑副本、格式转换与 AI 修改保留原件；增加草稿恢复、历史版本、撤销与替换冲突检查，避免未经确认覆盖文件。
- **AI、导出与插件**：精简设置面板，提供连接反馈，密钥不回显；完善导出预设和预览一致性；优化插件搜索、安装、卸载及开关持久化。
- **原生窗口**：统一原有 ReadMD LOGO，自定义 Windows 标题栏，将全局操作收进上方菜单；完善托盘驻留、热启动和默认打开方式入口。
- **功能验收**：按 104 条功能流程、399 个静态控件、80 组动态交互和 100 个接口建立实现及验收清单。
- **演示网站**：沿用 Apple 风格进行精修，更新 104 段简短操作演示。画面只展示软件，不包含个人文件夹窗口、录制编号或提示浮层；字幕默认关闭，提供独立宣传视频素材清单。
- **发布修复**：修正下载链接、更新仓库和 Windows 便携包匹配；桌宠运行时随安装包和便携包一同交付。锁定依赖，离线编译；附 SHA-256 校验清单。
- **本次修复**：移除桌宠上方的暗色覆盖层，恢复透明桌面；修正 Windows OCR 原图路径中的反斜杠，避免生成的 Markdown 图片链接失效。
- **AI 与桌面扩展修复**：模型发现改为使用当前保存的连接和安全凭据，支持 OpenAI 兼容、Anthropic 与本地模型服务，并准确反馈认证和网络失败；收紧 AI 输入区，统一为 14px 复选框。桌宠扩展安装到用户数据目录，避免安装目录权限导致启用失败，恢复重启后的运行方式，并保留真实失败状态。
- **生产场景复查**：修复 Windows 虚拟化安装目录下的资源拒绝、扫描 PDF 取消无效、损坏 Office 文档假成功、超宽表格的内存扩张，以及导出 HTML 重导入失败；限制启动索引工作量，避免大资料目录延迟窗口出现。真实资料副本与并发读写、冲突和批量转换压力检查见 `docs/reviews/readmd-production-audit-2026-10-06.md`。
- **编辑与语言精修**：编辑工具栏始终单行，根据窗口和当前语言收纳低频工具，字数与恢复状态移至底部；移除设置里的重复退出项。补齐 46 语言词库中的新增面板文案，修复预览标题、草稿状态和 AI 空态切换语言后遗留旧文本的问题。
- **图谱路径修复**：扫描文件在权限检查前统一规范路径，避免 Windows 临时目录别名和含 `..` 的合法工作区被错误跳过，保留原有目录边界检查。
- **桌面查找与替换**：编辑时搜索当前草稿，支持逐项替换和可撤销的全部替换；CodeMirror 加载失败时备用编辑器仍可使用。大批匹配明确提示上限，不静默替换一部分，原文件在保存前保持不变。
- **VS Code 与 MCP 升级**：工具箱分组折叠、完整中英文界面；异步识别已安装内核，修复握手、重连和取消。预览、公式及工程图表随 VSIX 离线打包，演讲主题和源目录图片生效；AI 只应用最终正文并保护期间发生的修改。MCP 提供 22 个工具，兼容当前逐请求发现及旧握手，补齐模型、真实导出预设、图表、结构化结果、参数校验、并发和消息上限。社区对照及实测边界见 `docs/reviews/readmd-community-upgrade-2026-10-07.md`。
- **演讲导出复验**：单文件导出移除外部字体请求，按需内嵌 Mermaid 渲染器，与公式、本地图片一起在断网浏览器中验证；不只检查文件是否生成。
- **后续发布**：发布流程从 `VERSION` 读取版本，核对标签与源码提交；内核、前端和 UI 的共享质量检查及全部打包任务通过后，才允许正式替换下载包。完整上传并校验全部新包后再替换旧包，最后更新校验清单与正式发布状态。

## 下载

| 平台 | 文件 |
| --- | --- |
| Windows x64 | `ReadMDSetup-windows-x64.exe` / `ReadMD-windows-x64.zip` |
| macOS Apple Silicon | `ReadMD-macos-arm64.dmg` / `ReadMD-macos-arm64.zip` |
| macOS Intel | `ReadMD-macos-x64.dmg` / `ReadMD-macos-x64.zip` |
| Linux x86_64 | `ReadMD-linux-x86_64.deb` / `ReadMD-linux-x86_64.tar.gz` |
| VS Code 扩展 | `readmd-vscode-0.0.4.vsix` |
| MCP 文档与连接模板 | `readmd-mcp-server-0.0.4.zip`（内核使用桌面主程序） |
| 完整性校验 | `SHA256SUMS.txt` |

Windows 便携版须解压整个 ZIP 后运行 `ReadMD.exe`；不要只移动其中的可执行文件。桌宠的全局键盘/鼠标监听和 BongoCat 敲击响应目前在 Windows 上可用，Linux/macOS 尚未提供全局输入监听。Linux 桌面版需要 GTK/WebKitGTK 运行库。macOS 包未做 Apple 公证。此版本不提供 Windows/Linux ARM64 或 AppImage；VS Code 扩展需要桌面内核，MCP 使用主程序的 `--mcp` 入口。

从旧版本迁移时请先下载本版本安装包或完整便携包；旧版本内部版本号与原有发布标签不一致。本版本建立统一的后续更新基线。

验证与功能证据见仓库 `docs/reviews/`。四个发布目标由 GitHub Actions 构建；Windows 原生交互已在本机验收。AI、邮件、联网转换等功能仍需要可用的服务配置，发布文件不包含用户密钥或私人资料。

---

This release upgrades native desktop pets, safe editing and recovery, AI/export/plugin panels, Windows chrome and tray behavior, and the Apple-style website with 104 privacy-reviewed immersive demonstrations. Release versions and update endpoints now consistently target this Rust repository. All desktop bundles include the verified Rust pet runtime and ship with SHA-256 checksums.
