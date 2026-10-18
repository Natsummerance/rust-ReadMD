# Math DSL v1

本协议与八轮修订验收账本 R059 关联。已接通 Rust 编译、正文渲染、源范围编辑和现有 OfficeMath 导出适配；完整 Office 属性、外部扩展、迁移、辅助技术与实际 Word 往返仍须分别验收。本文件不宣称 Office 全兼容。

## 文档语法

行内使用 `:math[x^2 + α]`。块公式使用：

````markdown
```readmd-math v1
frac(a + b, c, style: skewed)
```
````

既有 `$…$`、`$$…$$`、`\(…\)`、`\[…\]` 不改变解释。代码示例、行内代码、HTML 注释中的 DSL 不参与公式编译。`v1` 是语法版本，不是软件版本。

## 自然表达与结构

优先级由低到高为比较、加减、乘除、幂；幂右结合，其他二元操作左结合。`a / b` 表示分式，`x^2` 表示上标，`x_i` 表示简单下标。括号改变结合方式；复合下标使用 `subscript(x, i + 1)`。`*` 输出数学乘点，`<=`、`>=`、`!=` 分别输出 `≤`、`≥`、`≠`。

普通表达式的括号同时保留为可渲染定界符，例如 `(a + b) * c` 显示括号并保持原结合语义；属性数值的括号仍只作数值分组。Unicode `−` 支持一元及二元减号，`÷` 为同乘法优先级的可见除号，`/` 仍生成分式。运算符、下标基底和下标内容分别保留精确 UTF-16 源范围，渲染中的重复续行符号仍对应一个源码节点。运算符槽仅接受单个合法运算符，避免注释吞掉后续操作数；普通内容槽仍可输入完整表达式。无效或过期修改被拒绝且保留草稿，不改正文。

合法 Unicode 可直接输入；符号别名复用内置数学符号目录，例如 `alpha`。双引号字符串保留文字而不展开别名，转义遵循 JSON 字符串。`//` 或 `#` 到行末是注释。命名参数使用 `name: value`；重复参数、缺少参数、非法属性、非矩形矩阵和未知函数会报错。

| 用途 | 写法 |
|---|---|
| 分式 | `frac(a, b, style: stacked)`；样式还有 `skewed`、`linear`、`no_bar` |
| 根式 | `sqrt(x)`、`root(3, x)` |
| 上下标 | `subscript(x, i)`、`superscript(x, n)`、`subsup(x, i, n)`、`prescript(i, n, x)` |
| 矩阵 | `matrix([a, b], [c, d], align: [left, right], column_gap: 2em)` |
| 方程组 | `equations(a = b, c = d)` |
| 算子 | `sum(x_i, from: i = 1, to: n, limits: above_below)`；另有 `product`、`integral`、`nary` |
| 定界符 | `delimiter(a, b, open: "{", close: "}", separator: "|")` |
| 函数 | `function(sin, x)` |
| 上下限 | `lower_limit(x, n)`、`upper_limit(x, n)` |
| 重音、横线、组合字符 | `accent(x, character: "̂")`、`bar(x, position: below)`、`group_char(x, character: "⏟")` |
| 其他 Office 结构 | `box(x)`、`border_box(x)`、`phantom(x, show: false)` |
| 空槽与连续内容 | `empty()` 表示零子节点槽；`row(a, b)` 表示连续内容而非附加括号 |

矩阵间距必须能由 Office 精确表达：`em` 为半 em 步进，`pt` 为 1/20 点步进。不能精确表达的值报错，不能悄悄舍入。当前矩阵限制 256 行、64 列，单公式文本上限 64 KiB，编辑辅助面板暂保留 16,000 字符限制。

## 高级 Office 属性

结构函数接受标准属性名，或通过 `properties` 提供完整属性容器：

```text
frac(a, b, properties: office.fPr(office.type(val: "skw")))
office.oMath(office.r(office.rPr(office.nor(val: "0")), office.t("x")))
```

`office.<元素名>(子对象…, 属性名: "值")` 表达固定 schema 中的标准节点。`word.<元素名>(…)` 表达关联 Word 信息；`"xml:space": "preserve"` 等带前缀属性使用引号。该层是可读文本，不是压缩 XML。Word 子对象目前按信息保留，尚未有完整 Word schema 验证，不能计入完整属性认证。

