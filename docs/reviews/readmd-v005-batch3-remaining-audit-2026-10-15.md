# 第三批 47 项剩余实现与验收审计（2026-10-15）

<!-- closeout-20261018 -->
**2026-10-18 最新限定收尾：** [本轮实现／失败修复／证据](readmd-v005-batch3-closeout-2026-10-18.md)已归档。EPUB 数字继承、普通文字基线、公式最小未知扩展／精确身份三条最终同构建原生流程各 8 组通过（共 24 组、7 DOCX／3 PDF／6 EPUB）；实际保存与进程重启纳入其中。冷启动公式误随正文偏移已修复。Rust 全库 2966／0／3 既有忽略，最终前端 115／115。完整第三批仍未完成，47 个 ID 不整项销账。下方旧日期的“下一优先／待实现”只代表历史时点；本次以更新后的逐项表和本段为准。独立阅读器操作被用户按 Esc 中止，本次不计新增外部应用互操作。

下一步：按[第四批交接](../design/v005-batch4-handoff-2026-10-18.md)先实现持久审阅内核、最小真实 AI 工具桥和保存重启闭环；第三批完整语义、全参数及互操作继续独立跟踪。

**2026-10-17 收尾更新：最新状态及下一轮执行顺序统一见[本轮收尾清单](readmd-v005-batch3-closeout-2026-10-17.md)。** 完整离线 Rust 库已达 2932 通过、0 失败、3 项历史忽略。M05 限定算子（8 组、4 PDF、11 次真实 MCP 调用）、P15 WinAnsi 常规／粗体（8 组、5 PDF）、P13 电子书导航（8 组、5 EPUB、真实跳转与保存重启）原生证据均已归档，实际页面已复核。另修复 EPUB 默认语言覆盖源声明问题，11 项前端及 7 组原生／4 EPUB 通过。新发现的缺省数字参数物化最小值尚未修，已列为下一优先项。下列日期较早的“正在实施／待实测”仅为历史状态，不能取代最新证据；整个第三批仍未完成。

47 个 ID 是验收分组，不是 47 个完全未实现的功能。后续优先：P12 数字参数继承及 P02 语言持久化 → I06／I07 独立阅读器 → M13／M12 扩展与源码身份 → P04 基线偏移；D04 资料闭包并行准备。第四批只有规格，不计作实现。


2026-10-16 最新执行：当前已验候选 EXE `adf6928dac2f2e8d8544ad8d65fe83463562a5e9fa0e29e49963d96db31640e6`，boot `abc05d4449dec9eb86ca2cf85bfaef6cebfc92e62ce6d8b67d9d283bb7624d7b`；离线全库 **2876通过、0失败、3历史忽略**。新增限定原生证据：

- [字距与Reader](../evidence/capability-upgrade/2026-10-16/publication-tracking-reader-scoped/manifest.json)：8组、6份DOCX、保存重启及四张明暗/尺寸截图验收。修正了把跨行inline外框误作字形宽度的检查器；原失败保留，字符/片段/字体/行距容差未放宽。
- [字面公式与精准字符编辑](../evidence/capability-upgrade/2026-10-16/math-literal-character-editing-scoped/manifest.json)：9组、4份PDF、真实MCP、第二重复公式内指定字符、取消/撤销重做/保存重启。修复样式转义花括号与数学反斜杠别名；重开截图有瞬时通知，不能当成宣传素材。
- [TeX几何续测](../evidence/capability-upgrade/2026-10-16/publication-tex-geometry-continuation-scoped/manifest.json)：8组、56次实际请求、7份新PDF、21个旧目标保护及重启；生产端显式输出标记空格，严格解析器保持不变。18份旧PDF不计作当前候选重跑。warm运行不认证冷态性能。

以上已归档并追加账本partial证据，不改写历史失败。M05算子投影、P13电子书导航、P15常规/粗体字体嵌入正在实施；复杂语义、完整参数、真实Word/独立阅读器、平台与长期验收仍按下表保留。**第三批尚未完成，第四批只有设计规格。** 下文各候选与状态为对应历史时点，不能覆盖本段最新结果。

2026-10-16 当前候选更新：EXE `39bc423e208457648f95d622eff7322e9b5cbb97282711c78acee99515c84c07`，boot `68a621febcd776153adf328536c53a8ed588396b82d675be6834827d4ac62d9c`；完整离线库 **2854 通过、0 失败、3 历史忽略**。合并了有限宽 MathML、TeX 正文三种行距与 DOCX 普通段落字距。候选清单位于 `.cache/capability-upgrade/publication-structure-inputs-20261015/p04-combined-20261016/candidate-combined-20261016.json`，SHA `2a54f9b74017fc968a068ee7776122eafdc30c2603c278c8228648a51011e914`。首次全库的旧 TeX 能力断言失败保留；调整为新导出合同后全库重测通过，没有改变产品容差或跳过测试。

此候选的 tracking 原生流程 `publication-tracking-ui-native-nGtEDh` 已走通8组、6份实际 DOCX、BOM/CRLF保存、重复段落精准修改、撤销重做及真正重启，拥有者进程归零；**四张界面图尚未验收**：检查器误等待不存在的 `#toast.show`，提示遮挡了小窗口按钮，且局部格式说明过度承诺所有格式。正在修正自动预览及文案，原8组报告保持不可变，只作为功能子项证据。六份文件明确关闭前置页，前置信息不继承字距目前仅有 Rust OPC 回归；不包含 Word 字形/编辑互操作。

