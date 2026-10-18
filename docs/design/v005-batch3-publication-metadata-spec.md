# 第三批 P01：共享出版元信息执行规格

P01/P07 核心已实施并完成下述限定 Windows 验证；R025/R026、完整第三批、其他平台及发布仍未整体验收。

共享作者／机构／基金及书籍模型、有界 YAML、稳定 ID 关系、继承／覆盖／清除和 UTF-16 正逆事务已实施。既有文档属性与导出高级区支持结构化行及稀疏用户预设；DOCX core/custom、PDF Info/XMP、EPUB OPF 和 typed 前置内容共用模型。

EXE SHA256：a231a2f545c002590454e47b8768f4ab0abc6573102265f576faf05ed54acf3c（70362624 字节）；boot SHA256：2610f804e9a5fb9114ec61b74aa7e7eec9af01e4b6baf14d15d08caf87c8908b。源码清单 SHA256：f6cbf2794aaecdb2d094d7fa142ef4f2ed4685ed5ee94c40cf9b54b48cca1754。

P01 原生 16 组全部通过；(35 个逐字段案例＋2 个基线)×3 格式＝111 个矩阵文件，整套 121 个实际文件；40 个离线 EPUBCheck 零致命错误／错误／警告。

P07 原生 14 组全部通过，3 个实际输出；独立 oracle 20 必需通过（15 个官方 CSL test-suite＋5 个 Juris-M／citeproc-js 补充）、0 必需失败，1 个可选扩展未支持，整体 oracle passed=false。共享真实 stdio 7 组，不重复计数。

文献布局 4 个案例／20 个实际文件／12 个 PDF：reader 与 Rust、Typst、XeLaTeX 真实 PDF 几何通过；DOCX/EPUB 仅验证实际包布局参数／结构，不认证 Word／独立阅读器几何。字形 proof：typst 4 字面／72 字形；xelatex 4 字面／72 字形。

证据：[最终 manifest](../evidence/capability-upgrade/2026-10-14/batch3-publication-context-manifest.json)；[实现与保留边界](../reviews/readmd-v005-batch3-publication-context-2026-10-14.md)。历史失败原件保留，当前通过数仅来自上述同一候选的五份完整原生报告。

保留完整 CSL 覆盖及 cs:intext 可选兼容、复杂多段／嵌套 display、Light、长注复杂分页、全部预设参数组合、可编辑引用来源往返、真实 Word／独立 EPUB 阅读器、其他平台与长期压力门槛。旧通用提示仍观察到中英混用；新增词条检查与46语言冒烟不能替代全界面语义审校。正式 TeX 包及独立冷态 120s 仍未认证；600s 完整性诊断或复用运行库的 ready=true 不构成性能通过。

下方第 1–7 节为实施前历史规划；其旧缺口与拟议路径不覆盖当前实现结论。

## 1. 实施前缺口与可复用入口（2026-10-13 规划快照）

| 审计时文件／函数 | 审计时能力与缺口（非当前状态） |
| --- | --- |
| `md_ast.rs::Document::{meta,meta_list,publication_language}` | 保留原始 front matter；`meta` 按行取值且可能命中嵌套同名键，`meta_list` 粗糙按逗号拆分，不能表示作者／机构关系。姓名、机构、关键词中的逗号会被误拆。P02 语言归一化与覆盖规则可复用。 |
| `export_styles.rs::{sanitize,deep_merge}` | 旧 `meta.title/author/subject` 被字符串化并截到 120 字；Rust 合并跳过 null，前端 `expDeepMerge` 保留 null。不能直接把 null 当作跨入口的清除命令。 |
| `assets/js/editor/editor.js::{frontmatterParts,yamlFieldBlock,openFrontmatterModal,insertFrontmatterFromModal}` | 现有文档属性入口只有单作者等少数字段；有过期快照保护，但提交会重建整个 front matter、规范化换行。不能把作者对象塞进文本框后写回字符串。 |
| `assets/js/features/export.js::{getExportSections,collectExportOptions,loadExportPresets}` | 公共 title/author/subject 与 EPUB 专用 title/author/publisher/isbn/language 分离；普通 `[data-k]` 收集器不支持结构化行。`server.rs::h_export_presets` 和现有 last/custom 存储可复用。 |
| `docx_writer.rs::{doc_title,build}` | 标题有源文档回退；封面作者和 core creator 只读 `meta.author`。无 custom properties、机构关联、摘要／基金、完整书籍前置内容。 |
| `mdexport.rs::{epub_build_entries,export_epub}` | 作者被拼成一条 `dc:creator`；publisher/isbn/description/rights/date/keywords 各自取值。无贡献者角色、完整标题细化与系列投影。 |
| `pdf_render.rs::{DocGeometry::from_style,write_pdf}`、`typst_writer.rs::render_inner` | 基础 PDF 只有 Title/Author/Subject；Typst 当前只设置 title 和拼接后的 author。缺共同 Info/XMP 与完整前置内容合同。 |
| `mcp.rs::{tools,export_document,readmd_export_epub 分支}` | 通用工具 options 未细化；EPUB 工具无 options，实际传 `{}`。需统一元信息覆盖、校验、错误及告警。 |
| `assets/js/editor/preview.js::{saveEdit,saveEditOnce}`、`export_preview.rs::{key_with_data,prepare_with_data}` | 真实修订保护保存与源／options 缓存键可复用；需验证复杂元信息保存重开、预设恢复和预览失效，不能另建保存体系。 |

