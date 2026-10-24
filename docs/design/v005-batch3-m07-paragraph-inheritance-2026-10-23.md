# M07 下一有限单元：关联 Word 段落的对齐与物理缩进

**实施更新：有限LTR子集已有真实实现与四路验证，完整本规格矩阵未验收。** 以[本轮报告](../reviews/readmd-v005-batch3-word-paragraph-inheritance-2026-10-23.md)的正例和边界为准；下面的函数位置／“拟议”是实施前审计，当前实现见第10节。

初始设计日期：2026-10-23。原状态为设计待实施；当前有限实现与未验部分按上方更新及第10节区分。本规格依据既有源码、文档和固定离线 schema 审计整理；当前验证中的题注／逐节编号候选不因此改变。可替换 MATH 字体、TeX 原生 OfficeMath 排版、Word 应用重编辑另排。

## 1. 可以交付的正向闭环

交付对象是正文中独立的显示公式 `oMathPara`（包括同段多式），其源上下文来自该公式当前所在的 Word 段落。先落实 `dispDef=false` 时的 LTR `w:jc=left/center/right` 和 `w:ind/@w:left,@w:right` 继承。每个公式保存自己的稀疏声明，输出副本采用算出的有效值。普通裸公式、行内公式和空上下文保持原行为；不得吸收前一公式或文档中任意同标题公式的设置。

首单元渲染域：left/right 使用整数 twip，1 twip = 1/20 pt；正向范围 **0..31680 twip**，显式 `0` 是有效覆盖。此上限是拟定的有限消费者域，复用当前数学布局可控范围，**不是 Word `ST_SignedTwipsMeasure` 的完整合法域**。固定 Word schema 的 left/right 使用 signed measure，Office 2010 还有 `mm/cm/in/pt/pc/pi` 联合字面量；负值、超过本域的值和带单位字面量须保留为源信息，但首单元消费者准确报告尚不支持，不转成 0、不套 unsigned 数学 schema。之后可独立扩展有符号越界/悬出布局。

必须实际跑通 native DOCX → 当前转换原文 → 共享迁移 → 可读源定点事务 → revision 保存 → 正常退出 → **不同 PID、同 profile 重开** → 每个目标请求 `options:{}` 输出 → DOCX 再导入/迁移。DOCX、Reader/预览共享矢量、Rust PDF、受管 Typst PDF、HTML/EPUB MathML 分别记录正例；不能以拒绝、生成字符串或 warning 代替这些正例。TeX 当前是 LaTeX 投影，不在本闭环声称继承成功。

## 2. 核对依据与真实缺口

固定 Word schema：Open XML SDK commit `b424997c2c81410d375a72bf68837b87b7b807e8`；缓存 `publication-structure-inputs-20261015/p04-tracking-docx-draft-20261016/primary-schema/wordprocessingml-fixed.json`，1,844,603 bytes，SHA256 `05a5be64625cd79e756b27606587c3a2868f2e42ade196bfca2d6f647e74fdb5`。同目录 `root-fetch.json` 和 `tracking-schema-audit.json` 记录原地址、MIT 许可和固定提交。没有重新联网。

- `w:CT_Jc/w:jc` 的 `w:val` 必需。`ST_Jc` 含 left/start/center/right/end/both/mediumKashida/distribute/numTab/highKashida/lowKashida/thaiDistribute；不能交给当前 `MathJustification::parse` 将未知 Word 值折成 centerGroup。
- `w:CT_Ind/w:ind` 的各属性独立可缺省；空 ind 不是清零。physical left/right 与 logical start/end、字符单位和 firstLine/hanging 是不同语义。
- `docDefaults` 顺序为 rPrDefault、pPrDefault；pPrDefault 内是 pPr；直接 pPr 支持 pStyle，而 style/default 的 pPr 类型不含 pStyle。本子集写出次序：直接 pStyle（若保留）在前，ind 在 jc 前；输出没有合法目标 style 定义时移除 pStyle，改写有效直接值，不能留下悬空 ID。
- schema 校验必须基于 namespace URI 和具体父容器类型；固定数学 schema 仍独立，不扩张其 133 元素/12 枚举认证范围。Word 本次仅固定选中的粒子和叶子校验，不能宣称完整 Word schema 验证。

当前函数核对：

