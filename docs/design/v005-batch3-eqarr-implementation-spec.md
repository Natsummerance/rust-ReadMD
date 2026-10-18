# V0.0.5 第三批 M04：方程数组对齐与分布执行规格草案

日期：2026-10-15。状态：**只读审计与待授权实现设计**；本文没有运行 Rust、原生应用或 Word，没有改变 M04/第三批完成状态。现有 P01/P07、字体及分节证据保持原样。上位要求见 [剩余实施规格 M04](v005-batch3-remaining-implementation-spec.md)。

## 1. 固定依据与兼容边界

| 依据 | 本次确认的规则 | 不能据此声称的能力 |
| --- | --- | --- |
| [ISO 29500 摘录：eqArr](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.equationarray?view=openxml-3.0.1) | 每个 `m:e` 是独立行；行内奇数 `&` 是对齐点，偶数 `&` 是列间伸展位置，行首有隐含伸展位置；默认 `maxDist=0,objDist=0` | 不能把所有等号自动当作者指定标记，也不能删除连续标记形成的空组 |
| [maxDist](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.maxdistribution?view=openxml-3.0.1)、[objDist](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.objectdistribution?view=openxml-3.0.1) | 前者改变数组可分配宽度，后者控制数组内部内容的分布；元素省略为关，空元素为开，显式 false 必须生效 | 四个组合必须都测，但不要求四者必有不同几何；未知可用宽度不能假设为页面宽度 |
| [Alignment](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.alignment?view=openxml-3.0.1) | `boxPr/aln` 仅在 `opEmu` 开时使 box 成为对齐点；同页明确示例还以 `rPr/aln` 标记 run 开头的减号，作用于同一数学段落 | 不能因概述只写 box 就忽略 `rPr/aln`；box 内容 `==` 是一个完整对象，不能按文字拆开 |
| [MS-OE376 eqArr 注记](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oe376/d8d37e2b-532d-49db-9cba-2ef4bd8d9baa) | 有 c 列时，flush 开把多余空间放入 c−1 个列间位置；flush 关放入含两端的 c+1 个位置 | 该详细注记明确适用于 Office 2007/SP1/SP2；不是本机现代 Word 的实测 |
| [MS-OI29500 maxDist 注记](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/4a8c035c-c930-4e87-aabf-32715949ba48) | Word 对无对齐点的数组不扩展内容 | 普通无标记数组不能只因 maxDist 开就改其字符间距 |
| [MS-OE376 objDist 注记](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oe376/30da79e7-70b5-46e3-9df0-66020e249801) | 原文两处写 `maxObj`，并称其关闭时忽略 objDist | 固定 schema 没有 maxObj。本文不把它偷偷改名为 maxDist；该兼容注记存在命名歧义，须保存四组合 Word 实例后裁定 |
| [MathML Core §3.5](https://www.w3.org/TR/mathml-core/#tabular-math)、[MathML 4 固定草案 §3.5.4](https://www.w3.org/TR/2026/WD-mathml4-20261001/#presm.alignment) | Core 用 CSS table 的 `mtable/mtr/mtd`；MathML 4 明确 `maligngroup/malignmark` 不属于 Core，并建议通常改用表格列 | 不能只输出上述扩展标记或旧 columnspacing 属性就认证 Chromium/独立阅读器的几何等价 |

以上链接于本日浏览核对。只引用必要规则，不复制标准图片或长 XML。输入 XML 中的标记写作 `&amp;`；本文的 `&` 指 XML 解码后的字符。

本机固定资产：`docs/design/office-math-coverage.json` SHA256 `4b70f8bf4ab29029b3aa2b64daad456c9a3c92d1fa32eba8bcf2a517cee78e5e`，来源 Open-XML-SDK commit `b424997c2c81410d375a72bf68837b87b7b807e8`、源 SHA256 `237395f1e5cee42e34ab12675251f5c01c212fe0dac5484aa36ffcbdfa58d892`。其中 `aln` 的合法父级是 boxPr/rPr；maxDist/objDist 均为 val 可省略的 OnOff。清单存在性不等于布局支持。

## 2. 当前缺口与可复用实现

| 当前落点 | 实际行为 | 最小修改职责 |
| --- | --- | --- |
| `math_layout.rs::Ctx::table`、`node_context("eqArr")`，约 1462/1657 行 | 每个 e 整体成为一个左对齐单元格；maxDist/objDist/aln 不参与分配；已有 rSp/baseJc 生效 | 数组另走 `equation_array`，复用现有行高、基线和标记追加方式，矩阵支路不改 |
| `Ctx::run/row_parts/hlist`，约 1117/904/1084 行 | `&` 是有墨迹的普通字符；按 run 和结构计算共享数学间距、物理字号与源片段 | 增非绘制标记事件，保留原 run 字形/词识别/字体/间距上下文；不先用字符串 split 再各自重新布局 |
| `Ctx::display_paragraph/display_equation/argument`，约 1047/1028/1113 行 | 只有外层 display 获得扣除 math margins 的宽度；普通参数没有局部宽度传递 | 引入显式局部宽度约束；不从任意嵌套节点直接读取全局 available_width |
| `omml.rs::emit_inner("eqArr")`，约 1163 行 | 每行一个 mtd，原 `&` 仍可见 | 与矢量共用标记语义，按左右半列及伸展列投影；Core 的实际 CSS/浏览器几何单独验证 |
| `math_document::{marked_root,vector_preview_constrained,preview_at_size}` | 现有 readmdId、原始 XML、UTF-16 与预览桥；矢量返回 Option，尚无数组溢出的 typed 结果 | 保留旧成功调用接口；新增有诊断的内部结果并由公用 API 暴露，不能把窄列失败吞成空图 |

审计时 `math_layout.rs` SHA256 `6e661718b03b135ea4a9126f014d703cc07995bf298f23b759cedbc60bfecc0b`；`omml.rs` SHA256 `45e3ee24f44ac8327f8c82acc38e2b69df215c33277f0bef22a1e98e1e95a867`。后续实施必须在 root 解冻后核对差异，不覆盖当前出版改动。

## 3. 语义计划：先识别，再测量，最后分配

建议新增无依赖内部 `math_equation_array.rs`；纯语义计划供 vector/MathML 共用，几何 solver 不读取 XML 或字体。名称为建议，实施时统一确定，不把本文当已存在 API。

- `ArrayPlan { rows, flags, alignment_sources }`；row 含有序 `ColumnPlan { before_anchor, after_anchor, anchor_source }` 和非绘制 `Marker { kind, node_id, utf16_from, utf16_to }`。行首/尾、连续标记和缺组明确保留；二维预算受现有节点/字符预算约束，不能按最大列数盲目填满稀疏巨阵。
- 仅在当前数组行的数学水平序列识别标记，跨 run 延续奇偶位，进入下一 e 重置。嵌套 eqArr 建新作用域；分子、脚标等局部参数中的文字不能被外层 scanner 抽走。透明包装可以传播有来源的显式对齐事件；改变二维结构的对象保持整体。
- `rPr/aln=true` 的 anchor 在该 run 第一个字符之前；`boxPr/aln=true && opEmu=true` 的 anchor 在完整 box 内容之前。false/省略不生效；非 emulator 的 box 不因 aln 改布局。段落内多个 oMath 的显式单锚点还需由 paragraph 层共同测量，不能仅修 eqArr 后宣称 aln 已全消费。
- 对多个显式 aln、aln 与 `&` 混合，计划必须保留每一个来源，不能只取第一个或把显式属性改写成源码字符。标准摘录没有明确其与原始 ampersand 奇偶序号的归并规则；实现前用最小 Word 创建/重存样例固定该规则。纯 `&` 多列与每行单个显式 aln 的实现不依赖此裁决，但混合项未验证前 M04 不得收口。
- `lit/nor` 下字面 ampersand、属性标记与 literal 的相互作用沿 M06 另建最小仲裁夹具；不能为 M04 全局删掉所有 `&`。非数组的普通 ampersand 保留既有字面显示，明确 aln 除外。
- 先建立整行的词识别与共享 spacing plan，再形成可定位片段。标记不参与可见 glyph/class；对齐符号仍用作者原对象和墨迹。二元/关系的行首归一化、diff/phantom、Office 显式物理字号、脚本间距不得因切分改变。

## 4. 纯几何算法与独立数值预期

对列 j，测量每行锚点两侧的逻辑宽 `l[i,j],r[i,j]`；`L[j]=max_i l[i,j]`、`R[j]=max_i r[i,j]`、`C[j]=L[j]+R[j]`。没有奇数锚点的非空最后一组按一整块左对齐，不臆造等号锚点；缺列为空，显式空列保持其索引。墨迹 bbox 另算，不能用 bbox 代替 advance 分配而丢负左边距。

自然宽 `N=sum(C)`，不加硬编码 TeX 列距。已有字符间距属于测得的内容宽，不额外重复加。数组有可用锚点且 maxDist 开、父容器给出有限宽 W≥N 时，分配宽 A=W；否则 A=N。`D=A−N`：

- c≥2、objDist 开：两端额外空白为 0，c−1 个列间额外空白各 `D/(c−1)`。
- objDist 关：两端和 c−1 个列间额外空白各 `D/(c+1)`。
- c=1：不除以零、不拉伸字形。拟定单列作为整体保留共同锚点、在 A 内按既有段落对齐放置；该退化分布是待 Word 样例确认的产品策略，不冒称标准已经给出唯一答案。
- maxDist 关：A=N，以上 D=0，因此 objDist 两值可以有相同实际坐标。objDist 不独立把数组扩成页面宽度。无标记数组继续既有内容几何。

独立 solver 用人工给定的点值，不调用产品布局生成 expected：两行分别 `[(10,12),(8,6)]`、`[(18,4),(5,14)]`，得 L=[18,8]、R=[12,14]、C=[30,22]、N=52。W=100：

| maxDist,objDist | 数组宽 | 左端/列间/右端额外空白 | 列原点 | 公共 anchor x |
| --- | --- | --- | --- | --- |
| 0,0 | 52 | 0/0/0 | 0,30 | 18,38 |
| 0,1 | 52 | 0/0/0 | 0,30 | 18,38 |
| 1,0 | 100 | 16/16/16 | 16,62 | 34,70 |
| 1,1 | 100 | 0/48/0 | 0,78 | 18,86 |

同列每行左片段原点为 `columnOrigin+L−l[i,j]`，右片段从公共 anchor 开始。例如 (1,1) 第一行的首左片段 x=8，第二行 x=0，右片段均 x=18。对上述人工数据可以精确比较；真实浮点只按独立输入误差用窄容差，不能见失败后增容差。

## 5. 局部宽度、窄容器与源映射

按 intrinsic measure → parent allocation → child placement 两阶段传 `Intrinsic | Limit(points)`，必要时缓存本次布局的自然尺寸，不建跨文档隐藏状态。页面/栏/表格单元先扣自己的 margins/padding；水平 siblings、根号/定界符和边框先保留实际占宽，子数组只拿局部剩余宽。上下堆叠对象的子参数按该对象自己的分配宽度工作；脚本不会继承整页宽。嵌套数组以所属列/参数的局部分配为边界，先测内层自然宽再分配外层，不能形成无限“子数组要父宽、父宽又等子宽”迭代。

W<N 时禁止负间距、缩字号、删列、静默剪裁；返回包含数组 id/原 UTF-16 范围、required/available 物理点值的溢出诊断，同时保留自然宽预览与原文供编辑。导出是否阻止写入由统一格式能力合同明确，不能把 Option::None 当作已报告。新稳定 code 与 i18n/API 接线由 root 协调；当前没有声称已有数组 overflow code。无宽度的行内/导入式保持自然宽，不能为满足 maxDist 编造容器尺寸。

非绘制标记仍占原 run 的 UTF-16 长度；各可见片段沿用原 readmdId 与原片段 offset，禁止重编号后套旧点击范围。零宽标记没有墨迹点击面，但可通过源范围编辑/键盘到达。控制字号继承、at_size 重排、SVG/PDF bounds 与 hit regions 采用同一最终坐标。现有 `brk/alnAt` 段落手动换行、自动折行与 box noBreak 回归原样保留；数组自身是有共同列约束的整体，不把列分隔符误用作任意自动换行点。

## 6. MathML 与原文往返

对标记列输出 `mtable` 的成对右对齐/左对齐内容单元格，伸展部分用独立 spacer 列或明确有限 CSS 宽，不把 `&` 绘成 glyph。不依赖 Core 未规定的 malignmark/groupalign/columnspacing。消除默认 mtd padding，并在声明的浏览器环境下实测 CSS table 的最小内容宽、左右对齐和百分比/物理长度；边界数学间距从同一语义计划显式保留，不能让拆 cell 后的 mo spacing 悄悄改变。

原生 MathML 导出分三种结果记录：纯 Core 结构；受控 CSS 与实际容器约束的投影；无法保真时有定位的 projection 诊断。向不执行本产品 CSS 的阅读器输出时，未实测的 maxDist 自适应不能宣称等价。无损 OMML/DSL 源保存仍是原来的 lossless 树，不经可见 MathML 逆造作者属性；原始 maxDist/objDist/aln 拼写和未知命名空间均按现有安全编辑合同保留。

## 7. 最小有意义测试与退出条件

下表是**待实施/待运行**清单，不计入当前通过数；测试名可在实施时保留为过滤器 `equation_array_`。

| 测试组 | 独立 expected / 必须失败的反例 |
| --- | --- |
| `equation_array_distribution_four_combinations` | §4 人工 widths 的四组完整坐标；W=N、无 width、c=1 与 W<N；NaN/负/零 width 走既有输入错误，不能输出 NaN SVG |
| `equation_array_markers_cross_runs_and_keep_empty_groups` | 自有逻辑行 `a&b&&&x&y` → `[(a,b),(空,空),(x,y)]`；`a&b&` 保留末尾空列；同字串任意 run 切分结果一致，行重置、嵌套重置；未标记 `a=b=c` 不自动分三列 |
| `equation_array_explicit_alignment_preserves_objects` | 自有 opEmu box `==` 与另一行 `→`，anchor 对齐而对象完整；opEmu/aln 四组合/空/false；同段落两个 oMath 以 rPr/aln 指向不同前缀后的减号，不能仅首行正确 |
| `equation_array_font_oracle_and_control_sizes` | 固定 FontTools fixture 原 advance/ink 推导坐标：plain a/b/x/y 为 480/522/479/483 units，upem1000；自有两行 a&b&x&y、x&y&b&a 得列宽1002/1005 units。12/24pt、Office18pt、脚本/嵌套分别计算，不写 STIX 数值为产品常量 |
| `equation_array_local_width_and_overflow` | 页面360pt/两栏有效168pt/单元格84pt，同一输入按真正内容宽展开；嵌套 fraction、rad、matrix、box siblings 不得使用整页宽；窄列诊断含正确 id/范围且无墨迹裁切 |
| `equation_array_source_ranges_and_existing_spacing` | 原文 BOM/CRLF/注释/重复数组；标记前 emoji 或 supplementary math 字符的 offset 按 UTF-16，不按 char 数；改第二个数组一次 undo；保存重开原未改子树一致；M01/M02/M03/M05/M06/font-size 既有回归保持 |
| `equation_array_mathml_actual_geometry` | 同候选正式 CSP 下 Chromium 实测 vector 与原生 MathML 的 anchor/列边界、墨迹、字号；各 cell 的空间不能靠隐藏附载 DSL 声称原生支持；独立 MathML 阅读器另计 |
| `equation_array_word_native_roundtrip` | Word 创建/导入四组合、run/box aln、多列空组、单列/混合标记、page/column/cell；改属性→撤销→保存→退出重开→Word 重新编辑；精确记录真实版本与首失败，环境 timeout 不计通过 |

字体 oracle 复用 `rust/readmd-kernel/tests/fixtures/math-font-geometry-stix.json` SHA256 `dfa710a53605ddbab2f1aa21f595c815f0d955d28c675e3d0666d8d2b0a17d65`；真实字体 SHA256 `49b22767985171bce579307b21d98f4d35012fa39e316acc4ffcab8644dfb69b`。fixture 是 FontTools 独立解析的既有材料，不从本引擎 SVG/PDF 反推 golden。

实施顺序：root 先确认单列与显式/字符混合标记的兼容选择及诊断合同；随后一人独占 shared scanner+vector，另一人可在稳定 plan 后接 MathML/API，QA 独立准备手算与 Word 样例。Rust 统一冻结后只由 root 执行 Cargo，再固定同 EXE/boot/font 包做一次原生专项。模型/solver 通过只代表已实现并有库级依据；Word/独立阅读器及保存重开未取得前，不将 M04 或第三批记作完成。
