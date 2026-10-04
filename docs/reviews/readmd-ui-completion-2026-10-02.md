# 功能盘点、实现与验收记录

> 后续逐项复查已完成，最新结果和101项证据见 [真实实现复查](readmd-ui-feature-verification-2026-10-03.md)：250桌面UI、1移动UI、1798内核、114桌面宿主、71桌宠均通过；389静态控件。下表保留首次盘点验收记录。

更新：2026-10-03，北京时间；源码基线 `05ec02d`。用户要求按层级盘点每个前端可操作入口，写清按钮、流程、交互和 Rust 实现，并在检查中补齐已设计功能、修复 bug。本轮源码、文档和测试修改留在当前工作树，尚未提交或推送。

[完整清单](readmd-ui-function-inventory-2026-10-02.md) 包含 **21 个层级、101 条操作流程、387 个静态控件、74 类动态交互**。24 份附件包括 143 行转换格式矩阵、14 个插件、98 个导出配置字段、65 条 Shell 命令、23 条斜杠菜单、99 条 HTTP 路由与桌面 IPC 桥。清单保留条件入口、旧入口及明确拒绝的格式；1,161 处源码引用检查了路径和行号。

## 补齐与修复

| 项目 | 最终行为与实现 | 验证 |
| --- | --- | --- |
| 代码顶部编辑、演示命令 | 代码按钮调用 toggleEdit 并保留源码；view.presentation/F5 播放，export.presentation 导出 Reveal，独立注册 | 真实代码文档和命令端到端 |
| 离线音视频转写 | Rust 调用已安装系统 SAPI，压缩媒体用本机 FFmpeg；自动/中/英/日语言传递到单文件、拖放与批次，错误/无语音明确反馈 | 实际两段语音 WAV、MP3、语言/错误/超时/取消 |
| 时间戳 | 使用输入流绝对位置，去除上一段结束时间的重复累加 | 多段语音最后时间不超过 WAV 时长 |
| 批次保持与取消 | 关闭保留轮询/锁，重开显示原批次，拒绝重复提交；首个未开始 OCR 也能取消 | 关闭/重开、重复提交、任务取消测试 |
| ZIP 选择转换 | 文件选择、收集、拖放进入原生拆包和子文件转换，写真实 Markdown | 浏览器选真实 ZIP，HTML/CSV 两项转换；Rust 路径/大小限制 |
| 原生插件生命周期 | 14 个目录项对应明确 Rust 能力；原子 profile/manifest 安装、读取校验、互斥启用、持久化、失败回滚、运行占用保护及卸载 | Rust 生命周期；浏览器安装→执行→关开关→重载→卸载 |
| 动态/私有网页 | 隔离系统 WebView 捕获实际 DOM；登录后显式授权，grant 限定任务/来源/时效；提取、超时、取消、窗口关闭、撤销各有路径 | 真实 WebView2 动态正文、Cookie、输入值剔除、桥隔离及关闭窗口 |
| EML/MSG | MIME 编码头/multipart/正文/HTML/附件/CID；MSG 分层 CFB 普通/小流、代码页、嵌入邮件与压缩 RTF，资源路径验证 | 实际结构邮件样本、小流、压缩 RTF 官方向量、同名属性隔离 |
| PDF 真实预览 | 先生成真实 PDF，展示实际页面 PNG/尺寸/文字；大预览按实际页数；同输入且缓存有效时导出复用产物 | PNG 解码，导出文件与预览 PDF 逐字节相同 |
| DOCX 预览 | 实际生成 DOCX，并以共享 AST/配置的 PDF 显示版式参考；界面注明 Word 分页边界 | DOCX 内容/预设端到端、共用内容与设置路径 |
| 空内容导出 | 编辑器清空后导出空内容，避免回退到旧正文 | 浏览器空编辑内容断言 |
| TeX 文献 | biblatex/bibtex/natbib 进入 preamble/引用命令；局部 .bib 复制到资源目录，路径越界有警告 | 三引擎、引用、资源复制和越界测试 |
| PDF 子文档 | 现有 WinRT PDF 组件将指定页渲染为 PNG；检查页码/预算，页码 0 明确拒绝 | 真实 PDF、PNG 签名、页码/预算错误 |
| 富文本剪贴板 | Rust 本地 HTML→Markdown 保留标题/强调/列表，去脚本，纯文本回退 | 实际 HTTP 接口与剪贴新建流程 |
| WSD/D2/ditaa | 已有围栏/重试入口接入 Rust 基础离线 SVG，复杂未知语法明确失败 | 25 项图表相关 Rust 测试；浏览器三个实际 SVG；见 [语法范围](ui-function-inventory-2026-10-02/native-diagrams.md) |
| 叠加弹层焦点 | 共同 MutationObserver 按变更顺序维护弹层栈，Tab 留在最上层 | 原先失败项、IME、Esc、焦点返回 |
| 桌宠隐藏预览加载 | 设置未打开时不构建角色预览/下载整库缩略图；打开后按实际角色加载 | 伴读使者和多角色图片解码；两种尺寸；启动不请求 thumb |
| 公开静态资源 gzip | 复用已有 flate2 压缩 JS/CSS/JSON/SVG，按 Accept-Encoding 协商，区分 ETag，设置 Vary | 解码还原、q=0、条件请求、实际启动预算 |
| 测试夹具 | 插件持久化使用真实安装；开关点可见 label；高级设置先展开；对比度读当前主题样式；启动恢复真实默认配置并清理路由；源码形状检查兼容 CRLF | 保留原业务断言与性能预算，对应失败项重测通过 |

