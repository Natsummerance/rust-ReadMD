# D04/D05：固定源码对应、许可材料与统一闭包准备（2026-10-19）

**D04、D05 均未完成，`redistributionApproved` 保持 false。** 本轮新增三个组件共 **88 个运行文件的逐字节源码对应证据**，完整复验现有材料摘要，并给出独立的统一新闭包规划工具。8 个旧锁定容器仍缺失，共 **11,460,408 字节**。源码对应证明没有替代这些容器、旧编译文档或原生二进制构建源码。

本轮证据位于 [tex-materials](../../.cache/capability-upgrade/longtail-20261019/tex-materials/)，入口为 [summary.json](../../.cache/capability-upgrade/longtail-20261019/tex-materials/summary.json)。全程使用文件、官方网络端点和无界面命令；现有 TeX 运行目录、两个锁、四个 partial 及产品源码保持原样。没有下载大引擎、安装依赖或生成正式可分发包。

## 本轮完成

| 组件 | 原锁修订 | 固定 upstream 提交 | 现有运行文件逐字节一致 |
| --- | --- | --- | ---: |
| pdfmanagement | 80357 | `7f080362977671bab90df36e21ea86f9fff4c5ef` | 29/29 |
| tagpdf | 80360 | `3b0bd42134023279685ec2b7c0c24f9b5cd00a19` | 22/22 |
| latexindent | 79306 | `748f0f68397793b4646fa48762b0041b889cfcb4`（V4.0.2） | 37/37 |

见 [最终对应清单](../../.cache/capability-upgrade/longtail-20261019/tex-materials/16-final-runtime-comparison/comparison.json)。所有下载文件均绑定明确 commit → tree → blob，核对 Git blob SHA-1（含长度前缀）、字节数及本地 SHA-256。每个组件只使用同一固定提交；源文件和实际 docstrip 输入也逐字节核对。

- pdfmanagement：固定提交的 22 个 `.dtx`／`.ins` 文件，使用已锁 TinyTeX 的 `pdftex.exe -interaction=nonstopmode -halt-on-error -no-shell-escape pdfmanagement.ins` 离线生成。25 个生成文件与原运行文件一致；另外 4 个辅助文件直接对应同一提交的 upstream 文件。
- tagpdf：复用 2026-10-13 已验证的 13 个源码文件，以同一离线命令运行 `tagpdf.ins`。15 个生成文件一致；另外 7 个 HTML、文档辅助和 verbatim 文件绑定同一提交后逐字节一致。新增 26 个该提交的文档源／图片／辅助文件，未把它们称为原编译 PDF 文档已取得。
- latexindent：37 个脚本／模块／YAML 文件均对应 V4.0.2 的同一固定提交；另取得完整 GPLv3 文本、README、变更记录、构建说明及主文档 TeX 源。Windows 打包 EXE 与其内嵌 Perl 依赖的构建源码没有由脚本对应证明推定通过。

[license-materials.json](../../.cache/capability-upgrade/longtail-20261019/tex-materials/license-materials.json) 包含完整 LPPL 1.3c、GPLv3 文本的文件摘要、固定提交的通知材料，以及六个目标组件每个现存运行文件中的许可／版权声明位置。`texlive-scripts`、`texlive.infra` 的包级许可仍为 null；个别脚本明确声明 GPL2-or-later，不足以替整包的所有配置、数据及辅助文件归类。363 个组件中有 48 个 catalogue 许可未声明项，逐组件审查仍开放。

## 真实验证与保留的失败

1. [首次离线准备日志](../../.cache/capability-upgrade/longtail-20261019/tex-materials/01-offline-preparation-first.log)：退出 1，既有正式锁 `529/533` 精确容器通过，pdfmanagement／tagpdf 的四个 source/doc 容器缺失。
2. [最终完整盘点](../../.cache/capability-upgrade/longtail-20261019/tex-materials/15-final-closure-inventory/inventory.json)：363 组件（320 base + 43 addition），连同原锁的四项 `unresolvedMaterials` 共 537 容器条目；529 精确、8 缺失、0 错配。四个 partial 的字节数和摘要均与历史记录一致。六个目标组件的 169 个声明 run 文件中，168 个存在并与 TinyTeX 冻结清单一致；`install-tl` 原本就未包含在该 TinyTeX ZIP 清单中，已明确记录这一裁剪。
3. 原 TinyTeX ZIP 长度及 SHA-256、已锁包数据库、198 个 EXE/DLL 均重新复验。对应 [binary-source-matrix.json](../../.cache/capability-upgrade/longtail-20261019/tex-materials/binary-source-matrix.json) 给出 28 个包归属组及逐文件摘要；198 个文件的完整构建源码对应状态全部保留为 false。
4. [有界官方来源搜索](../../.cache/capability-upgrade/longtail-20261019/tex-materials/04-discovery/receipt.json)：Illinois 与 Utah 两个官方镜像的 16 个旧 revision URL 均返回 404；texlive.info 的一次历史归档请求连接失败。没有据此断言旧材料永久消失，也没有继续轮询相同镜像。三个网络阶段共 155 请求、7,538,427 接收字节，主要用于固定源码和 Git 身份证据；没有大引擎传输。
5. 首次 pdfmanagement 候选 `55d1491287f36c72ceee85810ddf739e003973be` 的 `pdfmanagement-firstaid.sty` 与原运行文件存在 luacolor 补丁差异。[原始比较](../../.cache/capability-upgrade/longtail-20261019/tex-materials/08-pdfmanagement-docstrip-first/comparison-first.json) 和 [原始 diff](../../.cache/capability-upgrade/longtail-20261019/tex-materials/08-pdfmanagement-docstrip-first/firstaid-difference.log) 保留。最终采用其完整父提交后重新离线生成，29/29 精确；没有改断言、改运行文件或拼接两个 commit 的源文件。
6. 新盘点器首次将已安装包数据库中的 `install-tl` 误认为必然存在而退出 1；[首次日志](../../.cache/capability-upgrade/longtail-20261019/tex-materials/06-closure-inventory-first.log) 保留。修正为如实记录被原 TinyTeX 裁剪的文件，再完成最终盘点，没有把缺失文件造作已校验。
7. [最终工具测试](../../.cache/capability-upgrade/longtail-20261019/tex-materials/17-final-tool-tests.log)：11 通过、0 失败。包括旧审计四项反例、Git 路径／长度／blob／摘要篡改拒绝、换行一致与逐字节一致分开记录，以及新闭包不得借旧 installed 包短路选择／填补缺依赖。