## 2. 单一模型与解析合同

新增 `publication_metadata.rs`，负责解析、归一化、分层覆盖、字段范围、序列化与各格式共同的前置信息内容。writer 不再各自解析 author 字符串。建议公开 `parse_source`、`merge_layers`、`resolve`、`plan_patch`、`schema`；输出携带字段来源与诊断，避免把继承值物化回正文。

| 类型／字段 | 合同 |
| --- | --- |
| `PublicationMetadata` | title、subtitle、authors、affiliations、translators、abstracts、keywords、funding、publisher、date、rights、edition、series、identifiers、language；保留旧 subject 兼容入口。 |
| `Person` | 完整 name；可选稳定 id、显式 given/family/fileAs、orcid、corresponding、affiliationIds。不从逗号或空格猜姓名组成。作者与译者顺序有意义；被 funding.recipientIds 引用的人必须声明唯一 id，重排不改变 ID，不能用显示姓名猜关联。 |
| `Institution` | affiliations 中的独立实体：id、name，可选 department/address。人通过 ID 引用；允许共享机构和多机构，不按同名合并。重复 ID／悬空引用返回字段范围错误。 |
| `Abstract`／`Funding` | abstract 接受字符串或按语言组织的文本，归一为有序 `{language?,text}`；funding 接受字符串或 `{funder,award?,recipientIds?}` 列表。recipientIds 必须指向本元信息内的作者 id，重复人员 ID／悬空引用定位报错；仅显示文本，不执行 Markdown/HTML/TeX 代码。 |
| `Series`／`Identifier` | series 为字符串或 `{title,number?}`；edition 保留字符串。identifiers 为有序 `{scheme,value,primary?}`；isbn 是兼容入口。最多一个 primary；缺省不伪造 ISBN/ORCID。 |

源 front matter 使用顶层字段；兼容 author 标量、单对象、字符串／对象列表，规范字段为 authors。affiliations、abstract、keywords、funding、translator、isbn 等兼容形式集中在解析器处理。标量姓名／机构／关键词是一项，**不按逗号分项**；多个值必须使用 YAML 列表。规范字段与别名同时声明且冲突时定位报错，不静默丢弃一方。

复用已锁定 **yaml-rust2 =0.13.0** 的事件解析，不增加依赖、不执行构造器：front matter ≤64 KiB、单 YAML 文档、深度 ≤16、事件／节点 ≤4096；拒绝 alias、anchor 和 tag。仅提取开头 front matter，不扫描正文同名文本。建议上限：作者／译者／机构各 32、摘要 8、关键词 64、基金／标识各 16；普通文本每项 ≤4096 字节、单摘要 ≤16 KiB，整体仍受 64 KiB 限制。超预算和不支持语法明确失败，不截断后继续导出。

解析必须保留标记范围与原文 trivia；已知字段的标量保存解码文本，日期、ISBN、项目号等不经浮点数／整数中转。ORCID/ISBN 可缺省；提供时做格式和校验位检查，诊断包含字段路径与原始 UTF-16 范围。验证时可临时去除合法分隔符，但**不自动改写用户标识原文**，不把人员 ORCID 当作书籍 identifier。

