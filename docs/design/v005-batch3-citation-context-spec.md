# ReadMD v0.0.5 第三批 P07 引用上下文执行规格

P01/P07 核心已实施并完成下述限定 Windows 验证；R025/R026、完整第三批、其他平台及发布仍未整体验收。

逐项富 affix、locator、normal/suppress-author/author-only/prose、真实 note 状态、排序折叠及消歧已实施；byte/UTF-16 字段范围、修订与 nonce 唯一归属贯通 typed AST、reader／split preview 精确定位与多格式输出。ReadMD :cite[author-only ...] 是扩展，不冒称 CSL/Pandoc 标准。

EXE SHA256：a231a2f545c002590454e47b8768f4ab0abc6573102265f576faf05ed54acf3c（70362624 字节）；boot SHA256：2610f804e9a5fb9114ec61b74aa7e7eec9af01e4b6baf14d15d08caf87c8908b。源码清单 SHA256：f6cbf2794aaecdb2d094d7fa142ef4f2ed4685ed5ee94c40cf9b54b48cca1754。

P01 原生 16 组全部通过；(35 个逐字段案例＋2 个基线)×3 格式＝111 个矩阵文件，整套 121 个实际文件；40 个离线 EPUBCheck 零致命错误／错误／警告。

P07 原生 14 组全部通过，3 个实际输出；独立 oracle 20 必需通过（15 个官方 CSL test-suite＋5 个 Juris-M／citeproc-js 补充）、0 必需失败，1 个可选扩展未支持，整体 oracle passed=false。共享真实 stdio 7 组，不重复计数。

文献布局 4 个案例／20 个实际文件／12 个 PDF：reader 与 Rust、Typst、XeLaTeX 真实 PDF 几何通过；DOCX/EPUB 仅验证实际包布局参数／结构，不认证 Word／独立阅读器几何。字形 proof：typst 4 字面／72 字形；xelatex 4 字面／72 字形。

证据：[最终 manifest](../evidence/capability-upgrade/2026-10-14/batch3-publication-context-manifest.json)；[实现与保留边界](../reviews/readmd-v005-batch3-publication-context-2026-10-14.md)。历史失败原件保留，当前通过数仅来自上述同一候选的五份完整原生报告。

保留完整 CSL 覆盖及 cs:intext 可选兼容、复杂多段／嵌套 display、Light、长注复杂分页、全部预设参数组合、可编辑引用来源往返、真实 Word／独立 EPUB 阅读器、其他平台与长期压力门槛。旧通用提示仍观察到中英混用；新增词条检查与46语言冒烟不能替代全界面语义审校。正式 TeX 包及独立冷态 120s 仍未认证；600s 完整性诊断或复用运行库的 ready=true 不构成性能通过。

下方第 1–7 节为实施前历史规划；其旧缺口与拟议路径不覆盖当前实现结论。

## 1. 实施前缺口与可复用资产（2026-10-13 规划快照）

