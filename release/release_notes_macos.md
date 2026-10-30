# ReadMD V0.0.6 for macOS

包含 V0.0.6 迁移分支的审阅、任务、快照和恢复增量，以及中文路径、资源定位和原生导出预览修复。Mac PDF 页面使用系统 Quartz 渲染；演示文稿按真实 16:9 幻灯片预览并导出离线 HTML，TeX 预览为生成源码。

- Apple Silicon：ReadMD-macos-arm64.dmg / ReadMD-macos-arm64.zip
- Intel：ReadMD-macos-x64.dmg / ReadMD-macos-x64.zip
- 完整性校验：SHA256SUMS.txt

采用免费 ad-hoc 签名，没有 Developer ID 或 Apple 公证。首次打开按[安装说明](macos-install.md)在系统设置中仅对 ReadMD 允许打开，无需付费开发者账号；默认 Gatekeeper 并未被声明为放行。

基础中文路径编辑、保存、关闭、会话恢复、外部修订保护，以及六种格式的原生预览／提交采用本轮独立候选记录。原生 QA 构建、正式安装副本冷启动及 headless 录制分别绑定摘要，不混用。较大的中文出版字体支持受管离线安装。

非 Windows 多文件引用重命名继续明确拒绝并保护原文。DOCX 预览仍是共享布局参考，不是 Word 分页；HTML 幻灯片不是 PPTX。实体 Intel、旧版系统、真实 IME／键盘和全部功能认证继续待验，详见 [V0.0.7 延续清单](../docs/design/v007-carryover-2026-10-30.md)。
