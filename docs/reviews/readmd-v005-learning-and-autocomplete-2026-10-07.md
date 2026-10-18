# V0.0.5：学习依据、AI 补全与平台验收

## 先回答：是否吸收了所有优点？

没有。上一份[社区清单](readmd-community-research-2026-10-07.md)列出 28 个产品、4 个组件，主要属于官方功能与设计对照；不能用“列出项目名”代替深入源码、真实运行或完整复刻。此次补查官方文档、宣传与特定源码模块，以下分开记录。闭源产品只参考公开资料，不声称读过其未公开代码。没有引入另一套文档数据库、编辑器或新的顶栏。

## 实际研究范围

| 工具 | 本次证据与学习内容 | ReadMD 实现及差距 |
| --- | --- | --- |
| [Obsidian](https://obsidian.md/help/links) | 官方双链、链接目标与属性资料；未阅读闭源核心 | document_intelligence.rs / link_indexer.rs：Wiki、锚点、歧义与失效目标检查。没有 Obsidian Sync、Canvas 或完整插件生态。 |
| [Typora](https://support.typora.io/Markdown-Reference/) | 官方语法与编辑交互资料，非核心源码 | Markdown、公式、表格、任务、脚注与即时预览；当前是明确源码/预览模式，未声称复刻 Typora 所见即所得模型。 |
| [VS Code Markdown](https://code.visualstudio.com/docs/languages/markdown) | 官方预览安全、导航、路径建议；源码资源粘贴/复制模块 | VS Code 原生 Problems、Wiki DocumentLink、Quick Pick、离线 WebView。宿主标准能力继续复用。 |
| [MarkText](https://github.com/marktext/marktext) | 阅读 packages/desktop/src/renderer/src/store/editor.ts：草稿、编辑会话与历史 | CodeMirror 撤销及独立预览；没有复制 Muya 富文本实现。 |
| [Joplin](https://joplinapp.org/help/apps/note_history/) | 官方历史恢复；packages/lib/services/RevisionService.ts 及相关测试 | document_history.rs 集中保存检查点，冲突前置检查，删除原件后仍可审阅；没有跨设备同步。 |
| [Zettlr](https://www.zettlr.com/features) | 官方学术写作与检索；citations.ts / files.ts / code-blocks.ts | bibtex.rs、引用、LaTeX、DOCX/PDF/EPUB 导出、工作区检索；不宣称完整 CSL/Pandoc 生态等价。 |
| [SiYuan](https://github.com/siyuan-note/siyuan) | kernel/model/history.go / history_diff.go 与测试 | 吸收历史、差异与明确恢复原则；本地 Markdown 文件仍是原件，没有移植专属块数据库。 |
| [Continue](https://docs.continue.dev/ide-extensions/autocomplete/how-it-works) | CompletionProvider.ts、BracketMatchingService.ts、StreamTransformPipeline.ts | 防抖、上下文、取消、缓存、后处理与灰字/Tab；没有项目级检索增强、Next Edit 或模型训练。 |
| [Tabby](https://github.com/TabbyML/tabby) | codeCompletion/debouncer.ts；Rust completion.rs / completion_prompt.rs | 输入节奏防抖、并发限制、真实 FIM 令牌协议、上下文重叠消除；没有复制其推理服务、索引服务或账号系统。 |
| [ohos-rs](https://github.com/ohos-rs/ohos-rs) | Rust → N-API → ArkTS 架构及许可；[Rust OHOS 目标说明](https://doc.rust-lang.org/rustc/platform-support/openharmony.html) | 使用现有 Rust 内核的 C ABI + 系统 N-API，不增加 napi-rs / ohos-rs 依赖。SDK 与设备验证待完成。 |

此前还做了功能级对照、此次没有逐个深读源码的产品：VNote、ghostwriter、QOwnNotes、ReText、MacDown、Remarkable、StackEdit、Dillinger、HedgeDoc、CodiMD、Markor、Glow、Frogmouth、Markdown Viewer、Markdown All in One、Markdown Preview Enhanced、Foam、SilverBullet、Logseq、TriliumNext、Notesnook、Dendron、AppFlowy。组件：Milkdown、Vditor、Cherry Markdown、TOAST UI Editor。它们的官方来源逐项保留在原社区清单。对照不等于全量移植，也不代表在本机安装运行过全部项目。

本机原始研究快照及各文件 SHA-256 位于 .cache/v005-ai/research/evidence.json。读取上游模块用于理解机制；GPL/AGPL 模块没有直接复制进 ReadMD。实现采用项目自己的 Rust/CodeMirror/VS Code 架构。没有根据 GitHub 星数宣称某一个项目“最先进”。

## AI 自动补全的实际架构

- assets/skills/readmd-autocomplete：可替换的补全文案，沿用内置 Skill 管理，不把系统提示写死在前端。
- ai_completion.rs：Unicode 上下文限额、前后文打包、指纹、FIM 协议、推理标签/终止标记过滤、重复和右侧重叠消除。补全不读取磁盘、剪贴板、其它文档或私人路径。
- server.rs / ai_providers.rs：POST /api/ai/complete 复用保存的连接、凭据库及 OpenAI/Responses/Anthropic 适配。新增 `Accept: text/event-stream`，从上游流式增量得到清理后的候选快照，立即 flush 给桌面；MCP 继续使用同一引擎的 JSON 结果。单次生成上限从 256 降为 128 token；有完整短句时提前返回，不等待长回答。禁止原始密钥、URL、Skill、会话字段改变此通道权限。
- App 持有补全 Runtime：复用 HTTP Agent；按连接隔离的模型时延/失败观测、短期模型目录和 32 项/60 秒候选缓存，仅存内存，不保存正文或测速历史。模型名只作为冷启动启发式，不代表测得性能或模型质量认证。
- AI 高级参数中增加一项独立补全模型：自动优先速度、跟随对话模型、固定模型。保持当前连接和凭据，不跨连接发送，不修改对话模型。自动失败可有限尝试当前连接的另一个候选，固定模型不暗中切换。
- ai-inline.js：CodeMirror Widget 灰字；动态 180–650 ms 防抖、24 项/60 秒内存缓存，命中直接显示；上传附近 3000 字前文和 750 字后文。Esc、组词、失焦、页面隐藏、连接切换及新版本取消旧候选。继续输入候选前缀时保留剩余灰字。
- 真实流式候选逐步出现；反代一次返回大块文字时，只对已收到的字做至多约 120 ms 的短暂呈现。尊重系统减弱动态设置。Tab 接受当前可见片段并终止后续内容；中断或错误清掉临时候选，原稿保持不变。
- Tab 才插入草稿，一次撤销可恢复。未接受的灰字不进入保存、字数、预览、草稿历史。没有自动写文件。
- 默认关闭，位于已有“编辑视图”菜单；启用前提示只上传光标附近内容。Ctrl/⌘+Alt+Space 手动重试。表格 Tab、斜杠菜单和多选区保持各自优先级。
- VS Code：readmd.aiAutocomplete 默认 false，工作区信任检查；readmd.aiAutocompleteModel 可独立固定模型，留空沿用共享策略。原生 InlineCompletionItem 接受/撤销，版本、位置、取消令牌检查；宿主原生接口返回一个候选，桌面才逐步绘制灰字，不混称相同的渲染机制。
- MCP 新增 readmd_ai_complete，总数 28。只返回候选，不做文件写入，也不进入聊天历史。
- 扩展：标准 chat 模型直接可用；专门的 completion 提供方可选择 qwen / starcoder / deepseek FIM，发送真实前缀/后缀令牌。FIM 协议经模拟真实传输检查；本机 Antigravity 是 chat 模型，不能把它描述成训练/验证过 FIM 模型。

边界：防抖与取消能阻止旧内容进入草稿，但无法强行中断第三方服务器已经开始的生成；Rust 在安全点丢弃取消结果，超时和并发上限限制后台占用。模型可能返回空候选或不合适的表达，使用者可忽略；不宣称生成内容天然正确。

## 原生平台与打包

| 平台 | 这次完成的内容 | 证据边界 |
| --- | --- | --- |
| Windows x64 | 真实 WebView2 / 当前 Antigravity / Tab、Esc、撤销、故障路径 | 本机可实测，以验收记录为准 |
| macOS Intel / ARM64 | 沿用 tao/wry WKWebView、osascript 原生文件对话框与现有发布作业 | 当前 Windows 主机不能替代 Mac 实机验收 |
| Linux、UOS、Kylin、Deepin x64 / ARM64 | 修复离线构建命令、资源复制、版本源及 GTK3/WebKitGTK 4.1 精确依赖；zenity 不可用时继续尝试 kdialog | 没有在本机安装这些系统，不把脚本修复算成实体机通过；老系统缺少 4.1 时应明确安装失败 |
| openEuler / Anolis | RPM 构建入口、明确架构及原生运行时硬依赖 | RPM 不等于所有发行版、CPU 和显卡驱动已验证 |
| HarmonyOS NEXT / OpenHarmony | Rust mobile-ffi → C ABI → 系统 N-API → ArkTS → ArkWeb，嵌入同一个 loopback HTTP 内核；离线资源清单；原生选择器/另存为/剪贴板 | 已验证 Rust 嵌入服务启动、HTTP、停止与 ABI 生命周期；本机没有 OHOS SDK、目标 std、签名或设备，因此不提供未经验证的 HAP，不标成正式支持 |
| 龙芯/MIPS/申威/RISC-V、Windows ARM64 | 明确列为尚未验证 / 当前无发布作业 | 不用更改包名伪装成支持新架构 |

release/platform-matrix.json 已更新到 0.0.5，分别列证据、实验支持与待验收平台。鸿蒙打开文件先导入应用私有区，原始外部文件保持原状；另存为须经系统选择器。托盘、桌面桌宠、系统 OCR 等平台专属能力仍需各平台原生接入与设备验收，不能从共享内核推断已支持。

## 验收与媒体

后续独立模型、真实流式灰字、速度对照和新增 4K 补录见[流式补全复核](readmd-v005-streaming-completion-2026-10-07.md)。该记录包含初版选模失败、最终修复，以及同一个 Gemini 未必更快的真实结果，不用缓存耗时冒充推理速度。

完整结果见[本轮验收与发布边界](readmd-v005-acceptance-2026-10-07.md)；原始日志保留 .cache/v005-ai。19 个内置 AI Skill 都取得真实成功结果，但 Gemini 的摘要、大纲、导出样式超时，需要用同一反代的 Claude Sonnet 才通过，不能宣传成单一模型全部正常。真实 ReadMD 补全、VS Code 原生补全及生成 Skill 的评估/发布/执行闭环均另有验收。

所有 AI 测试只使用合成研究文字，使用已保存的 Antigravity 凭据，不输出密钥或远程令牌，不改日常配置。官网与补录仅展示已完成能力；媒体、社交素材仍留在被忽略的 showcase/updates/v0.0.5，不进入软件提交。V0.0.5 保持候选，未自动发布。