| 审计时落点 | 审计时行为／缺口（非当前状态） | 当时拟议复用与修复 |
| --- | --- | --- |
| `academic.rs::Cite / locator / citations / render!` | 请求仅 key/locator/kind；locator 手列 8 种；没有前后缀和作者模式 | 保留共享离线驱动、预算、BibLaTeX/CSL JSON 入口、locale 与自定义 CSL；扩展同一个模型，locator 用锁版 citationberg 枚举校验 |
| `publication.rs::parse_cluster / clusters` | 仅匹配以 `[@` 开始的括号引用；每项必须以 @ 起始；逗号后全部当 locator；不识别 prefix、suffix、-@、裸 @；格式化 affix 会被 Text event 范围切断 | 沿用 pulldown offset/protected-region 思路，改为有界状态扫描；在原始源字节范围上解析，不能在格式化后文本中找相同字符串 |
| `publication.rs::prepare_citations` | CSL → Markdown → 再解析；缺项只有 cluster/key，源映射只有整个 cluster 的 UTF-16 from/to；所有 note style 引用都变成新脚注 | 复用只读资源闭包、UTF-16、原文保留、bibliography marker；增加条目/字段范围、修订与 note 上下文，转成类型化投影 |
| `academic.rs::rich` | 保留部分 HTML，但 Markdown 丢 smallcaps/underline/sup/sub/light；忽略 Elem.display/meta；first_field 只有纯文本 | 输出受限 RichNode，保留格式、条目身份、display 和 first-field；现有 html/text/markdown 字段兼容保留，不能成为生产布局的唯一载体 |
| `assets/js/reader/publication.js::prepare` | 缓存 content/诊断，不保存 source_citations；reader/split preview 只消费投影 Markdown | 同源安全 HTML 显示 RichNode，保留原文/修订及精确点击范围；沿用当前迟到结果检查、撤销、显式保存 |
| `render.js::processBibCitations` | 无显式 bibliography 时的旧邻近 `.bib` 简易徽章路径；仅单 key、无 CSL 上下文 | 保留旧文档行为；完整上下文进入显式 bibliography 的共享 CSL 路径，不另扩一套格式引擎 |
| `editor.js::cmKnowledgeContext`、`document_knowledge.rs` citation 分支 | 补全仅部分 `[@...`；文献建议只读 `.bib`，不读 CSL JSON | 使用相同 key 语法，补 -@、prefix 后 @、裸 @ 和 author-only 扩展；复用有界本地资源读取并补 JSON key |
| `upgrade_api.rs::academic_citations / academic_document`、`mcp.rs` | HTTP 共用驱动；stdio 仅暴露 readmd_academic_document；未知 Cite JSON 字段现会被静默忽略 | 同一上下文验证/错误/范围供 HTTP、MCP 与前端使用，MCP 不另实现处理器 |

具体反例必须保留为测试：`[see @a, pp. 33-35 and *passim*; @b, chap. 1]` 当前不处理；`[-@a, p. 42]` 和 `@a [p. 42]` 未处理；Chicago 文献在作者脚注定义里时仍生成二级脚注，`docx_writer.rs::footnote_ref` 会报非法嵌套，Typst 也拒绝；一 cluster 缺一项时不能让已找到的子集污染之后的首引/ibid 状态。

## 2. 锁版扩展路径、许可与体积门槛

锁定 `hayagriva = 0.10.1`（默认 biblatex/archive 加 csl-json）及 `citationberg = 0.7.0`。本机 registry 原件在 `$CARGO_HOME/registry/src/index.crates.io-1949cf8c6b5b557f/`；Hayagriva VCS 基线为 `e51299ecd1a9f62b2f069af2f554ae4e84d50527`。只复制此已缓存版本至拟议 `rust/vendor/hayagriva`，由 root 在 workspace `rust/Cargo.toml` 设置 patch；不升级版本、改全局 registry、添加第二引擎/依赖或生成新 target。

已核实 public `CitationItem` **没有** prefix/suffix；`CitationRequest::prefix/suffix` 是 CSL layout 的外围符号；`CitePurpose` 只有 Author/Year/Full/Prose，私有 `SpecialForm::SuppressAuthor` 才是正确隐藏作者逻辑。`Year` 会丢非年份内容，不能替用。citationberg 的 JSON CitationItem 已有 prefix/suffix/suppress_author/label，可复用其合同，但它本身不是渲染入口。

