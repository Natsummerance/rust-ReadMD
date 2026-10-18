# 第三批增量：公式装饰语义与注释起始编号

本增量属于 B3-55、B3-56，不改变完整第三批或平台验收门槛。

## 公式

- OMML 分组字符省略 pos 时放在下方，省略 chr 时为 U+23DF；显式空字符保持为空。
- vertJc 省略为 top，元素存在但值省略为 bot。位置与基线独立，四种组合须改变实际矢量排版，MathML 使用相应基底与上下附加项。
- 长分组括号和重音优先使用 OpenType MATH 横向变体与装配，保持笔画粗细；装配数量有界。
- MathML 上下装饰区分重音、横线、分组符和普通上下限。显式 false 覆盖默认值；已知重音符支持自动识别。不能准确映射的复杂装饰保留表达内容并报告投影诊断，不计为完整 MathML 认证。
- 可读 DSL 与 OMML 往返、重复公式槽位编辑、一次撤销、明确保存后重开和实际 DOCX／EPUB／PDF 输出均需验证。Word 应用和独立 EPUB 阅读器证据单列。

## 出版

- 在现有注释高级选项添加起始编号，1–9999；缺省仍为 1，不改变旧预设的结构身份。
- DOCX 使用原生 w:numStart；Typst 脚注使用计数器，尾注使用统一序号偏移；EPUB 标记与回链显示使用同一偏移。
- 序号与注释身份分离，重复引用不能新建定义或改变链接目标；原有 EPUB 逐章脚注／全书尾注策略保持兼容。
- 原生 UI 验证用户预设保存与进程重启、真实 PDF 文字与链接、DOCX 原生属性、EPUB 验证及不支持引擎的损失提示。
- 控件文字在现有 46 种语言提供本地化；人工语义审校仍是独立未完成门槛。

依据：[Office GroupCharProperties](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.groupcharproperties?view=openxml-3.0.1)、[Office VerticalJustification](https://learn.microsoft.com/en-nz/dotnet/api/documentformat.openxml.math.verticaljustification?view=openxml-2.10.1)、[MathML 3 上下附加项](https://www.w3.org/TR/MathML3/chapter3.html)、[Word NumberingStart](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.numberingstart?view=openxml-3.0.1)。
