# 第三批：复合上下标的真实字体竖向间距

接续 B3-55。当前布局已读取真实四角 kern，但上下标同时出现时只向下推下标，遗漏字体的 `SuperscriptBottomMaxWithSubscript` 上推分配规则。

- 读取已自带数学字体的对应 MATH 常数，不绑定系统字体或 Office。
- 在上下标最小间距不足时，按标准先在允许范围上推上标，再向下移动下标；保留单独上下标、紧缩样式和真实字体横向角距。
- 依据 [OpenType MATH](https://learn.microsoft.com/en-us/typography/opentype/spec/math) 和 [MathML Core 复合上下标](https://www.w3.org/TR/mathml-core/#base-with-subscript-and-superscript)，以独立 FontTools 字形／常数材料比较真实绘制坐标，不以实现自身生成期望值。
- 覆盖无需调整、部分上推、完全上推、达到上限、分组和嵌套，检查 DSL／OMML 读写、预览、撤销、保存及真实 HTML／PDF 输出。
- 完整 MATH 常数、完整 OMML／MathML／UnicodeMath、实际 Word 和完整第三批仍分别取证。未实测的部分不得认证。
