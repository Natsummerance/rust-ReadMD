# 第三批：公式默认隔离、逐节脚注与准确转换诊断

日期：2026-10-21。接续 candidate-31，依照[实施规格](../design/v005-batch3-next-execution-2026-10-21.md)完成本次可表达子范围。完整第三批、跨应用及平台验收保持原要求，不因局部通过结项。

## 实现及实际行为

- 数学默认值区分缺省、空元素与显式值，并按有效语义比较。DOCX 对可等价局部化的对齐和上下限写真实原生属性；不能等价共享的文档级默认在打包和提交前返回准确冲突，不使用首份设置覆盖其他来源或裸公式。
- mathPr 复用固定 schema 的现有验证器，检查命名空间 URI、属性、语义重复、顺序、互斥和元素内容。真实命名空间别名可用；未知扩展保留与可编辑能力分别记录。行内载体夹带显示段落、旧原生载荷把 mathPr 嵌入公式或槽位，均不能绕过消费范围检查。
- 四项有文档化默认的空值与既有三项边距默认共用兼容表，源码保留三态，DOCX 副本补齐原生 required 值；固定 schema 未修改。dispDef 关闭时四种间距继续生效。依据 [Word 实现说明](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oe376/f5f7b70e-9d07-40f0-b78f-4701a036eef5)及 Microsoft SDK 的 [defJc](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.defaultjustification?view=openxml-3.0.1)、[intLim](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.integrallimitlocation?view=openxml-3.0.1)、[naryLim](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.narylimitlocation?view=openxml-3.0.1)、[brkBinSub](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.breakbinarysubtraction?view=openxml-3.0.1)。
- 原生显示公式与旧保真信息进入 AST 时保留实际显示／行内模式。同段或独立胶囊都不能通过 payload 反推模式；普通正文、重复胶囊、多公式及真正行内公式有独立保护。正文已变而摘要失配时，新正文仍决定显示模式，旧属性只留作恢复；MCP原文编辑保存与UI自动迁移的证据分别记录。
- 两种原生逐节脚注位置写入可读 readmd-section:v1，每节独立合并全局策略；保留节身份、边界、链接和重复引用。当前原生修改优先于旧辅助来源。其他格式先验证等价性，不能表达时保留旧目标并明确提示。
- 修复相邻脚注与链接的 Markdown 歧义：仅在生成引用与下一 bracket 语法相邻时加无可见宽度的分隔，不改变作者正文和间距；同时验证 AST 和重新输出的两类原生引用数量。
- 已知原生注释／编号转换错误由同一有界解码器提供给单文件、批任务和 MCP，不再追加“文件可能已损坏”。畸形／伪造详情不能冒认已注册错误，真实损坏文件保留通用故障处理。批处理行使用现有多语言摘要，已有悬停和键盘说明提供严格有界的原生定位，不增加常驻控件；详情只进入纯文本属性。

本次没有新增依赖。保留单行 toolbar、编辑首绘和 4K 五个受保护文件的 candidate-31 原始字节；新增错误提示覆盖现有 46 种语言，结构检查 2822 键。结构及 VM 检查不等于全部母语和原生布局审校。

## 冻结与实测

最终 candidate-35，1029 个产品及 QA 文件：

- EXE SHA256：d6f328d2857bc11edda474b222b4cc3f2636b1dcb56bd53f5416dd76149dc105。
- boot SHA256：6ec65cf783834adbda4f19717ba0712ecbe2c45060e9ae6ba00a8cd4c2a08e6f。
- 源清单 SHA256：09c4ef153ebe2fc10ad909788ce4416c1d27e22ac012dbd535e79322f93f3eab。

| 检查 | 结果 |
| --- | --- |
| 完整 Rust 内核库，locked／offline | 3253 通过／0 失败／3 既有忽略 |
| 完整前端和工具 | 676 通过／0 失败／0 跳过 |
| 资源、样式、接线、i18n、无产品 Python、账本 | 六项通过，boot 已离线重打包 |
| 新独立检查器纯控制 | Node 93／93、Python 42／42；与真实产品结果分开记录 |

| 真实候选流程 | 执行候选 | 检查项 | 实际输出 | stdio 请求／等量响应 | 保存重启链 |
| --- | --- | --- | --- | --- | --- |
| mathsection | candidate-35 | 91 | 47 | 423 | 27 |
| settings | candidate-33 | 33 | 18 | 98 | 12 |
| numbering | candidate-33 | 10 | 4 | 25 | — |
| citations | candidate-33 | 31 | 8 | 75 | — |
| notes | candidate-33 | 10 | 28 | 61 | — |
| tex | candidate-33 | 10 | 20 | 52 | — |

最终候选与 candidate-33 的 EXE、boot 及全部产品源逐字节一致；仅数学／分节独立 QA 变更 4 文件。该路用最终候选重跑，其余五路保留实际执行候选和原报告身份，各自全部 QA／产品依赖均与最终快照一致。等价证明单独留档，没有重标旧报告，也未因独立检查器修正重复运行无关的长 TeX 测试。

