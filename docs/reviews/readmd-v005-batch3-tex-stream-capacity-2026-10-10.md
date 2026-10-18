# 第三批：完整中文运行库的流式容量验证

2026-10-10。本增量支撑 R030/R053/R075，继续沿用核心内置、重型按需安装的边界。没有添加新的前端面板，没有发布未审查的 TeX 包，也没有修改用户安装。第三批完整数学、出版及科研图表范围保持不变。

## 实现

完整中文准备输入有 34,822 个文件，原流式格式的 30,000 文件限制不足。Rust 提取器和维护装包工具统一采用 40,000 文件预算；8 MiB 清单、2 GiB 下载、4 GiB 展开、256 MiB 单文件及 64 KiB 工作缓冲的限制不变。每一个实际包仍须与应用锁完全一致，增加总数预算不会允许清单之外的内容。

大清单采用紧凑 JSON 保存，避免相同数据因格式化缩进越过清单文件大小限制。首遍摘要／压缩长度准备最多四个有界管线并行，第二遍仍按清单顺序写入并再次检查摘要和长度；任何首遍失败等待已开始的工作结束后才报错。保持旧输出不可覆盖，不执行包内脚本、不搜索系统引擎、不联网。

测试材料是固定维护输入的完整运行树，所有路径、尺寸和摘要与该输入逐项比较。其 ID 为 `tex-review-fixture`，分发审查状态为 false，不属于软件允许安装的四个正式能力 ID，不能通过实验包绕过应用目录。

## 验证状态

| 项目 | 结果 |
| --- | --- |
| 最终完整 Rust 内核 | 2073 通过、0 失败、3 项既有忽略，260.94 秒 |
| 维护工具回归 | 18 通过、0 失败／跳过；含文件与清单两类边界、有序并发准备、失败等待和输出不可覆盖 |
| 静态检查 | 资源、接线、安装器体积、无运行时 Python 及 Git 差异检查通过 |
| 完整归档及原生容量流程 | 4 条通过：500 文件后取消及清理、完整文件摘要与长度、暂存库实际中文书籍输出、完成清理及旧资源保留 |
| 独立 PDF 检查 | 3 页、9 个实际嵌入字体；精确中文、映射前后逐页像素一致；人工查看联系表 |

完整归档 373,309,453 字节，展开 736,565,687 字节；Windows 实际提取 144.874 秒，包含取消尝试、编译和清理的全程 209.262 秒。单独 QA 父进程峰值工作集 145,170,432 字节，不包含外部 TeX 子进程，不能作为普通阅读内存或整棵进程树指标。取消尝试的 11.140 秒包含读清单和提取前 500 文件，不是取消反馈延迟。

证据索引见 [manifest](../evidence/capability-upgrade/2026-10-10/batch3-tex-stream-capacity-manifest.json)，实际检查见 [native](../evidence/capability-upgrade/2026-10-10/batch3-tex-stream-capacity-native.json) 和 [独立检查](../evidence/capability-upgrade/2026-10-10/batch3-tex-stream-capacity-inspection.json)。正式安装／修复／卸载和 UI 安装继续仍未认证；此次没有把实验归档加入正式目录。

先前串行准备因小文件处理过慢，在首遍读取阶段主动停止；当时没有输出归档、没有安装资源。其源码摘要、读取量和未完成清单单独保留，不计入最终通过。后续使用独立目录，不覆盖该尝试。

## 剩余交付与复现

正式 TeX 下载目录、上下文安装／继续、真实安装修复卸载、匹配二进制源码和缺失历史宏包材料、逐组件许可证、任意模板及全平台原生仍需完成。此处的流式暂存不替代这些验收。

在已有锁定中文准备输入的开发环境中，离线执行：

```powershell
node ui-tests/prepare-tex-stream-fixture.mjs --output-name stream-review-fixture-concurrent
cargo build --manifest-path rust/Cargo.toml -p readmd-kernel --example capability_tex_archive_probe --offline --locked --target-dir .cache/community-upgrade/build
node tools/stage-windows-runtime.mjs .cache/community-upgrade/build/debug/examples
```

随后用该例程指定生成的清单、归档、`publication-runtime.json` 及集中证据父目录。输出目录独占创建；复现新尝试时选择新的 `--output-name`，不覆盖旧资产。普通构建、安装器和应用不会调用该准备脚本。

## 缓存分类

- `tex-inputs/stream-review-fixture/`：未完成串行尝试的清单，仅保留审计，不作为可安装包。
- `tex-inputs/stream-review-fixture-concurrent/`：可复用的离线容量归档、紧凑清单和准备报告，分发状态仍为未批准。
- `tex-stream-native-*`：本次专用合成文档、输出、独立检查及报告。提取暂存由任务自己的 `TempDir` 管理，不向用户目录写资源。
- `tex-large-pack-*.log`：构建、全量测试、工具、静态及原生证据；共享离线构建缓存和此前证据保留。
