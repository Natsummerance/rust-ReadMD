# 第三批：上下标与显式上下限实测

完成 B3-55 的限定增量。完整第三批仍未完成，未发布或替换用户安装程序。

## 实际修正

- 行内 limLow／limUpp 保持真正的正下／正上缩小限位，不再错误变成右侧脚标；大算子的独立规则保留。
- 前置上下标靠右对齐到基底，保留间距；空槽不产生额外间隔。
- alnScr 的缺省、显式关闭、开启和所有合法布尔写法作用于原生矢量布局。MathML 可重新导入恢复开启属性，不把任意间距误识别为对齐。
- 在预览中修改第二处重复公式的属性，精确回写可读 DSL；一次撤销、重做、真实保存及重启成立。

依据：[Office 上下标对齐](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.alignscripts?view=openxml-3.0.1)、[MathML Core 脚标布局](https://www.w3.org/TR/mathml-core/)。完整字形轮廓 kerning 不因本增量自动认证。

## 独立浏览器发现的未通过项

严格测试发现：使用同一捆绑 STIX 字体，Chromium 的原生 MathML 缺省上下标未显示 Word／产品矢量使用的斜体修正。最小公式也复现；不能归因于包装层。另检查了正确命名空间、实际字体摘要及应用字体特性清零。开启对齐、前置右对齐及行内上下限的独立排版检查通过，但**缺省 MathML 字距保真仍未验收**。

保留首次严格失败及诊断。最终流程检查范围明确调整为已修正原生语义、已成立的独立排版，以及明确的兼容失败行为；没有把缺省字距失败改写成通过。MathML 序列化和 EPUB 导出返回独立的 `math_script_spacing_fallback` 提示，新增文案覆盖 46 语言。默认间距的跨阅读器保真继续留在第三批。

## 构建与验证

- 最终 EXE：93e5e12c8abfb6cc5de9983e9929e9f4cb21d404d3bd96fec6cd868fa3eb70a3；前端：632904c51343aabebf2e3ac67960f03c073ad21f1fc28560a4bca182814d4ac0。
- 完整 Rust：2343 通过、0 失败、3 项历史忽略，145.90 秒；本增量新增 5 项测试。
- 同一最终构建正式 CSP／真实 Windows WebView：8 组 51 项。新公式流程 5 项；数学样式、奇偶页、装饰、MathML EPUB、脚注起始、出版布局和尾注 46 项回归。
- 实际 DOCX／EPUB／PDF；离线 EPUBCheck、受管理 PDFium 重开；目检深色 1024×680 与实际 PDF 页。DOCX 检查是文件结构证据，不能代替 Word 应用往返。
- 实际 stdio MCP 4 项，范围限发现、源码运算符定位、过期拒绝及 OMML 输出，不计为完整 VS Code 或新脚标语义认证。
- 46 语言／2603 键及资源、接线、样式、无 Python 产品依赖、账本、前端新鲜度、i18n 工具测试通过；不代表人工语言审校完成。
- 冻结 [manifest](../evidence/capability-upgrade/2026-10-12/batch3-math-script-placement-manifest.json)，SHA256：a99aa16cd06882eb26487764a96e3214096308496db0b98b7c73c3d11e83fe62；236 文件、18,865,679 字节、191 项产品源码身份。

## 保留的缺口与缓存

全 OMML／UnicodeMath／MathML、关联 Word 样式修订、任意节和完整出版参数、实际 Word／独立 EPUB 阅读器、全部故障及八小时压力、人工语言审校、其他平台证据仍开放。复用单一编译目录；小型合成产物和证据归档，运行时及配置未复制进证据库。缓存分类见 manifest 同目录内 cache-inventory.json；此前被自动审批拒绝的递归清理没有重试。
