<p align="center">
 <b>Languages</b>:
 <b>简体中文</b> |
 <a href="README.zh-TW.md">繁體中文</a> |
 <a href="README.en.md">English</a> |
 <a href="README.ja.md">日本語</a>
</p>

<div align="center">
 <img src="assets/icon-256.png" width="88" alt="ReadMD logo">

 # ReadMD (Pure Rust Edition)

 **Rust 原生内核的跨平台 Markdown 阅读器与编辑器**

 长文档语义分页 · 非破坏显示修复 · Office/PDF 原版阅读与提取 · 表格编辑与出版 · AI 审阅 · VS Code / MCP

 [官网](https://rust.readmd.asia) ｜ [V0.0.7 发布说明](release/release_notes.md) ｜ [赞助支持](https://ifdian.net/a/natsummerance)

 [![platform](https://img.shields.io/badge/Windows%2010%2F11%20%7C%20macOS%2013%2B%20%7C%20Linux-blue)](#下载与安装)
 [![i18n](https://img.shields.io/badge/languages-46-orange)](docs/i18n-language-reference.md)
 [![license](https://img.shields.io/badge/license-MIT-green)](LICENSE)
 [![release](https://img.shields.io/github/v/release/Natsummerance/rust-ReadMD)](https://github.com/Natsummerance/rust-ReadMD/releases/latest)
</div>

ReadMD 把阅读、编辑、格式转换与出版放在同一个本地工作流中。Rust 内核管理正文和保存，桌面界面使用系统 WebView，运行应用无需安装 Python 或 Microsoft Office。较大的排版、OCR、字体与媒体能力按平台通过受管理能力包提供，支持离线导入。

## 30 秒判断它适不适合你

| 你的情况 | ReadMD 的做法 |
| --- | --- |
| 文件超过约 8,000 行或 500 KB | 自动启用语义分页，目录与全文搜索跨页联动 |
| 表格缺分隔线或公式未闭合 | 在内存中修复显示；只有明确编辑保存才改写源文件 |
| 需要先看 PDF、DOCX 或 PPTX | 先阅读页面，再按需提取 Markdown；办公文档原排版视图为只读 |
| 需要整理复杂表格 | 合并／拆分单元格、调整行列尺寸、复制粘贴合并区域，并撤销修改 |
| 写论文、书籍或技术资料 | 使用引用、公式、定理块和出版预设，导出 PDF、DOCX、EPUB、HTML、TeX、PPTX |
| 希望 AI 帮忙修改 | 显式选择连接和模型，逐块审阅修改，保留草稿与恢复记录 |
| 在编辑器或 AI 客户端中工作 | 使用配套 VS Code 扩展和 Rust stdio MCP 入口 |

## V0.0.7 更新

- **先阅读，再提取**：PDF 默认进入原版页面阅读；DOCX／PPTX 增加只读原排版阅读。提取时报告格式损失，保护源文件和已有同名 Markdown。
- **表格与出版**：增加矩形合并、拆分、单元格属性、行列尺寸和合并区域粘贴；PDF／DOCX／PPTX 输出保留各自支持的表格布局。
- **保存与恢复**：改进连续关闭标签、编码确认、图片与历史跨目录另存、待审修改撤回、批量转换回执及媒体任务续作。
- **共享审阅与绘图**：冲突建议可对照原文、当前内容和建议手动合并；绘图保存为 SVG，支持重开、撤销和恢复。
- **开发者与 AI**：多文件 OpenAPI 本地引用与更新复用同一生成服务；过期源文件会使旧预览失效。聊天取消、失败与隐私模式遵循统一的历史保存边界。 发行收尾还覆盖 SSE 接收停滞时的取消，以及 Office 固定依赖许可通知的完整性。

详见[本版说明](release/release_notes.md)；[历次更新](RELEASE_NOTES.md)保留旧版记录。

本次同版更新增加五套原生 PPTX 设计：**合流、万象光场、浮光、液相、在场**，正文与表格以可编辑对象写入，保留各套设计的原生背景与可用动画。HTML 提供**出版阅读、技术文档、长文杂志**三种连续阅读版式，预览与独立文件共用正文、目录、排印和宽表格处理。设计来源与内嵌方式见[模板说明](rust/readmd-kernel/templates/pptx/README.md)。

此前排版发布的全部 10 份桌面／集成资产由同一源码提交 [`43905601f2a0ae747f4000aad8e69a80e955ce7c`](https://github.com/Natsummerance/rust-ReadMD/tree/43905601f2a0ae747f4000aad8e69a80e955ce7c) 构建；[对应源码 ZIP](https://github.com/Natsummerance/rust-ReadMD/archive/43905601f2a0ae747f4000aad8e69a80e955ce7c.zip) · [本次发行 CI](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44669807015)。原 `V0.0.7` 标签保留首次发布时的提交，获取本轮源码请使用这里的固定链接。

<!-- readmd-autopublish:start -->
本轮图表阅读界面与 AI 修复已自动发布。全部十份桌面／集成包来自同一完整源码 [`f6267fdf820135bcb2b2f2dd56313e0ee2160646`](https://github.com/Natsummerance/rust-ReadMD/tree/f6267fdf820135bcb2b2f2dd56313e0ee2160646)；[发行 CI](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44750315786) 与 [PET 三系统检查](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44750341039) 均通过。应用保持 0.0.7，随包伴读组件为 1.0.2。
<!-- readmd-autopublish:end -->

本轮源码让阅读中的图表直接融入正文，源码与复制控件默认折叠。渲染失败时提供错误定位和本地修复预览；用户可主动把当前图表及诊断发给已配置 AI，生成可编辑候选，先预览再提出待审修改，最后明确保存。

## 下载与安装

桌面包、扩展和 MCP 配置包统一为 **V0.0.7**。[全部资产与 SHA-256](https://github.com/Natsummerance/rust-ReadMD/releases/tag/V0.0.7)。

| 平台 | 下载 | 安装说明 |
| --- | --- | --- |
| Windows 10/11 x64 | [安装版](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMDSetup-windows-x64.exe) · [便携 ZIP](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-windows-x64.zip) | 便携版须完整解压并保留 assets 和运行库，再运行 `ReadMD.exe`；需要 WebView2 |
| macOS 13+ · Apple Silicon | [DMG](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-macos-arm64.dmg) · [ZIP](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-macos-arm64.zip) | 把 `ReadMD.app` 移入“应用程序”；见 [Mac 安装说明](release/macos-install.md) |
| macOS 13+ · Intel | [DMG](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-macos-x64.dmg) · [ZIP](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-macos-x64.zip) | 选择 x64 包，使用系统 WebKit |
| Linux x86_64 | [DEB](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-linux-x86_64.deb) · [tar.gz](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-linux-x86_64.tar.gz) | 需要 GTK3、WebKitGTK 4.1 等系统运行库；DEB 声明所需依赖 |
| VS Code／兼容编辑器 | [readmd-vscode-0.0.7.vsix](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/readmd-vscode-0.0.7.vsix) | 安装桌面端后，从扩展菜单选择“从 VSIX 安装”；见[扩展说明](packages/vscode-extension/README.md) |
| MCP 客户端 | [readmd-mcp-server-0.0.7.zip](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/readmd-mcp-server-0.0.7.zip) | 文档与连接模板；内核使用已安装桌面程序的 `--mcp` 入口 |

下载后可用 [SHA256SUMS.txt](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/SHA256SUMS.txt) 核对文件。替换旧版前先退出应用，个人文档无需移动。

Mac 包采用**免费 ad hoc 签名，未使用 Developer ID，未作 Apple 公证**。首次打开可能被 Gatekeeper 拦截；按[安装说明](release/macos-install.md)在“系统设置 → 隐私与安全性”中仅允许 ReadMD 打开。

本版发布目标为 Windows x64、Linux x64、macOS ARM64／Intel，并提供 VSIX 和 MCP 配置包。UOS、银河麒麟、Deepin 等系统的兼容性取决于系统运行库与对应实机记录。Windows／Linux ARM64、AppImage、HarmonyOS／OpenHarmony 不在本版下载资产内。

## 原版阅读与可选能力

桌面分发包包含匹配平台的 Office 阅读运行时及离线 PDFium 包。应用按需准备这些资源，功能状态会区分安装、探测和实际操作。Office 原排版视图使用随包排版引擎，复杂文档的显示可能与 Word／PowerPoint 有差异；提取为 Markdown 会明确报告不能保留的内容。

OCR、Typst、出版字体、代码运行时和音视频转写按实际平台目录提供。扫描 PDF 需要 OCR 与 PDFium；转写需要匹配平台的 FFmpeg、Whisper 引擎和模型。**FFmpeg／Whisper 独立离线包不包含在上表的桌面和集成资产中**。本轮媒体包实测范围为 macOS ARM64，其他平台以实际可获得的包和能力状态为准。

PDF 默认以阅读模式打开；选择“编辑原 PDF”后进入支持的编辑操作，保存前保留恢复与源文件冲突保护。PDF 表单、签名、权限及复杂 Office 往返按具体格式的支持范围处理。PPTX 公式保留为图片并在备注保留源码。

## 本地工作流与 AI

- 阅读、编辑与本地转换在本机进行；网页提取、AI、邮件等联网操作需要用户主动调用并配置服务。
- AI 结果可逐块审阅；失败、取消和隐私聊天不写入部分聊天历史。保存正文、同意修改和任务完成分别显示状态。
- 分享支持局域网阅读、编辑和建议审阅；共享内容在本地明确保存后才写入文档。
- 可使用 BibTeX 卡片、学术块、公式编辑、离线 HTML 演讲与桌面伴读宠物。桌宠全局键鼠监听以 Windows 支持范围为准。
- 提供 46 种界面语言；键与占位符有自动检查，部分新增工作流提示仍使用英文。

## VS Code 与 MCP

安装 [VSIX](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/readmd-vscode-0.0.7.vsix) 后，使用命令面板中的 ReadMD 命令进行预览、修复、转换、导出、工作区检索和 OpenAPI 文档更新。需要时将 `readmd.executablePath` 设为桌面可执行文件的绝对路径。行内 AI 补全默认关闭，可在受信任工作区中选择启用。详见[扩展文档](packages/vscode-extension/README.md)。

MCP 与桌面共用 Rust 内核，不需要 Python。把以下 `command` 替换为实际可执行文件的绝对路径：

```json
{
  "mcpServers": {
    "readmd": {
      "command": "/absolute/path/to/readmd",
      "args": ["--mcp"]
    }
  }
}
```

macOS 通常为 `/Applications/ReadMD.app/Contents/MacOS/ReadMD`；Windows 使用 `ReadMD.exe` 的完整路径，JSON 中反斜杠写作 `\\`。更多模板见 [MCP 说明](packages/mcp-server/README.md)。

## 验证与源码

此前排版发布的正式 Apple Silicon ZIP 已在本机完成两次原生冷启动／重开，并更新安装。Windows／Linux 由 CI 中实际发行二进制的生命周期与图像导出回归覆盖；Intel 由 Mac CI 构建、签名及封包检查覆盖。

2026-11-19 首次发布的 Mac 本机候选在 ARM64 上完成六组共 **244 项原生检查**，包括关闭与恢复、六格式导出、候选 ZIP 冷启动／重开和 V0.0.6 隔离会话升级；另从归档中的生产二进制执行 46 语言的保存编码流程，页面异常为 0。[Mac 验证记录](docs/reviews/readmd-v007-macos-rc-2026-11-19.md)保留一次 WebKit 冷启动首败及同包复测结果。这里的检查范围与跨应用兼容性、各系统实机覆盖分别记录。

2026-11-19 首次发布时，Windows x64、Linux x64、macOS ARM64／Intel 的发行构建和共享质量检查已通过。该次正式 ARM64 下载 ZIP 另完成正常系统冷启动与重开，原测试文档字节保持；本次正式包与早期 244 项候选检查分别记账。见[发布验证记录](docs/reviews/readmd-v007-published-2026-11-19.md)与[平台矩阵](release/platform-matrix.json)。

```sh
cargo build --locked --release --manifest-path rust/Cargo.toml -p readmd-kernel
```

桌面二进制位于 `rust/target/release/`。分发包还需要前端资源和平台运行时，相关脚本在 [tools](tools) 与 [scripts](scripts)。可选代码执行需要对应语言运行时。

[产品索引](https://rust.readmd.asia/llms.txt) · [完整产品资料](https://rust.readmd.asia/llms-full.txt) · [问题反馈](https://github.com/Natsummerance/rust-ReadMD/issues) · [MIT 许可](LICENSE)