MathML原生首轮分别保留 CSP检查位置错误与几何对照字体不一致的失败：正式 CSP来自打包HTML meta；后者实际将浏览器默认 `math` 字体与指定 STIX输出比较。保存响应的独立浏览器对照已定位原因，检查器修正为同字体，仍保持原行高、轴线、基线容差。**修后6组、93次实际API复测通过**，归档 [有限宽 MathML](../evidence/capability-upgrade/2026-10-16/mathml-equation-array-finite-scoped/manifest.json) SHA `4b860125837f623555d31c30ff13a90f826b7637246ce34dc987275dcb02350f`；仅认证明确限定的顶层核心run方程组，不认证任意嵌套或一般电子书回流。TeX已生成16份实际PDF，前15份检查通过，第16份因pypdf将分母Z与下一行THIRD连成一个词而被词边界检查拒绝；真实字形和MuPDF文本存在，原失败保留，检查器与后续流程待完成。第三批仍未完成，第四批规格仅为后续路径。

2026-10-16 最新补充：P01/P07 限定续测已完成7组 Windows 原生流程，生成论文／书籍各 DOCX、Rust PDF、EPUB 共6个新文件，两个 EPUBCheck 通过；阅读偏好、1024×680／1160×820 明暗布局、重复引用精准选择与修改、撤销重做、BOM/CRLF 保存重开及程序重启均有实际证据。[归档](../evidence/capability-upgrade/2026-10-16/publication-structure-continuation-scoped/manifest.json) SHA `934828e5202446b1655d624b32184ff1d59e7dfecc43ee98eac5129dd725dbfd`，拥有者进程为零。保留初次空间检查器误将未使用字体当成 U+0020 轮廓来源、第二次就绪检查误拒 `installed + pdf_busy` 的失败；分别修正检查范围和有总期限的状态观察后复测，没有修改原失败报告。该续测不重新认证旧44文件矩阵，也不替代 Word、独立阅读器、完整字体嵌入、全部 CSL 或第三批验收。

这是只读盘点，不变更账本状态，不宣告第三批完成。ID 取自 `docs/design/v005-batch3-semantic-audit.json::workItems`：M15＋P15＋I10＋D7＝47。判据为当前源码、剩余规格、2026-10-14 不可变增量证据及今天实际报告；旧规格表的“实现缺口”标题不自动代表当前仍未实现。

`U`＝明确仍缺功能或仅部分语义；`T`＝已有实现待对应实际流程／范围验证；`E`＝应用、平台、硬件或交付资产条件缺口。一项可以有多个标记，标记针对所列剩余子范围，不将整个 ID 归零。

当前第二候选 EXE SHA256 为 `ce96f7fc8eb6daeb0f90d174228bb1b12490f834bcad7c8f68afcae425f295b0`；boot 为 `dbea0f86b4fc2b8edadd78b5189da30d7da8ed27de3485e1a250759fcca473af`。完整离线 Rust 2651 通过／0 失败／3 历史忽略，149.79s；这不是原生或外部应用验收。身份见 `.cache/capability-upgrade/publication-structure-inputs-20261015/candidate-second-20261015.json`，源码冻结 manifest SHA256 `4277325c411f145805db4cbf742de5477bf164bcb1c5dd4b2f36cac5a422b609`。

## 47 项逐项边界

