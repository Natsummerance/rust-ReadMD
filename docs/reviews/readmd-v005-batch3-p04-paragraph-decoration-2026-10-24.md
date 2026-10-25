# P04最小切片：普通段落局部底纹与边框（声明→DOCX原生pPr与core PDF绘制）

2026-10-24，单元B（实现轮+验收轮）。**本单元有限域通过；P04 整项、完整第三批与发布仍未完成。** 真实 stdio 验收（新 PID、独立核对、真实二进制）已执行，见下文"真实验收结果"。

## 范围与取舍

本单元只销 P04 账目（`docs/reviews/readmd-v005-batch3-rust-note-numbering-2026-10-24.md:42`）中"边框底纹"最小子项：作者在源 Markdown 中对 **ordinary body 段落** 声明 `<!-- readmd:layout v1 … -->` 局部装饰（`paragraph.shading.fill` + `paragraph.border.{top,left,bottom,right}.{color,widthPt,spacePt}`），物化为：

- **DOCX**：`docx_local_style.rs` 的 `apply` 在 `w:pPr` 注入 `<w:pBdr>`（每边 `w:val="single" w:sz=(widthPt*8).round() w:space=spacePt w:color`，四边按 CT_PBdr 序）与 `<w:shd w:val="clear" w:color="auto" w:fill="RRGGBB"/>`，按既有 `P_ORDER` 排序（pBdr<shd<spacing<ind<jc）。
- **core PDF（rust 引擎）**：`publication_pdf_paragraph.rs` 解析 decoration 进 `PStyle` 新字段；分派条件（`pdf_render.rs` 的 paginate/first_height/build 预检三处）加 `st.decoration.is_some()` 使 decoration-only 段落走 typed 路径；`add_typed_paragraph` 按**每页段**在文本前插入 `DrawItem::FillRect` 背景（颜色=声明RGB，纵向覆盖该段全部行盒，横向=该段可用文本宽 `avail-left_indent-right_indent`）与每边实心矩形条（宽=widthPt，位于文本区对应侧外移 spacePt，颜色=声明RGB）。背景→边框→文本的绘制序。
- **资格矩阵**：`Consumer::Docx/RustPdf` 的 ordinary 段落放行；quote/list/table/code/footnote/bibliography 容器内段落、heading/caption/math 挂靠 → `publication_paragraph_decoration_unsupported`；Typst/TeX/EPUB/HTML 导出与 reader view、HTML preview 收到含 decoration 的源 → 同码拒绝，在任何文件/资源/staging 之前失败，旧目标字节逐字节保留、无 assets 目录。
- **错误码**：`api_codes.rs` 新增 `PUBLICATION_PARAGRAPH_DECORATION_UNSUPPORTED` 并入 `ERROR_CODES`；`assets/i18n` 全部 46 个 locale 各加 `error.publication_paragraph_decoration_unsupported` 键（en/zh-CN 准确文案，其余 44 个先放英文兜底，46 语言语义审校沿账本另账）。

不覆盖（留账，不冒称 P04 完成）：全局样式/预设入口、Typst/TeX/EPUB/HTML 真实装饰输出、非 ordinary 语境装饰（显式拒绝）、通用制表位、水平伸缩、非正文 stories、复杂嵌套属性、CSL/bibliography 路径、Word 实际渲染验收（待验）。

## 实现挂点（代码事实）

| 环节 | 文件:位置 | 行为 |
| --- | --- | --- |
| 单点解析器 | `rust/readmd-kernel/src/publication_decoration.rs`（新） | `parse`（hex6/widthPt∈[0.25,12]/spacePt∈[0,31]整数/四边独立/未知键与null拒绝）、`parse_key`（validate_style 表适配）、`used`、`validate_source_consumer`/`validate_document`（语境与consumer矩阵）、`Decoration::docx_pPr`（(widthPt*8).round() 四舍五入） |
| 声明链放行 | `publication_blocks.rs` `validate_style` 匹配表 `("paragraph","shading"|"border")` 两分支委托单点解析；`validate_target` 白名单 `kind!="paragraph"` → INVALID | 其余键行为不变 |
| consumer/语境 | `publication_typography.rs` `validate_consumer` 挂 `decoration::validate_source_consumer`（Docx/RustPdf/Unsupported 三分）；`validate_reader_view`/`validate_preview` 显式 `decoration::used → Err(UNSUPPORTED)`；`validate_input` 计入 decoration（使 mdexport PDF 路径投影 publicationStyle） | |
| DOCX 注入 | `docx_local_style.rs` `apply` 的 `updates.push_str(docx_pPr)` | P_ORDER 既有位 |
| PDF typed/绘制 | `publication_pdf_paragraph.rs` `validate`（直连JSON同值同语境校验+image-only拒绝）、`style`（解析进 PStyle）、`plan`（拒绝条件放宽为 `line_spacing.is_none()&&!physical&&decoration.is_none()`）、`add_typed_paragraph`+`paint_decoration`；`pdf_render.rs` PStyle 新字段与三处分派条件 | 无 decoration 且无 lineSpacing/baseline 段落仍走 legacy `add_paragraph` |
| 错误码/词典 | `api_codes.rs`；46 locale JSON | `node tools/check-i18n.mjs` 通过（46 locales, 2836 keys） |

