# 第三批：HTML／电子书段落分页控制

补齐 B3-56 的限定增量，完整第三批仍未完成。HTML 和 EPUB 使用同一 Rust 规则输出孤行控制及段内不分页；现有正文设置可操作，旧预设省略值保持原行为。DOCX、Typst 原生实现保留，内置 PDF 隐藏不支持的控件，直接 API 仍返回损失提示。

完整 Rust **2351 通过、0 失败、3 项历史忽略**，277.25 秒；新增 2 项。正式 CSP／Windows WebView **7 组 46 项限定检查**通过：实际 Word 段落属性、HTML／EPUB 真实五行段落分栏、屏幕与打印样式、两种 PDF、脚注尾注、标题与奇偶页、预设保存重启及无目录污染。MCP 限定 4 项通过。46 语言／2603 键及本轮仓库检查通过，未新增文案。

实测五行段落：关闭约束为 4＋1，开启孤行控制为 3＋2，开启段内不分页则五行整体移至下一栏。依据 [CSS Fragmentation](https://www.w3.org/TR/css-break-3/#widows-orphans)。**三行短段落中 Chromium 放宽为 2＋1，未达到严格断言；该边界保留为兼容缺口，不能据五行场景通过宣称所有分页场景已验收。** 独立阅读器仍未验收。

保留两次失败：第一次测试选择了不存在的分组标识，改为通过实际控件定位；第二次严格短段落分页断言失败。没有自动重试或修改第二次失败为通过，保留独立硬换行／自然换行、三／五／七行矩阵诊断。最终范围明确限定已成立的五行分栏行为，同时记录短段落兼容差异。

目检 1024×680 的正文参数及实际 Typst PDF。Word COM 注册存在，但原自动化超时没有复测；正在使用的 WPS 未操作。产品 Word 导入导出不依赖 Office，实际应用互操作继续独立记录。

EXE：7df1cf744238b25d229deaac16b0ebfd9d3672739e9e17e7cd8f32e5de28aae3；前端：e1e8a07aa3ca4fae265bfb667abc49512686106c4bfb197bf86064865843d4a8。

冻结 [manifest](../evidence/capability-upgrade/2026-10-12/batch3-publication-paragraph-flow-manifest.json)，SHA256：853fbcab42514edfcf1b7aa2d6f01df9a220229243f084ef75d7c502eee25d74；192 文件、12,764,857 字节、192 项产品源码身份。复用唯一编译目录，未归档运行时及原生配置；缓存分类见归档 cache-inventory.json。

检查另发现 HTML 导出新 DSL 被原样显示，已用真实导出文件及实际捆绑 marked 复现，html-math-before.json 记录 rawDslVisible=true／renderedMath=false。该问题没有在本轮标为修复，接续单独实现并取证。

完整公式／出版属性、任意节、短段落及 MathML 兼容、真实 Word／独立阅读器、全部故障／八小时压力和非 Windows 原生门槛全部保留。未替换安装版或发布。
