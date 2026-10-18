# V0.0.5 第四批首条完整流程规格

状态：设计已纳入第四批执行规格；未实施、未运行 Cargo/ReadMD/native/provider。
不改变第三批未完成项或任何发布状态。以
`docs/design/v005-batch4-execution-spec.md` 为主合同，本文补具体步骤与边界。
设计源索引与原创材料保存在 `.cache/capability-upgrade/batch4-first-slice-draft-20261015/`，不能当候选冻结或测试证明。

## 1. 先冻结四处容易误接的合同

1. `document_transaction::revision/apply` 的 SHA 只校验一份传入文本；
   `/api/documents/transaction` 不写会话或文件。新的 Rust session 必须在同一
   store CAS 中校验单调 generation、原文切片、事件序号及 review_revision。
   同文撤销回原样仍是新 generation；服务不信任客户端自行宣称的新世代。
2. `publication_blocks::node_identity` 含当前范围及子串 hash，第三批 citation
   projection 也绑定当次 revision/nonce。可复用源范围分析，不能把这些值当
   跨编辑、保存重启的 ReviewChange ID。用现有 uuid crate 创建稳定业务 ID，
   按有序操作日志重定位；同文两处也必须独立。
3. 当前上限是 transaction 32MiB、history 16MiB、collaboration 更新/快照
   8MiB，但 `collaboration::Service::create` 初始文本实际限 **4MiB**。
   新审阅首切片统一正文 UTF-8 上限取4MiB，仍用4096操作上限；这是
   已确定、待实现的新增合同，不能声称现入口已统一。资源/store保留额另
   按实际字节计费，达到限额拒绝新操作，不淘汰 pending 原文。旧普通阅读、
   保存能力不因新增审阅上限而缩小。
4. 快照 prepare 校验当时 generation/review_revision；finalize 校验所选
   `change_id+version`、依赖、当前归属及实际输出回执。不相交新 C 可以改变
   全局修订，不能据此顺便接受 C，也不能仅凭全局 revision 不相等误拒旧集合。
   所选 D 被修改/撤回则不得接受旧 D。目标锁/取消与实际 rename 谁先提交，
   仍依据现 `cancel::TaskGuard::try_commit`，不凭 UI Promise 猜结果。

## 2. 最小原创材料与独立期望

`fixtures/Review.md` 是 UTF-8 BOM+CRLF，一张本地 `figure.svg`；A、B 两处
正文完全相同且含甲/😀，中间另有手改段。有最小共享作者/机构及未知 YAML
注释；不修改它们。`fixture.json` 固定原文件 SHA、无BOM源字符串、CM-LF
字符串、两套明确命名的 UTF-16 范围及每一步独立期望全文/SHA。

真实 provider 的任务只允许读取本 session，并对A/B把 `original` 改为
`AI-reviewed`；必须返回两个精确来源块，禁止全文替换、改元信息/图片/手改段。
输出不符合工具 schema/范围即保留失败，不能静默把模型全文结果降格后通过。
provider/model/protocol/credential_id 可记录，不能写 key、headers 或用户配置。
故障 provider 仅供单元/集成；最终 native 必须是实际已配置 provider 同 Rust 服务。

这里的期望 SHA 只是原创 fixture 的算术检查，没有调用产品验证。块身份应由
服务回执产生，不能把 fixture 的A/B标签、DOM文字或数组索引当产品的稳定 ID。

## 3. 主流程：一次真实 AI → 保存 → 重启 → 单块撤回

| 步骤 | 实际动作 | 必须观察/保留 |
| --- | --- | --- |
| 0 | 新 owned profile/动态端口，正式 CSP，打开原 MD；另一标签放哨兵文档。 | 候选/boot/served boot/QA源绑定；内部文本无BOM、保存编码utf-8-sig；CRLF源与CM-LF范围不可混用。 |
| 1 | 在手改段先输入 `SESSION_ONLY`，不保存，flush人工操作队列。 | session/read_document看到未保存文本；磁盘仍为原 SHA；Rust确认新 generation，另标签未变。 |
| 2 | 现 AI 入口真实调用 read_document→propose_review(A/B)。 | Rust先持久事务提交正文基线、两块原文/候选/ID/版本/资源引用，再一次CM事务显示。两块均applied+pending，两个 ID 不同；工具失回执按call ID取结果，不重复应用。 |
| 3 | 中间段再输入 `HUMAN_STAYS` 并 flush；只点接受A。 | 手改日志映射B的新位置。接受A只改决定/review_revision，不重插候选、不增加内容 generation。B仍pending。 |
| 4 | Ctrl+S，正常退出 owned host，原进程全部停止后重开相同owned profile及文件。 | 文件=fixture期望savedAB，BOM/CRLF保留；pending B及original仍在；A accepted、手改仍在。重启 task paused/needs_user_resume，无自动模型请求。 |
| 5 | 正文B旁逐块撤回，只撤回B，再保存、退出、重开。 | B恢复original；A仍AI-reviewed；SESSION_ONLY/HUMAN_STAYS及未知YAML/图片引用逐字保留。B有withdrawn补偿事件/lineage；不能用本进程CM Ctrl+Z冒充持久逐块撤回。 |
| 6 | 在重开会话安全撤回已accepted A，保存后再重开。 | A恢复original，手改不丢；历史accepted事件保留，新补偿事件明确引用它。人工修改若重叠A则返回冲突并保留original/candidate/current，不能先删除旧决定。 |