## 测试（本轮实际运行）

新测试 24 项，经 `publication_decoration.rs` 尾部 `#[cfg(test)] #[path=…]` 注册进 readmd-kernel lib 套件（`publication_decoration_tests.rs` 9 项：值矩阵/目标矩阵/语境矩阵/consumer 拒绝+旧目标保护/reader+preview/used/裸码+diagnostics/源级往返/既有键回归；`publication_decoration_docx_tests.rs` 8 项：单边/取整双向/四边序/pBdr先于shd/与align+lineSpacing共存及P_ORDER/无声明文档不变+源级两次字节相同/两段分别装饰/tab+emoji+脚注共存；`publication_decoration_pdf_tests.rs` 7 项：typed分派与legacy行距对照/两行覆盖几何+缩进收窄/四边条带几何与spacePt偏移/跨页分段绘制/tab+CJK+CRLF真实导出/直连JSON合法非法矩阵+旧目标保护+零资源回调/底纹边框共存绘制序）。

定向检查命令与结果（实现轮实测）：

| 命令（`rust/` 下，`CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=.cache/community-upgrade/build cargo test --locked --offline -p readmd-kernel --lib <filter> -- --quiet`） | 结果 |
| --- | --- |
| `decoration` | 27通过/0失败（含本单元24项新测试） |
| `local_style` | 过滤命中0项（当前测试集无此名测试；该邻域由 `publication_blocks`/`tracking`/`baseline` 过滤覆盖） |
| `publication_blocks` | 19通过/0失败 |
| `typography` | 112通过/0失败 |
| `bibliography_layout` | 14通过/0失败 |
| `light` | 25通过/0失败（与基线 25/0 一致） |
| `tracking` | 20通过/0失败 |
| `baseline` | 79通过/0失败 |
| `publication_paragraph` / `md_ast` | 1通过 / 11通过 |
| `node tools/check-i18n.mjs`（仓库根） | ok（46 locales, 2836 keys） |

全库 3853 基线、六项仓库门槛与真实运行 profile 由协调者统一串行执行（纪律），不在本单元 targeted_checks 内。

## 真实验收结果（验收轮实测）

二进制：`.cache/community-upgrade/build/debug/readmd.exe`，SHA-256 `b73aab39d6a907a0453ebf1520cf8d0c777edf5c6349bcc080ea488ffc7a30a8`（协调者统一门禁产物）。**协调者重建门禁后该二进制逐字节未变（SHA-256 相同），原始验收 session 成果全部续用有效；另执行一个新 PID 的活体复检 session（recheck）证实当前二进制行为。** 验收矩阵经真实 MCP stdio（`--mcp` 模式）执行，每个 session 一个新 PID，私有 data/workspace，全程未启动 GUI/浏览器/Office。

| Session（工具调用全部经 stdio JSON-RPC） | PID | 结果 |
| --- | --- | --- |
| positive：blocks list 装饰源、export DOCX、export PDF(rust)、同源再次 DOCX | 22100 | 4/4 通过 |
| rejections：typst pdf／tex／html／epub 拒绝、quote 语境 DOCX 拒绝（DECORATION_UNSUPPORTED）、heading 挂靠与非法 hex 拒绝（INVALID 裸码） | 20680 | 7/7 通过 |
| reading：`readmd_academic_document` view=true（应用内阅读入口）含装饰源拒绝、无装饰源正常 | 7696 | 2/2 通过 |
| roundtrip：blocks apply 装饰声明→list 校验→export DOCX→reset 恢复原文逐字节 | 40292 | 4/4 通过 |
| recheck（重建门禁后活体复检）：typst 拒绝+旧目标字节保护、rust PDF 正向导出 | 14296 | 2/2 通过 |

**旧目标字节保护**：拒绝 session 前 4 个预置旧目标（old.pdf/.tex/.html/.epub）写入手工字节并记录 SHA-256，7 次拒绝后逐字节相同（`qa/reject-prep/work/{before,after}.sha256` 逐行 diff=IDENTICAL）；recheck 复检中 old.pdf 在拒绝调用前后 SHA-256 同为 `7c980f32…a8b6`；session 目录树核对无 assets 目录新增。

**进程外独立核对**（`qa/verify.py`，Python 标准库 zipfile/zlib/re，不引用任何产品代码，19 项全过；重建门禁后重跑仍全过）：