| ID | 剩余分组 | 当前已有与具体剩余 |
| --- | --- | --- |
| M01 | T/E | 字体驱动 skw/noBar、smallFrac 与 Office 物理控制字号已实现，并有 2026-10-13 字体 oracle／实际 SVG/PDF 增量；全结构组合及真正 Word 可编辑重存仍欠。 |
| M02 | T/E | 根指数、附件点、上下 bar、宽重音及墨迹／phantom 边界已实现；不是仍用旧固定比例。Word 对照及完整组合仍欠。 |
| M03 | T/E | cSp 最小列宽、plcHide、空槽编辑与 MathML 回流已有增量；外部应用完整矩阵及 M15 裁决仍欠。 |
| M04 | U/T/E | maxDist/objDist、对齐点与容器分配的 Rust/SVG/PDF 算法已实现并有独立几何与原生证据。有限宽 MathML 路径5项 Rust及6组真实浏览器流程通过，93次API有同字体几何、实际字号、源身份与失败边界证据；复杂嵌套／混合结构的原生 MathML 和真正 Word 对照仍欠。 |
| M05 | U/T | diff 薄间距、noBreak=false 内部分行、脚本间距和 brk/alnAt 已有实现／限定原生证据；10-17 opEmu 的限定 Bin／Rel、纯 em 间距及带脚标 TeX 投影已取得8组原生、4PDF和11次MCP调用证据。剩余属性组合、来源与任意嵌套仍需补语义及实测。 |
| M06 | U/T | phantom transp 类别传播／zero/show 已实现；10-16字面字符解析与第二重复公式精准编辑已获9组原生、4PDF、MCP及保存重启证据。全部 lit 组合、外部应用裁决不由该限定流程替代。 |
| M07 | U/T | mathPr 部分默认及显式覆盖已有；全部默认项、可替换 MATH 字体选择、跨来源默认冲突策略尚未完整。 |
| M08 | U/T | STIX MATH、真实出版 italic/smallcaps、核心 DejaVu SMP 回退有实测；复杂脚本整形／bidi、精确缺字源定位、EPUB 字体子集及全部受管字体组合仍缺。 |
| M09 | U/E | `math_word.rs`／`math_theme.rs` 有 Word 样式及部分主题继承；完整主题变换、样式身份冲突、段落／修订往返未闭环，依赖真正 Word 返回材料。 |
| M10 | U/T/E | UnicodeMath 共享树／源范围已有；固定 UTN28 全语法及参考差分矩阵、Word 歧义裁决仍欠。 |
| M11 | U/T/E | mspace 已保留 width/height/depth、单位、显式0与省略，DSL/MathML往返、实际SVG/PDF及6组直接属性编辑／重复定位／保存重启已有证据。完整 MathML 表格／多脚标、Content MathML 分界及独立 Word/阅读器仍欠。 |
| M12 | U/T | 可读 DSL、UTF-16 局部修改及修订保护已有；本轮限定验证三重复嵌套公式第二处符号编辑、Esc、一次正文撤销重做、BOM/CRLF 保存重开和宿主重启。全部属性、任意 trivia／空槽／结构转换、切页与过期回调组合仍未穷尽。 |
| M13 | U/T | v2 最小未知扩展载荷已实现，按公式身份、精确源码摘要及结构摘要绑定；未知属性／命名空间元素／注释／兼容分支可在限定邻近符号编辑后保留。失配以正文为准，显式保存先持久恢复再解除过期关联；非法编码保留原注释并诊断。未知扩展不计为可编辑或等价预览；复杂组合、通用锚点重映射及 Word 往返仍欠。 |
| M14 | U/T/E | 结构编辑、填槽和符号入口已有；全 schema 属性操作、真实 IME／触控／Narrator、46 语言语义与高 DPI 全流程仍欠。 |
| M15 | T/E | 省略／空／0 的兼容代码与测试已有；SDK required 与规范默认 0 的冲突仍需 Word 真生成／重存夹具裁决，不应全局放宽 required。 |
| P01 | T/E | 共享人物／机构／基金／书籍 metadata 已完成限定 native16、111 字段矩阵、121 文件和 40 EPUBCheck；全预设／真实应用组合及新候选重新绑定仍独立验收。 |
| P02 | U/T | 共享有效语言／CSL locale 已实现；完整 RTL、复杂脚本、地区与断词／混排能力尚未覆盖。 |
| P03 | U/T/E | 稳定节身份与限定 DOCX、Typst、XeLaTeX 分节已在历史第四候选取得原生证据；原首节多页／编译失败保留。完整节组合及真正 Word 打开、修改、重存仍欠，旧候选证据不算本轮重新验证。 |
| P04 | U/T/E | 普通正文三种行距已有分格式原生证据，TeX 续测及 DOCX 字距／Reader 已归档。本轮普通文字基线的 DOCX、Rust PDF、Reader 限定 8 组／6 DOCX／3 PDF、局部清除与保存重启通过，冷排版误移公式已修。Typst/TeX/HTML/EPUB 基线未开放；水平伸缩、边框底纹、通用制表位、完整字体脚本及其他 stories 仍欠。DOCX 参数正确不等于 Word 实际版式通过。 |
| P05 | U/T/E | H1–H6、TOC depth/leader/tab 已有实现；多级编号规则／按章重启、Word 更新字段后页码和严格 widow/orphan 仍欠。不要把 leader/tab 已实现列为全缺。 |
| P06 | U/T/E | `publication_layout.rs:60/108/112` 有 note size/lineHeight/start/format 与 page/document 位置；没有每节／每页重启及续注线／提示完整模型；混合两类、三页复杂长注和真实应用仍欠。 |
| P07 | U/T/E | prefix/suffix/mode/locator、源范围、note距离、匿名substitute、typed rich和必需oracle已有证据；复杂block/indent/nested Columns已实现。7组续测补齐阅读偏好、明暗尺寸布局、重复引用精确编辑、撤销、BOM/CRLF保存及重启。旧44输出矩阵初次失败保留，修正检查器后只读验证不算重跑。cs:intext作者来源已由8项内核及21份固定原RESULT原生证据补齐；Light字重的真实字体效果、可编辑citation导出回导与全格式组合仍欠，不能称完整CSL。 |
| P08 | U/T/E | 稳定标签与 SEQ/REF 已有；按章／子图编号、续题及完整位置／分隔参数与 Word 更新字段往返仍欠。 |
| P09 | U/T/E | 基础三线表／重复表头已有；旧 B7F 修后在 Thorium 实测 300% 末行完整且与后续标题分离，Calibre 另有默认暗色表格／脚注对比证据。通用合并单元格、小数点列对齐、完整表注及全书／Word 验收仍欠。 |
| P10 | U/T/E | 五种统计模板与混合八图已有；复合图、最终物理 DPI／线宽、图引擎固定 ExportSnapshot 与过期拒绝尚未完整，Word 真实打开仍欠。 |
| P11 | U/T | `publication_blocks.rs`／`publication_pagination.rs` 有顶层及部分嵌套回写；纯链接、复杂注释／嵌套数学、多页面片段及迟到命中全面矩阵仍欠。 |
| P12 | T/U | 13 个既有预设及部分保存重启已有历史证据。本轮 EPUB 数字继承／显式零／清除／切格式／保存预设／进程重启 8 组及 6 实际 EPUBCheck 通过，非法值保护旧目标及预设。丰富材料下全部参数、机构模板和版本化迁移仍欠，不能用该限定流程认证全部预设。 |
| P13 | U/T | EPUB MathML／完整字体嵌入、封面、目录与 notes 已有；历史导航 8 组／5 EPUB、语言 7 组／4 EPUB 保留各自候选绑定。本轮数字继承 8 组／6 EPUB、实际回流几何与语言持久化通过。字体子集、长章策略及完整独立 reader 仍欠；本次 Calibre 操作中止，未新增独立阅读器通过项。 |
| P14 | U/T/E | 受管理 XeLaTeX 与 standalone native 参数已有限定实测；模板完整闭包／结构化全参数及正式可分发中文包未完成。native bibliography/raw code 不认证核心 SMP 回退，standalone 未覆盖 SMP 保留原字由用户字体处理；managed 缺字严格失败，pdfLaTeX 不认证该回退。 |
| P15 | U/T | 基础 PDF 已有 MATH／CJK／DejaVu 字体路径、Info/XMP/Lang/链接及损失诊断；10-17 WinAnsi 常规／粗体真实受管字体、CID／Unicode别名、子集及原字体度量获8组／5PDF证据。复杂脚本、全部字体角色、无字体包原生及可访问结构／tagged PDF仍欠。 |
| I01 | E/T | Word 19 类公式创建→修改→可编辑重开未取得；Word 已安装但最新有界 COM activation 45040ms 超时，不能以 XML/SDK 合法替代。 |
| I02 | E/T | Word 原生节／注释／表格、字段与 TOC 更新、自己的 PDF 分页对照尚未取得。 |
| I03 | U/E/T | Word 修改返回及修订身份关联公共审阅仍未闭环；既有导入不能证明此流程已完成。 |
| I04 | E/T | 已检测 WPS，现有用户实例保留；没有独立拥有的编辑／保存／重开流程证据。 |
| I05 | E/T | 有界盘点未检测 LibreOffice，不等于证明全机没有；GUI 编辑／字段更新／保存回读未验。 |
| I06 | T | Thorium 已有目录、公式、note popup／回跳，以及修后封面和 300% 表格的历史限定证据；原失败保留。当前修后同书的完整导航、主题／字号重排及关闭重开仍待验，不能拼接不同候选当作一套通过。 |
| I07 | T | Calibre 已有 Black+Never、中文／emoji／行内公式、另一修后书默认暗色表格／脚注，以及 B7F 关闭重开位置的历史分别证据。notes／crosslinks／display math 与当前同书完整流程仍待；本次合成新书检查被用户按 Esc 中止，不计通过。 |
| I08 | T | 旧同候选 System32-only PATH／正常 CSP 的格式与受管理引擎证据已有；今天第二候选也通过同前置，仍需最终范围绑定，不将 QA-only TeX runtime 叫正式产品包。 |
| I09 | T/E | 必须实际已安装 VS Code Extension Host 做完整诊断／定位／编辑／导出／取消；MCP 或 JS library 测试不能代替，当前无该完整证据。 |
| I10 | T | 真实 stdio 引用 metadata7 等旧增量保留；当前复杂分节／布局及数学编辑、取消／过期／读产物完整链仍需同最终候选实测。 |
| D01 | T | 字体／OCR／资源租约与保护已有，不列为全未实现；最终候选的安装／修复／导入／卸载竞争、等待含总期限及取消完整矩阵仍欠。 |
| D02 | T/U | 既有任务预算／拥有者／恢复与部分三引擎测试保留；Typst/OCR 完整崩溃、后代句柄、桌面/MCP 与旧协议矩阵不能由 TeX 单链证明，未覆盖边界须逐项补实现或实际测试。 |
| D03 | T/E | WAL／task workspace 归属与恢复代码已有；首归属前、OS API 成功后、任务引用、长暂存／远程卷、隔离磁盘满／硬配额仍需实际故障验证，不拿普通缓存清理代替。 |
| D04 | U/E | 匹配版本资料闭包仍欠规格列的八条 source/doc／历史材料；不能用不同版本或 QA 私有 runtime 冒充正式完整审计。 |
| D05 | U/E/T | 现 QA-only 受管中文 TeX 实测不等于正式锁定、许可／源码齐全的可分发包及安装全周期；正式闭包仍缺。 |
| D06 | T | 校验并发优化已有，原120s不放宽；复用 profile warm ready 与旧600s cold诊断都不认证性能。仍缺至少三次独立冷态全过程分别达标的证据，不能以均值或 warm 通过代替。 |
| D07 | U/T/E | 现 task workspace reserved_bytes／资源保护已有；安装同卷完整峰值预留、去重／回滚／在用资源与低空间／锁／取消／启动恢复的完整边界仍需补齐和实际小卷验证。 |

