# 第三批下一执行单元：Windows 真实资源与任务故障

**最新状态（C84）：** 第1项的字体/Typst有限域已实现并真实通过，包含前置字体及缓存身份的共享预算、MCP两类资源锁取消、HTTP导出/预览取消和真实120秒自然超时，解除后新拥有者实际导出三份PDF。[结果与边界](../reviews/readmd-v005-batch3-resource-budget-citation-2026-10-24.md)。HTTP每条请求进入内部锁轮询未独立观测，OCR和全部I/O仍不因此认证；不能写成整个第1项或D01/D02完成。下一Windows故障单元为第2项，仍按下方固定上限实施；下面“尚未修改”等段落是保留的实施前调查。

这是待实施规格，不是通过记录。先收尾[公式默认属性和分页墨迹单元](v005-batch3-native-defaults-typst-ink-2026-10-24.md)，然后按下面顺序补 D01／D02。既有 23 项真实 Typst／OCR 进程矩阵及 10 项任务退役矩阵保留，不重复计作新增。

## 隔离与公共证据

使用全新、明确归属的私有 profile 和文档目录，不借用本轮只读复用的 `publication-engines-native-n2e7Ug`，不接触真实安装或用户文档。锁定包先核摘要、展开大小及空间；不复制 TeX 运行库。原始包保持只读，受控破坏仅限私有安装副本且所有读者均结束后。

每条记录候选与源摘要、原始传输、单调时间、实际拥有者／worker 进程对象、句柄、源及目标文件身份、资源摘要和退出回执。故障步骤与解除故障后的显式恢复是两次不同操作；不得重试覆盖首败。保留未知哨兵文件，禁止按宽泛目录名清理其它任务。

## 五个有界步骤

| 顺序 | 实现检查点 | 真实注入与通过条件 |
| --- | --- | --- |
| 1. 资源锁等待 | `capability_packs::resource_guard_checked`／`lease_reserved`、公开导出前置字体探测及缓存 key、Typst／OCR 获取租约的统一期限和取消 | 对新私有 profile 的真实资源锁持有 Windows 禁止共享句柄，提供 ready／release 屏障。覆盖 MCP 导出、HTTP 导出／预览的前置字体等待和实际引擎等待；等待计入同一有限预算，不能拿锁后重置。Typst 保留一条真实 120s 自然超时；OCR 正式识别保持既有 90s，探测为 12s。HTTP 用真实 cancel ack 核反馈 ≤200ms；MCP cancelled 通知按协议无回复，用独立屏障和拥有者／任务状态分别证明主机响应与 ≤2s 退役。未准入不得启动引擎或建立引擎工作区；已存在的外层任务工作区必须准确记录并受生命周期管理。释放后新 PID 显式恢复导出、独立核文件。 |
| 2. 跨传输拥有者 | `capability_process` 槽位、`worker_sandbox`、HTTP task cancel 与 MCP cancellation | HTTP 和 MCP 各持一个实际 Typst／OCR worker，同一私有 profile；第三个请求须准确忙碌。终止一个拥有者只回收它自己的 worker，另一个保持存活与租约；新拥有者占用释放槽位，明确取消与重试成功。两类引擎串行，每类含正常完成和崩溃两个子流程。 |
| 3. 读写竞争及修复 | 能力包 install／import／replacement／recovery，跨进程写入准入 | 两个实际读者存活时分别请求安装、重装、导入、卸载，核精确错误及资源未写；结束一个读者不能释放另一个的租约。全部结束后，才破坏一个已钉私有非执行成员并从锁定包修复，未知哨兵保留，再实际输出。补一条真实导入取消及配额回收，不冒充完整上传矩阵。 |
| 4. 最终目标被锁 | `content::write_bytes_atomic`、Typst save error 和两传输映射 | 对已成功生成的私有 PDF 持有 Windows 禁止写入／删除共享句柄，再经 MCP／HTTP 真实编译替换；必须证明引擎已成功，错误发生在最终提交。旧字节和文件身份保持、无临时 part 泄漏。仅释放该句柄，新 PID 明确再导出，独立核对正文、绘制和链接。此项不认证磁盘满或硬配额。 |
| 5. C50 混合负载超时 | `code_chunk_runner::run_process`、进程创建／终止／pipe drain、HTTP code run 的准入 | 保留 14.8302342s 原首败与孤立 3600ms 结果。固定 idle 与一个 Typst＋一个 OCR 的短混合两臂，分阶段测 request／precheck／runtime／spawn／deadline／kill／drain／response。原 2s 执行、总 8s 断言不变；MCP 无 timeout 参数，不向 schema 虚构字段。现 deadline 建于 spawn 和 pipe setup 之后，先定位延迟再决定代码调整。 |

