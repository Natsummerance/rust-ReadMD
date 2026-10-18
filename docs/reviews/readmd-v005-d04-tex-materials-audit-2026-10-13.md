# D04：TeX 对应源码与历史小材料审计

本次完成限定输入审计，**D04 未完成**。8 个原锁定容器仍缺失，共 **11,460,408 字节**；没有将新版本、同尺寸文件或源码候选算作旧容器。`redistributionApproved` 保持 false，二进制对应源码、逐组件许可及正式分发审查继续开放。

独立证据见 [audit.json](../evidence/capability-upgrade/2026-10-13/tex-corresponding-materials-audit/audit.json) 和 [summary.json](../evidence/capability-upgrade/2026-10-13/tex-corresponding-materials-audit/summary.json)。原 [2026-10-10 输入证据](../evidence/capability-upgrade/2026-10-10/batch3-tex-publication-inputs.json)、两个锁文件及四个 partial 均保留，审计前后摘要一致。历史的 529/533 容器结论没有重新执行或扩大为完整许可认证。

| 仍缺材料 | 锁定修订 | 锁定字节数 | 本轮当前 CTAN 响应字节数 | 判定 |
| --- | --- | ---: | ---: | --- |
| pdfmanagement.source.tar.xz | 80357 | 157,264 | 157,648 | 尺寸不同 |
| pdfmanagement.doc.tar.xz | 80357 | 5,440,664 | 5,440,732 | 尺寸不同 |
| tagpdf.source.tar.xz | 80360 | 81,388 | 81,544 | 尺寸不同 |
| tagpdf.doc.tar.xz | 80360 | 3,329,344 | 3,340,532 | 尺寸不同 |
| latexindent.doc.tar.xz | 79306 | 1,286,700 | 1,287,820 | 尺寸不同 |
| texlive-scripts.doc.tar.xz | 80452 | 471,868 | 473,384 | 尺寸不同 |
| texlive.infra.doc.tar.xz | 79982 | 210,524 | 214,156 | 尺寸不同 |
| xifthen.doc.tar.xz | 77682 | 482,656 | 482,656 | SHA-512 不同 |

xifthen 实际下载的 SHA-512 为 `dec04d5e215fbcf7da0d2757821f085ad5ed7647fc86593a0fa589d09f685adfd3875dc1b15f6e58d3f96bb647b877ae7f73b8f71b53236ebac87fbad497164b`；旧锁为 `d8d05abe61ce6a84a8b58a3342382c9a27d458e7027a290b3233b7f00c39c9af886d80685cefd9483b5c764bacca465c599020a8c7ce55034dfe1a4934e4eb96`。这项同尺寸反例已作为拒绝证据保存。

四个 partial 分别还差 10,172／17,926／16,215／3,758 字节，未续拼、覆盖或重命名。2026-10-01 按日存档的版本化 URL 本轮返回 Anubis 拒绝 HTML 或连接错误；HTTP 200 没有被当作归档成功。旧版本是否仍可由其他可授权来源取得，保持未验证。

新增小材料为 tagpdf 1.0g 的 [固定 upstream commit](https://github.com/latex3/tagpdf/commit/3b0bd42134023279685ec2b7c0c24f9b5cd00a19) 源码候选：13 个 `.dtx`／`.ins` 文件，共 464,723 字节，另含 1,193 字节的 CTAN README。固定 commit 的 tree 为 `70622c3572a48c5da174b0639bdd78ac57ea4878`；全部文件经长度、Git blob SHA-1 和本地 SHA-256 核对。README 与源码声明版本 1.0g、日期 2026-09-23，README 声明 LPPL 1.3c 或更新版本。可读副本位于 [source-candidates/tagpdf-3b0bd421](../evidence/capability-upgrade/2026-10-13/tex-corresponding-materials-audit/source-candidates/tagpdf-3b0bd421/doc/CTANREADME.md)。

这些文件只作为对应源码候选：未替代 `tagpdf.source.tar.xz` 的旧 SHA-512，未取得旧编译文档，未证明全部生成运行文件相同，更不证明 TeX Live Windows 引擎、DLL、工具及构建源码对应。latexindent 的锁定许可为 GPL3+；texlive-scripts／texlive.infra 的锁定记录没有许可声明，仍需分别查证，不能套用 tagpdf 的许可。

可复用工具为 [tex-corresponding-materials-audit.mjs](../../tools/test/tex-corresponding-materials-audit.mjs)，含 `--help`。默认离线，联网必须显式 `--download`；输出目录必须全新。上限为 48 请求、20 MiB 接收、单响应 6 MiB、单请求 15 秒、整体 180 秒、并发 3。最后网络轮实际 33 请求、1,142,849 接收字节、34.828 秒。四项反例测试通过：同尺寸异摘要、截断／HTML、Git blob 身份以及伪造 JSON 的“精确容器”声明。退出码 0 仅说明审计或复核执行成功，需求状态由 JSON 单独表达。

```powershell
$env:NODE_OPTIONS='--preserve-symlinks --preserve-symlinks-main'
node tools/test/tex-corresponding-materials-audit.mjs --help
node tools/test/tex-corresponding-materials-audit.mjs --recheck docs/evidence/capability-upgrade/2026-10-13/tex-corresponding-materials-audit/audit.json
```

首轮源码候选因 tree 接口回显 commitish 而被严格身份比较拒绝，原始 JSON、响应和当时脚本已保留于 `run-01`；后续改为先读取固定 commit 指向的明确 tree，再逐文件取证，见 `run-02`。最终工具新增 `--help`，没有重写先前审计记录。没有下载引擎、修改 Rust、锁文件、全局账本、能力目录或发布标志，也没有构建或发布。

若旧材料继续不可得，应另立一个完整、固定日期的 TinyTeX／TeX Live 基础输入和包数据库，重新锁定整个 CTeX 声明闭包及匹配的 run/source/doc 容器；逐项保留旧新修订、来源、摘要、许可、通知及二进制构建源码映射。随后对新闭包测量压缩／展开／暂存／回滚峰值，重做离线安装、修复、升级、回退和真实出版验收。新闭包须单独审查并作为新的不可变包提出；当前审计不会修改审批状态或将新旧材料混为原版本已审查。