| 拟议 vendor 文件／函数 | 最小变更及必须守住的语义 |
| --- | --- |
| `src/csl/mod.rs::CitationItem` 及现有构造函数 | 加 per-item affix 容器/设置方法，原构造函数默认空；affix 允许受限格式元素，禁止任意 Typst Markup；随原 initial_idx 参与排序，不拼到 cluster 外 |
| `CitePurpose / StyleContext::citation / CitationRequest::shall_affix` | 增公开 SuppressAuthor 并接现有私有逻辑；隐藏作者仍保留 locator、标题/其余信息及正常引用外围符号；Author 与 Prose 分别处理，不改变旧 Year |
| `BibliographyDriver::finish / collapse_items` | 在排序后、Entry(initial_idx) 对应的条目内放 affix；按实际附着位置和固定独立 RESULT 确定折叠边界，先分别核定首项 prefix、末项 suffix、中间说明及相邻标注；不能把所有 affix 一律阻断折叠，也不能让中间说明被隐藏。mixed normal/suppress/author-only 的排序、分隔符与折叠同样由独立期望验收，不能全局关 collapse 冒称等价 |
| `BibliographyDriver::finish` 的 seen/last_cite/first_note/is_near_note | author-only 不消耗脚注号、不改变后续引用的 back-reference；near-note 用真实 note_number 差而非 `res.rev().take(distance)` 的 cluster 数；保留同 driver 的消歧与全书文献排序 |
| `src/csl/elem.rs` | 优先复用现有 public Elem/Formatted/Entry meta，不需改写树模型；只有独立用例证明现有信息不足时才追加最小类型字段 |

本机原包计 224 文件、3,999,050 字节，其中 src 626,673、archive 3,117,827 字节；这只是源码审计值，不是安装体积或二进制增量。生产只复用已嵌入 archive，不另包全集/CLI。实施前留原件清单、SHA、最小 patch 与来源 diff；构建后 root 比较 EXE/安装包真实字节增量及离线闭包。

基线 `src/csl/mod.rs` SHA256 `6b7d076ea3ca13ada9585d9e790c5243a4cbfa8267d5b8dea049a10e4114f5b0`；`src/csl/elem.rs` SHA256 `4262a4507ab6a3f1c9f177ffedac5ac005ef20fb2257d9b97f39b390b7df3d4f`。代码许可为 MIT OR Apache-2.0，复制时保留 LICENSE-MIT、LICENSE-APACHE、NOTICE，并注明修改；registry NOTICE 对 archive styles/locales 和 tests/data 声明 CC BY-SA 3.0。复用 `assets/licenses/csl-attribution.json` 的既有作者/样式署名，未改 archive 不谎报为修改样式。官方测试仓库及具体所选文件仍需逐文件锁许可/提交/SHA；**现未取得精选 fixture，许可和固定独立期望是开写前门槛。**

## 3. 请求、源语法和来源合同

保留 key/locator/kind；增加 `prefix`、`suffix`、`mode: normal | suppress-author | author-only | prose`。兼容 `suppress_author`/`suppress-author` 与 `author_only`/`author-only` 布尔键；同义键不同值、两个 true 或 mode 与显式 flag 冲突必须报字段路径，不按先后覆盖。需要保留“未提供”和显式 false 的区别；例如 mode=suppress-author 配 suppress_author=false 也拒绝。mode 缺省时由 legacy true flag 推导，没有 true 时为 normal。API affix 字符串默认为字面文本；正文 Markdown affix 解析为同一受限 RichNode，不能把请求里的任意 HTML 当样式执行。

原有输入预算保持；拟新增 key≤1024 字节、每 affix≤4096 字节、单输出 RichNode 深度≤16/节点≤2048、总富格式输出≤64MiB，溢出明确拒绝，并测试边界。Cite 拒绝未知字段，防止拼错属性被静默忽略。locator 至少覆盖 citationberg 0.7.0 支持的标准类型，使用 `Locator::from_str`，拒绝未知值；schema 里 `version` 与锁版库的差异必须明报，不能声称全量最新版支持。`kind` 兼容 `label`，两者冲突报错。locale 沿既有 P02 有效内容语言，不借 UI 语言推断。

