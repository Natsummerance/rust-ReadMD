# 第三批：Office 公式段落、对齐与间距

修复同一个 Office 公式段落在导入时被拆散、矢量预览把多条公式横排的问题。段落保留身份及完整属性，共享布局纵向排版；局部四种对齐优先于文档默认值，前后距、式间距及相邻段距以实际点值换算。手动 run／box 换行按上一行起点或指定运算符定位，不改变数学优先级。显示默认明确关闭时不应用全局间距；继承 Word 段落设置尚未完整实现。

复用正文就地编辑，不新增常驻 UI。第二行符号按节点身份回写，保存迁移为可读 DSL，整次操作支持真实键盘撤销／重做。普通 TeX 投影保留多行，补齐 `gathered` 公式数组解析。DOCX 保留原生可编辑段落和唯一文档设置；Rust PDF 与受管理 Typst PDF 使用共享矢量几何及对齐。

MathML 生成实际行与点距，手动换行保留；按上一行第几个运算符定位仍不能等价呈现。实际 EPUB 导出返回 `math_manual_alignment_fallback`，46 种语言有独立译文，中文和 Arabic 经过实际警告格式化检查。这是明确的格式限制，不作为完整互操作通过。

数学128项；完整内核 **2237通过、0失败、3项既有忽略，142.17秒**；正式 CSP／已安装 Edge 严格检查八项、零重试，49.1秒。Windows 正式宿主六流程、十四组真实绘制坐标，覆盖七份专用 Word 输入、导入不写盘、四种对齐、手动对齐、保存重开、第二行符号修改及真实三格式输出。隔离配置安装固定离线 Typst 包、导出并正常卸载；选择使用的临时包副本在摘要复核后移除。未替换用户安装、未提交或发布。

独立 ZIP/XML 检查确认 Word 两个公式段落、三个可编辑公式、一个全局设置及 `alnAt=2`；EPUB 四行、9pt式间距和6pt／12pt前后距。两套 PDF 各一页、十三个矢量绘制、零栅格图，逐坐标检查右对齐和续行等号对齐。查看两份真实PDF渲染及明暗／窄窗口截图。QA 输入是坐标测量用简式，截图含测试标题和正常软件提示，不作为宣传片素材。

中途数学测试有一次新增断言失败：假设 `<m:oMathPara>` 不带命名空间属性。改用命名空间和节点数量验证，最终完整回归通过，原始失败日志保留。首次浏览器命令误用了根目录不存在的依赖路径，改用已安装的 `ui-tests/node_modules`；没有下载依赖、测试重试或 CSP 绕过。文档补丁初次上下文失配，没有改动文件，核对原文后重新应用。

固定证据见[归档清单](../evidence/capability-upgrade/2026-10-12/batch3-math-paragraphs-manifest.json)，103文件、11,781,995字节，SHA-256 `747f26a21ff9a2934ed759ca9c9ec5f9d882f8dbb35932e5a1ef55f2d6935462`。源摘要 `cea6afb34bed7d96e2e136783358f7bed32d58bec47de34d7f5422f869a315e7`；核对前序小分式33文件摘要，复用单一离线、关闭增量编译的 target。保留既有证据与 WebView 配置，没有递归清理。

第三批保持未完成。剩余包括完整 Word 段落继承、边距／自动折行、字体行高及任意嵌套手动换行／全部运算符、全 OMML 与出版参数、实际 Word 重编辑和独立 EPUB 阅读器。此前 EPUBCheck 缓存当前不可用，本次没有重跑认证。B3-51–54 的生命周期、传输／历史迁移、配额／磁盘满、正式 TeX 材料与冷态性能继续保留。非 Windows 平台和长期测试仍缺原生证据。

依据：[Office 公式对齐](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.justification?view=openxml-3.0.1)、[式间距](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.interspacing?view=openxml-3.0.1)、[相邻公式段落间距](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.intraspacing?view=openxml-3.0.1)、[手动换行](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.break?view=openxml-3.0.1)。标准语义投影与实际 Word 视觉验收分别记录。