## 3. 覆盖、空值及源事务

顺序固定为源元信息 → 预设 → 本次覆盖 → 当前格式的兼容覆盖。EPUB 的既有专用覆盖只作用于 EPUB。保留 P02 的 language/lang 优先级、合法化及无效覆盖回退；UI 语言不参与有效内容语言计算。

- 每层区分 inherit、value、clear：缺键/null 为 inherit；旧 meta.title/author/subject 的默认空字符串仍为 inherit；空列表明确清空列表。标量显式清除使用 `meta.clear:["subtitle",…]`，字段名限定为已知规范字段。
- 同一层同时清除并提供非继承值属于冲突；后层 value 可替换前层 clear，后层 clear 可替换前层 value。数组整体替换，不按索引递归合并。必需的容器 title/language 缺失时使用有说明的最低回退，不把回退写入正文。
- 在 `merge_layers` 中按层处理元信息，**不能先经旧 deep_merge 丢失层次再猜 clear 的来源**。只调整元信息合并接线，不改变全局 null 合并行为。保存的预设保留稀疏字段与 clear 状态，不保存“解析后的源作者”。
- `plan_patch(content,base_revision,patch)` 复用 `document_transaction::{ContentTransaction,revision}`，返回不重叠 UTF-16 编辑、逆事务及 changed。无变化不产生编辑；过期修订拒绝；API/MCP 不直接写文件。
- 仅改动被操作字段的值／行范围；未知键、未修改字段、注释、BOM、换行与正文保持原字节。列表重排移动其原始行片段；单人改名不得重序列化整份作者列表。使用现有 Save/Save As、冲突保护与恢复链路，打开／预览／取消不写源目录。

## 4. 元信息与前置页排版分层

模型不包含纸张、字号或分页。新增可选 `frontMatter.mode=none|paper|book`，旧默认 none 不物化新字段、不插入内容。布局适配生成可编辑／可重排的前置信息，不把一份 JSON 当作可见交付：paper 包含标题、副标题、作者与机构对应、通讯／ORCID、摘要、关键词和基金；book 包含书名、副标题、作者／译者及版权／版本／系列／出版社／日期／标识信息。

复用现有 cover 开关／分页及样式，明确协调前置页与封面，禁止重复生成两套标题页。生成内容不加入正文 TOC 标题、不冒充正文源范围。摘要／关键词等标签按内容语言（摘要可用其声明语言）和显式 `frontMatter.labels` 覆盖选择；缺项回退必须可说明，不能套应用 UI 语言。

界面只扩展已有文档属性弹窗和导出高级区：常用字段先展示，作者／机构结构化增删排序，书籍和论文细项折叠。源编辑与导出覆盖分开，提供继承／清除状态；不增加常驻工作台，不要求用户编辑 JSON。复用 46 语言资源，检查长姓名、长机构、RTL 及 1024×680／1160×820；导出收集器保留数组／对象类型。

## 5. 格式投影与真实能力边界

| 出口 | 映射与限制 |
| --- | --- |
| DOCX | core 写 title/creator/subject/description/keywords/identifier/language 可表达字段；完整作者、机构、ORCID、基金和书籍信息进入版本化 custom properties，补齐 `_rels/.rels` 与 `[Content_Types].xml`。前置内容为原生段落／链接。custom 不等同于 Word 的原生机构关联界面；真实 Word 打开、编辑保存仍单列应用验收。 |
| 基础 Rust PDF | `publication_metadata` 序列化 Info 值和 XMP；由现有 `pdf_render::write_pdf` **原生**写入 Info、Catalog `/Lang` 和 `/Metadata` XML stream。作者 Info 是显示串，XMP `dc:creator` 为有序数组；title/description/rights 为语言替代，keywords 为 `dc:subject`。完整细项保留在声明的扩展命名空间。不新增 PDF 写入器，不事后重写基础 PDF。 |
| Typst PDF | `typst_writer::render_inner` 使用共享模型，优先原生 document 元信息与前置内容；须对锁定 0.15.1 核定 title/author 数组及其余参数，再实际编译验证。现有 `typst_compile::apply_publication_language` 只证明 Catalog Lang 增量更新，**没有证明完整 XMP 更新无损**。未完成字段按下节条件验证后处理，不能仅凭生成源码宣称通过。 |
| LaTeX／受管理 TeX PDF | `latex_writer::{LatexOptions,render_document}` 与 `mdexport::export_tex` 共享标题／作者／日期及可见前置内容，所有文本转义，禁止作者文本成为命令；核定现有模板／引擎的原生 PDF 元信息能力。需要后处理时遵守同一 staging 合同。没有匹配的真实引擎运行证据，源码映射不计作已编译 PDF 验收。 |
| EPUB | 独立有序 dc:creator；译者为 dc:contributor，MARC role 细化；main/subtitle/edition 等标题区分，series 使用 belongs-to-collection/collection-type/必要的 group-position；publisher/date/rights/description/subjects/identifiers/language 对应 OPF。机构关系、ORCID、基金等扩展及可见前置 XHTML 保留，阅读器不保证原生展示，返回字段级能力说明。EPUBCheck 不替代独立阅读器。 |
| HTML | 保留同源有效元信息与可见前置内容；尚不支持的字段显式告警，不静默声称与 Word/PDF 元信息等价。 |

