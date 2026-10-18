# V0.0.5 第三批并行语义与出版增量

日期：2026-10-13。按照用户指定，接续子代理使用 GPT-6.1 Sol / max，源码按文件分工，Cargo 与原生候选由主代理统一。全部 75 项需求继续保留，参见[并行执行清单](../design/v005-parallel-execution.json)。本报告只认证下列增量，完整第三批及发布仍未验收。

## 实际完成的用户流程

| 单元 | 实现与实际验证 | 仍需完成 |
| --- | --- | --- |
| M05/M06 | 微分 box 的实际薄间距；透明 phantom 将内部数学类别传递给外部；可见性与零维度独立；显式 noBreak 关闭允许内部断行。矢量与原生 MathML、重复公式属性编辑、一次撤销重做、原子保存及真正进程重启通过。 | 微分 3/18 em 是明确排版策略，非 Word 像素认证；多字符 run、lit、完整 opEmu 组合及 Word 对照继续开放。 |
| MathML 脚本 | 真实浏览器发现普通上标算符仍有字典间距、透明 phantom 路径为零；统一脚本输出间距策略，保留普通非脚本路径和矢量基线。原生二元／关系对照通过。 | 完整 MathML/Office 属性等价与独立阅读器继续开放。 |
| P02 | CSL 与正文语言共享有效值；导出 meta.language、EPUB 专用 language 覆盖一致。de_DE→Januar、fr_CA→janvier，zh-Hant-TW 使用对应传统中文 locale，保留自定义 CSL locale。HTTP、stdio MCP、真实文件一致，作者源文件不改。 | 全 CSL 语义、RTL 整形及所有语言母语审校继续开放。 |
| P05 | 目录深度 1–6、点／短横／下划／无线引导线；DOCX 可更新 TOC 字段和样式、EPUB 嵌套导航与 NCX、Typst 真实目录；预设保存重启与格式适用性通过。四种 PDF 均实际编译、逐页 PDFium 打开，保留正文与本地化引用。 | 多级标题编号、章级重启、Word 实际更新字段和跨节排版继续开放。 |
| 超时清理 | 原生终止树只枚举 PID/父子关系，不再查询所有无关进程的完整路径；保留原有终止选择与顺序。原先完整回归的两项超时失败修复，34 项代码运行测试整组 2.30s，通过最终全库。 | 八小时混合压力及全部平台停止时限继续开放。 |
| I01–I07 设施 | 新增有界只读安装盘点、隔离 Word 夹具、进程身份校验与独立报告；6 项工具测试及 PowerShell 解析验证通过。 | 本机 Word 已安装但真正编辑往返未验收；独立阅读器在限定盘点内未检出，不代表整台机器未安装。 |
| D04 | 新增有界材料审计、离线复核与反例测试，固定 tagpdf commit 源码候选单独归档。 | 8 个精确旧容器仍缺失，共 11,460,408 B；D04 与 redistributionApproved 保持 false，不能将候选源码视为原包认证。 |

## 同一最终构建的证据

- Rust 库：2388 通过、0 失败、3 项历史忽略；本轮较矩阵基线新增 16 项测试。最终完整运行 138.99s。
- Windows 正式 CSP 原生：4 组 27 项（间距 7、行距 6、出版 6、矩阵回归 8）。
- 真实 stdio MCP：2 组 9 项（数学 4、引用语言 5）。
- 严格 CSP UI：12 项（新增目录 6、既有预设 6），0 跳过、0 重试、0 flaky；自动清理正常。
- 仓库门槛：8/8；46 种语言、2605 键。新增 2 个目录标签都有对应文本，语义母语审校仍单列。
- 实际 DOCX、EPUBCheck、四种 Typst PDF 与 PDFium 验证；Word/外部阅读器不是这些检查的替代名称。

最终 EXE SHA-256：

`255f4c20425f83c5d42ee5a06d056e3fabc30dcd790b6bd108c32228b6b65754`

boot SHA-256：

`2455b4744f41a5936c0cadbadfd75f9f61341f1828acb12c19e8d5904e1f2c44`

[冻结清单](../evidence/capability-upgrade/2026-10-13/batch3-parallel-semantics-manifest.json) SHA-256：

`101409ee18df24e4882836814f33ac805f00e686b84ff9b7485623177abfa6fe`

归档 235 文件、50,475,826 B，复用历史源码引用，未复制整个浏览器配置或引擎。235 归档文件及 193 当前产品源码摘要均复核通过。源码摘要以源清单中 path/sha256 序列的 JSON 做 SHA-256；日志、产物及源清单各自另有摘要。

## 失败记录与修复依据

受限沙箱的文件所有权测试报 Windows code 5；正常原生权限对同一测试通过。受限原生 WebView2 的 CDP 页面失去连接，正常权限对照成功；保留受限全库中断及失败报告，不计为产品通过。

真实测试发现并修复 MathML 脚本路径差异、代码超时清理开销和 Typst 错误参数：fill 属于 outline.entry，改为 set outline.entry + outline(depth)。四种语法先由锁定 0.15.1 引擎生成真实 PDF，再跑完整应用导出。依据：[Typst 官方目录接口](https://typst.app/docs/reference/model/outline/)、[MathML Core mrow 布局](https://w3c.github.io/mathml-core/#layout-of-mrow)、[CSL locale 回退](https://docs.citationstyles.org/en/stable/specification.html#locale-fallback)。

其他 QA 问题包括缺作者夹具、折叠预设下拉、默认值不应被未改变的控件物化、SVG 展示宽度的舍入、Boolean 属性的 select 操作与输出诊断变量。修复的是夹具／操作／精确测量；保留失败，未放宽数学或保存断言。无安装默认浏览器时显式使用已安装 Edge 的隔离上下文；正式 CSP 未绕过。

目检最终中文 1160×820、英文 1024×680 导出界面、暗色公式与真实 PDFium 目录页，内容清晰、无文字重叠。行距回归的一张方程组截图没有完整捕获目标，不认证该张完整目检；几何断言及其余截图独立保留。测试画面含正常提示，不是宣传录屏。

## 缓存与下一步

已验证证据后清理最终隔离配置的 webview/capabilities，释放 271,178,283 B（约 258.6 MiB），见[清理记录](../evidence/capability-upgrade/2026-10-13/batch3-parallel-cleanup.json)。保留构建 target、锁定离线包、全部合成输入输出、旧失败配置及证据；不清理用户安装或私人文件。

| 目录 | 用途与后续处理 |
| --- | --- |
| .cache/community-upgrade/build | 唯一离线构建 target；关闭增量编译，继续实施期间保留，发布候选完成后再按需清理。 |
| .cache/capability-upgrade/locked-packs | 固定离线引擎/字体包；后续真实复验需要，保留。 |
| .cache/capability-upgrade/*-native-* | 自有 QA；本轮最终配置重复运行时会重新创建，报告/产物已归档。旧失败配置保留供诊断。 |
| docs/evidence/capability-upgrade/2026-10-13/parallel-semantics | 本轮冻结测试证据，约 48.1 MiB。 |
| docs/evidence/capability-upgrade/2026-10-13/tex-corresponding-materials-audit | D04 独立证据与小源码候选，约 2.4 MiB；不能视为正式包。 |

下一实施单元按既有细则推进 M01/M02（字体驱动分式／根式）及剩余出版参数；D04 原版本材料不能取得时采用可完整追溯的新闭包，重新审查而不混配。全部 75 项和第三批 47 个工作项继续由原账本与并行清单跟踪。未进行版本发布、网站宣传更新或成片制作。
