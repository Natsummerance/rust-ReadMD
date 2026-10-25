# V0.0.5 第四批路径前置研究（2026-10-15）

状态：只读代码审计与候选路径，供第三批验收后决定实施顺序；不是最终设计、实现记录或验收结果。本次只新增本文，未运行 Cargo、ReadMD、AI 请求或原生测试，未下载新依赖、改总账本或更新第三批完成状态。已有测试文件仅作为后续回归资产列出，不继承其历史通过结论。

审计工作树：`<repository>`。依据 [五批规格](<repository>/docs/design/v005-five-batch-spec.md:329) 与 [R063–R065 原验收账本](<repository>/docs/design/capability-upgrade-acceptance.json:12278)，第四批归属仍为 R014–R024、R031–R047、R050–R052、R062–R068、R070；B4-01 还复用前批 R001/R002/R058 的事务恢复能力。优先顺序是持久逐块审阅 B4-01 → 快照导出 B4-02 → Rust 多轮任务 B4-03，再接协作、程序员、PDF、媒体、图片和长篇场景。R075 的能力包交付属于跨批依赖，不重新计数。

## 1. 现有实现与实际缺口

| 公共链或场景 | 已读到的实现 | 尚缺的连接或语义 |
| --- | --- | --- |
| 正文事务 | [document_transaction.rs](<repository>/rust/readmd-kernel/src/document_transaction.rs:16) 的 `ContentTransaction/apply` 校验 UTF-16 边界、正文 SHA、重叠编辑与 origin，返回 inverse；上限 32 MiB/4096 操作。模块本身不写文件。 | 没有统一 `ReviewChange` 或独立审阅修订；inverse 不能直接套到经历后续手改的正文。正文摘要也不能区分“改过又撤销回同内容”的任务世代。 |
| AI 应用 | [features/ai.js::applyAi](<repository>/assets/js/features/ai.js:2632)、[editor.js::applyEditAiResult](<repository>/assets/js/editor/editor.js:1817) 固定标签页/选区快照，失败转恢复副本，应用前 checkpoint，按一次 CodeMirror 事务插入。部分路径允许全文唯一匹配重绑。 | 目前是一次应用和编辑器撤销；没有逐块 durable original/decision/lineage、保存重启后的逐块撤回，也没有人工接管之后的范围映射记录。不能把唯一字符串命中升级为持久块身份。 |
| 恢复与保存 | [document_history.rs](<repository>/rust/readmd-kernel/src/document_history.rs:8) 有压缩集中存储、原子写、普通草稿/版本/checkpoint；只有 `math_draft/math_recovery` 受特殊保护。普通记录受 30 天、每文档 10 条和总额约束。 [server.rs 保存入口](<repository>/rust/readmd-kernel/src/server.rs:2436) 在共享 SAVE_LOCK 下检查磁盘修订并保存正文/资源。 | 普通 checkpoint 会受清理策略影响，不可充当永不淘汰的未审原文。需要受引用保护的审阅基线及正文保存与审阅决策的分离；不能冒用数学记录 kind。现事务/历史/协作分别有 32/16/8 MiB 文本预算，接线时需显式处理差异。 |
| 导出与预览 | [export_preview.rs::key_with_data/prepare_with_data](<repository>/rust/readmd-kernel/src/export_preview.rs:15) 已纳入正文、选项、图片、文献/CSL及字体/引擎身份；预览与导出可共用 artifact，生成后复核 identity。缓存为进程内 6 份、600 秒。 [server.rs 导出提交](<repository>/rust/readmd-kernel/src/server.rs:8020) 对带 task_id 的多数格式先暂存，再取消检查与 rename；TeX 因资源目录有不同提交边界。 | 没有持久 `ExportSnapshot`，没有待审 ID 集合或接受范围绑定。hash 依赖不等于锁定资源字节；artifact 缓存不能作为重启后接受回执。文件提交与审阅数据库提交之间的崩溃窗口还需明确恢复协议。 |
| AI 传输/对话 | [server.rs::h_ai_chat](<repository>/rust/readmd-kernel/src/server.rs:4140)、[ai_providers.rs](<repository>/rust/readmd-kernel/src/ai_providers.rs:832) 已有协议适配、模型/连接配置、流式处理与取消接线。 [conversation_history.rs::handle](<repository>/rust/readmd-kernel/src/conversation_history.rs:11) 原子保存会话；当前保存模型只接受 user/assistant 文本，拒绝 ephemeral 消息。 | 没有持久 plan/tool-call/tool-result/checkpoint 状态机；对话文本不能记录工具是否已提交。不能据单次 chat/SSE 或历史会话存在宣称完整 Harness 已有。Antigravity、多模态工具轮和首字性能仍须实际协议用例。 |
| Rust 任务底座 | [cancel.rs::TaskGuard/register](<repository>/rust/readmd-kernel/src/cancel.rs:52) 是进程内取消表及 commit 竞争仲裁，Drop 注销；同 task_id 会取消旧请求。 [task_workspace.rs::Workspace/pin/release](<repository>/rust/readmd-kernel/src/task_workspace.rs:417) 有工作区日志、原生资源身份、租约、显式引用和恢复清理。 [capability_process.rs](<repository>/rust/readmd-kernel/src/capability_process.rs:9) 有跨进程资源/worker/传输预算；[worker_sandbox.rs](<repository>/rust/readmd-kernel/src/worker_sandbox.rs) 保留工作进程隔离。 | 资源恢复日志不是 AI 检查点，`pin` 也不替代任务业务状态。还缺固定文档身份、持久 round/tool 幂等记录、预算累计、UI 生命周期到后台任务的控制，以及重启默认暂停而非自动执行。前端 [task-feedback.js](<repository>/assets/js/core/task-feedback.js:20) 的 single-flight Map 是交互反馈，不是任务所有权数据库。 |
| 共用工具入口 | [mcp.rs::tools/call_tool_cancellable/validate_tool_arguments](<repository>/rust/readmd-kernel/src/mcp.rs:85) 已有 Rust schemas、验证、取消桥、经服务调用的文档/导出等工具。 | 可抽取共用注册/调用层，但当前 MCP 暴露列表不等于内部自主 agent 已能执行多轮工具。`readmd_read_document` 是磁盘字节接口，不可直接当未保存会话的 read-current。内部工具不可通过伪造 `confirm:true` 获得用户授权，也不应另实现一套读写服务。 |
| 协作 | [collaboration.rs](<repository>/rust/readmd-kernel/src/collaboration.rs:119) 已用 Yrs/UTF-16、SQLite snapshot+updates、角色/撤权、presence/附件及建议；`review` 建服务端 StickyIndex+原文校验，`resolve` 在同 DB 事务内提交 CRDT update 与 accepted 状态。 [session.js](<repository>/assets/collaboration/session.js:8) 有 IndexedDB 更新日志/待同步队列和 trackedOrigins 个人撤销；[features/collaboration.js](<repository>/assets/js/features/collaboration.js:103) 有退出前 checkpoint。 | 当前协作建议是“接受时才应用”，本地 AI 是“应用后待决定”，不能仅映射 status 就统一。还缺跨来源逐块撤回、结构级重叠双方保留和统一审阅授权；完整 3/10/50 人、乱序重启和自托管升级需独立验收。 |
| 图片 | [editor/image.js](<repository>/assets/js/editor/image.js:8) 当前是 canvas 裁剪/旋转/翻转/缩放与内存 40 步历史，插入固定原标签页。 [document_assets.rs::stage/materialize](<repository>/rust/readmd-kernel/src/document_assets.rs:39) 与 [image/save 的 stage 分支](<repository>/rust/readmd-kernel/src/server.rs:4471) 已支持集中暂存、引用解析、保存去重和失败回滚。 | 尚未见 Rust `ImageDocument` 图层/蒙版/可恢复操作栈及共享手工/AI/MCP像素算子。不能把浏览器 canvas 参数栈称为 Rust 非破坏模型；现资源暂存可复用，但待审候选仍需持久引用保留。 |
| PDF | [pdf_workspace.rs](<repository>/rust/readmd-kernel/src/pdf_workspace.rs:24) 有原 PDF 基线、操作/redo、修订保护、集中草稿/恢复、资源 worker 与保存；[pdfium_worker.rs::apply](<repository>/rust/readmd-kernel/src/pdfium_worker.rs:45) 真实操作文字/图片/删除/旋转/页面/批注/部分表单，并区分不可编辑对象。 | 不是空白待建模块；应复用现 worker/操作栈逐项补缺。跨正文审阅、扫描区域来源映射、AI候选及完整加密/签名/字体/未改对象保真仍需范围化验收；本研究未操作任何用户 PDF，也未认证其全部对象类型。 |

