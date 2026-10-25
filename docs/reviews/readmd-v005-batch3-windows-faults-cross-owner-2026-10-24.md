# 第三批Windows故障第2步：跨传输拥有者（实现与独立控制记录）

2026-10-24 交付代码、测试与QA编排；2026-10-25 两次验收轮实跑真实矩阵（candidate-94，二进制
b73aab39…）：**矩阵在第一步（深长私有 profile 安装三包）被同一产品缺陷阻塞**——`capability_packs/
space.rs` 的 `capacity()`（:64-77）对超过 MAX_PATH 的普通路径在 readmd.exe 进程内
`GetDiskFreeSpaceExW` 失败（422 `capability_install_failed`/`reason:space_query`）；同一二进制同一包
对短 profile（106 UTF-16）安装成功。对照实验链：windows crate 0.62.2 同绑定独立探针无 manifest 对
同一 421 字符 pack_root 失败（0x80070003）、嵌入 `longPathAware=true` 简版 manifest 成功、嵌入
**readmd.exe 逐字节相同的完整 manifest**（含 Common-Controls）也成功——排除 SxS 假设，缺陷完全
收窄到产品进程内空间查询对普通长路径不健壮；python.exe（manifest 生效）三 API 全部成功
（free≈31.4GB）。第二次验收轮核实重建门禁的二进制 sha256 与首败轮**相同**、`git diff HEAD` 对
space.rs 为空（重建未包含修复），replay 与 runid-06 双重复现。修复属 space.rs（建议对查询路径加
`\\?\` 前缀规范化，不在本单元 owned_files），未修改；**needs_code_change=true**，首败原样保留。
完整证据链见 [evidence/unitC](../evidence/capability-upgrade/2026-10-24/unitC/README.md)。除该产品
阻塞外，本单元代码与观察器窄扩已全部真实验证（下文）。矩阵 busy/取消/崩溃/收尾阶段待 space.rs
修复后重跑；本单元不改产品源码；规格见
[第2项实施路径](../design/v005-batch3-windows-fault-next-2026-10-24.md)。

## 实际改动（owned_files）

- `tools/test/batch3-windows-faults.cjs`（新建）：四阶段串行矩阵。`--plan` 仅打印计划JSON
  （`noProcessesStarted:true`、`noFilesWritten:true`，四阶段标签、固定崩溃方向 Typst=MCP/OCR=HTTP
  不可配置、每阶段传输分配表、全部上限常量、三包绑定与12文件冻结清单）；`--case <label>` 仅校验
  标签合法并打印该阶段计划。实跑（`--candidate --python --output`，win32门禁，输出目录必须是
  `.cache/capability-upgrade` 直接子目录且名为 `batch3-faults-native-<runid>`）：装三包（实测
  inventory 字节与128MiB核对）→ 正常停止安装宿主 → `inspect-windows-faults.py` 派生 3000×3500
  OCR材料并先取得一次真实识别成功（READMD+12345）→ C#观察器 → 四阶段串行，每阶段：MCP拥有者A与
  HTTP任务宿主B（Node→Python broker→宿主，arm 经两级精确父链）各持一个真实worker（observe物理
  冻结屏障≤20s）；第三请求经MCP与HTTP各一次（串行，断言≤200ms忙碌码且各自 jobAccounting 累计
  total 不变）；正常阶段 resume 后真实完成输出、崩溃阶段 kill 对应方向拥有者（HTTP方向由 harness
  直接 kill、不调用 `HttpHost.stop()`，等待 broker 自然收尾并核对 `broker-report.json` 崩溃形状：
  `passed=false`、`hostExitCode` 非零、`instanceRemoved=false`、error 含
  `Owned HTTP host exited before shutdown`，instance.json 残留如实记录、broker 非零退出码仅记录不判
  harness 失败）；每次结束后断言被结束者worker树≤2s全停、同胞worker存活、按引擎租约独占试探
  winerror 32（Typst=typst.lock+fonts.lock、OCR=ocr.lock）、旧目标字节不变、无 `.readmd-part` 泄漏；
  新拥有者一律MCP新stdio宿主（占空槽→notifications/cancelled取消：独立ping≤200ms、原请求无回复、
  worker停止+槽位退役≤2s→同拥有者真实重试成功，Typst经第1步pdf子命令核marker/字形/链接、OCR文本
  含READMD与12345）；阶段1–3存活HTTP同胞经 `POST /api/task/cancel` 明确取消（ack
  `{ok:true,state:'cancelling'}`≤200ms，终态≤2s且 `error_code=='cancelled'`：export 200、OCR 422，
  `canceled` 布尔字段恒false不作断言依据）后正常 `stop()`；阶段4 MCP同胞经cancelled通知取消后正常
  关闭。最终回收≤30s：直接spawn善后宿主（复用 managed-engines 直接spawn模式，验证产品“死
  instance.json ping失败→新宿主继续启动并覆写”语义）卸载 ocr→typst→fonts（各200/ok:true），
  instance.json 最终残留如实记录、绝不删除。
- `tools/test/inspect-windows-faults.py`（新建）：PyMuPDF 平铺派生（源摘要锁定
  `b978c564…b29`、几何固定3000×3500、原生比例平铺不重采样、固定摘要）与 OCR 文本校验（必须同时含
  READMD 与 12345，缺一即败）。
- `ui-tests/helpers/windows-worker-observer.cs` / `.ps1`（窄扩，纯增量）：构造器第二显式前缀
  `batch3-faults-native-`（缺省仍 `engine-mcp-native-`，父目录==canonical(.cache/capability-upgrade)
  约束不变）；arm 可选 `ownerParentPid` 两级精确父链（缺省仍父==Node）；arm 时 Assign 观察器私有
  accounting Job（无 kill-on-close）并新增 `jobAccounting` 动作（累计 total/active/terminated）；
  新增 `lease`（按 resource=typst|fonts|ocr 独占试探，只认 winerror 32 + 零字节身份）；新增 `resume`
  （只释放自己新增的挂起线程计数、保留 process 句柄）；新增 CreateJobObjectW/
  AssignProcessToJobObject/QueryInformationJobObject 三个 P/Invoke。PS1 透传新可选参数与三个新动作；
  既有 arm/observe/freeSlots/stopped/alive 语义与调用不变。
- `rust/readmd-kernel/src/capability_process.rs`：新增 lib 套件测试
  `cross_process_typst_ocr_crash_releases_only_owner_slots_and_keeps_sibling_leases`
  （测试二进制子进程 `--exact --test-threads=1`，env `READMD_FAULT_OWNER_ROOT/NAME/ENGINE`）：对
  typst 与 ocr 各一段，双子进程持满各自引擎worker槽+Shared资源租约→`check_worker_capacity` 返回
  typst_busy/ocr_busy→kill其一→容量恢复+新占位成功+幸存者槽位保持且Exclusive资源锁仍被Shared阻止
  （并验证另一引擎容量独立）→stop幸存者→锁文件0字节+Exclusive可获。既有 latex 跨进程测试未动。
- `tools/test/batch3-windows-faults.test.cjs`、`tools/test/inspect-windows-faults.test.py`（新建，
  独立控制）：plan结构纯函数与四阶段/传输表/上限逐项、三包与冻结清单、参数与case校验、busy拒绝
  >200ms、lease按引擎只认winerror 32、jobAccounting不变+正向校准、resume只按owned key、取消终态
  锚定（canceled字段不判通过）、崩溃形状断言、tile几何/摘要纯函数与OCR双token对抗控制。
- `tools/no-python.allow.json`：登记上述4个新文件（R1/R2），登记后 `check-no-python` 无命中无stale。

## 本轮定向检查（全部实跑）

- `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR="Z:/Natsumer/.codex/worktrees/b2bb/readmd/.cache/community-upgrade/build" cargo check --locked --offline -p readmd-kernel`：退出0（统一配方）。
- `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR="Z:/Natsumer/.codex/worktrees/b2bb/readmd/.cache/community-upgrade/build" cargo test --locked --offline -p readmd-kernel --lib cross_process_typst_ocr_crash_releases_only_owner_slots_and_keeps_sibling_leases -- --quiet`：
  **1通过/0失败**（其余套件按过滤排除，0.29s）。
- `node --test tools/test/batch3-windows-faults.test.cjs`：**20通过/0失败**。
- `python -I -B tools/test/inspect-windows-faults.test.py`：**10通过/0失败**（直接脚本方式，
  同既有 `.test.py` 形态）。
- `node tools/test/batch3-windows-faults.cjs --plan`、`--case typst-owner-crash`、`--case
  ocr-owner-crash`：退出0、仅打印；`--case bogus`：退出1并指明未知case；均未启进程未写文件。
- `node tools/check-no-python.mjs`：`no-python check ok (3983 tracked files, 114 allowlist entries)`。
- C#观察器经 Framework64 csc（.NET 4.8，即 Add-Type 实际编译器）编译：零错误零警告。
- 观察器分发冒烟（真实 powershell.exe 子进程，两种前缀各一次）：`ready` 通过且
  `batch3-faults-native-` 前缀被接受、缺省路径（不传 `-QaRootPrefix`）保持
  `engine-mcp-native-` 行为；`jobAccounting/lease/resume` 三个新动作与既有动作全部正确路由，
  未知动作报 `Unknown observer action`，无 rescue 输出、stdin EOF 后正常退出。

## 本轮受限项（如实记录）

- **真实验收矩阵被产品缺陷阻塞（2026-10-25 验收轮）**：candidate-94 实跑至第一步安装失败
  （深长 profile 422 space_query）。矩阵的 busy/取消/崩溃收尾/善后卸载等阶段全部未执行，未测即未证。
  修复路径与对照实验见文首与 evidence/unitC/README.md。
- 观察器实现轮的 arm+jobAccounting 冒烟曾在实现环境被误判为"沙箱Job限制"；验收轮定位为窄扩 bug
  （`OpenProcess` 缺 `PROCESS_SET_QUOTA`，windows-worker-observer.cs 的 Arm），修复后于真实
  readmd.exe --mcp 宿主端到端通过（arm/Job累计观测/kill后ownerAlive=false/lease独占试探32/freeSlots），
  见 evidence/unitC/observer-arm-smoke.json。该首败（三次 error 5）原样保留。
- 既有23项矩阵回归 `node ui-tests/managed-engines-process-native.cjs` 仍未在本轮运行（同样会被
  space_query 阻塞——它安装三包到深长 profile；这也反证该回归自 space.rs 引入后未再真实运行）。
- 冻结候选说明：C#观察器 helpers 在冻结 pattern（ui-tests/<file>.cjs|json 一层）之外，与所有既有
  candidate manifest 一致；harness 改为在 report 里记录两个 helpers 的 sha256/bytes（OBSERVER_FILES），
  冻结绑定清单 FILES 保持 tools/* 与 capability_process.rs。

## 验收轮已实测（candidate-94，实跑记录）

- `node tools/test/freeze-longtail-candidate.cjs candidate-94` → 1189 源 --verify 通过；
  binary b73aab39d6a907a0453ebf1520cf8d0c777edf5c6349bcc080ea488ffc7a30a8（83,841,024 字节），
  boot 627ac1119a3c9c2a5375da5fe3d17e328aff95f6fac6242428ef211d10760199。
- `node tools/test/batch3-windows-faults.cjs --candidate …/candidate-94/candidate.json
  --python T:/programming/project/Hermes/hermes-agent/venv/Scripts/python.exe
  --output .cache/capability-upgrade/batch3-faults-native-20261025-05`：退出1，
  failedStage=install pinned packages through a real HTTP owner，firstFailure=422 space_query
  （report.json 已归档 evidence/unitC/）。prepare 阶段的 HttpHost 就绪、broker 宿主 spawn、
  请求收发与 retain 全部真实工作到失败点。
- 深长 profile（389/421 UTF-16）安装失败 vs 短 profile（106）安装成功的完整对照、无/有 manifest
  探针对照、readmd.exe manifest 枚举：见 evidence/unitC/ 各文件（本轮全部实跑）。

## 验收轮计划与上限（硬性，不放宽）

- 准备≤90s、四阶段各≤90s、回收≤30s、总计≤8min（watchdog 480000ms）；busy/ack/ping≤200ms；
  退役≤2s；屏障≤20s；单次挂起≤60s；安装≤128MiB（inventory实测合计口径）；scratch+documents+证据
  ≤256MiB；单件小型证据≤32MiB。
- 已知真实风险如实暴露：HTTP第三请求 `/api/export` 先做 `key_with_data_checked`（字体指纹/引擎
  digest/全文档图片revision）再做容量检查（server.rs:8111、export_preview.rs:98→mdexport.rs:3017），
  长文档放大概率；若实测 >200ms，保留首败，前置廉价容量检查修复另立候选（不在本单元owned_files）。
- 全部失败写入 `report.json`（`firstFailure`+原始测量JSON落盘）供独立重读；不重试刷绿、不放宽
  断言、不删检查、不删 instance.json。

## 边界

本单元不认证：第1步（C84已过）、第3/4/5步（读写竞争/目标锁/C50混合负载）、ENOSPC、八小时、真实
外部应用互操作、其它平台、GUI；不宣称第三批整体完成。`--case` 只是不启进程的规划校验，不是通过
证据；实际矩阵必须以完整 `--candidate/--python/--output` 执行并冻结候选。
