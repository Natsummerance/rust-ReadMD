# 第三批：流式出版的段落分页控制

继续 B3-56，复用正文样式中的孤行控制、段内不分页，不增加常驻 UI。

- HTML／EPUB 支持显式孤行开关：开启时段首段尾至少两行，关闭时允许单行；使用 CSS orphans／widows。HTML 既有预设未设置此参数时保持原打印默认值。
- 段内不分页映射 break-inside 与兼容 page-break-inside；显式关闭恢复 auto。不以一个过长段落无法放进单页为软件错误，CSS 分页约束仍依输出引擎。
- DOCX 和 Typst 的原生实现保留；内置 PDF 不显示失效开关，API 对无法表达的显式设置继续报告损失。
- 真正输出 HTML、EPUB、Word 及 PDF；检查原生计算样式、原生 Word 属性、实际 PDF 重开、预设保存重启、EPUBCheck、窄窗及目录无污染。独立 EPUB 阅读器及 Word 应用仍需另外的互操作证据。
