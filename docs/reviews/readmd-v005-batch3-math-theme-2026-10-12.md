# 第三批：Word 公式主题色与视觉编辑格式保留

本次完成 B3-55／57 的一个子集。主题名称不再只保存在 DSL 中、渲染却继续采用过期 RGB：实际 DOCX 主题和设置关系经过命名空间检查，按公式及其样式引用捕获最小调色板。可读 `drawing.clrScheme`／`word.clrSchemeMapping` 保留原始主题属性，预览和导出副本才计算有效颜色，不修改作者源码。

支持 sRGB、系统色 lastClr、角色映射和 Word HSL tint／shade；tint 优先。主题缺失或关系为外部地址时，使用生产者已保存的 RGB，不重复施加 tint，不读取系统主题或远程资源。重复调色板、非法映射、缺失本地主题及错误命名空间明确失败。完整 DrawingML 色彩类型／变换和主题字体仍未实现。

真实桌面检查发现了另一处产品问题：在预览中替换分子，会连同原运行的显式 Word 格式一起删除。现将内容替换与高级源码替换区分：视觉槽位展示去除运行格式后的输入；单一运行的颜色、字号等格式保留在模型中，替换内容时继承，明确指定的新格式优先。混合运行没有统一格式时不擅自套用第一段样式。普通 DSL 保留作者的原始写法、注释及原有局部替换保护。

同一最终构建 SHA256 `fd48f19fd0e9395ee279be4189d94a50cc69ca450f27e4f1a2afe6da4de98063`，前端包 `77ae6f7c5cb76cdf9436c512c7be7cc5a6736ecbf43d7903d1d7d7d73b60e577`，锁定离线构建，无新增依赖：

- 完整 Rust 内核：2279 通过、0 失败、3 项既有忽略，145.45 秒；数学专项170通过。
- 严格 CSP Edge：8 项通过，零自动重试，49.2秒；实际 Windows 主题流程6项、既有控件6项、自然运算符4项、MathML／EPUB4项全部通过。
- 实际 stdio MCP：4项主题解析、可读 DSL／OMML 往返、无效上下文及只读检查通过。
- 五份专用 DOCX 验证绝对包路径、不同命名空间前缀、自定义设置路径、缺失及外部主题；预览不创建 MD 或资源。保存迁移、只改第二个重复公式、一次撤销／重做、原子保存及重开检查通过。
- 实际 Word 输出前两个公式包含有效 RGB 且没有悬空主题引用；EPUB MathML 包含相同颜色。EPUBCheck 5.4.0 对实际书籍零致命、零错误、零警告。
- 独立 PyMuPDF 检查一页 PDF，零栅格，五种主题颜色均实际绘制两次以上、无越界。用 Poppler 渲染并目视检查 PDF，以及1160×820浅色／1024×680深色原生截图；公式和正文可见。Poppler 输出 Symbol／ArialUnicode 显示字体提示，保留为检查器环境信息，不能据此认证完整字体兼容。

必须保留的失败与差异：第一次原生检查用了迁移后才存在的选择器；第二次确实暴露产品丢失颜色，已修复并完整重新验证。首轮测试误用不存在的 Rust 函数而未编译；最终严格 Edge 命令第一次指向根目录不存在的依赖路径，修正为已有 ui-tests 依赖，未安装或下载。原始失败日志和报告归档，未跳过断言。

[Microsoft HSL 说明](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/8229a077-7fc8-4fba-96cc-c77b6a4fc768) 的 shade 插图示例给出 `C0504D/BF → 943634`，与该页明确公式不一致。标准库 colorsys 以精确 HSL 独立计算为 `953735`；官方 tint 示例与精确计算均为 `95B3D7`。最初按 shade 插图设定的测试失败保留，未以特殊取整迎合插图。实现采用明确公式，**实际 Word 的 shade 像素一致性仍待验**。[颜色映射依据](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.colorschememapping?view=openxml-3.0.1)。

全仓库无 Python 检查仍有10条历史 QA 依赖违规，无豁免；新增原生材料使用现有 Node ZIP。缓存中的独立检查脚本只用于 QA，不进入运行依赖。

证据见 [本轮清单](../evidence/capability-upgrade/2026-10-12/batch3-math-theme-manifest.json)。完整 Word 字体／复杂文字／修订、全部 OMML 语义、任意嵌套回写、辅助技术、完整出版参数、实际 Word 与独立 EPUB 阅读器、B3-51–54 及原平台／长期门槛保持未完成。第三批不结项；没有安装替换、提交或发布。
