# 第三批：文末尾注交付合同

2026-10-12。接续 B3-56／57、R028／R029／R048。沿用普通 Markdown 注释、局部排版、预设和三格式输出，不另建注释工作台。

- 现有导出高级设置支持页下注／文末尾注，缺省保持旧行为；设置可保存为用户预设并重新加载。普通文档和打开操作不新增元数据／资源。
- DOCX 使用原生 endnotes 部分、endnoteReference／endnoteRef、所属关系和文末位置。重复引用共用定义，外链及图片关系完整；公式、列表、表格与局部格式保留。原有嵌套 Office 注释拒绝和原子输出保护继续生效。
- Typst PDF 将尾注放在正文之后，按所选编号样式集中排版，标记双向链接；长尾注正常分页，重复引用不重复内容。局部样式和资源复用既有注释链路。
- EPUB 将文末尾注作为全书最后一个导航章节；跨章重复引用共用身份、定义和完整返回链接。尾注内容保留结构、MathML、图片及样式；独立 EPUBCheck 必须通过。
- 支持格式必须实际生效；不支持的引擎报告损失，不能默默忽略位置。UI 在不支持的引擎隐藏选项，动态状态和新文案覆盖46语言。
- 使用真实 Windows 宿主、锁定离线能力包和合成书稿，验证预设重载、三格式输出、保存重启、取消／错误与旧输出保护。实际 Office 应用往返与非 Windows 证据仍独立保留。

本增量只验证 Markdown 原文保存重开及三格式输出，不以输出检查冒充外部 Word 文件往返。现有 DOCX 导入器尚无完整注释读取链路，保留在 B3-57；需另行实现注释部分及关系解析并测试后，才可声称 Word 脚注／尾注导入无损。该缺口不阻塞已确定的第三批 MVP 主流程。

参考：[Microsoft Endnote](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.endnote?view=openxml-3.0.1)、[EndnoteProperties](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.endnotedocumentwideproperties?view=openxml-3.0.1)、[EPUB 注释语义](https://www.w3.org/TR/epub-ssv/#endnotes)、[Typst 编号](https://typst.app/docs/reference/model/numbering/)。
