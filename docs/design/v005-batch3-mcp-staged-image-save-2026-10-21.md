# 第三批补充：MCP 转换图片随明确保存物化

状态：限定主链已实现并实测；完整功能、外部应用与跨平台验收仍保留各自缺口。

<!-- scoped-author-inline-20261023 -->
2026-10-23：明确保存路径已实现并完成真实素材物化、去重、清除测试临时图、不同 PID 重启和原生回读；源事务与适用的安全／回滚控制已通过。未保存内存直接导出及所有外部 OS 竞态仍未认证。本次证据来自 candidate-47 的 numbering、author 流程，原候选首败仍保留；未用测试保护代替功能完成。[限定验证报告](../reviews/readmd-v005-batch3-author-inline-roundtrip-2026-10-23.md)。

## 已有事实

转换返回的两张图片为真实 `/raw?p=<当前 profile/resource-staging/hash.png>`，媒体字节仍存在。`readmd_edit_document` 直接把临时引用写入 Markdown；它没有经过桌面保存使用的 `document_assets::materialize`。换进程后 DOCX loader 把整个 URL 当文件路径，响应 `image_missing`，实际 drawing 为零。首次 DOCX、源标签、媒体和修订均正确。

## 有限修复合同

- 保留 convert 的文本响应兼容性，不让客户端手工搬资源。明确提交 `edit_document` 时，在现有 SAVE_LOCK、修订、编码、历史和原子写入链中复用共享物化器；普通编辑的返回值和正文不变。
- 从真正的 Markdown Image destination 中发现当前 profile 的 staged 引用；正文、代码、HTML 示例或普通链接中的同文不取得读取权限。仅接受严格的单一 p 参数、有效 UTF-8、当前 staging 目录的实际普通文件、合法摘要文件名及一致字节摘要；拒绝外部路径、假参数、遍历、重解析点和丢失／篡改资源。
- 不扫描整个 profile、不建立另一套资源注册表。使用既有 32MB 单图／512MB 总量／条数预算，重用资源名、共享 readmd-assets、去重及回滚生命周期。
- 未确认、dry-run、过期修订、无修改、取消或其他失败，不创建资料夹资源。dry-run 预览仍是请求的逻辑编辑；若有资源重写，明确标记其必要性，不谎称临时 URL 已成为最终磁盘正文。
- 真实提交成功后返回实际 saved_content、actual revision 和 asset_edits／saved_assets，最终内容作为调用方后续阅读、比较与导出的依据；必要时追加字段，普通无图片合同不退化。提交失败不残留新资源、不得触碰旧图片或旧目标。
- 本增量优先闭合“转换→明确保存→重启→空 options 导出”。未保存内存直接导出的 staged URI 消费另行记录，不能把它冒充本次已实现。

## 必须验证

1. 真 DOCX 两章图片 → 转换内存：输入目录零新增；实际 read/edit 取得修订并提交；磁盘为 readmd-assets 相对引用，替代文字、图题和来源标签不改。响应 saved_content 与磁盘、actual revision 一致。
2. 正常 EOF、不同 PID，在确证永久资源及摘要后移除本测试自有临时图片，再用保存正文和空 options 导出；独立检查两张 drawing、CRC/PNG 像素、描述、STYLEREF/SEQ/REF、18/24 半点和完整正文，再实际转换回读。
3. 单图多引用内容去重；只有真图片引用物化，代码和普通链接保持原文。UTF-8/BOM、CRLF、已有共享资源、重复提交及当前磁盘冲突不退化。
4. dry-run、未确认、无修改、过期、取消、缺图／摘要不符、目录/目标失败、只读／锁冲突等适用故障：旧 MD 与旧资源不变、新资源无残留。无法真实施加的系统故障不虚构验收。
5. 参数污染、重复 query、片段、foreign profile、父级／symlink／reparse 逃逸、伪造 digest、超预算均有严格负控。不得扩张 `/raw` 路由的任意文件权限。

按新冻结候选执行完整 Rust、前端、六门禁及受影响的真实 stdio；保留 candidate-42 原始失败、源和 EXE。没有 GUI／外部 Office／其他平台证据时仍保持未验收。
