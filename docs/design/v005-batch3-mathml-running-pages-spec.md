# 第三批接续：MathML 样式与书籍双面页眉页脚

本增量继续 B3-55、B3-56，完整第三批验收门槛不变。

## MathML

导入时解析 math／mstyle 的数学字体变体、前景色和物理字号继承；内层显式值覆盖外层，兄弟节点互不污染。单字符 mi 缺省斜体，多字符 mi、mn、mo 缺省正体。文字 mtext 的粗斜体写入 Word 文字属性，避免与数学 nor 冲突。

支持标准数学变体；前景色支持 #RGB、#RRGGBB 及标准基础颜色；字号支持可无损表示为 Word 半磅的 pt、px、mm、cm、in。不能准确表达的颜色、字号、背景、绝对 scriptlevel 及其他高级属性保留内容并报告已有投影诊断，不 silently 宣称兼容。控制符、分式线与根号使用所在节点的样式，不能被子槽位覆盖。

math／mstyle 中已支持的分式、括号、装饰、矩阵缺省属性按各自作用范围继承。需要验证嵌套覆盖、重复公式槽位、可读 DSL 往返、实际矢量字形／颜色／尺寸及真实 Word、EPUB、PDF 输出。

## 双面出版

现有高级页眉页脚设置增加“奇偶页不同”及偶数页文字。未启用时保留现有样式身份和输出；启用后现有文字为奇数页文字，偶数页使用明确的另一组文字，空值必须保持空白，不能偷偷继承奇数页。

DOCX 使用原生 evenAndOddHeaders、偶数页部件、关系与节引用；Typst PDF 按物理页奇偶选择，页码起始值不改变单双面分配。首页隐藏与奇偶设置独立组合。共用纸张、字体、距离及对齐参数。

不支持的输出引擎必须报告版式损失；设置保存在用户预设并通过真实进程重启验证。46 语言文案、1024×680 和 1160×820 原生 UI 检查，测试材料只用专用合成内容。

依据：[MathML 3 样式](https://www.w3.org/TR/MathML3/chapter3.html#presm.mstyle)、[Word 双面页眉页脚](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.evenandoddheaders?view=openxml-3.0.1)。实际 Word 应用、独立电子书阅读器及完整平台矩阵继续独立记录。