| 源输入 | 模型与边界 |
| --- | --- |
| `[@a; @b]`、`[see @a, p. 33; cf. @b, chap. 1]` | normal；prefix/suffix 属于各自条目，包含其原始字段范围；分号只在未转义且不在平衡括号/花括号内时分项 |
| `[-@a, p. 33]` | suppress-author；仍渲染其余信息，不能换成 Year |
| `@a`、`@a [p. 33]` | prose，即作者入正文加其余引用信息；不是 author-only |
| `:cite[author-only @a]` | root 已选的 **ReadMD v1 扩展**，只渲染作者或合法替代项；不冒称 Pandoc 标准，不造新脚注。允许 `:cite[author-only @a; @b]` 和同一条目的 affix；locator 保留用于往返但此模式不显示。directive 内冲突作者模式拒绝；JSON mixed-mode cluster 必须通过独立期望，不能无诊断降级 |
| 花括号限定 key、locator 及空 locator；如 `[@a{ii, A, D-Z}, suffix]`、`[@a{}, 99 years later]` | 强制消歧优先；常规启发式按有效 CSL locale 的单复数 locator 术语识别，裸数字默认 page；不能将所有尾文当页码；不确定或非法输入保留原文并定位诊断 |

key 处理兼容 Pandoc 普通/花括号形式、内部单标点和末尾标点边界；不把邮箱、URL、转义 @、代码、数学、链接目标/图片、front matter、HTML 注释或示例变成引用。保留嵌套列表、引用、表格、脚注中的真实引用；仅禁止不合法的嵌套引用结构。

每 cluster 和 item 返回原始 byte span、UTF-16 from/to、raw source、mode、locator/affix 字段范围及 doc revision；重复同文用范围区分，不按显示文字反查。错误统一有 `code/key/cluster/item/field/from/to`（无源低层 API 至少有 JSON 字段路径）。引用编辑使用既有 document_transaction 修订校验及 inverse；过期结果拒绝，保存仍只保存作者原文。

显式 note context 由正文首个 note reference 的实际阅读顺序计算，不能用脚注定义的文件位置代替；已经在作者脚注内的引用沿用该 note_number、直接替换该脚注内容，不能生成嵌套 footnoteReference。普通 note-style 引用生成单独 typed note，author-only 不占号；重复/近引共享同一 driver。补兼容 preceding_notes 的显式 context 向量，不强迫旧 API 调用者更新。

样式切换增加统一受校验的 `options.citations.style` 临时覆盖（优先于文档 csl，资源仍在 baseDir 闭包内），供预览/导出/MCP 共用。显式编辑 front matter 是作者修改；仅切预览/导出样式不能回写 source/bib/csl。

## 4. Rich AST 与一次安全映射

academic 返回自有受限 RichNode，writer 不依赖 Hayagriva：`Text{text,format}`、`Link{href,children}`、`Group{children,display,entry_index}`。format 保留 normal/italic/oblique、normal/smallcaps、normal/bold/light、none/underline、baseline/sup/sub；group 保留 block/indent/left-margin/right-inline；bibliography 保留 typed first_field、hanging/line/entry spacing。只允许经校验 URL，Markup/Transparent 不执行、不静默删除，缺转换能力时返回定位诊断并停止该格式导出。

`Prepared` 增类型化 Projection（citation spans、rich nodes、notes、bibliography、revision）；`md_ast::parse_prepared` 在 cross_references/math_dsl 改写原文之前，**按精确 byte edit** 放本次随机 nonce 的 opaque slot comment，再执行成熟解析及一次类型化 attach。slot 只认本次 Prepared 显式绑定的 nonce/id，必须一一消费；伪造、重复、缺失、越界/重叠或修订不符拒绝。不得任意字符串匹配正文、从用户 HTML 猜格式，或把 slot 写回源文件/留在成品中。

增加 `Inline::Styled {format,children}`，citation 身份由 Projection/可选 `Inline::Citation` 包装承载；notes/bibliography 插入 typed Block，不把 CSL 重排为普通 Markdown。`md_ast::inline_plain / walk / inlines_json` 及 cross_references、math_dsl、math_migration、publication_blocks 的递归都必须覆盖新 inline；不能因新增分支丢公式、源映射或脚注。bibliography display/label 需要 typed block/结构布局，单 Inline::Styled 不够。

reader/split preview 的 content 兼容字段可保持，但富引用用同一 RichNode 的安全 HTML serializer/本次 slot 表水合，不靠有损 Markdown。导出同一 Prepared 的 typed Document；不要把 server 的 HTML 塞进 Inline::Html 试图跨格式保真。

## 5. 必须贯通的真实入口