建议未来新 review UI 用 `data-review-change-id`+动作属性定位，仍沿现正文
装饰/选区入口；这些控件目前不存在，本文没有声称旧 `edit-ai-apply` 就具有
逐块持久语义。复制/字数/搜索只使用可见正文，Markdown不落审阅载荷或标记。

人工输入可以保持编辑器交互，但其未确认队列不能被宣称已持久。AI读取、
决定、撤回、保存、导出前须等待按序CAS确认；失败保留当前手改副本并阻止
依赖该状态的新操作，不能用迟到的服务器全文回执覆盖用户继续输入的文字。
Undo/redo也要进入同一来源日志，不能只在服务外做内存逆事务。

## 4. 导出是第二段闭环，不复活已撤回的B

主流程撤回B后已没有“剩余pending B”。另在同文创建新的真实待审 **D**
（或在全新owned会话重做A/B；不能复制尚打开的SQLite/WAL冒充恢复）。
对D明确选择“接受这些并导出HTML”，固定 snapshot S：

1. S 保存当前源/有效选项/稀疏覆盖来源、D的确切版本及资源字节。独立原图
   SHA=A；现 HTML `embed_images_for_html` 可将冻结图片嵌入 data URI，复用
   原 exporter。不能让 exporter 继续从 live figure.svg 读取，不能只重算hash。
   必需图片丢失应在新快照层失败，不能继承旧 HTML 的“跳过并warning”当完整快照。
2. prepare后、正式执行S前，在不相交手改段加入已持久C。可用同工具的
   prepare/commit动作完成确定性交错，或测试用barrier做集成故障；无需新增
   production raw执行路由/额外常驻工作台。native必须记录实际交错，不能靠sleep猜。
3. 导出成功只接受D的快照版本，C保持pending；HTML取S的旧正文，不能包含C。
   独立读HTML的实际脚本Markdown载荷/最终DOM，解码data图片并核A字节SHA；
   关闭产品后HTML仍可离线读，不能依赖已释放temp路径。输出SHA和决定提交回执关联。
4. 把live图片换为原创B版本后执行S，允许的是冻结A仍输出或明确失效失败；
   绝不偷用B成功。改变/撤回D、锁目标、取消、写失败、缺字/排版错误都不能接受D。
5. `prepared→artifact_committed→review_committed`每边界做崩溃控制。文件rename
   与SQLite不是同一个原子提交；只有目标身份/实际SHA与持久意图一致才补回执。
   目标被外部替换时进入needs_recovery，保双方，不自动当成功或重新覆盖。

三只工具名称沿原规格：read_document、propose_review、export_snapshot。
prepare动作只是该服务内部合同的建议；当前没有这些已发布端点/CLI。
执行器授权来自用户授予的固定文档/范围/输出目标；model的 `confirm:true`
不能变成权限。既有MCP确认字段仍需兼容，但内部driver不能直接把模型参数
塞入 `mcp::call_tool` 并把旧确认门当真实用户授权。

## 5. 必须先写的失败用例/状态（名称是语义，非已注册错误码）

| 场景 | 退出状态/旧状态保护 | 独立判定 |
| --- | --- | --- |
| surrogate中点、过期正文、重叠范围 | 复用transaction_invalid_range/conflict/overlapping_edits，正文与决定不变 | 范围切片、原target SHA、事件数不增。 |
| 改后撤销回同文本再收迟到AI | stale_generation，保旧候选供恢复 | hash相同仍因generation不同拒绝，不串哨兵页。 |
| store写失败/迁移失败/未知新schema/预算满 | persistence_failed或read_only/unsupported_schema，不发durable成功 | 同一次DB事务回滚；原正文/库/决定完整。新增码由root冻结并46语言，不误借数学recovery。 |
| apply事务成功但UI丢回执/同call并发 | 重读同回执；同key不同参数冲突 | 实际一次正文提交、同change ID；不能靠取消注册表达成幂等。 |
| 普通history过期/10条清理后重启 | pending原文与resource pin仍可用 | 真实prune后恢复/撤回；不把math_draft类别当审阅保护。 |
| 人工不相交/重叠修改 | 前者精确撤回；后者conflict保三份 | 失败操作不半改正文/旧accepted事件；全撤回先整体预检。 |
| 外部磁盘修改/保存回执前崩溃 | save_conflict或needs_recovery | expected_revision及真实文件身份/字节，不凭路径一致认成功。 |
| 新C/失效D/目标锁/取消/资源换版 | 仅D可提交；失效/失败不推进决定 | artifact实际内容/SHA、D/C状态、旧目标字节同时核。 |
| 64轮/30分钟、部分工具成功后失败 | incomplete/budget_exhausted，重启paused | 累计预算不重置，已提交回执仍可查，owned worker停止；不增加现deadline。 |
| reader/reviewer/撤权或工具越权 | server authorization_denied | 正文/附件/决定不变；协作未来adapter不绕权限。 |

