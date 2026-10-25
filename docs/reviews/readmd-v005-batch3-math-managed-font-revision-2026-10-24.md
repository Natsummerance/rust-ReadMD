# 第三批：数学字体受管理消费收口与公式修订标记隔离

2026-10-24。单元 A（M07 mathFont 消费收口＋M09 rPrChange 隔离）实现轮：代码与测试完成，等待协调者统一门禁与真实验收。完整 M07、M09、第三批及发布仍未认证。

## 范围

两处最小内核改动，各配新测试模块（readmd-kernel lib 套件）：

1. **M07 mathFont 消费点收口**（`math_layout.rs` font_warnings 的 mathFont 分支）：从字符串不等比较改为显式谓词——`publication_fonts::family(f)=="STIX Two Math"` 则不产生任何 font_fallback 条目，否则恰一条 `{code:"font_fallback", font:原名, actual:"STIX Two Math"}`。`"Cambria Math"` 与 `"STIX Two Math"` 经既有 family() 映射同属受管理数学族，故确认采用；未知名与非数学族名保留真实回退诊断。同函数 run 字体分支的谓词与行为明确不照抄不改动。
2. **M09 修订标记隔离**（`math_word.rs` apply() 的过滤器）：追加排除 `w:rPrChange`，使其不再作为有效格式属性进入样式链/默认层/直接格式的合并 Properties。apply 为 materialize 与 native_run_context::apply_properties 共用，两路径同时收口。不新增失败码，不改 materialize 替换/保留两分支结构，不改 native_runs.rs 的 direct_projection/restore 代码（其行为经调用链自然跟随 materialize 分支）。

统一语义规则：**投影改写 rPr 内容（替换分支，含全展开重建）时不携带 rPrChange；投影保留原 rPr 容器（保留分支或无上下文早退）时原文保留**。修订事实的持久锚点是作者源与 DSL（RunSource 的 direct 与 source_digest 绑定全部原文），投影剥离不构成丢失。

## 预期核对矩阵（供验收员逐格核对）

### mathFont（M07）

| mathFont 值 | font_fallback 条数 | actual | 墨迹与默认一致 |
| --- | --- | --- | --- |
| STIX Two Math | 0 | — | 是 |
| Cambria Math | 0 | — | 是 |
| Foo Math | 1 | STIX Two Math | 是 |
| Times New Roman | 1 | STIX Two Math | 是 |

- Cambria Math 公式经 html_math/EPUB 路径不再因 mathFont 触发 math_font_substituted（html_math.rs:47/88 以 font_warnings 非空为条件）。
- actual 恒为捆绑数学字体名 "STIX Two Math"，不报映射文本族名（如 "ReadMD Serif SC"）——锁死不照抄 run 字体分支谓词。
- mathFont=Cambria Math 与无 mathFont 公式混排：math_document_defaults_conflict {property:"mathFont"} 拒绝行为不变；mathPr settings 往返（含 Cambria Math 原文）不变。

### rPrChange（M09）

| 输入形态 | 源保真 | materialize 投影含 rPrChange | native 导出投影含 rPrChange | run_style 与无修订逐位一致 | DSL 往返 xml_semantics 等同 |
| --- | --- | --- | --- | --- | --- |
| A：run 直接 rPr 含 w:b+rPrChange（或样式链/docDefaults 注入有效格式） | 是 | 否 | 否 | 是 | 是 |
| B：直接 rPr 仅含 rPrChange 且无上下文注入有效格式 | 是 | 是（含 author/date） | 是（含 author/date） | — | 是 |

- 形态 A 实现事实：materialize 走替换分支（effective 非空，以 properties_node 全量重建 rPr），投影不含 rPrChange/author。
- 形态 B 实现事实：两种到达路径均原文保留——(a) 无任何上下文时 materialize 走早退分支（remove_context 后原样返回），rPr 容器逐字节保留；(b) 带样式目录但 effective 为空时走保留分支（math_word.rs:264-271，仅删 rStyle）。native 路径中 direct_projection（native_runs.rs:377-382）从 RunSource.direct 原文恢复，导出投影保留原文 rPrChange。
- 样式定义 rPr 与 docDefaults/rPrDefault/rPr 内的 rPrChange 不泄漏进任何 run 投影，而样式自身格式（w:b/w:i）照常生效。
- w:rPrChange 是合法输入：不产生错误码；pPrChange 的段落侧既有 Defaults 拒绝行为不变（paragraph.rs 未触碰）。