桌宠回归使用真实离线渲染资源，验证 BongoCat、原创精灵图与 Live2D 的各自机制、输入释放、拖动阈值、透明区穿透、紧凑尺寸、点击台词/动作、右键菜单、角色变化、锁定、置顶及偏好保存。在 `1160×820`、`1024×680` 下，伴读使者和多角色预览均实际解码，面板位置与点击区域检查通过。

## 验证结果

| 检查 | 结果 | 日志 |
| --- | --- | --- |
| Rust 内核 library 全套 | **1,795 通过、0 失败、3 条条件忽略** | [完整日志](<local-evidence>) |
| Rust 桌面宿主 binary 全套 | **114 通过、0 失败** | [日志](<local-evidence>) |
| 独立 Rust 桌宠全套 | **71 通过、0 失败**；binary/doc-tests 无测试项 | [日志](<local-evidence>) |
| 实际离线桌宠包安装 | **1 通过**；已有本地包，单独执行条件忽略项 | [日志](<local-evidence>) |
| 桌面 UI 全套与复测 | 全套 **222 通过、2 个夹具失败、1 个移动端专属跳过**；两项失败在 9 项复测中通过；最终 **224 个桌面用例已通过** | [全套](<local-evidence>)、[9 项复测](<local-evidence>) |
| 移动端点击区域 | 上述桌面跳过项在 mobile project **1 通过** | [日志](<local-evidence>) |
| 原生 WebView2 | **通过**：动态 DOM、授权/Cookie、独立桥、输入剔除、超时、取消、标题栏关闭、撤销 | [日志](<local-evidence>) |
| i18n | **通过**，46 语言 / 1,963 keys | [日志](<local-evidence>) |
| wiring / styles / assets / no-python | **全部通过**；267 份资源、1,148 条 provider；10 个历史 Python 允许项 | [接线](<local-evidence>)、[样式](<local-evidence>)、[资源](<local-evidence>)、[依赖](<local-evidence>) |
| tools 检查器单测 | **26 通过、0 失败** | [日志](<local-evidence>) |
| 前端离线包 | `cargo xtask bundle-boot --check` **通过**，boot 已重打包 | [日志](<local-evidence>) |
| Release 离线构建 | 最终源码 **通过**；二进制位于 Z:/readmd-target/main/release/readmd.exe | [日志](<local-evidence>) |
| 差异空白 | `git diff --check` **通过** | 无输出、退出 0 |
| 隔离运行快照 | 两种尺寸各打开8个面板、6个导出页签；编辑器初始化；pageerror=[] | [目录与记录](ui-function-inventory-2026-10-02/catalogs.md) |
| 清单一致性 | 11份Markdown中的1,216个本地链接全部有效，JSON各项数量与实际记录一致 | 路径/源码行号检查，0错误 |

