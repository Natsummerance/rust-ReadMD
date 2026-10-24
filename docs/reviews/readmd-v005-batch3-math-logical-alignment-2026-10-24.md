# 第三批增量：Word 公式 start/end 逻辑对齐

2026-10-24。本轮完成已证明水平LTR显示公式的逻辑对齐消费：导入的`w:jc=start/end`能够复用left/right物理布局，源中的Word原始枚举、稀疏声明与继承来源保持不变。完整M07、P04、第三批及发布未结项。

## 实现与适用范围

`math_word/paragraph.rs`只在`WordFiniteParagraph`输出副本中映射Start→Left、End→Right；原始上下文、有效Word枚举和origin不改。共享矢量、MathML、HTML、EPUB及两套PDF使用同一有限布局。局部数学jc优先，`dispDef=true`仍走数学默认值；新逻辑对齐只接受省略或`lrTb`的已证明方向，`tb`别名不扩大到新枚举。

保留bidi/mirror、竖排、逻辑缩进、未实现段落级联、特殊宿主、预算及TeX保护。原physical三种对齐的既有行为不变。新增8项有意义的Rust控制，覆盖三层来源、原始对象不变、方向负控、局部优先、精确重复公式范围和真实几何A/B。

**生成DOCX的边界：** 当前writer使用ReadMDMath样式，并将有效轴物化为direct。输出保留start/end逻辑值和左右有效缩进，重新导入按照实际direct来源计算；原style chain/docDefaults样式包来源并未原样输出。本轮的源事务、保存与重开保留原来源，不能据此声称原样式包往返完成。

toolbar、编辑首绘、4K宽度修复和boot逐份摘要保持。本轮没有新增UI文本、依赖或构建下载；boot未发生修改。

## 冻结与自动检查

最终candidate-57，1103源文件；源码冻结SHA256为`6675c8a7fdaca1283d5d18891c6282f950bbaa94029e2000073d1d2e1b977028`。EXE为79,881,216 bytes，SHA256 `aecf52470084a6b4bddbebc6d0515884d395a622feba8a4a6e0054c27c744022`。

640项Rust/assets与完成`--locked --offline`编译的输入逐行一致，构建EXE摘要对应最终候选。C57只修订QA，复用该确切产品输入上的完整库结果，不冒称重新编译过另一个产品。

| 检查 | 本轮结果 |
| --- | --- |
| 完整Rust库，无运行时过滤 | 3634通过／0失败／3既有忽略 |
| 前端及工具 | 1055通过／0失败／0跳过 |
| assets/styles/wiring/i18n/no-Python/ledger | 六项通过 |
| 新QA独立控制 | Node25／Python21通过，plan正常退出 |
| 受保护前端及boot | 6文件逐份摘要一致 |

## 真实Windows流程

实际使用自有测试目录、真实stdio/MCP和锁定输出引擎，无GUI或Office启动。两份native-only DOCX分别为start/end，每份三式相同公式，jc/左右缩进分别来自direct、实际选中的basedOn链、docDefaults；低优先级放相反值。先检查原ZIP/XML，再通过实际转换、迁移及API保存。

保留两份迁移LF原稿与两份不可变全CRLF派生材料，后者属于显式QA材料准备；读回真实revision、比较三式完整XML/稀疏来源后再做CRLF与astral范围测试。对重复公式中的第二式增添相反direct jc，检查正文与公式陈旧修订、逆事务/重做、保存、删除direct恢复继承，其他范围逐字节不动。

PID2424正常EOF退出后，PID15384重新读取保存源，以空`options:{}`再次导出；临时预设经CAS恢复，原18份引擎证据摘要保持。两个host exit0、请求/响应匹配、pending0、无强制停止；这里仅证明host正常关闭，不据此认证子进程取消或JobObject全链。

| 实际证据 | 数量／结果 |
| --- | --- |
| 总检查 | 7组通过 |
| 工具调用／线协议请求响应 | 192次／196对匹配 |
| 独立检查器调用 | 158次正常退出 |
| 真实产品输出 | 14份：两份DOCX、两份HTML、两份EPUB、四份逻辑PDF、四份物理控制PDF |
| 实际几何记录 | 28条，共享SVG、重开、逻辑/物理对照及两套PDF |
| 失败请求的已有PDF保护 | 14份全部保持摘要及目录不变 |

SVG/PDF检查实际字形路径、advance、y、纸框/缩进和相同引擎物理A/B，保持既有0.25pt容差；HTML检查实际选中renderer，EPUB遍历全部spine，隐藏/phantom/惰性SVG控制不能充作可见输出。源script或XML合法均不能代替这些检查。生成DOCX的原生start/end及ReadMDMath/direct物化再导入单独记录。

四份Typst PDF均保留`publication_font_substituted`警告，逻辑/物理对照使用相同实际字体；本轮不认证原请求字体的原样匹配或完整字体覆盖。报告保留全部warnings，没有将“无缺字”扩大成“无字体替代”。

实际报告SHA256 `7e91f9f79670d8a6fb9641650ccfbf0b4971e876d82baa61185e76d658bdf406`。详见下方归档中的`evidence/qa/stdio-candidate-57-01/report.json`、原始请求/响应、wire、XML和产品文件。

## 首次失败及修正纪律

- 首次库编译失败于测试侧WordParagraphOrigin未重导出；增加`cfg(test)`连接，保留typed断言和原编译日志。
- 首次QA策略检查遗漏两个显式旧oracle路径；新增严格限定开发文件的例外，没有添加产品Python依赖。C55为该策略修正前的QA快照，未运行产品，不计作产品正向或反向证据。
- C56首次真实保存失败：测试给LF文件发送混合换行，却按请求原摘要预期落盘。现有保存服务保留原文件EOL，实际仅去掉三个CR，修订与真实落盘一致。C56报告、017请求/响应和磁盘原字节均保留失败。C57使用新目录与明确CRLF原始材料，保存断言仍严格逐字节/修订比较；未修改保存产品逻辑、未归一化旧失败结果，也不把QA创建材料认证为产品另存/复制功能。

此前shaping/copy、EPUB转换与混合负载超时等失败继续开放，本轮不据范围外新测试销账。

## 归档、剩余与下一步

[可恢复证据清单](../evidence/capability-upgrade/2026-10-24/batch3-m07-logical-scoped/manifest.json)保留C55/C56/C57源码、首败、QA控制、实际请求/响应、14产品文件和最终唯一EXE；不复制运行库/profile/build树。

归档SHA256 `fc01164d0249a25ea5c203008b14ccd3017eecacdc843a49ac7d153a268500f0`；[独立读回](../evidence/capability-upgrade/2026-10-24/batch3-m07-logical-readback.json)核对1156文件，物理34.81 MiB、展开104.03 MiB。C55/C56/C57源码均可恢复，最终C57共1103文件；只存一份最终EXE压缩副本，读回不依赖.cache、不提取或启动产品。

下一单元优先实现可精确整除为现有整数twip的pt/in/mm/cm缩进，保留raw/origin，使用checked有理数及独立Decimal/Fraction对照；先固定有限合同再开发。原Word样式包往返、其他段落/字体轴、RTL/嵌套宿主、非整twip、完整出版/CSL继续保留。Typst非零正文baseline仍待逐run墨迹、碰撞和纸边证明，消费者/worker保护保持关闭。

真实Word/WPS/LibreOffice、独立阅读器、正式离线包/TeX材料、受控冷态、完整故障/八小时及其他平台证据仍欠。账本只追加本单元partial证据，75条整项状态维持59进行中／16待办／0整项通过。
