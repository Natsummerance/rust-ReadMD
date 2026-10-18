# 第三批 P04：普通文字与段落参数增量规格

2026-10-15，产品只读审计及下一单元执行规格；另有未应用、未编译的隔离 DOCX 行距草案。**产品尚未实施新参数，也不表示 P04 或第三批完成。** 当前候选、既有失败和 2026-10-14 档案保持原样；以下“应输出”均是待实现、待实际输出验证的合同。

本单元先补固定／最小行距、字距、基线偏移和普通段落底纹边框，再补通用制表位。水平伸缩、复杂文字整形和完整字体嵌入另列后续，不顺便升级引擎、增加字体／crate 或重做已有上下标、CSL、目录及分页控件。

## 1. 当前真实落点与缺口

| 现有入口／函数 | 已有能力，继续复用 | P04 尚缺 |
| --- | --- | --- |
| `export_styles::{default_style,sanitize}`；`assets/js/features/export.js` 的排版区 | 正文 8–20pt、倍数 `typography.lineHeight` 1–2.5、段后距 0–30pt、首行缩进 0–30mm、字体／颜色／对齐；可选 widow／keepLines | 固定／最小行距及下面的新字段；不要借本单元改变旧范围和默认值 |
| `publication_blocks::{validate_style,apply,Resolved::for_context,view}`；`assets/js/reader/publication.js` 局部样式弹窗 | `text.size` 6–72pt、font/color/bold/italic；段前后距 pt、左右／首行／悬挂 mm、倍数 `paragraph.lineHeight` 1–3；keepNext/keepLines/widow/pageBreak；修订校验、UTF-16 事务、null 删除局部覆盖 | 校验器目前拒绝字距、任意基线、固定行距、底纹边框和自定义 tabs；不能只加 UI 字段 |
| `docx_writer::styles_xml`、`docx_local_style::apply` | 正文／局部 `w:spacing lineRule="auto"`；真实 run 格式；代码／引用框有专用 shd/pBdr | 普通段落的新参数无消费者。`P_ORDER/R_ORDER` 出现 shd/pBdr/tabs/spacing/position 名称只是排序能力 |
| `docx_writer::{run_xml,build}` | 源 U+0009 可写成 `w:tab`；默认 tab 为 420 twips；TOC 和简单 CSL 有专用 tab | 普通正文的可配置 tab 集合、清除继承及 leader／对齐没有模型，不能复用 TOC 参数冒充 |
| `md_ast::push_blocks_json` → `pdf_render::{PStyle,TextStyle,Atom,paragraph_lines}` | 基础 PDF 有全局字号／leading、富引用真实上／下标及下划线、代码／callout 背景 | **`Block::Styled` 当前直接剥去块属性**；原有局部块样式也须先接通。`TextStyle::vertical` 的 Sup/Sub 不是任意物理基线偏移；无普通 tracking、border、tabs |
| `typst_writer::Writer::blocks` 的 Styled 分支 | 消费 Resolved，原生文字及段落参数；受管字面和回退；当前文字 strut 与连续栏 helper | `par.leading` 不是固定基线距离；要计算 text 边缘和行内物体，不能只改一个 leading 常数 |
| `latex_writer::{shared_text_settings,Writer::block}`、`mdexport::export_tex_with_resources` | 最新修正已接共享前置模式／全局样式／节；安全字体路径和物理 SC；局部真正支持的字体仍只有固定 Termes，其他局部视觉请求会告警 | 固定／最小行距及多数通用局部段落样式未完整消费；不能因传入 style 就宣称全部生效 |
| `epub_writer::{profile_css,Writer::block}`、`mdexport::build_epub_css` | 流式倍数行距、局部 CSS；代码／引用框装饰、CSL 专用列结构 | 普通段落参数尚缺；CSS 固定 line-height、tab-size、font-stretch 各有明确语义边界 |

已有段落分页控制见 `v005-batch3-publication-paragraph-flow-spec.md`，不在此重复实施。P03 的节模型当前不接受文字／段落补丁；本增量先做全局与源块两层，在已解析的节上下文中继承。需要节级文字覆盖时另扩 P03 schema，不能把未声明字段塞进 section JSON。

## 2. 最小共享合同

新增独立 `publication_typography.rs`，只负责强类型、稀疏覆盖、单位及格式能力；`publication_blocks::Resolved` 消费它。各 writer 不再分别解释原始 JSON。

