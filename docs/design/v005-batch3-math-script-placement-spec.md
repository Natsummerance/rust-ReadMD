# 第三批：显式上下限与上下标布局

继续 B3-55。修正实际布局，不能只保存属性。

- limLow／limUpp 始终把缩小的上下限放在基线对象正下／正上方，行内公式也不能变为右侧脚标。nary 的上下限位置规则独立保留。
- sPre 的前置上下标靠右对齐到正文，保留脚标与基底间距；空脚标不增加无用间隔。
- sSubSupPr.alnScr 区分省略、显式关闭和开启（含无 val 默认开启），实际矢量的上下标使用相同起始列。缺省保持现有斜体修正。
- MathML 使用末尾零宽非斜体节点表示共同起始列；导入识别准确结构，恢复 alnScr，不能把任意非零间距误识别。独立浏览器已暴露缺省斜体字距差异：严格失败单独保留，API／EPUB 导出必须返回本地化损失提示。本增量不认证缺省 MathML 字距保真，原第三批验收要求保持。
- 验证默认值、所有开关值、变量长度、嵌套、源范围、重复公式、一次撤销、保存重开及真实 DOCX／PDF／EPUB。独立 MathML 浏览器排版另测，不能只靠产品 SVG 自证。

依据：[Office AlignScripts](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.alignscripts?view=openxml-3.0.1)、[Upper-Limit 对象](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.limitupper?view=openxml-3.0.1)、[MathML Core 脚标及行布局](https://www.w3.org/TR/mathml-core/)。字形轮廓的完整 OpenType MATH kerning 不因本增量自动认证。