XMP 类型依据 [Adobe Dublin Core](https://developer.adobe.com/xmp/docs/xmp-namespaces/dc/)：`dc:identifier` 是文本；多标识使用 `xmp:Identifier`，不能误写成 dc 数组。EPUB 依据 [EPUB 3.3 metadata](https://www.w3.org/TR/epub-33/#sec-pkg-metadata)；custom properties 依据 [Open XML 文档](https://learn.microsoft.com/en-us/office/open-xml/word/how-to-set-a-custom-property-in-a-word-processing-document)。无标准投影的字段使用声明的扩展命名空间，不伪装成阅读器必然支持的标准属性。

### 外部引擎 PDF 的条件性处理与原子提交

只在 Typst/TeX 原生元信息不足且验证通过时，复用已锁定 **lopdf 0.42.0** 的 `IncrementalDocument::create_from`／`save_to`，沿用已有 Lang 更新路径；不调用全文件 `Document::save` 重整对象。若采用增量路径，将 Lang、Info 和 XMP 合为一次更新；保留 Producer、创建时间、未知 XMP 及引擎的合规／辅助技术声明，出版日期与文件创建时间分开。

只处理本次引擎生成、尚未发布的 owned bytes/staging。拒绝签名／ByteRange／DocMDP 或加密输入；不连接 PDF 编辑器，不修改用户已有或签名 PDF。验证新增字节以前的原始前缀完全不变；新增／替换对象仅限 Info、Metadata 及必要的 Catalog 引用，不改页面、内容流、字体、图片、链接与结构树对象。PDF 总量沿用 32 MiB 上限，解码 XMP ≤1 MiB；超限、未知合并或校验失败时中止，不以字段已生效返回。

原子边界：基础 PDF 在 `mdexport::export_pdf` 的 stage rename 前完成原生元信息校验；Typst 在 `document_bytes` 返回已验证 bytes 后才 `write_bytes_atomic`；TeX 在 `export_pdf_with_data` 的 compile_managed 与 write_bytes_atomic 之间处理／验证。取消、写盘失败、元信息失败均不得替换旧输出；无声明时保持旧路径。增量路径尚需原字节／对象比较、独立 PDF 解析及逐页像素对照证明，验证前保持未认证。

## 6. 实际测试矩阵与证据

固定两份小材料：两作者／两机构／多机构引用／通讯作者／ORCID／双语摘要／基金的论文；含译者／系列序号／版本／出版社／日期／版权／ISBN＋另一标识的书稿。独立期望从输入 fixture 编写，不反读归一化输出当期望。

| 参数组／流程 | 必须断言的实际结果 |
| --- | --- |
| 作者 name/顺序/姓名组成、机构 ID/引用、通讯/ORCID | 每次只改一项；Word 原生前置段落与 core/custom、PDF 可见文本与 Info/XMP、EPUB 独立 creator/role/前置 XHTML 同步变化；含逗号姓名／机构不拆，重复显示名不串引用。 |
| 每种摘要语言／文本、关键词条目、基金与项目号 | 分别 A/B；可见页包含全文和正确内容语言标签，包内元信息有完整值；引号、冒号、换行、Unicode 和空值不字符串化或截断。 |
| title/subtitle/译者/publisher/date/rights/edition/series/每个 identifier/language | 各字段单独 A/B；检查 OPF 对应元素／refinement、Word 部件、PDF XMP 类型与值及可见前置页。姓名角色、书籍标识、日期精度不混用；ORCID/ISBN 原值不自动改写。 |
| 继承／clear／空列表／旧空串／格式覆盖 | 源→预设→本次→EPUB 专用覆盖逐层测试；清除后不复活旧作者；切 DOCX/PDF/EPUB 不串值；无效语言维持 P02 合同，非法关系／标识定位并保护旧输出。 |
| 源与预设持久化 | 已有入口真实增删／排序、取消、无变化提交、过期事务、一次 undo/redo、Save/Save As、退出与宿主重启；核对正文、未知键、trivia、BOM/EOL 未污染，数组／对象／clear 状态恢复。 |
| PDF 两类路径 | 基础 PDF 原生 Info/XMP 解析＋PDFium 重开；仅改隐藏元信息时页面对象内容／字体／链接和渲染不变。外部引擎后处理另核原前缀与全部非元信息对象，并对原始／更新 bytes 逐页比较；前置内容开启的 A/B 则检查预期文本变化。 |
| API/MCP／预览 | UI、API 与真实 stdio 使用同一 fixture/覆盖；MCP EPUB 支持相同 options。元信息修改使 preview key 失效；预览／取消不写源；错误、能力损失与原子替换一致。 |
| 原生与兼容基线 | 正式 CSP、隔离 profile、无自动重试；绑定 EXE/boot/脚本/fixture 摘要。实际 DOCX、Rust/可用受管理 PDF、EPUBCheck 和目标截图逐项计数；亲自查看浅深色两尺寸目标与真实 PDF。无声明旧文档不新增前置内容／元信息标记，源字节行为及正文布局不变。Word／独立阅读器及不可用引擎明确未验，不能以跳过结项。 |

## 7. 编码步骤与共享边界

1. 先实现新模型、带范围的有界解析、逐层覆盖／clear、schema 与纯 UTF-16 事务；新增小 fixture/单元测试。保留 `md_ast::meta_list` 的文献资源旧消费者，P01 writer 停用其作者拆分，避免侵入 P07。
2. 接 DOCX、EPUB 和公共前置信息适配；元信息与布局模型分开。所有对外导出入口先校验，再进入 staging；直接 writer 测试也调用同一解析／模型，避免入口绕过。
3. 与 root 串行接 `pdf_render::{DocGeometry,write_pdf}` 原生 Info/XMP，不改数学几何；审查 Typst/TeX 原生能力，再实现和验证必要的受限增量适配。没有无损性证据就保留缺口。
4. 扩现有文档属性／导出高级区及 46 语言资源，接事务和现有保存／预设链；复杂行不经文本型 `[data-k]` 序列化。内容标签复用／扩展 `assets/data/publication-labels.json`，不借 UI locale。
5. 接 API 与 MCP：共享 metadata get/patch schema 与事务；通用导出和 EPUB 工具均使用同一元信息覆盖及能力告警。options 不再在 legacy EPUB 分支丢弃。
6. 产品源码冻结后由 root 顺序跑定向／全库／既有目标构建及门槛，再对同一候选执行上述原生／stdio／实际输出矩阵。逐项归档失败与范围，不修改旧证据或自动将 P01 标记完成。

实施前分工快照：主要新增 `rust/readmd-kernel/src/publication_metadata.rs`，专属小测试／fixture、UI 元信息组件及 QA；PDF 元信息适配不建立第二套 PDF writer。既有接线范围：`md_ast.rs`、`export_styles.rs`、`docx_writer.rs`、`mdexport.rs`、`typst_writer.rs`、`typst_compile.rs`、`latex_writer.rs`、`latex_compile.rs`、现有 editor/export 入口和 `assets/index.html`。`pdf_render.rs`、`server.rs`、`upgrade_api.rs`、`mcp.rs`、`lib.rs`、API codes 与词条为共享文件，由 root 协调串行合入；`publication.rs`／`academic.rs` 保持 P07 边界。

当前核心实现与已验证范围见本页首节；历史复合验收不改写旧失败，未取得的应用／引擎／平台证据继续单列。工程归档、总 ledger 与执行清单由 root 收口，本文不代替这些记录。
