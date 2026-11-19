# ReadMD V0.0.7 for macOS

2026-11-19

本版更新 PDF 原版页面阅读、DOCX／PPTX 只读原排版视图、表格合并与行列尺寸、图片和历史另存、共享建议冲突合并、SVG 绘图、OpenAPI 与聊天取消流程。版本与 VS Code／MCP 集成包统一为 `0.0.7`。

## 下载与安装

- Apple Silicon：[DMG](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-macos-arm64.dmg) · [ZIP](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-macos-arm64.zip)
- Intel：[DMG](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-macos-x64.dmg) · [ZIP](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-macos-x64.zip)
- [SHA-256 清单](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/SHA256SUMS.txt)

面向 macOS 13+。退出旧版后，把 `ReadMD.app` 移入“应用程序”，个人文档无需移动。**本版使用免费 ad hoc 签名，未使用 Developer ID，未作 Apple 公证。** 首次打开可能被 Gatekeeper 拦截，请按[安装说明](https://github.com/Natsummerance/rust-ReadMD/blob/V0.0.7/release/macos-install.md)在“系统设置 → 隐私与安全性”中仅允许 ReadMD 打开。

## 阅读、出版与能力包

桌面包提供对应架构的 Office 阅读运行时和离线 PDFium 包；运行应用无需 Python 或 Microsoft Office。Office 原排版显示使用随包引擎，复杂文档可能与 Word／PowerPoint 有差异。PDF 默认进入阅读模式，选择“编辑原 PDF”后才进入支持的编辑操作。

OCR、Typst、出版字体、代码运行时和转写按实际平台能力包提供。转写依赖匹配平台的 FFmpeg、Whisper 引擎与模型；这两个独立离线包未包含在本次桌面／集成下载资产中。本轮媒体包实际操作覆盖 macOS ARM64，其他架构以实际包目录及能力状态为准。多文件引用重命名继续保留非 Windows 平台的安全限制。

## Mac 本机验证

V0.0.7 的 Mac 本机候选在 ARM64 上完成六组共 **244 项原生检查**，包括关闭、保存冲突、草稿恢复、六格式导出、候选 ZIP 冷启动／重开与 V0.0.6 隔离会话升级。最终归档生产二进制完成 46 语言保存编码流程，页面异常为 0。[本机记录](https://github.com/Natsummerance/rust-ReadMD/blob/V0.0.7/docs/reviews/readmd-v007-macos-rc-2026-11-19.md)保留一次 WebKit 冷启动首败和同包复测结果。Intel 包由独立构建目标生成，其原生操作证据与 ARM64 分开记录。

正式 Apple Silicon 下载 ZIP 另完成正常系统冷启动与重开，真实文档可见、会话保留、原测试文档字节不变；本次正式包的二进制与归档摘要见[发布验证记录](https://github.com/Natsummerance/rust-ReadMD/blob/main/docs/reviews/readmd-v007-published-2026-11-19.md)。此结果与上文 244 项早期候选检查分别记录。

[完整 V0.0.7 更新](https://github.com/Natsummerance/rust-ReadMD/blob/V0.0.7/release/release_notes.md) · [使用指南](https://github.com/Natsummerance/rust-ReadMD/blob/V0.0.7/README.md) · [问题反馈](https://github.com/Natsummerance/rust-ReadMD/issues)