启动独立探针使用新临时目录，测得16次请求、535,858字节实际传输、约487 ms可交互。这是本机当次样本。自动测试保留原预算：可交互中位数<1,200 ms、首个内容绘制<850 ms、最大传输<1,250,000字节、请求≤35，复测通过。

两项桌面全套失败来自测试夹具：直接 `.uncheck()` 透明 input 被可见 switch label 拦截；启动样本关闭 context 时仍在 route.fetch。前者改为点击真实 label 并等保存解锁；后者用真实 API 恢复默认禁用宠物配置，取消重写响应的 fetch，关闭前清理路由。相关业务和预算断言保留。交接提到的旧对比度及叠加弹窗失败也已修复并通过。

## 重跑命令

在仓库根目录复用已有 Cargo cache 与 Node 测试依赖，路径可替换为本机已有安装。测试二进制是独立副本，避免 Windows 构建锁；真实桌宠渲染器通过仓库的 build-renderer.mjs 离线重建。

```powershell
$env:NODE_PATH='<existing-ui-node-modules>'
$env:READMD_BIN='<local-evidence>
$env:READMD_PET_RENDERER_BUILD='<local-evidence>
$env:READMD_UI_PORT='28650'
node '<existing-ui-node-modules>/@playwright/test/cli.js' test --config ui-tests/playwright.config.js --project desktop --retries=0 --reporter=line
node '<existing-ui-node-modules>/@playwright/test/cli.js' test function-completion.spec.js startup-performance.spec.js --config ui-tests/playwright.config.js --project desktop --retries=0
node '<existing-ui-node-modules>/@playwright/test/cli.js' test touch-targets.spec.js --config ui-tests/playwright.config.js --project mobile --retries=0
node ui-tests/native-web-test.cjs

cargo test --offline --locked --manifest-path rust/Cargo.toml -p readmd-kernel --target-dir Z:/readmd-target/main --lib
cargo test --offline --locked --manifest-path rust/Cargo.toml -p readmd-kernel --target-dir Z:/readmd-target/main --bin readmd
cargo test --offline --locked --manifest-path packages/readmd-pet-rust/Cargo.toml --target-dir Z:/readmd-target/pet
cargo build --offline --locked --release --manifest-path rust/Cargo.toml -p readmd-kernel --target-dir Z:/readmd-target/main

node tools/check-i18n.mjs
node tools/check-wiring.mjs
node tools/check-styles.mjs
node tools/check-assets.mjs --check
node tools/check-no-python.mjs
node --test tools/test/*.test.mjs
# 在 rust/ 目录运行，xtask alias 自带 --offline。
cargo xtask bundle-boot --check
```

## 条件与范围

- 143行格式矩阵逐行核对分派、读取器、资源和拒绝规则；“源码核对”区别于每种后缀都拿真实样本执行。旧Office、PDF、邮件、代码、HTML、表格、媒体等实际路径和限制均保留。
- 语音质量由系统识别器决定，压缩媒体需要本机FFmpeg，语言需要对应识别器。本机有中/日/英识别器，真实语音测试使用英文。兼容ID whisper/faster_whisper 显示 Rust/system-speech 能力，不声称运行同名Python模型。绝对音频位置语义参考 [Microsoft 文档](https://learn.microsoft.com/en-us/dotnet/api/system.speech.recognition.recognizedaudio.audioposition?view=netframework-4.8.1)。
- PDF预览来自实际产物，缓存6份/10分钟。DOCX实际生成，预览是共享AST/配置的PDF参考，打开后的字体/分页以Word为准。WSD/D2/ditaa的完整高级语法边界见附件。
- 外部AI、GitHub Skill、更新下载、PlantUML显式远程模式按账号/网络/授权条件记录；系统对话框、文件关联、自启、OCR、桌宠适配按平台条件保留。真实系统验证环境为Windows。
- 三条Rust条件忽略项中，本地桌宠包安装已另跑通过；联网HTTPS探针和旧Python PDF对照产物生成不作为离线验收执行。没有新增依赖；构建全部离线；凭据不进入清单；交接文档没有修改或提交。

所有真实业务测试使用隔离临时数据。WebView2只访问自建本机网页；关闭动作只匹配测试进程的辅助窗口。语音fixture不播放、不启用麦克风。审计生成脚本保留于 <local-evidence>
