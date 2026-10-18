# 第三批执行方案与首个实施单元

<!-- publication-context-final:start -->
2026-10-14 共享出版元信息与引用上下文增量：P01/P07 核心已实施并完成下述限定 Windows 验证；R025/R026、完整第三批、其他平台及发布仍未整体验收。

P01 原生 16 组全部通过；(35 个逐字段案例＋2 个基线)×3 格式＝111 个矩阵文件，整套 121 个实际文件；40 个离线 EPUBCheck 零致命错误／错误／警告。 P07 原生 14 组全部通过，3 个实际输出；独立 oracle 20 必需通过（15 个官方 CSL test-suite＋5 个 Juris-M／citeproc-js 补充）、0 必需失败，1 个可选扩展未支持，整体 oracle passed=false。共享真实 stdio 7 组，不重复计数。 文献布局 4 个案例／20 个实际文件／12 个 PDF：reader 与 Rust、Typst、XeLaTeX 真实 PDF 几何通过；DOCX/EPUB 仅验证实际包布局参数／结构，不认证 Word／独立阅读器几何。字形 proof：typst 4 字面／72 字形；xelatex 4 字面／72 字形。

EXE SHA256：a231a2f545c002590454e47b8768f4ab0abc6573102265f576faf05ed54acf3c（70362624 字节）；boot SHA256：2610f804e9a5fb9114ec61b74aa7e7eec9af01e4b6baf14d15d08caf87c8908b。源码清单 SHA256：f6cbf2794aaecdb2d094d7fa142ef4f2ed4685ed5ee94c40cf9b54b48cca1754。

见[当前实现与验收范围](../reviews/readmd-v005-batch3-publication-context-2026-10-14.md)及[不可变证据清单](../evidence/capability-upgrade/2026-10-14/batch3-publication-context-manifest.json)。下方旧日期的“待执行／下一单元”和通过数保留当时历史范围，不是当前未实现结论，也不与本轮相加。保留完整 CSL 覆盖及 cs:intext 可选兼容、复杂多段／嵌套 display、Light、长注复杂分页、全部预设参数组合、可编辑引用来源往返、真实 Word／独立 EPUB 阅读器、其他平台与长期压力门槛。旧通用提示仍观察到中英混用；新增词条检查与46语言冒烟不能替代全界面语义审校。正式 TeX 包及独立冷态 120s 仍未认证；600s 完整性诊断或复用运行库的 ready=true 不构成性能通过。
<!-- publication-context-final:end -->


2026-10-13 字体几何增量更新：M01/M02 的分式、根式、重音与横线已接入实际 MATH 常量，并修复 PDF 标题／表格中公式继承字号的重排。最终同构建 Rust 2410 通过／0 失败／3 历史忽略，21 项 Windows 原生、4 项 MCP、24 组实际 PDF 路径对照通过。见[实现与证据](../reviews/readmd-v005-batch3-font-geometry-2026-10-13.md)。下方 M01/M02 描述保留规划时状态；实际 Word／阅读器互操作及完整第三批仍未验收。下一单元先审计 P01 共享出版元信息与 P07 完整引用上下文，再按规格实施。

2026-10-13 并行实施更新：M05/M06 限定间距／断行、MathML 脚本、P02 引用语言、P05 目录及超时清理已实现并取得最终同构建证据（Rust2388/0/3历史忽略，27原生、9MCP、12UI）。D04独立审计完成但8旧容器仍缺。见[本轮记录](../reviews/readmd-v005-batch3-parallel-semantics-2026-10-13.md)；下方原缺口快照不代替最新账本，完整第三批保持未验收。

日期：2026-10-12。承接 [47 项剩余细则](v005-batch3-remaining-implementation-spec.md) 与 [逐元素语义索引](v005-batch3-semantic-audit.json)，不缩减第三批范围。先固定接口、规范依据和可观察验收结果，再修改产品。此前冻结证据保持原样，后续增量独立记录构建身份。

## 执行顺序与资源使用