| 位置 | 已存在行为 / 缺口 |
| --- | --- |
| `convert.rs:2458 docx_math_metadata` | 已从**当前** paragraph 获取 pPr；styles/settings 由实际关系目标读取，支持规范化后的别名。生成保真载荷前调用共享解析。 |
| `math_word.rs:79 WordMathStyles::capture` | 保留 defaults 和实际引用链；但没有 styles 根直接返回空，subset 空也返回空，导致独立 direct pPr 丢失。 |
| `math_word.rs:186 materialize` | rPr 文档默认 → paragraph basedOn → character basedOn → direct 已实现。无 styles/theme/fonts 的早退绕过 pPr 重复检查；结束删 styles/pPr，未传递有效段落布局。 |
| `omml.rs:189 math_paragraph_style` | 只读局部 math jc 与 mathPr；未读取有效 Word jc/ind。`margins()` 的大边距回退是 Office **数学边距**规则，不能未经证据套到 Word 缩进。 |
| `omml.rs:399 docx_math_fragment_tree` | 已合法局部化数学 jc/上下限并分离 settings；返回 `(content, settings)`，无外层段落投影。 |
| `math_defaults.rs:288 DocxDefaultsMerge::accept` | 同一 DOCX 的全局 mathPr 合并/冲突保护已存在，不能引入 Word 每式缩进后取消。 |
| `docx_writer.rs:1195 Block::OfficeMath` | 外层 pPr 无条件追加 center；真实继承必须替换这项决定，不能把 w:pPr 塞进 m:oMathPara。 |
| `math_layout.rs:1150 display_paragraph` | 消费共享段落 style，但 explicit_margins 会构造全宽 frame；需要区分 Word box 与数学 margin，防止重复偏移/对齐。 |
| `math_dsl.rs:805 compile` / `math_document.rs:554 SetAttribute` | DSL 已有 Word 节点精确 UTF-16 source nodes；但 SetAttribute 只放行数学节点。不能宣称已有 Word 属性事务。 |

`docs/design/math-dsl-v1.md:53` 已规定最小 Word 上下文与可读源码；`v005-batch3-next-execution-2026-10-21.md:43` 明确：**dispDef=false 关闭数学对齐/边距/折行覆盖，不关闭 preSp/postSp/interSp/intraSp**。早期 review 摘要的相反措辞不能用于新实现。旧 M07 paragraph/style 缓存只是基线，不是本新增继承的通过证据。

## 3. 稀疏声明、优先级与源表达

Word 段落有效属性依次：

1. `styles/docDefaults/pPrDefault/pPr`。
2. **所选** paragraph style 的 basedOn 祖先到子项；没有直接 pStyle 时才选择 paragraph default。不能先无条件叠 default 再叠一个无 basedOn 关系的显式 style。
3. 当前段落 direct pPr。

jc 单项覆盖；ind **按属性**合并。高层仅声明 right，不会清除低层 left。bidi/mirror 等开关用于 guard 时是段落绝对覆盖，direct false 必须能关闭低层 true，不能复用 run bold 的 toggle。character style 和 paragraph-mark rPr 不决定段落 jc/ind，也不把标记 rPr 套到正文。

最终显示公式选择：局部 `oMathParaPr/jc` 优先；dispDef enabled 时沿用已有 mathPr/default 规则；dispDef false 时没有局部 jc 才取有效 Word jc。左右缩进在 dispDef false 时取有效 Word ind；局部 math jc 只覆盖对齐，不屏蔽 Word ind。四项数学间距仍按现有 mathPr 生效，不与未实现的 Word spacing 相加。对完全缺省的轴保留已有内核/目标格式基准，不能把缺省冒充显式 0。

| 源状态 | 有效语义 / 清除 |
| --- | --- |
| 无 pPr / 无 ind / 缺少某 ind 属性 | 该层不覆盖，继续继承；最终未声明仍有 Missing provenance。 |
| `word.pPr()`、`word.ind()` | 合法空容器，不重置继承；源码必须保留这种显式 presence。 |
| `word.ind(left: "0")` | 覆盖有效 left=0，right 仍独立继承；输出不能因等于缺省而省掉显式状态的源信息。 |
| `word.jc(val: "left")` | 有效明确左对齐；即使等于既有基准，也不把源码 declaration 折掉。 |
| `word.jc()`、`word.pStyle()`、空/未知 jc val | 必需值缺失/非法，`math_invalid_property`；不采用数学 jc 空元素隐式 centerGroup。 |
| 删除 direct jc 或 ind 的某属性 | 恢复继承，不写默认/0；删除整个 ind 也只恢复继承。 |

