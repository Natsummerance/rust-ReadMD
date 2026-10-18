# 第三批：便携中文 TeX 的实际出版验证

2026-10-10。本增量继续 R030/R053/R075，不代表第三批或便携 TeX 正式交付完成。正式导出仍需迁移到受管理能力包；没有发布新版本、更新用户安装或把未审查运行库加入下载目录。

## 实现

- 固定 TinyTeX v2026.10 输入及匹配的 TeX Live 包数据库，锁定 CTeX 的完整声明依赖闭包。新增 43 个运行包，压缩输入 85,932,952 字节；离线组合后 34,822 文件、736,565,687 逻辑字节。基础不可变文件使用硬链接，该数字不是新增磁盘占用。
- 中文输出显式采用 Fandol，避免 CTeX 自动绑定 Windows SimSun/macOS 系统字体。pdfLaTeX 默认采用随运行库提供的 Latin Modern 轮廓字体，避免依赖缺失的 EC 位图字体生成程序。
- 报告与书籍的一级标题生成章，二级标题生成节；文章仍生成节，修复报告／书籍从 0.1 开始的问题。
- 对标准 Adobe CID 字体缺失的 ToUnicode 映射，用运行库中匹配的 CMap 补全。保留已有映射，共享相同集合的映射流，限定读取路径、体积及期限。独立渲染验证补全前后像素逐页一致。
- 修复工作目录统计与 TeX 临时文件重命名之间的竞争；仅忽略已消失的子文件，根目录丢失、权限错误和其他故障仍报错。统计同时受条目数、空间、取消及期限约束。
- 完善 `!pdfTeX error:` 诊断。不引入应用 Python 依赖；Python 仅用于独立 QA 和维护时的严格 XZ/tar 输入检查，并有逐文件允许清单。

## 最终源码验证

所有以下证据均取自最终源码重新构建后的运行，完整索引见 [manifest](../evidence/capability-upgrade/2026-10-10/batch3-tex-publication-manifest.json)。较早失败尝试单独保留于 [attempts](../evidence/capability-upgrade/2026-10-10/batch3-tex-publication-attempts.json)，不计入通过。

| 检查 | 结果及边界 |
| --- | --- |
| 完整 Rust 内核库 | 2072 通过、0 失败、3 项既有忽略，241.11 秒；不是八小时压力认证 |
| Windows 真实 Markdown → TeX → 隔离编译 | 5 条通过：article、ctexart、ctexrep、ctexbook 和 Latin pdfLaTeX；PATH 仅 System32，自带必要 DLL，无系统 TeX |
| 独立 PDF 检查 | 5 份、16 页；精确 Unicode 文本、实际嵌入字体、矢量对象、映射补全前后逐页像素一致；人工查看全页联系表和文章首图 |
| 最终程序隔离回归 | 10 条通过，含两个引擎、引用、错误位置、缺字、目录外读取拒绝、取消及并发限制；不是所有平台／所有攻击方式认证 |
| 维护工具 | Node 15、Python 5 项通过，无失败；Node 无跳过 |
| 静态检查 | 资源、接线、安装器空间及无运行时 Python 检查通过；本增量无前端修改，无需重打 boot |

生成材料是内容丰富的原创合成研究说明、三线表、公式和多页章节，非用户私人文档。联系表仅用于 QA，不冒充 4K 宣传素材或完整论文预设验证。

先前真实验证曾发现中文提取缺字、文件统计竞争和 Latin 字体缺失，修复后重新检查。一次维护调度同时链接及运行同一 EXE，触发 Windows 文件锁；已调整为先构建、后运行，保留该失败记录。

## 仍未完成

1. 正式 UI、MCP／VS Code 的 TeX 编译服务仍有旧系统路径，需要接入已安装能力、运行租约、上下文安装及继续原操作。
2. 本增量初次验证时的流式包预算为 30,000；实际输入有 34,822 文件。后续已调整到 40,000 并完成完整归档的真实流式暂存、取消和中文输出，详见[容量记录](readmd-v005-batch3-tex-stream-capacity-2026-10-10.md)；正式安装／修复／卸载仍待完成，没有删掉运行文件迁就旧上限。
3. 533 个匹配的运行／源码／文档容器中 529 个校验到位；`pdfmanagement`、`tagpdf` 的固定版本 source/doc 四容器暂不可得。另有四个历史文档记录尚无匹配材料。二进制对应源码及逐组件许可证审查也未完成，`redistributionApproved` 保持 false。
4. 任意模板导入、完整出版参数、全平台原生、全部故障和长期压力继续保留。此处不会把准备输入、合法 XML 或可运行示例当作正式功能验收。

维护输入与离线命令见 [离线能力指南](../guides/offline-capabilities.md)。CTeX/xeCJK 的版本与声明来源为 [CTAN CTeX](https://ctan.org/pkg/ctex)、[CTAN xeCJK](https://ctan.org/pkg/xecjk)；分发必须按 [TeX Live 逐组件许可](https://www.tug.org/texlive/LICENSE.TL)审查。标准 CID 不附 ToUnicode 的行为核对了 [TeX Live 的 type0.c](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/dvipdfm-x/type0.c)，该源码阅读不等于已取得当前二进制的完整对应源码。

## 本任务缓存分类

- `tex-inputs/`：固定原始归档、包数据库、529 个核验容器、硬链接组合树、运行清单和少量源码/许可检查材料，供后续交付审查复用。
- `portable-tex-publication-*`、`portable-tex-native-*`：真实隔离输出、PDF、提取文本和独立视觉 QA；最终验收使用证据索引绑定的两组运行。
- `tex-publication-*.log`：构建、全量测试、原始失败及最终运行日志。历史尝试已分类，不伪装为通过。
- `.cache/community-upgrade/build/`：现有共享离线 Rust 构建缓存。本轮未删除共享缓存或旧证据，没有更改用户安装、文档目录及系统软件。
