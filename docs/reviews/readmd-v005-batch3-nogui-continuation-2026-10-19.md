# 第三批无界面续做：PDF 修复与 TeX／互操作补证（2026-10-19）

本轮限定流程已通过，完整第三批仍未完成。遵守用户“别用 computer use”：没有操作桌面、浏览器、Office、独立阅读器或 Extension Host 界面，没有激活 Word COM。保留上轮 toolbar、目录、编辑首绘和 4K 改动；前端包未改变。所有新增结论绑定 candidate-14，旧失败保留。

[不可变证据清单](../evidence/capability-upgrade/2026-10-19/batch3-nogui-continuation-scoped/manifest.json)，SHA256 `1aaaa8d130c5bb549a48ffcb47e8bda31275e959da03d0326cadf004204bcc66`；[本轮验收摘要](../evidence/capability-upgrade/2026-10-19/batch3-nogui-continuation-scoped/root/acceptance-result.json)。只增量归档本轮源码；相同源码通过前轮归档摘要引用，避免重复保存整套资源。

## 真实修复及复测

实际受管理 XeLaTeX 已生成中文 PDF，随后真实 MCP 转 Markdown 时，提取出的单独 `|` 进入 `line[1..len-1]`，触发反向范围 panic。本轮改用顺序 `strip_prefix/strip_suffix`；退化边界进入普通文本处理，正常多列表格及 Unicode 单元格继续使用原规则。没有以捕获并吞掉异常掩盖问题。

- 两条新增 Rust 回归覆盖空输入、单／双分隔符、缺一侧边界、中文／emoji、空单元格、Tab 列，以及合并／独立段落里的符号和邻近内容。
- 首次测试把单空行误当独立段落而失败；原 PDF 清理规则是两次空行才分段，最终分别验证两种已有语义，首次日志保留。
- [原崩溃 PDF](../evidence/capability-upgrade/2026-10-19/batch3-nogui-continuation-scoped/tex-cold/baseline-mcp-first/run-01/Output.pdf) 经新 EXE 真实 stdio 读回成功；唯一标记和中文正文保留，原 PDF 摘要前后均为 `54c1b941a5b75c37183ed73cb1890c254d65294d4fd81c9a3ca336fa9030adc1`。此次读回耗时 1.231 s，未重新生成或替换原件。
- [真实读回质量检查](../evidence/capability-upgrade/2026-10-19/batch3-nogui-continuation-scoped/root/original-readback-quality.json)确认表头／单元格保留、控制字符为0、没有字体或CMap程序文字。PDF文字提取会把上标展平，不能把这次限定通过当作任意公式／表格完整语义往返。

## 短正文和字体流污染的进一步修复

修复竖线 panic 后，candidate-13 的真实回读仍失败：MD 含 4,190 个控制字符，标记和中文缺失。因此停止后两轮，保留该候选与全部失败输出。根因是三个字体感知分支硬性拒收不足 150 个非空白字符的有效正文，而随后的全流扫描仅凭 BT／ET 字节判断内容，甚至将 ToUnicode 注释 WHETHER 中的 ET 当作正文操作符。

最终删除长度硬门槛与任意流扫描兜底，保留可信字体映射以及替换／控制字符保护；数字计入有效文本，短标点按小样本处理。无映射文本继续进入原扫描图像／OCR／错误路径，不再把字体数据伪造成正文。

另四条新增真实 PDF 回归构造页树、Contents、字体和 ToUnicode：短英文、数字、中文、CID不等于Unicode的映射、空页、无映射字体和非页面流污染。分别实际调用 pdf_extract 与 lopdf 验证解码。噪声流前加超过300字节隔离，防止旧启发式意外过滤反例。

[三种独立解析证据](../evidence/capability-upgrade/2026-10-19/batch3-nogui-continuation-scoped/interop/pdf-short-text-independent-20261019-verified.json) 保存 Poppler、pypdf、MuPDF 的真实输出，均读到原PDF的50个非空白字符，另含ToUnicode与嵌入CFF轮廓检查。独立工具证据不代替最终Rust实际回读。

## 离线构建、回归与进程实测

| 检查 | 实际结果与边界 |
| --- | --- |
| Rust 库 | 2975 通过、0 失败、3 项既有忽略；`--locked --offline` |
| 前端及工具 Node 回归 | 356 通过、0 失败、0 跳过 |
| 仓库检查 | 资源、样式、接线、i18n 键值、无 Python 产品依赖、账本检查通过 |
| 前端／原生界面 | boot 与 candidate-12 一致；本轮没有新增 UI、CSP、视觉或语言语义验收 |
| 首次完整 TeX 校验 | 65,406 ms；34,822 文件、736,565,687 字节；保留完整 SHA 校验和原 120 秒预算 |

普通 Rust 单测中的计时 helper 只有显式提供环境参数才运行大目录测量；其普通计数不能当作性能证据，65,406 ms 来自单独真实调用。此前的慢诊断不被改写。

