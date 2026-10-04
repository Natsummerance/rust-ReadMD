# ReadMD V0.0.4

本次版本整合桌宠、编辑保存、功能面板、原生窗口与完整演示网站的升级。程序、安装包和更新检查统一使用 `0.0.4`，更新源切换到 `Natsummerance/rust-ReadMD`。

- **桌宠**：Rust 原生拖动与输入响应；BongoCat 的键盘、鼠标和敲击交互；原有精灵图与 Live2D 保留各自的互动机制。缩小窗口和尺寸范围，恢复角色预览、点击互动、文件拖入及右键菜单，支持选择是否置顶。
- **编辑与保存**：只在内容实际变化时提醒保存。编辑副本、格式转换与 AI 修改保留原件；增加草稿恢复、历史版本、撤销与替换冲突检查，避免未经确认覆盖文件。
- **AI、导出与插件**：精简设置面板，提供连接反馈，密钥不回显；完善导出预设和预览一致性；优化插件搜索、安装、卸载及开关持久化。
- **原生窗口**：统一原有 ReadMD LOGO，自定义 Windows 标题栏，将全局操作收进上方菜单；完善托盘驻留、热启动和默认打开方式入口。
- **功能验收**：按 104 条功能流程、399 个静态控件、80 组动态交互和 100 个接口建立实现及验收清单。
- **演示网站**：沿用 Apple 风格进行精修，更新 104 段简短操作演示。画面只展示软件，不包含个人文件夹窗口、录制编号或提示浮层；字幕默认关闭，提供独立宣传视频素材清单。
- **发布修复**：修正下载链接、更新仓库和 Windows 便携包匹配；桌宠运行时随安装包和便携包一同交付。锁定依赖，离线编译；附 SHA-256 校验清单。

## 下载

| 平台 | 文件 |
| --- | --- |
| Windows x64 | `ReadMDSetup-windows-x64.exe` / `ReadMD-windows-x64.zip` |
| macOS Apple Silicon | `ReadMD-macos-arm64.dmg` / `ReadMD-macos-arm64.zip` |
| macOS Intel | `ReadMD-macos-x64.dmg` / `ReadMD-macos-x64.zip` |
| Linux x86_64 | `ReadMD-linux-x86_64.deb` / `ReadMD-linux-x86_64.tar.gz` |
| 完整性校验 | `SHA256SUMS.txt` |

Windows 便携版须解压整个 ZIP 后运行 `ReadMD.exe`；不要只移动其中的可执行文件。Linux 桌面版需要 GTK/WebKitGTK 运行库。macOS 包未做 Apple 公证。此版本不提供 Windows/Linux ARM64、AppImage、独立 VSIX 或 MCP ZIP；VS Code 扩展源码在仓库中，MCP 使用主程序的 `--mcp` 入口。

从旧版本迁移时请先下载本版本安装包或完整便携包；旧版本内部版本号与原有发布标签不一致。本版本建立统一的后续更新基线。

验证与功能证据见仓库 `docs/reviews/`。四个发布目标由 GitHub Actions 构建；Windows 原生交互已在本机验收。AI、邮件、联网转换等功能仍需要可用的服务配置，发布文件不包含用户密钥或私人资料。

---

This release upgrades native desktop pets, safe editing and recovery, AI/export/plugin panels, Windows chrome and tray behavior, and the Apple-style website with 104 privacy-reviewed immersive demonstrations. Release versions and update endpoints now consistently target this Rust repository. All desktop bundles include the verified Rust pet runtime and ship with SHA-256 checksums.
