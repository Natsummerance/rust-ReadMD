# P1：跨平台容量前检与安装预算

这是 V7-02B 的输入与验收规格，不是通过记录。复用现有能力包锁、安装事务、恢复日志和 Mac P0 容量准入；不下载或加载模型，不修改用户正在运行的 LLM。V0.0.7 仍须 P1/P2 全部完成后发布。

## 输入及独立预期

- 同一用户、两个独立 profile、同一卷上的受管安装。第一个进程持有准入时，第二个在创建 payload 前明确繁忙；正常释放、取消和受控进程终止后可继续。普通资源读取与引擎使用仍由原有独立租约控制。
- 使用既有锁定字体包验证真实安装，输入包在小卷外，旧包、待审文档和安装暂存位于小卷内。固定检查安装、重复安装、旧包并存、取消、外部写入用尽空间、失败后重开及原文撤销。字体不涉及模型推理。
- Linux 从已经打开的目录查询 `fstatvfs`，用 `f_bavail × f_frsize`，同时核对有限 inode 预算；不把 I/O 块大小或总空闲块误作调用者可用容量。返回值不是所有文件系统用户/项目配额的完整证明。[Linux statvfs 文档](https://www.man7.org/linux/man-pages/man3/statvfs.3.html)
- Linux 目录身份同时绑定设备、inode 与实际 mount ID，保留打开的描述符；目录替换、最终符号链接、父级别名变化及同 inode 的 bind mount 替换分别拒绝。缺少必要原生查询时明确失败，不回到无查询的假成功。[Linux statx 文档](https://man7.org/linux/man-pages/man2/statx.2.html)
- Windows 保留调用者配额感知的 `GetDiskFreeSpaceExW`，复用已验证的目录 pin 与 reparse 检查，补卷身份及提交前复核。真正的小 NTFS 卷和硬配额必须与“调整数字后的单测”分列。[Microsoft 容量查询文档](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getdiskfreespaceexw)
- Mac 保留真实 `fstatfs`、分配单位与 APFS 同用户跨 profile 准入；补上原始目录别名在查询后的复核。原来的小 HFS+ 卷 ENOSPC 证据继续保留，后续新候选单独绑定。

## 实现约束

1. 预算继续计算新增 payload 的真实分配、目录、日志/清单同时存在及安全余量；输入包和旧目标已经占用 OS 空闲容量，不重复增加逻辑大小。容量读取是时点观察，外部写入后仍依赖既有事务处理 ENOSPC。
2. 合作安装准入使用同一用户主目录下固定、从不删除的空锁文件，独立于 `READMD_DATA_DIR`；Windows 跨会话也使用同一个用户目录。文件型协调沿用系统文件锁，避免线程可重入 mutex 把同线程重复安装误当成功。Mac 原生 QA 沿用已存在的强制 home 命名空间；测试的不同 profile 不得各自选一个准入锁。
3. 原生协调目录与锁保留身份见证，拒绝符号链接/reparse、不明内容及替换；失败不截断、不删除现有对象。Unix 的准入只是合作 flock，不是防任意外部写入的租约。[Microsoft 关于用户目录锁的说明](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-createmutexw)、[Linux flock 文档](https://www.man7.org/linux/man-pages/man2/flock.2.html)
4. 保留原有取消优先级、暂存归属、幂等回执及新进程恢复。先复核命名空间和卷，再进入提交；查询失败、空间不足和取消各自保留真实错误。
5. 本机只用共享 target、锁定依赖离线构建；Mac 测试 App 及容量资料均在工作区。Linux/Windows 的原生 API 与小卷测试由各自 CI 执行，不安装本机虚拟机，不把 CLI 结果当原生界面验收。

## 通过后才更新的证据

三平台分别记录源码/测试/二进制摘要、OS 查询结果、跨 profile 竞争、原生替换拒绝及取消/终止释放。Linux 额外核对 bind mount 与 inode；Windows 额外核对 NTFS 硬配额；Mac 新候选重跑受控小卷保存与安装失败。受控进程中断不等于真断电，同用户合作准入不等于抵御其它用户、其它程序或所有网络文件系统竞争。