```rust
enum LineSpacing { Multiple(f64), ExactPt(f64), AtLeastPt(f64) }
enum Decoration<T> { None, Value(T) }
struct TextPatch { tracking_pt: Option<f64>, baseline_shift_pt: Option<f64>, scale_x_pct: Option<u16> }
// ParagraphPatch: line_spacing / fill / border / tabs，均为 Option；不替代旧字段。
```

| 新全局路径 → 局部路径 | 数据与边界（待实现） | 缺省／显式复位 |
| --- | --- | --- |
| `typography.lineSpacing` → `paragraph.lineSpacing` | tagged union：`{"mode":"multiple","multiple":1.5}`（全局 1–2.5、局部 1–3）；或 `{"mode":"exact","pt":14}`／`{"mode":"atLeast","pt":14}`（6–144pt）；只允许对应分支的字段 | 缺省走**该上下文原有** `lineHeight`；`multiple:1` 是显式单倍，null 是继承，不能混为一谈 |
| `typography.trackingPt` → `text.trackingPt` | 有限数，−5–20pt | 缺省无新覆盖；0 明确清除父层附加字距 |
| `typography.baselineShiftPt` → `text.baselineShiftPt` | 有限数，−72–72pt；**正值上移**，不改变字号 | 缺省继承；0 明确回到正常基线 |
| `typography.paragraphFill` → `paragraph.fill` | `"#rrggbb"` 或 `"none"`，不接任意 CSS／颜色表达式 | 缺省继承；`"none"` 明确取消底纹 |
| `typography.paragraphBorder` → `paragraph.border` | `"none"` 或 `{sides:["top","right","bottom","left"],widthPt:1,color:"#334455",gapPt:2}`；side 非空、无重复；宽 0.125–8pt，gap 为 0–31 的整数 pt；首版仅实线 | 缺省继承；`"none"` 取消，不用 width=0 或空对象猜测 |
| `typography.tabs` → `paragraph.tabs` | `{stops:[{positionPt:72,align:"left",leader:"none"}]}`；最多 32 个，位置递增不重复，0–1440pt；align 为 left/center/right/decimal，leader 为 none/dot/hyphen/underscore | 缺省继承；`stops:[]` 清除继承的**自定义** stops，仍保留文档默认及原生悬挂缩进语义 |
| `typography.defaultTabPt`（仅全局） | 1–144pt；首个显式 tab 模型缺省取既有 DOCX 420 twips = 21pt | 没有新 tab 设置时保留各出口旧行为，不把所有源 tab 的旧布局一并迁移 |
| `typography.scaleXPct` → `text.scaleXPct`（后续增量） | 50–200 的整数；物理横向伸缩，y／字号不变 | 缺省不覆盖，100 复位；**不是**选择一个宽体字面 |

物理 pt 统一为 1/72 inch，mm 仍为既有缩进单位，不改老数据。DOCX 行距／tracking／tab 到 twips（20/pt），position 到 half-points（2/pt），边框宽到 eighth-points（8/pt）；记录实际量化值，不能声称无限精度。TeX 新物理字段使用 `bp` 或精确等价换算，不能把 1/72.27 inch 的 TeX `pt` 当作 PDF point。测试必须包含非整数及量化边界；不悄悄钳位到范围内。

稀疏规则固定如下：更新请求缺键表示不改；patch 的 null 删除本层覆盖；持久 source 不写 null；0/false/`"none"`/空 stops 必须保存为显式值。删除后重新从父层解析，不能把旧 resolved 值复制回源码。旧 `lineHeight` 保留为倍数别名：同层仅有旧字段时完全保持旧行为；新字段优先于**继承来的**旧值，同层显式同时提供两个行距字段则拒绝矛盾，不能按 JSON 遍历顺序决定。反过来，局部显式旧 `paragraph.lineHeight` 必须覆盖全局新 `typography.lineSpacing`，恢复为该局部倍数；不能让字段的新旧次序盖过层级优先级。全局冲突校验必须读原稀疏请求，不能把 sanitize 补出的默认 `lineHeight` 当作用户冲突。

新全局文字字段沿用字体层的继承范围；全局段落装饰／tabs 先作用于普通正文。标题、题注、代码、表格、脚注和 CSL 保留自身既有默认及专用布局，只有明确的相应局部覆盖才改变。已有 RichFormat 上下标／强调／链接和 Office Math 的物理字号继续独立，不能被一次块级重排抹平。