最终候选使用三个独立 MCP 新进程，PATH 仅保留 System32；每轮完整经历初始化、受管理资源校验、XeLaTeX 输出及真实 PDF 读回。未预先调用就绪或安装接口，运行库没有复制。计时期间暂停本任务编译和大文件归档。

| 独立进程 | PDF 生成完成 | 加读回的全流程 | 结果 |
| --- | ---: | ---: | --- |
| 1 | 74.649 s | 76.995 s | 通过；正常退出 0 |
| 2 | 26.197 s | 28.213 s | 通过；正常退出 0 |
| 3 | 75.050 s | 77.095 s | 通过；正常退出 0 |

**这些是新进程实测，OS 文件缓存未受控，因此 D06 的独立系统冷态仍未验收。** 没有通过增大超时、减少哈希或复用“已校验”捷径获得结果。stdio 不加载 boot，记录它仅用于候选身份；该测试不认证界面。每轮保护的是预先枚举的旧文件存在和内容，不涵盖目录身份或新增文件全集。主进程退出及 stdio 关闭已观察；不新增协议取消、后代退出或全部故障矩阵认证。

计时 QA 也已加固：最后响应之后的非法 JSON、异常退出、传输错误仍会锁定失败；等 stdio 真正关闭后才关闭日志；旧任务 record 缺少有效 Header 时拒绝测量。新增反例测试通过。首次真实 PDF 读回失败、原版观察器及后续改动分别留存。

## 资料和外部互操作

- TeX 三个固定源码提交对应现有运行文件 **88/88 逐字节一致**。完整材料盘点 529 个精确容器、8 个缺失，共 11,460,408 字节；198 个二进制的构建源码义务仍开放。原锁、四个 partial 和 `redistributionApproved=false` 不变。详见 [D04/D05 资料报告](readmd-v005-d04-d05-materials-2026-10-19.md)。
- 新增统一闭包规划器：整体重选 base＋ctex 来源，输出完整 revision、摘要、缺料和体积差异；不会把已有 installed 状态当作新闭包完成。现有完整材料量级约 588.5 MB，元数据及二进制构建源码另计；本轮没有换正式锁或下载大引擎。
- Word 只读诊断确认：有效 64 位 COM 目标是 WINWORD，32 位目标是 WPS；新增 QA preflight 会拒绝把 WPS 当 Word 验收。**历史 45 秒超时缺少工作进程位数证据，原因仍未确定**，未改注册表或用户默认项。
- VS Code 根路径原本正确，撤回早期误判；新增 `--plan` 已确认仓库与实际二进制路径存在，未启动 Extension Host。
- Calibre 9.16.0 的真实 console EPUB3→HTMLZ→EPUB3 流程：文字、4 条跨章／脚注返回链接和脚注角色保留，但 2 个 MathML（含块公式、分式）全部丢失。严格判据失败保留；输入是独立合成书，**不算 ReadMD 导出或阅读器原生认证**。未为外部 HTML 通道的损失修改 ReadMD。

## 候选与资源边界

- EXE SHA256：`dca033f3c917f077e951e0b94ad49183c4f8163ff89236b198df18dc77fb07d0`。
- boot SHA256：`48c81634b56fe056b360e5edbaba1aeee11674f8d4e4214f92895b306db79ba9`。
- 965 份源码／QA 文件，manifest SHA256：`31606137e95f30753eb5145ba5ae1258d4a00a629475c6a03aa909902ab73ca4`；包含 toolbar 回归、实际 VS Code 扩展源码、能力锁和新资料工具。
- [当前候选](../evidence/capability-upgrade/2026-10-19/batch3-nogui-continuation-scoped/candidates/candidate-14/candidate.json)；源和目标的原始范围以冻结清单为准。正式安装、打包、版本发布均未执行。
- candidate-13 的失败源码与最终 candidate-14 一并归档。共归档 750 文件、26,861,577 字节。复用上一轮源码归档，仅增加 44 个不同源码内容；没有归档运行库副本或用户配置。
- 复用现有 34,822 文件的 QA TeX 运行库；新增最小 QA assets 只有 9 文件／7,613,510 字节。构建只用共享 target，增量编译关闭。本轮使用合成文件；旧未知 PDF、journal 和原 WPS 实例保持原样。

## 尚未完成和下一步

完整 OMML、全部出版参数、真实 Word／WPS／LibreOffice、同书独立阅读器、Extension Host、I03／I08／完整 I10、D01–D03／D07、八小时压力和非 Windows 原生要求仍保留。75 条账本仍为 59 进行中、16 待办、0 整项验收。

后续优先：按统一来源闭包补齐 D04/D05 的正式材料与二进制来源；安排有明确 OS 缓存条件的 D06；继续无界面的持久任务、空间／配额、锁文件、取消和回滚故障链。外部应用的真正编辑互操作另行取得实际证据。第四批持久审阅路径继续保留，不用本轮限定通过替代第三批完整出口。