## 新增测试（readmd-kernel lib）

`math_font_managed_tests.rs`（挂接于 math_layout.rs）：
- `managed_math_font_family_names_are_accepted_without_fallback_diagnostics`（A1）
- `unmanaged_math_font_names_report_exactly_one_real_fallback`（A2，显式断言 actual≠"ReadMD Serif SC"）
- `math_font_defaults_conflict_and_settings_retention_kept_despite_warning_policy`（A3 回归守护）

`math_word_revision_tests.rs`（挂接于 math_word.rs）：
- `rpr_change_roundtrips_through_readable_dsl_with_author_and_date`（B1 事实先行：DSL 往返保真）
- `rpr_change_form_a_replacement_projection_strips_revision_and_keeps_axes`（B2）
- `rpr_change_form_a_docx_fragment_has_no_revision_or_author`（B3）
- `rpr_change_in_styles_and_docdefaults_never_leaks_into_run_projection`（B4）
- `rpr_change_form_a_native_docx_projection_strips_revision_source_keeps_it`（B5，markdown→docx 全链路，作者模型未变断言）
- `rpr_change_form_b_preserves_revision_verbatim_in_projection_and_native_export`（B6，早退/保留双路径＋native 导出）

定向检查命令（统一配方）：
- `cargo test --locked --offline -p readmd-kernel --lib math_font -- --quiet`
- `cargo test --locked --offline -p readmd-kernel --lib math_word -- --quiet`
- `cargo test --locked --offline -p readmd-kernel --lib math_document_defaults -- --quiet`
- `cargo test --locked --offline -p readmd-kernel --lib rPrChange -- --quiet`

## 实际验证

实现轮定向检查（统一配方 `cargo test --locked --offline -p readmd-kernel --lib <过滤> -- --quiet`，CARGO_TARGET_DIR 为仓库既有 .cache/community-upgrade/build，全部退出码 0）：

| 过滤 | 结果 |
| --- | --- |
| math_font | 14 通过、0 失败（含 A1/A2/A3 与 math_font_theme 既有断言） |
| math_word | 129 通过、0 失败（含 B1–B6 与 math_word 既有全部） |
| math_document_defaults | 33 通过、0 失败 |
| rpr_change | 6 通过、0 失败（B1–B6；规格清单写的过滤串 rPrChange 因 cargo 匹配大小写敏感实测命中 0 项，测试函数名为 rpr_change） |
| math_defaults（补充守护） | 39 通过、0 失败（含 math_defaults_tests.rs:327-334 mathFont 冲突拒绝回归） |

全库门禁（协调者统一执行，candidate-90 之后）：cargo build 退出码 0；全库 cargo test --lib 3887 通过、0 失败、3 项既有忽略；六项仓库检查 6/6；前端回归 1234 通过、0 失败。

### 真实验收（2026-10-25，stdio 工具调用、每新建一个 readmd.exe --mcp 进程、私有 profile）

二进制 SHA256 `b73aab39d6a907a0453ebf1520cf8d0c777edf5c6349bcc080ea488ffc7a30a8`。证据：[batch3-m07-managed-font-revision-unit-a](../evidence/capability-upgrade/2026-10-24/batch3-m07-managed-font-revision-unit-a/verification-summary.md)（原始请求/响应/meta/PID、输出文件、独立核对摘要）。