## 今天证据与当前阻断

- `.cache/capability-upgrade/publication-structure-native-yypBMF/report.json`：首候选前9组通过，第10组 Typst 真实分节失败；原件保留。
- `.cache/capability-upgrade/publication-structure-native-1Kwrei/report.json`：上述 ce96 第二候选前10组通过；第11组 XeLaTeX 真实页检查 `P03FRONT` 实际第2页／预期第1页失败，`hostStopped=true`。复杂 P07 及后续未执行，不能补写为通过。
- 今天纯 Rust／source diagnostic 的复杂 CSL、分节与列几何为实现证据；`typst-columns-mvp-focused-20261015`、`sections-source-diagnostic-second-20261015` 不替代真实 native。
- `.cache/capability-upgrade/epub-independent-20261015-2q1fctek/baseline-safe-archive-index-first.json` SHA256 `3ea2e19ce19075d652849a18202e9da05fb633afc6fa853dc54956301b1e6a95`：独立应用局部证据索引，不是整套通过报告。封面／300% 表格首失败及回跳／主题实际证明各自保留；修后未验不改旧失败。
- `.cache/capability-upgrade/interop-ULrFZp/report.json`／`post-run-summary.json`：Word installed-unverified、COM activation timeout；WPS 用户实例未复用；无真正 Word open/edit/save/reopen 验收。
- 2026-10-14 P01/P07 不可变来源：`docs/evidence/capability-upgrade/2026-10-14/batch3-publication-context-manifest.json`，candidate a231；旧111矩阵／20必需 oracle／三引擎72字形等不能直接迁移成 ce96 本轮通过。

