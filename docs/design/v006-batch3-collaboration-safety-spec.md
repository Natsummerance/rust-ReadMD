# V0.0.6 第三批：协作请求与单写入者安全规格

日期：2026-10-27。对应 [V6-33 执行计划](v006-execution-plan-2026-10-26.md)。状态：下述限定单元已通过 Windows 本机真实 HTTP／CLI 服务复测；V6-33 的完整多人、部署和审阅合同继续保留待验范围。本单元没有发布或推送版本。

## 已复现的两类首败

旧候选 `readmd.exe` 的 SHA-256 为 `f784e5492f17bbb720926f1570445649a0c48e4d4063446d109a0ce68f1c7555`。所有复现使用新建的合成资料目录，凭证仅为临时生成值；没有读取或复制用户私有连接配置。

### 变更字段被当作缺省值

桌面会话 API 和独立 LAN API 共用的请求分派存在两处问题：

- `sync` 将显式 `update:null`、布尔、数值、数组或对象当作没有增量，返回 HTTP 200 和 `synced:true`。依赖此确认清理离线队列的调用方可能丢弃尚未提交的增量。
- `permission` 将缺少 `role` 当作 `role:null`，实际撤销成员权限。该请求返回 200，随后成员访问变为 403。

真实旧二进制报告为 65 项检查、14 项失败：两个适配器各出现五类错误增量的成功确认，以及缺少 `role` 被接受和成员失去访问两个结果。首败保留在 [协议报告](../../.cache/capability-upgrade/collaboration-contract-s6cUef/report.json)。其余 51 项检查通过，不能把这 14 项改写为旧候选已通过。

### 同一数据库的独立缓存丢失已确认增量

两个可写 `Service` 可以打开同一个 SQLite 数据库。每个进程只在首次访问时加载房间，后续直接使用自己的 Yrs 缓存；压缩时又用本进程的完整状态覆盖快照并清空更新表。

真实复现通过两个 headless Rust HTTP 内核完成：A、B 都先加载相同原文；A 提交 `[ACK-A]` 得到 200；B 的缓存仍只有原文；B 提交 `[ACK-B]` 得到 200，再重复提交 B 的增量 126 次，使日志达到 128 次压缩阈值。停止两个进程并重启后，`[ACK-A]` 消失，`[ACK-B]` 保留。该内容丢失复现没有使用故障注入。

同一份探针另在合成数据库中临时添加延时触发器，制造撤权写锁竞争：另一服务中已经开始的同步，在撤权 API 确认之后才成功提交，重启后仍保留。撤权确认之后才发起的新请求正确返回 403。该结果限定为仪器化的在途写竞争，不表示新请求能绕过已提交的撤权。

原始结果保留在 [多拥有者首败](../../.cache/capability-upgrade/collaboration-shared-profile-J6yPPx/report.json)。该探针在清理时重复等待已通过信号退出的子进程，原报告保留 `shutdownFailed:true`；后续按复制探针可执行文件的绝对路径核对，存活进程为零，另存于 [进程核对](../../.cache/capability-upgrade/collaboration-shared-profile-J6yPPx/owned-process-check.json)。原报告未覆盖。

这些 `.cache` 报告是本机保留证据；随源码保留的复测入口见下文。

## 请求合同

| 操作与字段 | 接受的形状 | 拒绝与保全行为 |
| --- | --- | --- |
| `sync.update` 省略 | 拉取同步状态，不提交增量 | 保持既有客户端轮询行为 |
| `sync.update` 显式存在 | base64 字符串，继续由内核校验增量和额度 | `null`、布尔、数值、数组、对象返回 422 `collaboration_invalid`；没有成功确认、正文变更或 presence 变更 |
| `permission.role` | 显式有效非 owner 角色，或显式 `null` 撤权 | 省略、非法类型或未知枚举返回 422；不修改成员权限。现有 owner 保护继续由服务端执行 |

网页和桌面共用的 `assets/collaboration/session.js` 在没有待发增量时省略 `update`；成员管理始终显式提供角色，撤权使用 `role:null`。现有两种客户端形状保持兼容。拒绝请求后，调用方可以保留原始字节，正确重试；真实增量和重复投递仍通过既有 Yrs／SQLite 流程处理。

降权和撤权由服务端重新鉴权。延迟增量、评论和附件上传不能沿用旧角色；附件去重前仍需具备上传权限。已撤权成员不能通过已知附件 ID 下载或列举附件；其他房间中的有效成员不能读取该附件。

## 单数据库单写入者

`Service::open` 在打开 SQLite、执行建表／保留期清理或加载任何房间缓存之前，取得 `<数据库文件名>.owner.lock` 的系统级独占租约。

- 每个数据库只允许一个可写 `Service`，同一进程内的重复打开也被拒绝。重复拥有者返回 409 `collaboration_busy`；独立服务启动失败并非成功创建了第二个主持者。
- 父目录及已有数据库名称经过规范化，目录别名指向同一锁对象；数据库重定向及硬链接别名拒绝打开，避免 SQLite WAL 路径和拥有者身份分离。
- Windows 使用 `share_mode(0)` 和 `OPEN_REPARSE_POINT`，并检查已取得句柄的实际对象为普通、非重解析、空文件。Unix 使用 `O_NOFOLLOW | O_CLOEXEC` 及非阻塞 `flock(LOCK_EX | LOCK_NB)`。
- 租约由打开的系统句柄持有，锁文件始终为空，不包含 PID、凭证或正文，也不会被程序删除。正常释放时先关闭 SQLite 连接，再释放租约；进程终止后的系统句柄释放允许新的服务接手。Unix 的正常 Drop 显式解锁，遵循仓库现有租约约定。
- 锁或存储校验失败时停止打开／恢复，不覆盖现有数据库、备份或不相关锁对象。