**重建后门禁轮复证（同日第二次验收请求）**：复核 readmd.exe SHA256 与首次验收记录一致（重建门禁产出同一构建），此前全部验收产物继续有效；另以两个新 PID 在当前二进制上直接复证矩阵关键行——g-m07-recheck（PID 6096）：STIX 0 条告警、Foo 恰 1 条 math_font_substituted；g-m09-recheck（PID 36388）：migrate form-b 保留 rPrChange/审阅者（edits=0）、导出 DOCX document.xml 原文保留 rPrChange+author+date+内嵌 `<w:i/>`（zipfile 独立解包）。墨迹核对加强：HTML 导出将公式渲染为内联 SVG 存于 md-source JSON，解码后按 SVG SHA256 比较，七个 mathFont 变体（none/STIX/Cambria/Foo/Times/recheck-STIX/recheck-Foo）公式 SVG 哈希全部 `b03f9247561aaa95`，与默认逐字节一致（早前整文件 cmp 的表面差异仅为文档标题不同，非墨迹差异）。

M07 矩阵（HTML 导出路径；首次验收含上次中断续用的两组调用，PID 60156/8296，同一构建确认后续用）：

| mathFont | math_font_substituted | 公式 SVG 与默认 |
| --- | --- | --- |
| 无 / STIX Two Math / Cambria Math | 0 条 | 哈希 b03f9247561aaa95，逐字节一致 |
| Foo Math / Times New Roman | 恰 1 条 | 同上，逐字节一致 |

- actual:"STIX Two Math" 字段级断言由 lib 测试 A2 覆盖；DOCX settings 往返：m07-cambria.docx 的 word/settings.xml 含 `<m:mathFont m:val="Cambria Math"/>` 原文。
- M09 矩阵（首次验收 PID 58364→首败，3504/56960/58044 有效轮；复证 PID 6096/36388）：parse 形态 A/B ok（合法输入不报错）；migrate（content=markdown）两形态 content 原样保留 rPrChange/审阅者/日期且 edits=0；导出 DOCX 形态 A 的 document.xml 无 rPrChange 且 run 投影含 `<w:b w:val="1"/>`（真实导出走物化替换分支，伴随 math_run_style_materialized 警告，如实记录）、形态 B 原文保留 rPrChange（含 w:author="审阅者" w:date="2026-01-01T00:00:00Z" 与内嵌 `<w:rPr><w:i/></w:rPr>`）；DOCX 回读载体 OMML 与导出投影逐格对应（form-b 保留原文、form-a 已剥离）。

### 首败（原样保留）

- g-m09-a（PID 58364）id 31/33：migrate 以 `source` 字段传公式报 invalid_json——API 实际取 `content`（upgrade_api.rs:183）且输入应为 markdown 文档。属调用方参数形态错误，非产品缺陷；同 PID 的 parse（id 30/32）正常。修正后（g-m09-a2，PID 3504）通过。
- 上次验收尝试因提供商中断：M07 两组 stdio 调用与 5 个 HTML 输出已完成，经二进制 mtime（01:02）早于调用（01:11）确认同一构建后续用；M09 部分当时未开始，本轮全部补齐重跑（每新 PID）。
- 实现轮期间共享工作区曾因并行单元（publication_decoration* 系列）编译中间态阻塞约 20 分钟，未触碰其文件，等待其自愈后完成定向测试。

## 明确边界与后续

- run 字体分支（math_layout.rs:455）的警告谓词与行为保持原样；本单元只收口 mathFont 分支。
- native 导出投影在替换分支下剥离 rPrChange 是验收预期而非丢失；若产品后续要求导出 DOCX 保留可审阅修订标记（Word 接受/拒绝工作流），须另立单元修改 direct_projection/restore 或接公共审阅服务，不在本单元。
- 保留分支（形态 B）依赖 "effective 为空" 判定；若上游后续向 base 注入新默认属性，形态 B 可能滑入替换分支——B6 测试固化当前分支行为，不承诺该边界在未来单元不变。
- 真实 Word/WPS 对带 rPrChange 公式的编辑审阅行为、"Cambria Math 与捆绑 STIX Two Math 视觉等价"、mathFont 指向宿主任意字体的真实换入，均不因本单元通过而认证（I 域待验）。
- M07 的"字体换入后几何真实变化"（需第二个具备 MATH 表的受管字体资产）与 M09 其余长尾（主题色变换更多变体、样式目录跨文档 ID 冲突、完整段落修订闭环）明确不在本单元。
