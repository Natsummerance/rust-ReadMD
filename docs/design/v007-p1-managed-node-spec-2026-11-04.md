# V7-05 受管 JavaScript 运行环境

本切片复用 managed-v2 能力包、逐文件摘要、安装意图、共享租约、任务目录及既有代码停止服务。限定 Node.js 单块运行；其他语言、未知副作用检查点和整项 V7-05 仍另行验收。不改变当前 V0.0.6 下载或日常配置，不调用模型。

## 固定来源与范围

采用 [Node.js 24.22.2 LTS](https://nodejs.org/en/blog/release/v24.22.2) 官方六个归档，Mac／Windows／Linux 的 ARM64 和 x64 分别锁定归档及实际文件摘要。运行包仅包含原始 Node 可执行文件、官方完整 LICENSE（含第三方声明）及 ReadMD 说明，不含 npm、全局模块或安装脚本。维护者显式准备输入，正常 Rust 构建继续 locked/offline，运行时不下载。

[该版本平台要求](https://github.com/nodejs/node/blob/v24.22.2/BUILDING.md#platform-list)为 Mac 13.5+、Windows 10+、Linux glibc 2.28+；Linux 还须满足官方 libstdc++ 要求。实际最小运行失败就保持未就绪；不能用跨编译、存在可执行文件或 PATH 探测替代。本机只执行当前架构；其他架构的字节准备与原生执行分别记录。

## 输入与独立预期

1. 专用空 profile 即使 PATH 已有正常 Node，HTTP／MCP 的 JavaScript 仍返回 `capability_pack_unavailable`，不创建用户脚本，不启动宿主 Node；SQLite 保持可用。
2. 通过现有离线上传入口导入已锁定本平台包。安装探针执行固定的 Unicode、版本、算术及权限检查，任何版本／输出／执行权限不符不提交。非本平台包、摘要损坏和符号链接拒绝，原版本和未知对象保留。
3. 安装后执行实际 `console.log(6*7)`、Unicode 和长定时器。只从租约固定的绝对路径启动，不回落 PATH；返回运行资源 ID／版本／锁摘要。已有环境白名单、资源上限、取消和超时继续生效，不继承模型密钥或用户 NODE_OPTIONS。
4. 运行期间修复／卸载必须因租约而拒绝。取消、超时、正常退出及 MCP EOF 回收所拥有的执行后释放租约，再允许卸载。失败和取消不插入正文，不自动重跑。
5. 新进程重开，安装状态与实际最小运行一致；卸载只移除锁中对象，保留未知文件，下一次运行明确未就绪。相同正文修订、一次撤销、BOM／换行、待审原文和关闭决定保持已有回归。
6. 安装探针使用已有受管任务目录中的副本，计入安装峰值，任务目录另外检查其真实临时卷容量。探针和运行均有界，取消时清理仅限已登记对象。内存不足不加载额外引擎；本轮维护输入与产物总预算 1.8GiB，单一 Rust target 上限 5.5GiB。

Node 的 [Permission Model](https://github.com/nodejs/node/blob/v24.22.2/doc/api/permissions.md)只是额外限制，不是对恶意代码的 OS 沙箱证明。启用执行所需的最小文件读取权限，不授予写文件、子进程、原生扩展或 worker；保留 Windows 原生 worker 保护。不能因此把 Unix 任意代码安全、未知副作用或崩溃恢复标记完成。

## 证据

先保存未安装仍调用 PATH 的真实首败，再绑定新源码、boot、二进制、包及锁摘要。三平台各运行真实安装／运行／卸载／新进程检查，同时回归原有代码取消、EOF、正文历史和关闭保护。Mac Cocoa 的后续入口单独验收；CLI 不算原生窗口。六包准备、三系统本机执行、实体 Intel／ARM Windows／ARM Linux分别列账，不扩大认证。