可读源沿现有语法，无新 frontmatter/私有 XML 属性：

```text
row(
  word.styles(
    word.docDefaults(word.pPrDefault(word.pPr(word.ind(left: "240", right: "120"), word.jc(val: "left")))),
    word.style(word.pPr(word.ind(left: "480")), type: "paragraph", styleId: "Base"),
    word.style(word.basedOn(val: "Base"), word.pPr(word.ind(right: "360"), word.jc(val: "right")), type: "paragraph", styleId: "Equation")
  ),
  word.pPr(word.pStyle(val: "Equation"), word.ind(left: "0")),
  office.mathPr(office.dispDef(val: "0"), office.preSp(val: "120"), office.postSp(val: "240")),
  office.oMathPara(office.oMath(frac(a, b)))
)
```

此例最终 L=0、R=18pt、jc=right；删除 direct left 属性后 L=24pt。没有 pStyle 时采用实际 default paragraph style；显式指向失踪 ID 不能偷偷切回 default。源中原始整数拼写、空容器、注释和无关兄弟保留，有效输出才归一化为整数值。

## 4. 精确范围与拒绝合同

第一单元的 positive body scope：顶层显示块、一份 associated pPr、其被引用段落链；同一个 oMathPara 中多式共用该段上下文。行内 oMath 仍可保存相关上下文，但不拿它移动整个宿主段落；显示载体/inline 伪装校验沿用。表格/文本框/批注/页眉等宿主特定对齐、编号/首行悬挂、混合正文与显示式同段的定位后续单独验证，不泛化为已支持。

下表区分无效输入与有效但未消费的语义。source XML/DSL 可以保留有效的未支持声明；可读保存/恢复本身不代表该出口支持。消费者遇到会影响其有效结果而没有实现的项，要在资源访问/写目标前给带公式范围和字段的 capability 错误，不能成功后默认居中。已被更高层同轴合法值覆盖的 lower-level 声明保留，不误报为仍有效；语法无效永远不能被遮蔽。

| 类别 | 首单元行为 |
| --- | --- |
| jc left/center/right；left/right 整数 0..31680；空容器；guard false | 必须正向消费、输出和物理验证。 |
| jc start/end、both/distribute/numTab、Kashida/Thai 枚举 | schema 可能有效，本单元不转义成 left/right/center；有效命中时报未支持。 |
| 负/过域 physical 缩进、单位字符串；start/end、*Chars、firstLine/hanging | 保存原值；有效命中时报未支持，不丢值、不 clamp、不误称 schema 非法。firstLine/hanging 本单元整体不认证，即使为0也不能未经跨属性优先级证明就称中性。 |
| 有效 bidi=true、mirrorIndents=true、非水平 textDirection、framePr/numPr 等改变宿主几何 | 首单元未支持；false/省略本身不阻止 LTR 正例。不能只检查 direct 而漏继承。 |
| Word paragraph spacing/line/grid/tabs、flow、revision 等其余布局字段 | 不是本单元已支持属性。若出口无法保留/消费其有效语义，准确拒绝该组合或沿已存在明确损失合同报告，不以本单元放宽为成功保真；无单独消费证据不可销账。 |
| name/aliases/UI catalog 标签、paragraph-mark rPr | 保留信息；不拿作几何轴或绘制权限；不为不影响本子集的目录标签臆造错误。 |
| 重复语义 pPr/jc/ind/同 URI 属性、错误位置/顺序、必需值缺失、错误 URI、坏整数 | `math_invalid_property` / XML 错误；不按前缀授信，不吞第一个值。 |
| duplicate/default style、循环、缺被引用项、错误 style type、链超128 | 沿现有准确错误/budget 保护；不会改为默认样式。 |

拟议 capability code `math_word_paragraph_unsupported`（需实现时统一错误合同），详细字段通过既有动态错误/diagnostic 通路传递，不把 JSON 拼进静态 code。parse/保存和 render/native export 的能力判断分开；不得让“保留可读源”因渲染缺口退化成丢弃上下文，也不得让消费者悄悄忽略它。新 preflight 必须覆盖新 DSL、旧 capsule 和实际转换来源。

