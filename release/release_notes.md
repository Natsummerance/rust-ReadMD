# ReadMD V0.0.7

2026-11-19

本版围绕“先阅读、再编辑、明确保存”更新本地文档工作流：原版页面阅读、表格与出版、共享建议合并，以及连续关闭、恢复和任务取消的保护共同升级。桌面应用、VS Code 扩展与 MCP 配置包统一为 `0.0.7`。

## 本版更新

- **PDF／Office 先阅读，再提取**：PDF 默认以原版页面阅读，编辑原 PDF 与提取 Markdown 分别进入。DOCX、PPTX 增加只读原排版阅读；提取时报告修订、公式、演示效果等格式损失，保护原件与已有同名 Markdown。
- **表格编辑与出版**：支持矩形合并、拆分、单元格属性、行列尺寸和合并区域复制粘贴，保留单元格 Markdown 与撤销历史。PDF、DOCX、PPTX 输出按各自支持范围保留布局、尺寸和样式。
- **保存、恢复与批量任务**：改进快速连续关闭标签、编码转换确认、图片资源和编辑历史的跨目录另存、待审修改撤回。批量转换和媒体任务保留持久回执与恢复状态，取消、失败、过期结果和保存成功分别处理。
- **共享审阅与绘图**：建议发生冲突时，可对照原文、当前内容与建议手动合并，保留提案和决定历史；合并草稿可离线恢复，结果通过明确保存写入文件。新增绘图入口与 SVG 保存、重开、撤销和恢复。
- **开发者与 AI**：多文件 OpenAPI 支持本地引用、源映射、生成和更新；依赖文件变化后拒绝旧预览。VS Code 增加 OpenAPI 审阅更新及默认关闭的行内 AI 补全。JSON 与流式聊天共用取消和历史提交规则，失败、取消和隐私聊天不保存部分历史。
- **界面与语言**：修复语言切换、恢复按钮和编码确认提示；改善画布、原版阅读及表格在三种主题下的样式。46 语言的键、占位符和限定保存流程经过检查，部分新增工作流提示仍使用英文。

## 下载与安装

| 平台 | 文件 |
| --- | --- |
| Windows 10/11 x64 | [安装包](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMDSetup-windows-x64.exe) · [完整便携 ZIP](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-windows-x64.zip) |
| macOS 13+ · Apple Silicon | [DMG](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-macos-arm64.dmg) · [ZIP](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-macos-arm64.zip) |
| macOS 13+ · Intel | [DMG](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-macos-x64.dmg) · [ZIP](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-macos-x64.zip) |
| Linux x86_64 | [DEB](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-linux-x86_64.deb) · [tar.gz](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-linux-x86_64.tar.gz) |
| VS Code 扩展 | [readmd-vscode-0.0.7.vsix](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/readmd-vscode-0.0.7.vsix) |
| MCP 文档与连接模板 | [readmd-mcp-server-0.0.7.zip](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/readmd-mcp-server-0.0.7.zip)；使用桌面主程序的 `--mcp` 入口 |
| 完整性校验 | [SHA256SUMS.txt](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/SHA256SUMS.txt) |

替换旧版前先退出应用。Windows 便携版须完整解压并保留目录结构；Windows 桌面需要 WebView2。Linux 桌面需要 GTK3／WebKitGTK 4.1 等系统运行库。Windows／Linux ARM64、AppImage、HarmonyOS／OpenHarmony 不在本版下载资产内。

**Mac 使用免费 ad hoc 签名，未使用 Developer ID，未作 Apple 公证。** 首次打开可能被 Gatekeeper 拦截。按 [Mac 安装说明](https://github.com/Natsummerance/rust-ReadMD/blob/V0.0.7/release/macos-install.md)在“系统设置 → 隐私与安全性”中仅允许 ReadMD 打开。

## 能力与支持范围

桌面包随平台提供 Office 阅读运行时与离线 PDFium 包，应用按需准备资源。运行应用无需安装 Python 或 Microsoft Office。Office 原排版视图使用随包排版引擎，复杂文档可能与 Word／PowerPoint 显示不同；PDF 编辑和表单、签名、权限，以及各输出格式均按具体支持范围处理。PPTX 公式保留为图片，并在备注保存源码。非 Windows 的多文件引用重命名继续保留既定安全限制。

OCR、Typst、出版字体、代码运行时及音视频转写依实际平台能力包提供。转写需要匹配平台的 FFmpeg、Whisper 引擎与模型；**这两个独立离线包未包含在本次十份桌面／集成资产中**。本轮媒体包实际操作覆盖 macOS ARM64，其他平台以可获得的包及能力状态为准。网页、AI、邮件等联网操作需要主动调用并配置服务。

## 验证记录

macOS ARM64 完成六组共 **244 项原生检查**，涵盖关闭与恢复、六格式导出、最终 ZIP 冷启动／重开和 V0.0.6 隔离会话升级。归档内的生产二进制另通过 46 语言保存编码流程，页面异常为 0。[Mac 本机记录](https://github.com/Natsummerance/rust-ReadMD/blob/V0.0.7/docs/reviews/readmd-v007-macos-rc-2026-11-19.md)保留一次 WebKit 冷启动首败，以及未改包后成功的同包复测结果。

发行 CI 配置 Windows x64、Linux x64、macOS ARM64／Intel 及 VSIX／MCP；正式发布需等待共享质量检查和全部构建。[平台矩阵](https://github.com/Natsummerance/rust-ReadMD/blob/V0.0.7/release/platform-matrix.json)与[功能验证目录](https://github.com/Natsummerance/rust-ReadMD/tree/V0.0.7/docs/reviews)分别记录构建、原生操作及跨应用证据。

[使用指南](https://github.com/Natsummerance/rust-ReadMD/blob/V0.0.7/README.md) · [English guide](https://github.com/Natsummerance/rust-ReadMD/blob/V0.0.7/README.en.md) · [历史更新](https://github.com/Natsummerance/rust-ReadMD/blob/V0.0.7/RELEASE_NOTES.md) · [问题反馈](https://github.com/Natsummerance/rust-ReadMD/issues)

## English

V0.0.7 adds page-first PDF reading, read-only DOCX/PPTX layout views, merged-cell table editing, safer closing and recovery, manual resolution of shared suggestions, SVG drawing, multi-file OpenAPI updates and opt-in VS Code inline AI completion. Desktop, VSIX and MCP configuration packages share version `0.0.7`.

Downloads target Windows x64, Linux x64 and macOS Apple Silicon/Intel. Desktop bundles include matching Office and PDFium resources. Larger OCR, typesetting, font and media capabilities depend on platform packs; independent FFmpeg/Whisper packs are separate from these release assets. Mac packages use free ad hoc signing and are not Apple-notarized. The macOS ARM64 cycle completed 244 native checks; build coverage and native/platform interoperability evidence are recorded separately in the links above.
