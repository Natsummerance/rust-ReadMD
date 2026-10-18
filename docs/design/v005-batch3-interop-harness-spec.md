# 第三批：外部应用互操作入口与证据边界

日期：2026-10-13。范围为 I01–I07 的 Windows 外部应用盘点与可重复的局部 Word 验证入口；不修改产品依赖、验收账本或既有冻结证据。I08–I10 保留现有原生、VS Code 与 MCP 流程。

## 已补齐的验收基础

旧 `tools/test/word-publication-probe.cjs` 只保存重开，没有真正编辑公式；其 45 秒超时不保证 COM Word 同时退出，也只适合固定两节模板。旧证据原样保留，新入口为 `tools/test/interop-acceptance.cjs`。

- 默认仅查询 HKLM/HKCU 安装注册、32/64 位 App Paths、有限的已知程序位置和进程名。不读取窗口标题、用户打开的文档、浏览器数据或目录内容。
- 分开报告 `installed_unverified`、`registration_only`、`user_instance_preserved`、`inspection_incomplete` 和 `not_detected`。限定范围未检出不等于全机没有安装；程序存在不等于可用，更不等于原生验收通过。
- 每次使用独立 `.cache/capability-upgrade/interop-*` 目录，输入只复制，不覆盖已有结果。报告绑定脚本与夹具 SHA-256，不携带原始私人目录。
- 只有明确指定 `--word --docx` 才启动一次 Word 工作进程，无自动重试。它检查已有 Word 实例、确认新进程 ID 与创建时间、确认无其他文档，再打开复制的夹具。
- 局部 Word 流程在真实 OMath 内修改一个字母，更新字段、分页、另存 DOCX、导出 Word 自己的 PDF，再由 Word 重开检查修改与公式/节/栏/字段/注释数量。
- 原生局部成功只标记 `partial_native_roundtrip`，所有 I01–I03 完整要求继续未验收。静默 COM 流程没有观察修复对话框，也没有视觉审校，不能声称全类公式无损。
- 工作阶段写入报告。超时后只允许按进程名、ID、精确创建时间清理已证明归属的 Word；COM 激活尚未返回而无法证明归属时明确报告，不按名称批量杀进程。

## 用法

在仓库根目录执行；当前环境 Node 的盘符重定向需要两个 preserve-symlinks 参数：

```powershell
node --preserve-symlinks --preserve-symlinks-main tools/test/interop-acceptance.cjs
node --preserve-symlinks --preserve-symlinks-main --test tools/test/interop-acceptance.test.cjs
```

准备专用、已由当前 ReadMD 候选导出的 DOCX/EPUB，可只复制并盘点；不启动应用：

```powershell
node --preserve-symlinks --preserve-symlinks-main tools/test/interop-acceptance.cjs --docx <专用导出材料.docx> --epub <专用导出材料.epub>
```

Word 启动故障已处理或环境已改变后，再运行单次局部验证：

```powershell
node --preserve-symlinks --preserve-symlinks-main tools/test/interop-acceptance.cjs --docx <包含可编辑公式及英文字母的专用材料.docx> --word --timeout-ms 45000
```

脚本通过一次性 PowerShell 命令执行已记录摘要的本地源码，不修改系统执行策略。首次直接用 `-File` 的盘点遭本机签名策略拒绝，保留于 `.cache/capability-upgrade/interop-klqmkN/report.json`；随后统一为现有 QA 已使用的进程内命令方式。

## 本次实际检查

当前只读报告：`.cache/capability-upgrade/interop-LH75Oq/report.json`（与本次最终脚本摘要一致）。

| 应用 | 本次事实 | 验收状态 |
| --- | --- | --- |
| Microsoft Word | 注册和 WINWORD.EXE 版本均为 16.0.19426.20218 | 已安装，未运行真实编辑；此前 COM 启动超时未盲目重试 |
| WPS Writer | 既有用户进程 PID 7980 | 保留该进程，没有接管/关闭/读取文档 |
| LibreOffice Writer | 限定注册与常见路径未检出 | 未实测，不能认定全机未安装 |
| Thorium | 限定注册与常见路径未检出 | 未实测 |
| Calibre Ebook Viewer | 限定注册与常见路径未检出 | 未实测 |

6 个 Node 测试通过：安装状态不能伪装原生通过、用户实例优先保护、注册/查询不完整/未检出分类、Word 显式启用与超时边界、进程归属输入、夹具复制不覆盖及原件保持。3 个 PowerShell 文件 AST 解析通过。另实际执行了工作进程的无夹具前置检查，得到 `WORD_FIXTURE_REQUIRED`，没有激活 COM；实际执行清理器的归属不匹配分支，得到 `ownership_mismatch`，没有结束任何进程。

这些是 QA 入口及前置防护验证，不计作产品 OMML/出版功能通过；Word 成功分支仍需要真实环境运行。测试没有启动 ReadMD、没有运行 Cargo、没有修改用户数据。

## 完整原生互操作仍需取得的证据

| 项目 | 材料和操作 | 必须留下的证据 |
| --- | --- | --- |
| I01 | 同一候选产出的 19 对象/属性矩阵，Word 修改后回到 ReadMD | 导入前后 DOCX、可编辑 OMath、语义差异、视觉与修复提示检查、局部修改精确回读 |
| I02 | 论文/书稿/报告，真实字段更新、节、栏、奇偶页、注释与表格 | Word 分页 PDF、更新字段值、样式/节参数、保存重开结果；不能用 Typst PDF 代替 |
| I03 | Word 修改公式/段落/题注，并记录修订 | ReadMD 差异审阅与损失报告；不能静默合并未支持格式 |
| I04 | 独立拥有的 WPS 实例执行 I01/I02 子集 | WPS 版本、截图、输入输出文件、编辑与字段行为；不替代 Word |
| I05 | 独立 Writer GUI 实例编辑、更新、DOCX 重开 | GUI 过程及数学对象变化；headless 转换只能补充 |
| I06 | Thorium 导入相同 EPUB，目录、跨章、公式、脚注、字号、主题、退出重开 | 版本、设置、实际画面、阅读进度；EPUBCheck 只是前置 |
| I07 | 第二种阅读引擎打开同一 EPUB | 与 I06 同内容对照；标准错误和阅读器能力差异分开记录 |

当前机器未提供其他平台；这些脚本不为 macOS/Linux 出具原生证据。独立 QA 应用不进入 ReadMD 的运行时依赖。

## 对照依据

Word 公式转换与编辑使用官方对象模型：[OMath.Linearize](https://learn.microsoft.com/en-us/office/vba/api/word.omath.linearize)、[OMaths.Add](https://learn.microsoft.com/en-us/office/vba/api/word.omaths.add)。当前局部修改直接编辑已有 OMath 的字符范围，避免只新建旁边公式冒充原结构可编辑。

打开专用材料显式关闭 Recent 列表写入，不请求修复模式；参数依据 [Documents.Open](https://learn.microsoft.com/en-us/office/vba/api/word.documents.open)。完整 GUI 修复提示与视觉检查另列验收，自动化成功不替代它们。