检索范围仅上述模块、其路由/依赖和对应测试目录；不存在某个同名 Rust 类型是检索事实，不用于证明所有等价实现均不存在。Word 修订导入、八小时稳定性、平台交付未在本次执行验证。

## 2. 官方成熟组件：复用边界与许可证

以下均已浏览官方仓库；没有复制其实现或测试。除已锁 Yrs/Yjs 外，`main` 链接只代表本日审阅内容，不能充当将来导入的锁文件。

| 项目 | 官方核对与组件边界 | 本路径取舍 |
| --- | --- | --- |
| Yrs / Yjs | 锁定 Yrs **0.28.0**；本机 `.cargo_vcs_info.json` 指向 `23b7f5693bbf9e7d26340c521ee8647f79bdfba2`，对应 [官方 Cargo.toml](https://github.com/y-crdt/y-crdt/blob/23b7f5693bbf9e7d26340c521ee8647f79bdfba2/yrs/Cargo.toml) 与 [MIT LICENSE](https://github.com/y-crdt/y-crdt/blob/23b7f5693bbf9e7d26340c521ee8647f79bdfba2/LICENSE) 已核。官方 [sticky positions](https://github.com/y-crdt/y-crdt/blob/23b7f5693bbf9e7d26340c521ee8647f79bdfba2/yrs/src/sticky_index.rs) 与 [UndoManager](https://github.com/y-crdt/y-crdt/blob/23b7f5693bbf9e7d26340c521ee8647f79bdfba2/yrs/src/undo.rs) 适合位置/来源撤销；权限、审阅决定、文件保存和导出提交仍由应用实现。 | **复用现有依赖与服务**，不再引入第二套 CRDT。不能把 StickyIndex 当语义冲突解析器，或把 UndoManager 当跨重启接受/撤回日志。Yjs 13.6.33 已在现 [构建锁](<repository>/tools/collaboration-bundle/package-lock.json)，[对应官方 MIT LICENSE](https://github.com/yjs/yjs/blob/v13.6.33/LICENSE) 已浏览；保持离线 bundle/CSP 入口。 |
| Automerge / automerge-repo | 核 [core 官方说明](https://github.com/automerge/automerge)、[MIT LICENSE](https://github.com/automerge/automerge/blob/main/LICENSE)；repo 层另有 [多文档/网络/存储 adapters](https://github.com/automerge/automerge-repo/blob/main/README.md) 和 [自身 MIT LICENSE](https://github.com/automerge/automerge-repo/blob/main/LICENSE)。核心 CRDT 与 repo 生命周期/存储是不同层。 | **只参考层次边界，当前不采用**：已有 Yrs/Yjs 线上格式、SQLite和客户端离线队列。换库会增加双数据格式迁移与同步桥，仍不提供 ReadMD 审阅语义或权限策略。官方 README 不是本机故障证据。 |
| Zed | 逐 crate 核官方 manifest：[agent](https://github.com/zed-industries/zed/blob/main/crates/agent/Cargo.toml) 与 [collab](https://github.com/zed-industries/zed/blob/main/crates/collab/Cargo.toml) 当前声明 **GPL-3.0-or-later**；[GPUI](https://github.com/zed-industries/zed/blob/main/crates/gpui/Cargo.toml) 是 **Apache-2.0**。agent 依赖 action_log/project/db/task/GPUI 等，不是独立可放入现 WebView 的小 Harness。 | **只参考任务、活动日志、编辑器/项目分层，不导入代码**。不能因 GPUI 宽松许可把 agent/collab 当同许可；也不为了此路径替换现编辑器或窗口系统。 |
| Continue | 核 [Apache-2.0 LICENSE](https://github.com/continuedev/continue/blob/main/LICENSE)、[tools/index.ts](https://github.com/continuedev/continue/blob/main/core/tools/index.ts) 与 [callTool.ts](https://github.com/continuedev/continue/blob/main/core/tools/callTool.ts)：工具定义、参数处理、built-in 与 HTTP/MCP adapter 分离，依赖 SDK、IDE/config/extras，编辑工具还具有客户端边界。 | **只参考注册表/adapter分层，当前不采用其 TS 运行时**。Rust 继续拥有参数验证、范围授权、取消与提交；不照搬工具参数 coercion 或模型名推荐表作为 ReadMD 的安全/能力判定。若以后选取代码，需另锁 commit、逐文件 notice 与依赖清单。 |

本机资产可以直接复用：`rusqlite=0.40.2`（bundled）、Yrs、sha2、serde、flate2、tempfile、现 CodeMirror 与 Yjs bundle；本研究不增加 crate、Node 包、系统运行时或 WASM 构建。官方仓库许可核查不替代最终安装包 notice/依赖审查。

### 2.1 PDF、媒体与绘画的补充上游核查

以下为 2026-10-15 另一次上游只读核查，不是新增依赖或已完成能力。既有锁版继续使用；引入新组件前另外固定 commit、许可、源码、二进制和体积，不把滚动主分支页面当离线资产锁。

| 工作包 | 可复用或学习的项目 | ReadMD 的具体选择与测试入口 |
| --- | --- | --- |
| B4-06 PDF 对象编辑 | [pdfium-render](https://github.com/ajrcarey/pdfium-render) 提供文本／图像对象、表单、签名检查及变换示例；其线程说明强调 PDFium 调用不能假设可并发。 | 仓库已锁 `pdfium-render=0.9.4`、`pdfium_7881`，优先补现有 `pdf_workspace.rs` 及受管 worker，不再引入第二个 PDF 编辑器。一个 worker 内串行对象修改，多文档并发用现有进程预算；用未修改区域像素与对象清单、可选择文字、原表单、保存重开验证，不能只比整页截图。 |
| B4-07 离线识别 | [whisper.cpp](https://github.com/ggml-org/whisper.cpp) 支持量化模型及多平台后端，项目代码为 [MIT](https://github.com/ggml-org/whisper.cpp/blob/master/LICENSE)。 | 引擎与模型独立能力包；每包锁实际型号、量化、语言、摘要及单独模型许可。先走 CPU 路径，GPU 作为可选加速，不依赖宿主 Python。首测含非零起点、双音轨和跨分段句子的真实媒体，校验 SRT/VTT 时间偏移及重启续作；逐字流式显示不能替代识别正确率与延迟实测。 |
| B4-07 媒体处理 | [FFmpeg 官方许可与分发说明](https://ffmpeg.org/legal.html)区分 LGPL 基础部分和会改变许可范围的可选配置。 | 固定受管构建配置及实际编入的库，匹配源包与 notice 后才形成可分发能力包；不从 PATH 寻找用户的 FFmpeg。按明确时间基做解码和轨道选择，输出统一 PCM 供识别，检查边界采样、取消、小时材料峰值空间与重复任务缓存。 |
| B4-08 图片算子 | [image-rs/image](https://github.com/image-rs/image) 提供 Rust 解码、像素缓冲及基础图像操作，采用 MIT／Apache-2.0。 | 本仓库已有锁定 `image=0.25.10`，且关闭默认特性、只开现有格式。旋转／裁剪／缩放／基础调色先复用它；色彩空间、alpha、蒙版、图层和操作历史仍需 ReadMD 自己的明确模型及像素测试，不能把解码库称作完整 Photoshop 能力。 |
| B4-08 绘画渲染候选 | [Vello](https://github.com/linebender/vello)区分 CPU、GPU 和研究性 compute 渲染器，[workspace 许可](https://github.com/linebender/vello/blob/main/Cargo.toml)为 Apache-2.0 或 MIT。 | 只作为 CPU／GPU 同场景渲染和分块合成的研究候选，本轮不添加。先验证现有画布是否真遇到性能瓶颈；若需要，锁定 CPU 路径并用相同笔迹、alpha／混合和失去设备后的恢复测试对照 GPU。压感、倾斜、防抖、选择与可编辑 SVG 保存仍由 ReadMD 模型负责，不把渲染器误当完整笔刷或图层编辑器。 |

这些选择首先服务既有三条公共链：所有编辑都产生同一可撤销事务，AI 只调用同一个工具注册表，产物资源在明确保存时物化。不会因新增格式能力再增加常驻工作台。此补充不表示上述工作包已开始实施，也不替代 Windows／其他平台的各自原生证据。

## 3. 三条公共链的最小候选接口

以下名称是供评审的候选接口，尚未落产品或定为最终 schema。

**审阅链。** 新的 `ReviewSession/ReviewChange` 由 Rust 保存 document_id、内容世代+摘要、独立 review_revision、精确 UTF-16 来源范围、原文/候选、作者/origin/task/parent_change_ids 与决策。磁盘字节修订与编辑器正文修订分开，不能混用编码/EOL转换前后的摘要。另记录 application 状态：协作的未应用建议与 AI 已显示的待审修改必须区分。接受 AI 待审项只改变审阅修订；不能重写同一正文或把 Ctrl+S 当接受。稳定块 ID 由源身份/操作产生，不能用 DOM textContent、全文第一次命中或内容 hash 单独代替。

复用 `document_transaction::apply` 做验证与一次正文事务，复用保存服务的磁盘冲突保护；新增审阅持久 store（现 bundled SQLite 已可用）保护原文、修订日志和资源引用。UI 的人工编辑也必须记录 origin/范围映射，Rust 才能在重启后保留混合手改。撤回只逆转仍归属该变更的片段；不相交手改可精确重定位，重叠/结构不明确时保留双方并报告冲突，不能盲套旧 inverse。协作 adapter 复用现 `review/resolve` 的授权与同 DB 原子提交；不要强迫非协作文档创建网络房间，也不要把两个数据库的先后写当跨库原子事务。

**快照链。** `ExportSnapshot` 至少固定 document_id、内容世代/摘要、review_revision、确切待接受 change IDs、选项/来源身份及资源与引擎 identity；另固定实际资源字节或可持有的 immutable lease。准备期间资源变更必须失效，不可只重新 hash 后继续读取别的版本。未应用协作建议不因导出顺便应用。

复用现 exporter、预览 artifact 与目标保护；先生成/提交该快照输出，成功后只接受其 ID 集合，期间新变更仍 pending。导出取消、目标锁、缺字/能力错误或资源冲突均不得推进决定。文件 rename 与审阅 store 分属提交域，需持久提交意图/回执及恢复判定：`prepared → artifact_committed → review_committed`。崩溃后依据受控目标身份/实际 artifact SHA 对账；不能确认则保留待审并显式要求恢复，不自动当完成。TeX 的 `.assets` 多文件路径需沿现独立提交/回滚合同扩展，第一条路径不绕过此差异声称全格式完成。

**任务链。** `TaskSession` 固定原 document_id/世代、调用者授权范围、工具注册表版本、round、预算累计、tool_call_id/输入摘要/提交回执/结果摘要及 checkpoint revision。复用现 provider adapters、MCP 的 schema/服务实现、cancel、worker sandbox/lease/预算；新增 Rust driver，而不是在 WebView Promise 中运行全生命周期。幂等键绑定 task+call+输入+来源修订，重复同输入取已提交结果，同 key 不同输入拒绝。外部工具不能提供恰好一次保证时，不自动重放不确定副作用。

视图切换/侧栏/托盘不换任务文档或取消；真正关闭任务页与退出触发取消、工作进程结束及 checkpoint。重启进入 paused/needs_user_resume，再次校验文档与资源，不自动执行模型请求。64 轮/30 分钟是既有规格上限：持久累计已用预算，恢复不能清零；用完进入明确 incomplete/budget_exhausted。日志只保存必要输入/结果引用，凭据只用 credential_id，不把 headers/key 写入 checkpoint。待审内容/产物仍有 pin 时，不因 worker finished 清走；释放引用要在业务记录持久确认之后。

## 4. 可先实施的一条端到端路径

建议第三批验收后先做**单个本地 Markdown、两个不重叠块、三只受限工具的审阅/快照闭环**，再按同模型扩格式/来源。这是候选最小纵向切片，不是 B4-01–03 整包范围的替代：

1. 用户在现 AI 入口启动固定文档任务；Rust 建会话与 checkpoint。工具先仅 `read_document`、`propose_review`、`export_snapshot`，前者读取固定 session 的当前未保存正文，磁盘读取只用于外部修订核对；统一复用既有底层验证/保存/导出服务。输出工具仍需明确选择目标与待接受范围。
2. 一次真实 AI 编辑产生两个源范围明确的候选块；Rust 持久化原文/候选/身份成功后，现编辑器以一次正文事务显示待审内容，装饰与块操作沿现正文区域呈现。故障单测可用合成 provider；最终流程另跑真实已配置 provider，不把替身计为 Antigravity 验收。
3. 用户对另一处手改、接受一块、保存并重启；正文保存成功但另一块仍 pending，能精确撤回且不丢手改。复制/字数读取可见正文，审阅标记不进入 Markdown。
4. 对剩余待审块明确选择“接受这些并导出”，创建固定 HTML 快照（含一张本地图片以验证资源身份）。生成期间新增另一块，导出成功仅提交旧快照集合；失败/取消仍全部待审。复用同 exporter，不制造新的渲染服务。
5. 切标签/收侧栏/进托盘任务继续绑定原文档；关闭任务页停止工具并留检查点；重启仅用户继续。首次就装预算/幂等/恢复状态，不把它们推到重型场景阶段。

候选修改边界：新增小型 `review`、`review_store`、`export_snapshot`、`task_session/driver` 模块；窄接 `document_transaction`、历史/资源保留策略、服务保存/导出、MCP registry 与现 AI/正文 UI。模型先实现纯状态转移/typed errors，再接同服务，最后原生保存重启。对外 schema、store 迁移、预算统一和退出挂钩须在正式实施规格中确认；本轮不发布最终 API。

后续仍须做：Word 修订回流、协作审阅授权/结构冲突和个人撤销、全部导出格式、多协议工具/真实 Antigravity 长文多模态、VS Code 同服务、PDF/图片/媒体/教学场景、压力及其它平台。不能以本切片通过关闭这些要求。

## 5. 首轮必须安排的失败测试（全部尚未执行）

| 测试 | 独立可观察期望 |
| --- | --- |
| 两段完全同文本、emoji/CRLF、第二段建议 | ID/UTF-16 范围只命中第二段；surrogate 中间边界拒绝；磁盘字节/EOL不被审阅载荷污染。 |
| AI 返回前切页/改原文/改后又撤回 | 原文档身份+世代校验，迟到结果不串页；不能仅因摘要回到原值接受旧任务。 |
| pending → Ctrl+S → 正常退出/重启 | 可见正文已保存，pending及原文仍在，逐块撤回可用，决定不会隐式全接受。异常退出另等持久化确认再结束自有进程。 |
| AI块前/后手改、块内手改、依赖块撤回 | 不相交手改保留；重叠/结构冲突不覆盖；依赖不可用时给明确状态，不按旧 inverse 回写邻块。 |
| ledger写入失败/配额满/普通历史过期 | 不显示虚假已持久确认；已确认原文/待审资源不丢，新操作明确失败，文档目录无临时旁文件。 |
| 同一 call 并发/迟到、同 key 不同输入 | 至多一次正文/文件提交；重放返回同回执，不同参数冲突；取消旧请求不能误取消新任务。 |
| snapshot后新增/改变/撤回审阅块 | 只可能接受确切快照集合和其版本；新增项保持待审，失效旧项不能顺带接受。 |
| 导出取消/写失败/目标锁/资源中途替换 | 旧目标不变，待审决定不变；导出实际采用固定资源字节，不能用变化后的图/CSL。 |
| artifact提交后、决定提交前崩溃 | 恢复根据实际文件身份+SHA对账；回执缺失或目标被别人改变不能伪报成功。 |
| 切页/侧栏/托盘/关任务页/真正退出/重启 | 分别观察继续、停止与 paused；owned子进程在停止后为0，重启无自动网络/工具副作用。 |
| 第64轮/30分钟耗尽、部分工具成功后失败 | 显示 incomplete及已完成操作，预算不重置，无“任务完成”假状态；不通过增加产品 timeout 过关。 |
| reviewer/reader/撤权、敌对客户端接受请求 | 服务端授权拒绝，正文/决定/附件不变；网络 CRDT 合并不绕过权限。 |

可复用的回归入口：现 [ai-context.spec.js](<repository>/ui-tests/ai-context.spec.js)、[ai-protocols.spec.js](<repository>/ui-tests/ai-protocols.spec.js)、[ai-skills-native.cjs](<repository>/ui-tests/ai-skills-native.cjs)、[task-storage-native.cjs](<repository>/ui-tests/task-storage-native.cjs)、[resource-save.spec.js](<repository>/ui-tests/resource-save.spec.js)、[collaboration-native.cjs](<repository>/ui-tests/collaboration-native.cjs)、[pdf-workspace-native.cjs](<repository>/ui-tests/pdf-workspace-native.cjs)。新增测试需同候选/CSP/资源版本绑定，保留首次失败；纯函数、合成故障、实际 provider 和真实桌面流程分别报告。本次没有运行这些入口。

## 审计切面指纹

只用于复核本文观察所对应的源；不是新 candidate、产物或测试证明。

| 文件/资产 | SHA-256 | 字节 |
| --- | --- | ---: |
| document_transaction.rs | `e46baef9b86acdc1fd270e1d2dfa7a3c5415a5709300ea548594802ebda7da5f` | 5660 |
| document_history.rs | `a795ac6ebec090f424a8a4f9ca807e98ada2d00c9ffd65ba150f53f5e306e46b` | 20795 |
| collaboration.rs | `5bd2a291dd451254b1c4c37d81768212818f98c12332b14333f974fb952d320a` | 46371 |
| export_preview.rs | `fdc9817f1b7f2026a17a3d9bfcb47fc5107a70690e3324251c8872f78586c137` | 12040 |
| cancel.rs | `3a286e87e80df214d0ff8de5dd8e1fc2070205d158e32c504808f5bbcd1f1423` | 8328 |
| task_workspace.rs | `281db637214c6b7f1720da4606c73267c7e1641ce8aeee62213aa60295e14bac` | 66673 |
| assets/vendor/yjs.bundle.js | `6e0367275c15c81e61eb6dd55be7c8efd102740f771dda4bc765f6cc6d0cf1a9` | 86074 |
| assets/vendor/yjs.LICENSE.txt | `341baa53605ed85d6f95782322854cca56c655ebc7fd4712649b8e7afc6020ff` | 1211 |
| assets/vendor/lib0.LICENSE.txt | `5446db1e43fe52faf3faab7e9959fe3762ac11bdc61c0945934c498affd52d73` | 1108 |
| assets/collaboration/session.js | `c29ce3745600df384919f75ea7bbd275d9306b34246f2924a6a221953a1f8709` | 7191 |

现 Cargo.lock 的 Yrs 0.28.0 crate checksum 为 `52c70dc8beca8666c77612a96889106ca3cd65318609721f464624ff79685da9`。上游协议兼容说明、功能表和测试均只用于研究，未转记为 ReadMD 通过；原第三批与 2026-10-14 证据保持原样。
