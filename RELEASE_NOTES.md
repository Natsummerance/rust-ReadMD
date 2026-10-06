# ReadMD V0.0.4

本次版本整合桌宠、编辑保存、功能面板、原生窗口与完整演示网站的升级。程序、安装包和更新检查统一使用 `0.0.4`，更新源切换到 `Natsummerance/rust-ReadMD`。

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
- **后续发布**：发布流程从 `VERSION` 读取版本，核对标签与源码提交；内核、前端和 UI 的共享质量检查及全部打包任务通过后，才允许正式替换下载包。完整上传并校验全部新包后再替换旧包，最后更新校验清单与正式发布状态。

## 下载

| 平台 | 文件 |
| --- | --- |
| Windows x64 | `ReadMDSetup-windows-x64.exe` / `ReadMD-windows-x64.zip` |
| macOS Apple Silicon | `ReadMD-macos-arm64.dmg` / `ReadMD-macos-arm64.zip` |
| macOS Intel | `ReadMD-macos-x64.dmg` / `ReadMD-macos-x64.zip` |
| Linux x86_64 | `ReadMD-linux-x86_64.deb` / `ReadMD-linux-x86_64.tar.gz` |
| 完整性校验 | `SHA256SUMS.txt` |

Windows 便携版须解压整个 ZIP 后运行 `ReadMD.exe`；不要只移动其中的可执行文件。桌宠的全局键盘/鼠标监听和 BongoCat 敲击响应目前在 Windows 上可用，Linux/macOS 尚未提供全局输入监听。Linux 桌面版需要 GTK/WebKitGTK 运行库。macOS 包未做 Apple 公证。此版本不提供 Windows/Linux ARM64、AppImage、独立 VSIX 或 MCP ZIP；VS Code 扩展源码在仓库中，MCP 使用主程序的 `--mcp` 入口。

从旧版本迁移时请先下载本版本安装包或完整便携包；旧版本内部版本号与原有发布标签不一致。本版本建立统一的后续更新基线。

验证与功能证据见仓库 `docs/reviews/`。四个发布目标由 GitHub Actions 构建；Windows 原生交互已在本机验收。AI、邮件、联网转换等功能仍需要可用的服务配置，发布文件不包含用户密钥或私人资料。

---

This release upgrades native desktop pets, safe editing and recovery, AI/export/plugin panels, Windows chrome and tray behavior, and the Apple-style website with 104 privacy-reviewed immersive demonstrations. Release versions and update endpoints now consistently target this Rust repository. All desktop bundles include the verified Rust pet runtime and ship with SHA-256 checksums.
