# 第三批：脚注内容与局部排版接续

2026-10-12，接续 B3-56、R028／R029／R069。MVP 已有证据不覆盖；本段修复 Word 简化脚注写出器造成的内容损失，并补齐既有局部排版在脚注中的真实链路。

- Word 脚注复用正文的段落、引用、原生编号列表、表格、代码和公式写出逻辑；保持脚注自身的字号／行距和唯一标记，局部覆盖优先。
- 通过原有局部排版菜单编辑脚注段落，不增加界面。重复文字按源范围定位；元数据不拆断定义／列表；一次撤销、重做、重置和保存重开保持原文结构。
- Word、Typst PDF 和 EPUB 共享局部排版语义。首段局部覆盖不能让脚注编号单独占一行；PDF 需检查实际坐标及页面，EPUB 使用固定离线验证器。
- 普通阅读与编辑预览渲染脚注完整块内容，再进行 HTML 清理；代码示例和转义定义保持原文。编辑预览使用独立的注释锚点，避免与隐藏阅读区冲突，源行保持原定义位置。
- Word 标准不允许脚注里再次产生脚注引用。此类输入明确拒绝 Word 导出并保留旧输出和原文，不生成不合规文件、不静默删除引用；EPUB 自身支持的注释引用不因 Word 限制被禁止。错误翻译覆盖现有46种语言。
- 使用真实正式 CSP Windows 宿主、合成资料、固定能力包，验证实际文件与保存重启。自动测试和原生报告分别绑定代码／构建；不把 ZIP 合法计作实际 Office 应用验收。

实际 Office、所有复杂脚注／节／尾注、完整 OMML、全 CSL、长期压力和非 Windows 平台继续按原合同验收。本段不修改版本或发布状态。

标准依据：[Microsoft Footnote](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.footnote?view=openxml-3.0.1)、[FootnoteReference](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.footnotereference?view=openxml-3.0.1)。
