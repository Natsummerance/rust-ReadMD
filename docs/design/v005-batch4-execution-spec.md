# V0.0.5 第四批执行规格

状态：2026-10-15 的待实施执行方案，基于[路径研究](Z:/Natsumer/.codex/worktrees/b2bb/readmd/docs/design/v005-batch4-path-research-2026-10-15.md)与[五批归属](Z:/Natsumer/.codex/worktrees/b2bb/readmd/docs/design/v005-five-batch-spec.md:329)。本文不是实现或验收记录，不调整第三批、总账本或发布状态；第三批未完成的原生、互操作及交付门槛继续单列。下面的切片退出条件不能替代 B4-01–09 的完整出口。

## 1. 依赖与可并行边界

| 工作包 | 前置／现有资产 | 实施单元与出口 |
| --- | --- | --- |
| B4-01 持久逐块审阅 | `document_transaction.rs`、保存／恢复／资源服务 | Rust 审阅模型与 store → 会话正文及人工编辑日志 → 正文区域高亮／逐块操作。保存不等于接受；正常或异常重启仍可逐块撤回，保留不相交手改及受引用保护的原文。 |
| B4-02 固定导出与修订回流 | B4-01；现 exporter／preview；Word 原生环境另列 | Snapshot 固定内容、待接受集合及资源 → 文件与决定提交恢复协议 → Word 修订转同一 ReviewChange。只接受成功输出所绑定的集合；新变更、失败或取消不被顺便接受。首切片只收 HTML，其他格式和 Word 回流保留后续出口。 |
| B4-03 Rust 任务生命周期 | B4-01；输出工具须等待 B4-02；现 providers／cancel／MCP | 共用工具注册表 → 持久 TaskSession／调用回执 → Rust 多轮 driver → UI 生命周期。固定文档、真实授权／幂等／范围、累计64轮／30分钟预算；重启暂停等用户继续，实际 Antigravity／长文／多模态另验。 |
| B4-04 表格与协作 | B4-01；AI 建议依赖 B4-03；现 Yrs/Yjs／SQLite | 普通表格可独立补；协作将评论、建议与已应用 AI 修改接同一审阅语义，服务端决定授权、个人撤销与结构冲突保双方。真实3/10/50人及自托管备份／升级／HTTPS独立验收。 |
| B4-05 程序员链 | B4-01；AI／执行工具依赖 B4-03；语言包按需 | 仓库链接／标题与跨文件重命名先补事务回滚；OpenAPI3.0/3.1、代码环境／工作目录／取消、VS Code／MCP 同服务后接。打开不执行，失败不半改，不把宿主工具当随包能力。 |
| B4-06 导入与 PDF／OCR | B4-01；B3 输出与现 PDFium/OCR | 复用原 PDF 工作区和 worker 补对象编辑／损失报告／批量保存；扫描区域校订接审阅来源。保存仍是 PDF，未修改对象保真；不以整页截图重建称原位编辑。基础 PDF 可独立推进。 |
| B4-07 音视频 | B4-03；另审 FFmpeg／whisper.cpp 包 | 原始时间偏移／段轨选择 → 分段转写与 SRT/VTT → 回放定位／去重／断点。无宿主工具真实媒体，取消停止 owned worker，恢复不重复输出；资源许可先审不等于执行已通过。 |
| B4-08 图片与绘画 | B4-01／03；现 document_assets | Rust ImageDocument／算子／恢复 → 现就地简单编辑／专注画布 → AI/MCP 同参数。当前引用隔离、明确保存去重／依赖改写；图层／蒙版、SVG/PNG/PDF、数位板与60fps各自验收。 |
| B4-09 教学与长篇 | B4-01／03／06／08及 B3 出版 | 版本化讲义／练习／答案、思维图、章节与术语 → 扫描校订／AI审稿／作者确认／出版。专名／方言／情节仅建议；丰富多章材料的 EPUB/Word/PDF 真实保存重开及独立应用验证。 |

顺序是 B4-01 公共合同先冻结，B4-02 和 B4-03 的模型可随后并行，B4-03 的导出工具须等待 B4-02 提交协议。B4-04 普通表格、B4-05 链接事务、B4-06 基础 PDF 在 B4-01 后可并行；涉及 AI 多步副作用的分支等待 B4-03。媒体／字体／外部应用资料可独立只读准备，不能冒作实际验收。

公共 `server.rs`、`mcp.rs`、保存入口、`document_transaction` 与退出挂钩均由一个集成 owner 串行接线。各实现单元独占新模块和专属测试；前端、快照、任务和协作 adapter 只消费已冻结合同，不同时修改同一共享文件。root 唯一 Cargo／候选构建者，源码冻结后才统一测试；原生各用独立 owned profile，复用 profile 必须串行并证明前宿主已停止。

