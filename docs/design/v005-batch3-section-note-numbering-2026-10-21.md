# P06-C：逐节注释编号实施规格

**2026-10-23更新：本规格的本轮有限主链已有真实实现及证据，仍有严格互操作和全矩阵未验收。** 以[本轮报告](../reviews/readmd-v005-batch3-caption-section-roundtrip-2026-10-23.md)的具体正例、失败和边界为准；下文原始规格是目标与历史审计，不代表所有条款通过。

状态：待实施、未验收。2026-10-21 经当前源码及固定 schema 审查后确定；本规格不表示 P06-C 已实现，也不改变第三批完整验收要求。

## 有限目标

沿 P06-A 的真实 Section 声明加入 footnote、endnote 各自的 `numberFormat/start/restart`，并保留原生 `numFmt/@format` 配对。继续保留 P06-A placement、两类 story、重复引用及 P06-B 作者字号。逐节字体/分隔、endnote placement/suppression、P04 和全部输出引擎重写不在本增量。

## 当前代码及真实缺口

- `publication_note_policies.rs:55`：`SectionNotePolicies` 只有 `footnote_placement`；源码拒绝逐节编号及 endnote 键。全局 `NotePolicyPatch` 已有三项及 `number_pattern`，无需另造格式体系。
- `publication_note_policies.rs:111`：`resolve_section` 已从原始 global 独立计算每节，不能改成继承前一节覆盖。
- `publication_sections.rs:498`：`ResolvedSection` 有有效 style 和原 AST block_range，但没有稀疏注释覆盖。仅有有效值不足以保持“省略继承”。
- `convert/docx_note_settings.rs:244`：原生 parser 已读两类 `numFmt/@format/numStart/numRestart`，有 namespace、属性、整数范围检查。
- 同文件 `:115`：`plan_source_sections` 只放行 placement；其他逐节差异目前准确拒绝。
- 同文件 `:586`：`fold_equal_section_policies` 把相等逐节编号搬到全局并清空局部。P06-C 不能继续因此改变显式覆盖与后续全局编辑的关系。
- `docx_writer.rs:1577`：`note_section_properties` 只写 footnotePr/pos；相邻的 document-wide helper 已写原生编号三项。Section writer 在 `:1724` 消费有效 style。
- `docx_writer.rs:729`：note ID 按 story/首次引用分配，与显示编号分离。保留既有重复引用与富内容路径。
- `publication_note_policies.rs:437`：`project_legacy` 目前只证明逐节 placement，随后读取一份全局编号。直接扩源码会使非 DOCX 漏掉差异。
- `html_footnotes.rs:45`：当前 standalone HTML 的 `Notes::number` 仍是十进制 `order.len()`；均一非默认编号也不能直接当作 HTML 支持。
- `publication_profile.rs:120`：`for_format/preflight` 是现成的共享消费门槛。`pdf_render.rs:4714` 已明确拒绝 publication sections，不能把新编号宣称在 Rust PDF 分页实现。

## 模型、源码与 API

建议内部模型：

```rust
SectionNoteNumberingPatch {
    number_format: Option<String>, number_pattern: Option<String>,
    start: Option<u16>, restart: Option<NoteRestart>,
}
SectionNotePolicies {
    footnote_placement: Option<NotePlacement>,
    footnote_numbering: SectionNoteNumberingPatch,
    endnote_numbering: SectionNoteNumberingPatch,
}
ResolvedSection { /* existing fields */
    note_overrides: Option<SectionNotePolicies>,
}
```

保持窄类型，不能将整个全局 NotePolicyPatch 直接反序列化进 Section，误开放字号、行距、分隔和特殊 story。

```md
<!-- readmd-section:{"id":"notes-middle","break":"continuous","footnotes":{"kinds":{"footnote":{"placement":"beneathText","numberFormat":"upperRoman","start":7,"restart":"section"},"endnote":{"numberFormat":"upperLetter","start":27,"restart":"continuous"}}}} -->
```

- 保留全局 YAML `readmd.publication.version: 1`；不加 profile sections 数组、私有 XML capsule、新 UI 或新参数。`Section.numbering` 是页码，不得复用。
- section 的 absent/null 表示继承当前有效 global。显式 `continuous`、0、默认格式仍是显式覆盖；未知/重复键拒绝。notes/kinds/kind 必须是 object map，不能让 serde 将空数组当默认 struct。start 必须整数，字符串、布尔、小数和越界不能夹值。
- 先沿既有层规则合并 defaults/source profile/preset/request，再从同一 global 独立覆盖本节。修改 global 不删除 section 覆盖，也不让 section2 继承 section1。
- `numberFormat` 显式而 `numberPattern` 缺省时，应产生无 `w:format` 的本节 numFmt，清除继承的旧 pattern。仅显式 pattern 时，继承有效 format 并输出 required val。两者均缺省时不写本节 numFmt。不能直接用只覆盖 Some 的普通 overlay 留住旧 pattern。
- ResolvedSection 同时保留稀疏 override 与有效 style。导入存在的编号覆盖不得仅因有效值相同就移到 global；普通无注释文档固定默认仍保持稀疏。
- section 定点编辑沿当前 declaration 的 UTF-16 范围和 `readmd_content_transaction` revision/inverse；显式保存使用 `readmd_edit_document`。global profile API 不偷偷改 section 注释。保存本身不等于输出排版认证。

