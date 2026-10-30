# 静态 SVG 离线输出引擎

日期：2026-10-09。修复 Word/PDF 导出静态 SVG 时缺图的问题。复用图片解析器及既有输出管线，Rust 内核按需启动独立工作进程；主 Cargo 依赖没有增加。

## 来源、许可与大小

采用 [Linebender resvg v0.48.1](https://github.com/linebender/resvg/releases/tag/v0.48.1)，Apache-2.0 或 MIT。官方带 vendored 依赖的源码归档摘要：

```text
13ed5a2bae7a01156288ecae5bf944cf7d1c572742c19fc68027947a4d87294c
```

本机 Windows x64 使用离线、锁定依赖构建，工作程序 4,175,872 字节；60 个实际 normal/build 依赖的许可声明和通知保存在 `assets/engines/resvg/windows-x64/lock.json` 及相邻 `licenses/`。许可包括 MIT/Apache、BSD、Zlib、0BSD、Unlicense 及 Unicode-3.0，不引入已知强传染许可。锁文件固定源码、引擎版本、二进制摘要和体积，每次调用前校验。

```powershell
# 在已审核且包含 vendor 的源码目录，不下载依赖。
cargo build --offline --locked --release -p resvg --bin resvg
cargo metadata --offline --locked --format-version 1 > metadata.json
cargo tree --offline --locked -p resvg --edges normal,build --prefix none > dependencies.txt

# 返回 ReadMD 仓库；路径按实际构建目录替换。
node tools/build-svg-renderer.mjs SOURCE_DIR RESVG_EXE METADATA_JSON DEPENDENCIES_TXT assets/engines/resvg/windows-x64
```

Windows 解压归档时，若唯一失败为 `crates/resvg/README.md` 的相对符号链接，可复制归档根 README 到该处。它只影响文档，不能修改上游 Rust 源码或依赖。打包工具不联网，要求上游原始归档及其固定摘要可验证。

## 资源边界

- 输入至多 4 MiB，单 SVG 根、128 层、50,000 个节点；验证 UTF-8/XML、外部引用、事件与样式资源。
- 禁止 DTD、处理指令、脚本、HTML、动画、外部 href、CSS 导入及 CSS 外部 URL；允许片段和明确的内嵌栅格格式。
- 普通文本中的 `@`、反斜线和 `url(...)` 是文字，不按 CSS 误拦截。
- 隔离临时目录，不使用 shell，不允许输入目录解析依赖。15 秒超时终止工作进程并回收临时目录。
- 按输入摘要和引擎摘要缓存，最多 32 条/16 MiB；输出最长边至多 2400px、PNG 至多 24 MiB，保留透明度。

Word 和 Rust PDF 复用同一渲染参数，输出为高分辨率兼容 PNG，不能称为矢量 SVG。EPUB 保留原始 SVG；原图与引用保持不变。

## 尚未验收

仅 Windows x64 有实际构建和像素/透明度输出证据；其他平台的路径分支不能代表可用引擎。当前小型工作程序随资源目录交付，统一能力包安装、更新、卸载仍属 R003。未认证任意 SVG 的所有效果、操作系统级内存隔离、排队取消时限或八小时资源压力；资源上限不能代替这些测试。

## 2026-10-29 Mac 构建修正

Mac 刷新使用同一 v0.48.1 源码归档、Cargo.lock 和 vendored 依赖。最初采用的官方 Mac 预编译文件包含上游构建机路径：本机隐私检查通过，GitHub Mac 打包检查拒绝。保留首次失败，并改用路径重映射后的源码构建，没有放宽发布隐私检查。

在已配置 ReadMD `nativeBuildEnvironment` 的环境中，从归档源码目录运行以下命令；`CARGO_INCREMENTAL=0`，`CARGO_HOME` 和 `CARGO_TARGET_DIR` 使用工作区内的共享缓存。Intel 在 Apple Silicon 上构建时还需预先准备对应的锁定标准库。每次编译均离线，不在编译过程中获取依赖。

```sh
cargo build --offline --locked --profile production -p resvg --bin resvg --target aarch64-apple-darwin
cargo build --offline --locked --profile production -p resvg --bin resvg --target x86_64-apple-darwin
```

各目标分别生成 `cargo metadata --offline --locked --format-version 1 --filter-platform TARGET` 和 `cargo tree --offline --locked -p resvg --edges normal,build --prefix none --target TARGET`，再使用上述打包工具，最后一个参数分别为 `macos-arm64` 或 `macos-x64`。生产配置由已锁定的上游源码提供，启用 LTO、移除调试符号、关闭增量编译。主程序和工作进程均保留宿主架构检查。

Mac 锁文件记录源码归档、Cargo.lock、实际工作程序摘要及 61 个依赖的许可通知。打包前额外检查常见托管构建机根目录，防止更换开发机器后才发现预编译产物中的路径。此修正不构成物理 Intel、旧版 macOS、所有 SVG 效果或资源压力的验收。Mac 原生与安装副本证据另见 V0.0.5 刷新记录；本节前的 Windows 状态保留为当时记录。

## 2026-10-30 Linux 构建闭环

本轮 Linux 回归发现资源目录没有该平台的工作程序，不能把已有路径分支当成可用能力。官方 Linux x64 预编译包虽通过上游摘要校验，仍含构建机路径，因此未纳入产品，也未放宽隐私扫描。

`tools/prepare-linux-svg-worker.mjs prepare` 先校验同一份上游源码归档，审核归档成员，显式准备锁定依赖；`build` 使用同一个共享 target、路径重映射与上游 production 配置执行 `--locked --offline` 构建。打包在 Linux x64 宿主核对 ELF 架构及真实版本，生成二进制锁和实际 normal/build 依赖的许可通知。唯一已记录的相对 README 符号链接不提取，改为复制归档根 README，不改 Rust 源码或 Cargo.lock。

质量与 Linux 安装包构建工作流均使用这个入口，随后清理带所有权标记的源码准备目录。原生 Linux 输出测试与打包结果须以该次 Actions 实际结果为准；Mac 上审核归档和代码不能代替 Linux 验收。ARM Linux 工作程序仍未交付。
