# 第三批补充：生成型引用注释的当前作者字号保护

状态：限定主链已实现并实测；完整功能、外部应用与跨平台验收仍保留各自缺口。

<!-- scoped-author-inline-20261023 -->
2026-10-23：当前作者字号不丢失的同族安全降格已实现，并完成真实当前模板变化、保存／重启／空 options 输出检查。完整结构化 CSL 局部作者格式及后续 DOCX 原 CSL capsule 仍未完成，不能将此保护等同于全部引用编辑能力。本次证据来自 candidate-47 的 settings、citations 流程，原候选首败仍保留；未用测试保护代替功能完成。[限定验证报告](../reviews/readmd-v005-batch3-author-inline-roundtrip-2026-10-23.md)。

## 因果与合同

原生 footnote template 改为30pt，两条当前注释作者内容仍10pt。普通注释正确保存模板及作者 span；生成型 CSL note 因 SDT／note XML 摘要未变被直接还原为 citation，当前 run 字号被隐藏，而再次生成将继承30pt。不能将此情形谎报 XML render_modified。

只在已经识别的 generated-citation restore 边界，复用原生 run/style 索引证明作者两轴及 cs/rtl 能由当前模板与现有引用模型重建；不能因为摘要相同就抑制原始 note。marker／gap 与真正作者内容分别处理，不能仅比较首 run 或文档默认字号。

若可以等价重建，保持既有结构化引用还原；若当前模型尚不能等价表达，复用现有“保留当前可编辑普通注释＋来源冲突诊断”流程，**不抑制原生 note**。新精确 reason 为 `generated_citation_note_author_format_not_reproducible`，保留已有 binding、part、原／当前 source、原 span 和 visible_content_preserved，明确这是当前格式无法随 CSL 再生成，不能借此宣称所有引用格式已支持。

不另造黑盒，不猜测重复文字。完整的结构化 CSL generated-note 局部作者格式仍是可实现的后续模型工作；当前优先保证真实内容和字号不丢失。未知／混合 live field 的严格保护保留。

## 书目与引用族依赖

若一个生成 note 降格后仍让书目 marker 自动生成，可能因有效 citation 消失而删去当前书目；其余数字引用也可能重新编号。因此对这一新冲突采用保守的同族保护：先由已验证 manifest/generated anchors、真实 Notes 与当前 run 索引证明存在格式无法重建，再把依赖同一引用族的当前 citation／generated note／bibliography 都保留在既有 literal/manual 路径，保持当前可见全文、号码、note 和书目，保留来源与准确诊断。其他依赖 binding 使用 `citation_family_author_format_not_reproducible`，不得伪称其 XML 被修改。

判断不依赖书目出现在引用之前还是之后；使用有界的每次转换缓存，不能每次 run 重扫全篇。正常可重建材料不触发此路径。无需新的 CSL 语法或来源黑盒，也不宣称降格后仍具备所有结构化编辑能力。

## 验证

- native 仅改当前 template，辅助 parts 不变：两个 footnote 作者仍20/20半点，marker-gap60/60；endnote22/22，正文引用次数与内容均保留。明确保存、正常 EOF、不同 PID、空 options 导出，并以独立 native style inspector 核对。
- native 当前 generated-note 已等价变为模板值时继续结构化还原；不新增冲突。普通 manual note 混合两轴可读源、直接修改、保存重开均正确。
- 反例含单轴、显式 false、相同文字不同 note、真正 SDT 修改、重复 binding、未知 field。诊断精确，不用任意“modified”兜底。
- 多条同族引用、书目在前／在后、仅一条 generated note 冲突时，完整当前正文、引用号码、注释和书目均保留；正常对照继续自动还原。
- 源事务、输入包和辅助 parts 摘要不变；保存失败、过期和取消按公共链保护；保留首次失败与同一最终候选证据。未运行 Word GUI 或外部编辑器时不作应用认证。
