# 第三批本单元后的执行路径

**最新进展：** 有限局部字符四轴与实际编辑链已由candidate-68[实测并归档](../reviews/readmd-v005-batch3-math-native-runs-2026-10-24.md)。其后的来源相容性及 P04 出版参数按[新执行路径](v005-batch3-after-native-runs-2026-10-24.md)推进；下文保留此前规划和仍适用的保护要求。

**后续更新：** 下文第1项的有限文档默认值／普通 LaTeX混排已由candidate-66[完成本机实际闭环](../reviews/readmd-v005-batch3-math-document-defaults-2026-10-24.md)。其后的候选、接口和负控设计为实施前记录；下一实现优先级转为第2项的全局运行格式与非空pPrDefault来源，仍按属性证明后准入；首先按[局部字符四轴规格](v005-batch3-m07-native-run-styles-2026-10-24.md)实现。

2026-10-24。C61有限段落原生样式链已完成限定实际验证，见[结果与边界](../reviews/readmd-v005-batch3-math-native-styles-2026-10-24.md)。以下为后续实施与验收路径，保留各未完成项，不提前认证。

## 先补真实生产路径的断点

1. **无显式设置的普通 LaTeX 与导入 Word 公式混排。** 当前普通公式的默认 dispDef=true 与导入来源的显式false冲突，会在写目标前拒绝。下一单元先审查 math_defaults、文档数学上下文、阅读预览和各导出调用；把“未指定”与“显式默认／关闭”分开，不简单删除冲突检查。若采用文档级有效策略，必须由同一 ExportSnapshot／数学模型提供给阅读、矢量、PDF、DOCX及MathML消费者，原始源码仍保留缺省。先写双向顺序、单式与混合、不同全局属性、显式冲突、CRLF及保存重开的矩阵；同一来源链和普通正文不得被改写。无法证明消费者一致时保留现有明确拒绝，不只改DOCX放行。

2. **原生全局默认与运行格式。** 本单元只为获准的有限段落 pPr／basedOn 提供原生链；非空pPrDefault仍物化，run/character/theme、原样式显示名称、完整styles包和结构化alias/loss公开接口仍待做。全局默认需证明异源与模板相容，禁止给Normal补0或把defaults改名成数学基样式以冒充原来源。先按属性和实际可编辑场景拆分准入，区分保留、投影与可编辑互操作。

3. **保持同一基准做外部修改对照。** 原MD→已导native可能引入目标模板的run默认值；这与随后改native的因果必须分开。保留同一未改native的真实重导入、API保存、预览和两PDF控制，继续严格检查fill、所有字形／曲线、分数线及坐标。原→native默认变化单列，不把paint恒等或run/theme保真记为通过。后续任何回流项目复用该原则。

## 其余第三批工作保持独立账目

| 范围 | 下一个可验收出口 | 证据要求 |
| --- | --- | --- |
| P04 | Typst逐run基线的真实墨迹、碰撞和纸边证明；其后才开放非零值。其他字体轴、段落装饰、制表位逐属性推进。 | 同一实际引擎预览／导出、链接矩形、复杂嵌套与失败不覆盖；组件校准不能替代功能验收。 |
| CSL／出版 | 完整引用组合、局部参数和原生当前值回流；复用现有共享出版模型。 | 固定上游样本、作者手改保护、真实DOCX字段、PDF链接及EPUB读回；完整CSL和真实Word编辑继续待验。 |
| Word／WPS／LibreOffice／阅读器 | 原生打开、改变样式／字段、保存、重开和回流。 | 需要实际应用工作流；XML合法、CLI包检查或交叉编译不能代替。本次没有启动这些应用。 |
| TeX／离线资源 | 补齐对应材料闭包与正式可分发能力包。 | 来源、许可证、摘要、平台与回退证据；现有QA运行库不宣称正式资源包，复用启动不冒充受控冷态。 |
| 故障／压力 | 锁文件、空间／配额、取消、并发与退出恢复矩阵，然后八小时混合操作。 | 保留首败、真实文件、逐操作持久性及有界缓存；不将少量正常EOF解释为所有工作进程停止或取消延迟合格。 |
| 平台／国际化 | Windows实际工作流继续；46语言动态文案、RTL及布局需语义与原生证据。 | 本机缺少其他系统，保留未验收状态；键齐全不能代替翻译审校。 |

保持原toolbar／首绘／4K和boot保护；不另增场景界面。每次只固定一个可解决的语义单元，先规定输入／失败／保存重开和独立对照，再统一离线编译与实测。当前实现继续推进，不因外部应用或平台环境缺口停住，也不把缺证据项目销账。

## 资源纪律

只归档本任务的小型证据、可恢复源码增量和一份最终EXE压缩载体；多个QA候选共享同一产品摘要时不复制多份EXE。复用已核对运行库，排除assets覆盖树、profiles、capabilities、target和依赖目录。保留首次失败，归档先预检文件数及物理／展开体积。没有获得新的清理授权时不主动递归删除共享缓存。

## 下一单元的源码定位与最小合同

只读检查发现，`math_defaults.rs` 已有 Missing／Implicit／Explicit；缺口是 `from_xml` 将未指定dispDef立即投影为true，`DocxDefaultsMerge`随后只按有效值合并。C61材料还声明preSp=120、postSp=240、interSp=40、intraSp=60，只放宽dispDef不能打通实际材料。

- 继承资格先限无carrier的普通LaTeX `md_ast::Math/Inline::Math`，不把原生OMML、DSL和旧metadata的缺省全部变成通配。首批范围限dispDef=false与四个既有有界间距；其余全局差异严格拒绝。普通LaTeX独立使用保持现状，不在源码插入false或合成carrier。
- 全文先收集并验证声明，再生成不可变消费计划，顺序不能影响结果。当前没有名为ExportSnapshot的实现类型，真实公共入口是 `mdexport::publication_document`；应复用这一内容路径与数学模型，避免为了命名另造平行业务层。
- 作者MathDocument与渲染副本分离；字体检查、预算、SVG、MathML、DOCX、Rust PDF、Typst PDF、HTML/阅读及EPUB共用同一策略。不能让普通公式继承邻式Equation的pStyle/pPr。资源预检和目标文件写入之前必须完成上下文冲突判定。
- 接线审查包含math_document／upgrade_api／omml／math_layout、docx_writer、pdf_render、typst_writer／compile、epub_writer、html_math与math_word_projection。单式API须绑定已核对的文档身份、修订和公式范围，不能信任客户端任传mathPr；MCP契约同步调整。
- 阅读器 `assets/js/reader/formula.js` 当前缓存主要按公式源码、度量、字体epoch和metadata命中；必须加入document revision/context fingerprint，并覆盖请求、ready标记、缓存及异步过期丢弃。改JS后重新打包boot，同时保护既有toolbar/首绘/4K修复。

先建立自动矩阵：无配置单式保持；配置在前/在后；重复公式定位；四间距及真实折行；同源码但文档默认改变时缓存失效；CRLF、astral、stale与保存新PID重开。负控包括原生显式on/空dispDef与off、两个来源间距不同、mathFont/smallFrac/brkBin/brkBinSub差异、原生缺省不作通配、inline不供应显示专用设置，以及既有方向/嵌套宿主/超宽/namespace/重复属性/TeX限制。

实施分三段但共同验收：核心计划与反例→全部输出和API消费者→阅读缓存/boot与真实两顺序导出。DOCX、HTML、EPUB、两PDF仍用独立预期及严格完整绘制对照，保留普通正文和原生样式隔离。仅通过DOCX不得销掉此缺口；真实Office/阅读应用验收另记。