## 建议的下一小功能

先修正在阻断实际套件的 P03 XeLaTeX 首节多一空页（由拥有该 writer 的代理定位），同时继续一个独立 P07 小功能：**真正消费可选 `cs:intext` 的作者布局**。当前锁版 citationberg 的 IndependentStyle 只有 citation/bibliography；vendor `src/csl/mod.rs:1826` 的 `do_author` 仅从两者选择作者组件，没有 intext 来源，已知固定可选原期望中的作者丢失不能重新记作“显式不支持”。

建议保留 CSL1.0.2 默认路径，在明确可选扩展入口保存／解析 intext layout，作者输出使用原模型中的格式及名字选项；Prose 的余下 citation、note 距离、normal/suppress-author 和匿名 substitute 不变。以既有固定原 RESULT 作独立期望，并补无 intext fallback、intext 独有作者／宏／富格式、无作者 substitute、前后 affix 与 note/in-text prose 的 hand-authored 回归。不能只给作者字符串拼接，不能修改官方期望，不能把可选扩展通过称为全 CSL。

随后最清晰、不受外部应用阻塞的数学实现单元是 M04：在 `math_layout.rs::table`／eqArr 分支建立 m:aln 对齐点，向内部传可用宽度，分别实现 maxDist 与 objDist，覆盖四组合、多对齐点、窄容器及嵌套。它是明确实际算法缺口，比再重复现有投影审计更有价值。

本次未读私有官方 fixture 原文，未启动 Cargo／EXE／外部应用，未修改原账本或已有完成状态。

## 后续实际执行更新（同日，保留以上审计时点）

以下为上述只读盘点之后的实际执行，不将早先失败改写为通过：

- P07 的可选 `cs:intext` 作者布局已补齐并实测：8 项专属内核测试、正式 CSP 下真实 API 的 21 份固定上游原始 RESULT 全部通过。对应新候选 `5e4a3486…6c31a4a`、完整库 2671/0/3。归档见[引用作者布局增量](readmd-v005-batch3-citation-intext-2026-10-15.md)。因此上表 P07 中“作者来源仍漏”已由此增量解决；完整 CSL、可编辑回导与全格式布局仍欠。
- P03 共享 TeX 前置页和正文参数已新增实现，38 项 writer 与 7 项导出回归通过；第三候选实际 `publication-structure-native-NVS5s7` 再次通过 DOCX 包与 Typst 真实几何，但 XeLaTeX 返回 `latex_compilation_failed`。当轮原生宿主停止、源码不变均确认；此项仍未通过，使用受管持久诊断单独定位。
- 修后 B7F 电子书在 Thorium 的初始封面正常。独立确认 300% 字号后，末行 `C / 8 / Third fictional observation` 跨页内容完整、与后续 H3 分离，原表格串页问题的这一判据通过；不代表所有大字表格或全书验收。PrintWindow 白块已由仅截取拥有者窗口、前后无遮挡校验的真实屏幕像素证明是 QA 捕获方式问题，原截图保留。
- Word 新一次有界启动仍未成功。Calibre 旧拥有者实例已正常关闭、子进程清空，并用隔离的新配置启动修后电子书；尚未完成整套新工作流，不计为全套通过。
- [M04 方程组规格](../design/v005-batch3-eqarr-implementation-spec.md)和[P04 出版参数规格](../design/v005-batch3-publication-typography-parameter-spec.md)已完成具体算法／字段及测试设计，仍为待实施项。[第四批执行规格](../design/v005-batch4-execution-spec.md)明确九个工作包及首个持久审阅流程，不改变第三批完成状态。

## 第四候选与下一增量的实际更新

以下状态覆盖上述审计中的相应旧缺口，不改写原始失败报告。第三批仍为进行中。

- **P03 分节：限定原生流程通过。** 第四候选 `04ca73d7…b2e730` 的完整库为 2677 通过、0 失败、3 历史忽略。真实 DOCX 原生节属性、Typst／XeLaTeX PDF 的纸张方向、物理奇偶页／空白页、编号与单／双栏，以及流式 EPUB 锚点和损失报告全部通过。修复了 TeX 页眉宏全局化污染及 XeTeX 空白页未沿用当前纸张尺寸。归档见 [P03 与独立阅读器增量](../evidence/capability-upgrade/2026-10-15/batch3-publication-sections-readers-manifest.json)。这不包含 Word 真正打开、修改、重存后的互操作认证，也不认证冷启动性能。
- **P09／P13／I06／I07：独立阅读器只更新已实测子项。** 新电子书 `58021164…ce3f` 在隔离 Calibre 9.16 默认暗色配置下，交替表格行与脚注正文实际文字／背景对比分别为 12.4113∶1 和 13.7925∶1；原图经视觉复核。关闭时拥有者进程及子进程为零，源书和副本不变。另一本 B7F 候选已证明关闭重开恢复原位置，Thorium 的 300% 表格末行完整且与后续标题分离。不同输入、不同判据分别记录，不拼成完整九组阅读器通过；原暗色失败保留。
- **M04：实现已合入，原生验证待执行。** 方程组共享对齐点、四种 maxDist／objDist 组合、嵌套局部宽度、源身份及诊断已实现；20 项 Rust 和 9 项 Node 专属测试通过。真实容器 CSS px→pt 已修正。独立原生脚本已准备，未执行不计通过。MathML 容器分配及 Word 对歧义情形的裁决仍欠，不能标记整个 M04 完成。
- **P04：DOCX 行距增量已合入。** 严格的倍数／固定／最小行距、旧参数继承、局部覆盖、预设保存与公共入口校验已完成，23 项专属 Rust 测试通过。尚未接入相应语义的格式／预览会在资源暂存和覆盖目标之前明确拒绝。现阶段不宣称所有格式行距完成，也不把这个增量当作字距、基线、段落装饰和制表位已实现。
- **P07 新实测发现，正在修复。** 扩展矩阵的原始失败 `publication-structure-native-bxvEMC` 保留：Typst 超长无空格引用标签越界、XeLaTeX 嵌套左右列文字覆盖是真实产品缺陷。Rust PDF 的相同两例经实际页面复核没有列重叠，但原 QA 将左右列按整页文本流线性比较；测试需要按真实字形区域验证每列、保留全部字数。官方引用的结构归一化、脚注边界和长例超大正则也分别修正 QA，不能以放宽预期掩盖产品错误。修后同候选原生矩阵尚未运行。

