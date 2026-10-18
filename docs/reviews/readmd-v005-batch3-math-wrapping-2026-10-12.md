# 第三批：长公式自动折行与精确编辑

共享 Rust 数学布局按实际可用列宽选择顶层二元／关系运算符断行，接通 `brkBin` 的符号前／符号后／两行重复三种方式，以及 `brkBinSub` 的 `--`、`-+`、`+-`。重复符号仅属于渲染投影，不重复写入作者源码。续行使用物理点值缩进或右对齐；缺省／值缺省缩进为 1440 twips，关闭显示缺省时忽略续行缩进／右对齐。行距同时考虑字体指标和实际墨迹，避免高分式遮挡下一行。

长 Office run 的源区域按实际行片段裁剪，同一节点保持唯一 DOM ID。就地编辑使用分别命中的片段，避免整个跨行矩形覆盖其他符号。区间索引用于裁剪，单项测试覆盖 19,999 个字形的长 run，不丢字符；这不是百万字、八小时或所有嵌套场景的性能认证。

数学测试 **137项通过**；完整内核 **2246通过、0失败、3项既有忽略，145.62秒**。正式 CSP／已安装 Edge 八项、零重试，52.0秒。Windows 原生四条流程、十组实际绘制坐标，覆盖七种长表达式、明暗／窄窗口、真实 AI 侧栏缩窄、阅读字号增大、跨行同一源 run 的分片命中、末行分母符号的精确修改、键盘撤销／重做、保存重开。

真实 DOCX 经独立 ZIP/XML 解析确认八个可编辑分式、所需三个折行设置，并且未把自动换行写成手动断行。Rust／受管理 Typst 共六份长公式 PDF 都为矢量、零栅格图，全部墨迹位于页面内。相对于各引擎同参数的短公式参考件，减号尺寸保持不变：Rust 宽约 7.211pt，Typst 宽约 6.556pt；两引擎默认字号不同，比较只在同引擎内进行。Rust 路径坐标取到 0.001pt，独立尺寸检查采用 0.003pt 的序列化误差界。明暗窗口和两套 PDF 页面经过视觉检查。专用坐标材料保留正常软件提示，不作为宣传素材。

保留全部中途失败及原因：新单测首先误用了 quick_xml 字节接口；原生测试按 DOM 顺序选到生成的运算符而非最后一个源码符号，诊断后改为绑定明确的节点／UTF-16 范围，并断言真实命中位置和输入值；下一次把 `-+` 误当正则，实际 DOCX 设置正确；独立 PDF 检查最初未计入坐标舍入误差。单独诊断的 `waitForFunction` 被正式 CSP 拒绝，改为普通定位器轮询，未关闭 CSP。没有自动重试掩盖这些失败。

固定证据见[归档清单](../evidence/capability-upgrade/2026-10-12/batch3-math-wrapping-manifest.json)：72文件、2,888,829字节，SHA-256 `ae262f698baab4635a682680a0f11038f7db0675c8efc07fcb600add7a932855`，Rust 源摘要 `fa4d1fb4357d7e7bfb0b3636d2dc150304fdb063f7178b8c4f3dfdf508dd810c`。前序98份证据摘要复核通过；本次没有新增可见文案，46语言键检查通过不代表语义审校完成。

构建锁定离线，复用单一 target 并关闭增量编译，目标目录实测仍为 **6.33GiB／2690文件**。固定 Typst 包仅按摘要引用；本轮自有选择副本核验后移除，隔离安装正常卸载。归档不复制 WebView 配置、能力整包或构建目录。没有递归清理、替换用户安装、提交、推送或发布。

第三批继续进行中。分式／根式／矩阵、括号组和普通文本目前保持不可拆，超宽的不可拆对象仍可能溢出；任意可拆 box／嵌套、继承的 Word 段落／字体／样式／修订、全部 OMML 语义和完整出版参数仍待完成。自然 DSL 生成的运算符尚无独立源码范围，不能把本次有映射操作数的点击测试当作任意符号可编辑。MathML／EPUB 的响应式折行策略、实际 Word 修改重开、独立 EPUB 阅读器，以及 B3-51–54 生命周期／正式 TeX／冷态和其他原生平台保持未验收。

依据：[Office 续行缩进](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.wrapindent?view=openxml-3.0.1)、[续行右对齐](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.wrapright?view=openxml-3.0.1)、[二元运算符断行](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.breakbinary?view=openxml-3.0.1)、[减号重复规则](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.breakbinarysubtraction?view=openxml-3.0.1)。自动断点由渲染引擎选择，未宣称与 Word 逐断点完全相同。
