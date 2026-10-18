# 第三批增量：创建期工作进程归属与退出回收

日期：2026-10-11。B3-52，关联 R003/R004/R029/R030/R075。状态：源码实现，尚未类型检查、编译或执行；第三批未完成。

## 修复的生命周期

Typst、受管理 OCR 及 TeX 复用 `worker_sandbox` 的 Windows 启动核心。创建时使用 `PROC_THREAD_ATTRIBUTE_JOB_LIST` 绑定任务组，保留挂起启动及显式恢复；不再在创建之后调用 `AssignProcessToJobObject`。因此代码中的进程创建与任务组归属不再是两个独立步骤。任务组不命名、不继承，设置关闭时结束所属进程，标准输入、输出、错误三个句柄使用明确的继承列表。

Microsoft 说明该创建属性从 Windows 10 起支持；任务组提供进程组生命周期控制。实现依据见 [UpdateProcThreadAttribute](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute) 与 [Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects)。这说明所选机制，不等于当前代码已获得 Windows 实测证据。

TeX 保留原 AppContainer 的资源权限及网络隔离；通用 `ManagedWorker` 仅提供进程归属和生命周期，不宣称 Typst/OCR 获得同等文件/网络沙箱。TeX 保留原 512 MiB 单进程/1 GiB 任务组限制；通用工作进程为 1 GiB/2 GiB，两者均最多八个所属进程。大材料是否可正常完成仍须压力验证，不能由配置数值认证。

Typst/OCR 的 Windows 环境只带必要系统启动变量和明确的引擎参数；PATH 指向指定程序目录，临时与用户缓存目录指向本次任务目录，不继承宿主任意环境。程序须使用绝对路径，不从系统 PATH 查找。非 Windows 沿用原子进程执行，并增加作用域结束回收；父进程崩溃和完整进程树保证仍待平台原生实现及验证。

三类引擎在接受正常结果之前确认任务组已停止，再读取日志/正文。停止失败返回错误。显式取消、作用域清理共用第一次停止开始的两秒期限，不在重复清理时重置等待；真实反馈时间和停止时间尚未测量。Windows 不再按全机进程表/PID 扫描来结束 Typst，其他任务不属于本次任务组。

OCR 已校验运行库后的进程启动失败返回 `ocr_failed`；探测结果不满足实际识别要求返回 `ocr_probe_failed`。这些失败不会伪装成缺少能力后改走另一引擎。未安装能力时原有回退/安装链路保留；完整真实 API 流程待验。

## 准备的真实进程回归

新增三个 Windows 用例：受管理环境与目录、拒绝 PATH 搜索式启动、真实拥有者/引擎/后代进程生命周期。后者使用测试二进制的实际子进程，不替换 Win32 API；通过预先打开的真实进程句柄观察退出，避免 PID 重用误判。

生命周期用例准备验证：强制结束一个拥有者会结束其引擎和后代，另一拥有者仍存活；直接丢弃工作进程会结束所属树；主进程成功退出后仍遗留的后代必须被收回；stdout/stderr 不串流；宿主的合成环境标记不传入引擎。启动、等待和失败有界，清理由作用域结束和任务组负责。另增加 OCR 启动失败不会被误报为未安装的用例。

这些是进程机制的回归，尚未执行，也不能代替真实 Typst/OCR/TeX 文件、桌面/MCP 传输、创建瞬间故障压力或全部场景。清理结束后须先完整编译、运行库回归，随后重验正式 Typst/OCR 输出、TeX QA 原生示例及 B3-51。

## 证据与后续

用户清理 `.cache` 的限制继续生效；没有 Cargo、前端打包、准备运行库、替换安装或发布。本次[静态记录](../evidence/capability-upgrade/2026-10-11/batch3-worker-lifecycle-source-static.json)只记录源码摘要、语法及只读检查；旧记录原样保留，不将先前执行结果移记到当前代码。

旧客户端隔离、崩溃暂存/上传/事务回滚、AppContainer 配置与临时映射的异常退出回收、冷态性能、TeX 来源/许可证/正式包、完整数学与出版、原生互操作及平台证据均继续待办。Windows 新机制也尚未编译/实测，不能将 B3-52 标为完成。