第四批仍按既定依赖推进：持久逐块审阅 → 固定导出快照与 Rust 多轮任务 → 协作、开发者、PDF／OCR、媒体、图片绘画与长篇出版。实施细节与开源许可证／可复用边界保留在第四批规格及路径研究中；目前没有把设计稿计作第四批实现。

### 第五候选：DOCX 行距已获得原生证据

第五候选 `69a6b42f…dbdd2a`、boot `ce238d3a…f0616b` 已离线构建，完整库 **2728 通过／0 失败／3 历史忽略**，实际耗时 289.65s；测试总时长不作为启动／输入性能达标证据。

`publication-typography-native-0LNudI` 的 8 组真实 Windows 流程全部通过：正式 CSP／实际 boot，原生 DOCX 的全局 exact 及局部 exact／atLeast／multiple，未实现格式和无效参数保护既有目标，预览拒绝，预设持久化，真正停止和重启进程后重新读取及再导出。两个 DOCX 经独立 ZIP CRC／XML 参数检查，原始 BOM／CRLF 文件及源码摘要不变，事后拥有者进程为零。见[行距增量证据](../evidence/capability-upgrade/2026-10-15/batch3-publication-line-spacing-manifest.json)。这仅更新 P04 的 DOCX 行距链，其他格式、更多文字／段落参数及真正 Word 版式仍未验收。

同候选首次 M04 原生运行已通过正式 CSP 和四组合×12／24pt 的实际绘制坐标。第三组因 QA 将根 MathML 的 `alttext` 中原 OMML 的转义 `&amp;` 误当作可见对齐字符而停止；真实单元格内容已另外只读核对，修正版 QA 将按呈现树检查。原报告 `math-equation-array-native-mDpv5X/report.json` 及失败截图保留，不把尚未执行的保存重开等后续组补写为通过。原始 XML 作为可访问替代文本的可读性另记 M14 后续，不能因它能保存原文就称读屏体验已完成。

### M04 方程组：限定 Windows 原生流程已通过

第五候选的 `math-equation-array-native-biWui5` 已通过全部 9 组、50 次显式真实 API 请求：四种分布组合在 12／24pt 的独立字体坐标核对，跨 run／空对齐列、UTF-16 身份，嵌套可用宽度及真实溢出，过期修订拒绝，MathML 呈现树和尚不支持分配的明确诊断，CSS 内容宽度换算、本地化与诊断清除，第二个重复公式的真实属性修改／Esc／一次撤销重做，以及保存、重开和进程重启。事后拥有者进程为零；四张原生截图已人工视觉复核。见[方程组原生增量](../evidence/capability-upgrade/2026-10-15/batch3-math-equation-array-manifest.json)。

中间报告 `math-equation-array-native-b6lvXe` 保留了另一处 QA 首失败：两公式独立自然宽合计 48.168pt 放入 48pt 容器，产品正确报告每个子项所需 24.084pt／可用 23.916pt，旧测试误要求无诊断。修正依据独立字体 oracle，仍严格校验每个子项身份、完整墨迹及宽容器无溢出；未放宽几何容差或隐藏产品错误。两次失败原件都随增量归档。

此增量不关闭 M04 的 MathML 容器分布和真正 Word 歧义裁决，不关闭 M14 的可读替代文本、全属性操作、真实 IME／触控／读屏与 46 语言语义验收；高级属性面板的滚动体验仍可改善。第三批继续进行中。

### 第六候选：Rust PDF 行距与公式间距的实际进展

候选 `4e79c9cb…c90a4a9`、boot `ce238d3a…f0616b` 已离线构建，622 份源冻结摘要为 `e01cbcf3…a6a860`。完整库 **2794 通过／0 失败／3 历史忽略**，146.73s；候选索引为 `.cache/capability-upgrade/publication-structure-inputs-20261015/candidate-sixth-20261015.json`。首轮完整库 `2793/1/3` 保留：一个 legacy 迁移测试误要求有100ms上限的 Drop 同步清空 staging，修正后在独占锁内验证一次恢复、旧源不变和可验证迁移；未更改产品清理预算，未仅靠重跑取得通过。