轴数值合法不保证任意容器宽度可布局：已知实际父宽 A 时，`A-L-R<=0` 必须在开始该公式布局/引擎绘制前准确返回范围错误，不能借用数学 margin 的一英寸回退或把 left 清零。已有窄宽自动折行可继续使用同一有效 inner box，但完整 Word wrapping 不在本单元认证；自然宽预览没有 A 时不臆造页宽，保留自然内容加两边距的行为。这类纯几何检查在可用宽确定时执行，不探测字体/公式/引擎资源来决定支持。

## 5. 最小产品改动面（初始设计，当前实现差异见第10节）

1. `math_word.rs` 增加一个小型 `WordParagraphContext`：稀疏 declaration/presence、有效 jc/left/right、每轴 provenance、有效 guard/unsupported 详情。`paragraph_context(&XmlNode)` 复用当前 styles ID/chain/namespace helpers，独立于 rPr 计算；构造捕获副本前检查重复容器。capture 无 styles 或空 subset 时仍带 direct pPr；不复制整份 styles.xml。实际 used chains 和 budgets 不扩大。
2. `materialize` 返回/传出此 context，仍只在工作副本 materialize rPr，然后移除内部 styles/pPr/theme。`omml::parse` 把 context 绑定到解析根的内部 typed 字段（可用小 Arc）；保留 formatted-tree/clone 路径。不得在作者 OMML 写私有属性，不用字符串回扫找已删除的 pPr。语法错误立即返回；未支持语义可保留 typed diagnostic，由真实消费者 preflight 决定。
3. `MathParagraphStyle` 增加有效值来源/Word box 判别，不把 Word 缩进走数学 `margins()` 回退。`math_paragraph_style` 统一取本单元优先级；无 Word 声明时路径不变。`math_layout`、SVG/Reader、Rust/Typst vector 与 MathML 共用一次决定，避免 Word frame 被外层再次 center/right。仅显式右轴不应虚构显式左轴；有效 left/right 与 declaration presence 分开。
4. `docx_math_fragment_tree` 增加结构化投影 `ProjectedMath { content, settings, paragraph }`；`paragraph` 是受限有效外层 pPr patch，不能混进 OMML body。`DocxDefaultsMerge` 继续处理单一 settings 与原冲突，Word 各式值不加入 global signature；可增加 display accept 接口而保留 inline 字符串调用。`docx_writer::office_math/Block::OfficeMath` 以合法次序合并 scope pPr、有效 jc/ind，去掉无条件 center；相同 child 不能追加两份。无对应 style 定义时写有效直接值、不留 pStyle；源码不受此 materialize 影响。
5. `math_document::SetAttribute` 最小扩充只允许真实 WORD_NS 的 jc/@val、ind/@left/@right 和验证所需 guard；不泛开所有 Word 属性。jc 必需 val 不可用 None 产生非法空 leaf；恢复继承用精确 parent ReplaceSource 删除 leaf。ind 属性 None 则恢复该轴继承。添加原来不存在的 pPr/jc 用当前 source-node 范围的 ReplaceSource/现有文档事务，无需发明新的顶层模型。保留整树语义等价证明、最小可表达祖先重写和 unsupported 保留 draft。
6. 在 prepare、资源解析、字体/公式 callback、engine lease、输出临时件/目标创建之前跑纯 resolver/consumer guard；direct JSON/vector 和源出口走同一规则。沿现有取消、预算和 atomic commit 失败保护。不能用 render probe 来判支持。

源身份：数学 `MathDocument.revision` 绑定**当前公式源码**；文档 save 的 base_revision 绑定**当前完整文档字节**。from/to 是 UTF-16、来自这一份源码的 SourceNode/MarkdownRange；不按公式文本找第二次出现、不拿迁移后范围回写迁移前文本。BOM、CRLF、astral 字符前缀、注释、三处相同公式纳入事务回归。SetAttribute 处理别名前缀时比较 URI+local，保留其他 namespace 同名属性的源码，不能授信伪 URI。

当前 budgets 保持：DSL 64KiB、深度96、token12000；lossless XML 1MiB、节点12000、深度128、名字512B/URI4096B，namespace 声明/解析已有上限；style 链128。`math_document` 的 LaTeX 文本编辑局部16,000B限制不能误写成所有 DSL 的上限。不得以加大预算绕过本单元限制。

## 6. 当前原生值优先及 DOCX 有效值回写