| 入口／现位置 | 实施连接点与实际断言 |
| --- | --- |
| `mdexport::prepare_export_source` 约 2465；unified exporter | 返回/携带 Prepared，而非丢掉类型只传 content；只处理一次 citation driver，所有 API/preview 共用 |
| `export_docx` parse 约 2063 → `docx_writer::Writer::inlines/run_xml` | 接 typed Document；输出真实 smallCaps/u/vertAlign/字号与 link；notes 是合法 footnotes/endnotes 部件，不嵌套；先验 XML 再真实应用打开 |
| `typst_compile` parse 约 107 → `typst_writer::Writer::inlines` | 接同一 typed Document；真实 text/smallcaps/underline/super/sub 及 display，不能接受未校验 Typst 源；实际编译 PDF 测文本/位置 |
| `export_pdf` parse 约 2358 → `md_ast::to_render_json` → `pdf_render` | JSON 保留格式。基础 renderer 尚无全部格式，须补真实 run/baseline/ink 通路或明确拒绝对应格式；不能以 loss warning 计 P07通过。本单元当前不写已冻结 pdf_render |
| `epub_build_entries` prepare_with_language 约 1589 → parse 1591 → `epub_writer::Writer::inlines` | legacy/unified/字节 API 共用 typed Document；现 Inline::Html 只取文本，须真实输出 spans/sub/sup/u/style 和安全链接，再检查 EPUBCheck/独立阅读器 |
| `export_html` 约 1119 lower_dsl → marked；reader `ReadMDPublication.prepare` | HTML 路径没有 typed writer，接同一受限 RichNode serializer/slot 消费；检查 CSP、sanitizer、语义样式和恶意字面内容 |
| managed TeX PDF：`latex_compile::export_pdf_with_data` 约 714 → `export_tex` parse 1203 → `latex_writer` | 当前已在 dispatcher 做 CSL、禁重复 bibliography；接 typed Document，保留字体/上下标/链接，而非忽略 Inline::Html |
| standalone tex/latex：`prepare_export_source` 约 2464 跳过 CSL；`latex_writer::citation_text` 585 | 独立 BibTeX/natbib/biblatex 路径只有无 locator 的 cluster 正则；改为共用上下文模型映射支持的 cite 命令，不能丢 prefix/suffix/mode/locator。无法表达的混合组合必须明确拒绝；同一文件只选一个 bibliography processor |
| HTTP + stdio `readmd_academic_document`、VS Code 共用 API | schema/response 同一 revision/字段范围/富格式；不另建转换逻辑。直接 Word/EPUB/TeX 回读现会丢 CSL key/上下文，格式输出与可编辑 citation roundtrip 分开验，未实测保持未验 |

## 6. 独立验收矩阵和失败策略

先选官方 CSL test-suite 的 RESULT/INPUT/CSL/CITATION-ITEMS/CITATIONS 原件，锁 commit、每文件 SHA/许可及采样理由；自己适配 fixtures 到本 API，**不从 Hayagriva 生成 golden**。本机 Hayagriva `tests/citeproc.rs::can_test` 会跳过 affix/near-note/position/CITATIONS，并且 test_file 将 suppress_author 误映为 Author，因此不能沿用其通过率认证本任务，也不能跳过本单元失败用例。

