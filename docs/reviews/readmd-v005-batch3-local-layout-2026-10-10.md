# 第三批增量：源码绑定的局部排版

继续数学与学术出版合同。本增量将 R069 从待办推进到进行中；75 项完整要求为 59 进行中、16 待办、0 整项完整验收。主程序未发布，用户已安装版本未替换。功能入口安装、自有离线包与轻型核心沿用[能力交付方案](readmd-v005-capability-delivery-2026-10-10.md)，基础 Word 输出不依赖本机 Office/WPS。

## 操作与实现

光标放在顶层段落、标题、块公式或题注中，打开现有“文本 → 局部排版”。字号、字体、颜色、对齐和字形可按内容调整；缩进、间距及分页约束按需展开。空值继承导出预设，清除可恢复原文。公式只提供实际有效的字号、颜色、对齐、间距和分页参数。没有新增常驻工具栏。

Rust `publication_blocks` 使用当前正文修订、节点身份和 UTF-16 范围生成普通内容事务；相同文字的两段不会串改。应用一次撤销，重复应用不追加标记，旧窗口、切页或正文改变后拒绝过期提交。软件生成的局部属性保存为可读 `readmd:layout v1` 注释，编辑器收成可点击标签，并排除其字数；普通文档不添加元数据。保存前不写文件、不建资源目录，BOM、CRLF 与代理对范围保持。

统一 AST 携带局部属性。DOCX 输出原生 `w:pPr/w:rPr`，数学使用原生 OMML 运行属性；Typst 生成真实分页和向量公式；EPUB 应用对应内容 CSS。阅读页及编辑预览只投影显式属性，继承部分继续遵守主题，不把内部标记当正文展示。

无效、未知版本、孤立标记、重复 ID 和过窄几何在导出提交前失败，保留原文及旧输出。嵌套列表、引用和脚注当前明确只读。HTML、TeX 及基础内置 PDF 保留内容并返回局部样式损失提示，不能宣称它们已保留局部格式。

`readmd_publication_blocks` MCP 工具调用同一 Rust 服务；它只返回内存修改，不自行写磁盘。通用内容事务工具可执行其反向修改。

## 实际检查

固定源码、构建和证据摘要见[索引](../evidence/capability-upgrade/2026-10-10/batch3-publication-local-manifest.json)。材料均为专用合成文档，原生检查采用正式 CSP、独立配置和真实文件接口；Python/Poppler/Java 只用于独立 QA，不进入软件运行链路。

- [内核回归](../evidence/capability-upgrade/2026-10-10/batch3-publication-local-rust.json)：2040 通过、0 失败，240.69 秒；3 项既有忽略逐项保留，不计作通过。
- [相关界面](../evidence/capability-upgrade/2026-10-10/batch3-publication-local-ui.json)：46 通过、0 失败/跳过/重试，覆盖预设、单行编辑栏及公式交互。与既有套件有重叠，不累加为独立功能数量。
- [Windows 原生](../evidence/capability-upgrade/2026-10-10/batch3-publication-local-native.json)：10 条真实流程，包括第二个重复段落精确修改、真实键盘撤销/重做、取消及过期拒绝、保存重开、无资源目录、三格式实际输出、错误不替换旧输出和中文/德文/阿拉伯文窄窗口。
- [独立 DOCX 检查](../evidence/capability-upgrade/2026-10-10/batch3-publication-local-docx-inspection.json)：选中段落具有 16pt、右对齐、8mm 缩进和分页的原生属性。实际 Word 程序仍未完成往返，不能将该检查计作 Word 排版验收。
- PDF 经独立文本几何解析：未选段落 x=51.02pt、文字高 10.32pt；选中段落换到第 2 页，x=289.61pt、高 15.02pt。题注 x=245.11pt、高 9.38pt，符合居中 10pt 默认值。公式为矢量，真实脚注留在引用页；另有输出截图视觉检查。
- [EPUBCheck](../evidence/capability-upgrade/2026-10-10/batch3-publication-local-epubcheck.json)：包含公式、题注、引用及脚注的实际 EPUB，0 fatal/error/warning。独立阅读器全流程仍另列。
- [真实 MCP](../evidence/capability-upgrade/2026-10-10/batch3-publication-local-mcp.json)：4 条标准输入输出客户端检查，涵盖发现、精确提交、幂等/旧修订拒绝及公共反向事务，没有协议模拟。
- [辅助边界](../evidence/capability-upgrade/2026-10-10/batch3-publication-local-helpers.json)：3 条浏览器词法范围检查，包含 CRLF、示例不折叠和字数边界。[静态门禁](../evidence/capability-upgrade/2026-10-10/batch3-publication-local-static.json)全部通过，46 语言 × 2576 键。

18 个新增键实际翻译到 46 语言；视觉复核同时修正既有德文加粗/对齐/开关及阿拉伯文加粗用语。键集、占位符与三语言布局证据不等于完整语义认证。

![中文窄窗局部属性](../evidence/capability-upgrade/2026-10-10/batch3-publication-local-controls-zh-CN.png)

![真实输出的局部排版、公式、题注与页脚注](../evidence/capability-upgrade/2026-10-10/batch3-publication-local-page-02.png)

## 发现的问题与边界

原生 PDF 检查发现局部右对齐被收缩容器抵消，已将块宽绑定到实际栏宽并重新检查真实文字位置。视觉检查发现默认题注左对齐且为正文字号，已统一为居中 10pt 并加入独立 PDF 几何断言。展开高级属性时动作曾移出可见区域，已改为内容区滚动、动作保持可见。

第一轮界面回归为 43 通过、3 失败。其中两项确为窄窗工具栏溢出，原因是 44px AI 按钮仍显示文字；统一图标式按钮并保留悬停/无障碍说明后修复。一项测试把编辑时应隐藏的阅读设置当成焦点目标，改用当时可见的 AI 按钮，仍验证迟到恢复清理不能夺走焦点，没有删除或跳过断言。初次原生 QA 还错误要求关闭页码时一定有 footer1.xml，已改为可选文件；失败尝试分类保留于[记录](../evidence/capability-upgrade/2026-10-10/batch3-publication-local-attempts.json)。

这不是整批完成：真实分页画面点击回写、任意节/尾注/嵌套块及全部 Office 属性、完整 OMML 视觉语义和实际 Word 往返仍待实施。46 语言全语义、跨平台原生、干净环境及八小时混合压力仍待证据。下一步先推进真实引擎的页面—节点映射及局部修改，再补齐其余出版属性与 LaTeX 离线包/沙箱。

缓存按隔离原生测试、构建暂存、翻译工具配置、可复用工具与日志分列，见[清单](../evidence/capability-upgrade/2026-10-10/batch3-publication-local-cache-inventory.json)。没有再次执行此前自动审批拒绝的批量清理，用户文档、安装及恢复记录未清理。

使用说明见[版式指南](../guides/publication-layout.md)。布局依据为 [Typst block](https://typst.app/docs/reference/layout/block/)、[align](https://typst.app/docs/reference/layout/align/)和 [pad](https://typst.app/docs/reference/layout/pad/)，不以源码相似代替实际输出检查。