## 2. 第一纵向切片的稳定合同

首切片：一个本地 Markdown、两个独立文本块、一张本地图片、HTML 输出；真实已配置 provider 加三个受限工具 `read_document`、`propose_review`、`export_snapshot`。故障注入可用合成 provider，但最终原生流程必须走同一 Rust 服务及真实 provider，不能把替身结果记为产品成功。

| 实体 | 首次就必须固定的字段与规则 |
| --- | --- |
| DocumentSession | 稳定 document_id；单调内容 generation＋正文摘要；磁盘字节修订／路径身份单列。改后撤销回同文本仍是新 generation；未保存正文由会话读取，不能冒用磁盘内容。UTF-16 边界复用现事务检查，BOM／EOL 与磁盘哈希分离。 |
| ReviewChange | change_id、source identity、原文／候选、锚点与范围、作者／origin／task、父依赖、review_revision；`application=proposed/applied` 与 `decision=pending/accepted/rejected/withdrawn/conflict` 独立。协作未应用建议不能假扮 AI 已应用待审修改。 |
| ReviewStore | 复用已有 bundled SQLite，保存可恢复正文基线／操作日志、原文、决定与资源引用；schema 版本、并发 CAS／事务及迁移失败回滚。pending／可撤回记录的原文不受普通10条／30天草稿淘汰；不能冒用数学 recovery kind。达到预算明确拒绝新操作，不能悄悄淘汰未审原文。 |
| ExportSnapshot | snapshot_id、文档 generation/hash、review_revision、确切 change_id＋change_version 集合、选项、来源与引擎／字体 identity；固定实际图片／CSL等字节或 immutable lease。只重新算 hash 不等于固定随后读取的资源版本。 |
| TaskSession | task_id、固定文档／授权范围、注册表版本、round、累计预算、调用 ID／输入摘要／提交回执／checkpoint。幂等键绑定 task＋call＋参数＋来源修订；同键不同输入拒绝，不确定外部副作用不自动重放。凭据只存 credential_id，不写 key／headers。 |

首切片的新审阅正文 UTF-8 预算统一为 **4MiB、至多4096个操作**：当前协作更新／快照上限虽为8MiB，其会话创建入口实际只接受4MiB。实现时设共享预算常量和 typed error，不在不同入口各自 clamp；资源与持久原文另按实际字节计费。此限制只约束新的审阅操作，不阻止既有大文档普通阅读／保存。库、HTTP、MCP、VS Code 与内部工具必须调用同一服务，不能靠模型提供 `confirm:true` 取得权限。

审阅 ID 使用稳定业务身份，不能直接采用含当前范围和子串 hash 的 `publication_blocks::node_identity`。后者只用于当前修订的定位；跨人工编辑、撤销及重启的重定位依靠持久操作日志。人工输入、undo／redo也进入同一日志，保存／AI读取／审阅操作前等待其有序确认；迟到的服务器全文回执不得覆盖继续输入的文字。

## 3. 保存后可撤回的实际提交顺序

1. Rust 校验 session generation、精确范围／原文、工具授权和依赖，纯计算候选正文与 inverse；先在同一个 store 事务持久化可恢复会话正文、ReviewChange 原文／操作及资源引用，再向 UI 返回可显示回执。失败不显示“已持久保存”，UI 失去回执时按 ID 取同一次结果，不重复应用。
2. UI 用一次 CodeMirror 事务显示已确认候选，origin 绑定 change_id。人工编辑也提交有 base generation 的精确操作日志；接受 applied 修改只更新决定，不重插同一候选。复制、字数与正文保存只取可见正文，审阅装饰／后端原文不写入 Markdown。
3. Ctrl+S 复用原保存服务的磁盘冲突检查、资源物化与旧目标保护。记录保存的正文 generation／磁盘回执，**不修改 pending 决定**。磁盘文件提交与 store 回执之间留持久意图；重启只在文件身份／实际字节和日志可对账时补回执，否则保留双方进入恢复，不能凭路径猜成功。
4. 撤回根据后续操作日志／锚点重定位，在当前正文生成一个新的验证事务。不相交手改保留；范围重叠、源被外部改动或结构依赖不清时保留 original/candidate/current 并明确冲突，不盲套旧 inverse。accepted 的撤回记录新的补偿事件与 lineage，不抹去旧决定；全撤回按依赖逆序预检，失败不半改正文。
5. 重启恢复 Rust 会话、pending／原文／映射和资源 pin；普通历史清理不使逐块操作失效。任务恢复为 paused/needs_user_resume，核对文档与资源后由用户继续，不能自动联网或重放副作用。审阅资源只在业务记录持久释放引用后才交清理服务。

