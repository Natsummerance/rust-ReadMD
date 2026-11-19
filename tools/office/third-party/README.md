# ibisOffice 源码排版 worker

ReadMD 的 Office 原版阅读入口使用固定的 ibisOffice 源码构建。DOCX/PPTX 的包读取、Word 分页、幻灯片几何、字体塑形、GPUI 绘制和矢量 PDF 输出都来自上游；ReadMD 只添加一个独立进程入口和窄参数适配。不会调用已安装的 ibisOffice 或搜索 PATH 中的 Office 程序。

固定来源为 [ibisjp/ibis-office v0.8.6](https://github.com/ibisjp/ibis-office/tree/96e218055a41261c3c12c0ea1b07f143424f368d)，提交 `96e218055a41261c3c12c0ea1b07f143424f368d`。注意这个官方标签里的 `version.txt` 仍写 `0.8.5`，crate workspace version 为 `0.8.0`；来源清单同时记录标签、提交和源内版本，不改写上游版本声明。七个 GPUI fork 使用该提交锁定的 gitlink，并额外校验归档长度与 SHA-256。

## 构建

需要现有 Rust toolchain、Node.js、Git 和 tar。支持 `macos-arm64`、`macos-x64`、`windows-x64`、`linux-x64`。Windows 使用 MSVC；Linux 构建依赖见 release workflow 的软件包列表。这里没有 Win7/32 位构建支持。

在仓库根目录执行：

```sh
# 网络只发生在 prepare。主源码、fork、Cargo cache 和身份清单均留在工作区。
node tools/prepare-office-runtime.mjs prepare macos-arm64

# 使用项目的共享 target；编译强制 --offline --locked、jobs=2、无增量。
CARGO_TARGET_DIR=/absolute/shared/cargo-target \
  node tools/prepare-office-runtime.mjs build macos-arm64 \
  --output build/office-runtime/macos-arm64
```

已有该固定提交的工作区 checkout 时可复用，避免重复下载；输入文件会按 git blob 身份和关键文件 SHA-256 校验后再复制到自有源码目录：

```sh
node tools/prepare-office-runtime.mjs prepare macos-arm64 \
  --source .cache/ibis-office-v0.8.6 \
  --archives .cache/ibis-office-import-inputs
```

`prepare-source` 只导入源码，不准备 Cargo 依赖。`clean PLATFORM` 只移除带本工具所有权标记的 `.cache/readmd-office-source`，不删除共享 target 或输入 checkout。输出目录必须不存在；重新打包时使用新的候选目录。

## 集成边界

准备后的源码在 `.cache/readmd-office-source/upstream`。构建前逐个复核完整文件清单，之后仍可离线复建。主算法文件保持原字节，变更只有：

- `ibis-office-app/Cargo.toml` 追加 `readmd-office-render` 二进制 target。
- `lib.rs` 追加 `readmd_embed` 模块声明。
- 添加上一级目录的 `source-bridge.rs`、`worker-main.rs`。

调用链为：

```text
readmd-office-render
  → readmd_embed::export_pdf
  → ibis-office-app::run_export
  → ibis_pptx::read
  → export::export
  → docx::flow_layout / slide geometry
  → GPUI PaintRecord
  → pdf_vector::PdfWriter
```

worker 只接受 `--input ABSOLUTE.docx|pptx --output ABSOLUTE.pdf`。PDF 必须与输入快照同目录、同文件名干；已有输出只接受普通空文件，保留宿主持有的文件句柄。输入原件不写回。宿主继续负责 OOXML 预检、外部关系拒绝、嵌套包检查、进程超时和结果验证。渲染入口不运行 editor/updater/settings migration；上游 IMAGE 网络 fetcher 在解析前被替换成失败结果。宿主应清除所有 `IBIS_*` 环境变量。

## 打包

输出目录直接放入候选应用的 `assets/office/runtime`。release workflow 先准备源码，随后将离线构建结果装入 `dist/release-assets/office/runtime`；现有三个系统的原生打包步骤复制这份 assets。

`runtime.json` 的运行协议 engine 为 `office-layout`，另含标签版本、源内版本、完整提交、平台、可执行文件 SHA-256 和文件清单；`source.json` 含源码及 adapter 身份、Cargo.lock、fork 和使用到的依赖通知。Mac 打包在内部 worker 签名后刷新 runtime 的字节摘要，再签名外层 app。

Carlito 和图标由上游源码内嵌。Word 艺术边框 ZIP 保留在 `Resources/artborders`，对应上游的 `bin/../Resources` 查找路径。系统字体仍取决于本机安装；Windows/Linux 和 Intel Mac 的实际排版验收须分别留证，跨编译成功不代表在目标机器完成了运行测试。

原样保留上游 LICENSE、NOTICE、COMMERCIAL-LICENSE、TRADEMARK、第三方通知以及依赖许可证。产品界面按用户提供的双方复用约定处理；构建脚本不改写原通知，也不另写许可证。
