# V0.0.7 共享审阅冲突解决验收

2026-11-19，本次完成共享建议发生重叠后的手动合并路径，并接入现有桌面分享面板和协作网页。未提交、推送或发布版本；本记录不代表 P2-A 整项结案。

以前的建议接受接口会在审阅范围发生变化时返回冲突，但没有后续合并入口。现在可查看原先审阅内容、当前共享内容和建议替换内容，输入合并文字，必要时重新选择目标范围，然后提交。两端使用同一审阅界面适配器和同一服务端接口。共享内容仍先成为本地草稿，明确保存才写入文档文件。

服务端重新鉴权，并检查待审状态、正文 SHA-256、持久变更计数和 UTF-16 编辑范围。手动合并不能通过覆盖标志跳过检查。原提案和可信锚点保留，决定记录处理人、时间、实际事务、逆向片段及前后变更计数；审阅决定和 CRDT 增量在同一 SQLite 事务中提交。存储失败不会把已分叉的编辑泄漏到房间缓存。

`review_clock` 的迁移与可信元数据清理在一个事务中完成。旧库内可能由旧客户端传入的 `_generation` 被移除，新审阅只记录服务端计数。SQLite 的 `AFTER INSERT ON updates` 触发器更新独立计数，日志压缩、重复投递、编辑撤销和进程重启不会把计数退回。正文修改后又撤销到相同字节时，旧的手动合并预览仍会被拒绝；旧计数的普通接受则通过服务端生成的相对锚点重新定位，并逐段比较原文。

本机 IndexedDB 增加独立的审阅历史缓存和合并草稿记录。它们不加入正文增量队列，不会被正文同步确认删除。离线重开可以恢复已确认历史与未提交合并文字；旧草稿和重开的草稿必须刷新服务端对照后才能应用。降权后保留合并草稿，服务端拒绝写入。网页关闭保护会提示尚未完成的审阅存储，桌面关闭准备会等待持久化。

| 验证 | 结果 |
| --- | --- |
| Rust `cargo test --no-default-features --locked --offline --lib collaboration -- --test-threads=1`，单共享 target、关闭增量编译 | 22 通过，0 失败 |
| 合成 SQLite/Yrs 测试 | 包含同文 ABA、128 次压缩后计数保留、重开拒绝旧确认、旧库迁移、权限变化、存储事务回滚、原提案保留、备份与拥有者租约 |
| 生产 CSP 与鉴权下的 headless Chrome 实际控件流程 | 6 项通过，0 页面异常 |
| 语言键集、占位符和现有本地化门禁 | `check-i18n: ok (46 locales, 3058 keys)` |
| JS 语法及差异空白检查 | 通过 |

浏览器六项分别覆盖：重叠后拒绝直接接受并恢复离线草稿；同文编辑和撤销使手动预览过期；降权与恢复编辑权限后继续合并；桌面显示共享决定历史并明确保存；桌面直接完成另一处结构冲突合并；个人撤销保持远端决定与原提案。所有文档、会话和凭证均为隔离合成资料，未占用日常浏览器资料目录或桌面。已关闭所属浏览器和内核进程。

本机可复核证据为 `.cache/v007-collaboration-generation-rust.log`、`.cache/v007-collaboration-generation-final/report.json` 及同目录截图。浏览器驱动为 `tools/test/collaboration-resolution-browser.cjs`，要求明确提供已有的 `READMD_BIN` 和 `READMD_PLAYWRIGHT`，不构建或下载依赖。

本次浏览器候选的内核 SHA-256 为 `d8b7f6a9b843e4fc737d307bb8afcce391c32243a3250662e793e124a4b0c341`，加载的 boot SHA-256 为 `098bb499175b6157e5c2f7f9141c41cdfc8f868eb66b4b8f08625a0cd7342d29`。源码后续有变化时应按新候选重新核验。

实现位于 `rust/readmd-kernel/src/collaboration.rs`、`collaboration_host.rs`、`assets/collaboration/session.js`、`reviews.js`、`reviews.css`、`client.js` 和 `assets/js/features/collaboration.js`。专属测试为 `collaboration_resolution_tests.rs`、`collaboration_host_contract_tests.rs` 与上述浏览器驱动。`assets/i18n` 的 46 个语言文件增加 21 个共享合并词条和 27 个压感绘图词条，保留同期章节、PDF 等其他词条。

本次浏览器验证不替代 macOS、Windows、Linux 的原生系统 WebView、真实多人、物理输入法或长时间运行验收。共享建议仍属于协作房间审阅记录，未转换为本地 AI 审阅数据库条目；共享决定撤销、Word 修订回流、TLS 和长期自托管认证继续独立待办。