必要原生出口：真实 AI 产生 A/B 两块 → 用户在第三处手改并接受 A → Ctrl+S → 正常退出／重启 → B 仍 pending 且能单独撤回、A与手改保留 → 保存并再次重开。另测已接受 A 在重启后安全撤回及依赖冲突；仅 CodeMirror 当前内存 Ctrl+Z 不能通过本出口。

## 4. 导出与后台任务同一切片

“接受这些并导出”先固定 Snapshot 和所选集合：`prepared → artifact_committed → review_committed`。生成期间新建 C，成功仅接受快照中的旧 ID＋版本；输出失败／取消／目标锁／缺字／资源失配不推进决定。文件 rename 与 SQLite 不能假称跨域原子：持久提交意图＋目标身份／artifact SHA 回执用于崩溃对账，不能确认时保留待审／恢复状态。HTML 是首出口，TeX `.assets` 多文件提交及其他格式另按实际合同扩展；预览不是最终接受回执。

准备快照校验当时的内容／审阅修订；最终提交逐个核对所选 `change_id+version`、依赖及当前归属。不能仅因不相交的新 C 改变了全局审阅修订，就误拒仍有效的旧集合。若所选块已被修改或撤回，则旧版本不得接受。主流程撤回 B 后已无待审 B，导出测试须另建待审 D，以确定性交错验证 D／C，不能复活已撤回记录。具体原创材料、操作序列和失败判定见[首条完整流程规格](v005-batch4-first-workflow-spec.md)。

Task driver 归 Rust，复用 providers／MCP schema、cancel、worker sandbox／跨进程租约和预算。切标签／收侧栏／托盘不改变固定目标或取消；关闭任务页与真正退出取消并保存 checkpoint，owned 子进程结束另有证据。64轮／30分钟累计已用预算持久化，恢复不清零；预算耗尽必须 incomplete/budget_exhausted。view 的 Promise、single-flight Map、聊天历史及 task workspace 资源 WAL 都不能代替此业务状态机。

## 5. 首轮测试与失败出口

| 验证 | 必须观察的独立结果 |
| --- | --- |
| 重复正文／emoji／BOM／CRLF | 第二块仅命中第二块；surrogate 中间边界拒绝；保存后字节及源范围可解释。 |
| 手改与撤回 | 前后不相交手改逐字保留；重叠修改／依赖冲突不覆盖任一版本；全撤回失败不半提交。 |
| 保存／重启／清理 | 上述 A/B 原生流程通过；普通草稿淘汰后 pending 原文仍在；资源回收不删除 pinned 图片。 |
| store／迁移／配额失败 | 正文与决定不出现“假持久”；旧库／旧文件不坏；未知较新 schema 不被强制降级。物理磁盘满另在隔离小卷做，不填用户盘。 |
| 迟到与幂等 | 切页、改后回同内容、同call并发或同键不同输入；不串页、不重复提交、不误取消新任务。 |
| 导出竞态 | Snapshot 后 C 保持 pending；原块被改／撤回导致失效；资源被替换不能偷用新版。 |
| 导出故障／崩溃 | 目标锁、取消、写失败保持旧目标与决定；artifact提交后决定提交前崩溃按实际身份/SHA恢复，目标被外部改写不伪成功。 |
| 任务生命周期／预算 | 切页／托盘继续，关任务页停止，重启暂停；第64轮／30分钟不报完成且预算不重置，未完成操作可见。 |
| 协作授权后续接线 | reviewer／reader／撤权及敌对决定请求服务端拒绝；CRDT合并不绕过权限，冲突保双方。 |

纯状态／故障注入、共享服务集成、真实 provider 与生产 CSP 原生流程分开报告。绑定同 source/EXE/boot/工具注册表／能力包版本，保留第一次失败；不自动重试后覆盖失败、不扩大 timeout 或降低判定。实际 Word／独立阅读器、数位板、其它平台和长期压力继续独立待验，不能由 XML、截图或库测试继承通过。

## 6. 资产和组件决策