| 顺序 | 工作项 | 实现边界 | 必须通过的验证 |
| --- | --- | --- | --- |
| 1 | M03：矩阵最小列宽、空槽位 | 共用 OMML 属性解释；Rust 矢量、MathML；沿用高级属性编辑入口，不增面板 | 真实列位置及非截断；默认／开／关；精确源修改、撤销、保存、重启；DOCX、EPUB、PDF |
| 2 | M05、M06：微分、phantom 间距 | 先定义左右边界数学类别和嵌套传播，再接矢量与 MathML；透明间距与绘制可见性独立 | 各数学类别邻接、嵌套、显式关闭、脚本；独立几何预期；与旧断行测试回归 |
| 3 | M01、M02、M04、M07、M08 | 字体 MATH 常量、分式根式、对齐数组、全局默认值及字体回退；复用现有字体和源映射 | FontTools 独立常量夹具；复杂嵌套和不同字号；缺字与导出失败链路 |
| 4 | P01–P15 | P01/P07 核心已实施并取得限定证据；其余出版参数按原细则继续 | 当前实际输出见顶部；完整参数、应用／平台及第三批门槛保持 |
| 5 | M09–M15、I01–I10 | Word 上下文、UnicodeMath、源码／输入体验及 schema 组合；实际 Office、独立阅读器 | 官方／应用生成夹具与导入修改导出重开；不把 XML 合法算作 Word 验收 |
| 6 | D01–D07 | 同构建回归、压力、平台证据、文档及增量演示 | 保留失败记录；Windows 原生证据；其他平台缺证据仍待验，不虚报第三批完成 |

各单元先跑受影响的定向 Rust 测试，通过后跑完整内核、离线构建及真实宿主测试。长编译期间冻结 Rust 源码。只用 `.cache/community-upgrade/build` 一个 target，关闭增量编译；不下载依赖、不重建无关媒体资产、不反复复制全部历史证据、不改用户安装。

GitHub 资产的取舍沿用完整细则中的许可证审查：固定 Open XML SDK schema 用作语法与属性基线；MathLive 的槽位交互、UnicodeMathML 的文法与构造策略供学习；既有 STIX、Typst、Hayagriva、EPUBCheck 直接复用锁定资源。FontTools 仅作独立字体 QA。Pandoc、Calibre 的 GPL 代码不复制入本体。不为已有 Rust 模型另装前端数学真值引擎，也不引入重型系统 Office 依赖。后续确有必要增加组件时单独记录固定版本、许可证、平台和体积。

## 首个实施单元：M03

### 规范与接口

