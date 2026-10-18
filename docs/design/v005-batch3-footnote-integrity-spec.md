# 第三批 MVP：重复脚注与资源完整性修复

2026-10-12。M3-01–08 已通过；本次只修复真实论文／书籍输出的基础脚注缺陷，不扩张 MVP 或等待完整出版认证。

- 同一个脚注被正文多次引用时，DOCX 中只生成一个定义，所有引用指向同一 ID；内部递归身份回归检查 ID 不碰撞、不无限递归，不把这类输入计作 Office 支持。
- 脚注自己的外链与图片关系写入 `word/_rels/footnotes.xml.rels`；复用正文图片时仍能正确引用。不复制无关正文关系，不重复存储图片。
- 原生文件导出验证实际 Word 包、命名空间、关系目标与媒体；EPUB／PDF 保持可用，固定离线验证器检查 EPUB。
- PDF 脚注采用实际字体升部／降部边界，正文及脚注的图像区域不能与文字墨迹范围相交；以真实 PDFium 坐标和页面图共同验证，不只检查文本存在。
- 未保存不写原文；保存重开、重新导出仍保持引用身份与资源去重。无效输入不覆盖已有输出。
- 只复用现有界面与接口，无新增依赖、文案或系统 Office 依赖。实际 Office 应用渲染仍单列，ZIP／XML 检查不冒充应用认证。

脚注内局部排版、任意嵌套块／尾注、纯链接分页定位及完整 B3-51–57 门槛继续保留。Office 标准禁止脚注内容中再放 `footnoteReference`；嵌套脚注的拒绝／交叉引用策略仍需实现，不属于本次普通正文引用验收。依据 [Microsoft FootnoteReference 说明](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.footnotereference?view=openxml-3.0.1)。完成本修复不关闭 R028／R029／R069。
