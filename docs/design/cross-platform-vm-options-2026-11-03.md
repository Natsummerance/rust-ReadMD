# ReadMD 跨平台原生验收环境调研

日期：2026-11-03。用户要求本轮只调研，**不安装、不下载系统映像、不创建或启动虚拟机**。宣传与个人 Android 手机不用于软件故障测试。本机检测为 Apple Silicon arm64、macOS 27.2、256GiB 内存；总内存不代表当前可用内存。后续若获授权，一次只开一台测试机，并继续保留至少 10GiB 可用内存，不停止用户 LLM。

## 候选与适用边界

| 方案 | 可补的证据 | 成本与限制 | 本项目判断 |
| --- | --- | --- | --- |
| UTM | ARM Windows/Linux 的原生桌面；也可用 QEMU 模拟 x86_64 客体 | 官网提供免费开源版；ARM 虚拟化与较慢的 Intel 模拟是两种执行方式。Windows 许可证另计。 | 后续优先试用的免费入口；测试盘、配置和导出日志可集中到获授权的开发目录。先用干净快照与专用中文材料，不挂载私人主目录。[官方介绍](https://getutm.app/)、[Windows 指南](https://docs.getutm.app/guides/windows/) |
| VMware Fusion | Apple Silicon 上的 ARM Windows/Linux 桌面与系统文件选择器 | Fusion 已对个人、教育及商业用户免费；Apple Silicon 客体必须为 arm64/aarch64，Windows ARM 内运行 x64 应用仍有转译边界。 | 可作为 UTM 的替代；安装前重新核对当前 Fusion 对本机 macOS 的支持矩阵，不能把旧兼容表外推到 27.2。[官方免费政策](https://blogs.vmware.com/cloud-foundation/2024/11/11/vmware-fusion-and-workstation-are-now-free-for-all-users/)、[客体架构说明](https://knowledge.broadcom.com/external/article/315602/compatibility-considerations-for-arm-guest.html) |
| Parallels Desktop | Windows ARM 桌面整合、办公软件往返 | 商业产品。其 Intel 客体模拟目前仍是受限预览，官方列出速度、单 vCPU、最多 8GB、无声音等限制，不能作为完整兼容证明。 | 暂不购买。以后确有 Office/原生交互需要时再比较；本轮没有授权付费。[产品资源](https://www.parallels.com/products/desktop/resources/)、[Intel 模拟限制](https://kb.parallels.com/en/130217) |
| Windows/Linux x64 CI 或已有 x64 实机 | 实际 x64 编译、Rust 文件/持久化/故障测试；CI 浏览器连接实际服务 | CI 浏览器不证明原生窗口、系统关联、选择器与输入法。ARM 客体执行 x64 程序也不等同 x64 实机。 | 本次 P0 首选：重建实际发行目标，运行同一生命周期测试，逐项记录证据类型；将 UI 宿主缺项留在后续原生矩阵。 |

Apple 的 Linux VM Rosetta 支持的是 ARM Linux 客体中的 Intel 应用翻译，并不把整个客体变成 Intel Linux，也不能据此认证 x64 内核及驱动行为。[Apple 文档](https://developer.apple.com/documentation/virtualization/running-intel-binaries-in-linux-vms)

## 后续获授权后的最小方案

建议先建一台 UTM Windows 11 ARM 测试机：4 vCPU、8GiB 内存、80GiB 稀疏磁盘；如需 Linux，再单独建立 4 vCPU、4GiB、40GiB 的 ARM Linux。以上是拟定预算，未分配资源、未经性能实测。Windows 仍须使用有效许可，免费虚拟化工具不附带 Windows 授权。[UTM 官方许可提示](https://docs.getutm.app/guides/windows/)

每台保留一个已更新的干净快照和一个本次候选快照。先验证真实安装包→首次启动→中文路径打开→编辑/保存→取消/丢弃→关闭标签/窗口→正常退出→新进程恢复；再补默认关联、原生另存、编码、IME、外改/占位与空间不足。审阅、草稿、会话分别核对持久结果；测试中断后不把残留锁或恢复状态清掉冒充通过。

Windows x64 发布包先在 ARM Windows 的应用转译层测试时必须标明该条件；最终仍建议补真实 Windows x64。Linux x86_64 发布包先用 CI 的真实 x64 测试，再决定是否值得在本机模拟完整 x64 桌面。国产 Linux 与鸿蒙的依赖、宿主和包型分别列账，不把通用 Linux 或源码脚手架当成它们的原生通过记录。

本轮执行结果：只读取硬件信息和官方资料；未安装、下载或运行任何虚拟机，也未接触个人手机、模型或账号。