导入 Word 公式时，相关的 `word.styles(...)` 和 `word.pPr(...)` 与数学对象并列保存，仍使用上述可读语法；只保留文档默认格式和实际引用的段落／字符样式链，不复制整份 Word 文档。它们属于公式的格式上下文，不是插入正文的 Word XML。打开和预览不改源文件，明确保存时才迁移为 DSL。

Rust 共享解析器依次解析文档运行格式默认值、段落样式的 `basedOn` 链、所选字符样式链和运行直接格式。样式内粗／斜体的开启切换前值，关闭保持继承值；直接格式的关闭则明确关闭。段落标记自己的 `word.rPr` 不套用于整个段落的正文。源样式、原始值和显式关闭仍保留；样式循环、缺少被引用样式及错误类型返回错误，不能静默丢弃保真信息。[Word 粗体继承与 toggle 定义](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.bold?view=openxml-3.0.1)

当前运行格式的粗／斜体、半点字号和显式 RGB 颜色进入阅读、就地预览、MathML 及共享矢量输出；普通文本也保留粗／斜体。DOCX 导出副本将继承结果写为运行的直接格式，移除内部上下文及无外部定义的样式引用，源 DSL 不变。原始样式目录的身份回流、主题与字体选择、复杂文字字体、Word 修订和完整段落版式仍须继续完成；不能把这一子集称为完整 Word 样式支持。

公式控制字符遵循 [Office ctrlPr 规则](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.controlproperties?view=openxml-3.0.1)：存在 `office.ctrlPr(...)` 时，使用段落继承加控制字符自己的格式；整个元素省略时，使用对象首个文本字符的格式。空元素、明确自动颜色与省略不同。控制格式只作用于分数线、根号、括号、大算子及其他结构标记，不覆盖分子、分母、上下限或正文的独立格式。

Rust 共享矢量布局以半点字号计算结构几何、线宽及字间距，保留脚标自动缩小；显式字号不随宿主阅读字号再次缩放。每个绘制项分别保存已解析的自动颜色或 RGB，子对象的自动颜色不会被外层 RGB 覆盖。阅读、就地编辑、SVG 和矢量 PDF 使用相同结果；DOCX 继续保留控制属性。

MathML 与矢量布局共用数学样式深度、参数字号调整和 bundled STIX MATH 缩放常量。Word 格式先在临时渲染树计算独立控制字符／运行的物理点数及颜色，不修改源码或 DSL；显式大小不会覆盖脚标自动缩小，子对象自动颜色不会误继承外层 RGB。普通无 Word 格式的公式保留原生 MathML 重排。HTTP 的预览字号同时用于 MathML 与矢量；EPUB 的默认格式来自导出预设。含公式的 EPUB 自带现有 STIX 数学字体及完整 OFL 许可，普通书籍不增加这些资源。实际 Edge 检查已覆盖分式、嵌套、根指数、上标及默认颜色；完整字体／主题、响应式投影、实际 Word 和独立电子书阅读器仍需分别验收。

未写属性、空属性元素、显式默认值和显式关闭保持不同状态。标准枚举与数值约束使用已固定的 OpenXML schema 检查，包含必填属性、UInt16/UInt32 范围和 `xml:space`。粒子校验检查命名空间、子对象顺序、选择分支及出现次数；例如分母放在分子之前、重复分子、缺少分式样式的必填值均拒绝导出。12 类数学枚举的每个 facet 有可读属性编译/打印测试，不能据此认证所有组合的视觉效果。