继续使用可读声明，不增加新的语法工作台：

```markdown
<!-- readmd:layout v1 {"id":"p04-sample","text":{"trackingPt":1,"baselineShiftPt":0},"paragraph":{"lineSpacing":{"mode":"exact","pt":18},"fill":"#eef4ff","border":{"sides":["left"],"widthPt":1,"color":"#334455","gapPt":2}}} -->

原段落、链接、中文和公式仍是普通 Markdown。
```

沿用 16KiB 单声明／现有节点与文档预算；拒绝未知新字段、非有限数、坏颜色、重复 stops、非法组合和有效容器外的 tab 几何。不要先经过补默认／钳位的 sanitize 才验证新原始值。

## 3. 先确定物理语义，再接格式

**行距。** Multiple 保留各引擎的普通行距单位；比较同引擎倍率，不保证不同字体／引擎单倍天然相同。Exact 请求固定行距；AtLeast 是不小于给定物理值、遇到较高行内内容允许扩大。共享 resolver 保留模式，行布局阶段再使用真实 ascent/depth／行内公式和图片尺寸，不能在字号解析时把 AtLeast 简化成常数。DOCX 的 auto 为 240ths of a line，exact/atLeast 的 line 为 twips，见 [Microsoft Line／LineRule](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.spacingbetweenlines.line?view=openxml-3.0.1)。

Exact 与高字形／公式不能静默改成 AtLeast、缩小内容或裁掉公式。我们控制的 PDF 引擎若不能在指定距离保全内容，须在原子替换前给出定位明确的几何失败。DOCX 可写原生 exact，但 Word 的实际溢出／裁切及未知宿主字体仍需原生验证，不能用其它 PDF 引擎的画面认证；必要风险告警保留。

