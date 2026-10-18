# P07 CSL display 的递归布局投影

本单元补齐 2026-10-14 限定验收中明确保留的复杂 display；不改变旧候选、证据或 CSL RESULT，不宣称完整 CSL、Light 或 `cs:intext` 已完成。

## 范围与语义

依据 [CSL 1.0.2 Display](https://docs.citationstyles.org/en/stable/specification.html#display)：block 全宽；indent 为标准缩进块；相邻 left-margin/right-inline 构成共享标签宽度的两列；没有配对的两者为全宽块。多块、多个列行、块内列行及列内块均保留。透明处理器 Group 不构成布局边界，真实 block/indent 或非空普通行内内容构成边界；不越过这些边界去寻找另一列。连续 right-inline 属于同一标签的多个正文块。

`RichNode` 为不改写的受限处理器树，仍保留格式、链接、entry_index；正文来源 byte/UTF-16/revision/nonce 与布局无关。`RichLayoutNode` 是从该树动态生成的类型化流：Inline、Block（indent 布尔）、Columns（left/right 两个递归流）、Group（entry_index）、Link。Inline 内不含 display。可嵌套、可序列化；输出仍受现有节点、深度与字节预算约束。

`publication_rich::layout(nodes)` 生成递归流；`RichBibliographyItem::layout()` 把已有 first_field 映为外层 Columns，再递归处理两边；`layout_has_columns()` 供整个 bibliography 统一收集标签宽度。所有标签的最大真实宽度是 preferred width；各层 actual width 不超过各自父容器的 35%，保留 0.75em gutter、标签换行及完整正文。不能让深层列被强制使用外层绝对宽度；这是 CSL 未指定物理宽的自适应策略，不钳制用户参数。

受限 `layout_html()` 输出固定 `.readmd-csl-layout-columns/left/right/block/indent`，无 clip；复杂 reader 父 selector 是 `.readmd-bibliography.readmd-bibliography-complex`，服务端不伪造 measured-ready。初始 fit-content(35%) 是局部 intrinsic fallback；root 的 reader DOM 实测全部标签后写 numeric preferred-width 及固定 CSS 列模板，ResizeObserver/font-ready 后重算，过期结果丢弃。EPUB 无脚本路径使用 root-level intrinsic 共享 grid、nested 独立自适应，不冒称跨深度共同 preferred 已实测。

`normalize_display_columns()` 只把旧兼容的简单单对拆为 first_field/body；复杂输入保留原树并接受，不再为不能装进一个列对而拒绝。孤立列标记继续按全宽块兼容处理；已有 processor first_field 不重复提取。academic 在已有 rich 旁返回 layout，原树和原来源仍为真值，不把 layout 重新压成 Markdown。Markdown 兼容字段必须把 display 标为非精确保真。

共用 `layout_requires_flow()` 判定实际结构：纯行内内容、唯一纯行内列对、唯一非缩进的纯行内块保留兼容路径；多块、缩进、嵌套列以及包裹结构的链接使用递归路径。透明身份分组不触发升级。各 writer 和基础 PDF 的 JSON 分派使用同一判定，不能因一个孤立 display 标记而改变整个简单列表的旧列宽。

悬挂缩进按实际段落处理：每个全宽段落为 2em，所有列单元格及其后代关闭悬挂；结构容器不重复累加 2em。每层 indent 仍单独增加 4em。reader 使用固定段落 class 恢复 sanitizer 后的 block/hanging；阅读字号和字体变化时同样重新测量，不能只监听容器宽度。

## 接线分工

本代理仅修改 academic.rs/publication_rich.rs，必要时 publication_citations.rs 与专属测试/显式 QA example。root 接 md_ast、reader/CSS、HTML/EPUB、DOCX、Rust PDF、Typst 和 TeX：每个 writer 消费相同 item.layout()；在一条 entry 内递归输出块/列行，段距只加在完整 entry 之间，按 preferred/actual 规则布局。长正文允许分页，不能用不可分页的整条表格掩盖布局。

## 独立验收

官方 test-suite 固定 commit `6eefc5b07c6969ab8999e48542acbcc131cba864` 的五个 display_* 原始 RESULT/CSL/INPUT，仅缓存于 `.cache/private/publication-context-inputs/display-20261015/`，新 index 锁 bytes/Git blob/SHA；未知许可不进入仓库发行内容，旧 21 fixture/index 不变。AuthorAsHeading 验证全宽作者头+列行，DisplayBlock 验证正文+indent，SecondFieldAlignClone/MigratePunctuation 验证既有列对与标点，LostSuffix 验证后缀不丢。没有官方原件覆盖的嵌套/多行组合以 CSL 规范手工结构断言单独标注，不能冒称官方 RESULT。

纯单测覆盖块前后正文、连续/嵌套缩进、多列行、列内多段、透明分组/链接/全部格式/entry_index、孤立列、已有 first_field 的内外列、空节点、幂等和预算。源投影验证重复引用、emoji/CRLF、原 byte/UTF-16/revision 与 nonce 未变。官方五件由专属 `publication_display_oracle` example 显式传 private index 与新 private report 执行，不新增默认库 ignored 项，缺资产显式失败、skipped=0；fixture RESULT 结构与文字/格式由独立受限解析器读取，原始字节未改。最终须由 root 执行同候选真实 reader/各格式结构及 PDF 几何，纯投影测试不计跨格式或应用验收。
