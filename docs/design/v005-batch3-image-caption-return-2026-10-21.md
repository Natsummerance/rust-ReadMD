# 第三批补充：图片与自动题注的真实回读

状态：限定主链已实现并实测；完整功能、外部应用与跨平台验收仍保留各自缺口。

<!-- scoped-author-inline-20261023 -->
2026-10-23：当前图片替代文字、紧邻 typed Figure Caption 与真实媒体关系已完成保存／不同 PID 重启／空 options 再导出；不认证旧双题注的猜测重连或外部 Word 字段刷新。本次证据来自 candidate-47 的 numbering 流程，原候选首败仍保留；未用测试保护代替功能完成。[限定验证报告](../reviews/readmd-v005-batch3-author-inline-roundtrip-2026-10-23.md)。

## 已证实的问题

真实 `word/document.xml` 的 body[12]/child[3]/child[0] 是自动 Figure Caption 内的 STYLEREF 缓存。它应使用 Caption 的独立两轴 18/24 半点；当前导入却以正文 24/24 校验而拒绝。

原因是 DOCX writer 先输出图片 drawing，再无条件把 alt 写成一个有内容的 Caption 段，最后才写真正的自动图题。中间的 alt 段中断了导入器的对象关联。另一个独立缺口是 drawing 导入硬编码 `![image]`，丢弃实际 `docPr/@descr`。

## 最小实施范围

1. `docx_writer::blocks/block/block_inner` 仅通过紧邻下一兄弟的 typed `Caption`（kind 为 figure）传递局部上下文。此时抑制重复的 alt 视觉段，drawing 中原有替代文字仍保留。Styled 包装按现有 `unstyled` 规则处理。普通 alt-only、空 alt 及非 Figure Caption 保持其各自行为；不跨作者正文、节、列表项或其他结构寻找题注。不引入持久状态或新的用户设置。
2. drawing 导入沿当前图像关系和 DrawingML 的真实 namespace 获取当前图像的描述；按声明的优先级处理存在、空值和缺省，不从其他 drawing 或旧辅助信息猜测。采用现有安全文字编码形成 Markdown alt，保持字面含义，不能激活 HTML、链接、公式或脚本。
3. 通用 live-field、混合字号、未知 DATE 和真实 GFM 表格 guard 不放宽。新的正常导出应由原有对象／字段识别自然回读。旧双题注材料及真正有中间正文的输入不靠跨段猜测重连；保留原件和准确诊断。

## 必须验证

- 真实图像、12pt 正文、两章非空 alt、紧邻自动图题、STYLEREF/SEQ/REF：首次原生包只有真正图题视觉段，图像关系、媒体摘要和替代文字正确。
- 实际源 → DOCX → 转 Markdown → 受修订保护保存 → 正常退出 → 不同 PID 重开 → 空 options 再导 DOCX；再核对原 fig 身份、当前字段缓存、18/24 半点、图片和完整正文。
- alt-only、空 alt、Styled image/Caption、两图重复标题，以及中间作者段、section 和嵌套作用域；检查局部上下文不会泄漏到下一张图。
- 替代文字中的引号、括号、反斜线、实体、HTML、Unicode 和换行按解析后的原生语义保持为字面文本；未知 namespace 或其他图像的属性不能冒领当前 alt。
- 保留 unknown/mixed 字段负例，完整 Rust、前端与仓库检查通过；真实 numbered-flow 与其余受影响链路使用新冻结候选执行。

首次失败包、返回错误、候选源和旧 EXE 均须保留。源修改前先完成本规格，实测完成后才更新状态；仍不宣称 Word GUI 或其他平台已经验收。