- DOCX：第一段含 `<w:pBdr><w:left w:color="4472C4" w:space="4" w:sz="20" w:val="single"/></w:pBdr>` 与 `<w:shd w:color="auto" w:fill="DCE6F1" w:val="clear"/>`；第二段 `w:sz="21"`（2.6pt→21 取整锁定）；装饰 pPr 元素序实测位置 pBdr(139)<shd(219)<spacing(272)<ind(342)<jc(389)；无声明第三段无 pBdr/shd；同源两次导出的全部 pPr 块逐字节相同。
- core PDF 内容流（解压后）：DCE6F1→`rg` 后 `57.023622 764.69635 481.228346 14.5005 re f` 填充一个、4472C4 条带两个（2.5pt 与 2.6pt，对应两段）；2.5pt 条带 x=50.523622=文本区左缘(57.023622)−4−2.5（spacePt=4 精确）；条带高=填充高（覆盖段落行盒）；填充绘制于该段第一个 `BT` 文本块之前；ToUnicode CMap 解码页文本含三段完整文字且无 `readmd:layout` 泄漏。
- 源级往返：apply 产物 DOCX 的 pPr 与声明源直接导出完全同构（同 sz=20/space=4/fill）。

**核对脚本首败（非产品缺陷，原样保留）**：脚本初版假设 PDF 文本为 `(literal) Tj`（实际嵌入字体用 glyph-ID 十六进制编码，仅 ASCII span 可直读）；假设整页只有一个 4472C4 条带（实际两段各一条，2.5/2.6pt 各自正确）；假设流内第一个 Tj 即段落文本（实际页面装饰页脚合法地先于流程内容）。三项均为核对脚本假设修正，产品行为经修正后核对全部符合预期，未放宽任何产品断言。

## 锁定的语义决定（供验收核对）

1. **取整**：`w:sz=(widthPt*8).round()` 四舍五入，2.5pt→20、2.6pt→21，测试双向锁定。
2. **属性序**：注入 XML 经 `XmlNode` 序列化按属性名字母序输出（`w:color w:space w:sz w:val`）；XML 属性无序，ECMA-376 只约束元素序（pBdr 在 shd 前、二者在 spacing/tabs 前 pStyle/keepNext 后），测试按元素序断言。
3. **PDF 几何**：背景纵向=首行行盒上缘至尾行行盒下缘（含段前/段后距之外的行盒本身）；横向=x0()+left_indent 起、宽=avail-left_indent-right_indent；边框条带外移 spacePt（left 条右缘距文本区左缘 spacePt）。typed 路径 decoration-only 段落行间距=legacy leading 推导（11pt×1.6=17.6pt 实测一致），首行锚定沿用 typed 路径既有字形上界（与 legacy 名义 ascent 差约2.3pt，属既有 typed 行为，测试如实锁定）。
4. **image-only 段落**：RustPdf 拒绝（ImageBlock 投影无段落框可绘，静默丢键被禁止）；DOCX 放行（pPr 注入无障碍）。沿 lineSpacing 的既有 consumer 差异模式。
5. **空对象**：`{"shading":{}}`/`{"border":{}}`（reset 残留）视为无声明；存在键的 null 一律 INVALID。
6. **reader/preview 拒绝码**：含 decoration 源在 `validate_reader_view`/`validate_preview` 返回专用码（非 line-spacing 族码），`publication_blocks::view` 因此不可达（显式拒绝优先于静默丢键）。

## 证据与边界

QA 材料在 `.cache/capability-upgrade/batch3-p04-paragraph-decoration-20261024/`（gitignored）：验收样例源、五个 session 驱动与调用清单、stdio 原始日志、独立核对脚本及输出。证据归档（只新增）：`docs/evidence/capability-upgrade/2026-10-24/batch3-p04-paragraph-decoration-scoped/`（manifest 含 31 文件 SHA-256 清单、97,091 字节，远低于 4096 文件/物理 80MiB/展开 160MiB 上限；含正向/拒绝/阅读/往返/recheck 五 session 的 verdicts 与 stdio 日志、导出产物 DOCX×3/PDF×2、独立核对输出、二进制 SHA-256 及其逐字节未变说明）。

协调者统一门禁（两轮）：cargo build 退出 0；全库 cargo test --lib 3887 通过/0 失败/3 既有忽略；六项仓库检查通过；前端回归通过。本单元 24 项新测试含于全库数。重建门禁后的二进制与首轮验收二进制 SHA-256 逐字节相同（`b73aab39…7a30a8`），原始 session 成果续用有效，另以新 PID 14296 活体复检（typst 拒绝+旧目标保护+rust PDF 正向）通过。

首次中断盘点（验收轮 1）：私有 qa 目录仅有 driver.mjs 骨架（无任何运行成果），全部验收从零真实执行，无半成品冒充。二次中断盘点（验收轮 2）：五 session 产物完整（17 步 verdicts 全过），独立核对重跑全过，旧目标清单逐行 diff 相同，recheck 补充活体证据。

本轮未启动 GUI、浏览器、Office 或录屏；未运行全库测试或完整构建（协调者串行管理）；未改 Cargo.toml/Cargo.lock、未新增第三方依赖；未触碰 capability-upgrade-acceptance.json / v005-remaining-work.md（账本由冻结协调员统一管理）。44 个非 en/zh-CN locale 的英文兜底值只是键集合规，46 语言语义审校沿账本另账。Word/WPS/LibreOffice 实际渲染、Typst/TeX/EPUB/HTML 装饰输出、两引擎视觉一致认证均不据此认证（待验）。