首选 1，再 2。每个步骤独立实现、测试和验收，不能等八小时压力才能推进其它 Windows 功能。计划上限分别为 8／8／10／5／3 分钟；最多两台同类真实 worker，拥有者和观察者均限私有进程。不用 Cargo 并行构建充当真实生产混合负载。

2026-10-24 只读准备已定位第1项的确定缺口，尚未修改或验收：`mdexport` 的前置 `publication_fonts::scope` 早于 Typst 120s deadline；HTTP export 的 `key_with_data` 还位于 `speech::with_cancel` 之外，preview 也有前置 key 探测。下一次实现必须先把这些入口纳入 checked 预算及取消，不能只锁住引擎资源、测试内层等待就宣称公共全链完整。实际锁轮询与租约协议继续复用；短命版本探测须用拥有者 Job 的累计生命周期观测，瞬时进程快照不足以证明从未启动。

## 验收边界

拟建 `tools/test/batch3-windows-faults.cjs --case ...` 目前只是规划，不能列为可执行入口或通过证据。实现时先提供不启进程的 `--plan` 和独立控制，再统一离线构建、冻结新候选，逐步执行真实注入。输出和日志设容量、任务有取消与总 watchdog，故障不能损坏基础阅读或已有资源。

以上短矩阵不认证八小时、受控冷缓存 D06、真实 ENOSPC、完整后代树、所有传输／旧协议、桌面 UI 或其它系统。八小时仍须单独安排连续真实操作窗口；TeX 精确材料、Word／阅读器与其它平台环境缺口保持原状态。下一出版单元优先有界复杂 CSL／生成型引用作者格式，再逐项补剩余实际参数，不将其写成整项完成。

## 第2项实施路径细化：仍未验收

现有 `ui-tests/managed-engines-process-native.cjs` 的23项矩阵是两个MCP worker、HTTP仅执行资源写操作，不能替代HTTP worker拥有者覆盖。复用已有C#观察器的真实进程句柄、创建时间、程序摘要、父链及 `SuspendThread`；冻结真实 compile／recognize worker提供可观测屏障，跳过并恢复版本探测。该屏障只证明真实worker存活，不冒称引擎内部语义就绪。

必要的QA窄扩为：验证Node→Python broker→HTTP宿主的精确父链；按owned key只释放自己新增的挂起计数；检查共享资源租约；为预期崩溃提供精确拥有者适配。不能通过删除任意`instance.json`、放宽并发容量或排除同名进程获得通过。

OCR使用仓库原创 `assets/engines/ocr-probe.png`（1000×170，SHA-256 `b978c564d7f48e172d6bf3fa52f18cb81938f69348dbf531165ca68ca6150b29`，已知文字READMD／12345）。用已安装PyMuPDF平铺成3000×3500像素专用材料，保留原像素并固定摘要；不复跑旧管线中的浏览器材料生成。派生大图必须先取得真实识别成功证据。

按Typst正常完成、Typst拥有者崩溃、OCR正常完成、OCR拥有者崩溃四阶段串行。每阶段HTTP和MCP各持一个真实worker，第三请求准确忙碌；释放或结束一个拥有者，其同胞worker及租约继续存活；新拥有者占用空位，再明确取消并执行解除故障后的实际成功任务。Typst崩溃方向为MCP，OCR崩溃方向为HTTP，不泛化为所有排列。

整条矩阵硬上限8分钟：准备90秒、四阶段各90秒、最终回收30秒；最多两个同类引擎worker、三个ReadMD拥有者。实际屏障20秒，单次挂起不超过60秒；忙碌、HTTP取消反馈和MCP ping目标均≤200ms，worker及槽位退役≤2秒。私有安装≤128MiB，scratch及证据≤256MiB，小型证据≤32MiB；超预算保留失败，不提高上限。

已知风险是HTTP导出缓存key在worker容量判断之前探测字体及引擎，第三请求≤200ms不能仅靠代码检查保证。若真实测量失败，保留首败，再评估兼容缓存语义的前置廉价容量检查；不能放宽目标。所有观察器窄扩、材料和产品入口必须冻结新候选，先运行独立控制，再运行这条实际矩阵。
