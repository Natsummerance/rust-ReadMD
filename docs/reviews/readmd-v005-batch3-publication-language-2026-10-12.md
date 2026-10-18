# 第三批：出版语言与元数据边界

接续 [HTML 公式及复合上下标增量](readmd-v005-batch3-html-math-2026-10-12.md)，补齐 B3-56 的限定出版参数。没有新增按钮：文档开头的 `lang`／`language` 可以传递到 HTML、DOCX、EPUB 和 Typst PDF；导出通用元数据可覆盖正文声明，电子书专用语言设置优先。软件界面语言与作者内容语言独立。

## 实际变化

- 共用有界语言标签处理，接受历史下划线写法，保留文字、地区、变体、扩展、私用及祖父标签。规范大小写，拒绝注入字符、空段、重复变体及重复扩展等不合法覆盖；不宣称 IANA 注册与旧别名认证。只读取开头、顶层元数据，正文和代码示例不成为全书声明。
- HTML 的实际 `lang` 采用文档声明，并隐藏开头 YAML 元数据，后面的 YAML 代码示例保留。无声明的旧文档保持原有默认值。
- DOCX 原生默认样式的校对语言及核心属性采用声明；EPUB 的 OPF、导航与章节 XHTML 保持一致。当前证据是实际生成文件的 XML 检查，不能代替真实 Word 应用验收。
- Typst 分别接收主语言和支持的两字母地区，依据 [官方文本参数](https://typst.app/docs/reference/text/text/)。实际发现 Typst 0.15.1 生成的 PDF 主语言仅为 `de`，因此 Rust 在提交前追加 Catalog 的增量修订，保存完整 `de-DE`；原始页面、字体、链接及结构对象的字节保持。仍受原任务期限及 32 MiB 输出限制约束，提交继续原子完成。
- 使用既有 lopdf 增加单独的 Rust QA 检查程序，原生测试及产品都不依赖 Python。另一套 pypdf 仅用于本地额外 QA，没有加入产品、原生测试依赖或放行清单。

语言标签参考 [W3C](https://www.w3.org/International/articles/language-tags/)，Word 校对属性参考 [Microsoft](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.languages?view=openxml-3.0.1)。内容声明不自动覆盖作者的字体与对齐设置；完整 RTL 排版与所有元信息仍分别验收。

## 实测结果

完整内核库离线锁定测试 **2365 通过、0 失败、3 项历史忽略**，155.41 秒；本增量新增 6 项。前一构建 2364 项通过的日志作为中间结果保留，未混作最终构建证据。

同一构建 Windows 原生／正式 CSP **6 组 33 项**通过：出版语言 6、HTML 公式 5、数学字体 7、段落分页 6、标题参数 5、电子书 MathML 4。真实 stdio MCP 限定 4 项通过。46 语言／2603 键、资源、连线、样式、无 Python、boot 新鲜度、账本及翻译工具测试通过；没有新增可见文案，也不据此宣称全部语言已经人工审校。

实际输出覆盖德语声明、法语／加拿大导出覆盖、繁体中文文字／地区标签和日语电子书专用覆盖；HTML、DOCX、EPUB 文件内部属性一致。德语电子书通过离线 EPUBCheck。实际 Typst PDF 经 Rust 检查程序读取为 `de-DE`，在 PDFium 重新打开及渲染；独立 pypdf 6.16.0 的严格解析、正文提取和语言检查也通过，文件 SHA256 与原生输出绑定。目检 1024×680 阅读界面及实际 PDF 页面，使用的均为专用合成材料。

保留全部初始失败：新增测试首次编译的 4 个错误已修正；首次原生 PDF 地区断言得到 `de`，修复后仍使用原 `de-DE` 严格断言。仓库无 Python 检查最初发现原生测试调用 Python，已改用 Rust 检查程序，不增加放行。没有自动重试取得通过。

冻结 [manifest](../evidence/capability-upgrade/2026-10-12/batch3-publication-language-manifest.json)，SHA256：`d9961b5c91aed080b01b8e52fc02a5e2e3bc9a35e96245c9009826b4fb7e1a7d`；167 文件、42,261,101 字节、193 项产品源码身份。源代码、QA 程序、修复前诊断、失败与最终输出均有对应记录。

EXE：`5dc61c248aed223c875109524981f0104823b0fbb3ed3804a470ad71d632521d`；boot：`94ae15475e7d3201ba61cbb7c9826a7281ad9fb8e819665b173b20b129b74e90`。变更进入 V0.0.5 测试构建。

## 未完成范围与后续

第三批七个收口包继续开放，见 [执行清单](../design/v005-five-batch-spec.md)。接续重点是完整公式属性／适配器语义，以及 CSL 定位符、完整作者机构等元信息、局部覆盖、任意节、题注与复杂科研排版参数。完整 UnicodeMath／MathML、数值地区与文字变体的排版语义、全部语言及 RTL 不能由本轮常见标签取证替代。

实际 Word 应用、独立电子书阅读器、安装态 VS Code、完整资源安装／并发／崩溃／传输故障、TeX 正式分发材料与冷态性能、八小时压力及非 Windows 原生仍未验收。已有 MathML 缺省字距、Chromium 三行孤行控制兼容缺口保留。本轮不构成完整第三批或发布验收。

缓存分类见 [cache-inventory](../evidence/capability-upgrade/2026-10-12/publication-language/cache-inventory.json)。复用唯一构建目录；QA 检查程序不进入产品安装包，运行时能力包及浏览器配置不进入证据归档。原生配置保留，没有递归清理。