## 固定 schema、原生值及省略

固定 SDK schema commit `b424997c2c81410d375a72bf68837b87b7b807e8`，本地文件：

`<repository>/.cache/capability-upgrade/publication-structure-inputs-20261015/p04-tracking-docx-draft-20261016/primary-schema/wordprocessingml-fixed.json`

SHA256 `05a5be64625cd79e756b27606587c3a2868f2e42ade196bfca2d6f647e74fdb5`。本轮逐项检查，现 WORD_NUMBER_FORMATS 与固定 ST_NumberFormat 表一致；具体枚举保留 JSON 内。不得修改该 schema 迁就夹具。

- CT_FtnProps / CT_EdnProps 顺序是 `pos → numFmt → numStart → numRestart`，各元素至多一项；required val 不能因空元素而猜默认。特殊 story 引用只属于 document-wide properties。
- Word 的 numStart 为 UInt16，保留 0–65535；不是字号或半点。文档级缺省为 start1、continuous，footnote format decimal、Word endnote format lowerRoman。节字段省略先继承文档，不能直接回到这些文档默认。
- `page` 对应 `eachPage`，不是 eachSect/chapter；改 start 本身不等于要求重启。schema 往返与实际 Word 的 endnote eachPage 可见行为分开，后者本轮无宿主证据，不作认证。
- 本增量不加 section 字号。已有 note/run size/sizeCs 按精确半点及 native i32 上界保持，10.5pt/30pt 不经过旧 UI clamp；行距单位和两轴路由不改。
- 已知格式含 chicago/none/custom 等，DOCX 可保留原生值；其他格式消费者不支持时准确拒绝。`w:format` 仍采用既有有界纯文本模型，不执行 XSLT。