- **P04 M2 Rust PDF：限定原生流程通过。** `publication-pdf-typography-native-XhOEm2` 的7组、10次实际API请求和7份PDF通过，涵盖 exact18、multiple2、atLeast22、局部16pt/exact24、高矢量公式撑开最小行距、300行跨页、无效/不足行距不覆盖旧目标、预设保存及真正进程重启后再导出。两套独立文本解析器与实际baseline坐标核对，root抽查高公式、长段第一页及局部覆盖三张实际栅格；拥有者进程为零。原 `94RbY8` 首失败是QA使用已弃用的 `fitz` import产生stdout警告，破坏JSON传输；改用已有检查器同样使用的 `pymupdf` 后通过，原报告和PDF保留。仍不认证真正Word、其他输出引擎、所有容器或全部出版参数。
- **M11：尺寸语义和实际PDF已推进，错误传递仍需修复。** 共享 `mspace`、原单位/显式0与省略、DSL/MathML往返、UTF-16局部修改、真实字号预算及无墨迹空间盒已实现，34项专属测试通过。`math-space-native-C1uRBg` 前6组真实API与PDF几何通过，但第7组发现导出 `{ok:false,error:"math_budget"}` 漏掉 `error_code`，尚未完成UI编辑、保存重开等后续组，不能计整套通过。原报告SHA `07d1ab75…12bf828`，拥有者进程为零。正在修复共用导出／预览／MCP错误映射，并补准确本地化。
- **M04 MCP：4组真实stdio流程通过。** `math-equation-array-mcp-thusPm` 校验真实工具发现中的尺寸参数、12/24pt方程组布局、过期修订／错误类型范围和序列化，源文件与输入目录不变，进程已退出。报告SHA `c64ede36…8d6853`。不代表VS Code Extension Host或整项数学能力验收。
- **M14：可读数学替代文本已实现，7项专属测试通过。** 不再把原始XML或DSL作为呈现公式的可读说明；空矩阵槽位仍保留位置语义。这不代替真实Narrator、输入法、触控或46语言语义审校。
- **P07：继续保持未完成。** 独立8例oracle发现的空names组多余词项已修；Typst富文本叶片换行与TeX嵌套列留白修复已合入，各自回归通过。同候选复杂引用矩阵正在实测。原 `LuQCiO` 的120s请求超时保留；后来盘点发现该请求的3页PDF已存在，但缺精确abort/worker时序且原请求未成功，不能将后来文件存在改写成原验收通过。

第四批补充了[首条完整流程规格](../design/v005-batch4-first-workflow-spec.md)：稳定审阅身份、4MiB新审阅正文预算、有序人工操作记录、保存重启后逐块撤回及导出D/C交错。它仍是待实现设计；第三批、第四批和发布状态均不因新增规格改变。

### 第六候选引用矩阵结束与限定证据归档

Rust PDF 行距的7组原生结果、7份实际PDF、独立检查结果和3张原始页面图已归档为[限定行距证据](../evidence/capability-upgrade/2026-10-15/publication-pdf-line-spacing-scoped-sixth/manifest.json)，manifest SHA256 `c86dde1de4fba7fff1735c8065c871257395fe1cc2c1b2bd928ec773127aa6d3`。归档前重新核对622份冻结源、候选、检查器及拥有者进程为零；仅约189KB选定原件，不复制引擎、字体或私人资料。R028/R029增加证据引用，状态仍为进行中。

复杂引用矩阵 `publication-structure-native-GtRbxx` 已结束，共61个检查组，其中两处Typst结构和一处阅读字体／缩放联动未通过。最终 `artifacts.json` 实际记录44个矩阵输出、42个通过；原 `report.json` 中间统计为33/31，存在报告刷新问题，不能将旧统计当成最终输出数量，两个原件均保留。当前超长XeLaTeX请求和输出通过，但不改写上一候选的120秒超时首失败。结束瞬间记录的3个WebView子进程稍后已退出，单独后续盘点为零，不将原清理快照改成通过。产品缺陷与检查器边界正在分别诊断，未执行的余项仍不算通过。

### M11 公式间距：错误修复后原生流程通过

共享导出／预览／MCP错误映射补齐精确的 `math_budget`、`math_invalid_xml`、`math_unsupported_extension`；5项真实入口Rust回归通过。前端新旧错误格式使用相同的准确提示，预算错误不再建议仅切源码编辑器；46语言新增两个错误文案，全部2737键检查通过。译文为模型翻译及代码语义检查，不能计作全部母语审校。

候选 `e0a75305…d899e`／boot `a8497fdd…ebbcd`，完整库 **2799通过、0失败、3历史忽略**。真实 `math-space-native-jsn4ge` **10组、58次API请求、6份PDF全部通过**，覆盖尺寸单位、显式0／省略、MathML与DSL往返、UTF-16范围、实际字号限制、PDF真实字形位移与无额外墨迹、失败保旧目标、第二个重复公式编辑、无效草稿恢复、Esc、一次撤销／重做、保存重开及真正进程重启。原生拥有者进程为零，候选及检查依赖保持绑定。

见[公式间距限定证据](../evidence/capability-upgrade/2026-10-15/batch3-math-spacing-scoped/manifest.json)，manifest SHA256 `1b3972e58565aed3665f99ece53ea15f5f757913d987ccf1458ab434fc251be0`。两张1160×820界面图经root视觉检查：两处重复公式独立，第二处保存后间距增大，未见分式遮挡。长结构槽位文本仍需水平滚动；没有专用间距属性控件、全MathML、Word／TeX实际应用、1024窄屏或完整读屏认证。本次不关闭整个M11、第三批或发布状态。

### 共享行距界面及 Typst 实测（继续更新）

候选 `c1206e36…533aab`，完整 Rust 库 **2812通过／0失败／3历史忽略**，153.78秒。冻结源码626项；新行距控件复用导出高级区及段落局部格式，46语言三项文案补齐，全部2740键检查通过。未取得46语言母语审校，不作该项认证。

