# ReadMD V0.0.6 for macOS

包含 V0.0.6 迁移分支的审阅、任务、快照和恢复增量，以及中文路径、资源定位和原生导出预览修复。Mac PDF 页面使用系统 Quartz 渲染；演示文稿按同一份 16:9 模型预览并导出可编辑 PPTX，TeX 预览为生成源码。HTML 预览读取与导出相同的冻结正文和图片字节，并等待图片解码后显示。

最顶栏采用原生红黄绿窗口按钮：窗口操作在左，其余控件反转并靠右，LOGO 位于最右。阅读和编辑工具栏保留原有顺序；全屏、禅模式、未保存提示及关闭确认沿用原生窗口与现有保存保护。

- Apple Silicon：ReadMD-macos-arm64.dmg / ReadMD-macos-arm64.zip
- Intel：ReadMD-macos-x64.dmg / ReadMD-macos-x64.zip
- 完整性校验：SHA256SUMS.txt

采用免费 ad-hoc 签名，没有 Developer ID 或 Apple 公证。首次打开按[安装说明](macos-install.md)在系统设置中仅对 ReadMD 允许打开，无需付费开发者账号；默认 Gatekeeper 并未被声明为放行。

基础中文路径编辑、保存、关闭、会话恢复、外部修订保护，以及六种格式的原生预览／提交采用本轮独立候选记录。原生 QA 构建、正式安装副本冷启动及 headless 录制分别绑定摘要，不混用。较大的中文出版字体支持受管离线安装。

非 Windows 多文件引用重命名继续明确拒绝并保护原文。DOCX 预览仍是共享布局参考，不是 Word 分页；PPTX 的文字、项目符号、表格和代码可编辑，公式保留矢量外观并提示其为图像。网页演讲的离线 HTML 输出继续保留。实体 Intel、旧版系统、真实 IME／键盘和全部功能认证继续待验，详见 [V0.0.7 延续清单](../docs/design/v007-carryover-2026-10-30.md)。
