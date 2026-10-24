# 第三批 M07：完整单位体系与小数几何

2026-10-24，实施规格。承接C57的已归档结果，替代上轮“仅精确整twip单位”建议；本轮支持六种合法单位及非整twip值，不以拒绝毫米等常见输入完成单位能力。完整M07、第三批和发布仍按原账本验收。

## 数值、源码和原生表示

作者层继续使用现有`WordParagraphMeasure::{Integer,UnitLiteral,CharacterUnits}`、稀疏声明和逐轴origin。解析、源事务、迁移不得把`1mm`重写为`57`或浮点近似字符串。

有限布局的左右轴改为封闭的约分有理数points，`num/den`使用checked u128，den非零；到现有布局坐标边界才转换f64。无过时的整数twip与points双字段。系数固定如下：

| 作者单位 | 精确points系数 |
| --- | --- |
| 无单位整数twip | 1/20 |
| mm | 360/127 |
| cm | 3600/127 |
| in | 72 |
| pt | 1 |
| pc、pi | 12 |

十进制先去前导及小数尾零，再checked构造分子/10幂并交叉约分。精确比较当前合法消费域0..1584pt（原0..31680twip）；禁止先转f64比较、整数取整、溢出截断或把失败当0。比1584只大一个极小十进制、但转f64会舍成1584的值仍须拒绝。负零和超长全零可规范化为数学零，作者原文不改。超有理数容量的合法源码保持可编辑，消费给出明确Unsupported；非法词法按现有schema诊断。

原有负缩进、其他缩进轴、RTL/方向/宿主、预算等保护不在本单元随意扩大；局部math jc和dispDef的既有优先规则保持。单位有小数本身不再作为拒绝理由。

`DocxMathParagraph.left/right`使用有效作者词法StringValue；先通过同一有限验证，再XML属性转义写入。输出保留原单位，不整数化、不从f64格式化生成近似值；整数属性同样保持明确语义。原生包仍使用现有ReadMDMath/direct有效轴物化，不能把单位词法保留夸大成原style/docDefaults包的完整往返。

Office2007整数约束与Office2010后的整数/六单位联合分别记录。本轮不宣称把单位属性转换成任意旧Office版本的等价格式。

## 源码与实际流程验收

1. 纯数值验证覆盖六单位、等价写法、1mm/非整twip、上下界、极小超界、长零、容量、负值及非法词法。逐轴验证默认/选中style链/direct继承、删除direct恢复来源，以及原始context前后相等。
2. math parse/patch、共享SVG、MathML、HTML、EPUB和两PDF共用物理值；源码与原生DOCX各保留raw单位。保留局部math优先、行内/嵌套/其他宿主、宽度不足及消费前置保护。
3. 两份native-only DOCX分别left/right，各含三个相同公式；缩进来自direct、实际basedOn链和docDefaults。有效left预设1mm/0.5in/2pc，right预设1cm/18.125pt/1.5pi，所有六单位参与实际流程。独立Python Fraction解析raw与数值，不复用产品换算结果作期望。
4. 转换、迁移、API保存原LF稿；另建明确全CRLF原始测试材料，保存原件摘要，实际读取revision并比较完整模型后再编辑。只改第二重复公式的选定单位轴，验证邻近范围、逆事务/重做、过期公式/正文拒绝、保存、删除direct恢复继承。两个PID正常EOF/重开后，以空options导出。
5. 每case输出DOCX/HTML/EPUB/Rust PDF/Typst PDF，再做同源明确零缩进的两PDF对照，正向上限14份。逐式比较真实字形/GID/路径位移；left为+L，right为-R，各式不同，不能使用一个全局dx假定。既有绝对0.25pt门槛保留，另固定足够严格的差分门槛，抓出1mm被round成57twip约0.01535pt的误差。
6. DOCX有效轴raw词法逐项相等，重导入按实际direct来源计算；HTML检查真正选择的renderer，EPUB检查全部spine；SVG隐藏/phantom/惰性内容不能充当可见输出。
7. 非法namespace/顺序/缺值、超精度/超范围、方向/其他轴及非零Typst baseline拒绝，已有真实PDF和目录保持。全部warnings保留，字体替代不算无替代通过。

先冻结源码与QA，再锁定离线编译、完整Rust、前端/工具和六门槛，最后跑新候选实际流程。每个首次失败保留，新尝试使用新目录；不修改C55/C56/C57或旧oracle以追认通过。toolbar、首绘、4K及boot无前端改动时保持摘要，无需重打包。

## 原始依据与复用

复用现有math_word、共享布局、原生writer和锁定引擎，不引入依赖。

- [Microsoft Indentation](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.indentation?view=openxml-3.0.1)：缩进逐轴继承。
- [Open-XML-SDK官方schema](https://github.com/dotnet/Open-XML-SDK/blob/main/data/schemas/schemas_openxmlformats_org_wordprocessingml_2006_main.json)：CT_Ind left/right在Office2010后的union包含整数与`-?[0-9]+(\.[0-9]+)?(mm|cm|in|pt|pc|pi)`；2007单独约束。
- [docx项目的单位定义与换算](https://github.com/dolanmiu/docx/blob/master/src/util/values.ts)：六单位及pc/pi对应pica，换算不取整。仅参考定义，ReadMD按自己的Rust有理数实现，未引入该项目运行库。

真实Office/独立阅读器、原样式包保真、完整M07/P04、正式TeX材料/离线包、冷态、完整故障/八小时及其他平台依然分别验收；不由本单元成功替代。

## 本轮实施结果

C58已按此规格实现并完成限定真实验证：Rust3648/0/3既有忽略、前端工具1078/0/0、六门槛、单位7检查/14输出/28几何/27旧PDF保护；同候选另12组复杂出版回归。见[完整结果与范围](../reviews/readmd-v005-batch3-math-unit-measures-2026-10-24.md)和[下一样式链规格](v005-batch3-m07-native-style-registry-2026-10-24.md)。原style/docDefaults来源、字体替代、非零Typst基线及整项验收限制不变。
