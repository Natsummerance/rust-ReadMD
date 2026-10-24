# 第三批：DOCX 参数回导与文档排版继承

接续已验 candidate-26（999 源文件、Rust 3109/0/3 既有忽略、前端 479/0/0、真实 stdio 61 项/60 输出）。本增量针对仍开放的 P05/P06/P08 与相关 I10 子范围；不改写既有失败，不提前认证整个第三批。

## 交付闭环

真实 DOCX → 读取当前原生编号与注释参数 → 生成可编辑 Markdown → 明确保存 → 进程重启后重开 → 不重传旧 options 的 DOCX 再导出 → 独立核对原生结构。另从原始 DOCX 只改原生 XML、保留辅助来源，验证当前修改不会被旧值覆盖。

转换只生成当前工作内容，不创建 MD 或旁文件。普通无相关设置的文档不因 writer 固定默认值产生无用元数据。所有新能力由同一 Rust 服务供桌面、MCP 与预览使用；不新增场景工作台。

## 源数据与选项优先级

- 编号继续以现有 `readmd-numbering:v1` 为唯一源声明，不在另一份 profile 中复制编号规则。
- 其他文档级排版复用 YAML front matter 中的 `readmd.publication`，`version: 1`。本次开放 `footnotes`；未知字段保留原文并给有范围的诊断，不交旧 sanitizer 静默改值。
- 源解析借用现有有预算 YAML 基础，只在已声明的数字字段中转换词法数字；不能全局转换 ISBN、标识及未知 YAML。
- 优先级固定：程序默认 → 省略/上次记忆的预设 fallback → 源 profile → 本次明确选中的预设稀疏 delta → 本次显式 options。省略 preset 与明确选中 preset 分开；无源旧调用保持原 minimal 行为。
- 源只用于继承显示，不写入 Last、用户预设或全局设置。API 的 null 沿用继承；删除源叶子使用受修订保护的源事务，写显式默认值才是重置到默认。false、0、空字符串不一律当清除。
- 合并在预检和预览缓存键之前完成；预览与导出使用相同已解析选项。source 或 options 变化均使缓存失效。
- 来源辅助信息只保留原生无法推断的显式性及身份关联，必须与当前 native 结构绑定；不保存可强行覆盖当前原生参数的旧整份正文或旧配置。

示例：

```yaml
---
readmd:
  publication:
    version: 1
    footnotes:
      restart: continuous
      kinds:
        footnote:
          numberFormat: decimal
          start: 1
        endnote:
          numberFormat: lowerRoman
          start: 4
      kindByLabel:
        legal-note: footnote
        appendix-note: endnote
---
```

## 编号回导

先解析原生 numbering、段落 numPr、有效样式继承、SEQ/STYLEREF/REF 及书签；不依赖自家来源文件才能取得参数。区分标题编号、普通列表及题注，不能双重写入显示号码与自动规则。

原生可表达规则应进入可读模型；若现有模型缺少确实可实现的逐级文本、后缀或原生起点语义，先扩展共享模型及实际消费者，不能为了让已有测试通过就把规则硬投影成统一分隔。无法等价的外部规则明确给诊断并保留可见文本/来源，不能假装全部 Word 编号均已可编辑。

真实默认转换开启 `form_tables` 时，已与原生表题精确关联的表格必须保留为表格，不能被两列表单启发式改为列表而留下失效的题注和引用。关联使用不可变原生节点身份，沿用空结构段的判定；不得跨正文、样式段或节边界猜测对象。未关联的普通两列表单继续保留原行为。首次导入与保存重开后的再输出都要验证实际表格、题注及引用，不能只检查 `:label` 文本存在。

省略、显式默认、显式关闭与有效覆盖分别建模。正文、标题身份及当前手改优先；重复文字用节点与原生身份定位，不使用全局字符串替换。

## 注释策略与混合 story