该实现不提供多写入者或多副本部署。旧二进制不识别新租约，升级必须先停止旧进程，再启动新版本；不能让新旧版本同时使用同一资料目录。既有数据库格式没有因该租约改变，新增的稳定锁文件不是用户需要手工清理的过期标记。

当前界面对 `collaboration_busy` 可能只显示通用失败提示；专门的本地化反馈仍需后续补齐。失败不能作为会话创建成功的证据。

## 在线只读备份与恢复

独立 CLI 的 `--backup` 改用 `Service::backup_file`，通过 `SQLITE_OPEN_READ_ONLY` 打开源数据库并执行既有一致快照 `VACUUM INTO`。它不取得写入者租约，不建表、迁移或执行保留期清理，因此可以在服务仍持有租约并运行时备份已提交的 WAL 内容。服务内的管理员备份也继续可用。

恢复检查完整性与既有数据库结构，对新目标取得同一目标租约后再生成数据库；源备份只读。备份和恢复仍拒绝覆盖已有文件，锁失败和备份失败保留原目标。恢复到新目录后核对正文、评论、附件及成员权限，再切换服务。部署步骤见 [自托管、备份与升级](readmd-experience-and-capabilities.md#自托管备份与升级)。

## 实现与离线复测

业务实现位于 `rust/readmd-kernel/src/collaboration.rs`、`collaboration_host.rs` 和独立服务 `src/bin/readmd-collaboration.rs`；新增专属 Rust 测试为 `collaboration_host_contract_tests.rs`、`collaboration_lease_tests.rs`。没有新增依赖。

集成负责人统一执行的完整 Rust 回归为 4093 通过、0 失败、3 项既有忽略。该数字是集成候选的全仓回归结果。新增测试使用真实 Yrs、SQLite 和系统文件锁，包括独立测试进程被终止后重新取得租约。

2026-10-27 本机候选身份：

| 二进制 | SHA-256 |
| --- | --- |
| `readmd.exe` | `8f290f47d4dc4ddd4a31bdb9496a82693d1a58e1835189090491358d000ecb00` |
| `readmd-collaboration.exe` | `fd739bb61676c12fbbf8c89b84b993078a86c6b5ef664f4b7e1dd9a323a38599` |

| 真实服务复测 | 结果 | 本机报告 |
| --- | --- | --- |
| headless 桌面 API＋专属 LAN 请求合同 | 65 通过／0 失败 | [报告](../../.cache/capability-upgrade/collaboration-contract-WM8ksl/report.json) |
| 独立服务请求合同 | 43 通过／0 失败 | [报告](../../.cache/capability-upgrade/collaboration-contract-EjgNSr/report.json) |
| 重复进程及目录别名拒绝、进程终止后接手、在线 CLI 备份恢复、坏锁保全 | 23 通过／0 失败 | [报告](../../.cache/capability-upgrade/collaboration-owner-OfCaev/report.json) |
| 既有自托管备份／恢复流程 | 5 通过／0 失败 | [报告](../../.cache/capability-upgrade/selfhost-YkZ44l/report.json) |
| 既有 3／10／50 独立 Yjs 客户端 HTTP 场景 | 3 个场景均通过；已确认增量共 252 个 | [报告](../../.cache/capability-upgrade/collab-http-0gtiVU/report.json) |

HTTP 场景包含并发、重复投递、个人撤销、阅读者拒写、撤权、乱序依赖跨 128 次日志压缩及服务重启后补齐。3／10／50 客户端的本次同步 P95 为 14／28／95 ms；当时并行运行其他独立协作检查，这些数值只代表该负载。所有复测结束后，按五个资料目录限定的 Windows 进程库存为空。

在仓库根目录执行下列命令即可复测。需要预先构建的候选和已准备的锁定 Yjs 测试依赖；这些脚本不会拉取依赖或启动 Cargo，各自创建隔离合成资料和新报告。

```powershell
$env:READMD_BIN = (Resolve-Path .cache/community-upgrade/build/debug/readmd.exe).Path
Remove-Item Env:READMD_COLLAB_BIN -ErrorAction SilentlyContinue
node ui-tests/collaboration-mutation-contract.cjs

$env:READMD_COLLAB_BIN = (Resolve-Path .cache/community-upgrade/build/debug/readmd-collaboration.exe).Path
node ui-tests/collaboration-mutation-contract.cjs
node ui-tests/collaboration-owner-lease.cjs
node ui-tests/collaboration-selfhost.cjs
node ui-tests/collaboration-http-stress.cjs
```

## 未认证范围

本次真实 HTTP 客户端是独立 Yjs 文档；headless 桌面内核也没有打开原生编辑窗口。真实三人／十人／五十人原生操作、50 个原生窗口、物理输入法、八小时负载、受控空闲性能、其他平台运行以及 HTTPS 反向代理／证书部署继续待验。Windows 通过不替代 Unix 租约的运行证据。

评论和建议继续使用既有协作流程，本单元没有完成统一持久审阅接入、全部结构冲突矩阵或跨成员审阅回流。原生浏览器 IndexedDB 恢复的既有证据也没有因此次 HTTP 检查而扩大。上述通过只收口请求保全、单写入者和在线备份切片，不能结案 V6-33 整项。
