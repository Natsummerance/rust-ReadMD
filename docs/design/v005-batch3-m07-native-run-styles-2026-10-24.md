# 第三批下一单元：公式局部字符样式来源

2026-10-24。状态：candidate-68 有限单元已实现、实测并归档读回，见[结果与明确边界](../reviews/readmd-v005-batch3-math-native-runs-2026-10-24.md)。下文保留原实施合同；主题、完整全局默认值、外部应用及完整 M07／第三批未销账，后续按[下一路径](v005-batch3-after-native-runs-2026-10-24.md)推进。

## 可交付边界

先实现公式内 `m:r` 和已有 `m:ctrlPr` 的局部四轴原生来源：`w:b`、`w:i`、`w:sz`、`w:color/@w:val`。允许显式 `rStyle` 的 character basedOn 链，以及当前已选 pStyle 链中这些 rPr 声明。复用现有 Word 上下文与原生样式注册表，不新建平行出版模型。

不是只增加 styles.xml 定义：获准内容必须真正引用这些样式，且没有把继承值全部写成 direct 值遮住后续样式修改。作者 DSL、节点身份、原始声明和恢复来源保持不变。

| 属性 | 保留与消费合同 | 准入反例 |
| --- | --- | --- |
| b、i | Missing、空节点及显式关闭分别保留。复用已有 style toggle 与 direct absolute 的不同语义。 | 两层 style true 抵消；style false 不等于 direct off；删 direct 后应恢复继承。 |
| sz | 保留源整数词法，消费域沿现有 RunStyle 的 1..3276 半点。首轮实际材料包含 24、30。 | 缺省不得补 0；超消费域不宣称原生可编辑；恢复与当前实际来源一致。 |
| color | 仅六位 RGB，保留源大小写词法。 | auto、themeColor、tint、shade 不当作 literal RGB。 |
| 样式引用 | 原 style type、basedOn、显式 rStyle 及已选段落链分别建模；段落与字符 ID 使用同一冲突域。 | 循环、缺父、错误类型、过深链、同 ID 异定义及模板撞名。 |

每条链必须证明来源与目标四轴根默认相容，或由获准链／direct 完整覆盖。缺少证明就沿用物化与明确损失说明，不能把 Missing 默认为 false／0 来通过。默认 character style 的自动选择、段落标记 rPr、其他 Word run 属性、非空全局 rPrDefault 与主题仍单独保留。

## 实现定位与顺序

1. `math_word.rs` 的 chain/capture 已捕获 paragraph／character 链并校验循环、类型与深度；先补四轴来源描述、缺省状态和准入判断。
2. 同文件 apply/materialize 已按 rPrDefault → paragraph chain → character chain → direct 计算；保留这套消费副本作为独立对照，不修改作者树以迁就原生导出。
3. `math_word/native_paragraph.rs` 当前签名与 style_xml 主要覆盖段落 pPr。让真实四轴 rPr、递归祖先及类型进入同一注册／别名域，避免两个不同字符样式被合并。
4. `omml.rs` 当前先捕获段落来源，再物化内容。增加受资格检查保护的原生内容投影，保留有效 rStyle／direct 层次；不通过在注册表增加定义却继续输出全部 direct 有效值来冒充完成。
5. 复核 run_style、数学 m:sty 与 control_style 的优先级，包含分数线等控制对象。DOCX、阅读／SVG、Rust PDF、Typst、HTML、EPUB 和预检使用同一有效来源。

## 普通正文隔离

- 只由公式内引用获准的新别名；不输出新的 default=true，不替换 Normal、BodyText 或模板 character styles。
- 不改全局 docDefaults、theme、settings 和 sectPr，也不通过给普通正文补 direct 0／false 抵消影响。
- 比较普通段落完整 direct／有效属性、普通公式、代码、链接与注释；实际 PDF 比较可见字体程序、字形、颜色、路径和位置。
- 部分属性不合格时，不悄悄删掉来源。保持可恢复源码和现有准确物化行为，单列该来源没有取得原生编辑认证。

## 自动与实际验收

先以固定材料测试，再统一离线编译、绑定新候选，禁止改旧候选或降低绘制标准。

| 材料／操作 | 验证 |
| --- | --- |
| A/B 两份仅含原生 Word 声明的输入，同 ID 不同四轴定义 | 转换、迁移、精确保存、新 PID 重开；对照源声明及实际有效值。 |
| A+B 正序／逆序，混入普通正文和无载荷公式 | 别名稳定、段落／字符 ID 全域无冲突；普通内容和目标模板不受污染。 |
| style true/true、style false、direct off／删除 direct | 检查 toggle、覆盖、恢复继承；不能只断言序列化 XML 存在。 |
| 第二式精确改、撤、重做及旧修订请求 | 只有目标源范围改变，节点与非目标声明保持，保存重开仍一致。 |
| 只改已导 DOCX 当前 character style 的 color／sz | 用独立 ZIP/XML 工具修改，再重导入；真正继承者变化、direct 遮盖者保持。未改 native 重导入另作基线，区分模板迁移与外改因果。 |
| 删除已导第二式 direct off | 重新计算后恢复其样式 toggle；原 ZIP 非目标成员保持。 |
| 非空 pPrDefault、不同 rPrDefault／theme、消费域外值 | 原声明仍可找回，沿用明确物化／拒绝策略；不计原生保真通过。 |

五类实际输出为 DOCX、HTML、EPUB、Rust PDF、Typst PDF。数学 SVG、PDF 字体／绘制与手写 direct 有效值控制严格比较；复制层使用本单元明确语料的独立检查，不能沿用旧七式常量或整类隐藏字体放行。HTML 逐式核对实际模板分支选中的向量，EPUB 沿 spine 核对真实 MathML。

请求、响应、三进程 stdio/events、输出、检查器参数及 stdout／stderr绑定来源摘要和唯一矩阵。分别执行源保存、重开导出、native读回。失败需保留原目标字节、目录条目和首次记录；不自动重试取得通过。完成后才更新有限里程碑，完整 M07／P04／第三批保持独立。

## 明确保留的后续项

非空 pPrDefault 当前有 incompatible-paragraph-doc-defaults 保护；不能只删除 guard，或把源默认改成数学基样式冒充原来源。异源主题涉及包级固定槽位，需整包相容证明；style alias 本身不能解决 themeColor／themeFont。

本规格的 ZIP 外改与 CLI 回流不等于 Word／WPS／LibreOffice 实际编辑验收。宏逐字形映射、复杂来源编辑／恢复、完整全局默认、P04 非零 Typst 基线、完整 CSL、正式离线资源与 TeX 闭包、故障／八小时压力、其他平台继续按原清单推进。