DOCX 每次导入从 actual current document paragraph、实际 related styles/settings 重新求值；旧辅助 source 只用于身份/恢复，不能把旧 pPr 或旧 style 值覆盖当前可见 body。夹具必须独立改动输出 DOCX 的 current direct jc、ind、referenced style pPr，再导入；第二轮保留不变 OMML text/旧载荷，使“旧胶囊仍当前”不能成为漏测理由。显式删除 native override 后继承恢复，不能被旧值复活。

输出 DOCX 仍是一份合法全局 mathPr。两式同 dispDef=false、相同全局数学默认、不同 Word jc/ind：**必须可共存、逆序结果不变**。一式 true、一式 false，或不同不能局部化的全局策略：沿既有 `math_document_defaults_conflict`，前/反序都保护旧目标。不能为了本单元放宽为采用第一份 mathPr。

原生有效值策略是输出 pPr 写当时的 jc/ind（有限轴），不是声称导出整份原 style catalog。再导入的 readable declaration 可能由 inherited 变为 direct，但每式有效值、合法位置、可再编辑性一致。初始可读源的 sparse presence 则必须 byte/semantic 保持，不允许为 writer convenience normalize 整个源。普通/空 context 不平白生成 ind=0；纯 writer 默认模板差异要作为未显式轴边界记录。

## 7. 验收矩阵与独立 oracle

所有新增产品实测须绑定新候选 EXE、boot、完整 source freeze、driver 和资源哈希。旧 cached pass 与本次只读设计不能充当通过。无 GUI 方案复用真实 owned MCP/HTTP host、受管离线引擎与独立 ZIP/XML/PDF 检查；不执行下载。

### 7.1 真实生命周期

- fixture A：没有 styles relationship，仅 direct pPr left=720/right=180/jc=right；fixture B：alternate styles part + namespace aliases，docDefaults → Base → Equation，direct 只覆盖单轴；fixture C：空 styles/empty pPr/empty ind、显式0和删除。三份原生输入均预先 hash。
- 每份 native DOCX 都由独立 ZIP/XML 生成/检查；先 `convert preview`，确认输入不变、无额外 MD/目录/资源，再保存**原转换返回文本**；调用现有 math migration 得 readable source，并证明语义一致、幂等及逆事务恢复。
- 三处内容完全相同的显示公式，通过当前文档 UTF-16 source range + 目标 SourceNode/ID 只改第二处 jc/left；验证另外两段和非公式字节原样，undo/redo 与 stale revision 冲突；不用 replace(title,text)。
- 真正 revision save；正常 EOF/host shutdown exit0，记录 PID；同 profile 新 PID 重开读磁盘。输出请求参数精确记录 `options:{}`，基于文档显式合法 profile/既有默认选真正消费者，不在请求里塞测试对齐/字号。Typst 若需要文档选择由已保存源/profile 明示，仍空请求 options。
- 输出 DOCX 解包：公式数量/故事身份/段落分组不变；恰一全局 mathPr；w:pPr 在 w:p 首个合法位置，ind 在 jc 前，无重复 jc/ind，无内部 context、悬空 pStyle、图片/静态公式冒充。再真实 convert→migration→重开输出，逐式有效值稳定。Reader/HTML/EPUB 与两 PDF 记录实际消费者证据；没有运行的目标仍未验收。

### 7.2 值与优先级

必须覆盖：defaults-only、basedOn 两层+direct、default 与显式 unrelated style、每轴独立继承、empty/missing/explicit0、clear 恢复；无 styles direct 的 current bug；局部 math jc 覆盖 Word jc 而 ind 仍生效；dispDef=false/true 分开正例、enabled 原有规则不变；pre/post/inter/intra 四值物理保持；两来源逆序+裸公式隔离。运行 rPr toggle、字符样式、ctrlPr、group alignment/manual break 的旧回归不得删除。

### 7.3 物理位置（不是仅比 XML）

用足够宽、无需自动折行的相同简式，固定受管现有字体/字号，当前 host content box 左边 x0、宽 A，原式自然 ink 宽 W。L=left/20、R=right/20、B=A-L-R，要求 B>W：