- 保留 legacy `footnotes` 平铺设置及 `position` 的旧含义；新增 `kinds.footnote/endnote` 两个独立稀疏策略和精确 `kindByLabel`。不根据标签前缀猜原生类型。
- 每个原始稀疏层先展开其明确的平铺设置，再叠该层 kind 设置，之后合并层；高层旧字段不能被低层 kind 设置反压。typed 行距与旧比例行距按同一互斥语义处理。
- 原生解析共享已加载 story、关系及样式缓存，不按每个 note 重解析整份 story。保留当前重复引用、两种 story 相同数字 ID、生成引用脚注及手改作者注的身份。
- 使用中的策略包括字号、真实行距/单位、数字格式、起点、重启、位置、两种分隔、续注文字及长注段落策略。合法 native 值超出旧 UI 范围时不夹成另一数值；消费者不能表达则在提交前诊断/拒绝。
- DOCX writer 必须实际分流引用节点、定义、关系、style 与 settings；仅写 YAML 或返回未使用 DTO 不算混合支持。
- Footnote placement 处理 Word 的逐节实现；endnote placement 为文档级，不能错误提升 section 上被忽略的值。分节覆盖要恢复对应边界，不能以 H1 代替 native section。
- 特殊 story 根据设置中的引用 ID 和原生 type 选择，不能假定固定 -1/0。复杂无法等价的特殊内容不假装是一条默认线。
- `longNote` 不等于 `keepNext`。有效 keepLines 可构成默认；混合段落属性保留局部覆盖。悬空 label 映射须定位并诊断，不猜相似对象。

规范依据：[Word 脚注位置实现说明](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oe376/f43937cb-efc3-432b-82d2-0bdac95b763b)、[尾注位置](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.endnoteposition?view=openxml-3.0.1)、[Word 尾注缺省数字格式](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/a910e9f7-f95f-49ad-ad23-68ea9a7922cd)、[原生起点范围](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/3d4f2605-895d-43f7-9d2a-e695b1062787)。实际 Word GUI 互操作继续独立待验。

## 实施所有权

| 工作线 | 文件与责任 |
| --- | --- |
| 编号 | 独立 `convert/docx_numbering_import.rs`、专属回归；共享编号模型/writer 改动先报具体挂点，避免并改。 |
| 注释 | 独立 `publication_note_policies.rs`、`convert/docx_note_settings.rs`、专属回归；共享 Section/writer 改动分阶段接入。 |
| 主任务 | source profile、统一有效 options、convert/lib/API/preview/导出接线、UI 继承收集及本地化；最终编译与实际进程验收。 |
| 独立 QA | 真实 stdio 与独立 native 变异/检查；不靠来源载荷本身证明 native 支持，不修改验收期望来迁就实现。 |

## 验收与纪律

1. 手工原生 fixtures 与自家导出材料并列：混合格式/列表、起点和重启、native 手改/清除、重复/移动书签、独立两 story 策略、相同 ID、重复引用、精确/最小行距、非默认包路径、样式继承、分节、特殊 ID、长注及未知/恶意参数。
2. 源事务检查 UTF-16 范围、BOM/CRLF、相邻未知 YAML 与注释保留、一次撤销、迟到和过期修订保护；打开/预览不写文件。
3. UI 文档 A 的继承值不能进入 Last/用户预设污染 B；明确选预设、单字段覆盖、清除和重置另测。继续单行 toolbar 与既有控件风格。
4. 独立真实 MCP 完成保存、重启、重开及不传旧 options 再输出；核 native 参数/字段/关系，不只比较 YAML。完整 XML 检查不代替真正 Word 布局验收。
5. 新错误与动态文案覆盖现有 46 语言；完整 Rust/前端/仓库检查保留通过基线与首次失败，新候选绑定产品和 QA 后才跑实际流程。
6. `--locked --offline`；不加依赖、不启动外部应用、不直接运行受管 TeX 宏烟测、不触碰用户资料/配置、不发布。需要布局引擎时只通过现有受管理进程与隔离接口。
7. 未取证项目保持开放。该规格中的完整闭环全部实现并实测后才更新对应子范围；不能用前端入口、静态路由或不消费的模型结项。
