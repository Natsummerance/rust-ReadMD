# 第三批增量：共享出版版式与独立 Word 预览

本次继续第三批数学与学术出版，未缩减完整合同，未发布主程序或替换用户安装。R028/R029/R006/R071/R075 保持进行中；实际 Word 往返、完整 Office 数学、局部覆盖、LaTeX 沙箱和平台矩阵没有据此结项。

## 改动

Rust 新增共享 `publication_layout`，统一参数缺省、边界、分栏几何及页眉页脚冲突检查。旧预设不补写无意义的新键。DOCX 使用原生节、样式和字段；Typst 使用同一参数生成真实分页 PDF。封面/目录保持全宽，正文可分栏，图片及表格适应栏宽。

新增分栏/栏距、装订预留、镜像页边距、首页页眉页脚规则、运行文字距离/字号、五种页码格式及起始值/总页数、孤行控制、段中不分页、标题分页和脚注字号/行距/编号。界面复用现有折叠设置，高级标题设置逐级展开。非支持引擎隐藏控件，API 跨引擎参数显示本地化损失提示。

修复 Word 预览继承 PDF 页旧引擎的问题：基础 DOCX 和投影不要求 Office 或 TeX。已安装并通过探测的 Typst 可改善投影，安装改变后缓存失效。预览明确标注实际投影引擎，不能当成 Word 原生排版。真实 Word 程序仅用于兼容性验收。

导出设置切换语言时同步刷新预设、参数和警告，保留折叠状态；导出前取消过期预览。26 个新增词条实际生成并同步到 46 语言，检查占位符及中文/德文/阿拉伯文原生布局，未宣称 46 语言人工审校完成。

## 实测

证据索引为 [batch3-publication-layout-manifest.json](../evidence/capability-upgrade/2026-10-10/batch3-publication-layout-manifest.json)。全部材料为专用合成文档，Windows 原生测试使用正式 CSP、独立配置和真实文件接口，没有读取私人文档或覆盖当前安装。

- [完整 Rust 库](../evidence/capability-upgrade/2026-10-10/batch3-publication-layout-rust.json)：2029 通过、0 失败、3 项既有忽略，251 秒。忽略理由保留，不把它们当通过。
- [界面回归](../evidence/capability-upgrade/2026-10-10/batch3-publication-layout-ui.json)：27 通过，0 失败、跳过、重试；覆盖预设、出版引用、EPUB 与科研输出。
- [Windows 原生 10 项](../evidence/capability-upgrade/2026-10-10/batch3-publication-layout-native.json)：无 Office/TeX 的 Word 预览、离线安装后的缓存切换、控件/默认值与预设重启恢复、独立解析 DOCX、15 页实际 PDF、五种页码格式、分页开关实际效果、错误保留旧输出、跨引擎提示及三语言窄窗。
- PDF 经 Poppler 独立读取文本位置、页数和字体：两栏、物理奇偶页交替、装订预留、起始 VIII/总 15 页、脚注在引用页，字体嵌入；预览与最终 PDF 的 SHA-256 一致。另五份 PDF 从偶数页码 8 开始，物理左右页与逻辑编号保持独立。
- DOCX 经独立 ZIP/XML 检查，封面单栏、正文双栏、页码延续、原生字段/脚注/样式及列宽存在。Python 仅用于 QA，不进入产品运行链路。
- 小纸张边界材料六份真实 PDF 分别开关标题与下段同页、正文段中不分页及孤行控制。独立读取标题和五行正文的页位置，确认孤立标题、跨页段落和单独首行实际发生对应变化；这些是参数生效检查，不是视觉宣传素材。
- [本机 Word 探测](../evidence/capability-upgrade/2026-10-10/batch3-publication-layout-word-probe.json)：45 秒未启动或完成自动化，仍为未验收。该次输入摘要单独保留，不能把 XML 检查当成 Word 往返。

![中文窄窗口高级参数](../evidence/capability-upgrade/2026-10-10/batch3-publication-layout-controls-zh-CN.png)

![真实双栏与脚注 PDF](../evidence/capability-upgrade/2026-10-10/batch3-publication-layout-page-02.png)

## 失败与限制

[尝试记录](../evidence/capability-upgrade/2026-10-10/batch3-publication-layout-attempts.json) 保留早期失败。初版 Typst 生成使用了不支持的逻辑符号，实际编译失败后修正为引擎语法，再完整验证；没有删除这次失败。最初 DOCX 检查器受本机 PowerShell 未签名策略阻止，改用独立标准库检查器，没有绕过执行策略。一次 Playwright 命令参数错误未启动测试，与产品测试失败区分。

本次不是全出版模型认证：任意节、尾注、全部字体/段落/题注属性、局部覆盖与正文点击回写仍待完成；Word/WPS/LibreOffice 各自排版、其他平台、全语言语义及八小时混合压力没有完整证据。引用、数学、科研图和既有预设各套测试有重叠，不相加成独立功能数量。

后续先完成共享出版模型中的局部覆盖与源映射、其余脚注/尾注/段落参数，再推进完整数学语义及 LaTeX 离线能力与沙箱。对轻型核心与重型受管理包继续使用[既定交付链路](readmd-v005-capability-delivery-2026-10-10.md)。

缓存按隔离测试、可复用构建和日志分类，见[增量缓存清单](../evidence/capability-upgrade/2026-10-10/batch3-publication-layout-cache-inventory.json)。本次没有进行此前被自动审批拒绝的批量缓存删除，也没有清理用户历史、草稿或文件。

参数及使用方式见[版式指南](../guides/publication-layout.md)。依据为 [Typst 页面](https://typst.app/docs/reference/layout/page/)、[Typst 标题与块](https://typst.app/docs/reference/layout/block/#parameters-sticky)、[Typst 脚注](https://typst.app/docs/reference/model/footnote/)及固定 [OpenXML 3.0.1 schema](https://github.com/dotnet/Open-XML-SDK/blob/v3.0.1/data/schemas/schemas_openxmlformats_org_wordprocessingml_2006_main.json)。