| 测试层 | 必须覆盖／可执行出口 |
| --- | --- |
| 官方逐字/结构化独立期望 | per-item prefix/suffix 与 punctuation、排序/数字及年份折叠、suppress-author 保 locator、author-only、同年消歧、first/ibid/near-note；有 affix 的中间数字项不能被隐藏。官方 HTML只允许有说明的属性顺序等结构归一，不吞标点/空白/姓名差异 |
| 本地语法/模型单测 | 普通+花括号 key、富 affix、分号转义、locator/suffix 强制消歧、-@、prose 与 :cite author-only；4样式族的5种形态（APA、IEEE、GB/T2015 numeric/author-date、Chicago notes），BibLaTeX/JSON 的书/章/网页/法规/中文姓名；非法 flags 冲突（含显式 false）、label冲突、未知字段/预算定位 |
| note 状态单测 | 作者脚注内引用不嵌套；重复脚注引用只算一次；首引和近引之间夹 **100 条手写脚注** 必须转 distant；author-only 不改变下一条 ibid/first-reference；缺项 cluster 不以部分成功污染后续上下文 |
| 映射/往返单测 | emoji、中文、CRLF/BOM、重复同文三处、列表/表格/脚注；逐项/字段 UTF-16 切片等于 raw；源代码/数学/邮箱/HTML示例不引用；nonce 伪造/重复/缺失拒绝；新 inline 不破坏既有 math/label/局部样式；保存重开原字节不变 |
| 跨格式真实文件 | DOCX 包内 rPr/link/note、EPUB XHTML/OPF+离线 EPUBCheck、HTML 浏览器 computed style、基础及 Typst/TeX PDF 的真实字体/文本/上下标位置、standalone TeX cite 参数。至少 1 个 CSL 同时产生 italic/smallcaps/sup/underline/link，不能只验纯文本 |
| Windows 正式 CSP + stdio MCP | 同候选读取/未保存预览、原文修改一处、undo/redo、save/reopen/restart；5种样式投影切换不回写源文献；重复引用精确定位、窄屏/暗色无截断；MCP给同一 rich/source context；每脚本独立 profile/port，正常 Windows 权限，首失败留报告，不自动重试冒作通过 |
| 独立应用返回 | Word 真打开并保存、独立 EPUB 阅读器显示/跳转、TeX 导入保留命令上下文分别记录；没有应用证据不得标已验证；是否另补可编辑 DOCX CSL 来源封装由 root 决定，本规格不假定已有 |

缺项保留原 cluster、精确告知缺失字段；有效无作者的法规/机构材料按 CSL substitute 处理，不把所有无 author 一律当损坏文献。非法上下文、无法表达的 RichNode 或未支持输出必须明确失败并保留原输出文件；读者可显示原文及诊断，但不能声称格式已应用。样式/resource 仍受现有 baseDir、大小与只读约束。只有官方独立期望、typed 全格式路径和 native 实际保存输出共同通过，才可收口 P07；失配与未获得的应用/跨平台证据保留为未验。

## 7. 分工与实施顺序

1. root 先审基线、许可证、源码/归档体积和 vendor 来源 diff；独立 QA 所有者取得并冻结官方期望，选择包含上述实际反例的最小集合。不升级引擎/样式、不先更新账本。
2. math_sol（保持 GPT-6.1 Sol/max）承担经分工确认后的 vendor 最小补丁、academic 上下文模型、publication 精确解析/诊断与相应单测。先证明 affix/作者模式/真实 note 状态，再接 UI；当前没有文件写权交接，不自行开写。
3. root 承担受限 Rich AST、Prepared/nonce attach、各 writer/导出入口衔接，统一枚举穷举与冻结文件的顺序解冻。共享 publication.rs/academic.rs 的接口先定字段并交接，禁止并行覆盖。
4. 独立 QA 所有者准备官方 oracle、typed 文件断言及新的 native/MCP脚本；复用现有 publication.spec.js、publication-native.cjs、publication-locale-mcp.cjs、publication-references.spec.js 的成熟流程，旧冻结脚本/证据保持原样。
5. root 统一 offline 定向/全 Rust/构建，发唯一候选 SHA；随后跑各一次严格 native、MCP、真实输出与应用验证，最后归档并更新状态。现有 locked fonts/pdfium、EPUBCheck JAR、受管理 Typst/TeX闭包可复用；不下载新大包，不在已有 QA profile/用户进程做试验。

以上分工与顺序保留为实施前快照；锁版 vendor、核心上下文／typed 输出和所选独立必需 oracle 已实施并验证，当前结果见本页首节。1 个 optional unsupported 及独立应用／来源往返／平台／TeX 字形边界继续单列，不能由复合验收扩大认证范围。工程归档、总 ledger 与执行清单由 root 收口，历史失败不改写。
