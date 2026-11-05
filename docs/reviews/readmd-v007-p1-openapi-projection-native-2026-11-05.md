# V0.0.7 P1：OpenAPI 可读审阅的真实 Extension Host

2026-11-05，UI 源码 `457a1fc98982e58295375ef6e7f2afb12c829eab`，冻结内核生产者 `7a6ca78e31361ff542000a3a09d0f37707aa1673`，完整 Rust 子树等价。使用本机安装的 VS Code 1.143.1 ARM64 和专用测试配置，实际原生 Extension Host 完成 8 项业务检查及 3 项投影检查。[机器记录](readmd-v007-p1-openapi-projection-native-2026-11-05.json)保留源码、out、内核及实际回执摘要。

默认 diff 用可读摘要替代已验证的内部 metadata，不再让多行编码载荷占据正文区域；可显式切到完整原始内容，再切回摘要。切换只改变只读虚拟文档，目标原文及待应用的完整事务不变。metadata-only 更新明确区分 Previous／Updated，业务操作变化为零。未知或损坏载荷仍显示完整原文，不猜解或丢弃。

实际 Cancel 不改文档和磁盘；Raw 中 Accept 应用完整原事务，一次撤销成立，磁盘仅在 Save 后改变。编辑器关闭重开、幂等更新、依赖文件变化、作者并发编辑和生成区手改的拒绝与保护均通过。这里的关闭重开与原生 undo/redo 在同一 VS Code 进程内，不是 Rust 跨进程持久历史。

10 张新截图都先等待准确审阅通知的真实动画结束、几何稳定与按钮命中，再截图。已逐张独立复看：摘要正文可读，原文切换明确，metadata-only 差异可辨；动作按钮完整位于画面内，旧通知作为独立卡片堆叠，没有本轮此前捕获的底部裁切。截图来自实际原生 Code 的所属 Electron renderer，不能认证 WindowServer、实体 4K、物理键鼠、系统 IME 或全部语言／主题。

新的控制器确认 Code／driver 都正常 0 退出，精确 PID、所属进程组和唯一监听口为空后跳过所有信号，本轮 overall=true、源码绑定未变。旧 1e39 的 EPERM 控制器首败及未等待动画的截图保持原状，不重标成功。out 的 schema1 记录没有 Git HEAD；本轮重新比对当前全部源输入、23 个 out 对象（含记录）与脚本字节，不冒称重新编译了 TypeScript。

仍缺只读 provider 写拒绝专项、单文件父目录授权专项及 Windows/Linux 原生 EH 等完整合同证据。V7-06、P1/P2 整项保持开放，75 条总账状态不变，没有调用模型或发布 V0.0.7。
