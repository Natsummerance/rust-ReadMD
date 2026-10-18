# 第三批：真实字体上下标轮廓间距

完成 B3-55 的限定增量。完整第三批、全 MATH 常量和实际 Word 应用往返仍未完成；未发布或替换用户安装程序。

原生矢量公式读取字体 OpenType MATH 四角 kern 表，按实际高度合并基底与上下标的角值，保留正负修正。阈值相等、字号和基线移动均正确处理。显式 alnScr 将两列放在外侧共同起点；复杂盒子、没有表的字符和人工斜切采用零角修正。预览和内置 PDF 共用此 Rust 排版，不增加 UI、依赖或系统字体要求。依据：[Microsoft OpenType MATH](https://learn.microsoft.com/en-us/typography/opentype/spec/math)。

- 独立 FontTools 4.63.0 提取捆绑 STIX 数据：233 字形、4,411 取值及阈值边界；四种缩放、两种基线共 35,288 次断言。测试 fixture 锁定真实字体摘要，FontTools 不进入产品。
- 八组真实 Unicode 基底和上下标在行内／块模式下，与独立坐标 oracle 一致；对齐开关、复合盒子、缺表和人工斜切回退通过。
- 完整 Rust：2349 通过、0 失败、3 项历史忽略，290.13 秒；新增 3 项。首次测试借用冲突编译错误保留并修正。
- 同一构建正式 CSP／真实 Windows WebView：10 组 62 项，无自动重试。新流程 6 项，包含真实 API／矢量坐标、第二处重复公式精准属性修改、一次撤销、保存重开及 DOCX／EPUB／PDF 输出。原有数学与出版流程 56 项回归通过。
- 实际 stdio MCP 4 项；范围限既有发现、源码定位、过期拒绝与序列化，不能算完整 VS Code 验收。
- 目检中文深色 1024×680 及真实矢量 PDF。EPUB 离线验证通过，独立浏览器缺省字距仍不保真，保留明确提示和之前的严格失败证据。
- 46 语言／2603 键及全部本轮仓库检查通过；不代表人工语言审校或性能总门槛完成。

EXE：bbc7201ce2ea6be1d3cc386ca7282d244797aacebd6096f9339959de6c30b70f；前端：27a3c22f4838b5d92b2ea6cbffb2b7ec5616bdf2d1a90ab8741908949318aa87。

冻结 [manifest](../evidence/capability-upgrade/2026-10-12/batch3-math-kern-manifest.json)，SHA256：38346f9dd6edc6ba32513b2d3882c8715a193d25202daf5405371733a8eeb2ee；201 文件、11,400,566 字节、192 项产品源码身份。仅保留小型合成输出和证据，复用唯一构建目录；缓存分类见归档 cache-inventory.json。

完整 OMML／关联 Word 信息、UnicodeMath／MathML、任意节和完整出版参数、Word／独立阅读器、全部故障与八小时压力、非 Windows 原生证据继续开放，不改变完整需求状态。