- **P04共享控件：8组真实流程通过。** `publication-line-spacing-ui-native-R2lG75` 验证旧预设不被被动迁移、倍数／固定／最小三模式、非法值阻止保存、合法共享预设可在暂不支持的格式保存但不能假导出、预设真正停止重启后读取、真实Rust PDF预览和导出各18pt，以及仅第二处重复段落的局部修改、一次撤销重做和BOM/CRLF保存重开。两份PDF由独立解析器验证；拥有者进程为零。首次`QXjduC`是QA嵌套locator作用域错误，修复后复验，原报告保留。见[共享行距UI限定证据](../evidence/capability-upgrade/2026-10-15/publication-line-spacing-ui-scoped/manifest.json)，SHA256 `62f80a64aacdaa88ed674f73a9e1082503142aca9a68978f65bb75281b12615a`。
- **UI视觉边界如实保留。** root检查1024×680浅色和1160×820深色的导出／局部格式四张原图，控件均至少44px且未越界；浅色截图还显示此前主动触发的错误toast，覆盖部分底部按钮，不能宣称所有截图底部无遮挡。深色图底部清楚。英文是隔离测试配置，不改变用户语言。此素材不是宣传录屏。
- **P04 Typst首轮仍有真实失败。** `publication-typst-typography-native-VTxbOK` 前7组、15份PDF实际通过：三种行距、预览和导出缓存一致、重复段落来源定位、24pt局部覆盖、24/48pt矢量公式和100行跨3页。第8组固定行距不足被正确拒绝，但受管引擎的相对`document.typ`诊断未被识别，错误被泛化成`typst_compilation_failed`。已针对真实stderr合入仅绑定当前工作目录的窄修；必须重新编译并实测后才算修复通过。原报告和旧输出均保留。
- **P07检查器修正：原文件未变。** 162项独立检查器正反控制通过；root对原44份输出的文件／响应／检查摘要重新核对，26份PDF通过修正后的物理检查。该步骤只读、产品调用0，不能改写原`GtRbxx`失败或冒充新候选全矩阵。修正保留全部字符、空格身份和原容差，只纠正真实基线及经过嵌入字体轮廓证明的无墨迹行末空格。阅读器偏好、精确引用编辑等剩余流程另做实际补充验证。

第四批路径及首流程规格继续保留，仍未开始实现；第三批状态保持进行中。出版行距以外的字距、伸缩、基线偏移、段落装饰、通用tab及真实Word互操作不因以上增量变为已完成。

### 10月16日：直接公式间距控件与 Typst 行距通过

当前候选 `b9ba47ec…f8869`、boot `feaf5490…e127`，628份冻结源；完整Rust库 **2817通过／0失败／3历史忽略**，148.97秒，离线构建成功。以下仅更新实际验证的子项，全部原失败保留。

- **M11直接控件：6组原生流程通过。** 新增宽度、高度、深度属性输入，沿用公式局部事务；原单位、显式0与清除属性分别保存。实际验证第二处重复公式精确修改、非法草稿恢复、局部撤销／Esc、正文一次撤销、保存重开及宿主重启。两张1160×820原图人工检查，三个控件独立清晰；原长结构源码槽位仍横向滚动，截图带正常短暂提示，不作宣传素材。见[间距控件限定证据](../evidence/capability-upgrade/2026-10-16/math-space-property-controls-scoped/manifest.json)，SHA256 `adb0ee32c4af30848537c96ee98332622b48cd2a0a65778d4cb4fb6569afc87a`。46语言键覆盖2742；尚不代表46语言母语审校、1024窄屏／深色、IME、触控或读屏认证。
- **P04 Typst行距：11组、38次请求、18份PDF通过。** `publication-typst-typography-native-KRaBgW` 实际验证三种行距、局部字号、24／48pt矢量公式、100行跨页、预览与导出、参数继承及预设重启。实际PDF基线容差仍为0.004pt。前次相对 `document.typ` 诊断已修复并实测，固定行距不足返回具体几何错误且不覆盖目标。原`9F1lso`第二次失败是QA将无绑定的嵌套quote误要求“格式不支持”；原孤立样式病例保留其准确错误，并另加有效的unsupported heading，未删减保护检查。root抽看高公式、24pt局部正文及跨页尾页三张实际栅格，无可见遮挡。见[Typst行距限定证据](../evidence/capability-upgrade/2026-10-16/publication-typst-line-spacing-scoped/manifest.json)，SHA256 `d54612a34760daaa8502a921965cd405cc2ecc8a64818b6dca0354b4afd5606d`。
- **范围限制保留。** Typst固定行距采用保守的整段包围测量，诊断line=1不是精确物理行；可用高度是当前节正文总高度，不是当前页剩余高度。仅普通正文／支持的局部段落，不认证所有stories、TeX、EPUB或Word实际版式。两个归档均确认源码／检查器摘要和拥有者进程为零。
- **P07补充实测未全部通过。** `publication-structure-continuation-native-mNCHUJ` 前5组通过：真实字体／缩放测量、两尺寸两主题、重复引用定位与键盘操作、过期拒绝、一次撤销重做、BOM／CRLF保存重开和真正宿主重启。第6组元信息导出检查被QA的字体轮廓要求阻断：旧Rust PDF路径合法使用未嵌入Base14，空格验证不应无条件升级为“所有字体嵌入”认证。正在收窄检查器语义；原报告、输出和字体嵌入缺口保留，尚未执行的项目不计通过。

下一步继续TeX正文行距、DOCX字距与有限容器MathML方程组分布；均先隔离实现，再根进程串行编译和真实验收。第四批仍按既定共享审阅／导出快照／Rust Harness依赖路线，不提前标记实现。