**字距／偏移。** Tracking 是相邻文字排版单元之间的附加距离，不能按 UTF-16 code unit 或 Rust char 给组合符号、ZWJ／变体选择符分别加空白。首增量在受管 Latin/CJK 的可证实范围实施；复杂连接文字依赖 M08，缺整形能力时显式标未支持，不拆字伪造。字形路径、ToUnicode 与原文本不变，不插空格“实现”字距。[CSS Text](https://www.w3.org/TR/css-text-3/#letter-spacing-property) 也以排版单元定义 tracking，要求保留必要连接。

BaselineShift 不缩字号；与已有语义 Sup/Sub 分层。其组合必须保留原上下标语义并逐引擎验证；DOCX 若 position 与 vertAlign 的实际组合不能证明，保留未验证／能力诊断，不用替换成普通字认证。数学内部布局不消费普通文字 tracking/scale/shift。DOCX position 使用 half-points，见 [Microsoft Position](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.position?view=openxml-3.0.1)。

**装饰／tab。** 底纹、边框属于段落容器而非文字背景；gap 计入可用宽，stroke 按实际边缘计算，不能靠 unbreakable table/minipage 包住跨页段落。分页后各片段保全内容，首片段 top／末片段 bottom，中间侧边连续；Word 相邻原生段落边框的合并行为单独核验。Tab 位置以段落有效容器的起始边缘为基准，不能误用整张纸；LTR 先实现并认证，RTL 的起始边缘、decimal separator 和 bidi 另有能力门槛，不能从 UI 语言猜测。DOCX tab 集合是可叠加的，清除继承必须输出对应 `clear` 项，而不是空 `<w:tabs/>`，见 [Microsoft Tabs](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.tabs?view=openxml-3.0.1)。

## 4. 各格式的真实实现范围

下表是实现目标及限制，**不是通过记录**。固定物理布局不能表达时，分页导出失败保旧文件；流式出口保留全文与源参数，明确发出格式损失告警，不能将近似 CSS 标成精确实现。新告警如需新 code，先由 root 注册并完成 46 语言，再开放控件；writer 不私造未本地化的错误字符串。

| 出口 | 第一增量可落地的原生路径 | 必须单列的边界 |
| --- | --- | --- |
| DOCX | 全局样式及 `docx_local_style::apply`：pPr spacing 的 auto/exact/atLeast；rPr spacing/position；pPr shd/pBdr/tabs；真实 w:tab | 行距不能被文档网格抢占，检查 snapToGrid 的显式设置及 sectPr docGrid；已有 XML 中出现 linePitch 不等于实际启用了网格。量化、边框相邻合并、实际字体与 Word 打开／保存仍需真应用验证 |
| Rust PDF | 先保留 JSON 块样式；PStyle 保存行距模式和装饰，TextStyle/Atom 保存实际 tracking/rise；layout 与 draw 使用同一 advance；按实际行片段绘制 fill/stroke | 不再以 `Block::Styled` 扁平化后“输出成功”计通过；基础 PDF 的 P03 分节拒绝仍保留；M08 未完成时不认证复杂文字。链接命中区、分页高度及数学几何必须同步更新 |
| Typst 0.15.1 | text(tracking)、text(baseline)；原生 block fill/stroke/inset；安全普通段落计算 text edge＋par.leading | `baseline` 正值向下，故共享正向上需取负值。leading 是两行边缘间空隙，[Typst par](https://typst.app/docs/reference/model/par/#parameters-leading) 没有可直接冒充的 Word exact/atLeast 开关；混合高度必须真 PDF 校准，无法证明则拒绝精确能力 |
| 受管 XeLaTeX／独立 TeX | 局部组内使用真实 baselineskip／lineskip 策略、raisebox 的 bp 偏移；受管 fontspec 字距；原生段落装饰及 tab field helper | `linespread` 不能单独实现 exact；默认 TeX 可能因高 box 退到 lineskip。固定字体 letterspacing、CJK 和跨页装饰须分别校准；不可直接 colorbox/scalebox 整段而使其无法断行分页。所需 package 必须已在锁闭包，不自动下载、搜宿主字体或 shell-escape |
| HTML／EPUB | letter-spacing、vertical-align；段落 background/border/padding，保留 em/strong/链接/MathML；同一流式 CSS helper | CSS `line-height` 是行盒的最小贡献，不是 Word exact；高 inline 可增大行盒，见 [CSS Inline](https://www.w3.org/TR/css-inline-3/#line-height-property)。Exact 必须报告损失；AtLeast 也只在已测内容范围认证。CSS tab-size 只有规则 tab 间隔，不能冒充任意 right/decimal stops |

伸缩后续单独开小单元：DOCX `w:w` 与 PDF 真水平矩阵可按百分比验证；Typst `text(stretch)` 选择宽体字面，不能保证物理缩放；CSS font-stretch 同样不能冒充水平矩阵。Typst/TeX 的真实 x 变换还必须改变排版 advance 并保留自动换行，EPUB transform 不参与普通流的 advance，未解决前明确不支持。参见 [Typst text stretch/tracking/baseline](https://typst.app/docs/reference/text/text/)。

## 5. 最小分段实现与共享文件边界

1. **A：typed 合同及 DOCX。** 新模块、专属 tests；`publication_blocks` 校验／稀疏事务／Resolved 和 `export_styles` 新字段预检；DOCX 真 pPr/rPr。无新字段的旧源、旧预设及专用 CSL/TOC 部件不变化。先取得真实包结构，再按可用 Word 环境取得物理对照，不等同两者。
2. **B：line/tracking/shift 与两类 PDF。** root 接 `md_ast::push_blocks_json` 的 `publicationStyle` 可选字段，保留已有块 type/identity/children；无覆盖不生成空字段。Rust PDF 消费同一个 resolver；Typst／TeX 分别做小型受管校准，再接实际 preview/export。同一候选重验 P03 原分节与 P07 的倍率／条目间距，不能把 bibliography.lineSpacing 改成正文新字段。
3. **C：装饰及流式出口。** 使用可分页的段落片段；HTML/EPUB 局部与全局共用 CSS 投影，保留本轮 data-table reflow、cover 与 CSL 结构；新增 reader 控件只放既有局部弹窗／导出高级区。检查窄容器和跨页，不仅检查 CSS 字符串。
4. **D：通用 tab。** 源 literal U+0009 在普通 paragraph 保持身份和复制语义；必要时加 typed Inline::Tab，代码／缩进代码的 tab 不转成正文 stops。DOCX 全部原生对齐先落地；PDF/Typst/TeX 以真实 field 测宽布局，首版明确限定 LTR、短字段与普通文字，不把过长／带复杂物体字段静默截断。EPUB 不能精确表达的 stops 保留参数与格式告警。
5. **后续：scale 与 M08／字体。** 不追加未经许可核实的字体，不把宿主 font fallback 作为受管能力；复用现有真实四字面／SMP 字体证据，只重验受新 tracking/shift/transform 影响的路径。完整整形、嵌入／子集和任意 inline 选区格式不是此文档承诺已完成的内容。

每步单文件一个写入者。root 管 `lib.rs` 注册、API/MCP schema、路由、预检、46 语言、构建与总证据；领域代理管新模块及专属测试；DOCX／PDF／Typst／TeX writer 串行交接。复用 `publication_blocks::apply` 的 ContentTransaction／inverse，不另起正文替换器。新控件收集结构化值，不要求用户写 JSON，既有主题／字号偏好不能覆盖出版值。

### 5.1 第一份可评审补丁只开行距

隔离草案位于 `.cache/capability-upgrade/publication-structure-inputs-20261015/p04-line-draft-20261015/`，保存 original/proposed 副本、原 SHA 和统一 patch；**不得直接应用到本轮冻结候选**。首单元只有 LineSpacing 三种模式；tracking／shift／装饰／tabs 不进入这份 schema，分成后续独立单元。不得把隔离代码的语法检查或测试名称计作真实通过。

| 合并单元 | 精确改动与完成门槛 |
| --- | --- |
| M0 类型 | 新 `publication_typography.rs`：`LineSpacing::{Multiple,ExactPt,AtLeastPt}`、`Scope`、`DocxLine`；`parse_line_spacing` 拒绝多余键／错分支／字符串数值／超范围，`docx` 只量化，不钳位。`validate_request` 校验原请求冲突，`global_line_spacing` 读取已补默认的有效样式，二者不可互换。无新依赖 |
| M1 源与 DOCX | `publication_blocks::{validate_style,validate_target}` 接局部新字段并拒绝数学内部；沿用 apply/null/inverse。`resolve_line_spacing` 按局部新值→局部旧倍数→全局新值→旧路径解析；无新字段时返回 None。`docx_writer::{build,build_inner,styles_xml}` 在资源 loader 前预检，在 **BodyText** 写原生 lineRule 和 snapToGrid=0；不改 Normal/docDefaults、标题、表格、列表、引用框、脚注及 CSL 的专用行距。`docx_local_style::apply` 消费同一 typed 值，文字单独变色时不能把继承的 exact 重置为 auto |
| M1 同批必接 guard | root 在原始 API/MCP/options、`prepare_export_source` 和各 direct exporter 入口完成新字段预检；全局参数在 sanitize 前校验，源声明在投影／资源 staging 前校验。未接新字段消费者的 PDF/Typst/TeX/HTML/EPUB **明确拒绝本首单元的新行距请求并保旧目标**，不得输出后静默丢失。流式近似 CSS 与 loss 处理等 M3 接入后再开放；guard 的诊断须复用合适现有 envelope 或先注册本地化 code。隔离 patch 不伪造这些共享入口已完成，因此 readyForApply=false |
| M1 测试门槛 | 类型 4 项、真实 OPC ZIP/CRC/XML＋源事务 6 项草案；root 实编译后检查全局 BodyText exact、局部 exact/atLeast/multiple、非整数 twips、文字单独覆盖／旧倍数复位、BOM/CRLF/中文/SMP 的 null 与逐字 inverse、坏值资源调用零次、旧稳定 XML 部件不变。另补所有未支持 exporter 的旧目标与零 staging 负例后才合并 |

首单元的全局范围明确限定普通 BodyText；列表／引用框保留原专用上下文，作者可用局部声明显式设置行距。扩大到这些上下文时另加 A/B 输出，不能因 OOXML basedOn 继承就声称全局参数已对所有 story 生效。实际 Word exact 的高公式裁切、AtLeast 扩张及打开保存重开仍是后续原生门槛；ZIP/XML 只认证编码与结构。

其后每份补丁只扩一条链：M2 接 Rust PDF 的 styled JSON＋三模式真实布局及旧 math/P03/P07 几何；M3 分别校准 Typst／XeTeX 和流式 loss，再开放现有入口控件；M4 tracking＋baseline；M5 可分页底纹边框；M6 tabs。每份重复仅受影响的必要回归，不能为了首个 DOCX 参数先把所有格式标成支持。

## 6. 逐参数真实文件矩阵

所有正常 case 只改该参数做 A/B；各格式能力以“实际成功文件／明确拒绝／有定位的流式损失”分别计数，不把计划输出数量算成功。固定受管字体／引擎／源码／boot／QA hash，使用原创短文；不下载新包。各 PDF 使用现有独立 inspector 的 glyph baseline/advance/path、双 parser 文本和像素；DOCX 查实际 OPC，EPUB 查 XHTML/CSS＋EPUBCheck，再另测独立阅读器。

| Case | 独立期望及实际输出检查 |
| --- | --- |
| T01 旧值／继承 | 缺所有新字段与旧 `lineHeight` 控制组；比较稳定 XML 部件／正文文本／几何，不能对带时间戳 ZIP 盲目要求全文件 SHA 相同。global→local→null 恢复、显式 0/none、同层冲突、旧预设保存重启均测 |
| T02 Multiple | 1／1.5／3 的三行段落；同引擎以实际单倍 baseline 为 unit，验证倍率；旧全局最大 2.5 不被此任务扩大；CSL 1/3×entry0/2 原几何回归保留 |
| T03 Exact | 14／18pt；DOCX line=280/360、lineRule=exact；真实 Rust/Typst/Xe PDF 连续 baseline 差对应物理 pt，容差预定 0.25pt，不能只搜源代码。EPUB 明确 loss，而非声称 exact |
| T04 AtLeast／高物体 | 14pt 与较高 inline Office Math／升降字符；无高物体时满足最小值，高行按真实边缘扩大且无交叠；保留原公式 source/物理字号。Exact 不足时验证明确拒绝／Word 原生风险边界和旧输出不变，不自动改模式 |
| T05 Tracking | `ABCDE` 的 0／+1／−0.5pt；相邻 glyph origins 分别增加指定距离，末字相对首字增量为 4×tracking，原 Unicode／轮廓保持。另测 CJK、组合音符、ZWJ；不按 codepoint 数伪造期望，不认证尚未支持的复杂脚本 |
| T06 Baseline | 0／+3／−2.5pt，真实 glyph origin 移动、字号／x advance 不变；新全局上移＋局部 0 能复位。Sup/Sub／链接／公式混合验证分层及能力限制，不重写数学内部 glyph |
| T07 Fill | none／浅色；真实段落区域有底色，文字和链接仍是可选对象；无额外 padding 时 baseline/换行不变，复制文本和全文搜索不含元数据 |
| T08 Border | 单左边／四边，1pt／2pt，gap0／2pt；查 Word pBdr 的单位、PDF stroke 和实际 enclosing bounds；相邻段落、列表内段落、跨页长段落不丢文不变不可分页大箱；none 清除继承 |
| T09 Tabs | 72／144pt 两 stops；left/center/right/decimal、四种 leader、硬换行复位、局部空 stops 清除父层。查 w:tab＋pPr tabs 和真实字段首／末／小数点 x；source U+0009 不变；无精确能力的格式必须明确 loss／拒绝 |
| T10 容器／溢出 | 局部左右缩进、窄双栏、表格 cell 内，tabs 按有效容器计算；越界 stop、过长字段、装饰占满宽、高 glyph 均定位。旧文件先写 sentinel，preview/export 两路径失败后原字节不变、无新资源旁文件 |
| T11 持久化／原生 UI | 正式 CSP；既有弹窗真实改值→撤销重做→保存→关闭重开／重启；BOM/CRLF/中文 UTF-16、重复相同段落、stale 修订拒绝、input/resources 原 SHA 不变；浅深色 1024×680/1160×820，无新增常驻面板 |
| T12 伸缩（后续） | 100／120／80%；真实 glyph x 和 advance 同比、y/字号保持；行尾重排／链接 hitbox 一致；缺真实物理变换的出口不得用另一宽体字体取得通过 |

先使用“普通短文＋一个带行内公式的窄栏段落＋一个跨页装饰长段落”三份原创材料，避免与整本书／论文矩阵重复堆产物。Word 原生、独立 EPUB 阅读器、全平台和复杂文字整形分别记录通过范围；包结构、浏览器计算样式和另一引擎 PDF 都不能替代这些验收。

本轮交付此规格与 owned cache 内隔离行距草案；生产文件未应用该草案，也未运行 Cargo／产品验收。实施批准、实际新候选输出及未支持项的最终范围由下一单元逐段收口；旧失败不覆盖，P04 保持未完成。