- [Microsoft cSp](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.columnspacing?view=openxml-3.0.1)：列宽为 `max(本列最大内容宽度, cSp/20 pt)`，相邻列另外加 `cGp`。`cSp` 不能误作额外间隙；字号变化不把绝对 pt 变成 em。
- [Microsoft plcHide](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.hideplaceholder?view=openxml-3.0.1)：省略显示；空开关或 true 隐藏；false 显示。标准不限定槽位外观，采用细线空框，不产生正文字符。
- `omml::matrix_column_minimum()` 返回保留单位的 `MathLength::Points`。`omml::matrix_placeholders_visible()` 共用开关解释。`omml::empty_argument()` 只识别实际空参数，不把隐藏 phantom、空格或有结构的表达式算作空槽。
- Rust table 先生成单元格内容与空槽框，再计算各列最大宽度与下限，最后对齐；每个参数保留原始节点身份与可点击源范围。隐藏空槽仍可选中，不从文档树删掉。
- MathML 使用单元格 CSS `min-width:<pt>`，保持原生 `mrow` 与编辑标识；不使用 `columnwidth` 冒充下限。[MathML 3 columnwidth](https://www.w3.org/TR/MathML3/chapter3.html#presm.mtable) 是固定宽度，允许裁切，不满足本任务。规划探针已在本机 Edge 验证 `mtd min-width` 会影响实际宽度；正式验收仍检查产品生成的 MathML，不能用此探针替代。
- MathML 空槽用不含文字的 `mspace` 配合 CSS 边框，尺寸与矢量空框一致。不插入方框 Unicode 字符，不污染复制／检索。PDF、HTML、EPUB 均消费同一属性策略；DOCX 保留原生 `cSp`／`plcHide`。
- MathML 回流识别统一的单元格 `min-width` 及本程序生成的无文字空槽，恢复 `cSp`、`plcHide` 与相对参数字号。不同单元格下限、混合空槽可见性及其他 `mspace` 明确给出投影诊断，不静默丢弃或冒充等价。
- 原生实测发现 MathML Core 忽略 `columnspacing`，而全局 `border-box` 将间隙计入最小宽度。实现明确使用单元格 `content-box`、内部相邻边各半间隙、外沿零间隙和列对齐；验收检查内容起点的实际列位置差，不用固定列宽裁切补偿。格式化 XML 缩进与 CSS 属性空白也纳入输入边界。
- 现有 `office.mPr(office.cSp(val: ...), office.plcHide(val: ...))` 足以表达本单元；不增加新语法或可见文案。旧文档无最小列宽仍按内容布局。
- 规范允许空 `cSp`，而固定 SDK schema 要求 `val`，该冲突保留为 M15 待裁决。渲染读取已导入的缺值时按 0；不放宽全部必填属性校验。本单元使用显式 0／非零值完成可编辑流程。

### 编码与测试清单

1. 共用属性解释及空参数判断，测试省略、0、非零、最大合法值、开关四种拼写及命名空间；精确 DSL 往返保留开关。
2. 矢量布局测试：两列下限 36pt、间隔 6pt 时同边位置差 42pt；12／24pt 字号、左右／居中、超宽内容不裁切、空／非空／隐藏槽、嵌套矩阵；实际 draw 输出随属性改变。
3. MathML 测试：真实 CSS 单位、无错误固定列宽、空槽无文本、隐藏时不输出提示；实际浏览器生成几何下限与超宽不裁切。
4. 原生正式 CSP：用专用矩阵讲义点击高级属性，修改重复公式中的一处，检查源码映射、一次撤销重做、原子保存无旁文件、退出重启与再次编辑空槽。
5. 实际导出 DOCX、EPUB、Rust PDF；核对原生属性、MathML 几何、离线 EPUBCheck；PDFium 打开和渲染。1024×680／1160×820、深浅色目检。实际 Word 往返与独立电子书阅读器仍是 I 组单独门槛，不写成已通过。
6. 完整 Rust 库测试、离线锁定构建；既有矩阵行距、数学字符编辑、HTML 数学及 EPUB 回归；i18n、资源、连线、样式、boot 新鲜度和验收账本检查。

失败结果保留，修复后再运行并记录原因；不通过反复重试取得“成功”。本单元通过只收口 M03 对应产品链路，不能替代 19 类对象全属性或第三批整体认证。

## 下一实施单元：出版元信息与完整引用上下文

2026-10-13：M01/M02 候选及证据已冻结；新增千公式容量检查与清理记录独立归档。P01、P07 已完成代码缺口核查，具体契约分别写入[出版元信息规格](v005-batch3-publication-metadata-spec.md)与[引用上下文规格](v005-batch3-citation-context-spec.md)。这些是待执行规格，不能计入产品完成数。

| 顺序 | 所有权与操作 | 出口 |
| --- | --- | --- |
| 1 | `publication_sol`：共享元信息解析、校验、覆盖、原文范围及前置内容模型；`math_sol`：同版本引文引擎补丁审查、引用语法、受限富文本和源映射。独立文件可并行，先不占用 writer 或运行 Cargo。 | 两份领域模型和独立预期夹具就绪；无新依赖或 vendoring 在来源、许可证、大小及锁定策略未审查时进入产品。 |
| 2 | root 协调共用 `md_ast`、导出入口、路由和 MCP；逐一移交 DOCX／Typst／EPUB／LaTeX／PDF writer，单文件只有一个写入者。 | 元信息和引用进入同一出版 AST／导出快照；不靠额外 PDF 重写器或有损 Markdown 拼接绕过现有模型。 |
| 3 | 现有文档属性／导出高级区接通结构化参数，引用复用正文与已有文献入口；新可见文本按 46 语言流程完成。 | 未修改字段与正文原字节保留；明确清除／继承；真实编辑、撤销、预设保存及重开，不增加常驻工作台。 |
| 4 | 冻结源码后 root 顺序执行受影响 Rust 测试、完整库与离线锁定构建；使用丰富论文和书稿进行同构建原生及 MCP 检查。 | 检查实际 DOCX core/custom／可编辑前置内容、PDF Info/XMP／分页、EPUB OPF／离线验证，以及引用源身份和官方独立期望。失败修复后形成新候选，不覆盖当前数学档案。 |
| 5 | 独立记录实际 Word／阅读器检查、长期压力和平台缺口；归档输入输出，核验后清理本单元拥有的临时资源。 | P01／P07 逐项满足规格后才收口；完整第三批、平台和发布状态仍由总账本判断。 |

引用引擎的作者隐藏、前后说明、定位符、排序／合并、注释距离和富格式必须一起评估。现有上游测试跳过的场景不能当作通过依据；公开 API 中“仅年份”不能代替“隐藏作者”。新模型先保留原始引用范围，再生成只属于该次出版投影的结果，不能按显示文字寻找正文位置。
