# 第三批：文档数学默认值与普通公式混排

状态：candidate-66 文档默认值有限单元通过，见[实测报告](../reviews/readmd-v005-batch3-math-document-defaults-2026-10-24.md)。基线为 C61；原生 GUI／外部应用、完整 M07及第三批的其他未验收项继续保留。

## 解决的问题与边界

C61 导入的 Word 公式显式关闭 `dispDef` 并声明四个显示间距，未配置的普通 LaTeX 被过早解释成固定默认值，两者混排不能导出。本单元只允许无 carrier 的普通 LaTeX 继承已核对的文档策略：`dispDef=false` 及 `preSp/postSp/interSp/intraSp`。原生 OMML、DSL 和旧 metadata 的缺省不变成通配符；其余全局差异、显式冲突、嵌套宿主与 TeX 限制继续拒绝。

作者源码、节点身份和稀疏 Office 声明保持原样。渲染副本不能取得邻式的 Word 段落、样式或修订信息。无继承来源的普通公式维持既有行为。配置在前和在后必须得到同一策略。

围栏可合法地紧跟正文而无空行。内部临时数学展开必须保持块边界，不能让正文吞并显示公式而丢失 Office 载荷；也不能靠给测试材料添加空行绕开这种输入。原始 Markdown、展开后 AST、渲染 JSON 和真实导出都须保留其显式声明与冲突行为。

## 核心合同

`math_document_defaults::DocumentDefaultsPlan` 全文收集后一次校验，提供 `analyze`、`fingerprint`、`is_active`、`render_document` 和 `render_formula`。作者文档与派生文档有明确身份检查，不能把已投影副本再次当作者输入。按数学 occurrence 序号、display 和原始数学身份验证，禁止按重复文字查找。

DOCX、Typst、Rust PDF、EPUB、独立 HTML 与预检在各公共入口使用同一计划。预检在资源读取、任务暂存及目标写入前发生；下层写入器不绕开冲突检查。JSON 消费者仅在渲染副本中修改数学节点，保留全部非数学字段。

## 单式 API 与阅读器

增加 `POST /api/math/context` 及 MCP `readmd_math operation=context`。全文注册校验 `expected_revision`，返回有界上下文 handle、全文修订、策略摘要和精确 UTF-16 occurrence 范围。Runtime 上限 8 个上下文、64 MiB 计费内存（包括源码和模型副本）、每篇 4096 个公式，并设过期和淘汰；进行中的引用仍计入预算。

parse/patch/serialize 的可选 `document_context` 绑定 handle、修订、摘要和真实范围。一旦提供，不允许忽略或降级；源码、方言、display、相邻 metadata、过期和伪造范围均须验证。表达式编辑可沿用已核对策略，提供方声明改变则拒绝旧策略并要求重新分析。返回作者 MathDocument，预览、MathML、字体及预算检查使用派生副本；序列化不将继承值写入作者源码。

源范围由真实 Markdown offset 事件与 DSL 范围产生，并核对实际 AST 数学序列；无法证明映射则明确拒绝，不使用首个字符串匹配。HTML 共用这一源分析，不另建近似扫描器。

阅读缓存、排队请求、DOM ready 和错误绑定文档修订、策略摘要、公式范围、metadata、视图身份、字体 epoch 与尺寸；请求前、缓存写入前和异步渲染后均检查过期。同式未变但全文设置改变也要失效。保持现有 4 workers、首批 15、350px 懒载和 32 项缓存；修改 JS 后离线重打包 boot。

原文与出版处理后的视图分别分析，逐 occurrence 核对完整作者模型，然后由服务端产生 nonce／ordinal 槽位。MCP 的 `readmd_academic_document view:true` 使用真实出版投影；引用替换和分页不能让点击位置退回首次文字匹配。注册失败保留有明确错误的可读源码，不悄悄切到忽略文档默认值的单式渲染。

普通 LaTeX 的源节点 ID 与 XML／矢量热点 ID 不相同，本单元不猜测二者映射。混排中的普通公式使用现有就地输入区编辑整条表达式，每次从原文精确 body 范围构造候选全文、重新注册和解析；不新增常驻 UI、不转为 DSL、不回写继承值。取消不改正文，提交仍检查原文修订。DSL 和已支持的原生公式保留原逐槽位／字符编辑路径。普通宏的逐字形源映射、非连续嵌套 DSL 的就地修改及带 opaque 载荷的完整编辑仍另列欠项。

## 验证矩阵

- 正向：独立普通公式不变；配置前后两顺序；重复式；四间距；inline 不供应 display 参数；CRLF、astral；作者源码和普通正文不变；保存及新进程重开。
- 反向：原生显式 on／空 dispDef 与 off；两组不同间距；mathFont、smallFrac、brkBin、brkBinSub 等未准入差异；原生缺省；namespace、重复属性、方向、超宽、嵌套宿主和既有 TeX 限制。
- API／前端：同式异文档、异修订、伪范围、carrier 改变、handle 过期、普通表达式 patch、提供方 patch、serialize、旧成功／旧失败响应均不能污染缓存或 DOM。
- 输出：真实 DOCX 可编辑 OMML、HTML、EPUB MathML 和两种 PDF；两个顺序各与手写显式设置的控制式／PDF比较。检查普通行内公式、完整正文、spine 中的真实 MathML 结构、可见 SVG 墨迹、PDF 全路径和真实文字字形位置／样式。源节点身份不作为绘制身份；只将 SVG 的 `data-math-owners` 从绘制比较单列排除，原始文件保留。真实 Office 与阅读应用互操作另记，不以包检查代替。
- HTML 必须解析实际模板选中的 Markdown JSON 源，逐个校验七个公式的标签、可访问名称和对齐，再把每个 SVG 与同一上下文、自然宽度的 API 向量比较。空外框、缺路径、隐藏外框和错序必须失败，数量不能替代绘制验证。
- PDF 的不可见公式复制层单独核对：Rust 仅在全文精确七条来源文本匹配后，验证唯一专用 Type3 字体及其不绘制的字形程序；规范化花括号不改变可见数学比较。可见使用、未知字体、额外或缺失隐藏段均拒绝；Typst 及默认严格模式不适用此例外。可见字形、嵌入字体、位置和全部数学绘制仍严格对照。
- 实际证据逐项绑定输出矩阵、来源摘要、导出请求、响应、两进程原始 stdio/events 及每次独立检查器的参数和 stdout／stderr；同一文件或成功记录重复凑数不能验收。
- 工程：保留 C61 首次失败及证据；绑定新候选、离线编译、完整 Rust／前端／仓库检查，保护 toolbar、首绘和 4K 修复。不通过缩小整批范围或修改旧证据完成验收。

## 并行所有权

核心代理负责新计划与核心反例；上下文代理负责源分析、Runtime、API／MCP 与 reader；root 负责模块注册、各输出入口、HTML、JSON 桥、离线构建和实际输出验收。没有完成证据前不更新完成状态。本单元不发布、不替换安装、不启动 GUI、不删除共享缓存。
