# 第三批：函数语义、表格对齐、恢复与安装空间

日期：2026-10-20。接续 candidate-14；本轮执行用户“不使用 computer use”的约定，只用命令行、独立拥有的进程和合成文件。版本与发布状态不变，原 toolbar、编辑首绘和 4K 修复继续保留。

## 实施出口

| 项目 | 具体实现 | 实测与边界 |
| --- | --- | --- |
| M10 UnicodeMath | U+2061／funcapply 编译为现有函数结构，函数名、参数、脚标和 UTF-16 源范围准确；重复函数只修改指定一处 | 旧候选真实 stdio 留存零 func 的失败现象；新候选 parse、局部 patch、过期拒绝、序列化及重启回读。保留完整 UTN28、隐式函数识别和 Word 裁决缺口 |
| M11 MathML | table→row→cell 的列对齐级联，仅在同列有效对齐一致时写入可编辑 OMML 列属性；冲突和非法覆盖有明确诊断 | 旧候选真实 stdio 留存错误列值及无诊断；新候选验证级联、作用域、空／不规则行、间距和 DSL/OMML/MathML 往返。跨格和不可表达组合不冒充支持 |
| D03 任务退役 | 清理日志前保留经过校验的完整 WAL 硬链接见证；清理中断后的新进程可继续，未知文件和替换对象保持保护 | 真正终止独立测试子进程，覆盖意图写入、见证链接、锁删除、原日志删除和目录删除；检查重新启动、未知哨兵和预算去重。硬链接不可用时保守失败，远程卷不据此认证 |
| D07 安装前空间检查 | 用锁定清单和实际卷分配单元估算新增展开内容、目录和日志空间，保留安全余量；读取调用者可用空间，在创建 payload 前拒绝不足。安装按 profile 持有跨进程守卫 | 真实 Windows 卷查询、估算与边界回归、守卫冲突／进程退出释放、成功安装的真实 CLI。低空间判断的注入检查不等于真实填满小卷；跨 profile 同卷硬预留、完整硬配额和回滚矩阵仍开放 |

安装包已下载的输入以及现有安装都已占用系统报告的空间；替换只进行同卷重命名，不把旧安装再重复计入新增需求。检查是安装准入，不能保证安装期间其他程序不占用磁盘，写入失败继续沿用已有事务回滚。非 Windows 的系统可用空间检查没有原生证据，不降低平台验收要求。

Windows 依据：[GetDiskFreeSpaceExW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getdiskfreespaceexw) 的调用者可用字节（含用户配额），以及 [GetDiskFreeSpaceW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getdiskfreespacew) 的实际分配单元。UTN28 与 MathML 语义采用子项固定官方来源，最终报告记录具体测试和来源。

## 合并和证据

- 三个子代理均为 gpt-6.1-sol、max，分别只写 UnicodeMath、MathML 和任务恢复模块；主代理负责能力包模块、构建与账本。
- 共用 `.cache/community-upgrade/build`，关闭增量编译，Rust 一律 `--locked --offline`；不增加依赖，不复制重型运行库。
- 本轮证据在 `.cache/capability-upgrade/batch3-20261020/`；历史 candidate-14 和首次失败保持可对照。
- 先定向回归，再同源全库与主程序构建，冻结新候选，执行真实 stdio／子进程流程。只有通过的实际范围写入 partial 证据，完整第三批、外部应用、八小时与非 Windows 原生状态保持开放。

公式面板的粘贴检测还须覆盖 U+2061／funcapply，确保新后端能力能从现有入口使用；失败保留原输入，迟到结果不能覆盖新草稿。该项已补生产处理函数行为回归，并离线重打 boot。

四条限定流程及入口已实施并取得实际结果，详见[本轮结果、原始失败及边界](../reviews/readmd-v005-batch3-semantics-recovery-2026-10-20.md)。完整第三批、外部应用和平台要求未因此整项销账。
