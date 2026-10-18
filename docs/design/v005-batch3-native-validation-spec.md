# 第三批原生验证链路收口

2026-10-12。沿用 B3-51–57 的完整合同。本增量仅消除旧 QA 脚本的十条 Python 门槛失败，补齐独立输出完整性检查，保留真实引擎的物理故障测试。不改变产品依赖、正式能力包或版本发布状态。

| 检查 | 不可降低的要求 | 实测证据 |
| --- | --- | --- |
| Word 材料 | 15 份 DOCX 的 45 个部件与保留原材料逐字节相同，包括非默认关系路径、命名空间别名、显式关闭与循环样式 | XML/关系字节比较及独立 .NET ZIP/XML 解析 |
| 数学实际流程 | 原生正式 CSP 下导入、渲染、结构修改、撤销、保存、重开、实际多格式输出，原断言保持 | defaults、paragraphs、small-fractions、word-styles 四组 |
| OCR 材料 | 原迁移材料可真实识别；3000×3500 压力图不得减少内容或缩放 | 实际 Tesseract；与保留压力图逐像素比较 |
| 输出验证 | ZIP CRC32、长度、目录、局部头、重复部件检查；由独立 XML 解析器检查汉字、可编辑分式和 HTML 表格 | 损坏 ZIP 拒绝专项及实际 MCP 输出 |
| 引擎故障 | 确认 Node 祖先、拥有者、引擎路径和摘要；固定进程句柄；逐线程验证并真正暂停；只读观测 OS 锁 | 两个实际 Typst/OCR 拥有者、并发拒绝、取消、崩溃与重启恢复 |
| 清理 | 每个线程只抵消本测试增加的一次暂停；只处置归属已验证的测试进程；任何救援都使成功清理验收失败 | 正常退出后 rescue 为空，工作区与资源回收通过 |
| 仓库门槛 | 不新增豁免、不删除故障检查；无 Python、资源、接线、46 语言结构与前端包时效门槛通过 | 实际命令输出和固定摘要 |

可复现入口：`node --test ui-tests/zip-integrity.test.cjs`、四个 `ui-tests/math-*-native.cjs` 专项、`ui-tests/engine-legacy-native.cjs`、`ui-tests/task-storage-native.cjs`、`ui-tests/managed-engines-process-native.cjs`。原生检查使用 `READMD_BIN` 指定同一正式 CSP 开发二进制；任务存储使用 `READMD_WORKSPACE_TEST_BIN` 指定同一源码编译的真实 Rust 测试辅助程序。所需字体、Typst 和 OCR 包通过显式环境变量或 `locked-packs` 复用，不在线下载。Windows 观察器仅为 QA 使用，C# 编译和 Win32 调用通过系统 PowerShell/.NET 完成，不随产品分发。

本增量不替代 TeX 的全部创建边界故障、真实磁盘满、长期压力、独立应用互操作、非 Windows 实测，以及完整 OMML/出版能力验收。原始失败和修正后的验证分别保留，禁止将人工修正后的重跑记为首次全部成功。