- 直接复用现 Yrs **0.28.0**／Yjs **13.6.33** 及 bundled rusqlite、serde、sha2、flate2、tempfile、CodeMirror，未引入新依赖。Yrs [固定 commit MIT](https://github.com/y-crdt/y-crdt/blob/23b7f5693bbf9e7d26340c521ee8647f79bdfba2/LICENSE)，Yjs [固定版本 MIT](https://github.com/yjs/yjs/blob/v13.6.33/LICENSE) 的审查沿研究记录；StickyIndex／UndoManager 用于位置和 origin 撤销，不充当审阅决定／权限／跨重启账本。
- [Automerge MIT](https://github.com/automerge/automerge/blob/main/LICENSE) 只参考存储／网络分层，不替换已存 Yrs 格式。[Zed agent](https://github.com/zed-industries/zed/blob/main/crates/agent/Cargo.toml)／[collab](https://github.com/zed-industries/zed/blob/main/crates/collab/Cargo.toml) 为 GPL-3.0-or-later，GPUI 的 Apache-2.0 不扩展到这些模块：只参考边界，不复制代码。
- [Continue Apache-2.0](https://github.com/continuedev/continue/blob/main/LICENSE) 仅参考 registry／adapter 分层，不带入 TS runtime。任何未来取用先锁 commit、具体文件及 notice／依赖闭包；这些 main 链接沿2026-10-15研究，不当新增生产锁。上游功能或测试不转记成 ReadMD 已实现／通过。

执行第一单元前由集成 owner 定稿 ReviewStore schema／预算／协议错误码与文件写权；先做 B4-01 可恢复状态和专属测试，再接正文 UI 与保存原生出口。本文新增文档而已，没有运行 Cargo、ReadMD、模型请求或本表测试，没有完成或发布第三／第四批。

## 7. 第三批实现带来的集成要求

以下为第三批增量实测后的设计补充，不表示第四批已实施，也不将第三批尚未完成的输出认证转记为通过。

- **快照固定有效参数。** 保存用户的稀疏覆盖、所选预设和继承来源；执行时通过共享 resolver 取得该段落／标题／表格真正使用的字号、字体和可用宽度。禁止用固定 12pt 等默认值预检所有公式，再以另一组参数输出。公式绝对尺寸与正文尺寸分别保留。
- **错误保留结构。** 桌面、MCP 和 Harness 工具回执保留经过校验的几何位置及所需／可用尺寸，不能只传一个错误字符串。2026-10-15 已修复的 MCP 定位丢失就是这条合同的回归例；第四批快照失败不得据此错误推进审阅决定。
- **正文与出版样式分开。** 普通正文的行距不应顺带覆盖标题、脚注和引用的专用排版。任务编辑内容时保留原有结构与局部覆盖；能力不支持时明确返回诊断，不能通过扁平化得到一个看似成功的文件。
- **富文本按实际组合验证。** 相邻引用片段可能跨样式、列和页面，单片段测宽不能证明组合后不越界。保留实际空白和完整文字，按每列与读序检查；不能把不可见的公式复制层坐标当可见文字基线。
- **复用不继承验收。** 第三批内核、真实进程、独立 PDF 解析和外部阅读器结果各自绑定输入／候选。第四批改动这些链路后只重验受影响流程；Word 原生、其它平台和长期压力仍需各自证据。

### 2026-10-17 收尾补充：内容身份与导出失败

- **源位置来自原文件。** Markdown 解析器可能规范 CRLF，HTML carrier 也可能包含多个声明和可见正文。审阅锚点必须映射回原始 UTF-16 范围，覆盖 BOM、emoji、同一行重复声明及换行；不能把整个解析事件范围当作单一修改对象。正常正文不得因附带元数据而被吞掉。
- **字形与文字身份分开。** 同一字体的两个 Unicode 字符可能共用 GID。固定字体文件及字形仍不足以固定复制／搜索语义；PDF 的 CID／ToUnicode 要保留原字符身份，复合字形的组件引用另行映射。快照回执检查实际文字映射、轮廓和字体字节，不能仅凭截图或 ActualText 宣称保真。
- **字体失败也使导出失败。** 子集容量耗尽、组件缺失及嵌入构建失败不得静默略过字体并提交文件；沿现输出提交门保持旧目标和待审状态。B4 的导出成功回执必须在这些检查之后产生。
- **传输入口遵循同一继承规则。** 桌面、HTTP、MCP 与 Harness 的 schema 和运行时对 `null`、省略、`false`、空数组必须一致；模型调用不能把“继承”变成“禁用”，也不能绕过实际目标校验。使用同一合同的正反例，不建立第二套解析器。

此补充是第四批设计约束，第三批相关实现仍须各自完成测试。已审阅的 [Yrs](https://github.com/y-crdt/y-crdt) 用于协作位置／操作同步，[Continue](https://github.com/continuedev/continue) 用于参考工具注册与适配层次；没有新增第三方运行时或复制未锁定的上游源码。
