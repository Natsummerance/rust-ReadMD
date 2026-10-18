# V0.0.5 出版链路缓存清理记录

日期：2026-10-14。本轮 P01/P07 验证与证据归档完成后，执行既定的 60 个组件清理；全部成功，没有跳过或部分删除。

| 已删除类别 | 组件数 | 文件数 | 逻辑体积 |
| --- | ---: | ---: | ---: |
| 旧测试字体副本 | 19 | 133 | 922.3 MiB |
| 旧测试 PDFium 副本 | 19 | 684 | 246.3 MiB |
| 旧测试 Typst 副本 | 1 | 3 | 50.1 MiB |
| 旧测试 WebView 缓存 | 21 | 5,673 | 891.9 MiB |
| 合计 | 60 | 6,493 | 2,213,064,814 字节（约 2.21 GB） |

统计来自删除前的新鲜文件身份、体积和独占锁检查。NTFS 压缩、稀疏文件及回收行为会影响磁盘实际可用空间，因此此处不把逻辑字节数当作实测物理释放量。

| 保留类别 | 位置或范围 | 用途 |
| --- | --- | --- |
| 当前候选与构建 | `.cache/community-upgrade/build/` | 复核当前 a231a2… 候选及后续离线构建；本轮只使用同一 target，关闭增量编译。 |
| 锁定离线包及 QA 工具 | `.cache/capability-upgrade/locked-packs/`、`.cache/community-upgrade/qa-tools/` | 可离线重新安装字体、PDFium、Typst及执行 EPUBCheck。 |
| 当前及 HOLD 运行库、资源覆盖目录 | 固定 n2 数据目录、TeX 输入／夹具及既有保留列表 | 重现实际输出、核对资源和许可证；正式 TeX 发行包仍未验收。 |
| 文档、实际输出及诊断 | 各出版测试 profile 的 `documents/`、`outputs/` 和报告 | 保存原始失败及当前通过依据；只清理精确选中的资源子目录。 |
| 私有上游测试和字体审计 | `.cache/private/` | 保留独立期望与来源；未纳入公开归档。 |
| 不可变验收归档 | `docs/evidence/capability-upgrade/2026-10-14/publication-context/` | 本轮实际输出、检查与源码绑定，约 138 MB；不是构建缓存。 |

[完整 60 项删除记录](../evidence/capability-upgrade/2026-10-14/batch3-publication-context-cleanup.json)包含每项路径、文件数、身份检查、进程及删除结果。[主证据清单](../evidence/capability-upgrade/2026-10-14/batch3-publication-context-manifest.json)在清理前后保持同一 SHA256：`111e5dc43d18309f67fe7f966a06abdc5afbda1f07e71020bcb4fa6a94bb069f`；[独立归档复核](../evidence/capability-upgrade/2026-10-14/batch3-publication-context-root-verification.json)验证了全部 2,379 个归档文件和 524 个源码绑定。

此记录仅覆盖本轮精确清理范围，不将以前手动删除的缓存计入本次结果，也不将其余保留目录标为已清理。