出现次数的解释来自同一固定提交的 SDK [Particle](https://github.com/dotnet/Open-XML-SDK/blob/b424997c2c81410d375a72bf68837b87b7b807e8/gen/DocumentFormat.OpenXml.Generator.Models/Models/Particle.cs) 与 [ParticleVersion](https://github.com/dotnet/Open-XML-SDK/blob/b424997c2c81410d375a72bf68837b87b7b807e8/gen/DocumentFormat.OpenXml.Generator.Models/Models/ParticleVersion.cs)：未声明为 1..1，显式空对象为 0..无上限。校验有计算预算，不能无限展开重复分支。

未识别命名空间、注释和外部扩展不会被静默扁平化：转换为可读 DSL 返回不支持扩展的错误，原始文档仍可保留；隐藏扩展载荷迁移尚待完成。

## 编译与编辑契约

Rust `math_dsl` 编译到现有 `MathDocument`；OMML、MathML、LaTeX 投影及矢量 PDF 复用同一数学树。HTTP 与 MCP 的数学接口接受 `dialect: "dsl"`，序列化接受 `target: "dsl"`。

源码保持原样，并为渲染节点提供 UTF-16 范围。符号和槽位修改按节点身份回写原位置，保留注释与其他公式。常见命名属性更新只修改对应参数；结构操作寻找最小可映射祖先，只重写该结构，并重新编译比较整棵 XML 语义，确认未修改子树不变后才应用。不能精确重写时返回错误并保留草稿，不悄悄规范化整条公式。结构内部新排版可能规范化；任意注释格式与全部结构组合仍须扩大验收。

新插入公式默认 DSL，显式 LaTeX 导入及已有公式继续兼容。打开文件不迁移磁盘；正文导出在内存中降低到现有 OfficeMath AST，压缩适配信息不写回用户 DSL。无效 DSL 的正式导出在创建产物前失败。

源码编辑在 DSL 上下文内自动提供符号别名及常用结构补全，普通正文、frontmatter 和代码示例不触发。长文档按视口排版；后台排版请求上限为四个并发，同一表达式请求合并，槽位编辑使用独立交互请求。通用排版扫描排除已生成的 MathJax 容器，保持读屏用 MathML 原样。

块语法只接受完整的 `readmd-math v1` 信息串。不支持的版本（包括缺少版本或附加未知选项）保持可读源码，并在阅读与编辑预览显示本地化提示。`/api/academic/document` 返回原始 UTF-16 标头范围的 `math_unsupported_version` 诊断；正式导出和 EPUB 共用入口在写入前失败，保留已有产物。代码示例、缩进代码及原始 HTML 中的版本字样不触发该诊断。行内 `:math[...]` 当前固定使用 v1。

旧元数据打开时不写盘；明确保存时，只把经双向编译和 XML 语义比较证明无损的标准内容迁移为 DSL。源码摘要失配时，保留新正文，把旧属性先持久化为独立、受保护的可读恢复副本，再分离旧注释。未知扩展及不能无损迁移的内容仍保留原载荷，不计作迁移完成。具体限制与恢复入口见 [旧公式迁移与恢复](../guides/formula-migration.md)。

已完成第二批的有效离开应用和未完成草稿集中恢复；第三批新增源格式保护、可读空槽、上述版本保护和标准旧公式的明确保存迁移。尚需完成：完整语法错误位置、所有属性的局部格式保持、未知扩展最小隐藏载荷、完整结构属性补全及缺字替代、任意组合与所有标准属性/枚举的逐项认证、实际 Word 往返、全语言和辅助技术原生验收。

### Word 主题上下文及视觉内容替换

主题字体按需保留可读 `drawing.fontScheme(...)` 和 `word.themeFontLang(...)`。八种令牌按语言及补充脚本解析；同层主题优先，较高层直接字体仍覆盖低层主题。源 DSL 不被有效副本改写，未知字体不从宿主目录加载。普通文本按字符槽位使用私有 Sans／Serif 常规和粗体，数学字母保持核心 STIX；斜体目前合成倾斜。MathML 使用可嵌入族名，EPUB 按需携带字面和许可证，尚未子集化。字体安装／卸载后失效旧缓存；复杂文字、所有脚本及实际 Word 往返仍待验收。详见[字体选择记录](../reviews/readmd-v005-batch3-math-font-selection-2026-10-12.md)。

公式按需保留可读 `drawing.clrScheme(...)` 与 `word.clrSchemeMapping(...)`，只携带实际引用的调色板项；DrawingML 的普通属性不加命名空间前缀。Word 运行保留 themeColor／themeTint／themeShade 及原始 RGB。渲染和导出副本解析 sRGB、系统色 lastClr、角色映射与 HSL 明度调整；tint 优先。无主题时保留已有 RGB，不二次着色；外部主题不联网读取。重复／嵌套调色板及非法映射拒绝，其他 DrawingML 色彩类型和变换尚未支持。

视觉槽位输入与高级源码分离。Word 运行格式不直接塞进槽位输入框；内容替换保留单一运行的明确格式，新指定的属性优先；混合运行不盲目继承首段。普通源、运算符及注释仍走精确源码替换。实际两处重复公式中的第二处分子编辑、一次撤销／重做、保存重开和三格式输出已验证，见[主题色与格式保留记录](../reviews/readmd-v005-batch3-math-theme-2026-10-12.md)。官方 shade 示例与明确公式不一致，独立计算采用明确公式；实际 Word 像素对照仍待验。此子集不代表全部字体、主题或任意嵌套编辑已完成。