原锁引用的 `20,738,679` 字节上游完整元数据在预期位置缺失；[缓存路径搜索](../../.cache/capability-upgrade/longtail-20261019/tex-materials/19-local-metadata-search.json) 枚举 21 个 tlpdb 路径，没有同锁尺寸候选。已锁 TinyTeX 内的已安装数据库已通过原 ZIP 清单核对；没有用它冒充缺少的完整上游数据库。

复核命令（离线，`--compare-runtime` 的输出目录必须全新）：

```powershell
$env:NODE_OPTIONS='--preserve-symlinks --preserve-symlinks-main'
node tools/test/tex-material-closure-evidence.mjs --recheck .cache/capability-upgrade/longtail-20261019/tex-materials/15-final-closure-inventory/inventory.json
node tools/test/tex-material-closure-evidence.mjs --compare-runtime .cache/capability-upgrade/longtail-20261019/tex-materials/runtime-comparison-plan.json .cache/capability-upgrade/longtail-20261019/tex-materials/replay-new-directory
node --test tools/test/tex-corresponding-materials-audit.test.mjs tools/test/tex-material-closure-evidence.test.mjs tools/test/tex-material-proposal.test.mjs
```

## 仍缺内容

| 原锁材料 | 修订 | 字节数 |
| --- | --- | ---: |
| pdfmanagement.source.tar.xz | 80357 | 157,264 |
| pdfmanagement.doc.tar.xz | 80357 | 5,440,664 |
| tagpdf.source.tar.xz | 80360 | 81,388 |
| tagpdf.doc.tar.xz | 80360 | 3,329,344 |
| latexindent.doc.tar.xz | 79306 | 1,286,700 |
| texlive-scripts.doc.tar.xz | 80452 | 471,868 |
| texlive.infra.doc.tar.xz | 79982 | 210,524 |
| xifthen.doc.tar.xz | 77682 | 482,656 |

每项完整 SHA-512 见 `summary.json` 的 `missingLockedMaterials`。已取得的 Git 源码和许可文本没有改写这些摘要。旧编译文档、每个二进制的匹配源码／Windows 构建补丁／工具链／第三方通知、逐组件许可审查，以及 D05 正式包安装、修复、升级、回退、取消与空间峰值验收仍需完成。

TeX Live 官方将包级 `.dtx` 材料与编译二进制的源码分开提供，Windows 构建还有独立说明，见 [官方构建手册](https://www.tug.org/texlive/doc/tlbuild.html)、[构建来源说明](https://www.tug.org/texlive/build.html) 和 [Source 目录](https://ctan.org/tex-archive/systems/texlive/Source)。本轮没有把年度源码包、其他年份 Windows 源码包或相同版本字符串当作这 198 个二进制的对应证据。

## 下一步：完整统一新闭包

[new-closure-plan.json](../../.cache/capability-upgrade/longtail-20261019/tex-materials/new-closure-plan.json) 给出材料量及执行顺序。新工具 [plan-tex-material-closure.mjs](../../tools/plan-tex-material-closure.mjs) 只生成离线审查提案；需要明确传入新取得的完整元数据、已审 SHA-256、官方 archive URL 和全新输出目录。本轮没有选择新快照，也没有运行该工具生成实际新版锁。

```powershell
node tools/plan-tex-material-closure.mjs --help
# 待完整元数据取得并审定摘要后，显式替换下面三个输入再执行：
node tools/plan-tex-material-closure.mjs --metadata .cache/capability-upgrade/tex-new-snapshot/texlive.tlpdb --sha256 REVIEWED_64_HEX_SHA256 --repository https://ctan.math.illinois.edu/systems/texlive/tlnet/archive/ --out .cache/capability-upgrade/tex-new-proposal
```

规划器从同一新数据库重新选择全部原 base 包并闭合 CTeX 依赖，输出完整 run/source/doc 获取表和旧新 revision／摘要差异。旧 installed 数据仅用于比较，不会使某个 base 包沿用旧修订。容器获取仍须逐项核对长度、SHA-512 和展开文件集合，复用缓存也只认同一摘要；最终运行目录须从这个完整新集合单独组成。

以当前版本作为量级参照：363 run、187 source、307 doc，共 **857 个容器、588,521,172 字节**；再加历史完整元数据尺寸约为 **609,259,851 字节**。这不是新快照的实际下载承诺，也未计 28 个二进制归属组尚待确定的构建源码／第三方材料；取得完整新数据库后应重新精确计算。只有匹配源码、逐项许可与通知、展开／暂存／回滚／同卷峰值和正式离线生命周期全部通过并经独立审查，才可提出新的正式包审批。