- left：公式参考原点 `x0+L`；right：`x0+A-R-W`；center：`x0+L+(B-W)/2`。
- 同一简式、同字号，用差量去掉 glyph side bearing：left 改 L=0→36pt，所有 ink/锚区 x 平移 +36pt；right 改 R=0→18pt 平移 -18pt；center 改 L+24/R+12，平移 +6pt。其余式和字形宽高不变。若 W 是 ink 而 layout 使用 advance，先独立取 baseline 的 origin/advance 并记录 side bearing，不能用错 W 后加大容差。
- Reader/SVG 查真实 ink 路径/regions 与宿主边框；PDF 用独立解析的 text/path bounds + 实际 page/media/crop、vector 绘制坐标。Rust/Typst 都按各自页面实际 A/x0 计算，不假设模板相同。先换坐标单位/页面方向，再用有依据的 ≤0.25pt vector/≤1px DOM 容差；禁止任意宽容差或只看 `data-*`。
- 带局部 jc 的多式段，确认相同 paragraph box 与行间距；四项 spacing 的 before/after、段内/相邻段差量独立验证，不能重复加 Word spacing。显式0不强制造成额外全宽 frame/重复center。MathML 检查实际结构样式及独立 reparse；EPUB 包内样式不等于独立阅读器几何认证。
- 反例 oracle：错页、邻式、三处同文中错第二处、只改 wrapper 无 ink 位移、单位错20倍、重复居中、丢 right 继承、clear 回旧胶囊，均必须失败。原生 Word 应用实际排版目前无运行证据，独立保留为未验收。

### 7.4 失败与无污染

结构错误、必需值缺失、错误 URI/属性别名、重复 jc/ind、cycle/missing/type mismatch/链超限；有效超范围/unsupported guards；混合全局 mathPr 冲突；旧 capsule 与 new DSL 同规则；stale source/文档 revision、取消/预算逐一真实请求。预置目标文件+相邻 sentinel，hash 文件与目录树；失败 callback=0/无 engine lease/不解析坏 citation/asset，旧目标字节和输入不变，临时件/缓存无新残留。正常退出 cleanup 与失败保留诊断分别记录，首次失败不覆盖。

## 8. 完成边界

本单元完成需要前述真实 positive lifecycle、DOCX/reimport、源身份/三态、实际几何和失败保护同时通过；单纯拒绝 unsupported、Rust unit pass、schema 字符串或已有 M07 report 都不足。通过时也只增加“关联段落有限 LTR jc/physical ind 继承”证据，不标整个 M07、全部 Word 段落、MATH 字体选择、TeX 原生数学、Office 应用互操作或整个第三批完成。当前报告与旧首轮失败保持原样。

## 9. 集成前约束

- 先完成纯 resolver、捕获缺口与独立单测，再接实际消费者；不为内部 context 改写公开 XmlNode 或 MathDocument 的 JSON 形状。可以增加内部 parse-with-context 接口，旧 parse 入口保持兼容。正式选定接口前列出所有调用点，不以一处渲染成功代替完整接入。
- 当前源、渲染副本和 DOCX 外层 pPr 是三种职责。原始作者源保持稀疏；有效值只进入工作副本／结构化输出，禁止把当前公式的段落属性全局套到文档。
- 诊断使用共享 API 错误合同；能复用现有本地化提示时不新增实体。确需新错误码或文案时，注册前端、后端及46语言消息，并测试动态定位、长文本和读屏名称。不能使用英文兜底或仅补键值冒充语义审校。
- 上一增量的混合负载超时、外部复制／转换差异和真实应用缺证分别保留，不随此单元的实现变更完成状态。当前编译与真实候选队列结束前，只能在缓存准备提案，不能修改冻结产品输入。

## 10. 本轮实施证据

candidate-53：独立math_word/paragraph解析器、MathDocument精确属性事务、内部关联上下文、DocxMathProjection、共享预检及各消费者已接入。materialize只清理工作副本，缺少styles也移除根级上下文，直接运行属性原插槽保留。native ReadMDMath隔离与普通正文差分测试保留。source_only返回及三个前端消费者已重打包。

本轮三fixture原生→迁移→只改第二式→inverse/stale→保存→新PID→五出口→原生再导入通过；另jc-only/source-only12项、current-native-only三case及旧math/section94项回归有实际证据。并非原规格§6旧辅助源冲突、§7全失败／取消矩阵、任意宿主或实际Word应用已全部通过。

[最终报告](../reviews/readmd-v005-batch3-word-paragraph-inheritance-2026-10-23.md)与[归档](../evidence/capability-upgrade/2026-10-23/batch3-word-paragraph-scoped/manifest.json)记录首次失败、精确候选和未完成项。