依据：[固定 schema](https://github.com/dotnet/Open-XML-SDK/blob/b424997c2c81410d375a72bf68837b87b7b807e8/data/schemas/schemas_openxmlformats_org_wordprocessingml_2006_main.json)、[footnote section scope](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.footnoteproperties?view=openxml-3.0.1)、[endnote section scope](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.endnoteproperties?view=openxml-3.0.1)、[Word start 范围](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/3d4f2605-895d-43f7-9d2a-e695b1062787)、[start 语义](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.numberingstart?view=openxml-3.0.1)、[restart 语义](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.numberingrestart?view=openxml-3.0.1)、[Word endnote 缺省格式](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/a910e9f7-f95f-49ad-ad23-68ea9a7922cd)。

## 最小改动顺序

1. `publication_note_policies.rs` 及专属测试：窄 typed 字段、strict map serde、格式/pattern 联合覆盖；`publication_sections.rs` 及测试：传递 sparse note_overrides，不改变页面继承或连续节几何规则。
2. `convert/docx_note_settings.rs` 及专属测试：复用已有 native parser/caches；保留局部 presence，解除三项差异的 planner guard。沿真实 terminator_path/before_node/IDs/anchors；仍拒绝其他未支持差异。
3. `docx_writer.rs` 注释 helper/section 挂点及专属测试：同时收到 sparse override 和有效 style，只写明确编号字段；position 保留 P06-A 的现有作用域处理。两种 story 的编号都写入实际 sectPr，不能仅存源码。
4. `publication_note_policies::project_legacy` 与 root 的 `publication_profile::for_format/preflight`：逐节等价证明和精确拒绝。HTML 加最小真实能力门槛；必要的 mdexport/direct writer 挂点由各 owner 接，不扩编号产品。
5. 复用现 settings roundtrip driver/独立 XML inspector 或 root 批准的专属小模块；不新造验收系统。保所有原测试、首败和冻结证据。

## 各消费者边界

- **DOCX**：两 kind 的固定原生格式、pattern、u16 start、schema restart 值实际写回。原生结构/有效值可认证；Word GUI 字段刷新、分页和复杂自定义格式显示仍未认证。
- **EPUB / Typst / TeX**：比较所有有效 scope，包括 implicit prefix 和空节。只有单一实际 kind、每节完整编号 tuple/pattern 相同，且旧消费者可等价兑现时，才做 consumer-only flat 投影（现有五格式、允许范围和真实 continuous/section 行为）。不同节的任何差异仍精确拒绝。新增 section-origin eachPage 在该消费者未被独立证明前拒绝；旧无 P06-C 输入的 global warning 合同保持。
- **HTML**：当前 decimal-only renderer 下，仅均一 decimal/start1/continuous 可等价；新的非默认 section 编号在提交前拒绝，不能因 generic Other 投影成功就视为已渲染。
- **Rust PDF**：保留现有 publication_sections_require_paged_engine 拒绝；不能删 Section 声明冒充支持。
- meaningful 未使用的 section 配置也须保存/预检；不能因当前无 note 就静默丢弃。mixed、宽值、pattern 和其他既有 guard 不放宽。

## 每条实际正例闭环

1. 新候选绑定 source/binary/QA hash；读取真实 tools/list schema，保旧候选及首败。
2. 真 native-only DOCX，或只改当前原生目标 XML 且所有辅助来源字节不变；独立检查初始包。
3. 实际 `readmd_convert_to_markdown` 只返回内存；检查可读 global/section、身份和诊断。
4. owned sentinel MD 经 `readmd_read_document` 取得当前 revision；以 `readmd_edit_document` 的 expected_revision/old_text/new_text/dry_run:false/confirm:true 明确保存，核对磁盘 bytes。
5. stdin 正常 EOF，退出码 0。不同 PID、同 reviewed profile 重开该真实 MD；不回放旧 preset/options。
6. 实际 `readmd_export_document`，output_format:docx、options:{}、无 style_preset；fresh output 或明确 overwrite 合同。
7. 独立 stdlib ZIP/XML 沿关系、namespace、真实 section terminator、稀疏元素 presence 及有效 global+section 值核验；两种 story、重复引用、正文、锚点、原生单位都核对。
8. 再导入第二 DOCX，检查 IDs/bindings 与策略稳定。除明确保存 owned MD 外，原件、资源、preset/Last 和旧目标 hash 不变。

非法 native 负例不能保存伪成功 MD：首次失败后正常 EOF，新 PID 重试原件，保精确 typed error 与旧目标。有效但消费者不支持的源先走保存/重开，再检查 missing overwrite 的 output_exists 与 overwrite:true 后的真实 policy error；两次均保旧 bytes，目录无 sidecar。

## 精确矩阵（均复用上述闭环）

- **T01 原生三节 mixed**：global footnote lowerLetter/3/continuous、endnote lowerRoman/1/continuous；节1分别 upperRoman/7/section 与 upperLetter/27/section；节2 decimal/0/continuous；节3省略。保两种 story 同 id7，各局部覆盖和全局模板正确。
- **T02 当前原生优先**：真 source-bound 包只改一节三项、辅助来源不变；仅该节改变，旧值不得恢复。
- **T03 native clear**：删局部三项则恢复 global；另删 global 则恢复 Word 默认，不复活 auxiliary。
- **T04 global inheritance**：修改 global 只影响省略节；中间显式节不变；inverse 精确回原 source bytes。
- **T05 范围和枚举**：0/1/27/12000/65535；固定 schema 的 63 种编号格式逐项进行模型/原生往返，并在同一实际会话中保存、换进程重开后逐项再导出；另核对五种既有格式及 chicago/none/custom+bounded pattern 的代表组合。DOCX 不夹值、不插静态号码，不声称 Word 实际显示已验。
- **T06 省略与 pattern**：显式默认保持显式；global custom + local decimal 无 pattern 只清本节 format 属性；pattern-only 补 required val；空叶缺 val 准确失败。
- **T07 边界**：真空中节、CodeBlock 终止段、同名标题、continuous/nextPage、section hyperlink；声明位置、重开 ID 和 native bookmark 同一对象。
- **T08 重复引用**：同 note 跨节重复，新 note 出现在 restart 后，两 kind 数字 ID 相同；definition 数、kindByLabel、引用次序/目标、富内容均稳定。
- **T09 无 note**：固定 writer 默认不污染普通 MD；有意义的未使用 section 配置保留且不过早跳过预检。
- **T10 字号兼容**：混合作者 size/sizeCs、10.5/30pt、cs/rtl、精确行距及 note 表/代码/链接/公式，在只改编号时完整不变；资源与 preset/Last 不污染。
- **T11 均一非 DOCX**：先完成 DOCX 保存重开闭环，再实际 EPUB/Typst/TeX 核对可等价五格式、AA/BB、section restart、回链和全部严格字形/复制断言；HTML 仅默认等价正例。
- **T12 拒绝与旧目标**：varied section 的每个非等价 consumer 提交前 typed 拒绝；先 missing overwrite，再 overwrite:true；旧 bytes、原文、目录均不变。
- **T13 严格非法值**：未知/foreign URI/重复/错误 child 顺序/缺 val、数组冒充 object；start -1/65536/1.5/bool/string；其他 section size/spacing/endnote placement/suppression 保既有精确 guard。
- **T14 事务与预算**：BOM/CRLF/emoji UTF-16、stale/overlap/inverse；删改 label 的 binding mismatch 不猜前缀；128 节/8192B 声明/pattern/cancellation 预算不退化为重复扫 story。

## 已确定的实施约束

- 保留明确局部编号 presence，替代原来按相等有效值折全局的策略。
- 使用 ResolvedSection.note_overrides 传递稀疏覆盖；DOCX notes-only writer 只拥有注释属性，逐一更新构造器，不影响页面及页码属性。
- HTML 采用上述最小真实能力门槛，复用现有候选、stdio 和独立原生检查器；generic Other 投影不能代替 HTML 消费。
- 将 native eachPage schema 往返与真实 Word 分页/endnote 行为分开记账，不凭 XML 认证宿主。
