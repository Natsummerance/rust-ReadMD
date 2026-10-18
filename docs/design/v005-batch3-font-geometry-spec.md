# 第三批 M01／M02：字体驱动公式几何

日期：2026-10-13。承接 47 项剩余细则；本单元不替代完整 OMML、出版及应用互操作验收。

实施前基线：2388 项 Rust 测试通过、0 失败、3 项历史忽略；上一候选与证据见 `batch3-parallel-semantics-manifest.json`。只复用已锁定的 ttf-parser 0.25.1、STIX Two Math、字体/PDF 能力包及 FontTools QA，不新增产品依赖，不下载构建资源。

| 工作项 | 具体实现 | 可观察出口 |
| --- | --- | --- |
| M01 无横线分式 | 独立使用 StackTop/BottomShift 的行内／独立常量；不足 StackGapMin 时调整两个参数，不能借用横线位置与间距 | 独立字体夹具给出基线与最小墨迹距离；深分子、高分母、嵌套、smallFrac 及物理字号均验证 |
| M01 斜分式 | 使用 SkewedFractionHorizontalGap/VerticalGap，单独安排分子、分母与斜线；保留 linear 与普通分式的意义 | 测净间距、斜线伸展与界限；重复公式源定位、切换类型、撤销和保存重开 |
| M02 根指数 | 字体百分比定位指数墨迹底部；使用正／负前后 kern 并防止短指数使根号负向越界 | 长、短、带下伸部分的指数；隐藏／空指数不留空白；高根式和字号转换 |
| M02 上下横线 | 各自使用 VerticalGap、RuleThickness、ExtraAscender/Descender；额外空间只影响逻辑边界 | 实际 SVG/PDF 规则厚度、墨迹间距和布局边界；嵌套横线保留合理间距 |
| M02 重音 | 基底和所选重音字形的 TopAccentAttachment 水平对齐；复合／装配使用明确的几何回退；AccentBaseHeight 控制升高 | 独立字形附件数据，斜体短基底、宽／高基底、伸展装配；墨迹无裁切、命中范围不串 |

规范依据为 [OpenType MATH 常量与附件](https://learn.microsoft.com/en-us/typography/opentype/spec/math)；根指数的负 kern 夹限参考 [MathML Core 根式布局](https://www.w3.org/TR/mathml-core/#root-with-index)。OpenType 规定常量与附件含义，并不规定 ReadMD 与 Word 的每个像素必须相同。字体没有对应数据时保留有说明的回退；标准元素与属性仍由既有统一 OMML 树、DSL 和无损源码管理。

两名 GPT-6.1 Sol／max 子代理分别负责 M01 产品实现与独立字体 QA；主代理顺序修改 M02，避免同时写 `math_layout.rs`。产品源码冻结后集中离线测试／构建；原生专项必须绑定同一 EXE、boot、字体及脚本摘要。

验证顺序：字体 oracle 摘要／数值 → 定向 Rust 几何测试 → 完整 Rust 基线 → 离线构建 → 正式 CSP 的真实 Windows 宿主操作 → 实际 DOCX／EPUB／PDF 输出与重开 → 浅／深色截图人工检查 → 仓库检查和账本证据。覆盖 12／24pt、显式 Office 字号、行内／独立、两级嵌套、空属性、显示／隐藏与 undo/save/reopen。不以 XML、EPUBCheck 或 ReadMD 自己重开代替 Word、独立阅读器应用验证。

预览 SVG 与基础 Rust PDF 共用同一排版；EPUB 的 MathML 和 Word 的 OMML 由各自阅读器排版。输出中的结构保真与实际应用中的几何互操作分别记录。尚未实现的字体 flattening/复杂文字整形、其他 M/P/I/D 项，以及其他平台的原生证据继续保留，不从第三批范围中删除。

实施复核补充：冻结字体的 wide hat 只有有限变体、没有装配；超过最大宽度时只拉伸字形的横向变换，纵向高度保持设计值。斜分式超过最大 slash 变体时采用明确的均匀放大回退，可能增厚笔画，不能据此声称与 Word 像素等价。两种回退均进入几何与实际 PDF 路径检查。

隐藏 phantom 仍参与结构布局；其 `show` 与 `zeroAsc/zeroDesc/zeroWid` 独立。布局用内部有效尺寸标记覆盖该子树的绘制界限，最外层 phantom 拥有自己的零尺寸开关；普通横线的额外留白不当作墨迹重复计算。即使只剩一个可见字形，含占位的表达式仍使用复合基底附件规则。新增显示／隐藏、零尺寸、嵌套和真实窗口回归。