mathsection 的检查列表中有一项明确标记为 oracle-only 的非法夹具控制，不计为产品实现通过；其余检查查看真实接口和产物。数学／分节保存链共 27 条，settings 共 12 条：受修订保护保存、正常 EOF、不同 PID 重开、空 options 且不传旧预设、真实 DOCX 再输出与回导。

新流程另外运行真实本机 HTTP 预览及批处理，比较与 MCP 一致的类型和详情。独占隐藏控制台仅向本次宿主进程组发 CTRL_BREAK，退出码 0 且移除 instance.json；不启动浏览器、WebView 或 Office，不以强杀作正常关闭。旧目标保护包含实际 DOCX 副本、来源和目录摘要检查。向量预览检查实际轮廓、缩放、对齐、间距和窄宽换行。

## 首次失败与修复

1. 专属数学测试首次直接访问私有字段导致编译失败，改用公开 SVG／PDF 绘图流和精确尺寸断言，未扩大产品字段可见性；原日志保留。
2. 逐节真实结构测试发现相邻脚注被链接语法吞掉，原计数失败保留；修复共享转换器后继续使用三次脚注／两次尾注及实际链接断言。
3. 独立复审发现 inline oMathPara、属性 URI 及 schema Choice 缺口；正例拆成规范合法材料，非法输入单列控制，不放宽固定 schema。
4. 完整 Rust 首次为 3244 通过／4 失败／3 既有忽略。合法 Word 显示公式的旧载体误分类和原错误诊断被遮盖均在共享链路修复；另两项是新增链接夹具使用不存在锚点、旧测试用字符串前缀误计 oMathParaPr，分别换为真实外链关系和原生节点结构检查，保持功能要求。
5. VM 批处理测试首次缺少 document.querySelector，直接运行输出的凝练记录单独标明，未冒充完整 stdout；补齐测试宿主后通过。独立 QA 的 CRLF 和 Choice 首次失败也保留。
6. candidate-32 首次新流程在实际重导出时发现 QA 把编号声明追加到正文末尾，准确返回 publication_numbering_invalid；改为调用真实受修订保护的 readmd_publication_numbering 事务，并补 YAML/BOM/CRLF 及正文未改控制。candidate-33 又纠正了检查器对 YAML 后多一个空行的错误位置假设；不改变产品的合法插入行为。两次真实失败均保留，最终执行身份和 QA-only 等价范围见上表。
7. 冻结期只读复审发现旧载荷嵌套 mathPr 绕过校验、失配胶囊降级显示公式两处边界。保留精确输入、静态路径与候选 32，修复后加入原生/旧载荷、真实 URI、保存重开及输出断言。批处理实际定位详情未展示也保留修复前 VM 失败与修复后控制。
8. candidate-34 实际转换返回原有 LaTeX 加 current 载荷，检查器却直接要求 DSL 围栏；完整保留该失败。之后明确调用真实迁移服务，逐项验证原摘要、完整 OMML／Word context／mathPr 语义、逆事务与幂等，再在返回的可读源上编辑。没有用文字替换伪造迁移，原始转换和迁移后文本分开保存。

所有实际失败和后续变更分别留档，没有新增跳过或自动重试取得产品通过。

## 边界及后续

本次未认证实际 Word／WPS／LibreOffice 编辑刷新、独立电子书阅读器、Extension Host、全部 OMML／CSL、可替换 MATH 字体、完整段落继承及 TeX 原生 OfficeMath。P06-B 任意作者行内 size／sizeCs、其他逐节注释属性、剩余版式参数、完整故障／八小时压力、正式离线资源、受控冷态与其他平台继续按账本执行。缺少环境保持未验收，不写成不能实现。

下一增量先按[作者行内字号实施规格](../evidence/capability-upgrade/2026-10-21/batch3-math-section-scoped/implementation/notes/p06b-next-spec.json)统一模型与各消费者，再推进其余出版和数学语义。规格明确两维字号不等于复杂文字路由，代码块任意混合运行也需要完整模型，不能借少数尺寸正例认证。没有替换已安装程序、发布版本或推送宣传素材。

不可变证据：[manifest](../evidence/capability-upgrade/2026-10-21/batch3-math-section-scoped/manifest.json)，SHA256 9e62b9b23f60ddf5f6f6278f8648de2869c4d2ea7ffd2b728d44471f1a1d46fe；4118 文件／166355193 字节，源以内容寻址压缩记录复用 candidate-31 基础档案。75 条整项状态保持 59 进行中／16 待办／0 整项通过。

[缓存处理记录](evidence/batch3-math-section-cache-cleanup-2026-10-21.json)：核对归档摘要及解压后的 EXE 摘要后，删除两份任务自建的重复压缩备份，释放 52,001,249 字节。构建目录供下一增量离线复用，保持关闭增量编译；冻结源、首次失败和共享引擎继续保留。
