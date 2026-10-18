# 第三批增量：科研图表、三线表与实际分页

本次属于 V0.0.5 开发实现，未安装到用户主配置、未发布。第三批及全项目仍未整体完成。科研规范作为可编辑配置，不以“内置渲染器存在”冒充全部图形达到投稿标准。

## 入口与操作

编辑 → 插入图表 → Vega-Lite → 五种科研模板 → 替换演示数据。包含明确上下界的 95% CI、散点/OLS、箱线图、零中心热力图及函数曲线。导出 → 学术预设同时采用科研图样式与三线表；在原有折叠设置中修改宽度、字体、表格线或保存自己的预设。默认 89mm、7pt，可调整为 183mm；不自动修改数据、坐标范围及统计语义。

标题紧邻 LOGO，占用实际所需宽度并设上限，长名省略，图标紧凑排列。编辑和阅读设置采用与搜索/导出一致的无边框 44px 控件；阅读设置在编辑按钮右侧，只在阅读已打开的 Markdown 时出现，主页和编辑态隐藏，并关闭残留面板。

## 实现与修复

Rust 科研模块统一配置、离线渲染及源码/参数身份。图形从原始描述生成，Vega/Vega-Lite 与 Graphviz 可由后端直接渲染；浏览器引擎输出受源码摘要绑定的静态快照。相同图复用有限内存缓存，尺寸/源码变化使旧快照失效。原稿与目录不被改写。

DOCX 保留 SVG 原稿与高分辨率 PNG 兼容图，写入真实印刷尺寸。Typst PDF 保留矢量、可选择文字及嵌入字体；Chart.js 明确使用约 600dpi 位图，不能宣称矢量。EPUB、HTML 保留自包含 SVG。普通 Rust PDF 的 SVG 仍走位图，不能称所有 PDF 引擎都输出矢量。

发现并修复的生产问题：Typst 段落内的块级图片被引擎警告后忽略；共享图片解析不接受生成的 SVG data URI，导致图变成替代文字；Mermaid 导出仍包含 HTML 标签/动画样式；Vega 被拒绝的数据依赖可能只记录错误却输出空图；重复 PDF 表头不占高度导致首行覆盖；新科研参数空值落到最小值；HTML 预览原先显示图表 JSON；SVG 失败信息可能显示机器码。现在分别使用正确图片容器、受验证的 SVG 图片链路、静态 SVG 标签/样式、失败拒绝、表头占位、89mm/7pt 默认值、共用物化流程和现有多语言错误文案。

没有放宽 SVG 安全限制。网络/本地文件依赖、脚本、外部样式仍被拒绝；渲染失败不能替换已有文件。Word 自身能否打开和往返不由 XML 检查代替。

## 证据

最新 Rust 库回归 **2019 通过、0 失败、3 项原有忽略**，260.29 秒，离线锁定构建。正式 CSP 界面全组 20 项通过；之后只修正 SVG 错误本地化，科研 6 项复测通过（其中 5 项重叠，共 21 个不同用例），均无跳过或自动重试。

Windows 原生共 10 条检查，使用隔离配置与合成材料：三次新装离线 Typst 包及真实 PDF 探测；A5 双栏、文献、脚注、公式、代码、表格和插图；同一份 8 图材料在 PDF/DOCX/EPUB/HTML 输出；8 个 DOCX SVG/PNG 配对及 89mm 位置；120 行原生 PDF 表格分为 8 页，重复表头不与正文重叠；失败保护范围内原稿和已有结果不变。此项不冒充完整取消压力与八小时压力测试。

独立 PDF 检查找到七张矢量图及一张刻意保留的 Chart.js 位图。实际示例轴标题为 Arial 7pt，字体嵌入，文字可选择。Typst PDF 预览与导出字节一致。官方 EPUBCheck 5.4.0 离线验证科研 EPUB：零致命错误、错误、警告。

- [内核回归](../evidence/capability-upgrade/2026-10-10/batch3-scientific-rust.json)
- [界面](../evidence/capability-upgrade/2026-10-10/batch3-scientific-ui.json)与[最后错误本地化复测](../evidence/capability-upgrade/2026-10-10/batch3-scientific-error-ui.json)
- [Windows 原生流程](../evidence/capability-upgrade/2026-10-10/batch3-scientific-native.json)
- [EPUBCheck](../evidence/capability-upgrade/2026-10-10/batch3-scientific-epubcheck.json)、[PDF 字体检查](../evidence/capability-upgrade/2026-10-10/batch3-scientific-figure-inspection.json)
- [实际 PDF 渲染的 4K 图](../evidence/capability-upgrade/2026-10-10/batch3-scientific-figure-4k.png)
- [失败尝试与原因](../evidence/capability-upgrade/2026-10-10/batch3-scientific-attempts.json)

旧黄金测试的两项失败由无条件追加空 CSS 换行引起，已恢复兼容；UI 测试中过严的像素取整假设及选择不存在选项已修正，实际隐藏失效另行修复。Python 旧模块将弃用警告写到 stdout 的检查工具问题改用正式模块解决；并未放宽矢量、像素、表格或旧文件保护断言。

## 边界与下一步

全 OMML/DSL 与实际 Word 往返、全部出版参数和局部覆盖、完整 LaTeX 模板沙箱、每个图形引擎的复杂内容与复合图缩放诊断、后端浏览器引擎快照交接、独立电子书阅读器及其他平台继续未验收。46 种语言新增词条有独立翻译与一致性检查，仍不等于全语言人工语义认证。

账本新增 R073 科研图与 R074 三线表，74 项总体仍为 17 pending / 57 in_progress / 0 verified，保持完整项目门槛。下一步继续第三批：补出版参数能力与损失诊断、LaTeX 模板沙箱，以及全 OMML 对象/属性与真实 Word 证据；不能因本增量通过而标记全批完成。

此前自动审批拒绝批量清理隔离测试目录；本次未绕过拒绝或删除目录。约 1005MiB 分类记录包含此前 Typst 尝试、当前隔离测试与可复用离线引擎，非主程序新增体积。见 [缓存分类](../evidence/capability-upgrade/2026-10-10/batch3-scientific-cache-inventory.json)。共享构建缓存、草稿、恢复和用户文件保持原状。

操作与各引擎边界见 [科研图表说明](../guides/scientific-figures.md)。规范依据为 [Nature 图形说明](https://research-figure-guide.nature.com/figures/building-and-exporting-figure-panels/)及 [APA 表格说明](https://apastyle.apa.org/style-grammar-guidelines/tables-figures/tables)，不是全刊物认证。