纯函数状态/SQLite故障、真实HTTP/stdinMCP同服务、正式CSP+实际provider原生
三类分别报告。受管worker/真正异常退出只结束本轮owned进程；无审批权限时
保留环境失败，不关用户Word/ReadMD/浏览器。物理磁盘满用隔离小卷，不填用户盘。

## 6. 可以立即开的最小代码单元与已存在接点

| 单元 | 新模块/复用点 | 退出条件 |
| --- | --- | --- |
| U1，先做 | review.rs/review_store.rs及专属测试；复用bundled rusqlite0.40.2、serde、uuid、sha2、document_transaction::apply | source generation/CAS、事件映射、两种application语义、迁移回滚/额度/幂等/跨连接重开；不先接多轮AI。 |
| U2 | editor updateListener、features/ai.js::applyAi、editor::applyEditAiResult；新review adapter只消费Rust回执 | 人工队列与AI原子显示、逐块操作、同文两处/迟到/undo可解释；不改为全文unique-match兜底。 |
| U3 | preview.js::saveEdit/saveEditOnce、server::h_save、document_history::SAVE_LOCK、document_assets::materialize | 主流程0–6及保存/重启/冲突保护。store意图与文件回执分域恢复。 |
| U4 | export_snapshot.rs；现mdexport::export_document_with_data/export_html、cancel::try_commit、资源解析与atomic writer | S/D/C闭环、固定图片字节、旧目标保护及崩溃对账。preview进程内6份/600s缓存不当durable receipt。 |
| U5 | task_session/driver；现ai_providers与MCP schema/执行adapter、worker_sandbox、capability lease | 三工具限定授权、多轮累计预算、恢复暂停；不把conversation_history或task_workspace WAL当业务状态机。 |

共享server/MCP/保存/退出hook由一个集成owner串行接；新模块各自独占。
本地审阅不强迫创建协作房间。Yrs StickyIndex可用于后续协作锚点，UI CM映射
可作即时位置显示；都不能替代持久日志、审阅决定、授权及重启补偿。

第三批回归只选受影响链：元信息/source trivia与BOM/CRLF事务、重复引用精准
定位/nonce不落源、P03/局部样式保留及unsupported早拒绝/旧文件保护、结构化
geometry错误传播、typed rich AST不flatten。此HTML首切片不继承Word/EPUB/PDF
原生认证，不要求重跑全部TeX/runtime；实际修改导出入口后按受影响格式另跑。

## 7. 开源边界与证据出口

已锁Yrs0.28.0的[固定commit MIT许可](https://github.com/y-crdt/y-crdt/blob/23b7f5693bbf9e7d26340c521ee8647f79bdfba2/LICENSE)
及Yjs13.6.33的[MIT许可](https://github.com/yjs/yjs/blob/v13.6.33/LICENSE)可沿现依赖复用并保notice。
Continue的[Apache-2.0许可](https://github.com/continuedev/continue/blob/main/LICENSE)
允许按条件复用，但本切片仅参考registry/adapter层次，不导入TS runtime；未来
拣代码需固定commit、逐文件版权/notice与依赖闭包。Zed
[agent manifest](https://github.com/zed-industries/zed/blob/main/crates/agent/Cargo.toml)
实际声明GPL-3.0-or-later；不复制其实现或用GPUI的不同许可替代agent许可。
main页面仅为本日只读核查，不是生产锁。本轮未取用任何上游源码/测试。
Automerge不引入，避免在现Yrs数据之外再建一种CRDT格式。

未来native报告至少绑定输入/实际provider与工具注册表版本/source/EXE/boot/served
boot/独立profile/实际保存字节、store回执与schema、snapshot资源及HTML SHA、
正常/异常重启次数、源不污染、owned停止结果。新错误和恢复文案需46语言真实
资源；不声称人工语言审校。首失败原件保留，不自动重试或把合成provider记真AI。
本草稿不关闭B4-01–09整包；Word回流、多人协作/自托管、实际Antigravity长文
多模态、媒体/绘画/数位板、其它平台和长期压力均仍独立未验。
