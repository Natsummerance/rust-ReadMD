# V0.0.5 本轮验收与发布边界

日期：2026-10-07。V0.0.5 为本地候选，尚未发布。本报告区分真实连接、模拟异常、源码检查与尚无实机证据的部分；不将“存在按钮”“返回 HTTP 200”计为功能成功。

本报告之后的独立补全模型、真实 SSE、设置保存保护及新录制验收见[流式补全复核](readmd-v005-streaming-completion-2026-10-07.md)。该记录增加了 release 二进制真实请求，不覆盖本报告的首次失败与平台边界。

## 桌面补全与实际 AI

在隔离的真实 Windows WebView2 中，用保存的 Antigravity 连接验证模型发现、实际上下文补全、灰字不入草稿、原生 Tab 接受、一次撤销、Esc、内存缓存和聊天历史不保存补全上下文。没有改动日常连接配置，没有记录密钥。模型列表返回 23 项；实际使用当前 `gemini-3.8-flash-tiered`，列表没有 `gemini-3.8-flash-high`，因此没有声称测试了后者。

19 个内置 AI Skill 均经过真实 Rust `/api/ai/chat` 渲染和请求并拿到成功结果；人工阅读了合成材料的返回内容。**这不是同一模型全部通过**：Gemini 的摘要、大纲、导出样式在 65 秒验收期限内超时，重复仍超时；同一 Rust 路由和同一反代改用 `claude-sonnet-4-6` 后三项分别在约 4.95、4.27、6.76 秒成功。不得抹去首次失败或自动替用户切换模型。是否为模型路由、代理或上游服务问题，尚未定位到具体组件。

| Skill | 成功实测模型 | 内容检查 |
| --- | --- | --- |
| readmd-ask | Gemini 3.8 Flash tiered | 区分原文事实、推论与未提供信息 |
| readmd-summary | Claude Sonnet 4.6 | 一句话摘要保留观察值和后续任务 |
| readmd-outline | Claude Sonnet 4.6 | 三个 Markdown 标题 |
| readmd-polish | Gemini 3.8 Flash tiered | 原意、任务清单及代码保留 |
| readmd-translate | Gemini 3.8 Flash tiered | 英文正文，数值、任务和代码保留 |
| readmd-quick-read | Gemini 3.8 Flash tiered | 要点、未决问题；发现示例代码缺少除法 |
| readmd-code-analysis | Gemini 3.8 Flash tiered | 识别 sum/mean 差异、输入与空数组边界 |
| readmd-code-review | Gemini 3.8 Flash tiered | 指出真实代码缺陷并给出修正建议 |
| readmd-code-to-doc | Gemini 3.8 Flash tiered | 按实际求和行为解释，不将错误代码描述为正确均值 |
| readmd-continue | Gemini 3.8 Flash tiered | 续写一句与研究上下文相关的话 |
| readmd-export-style | Claude Sonnet 4.6 | 结构化 typography/headings/table/page/epub 配置 |
| readmd-format-fix | Gemini 3.8 Flash tiered | 补全代码围栏，不修改函数逻辑 |
| readmd-humanizer | Gemini 3.8 Flash tiered | 简洁改写，代码和任务保持 |
| readmd-localization | Gemini 3.8 Flash tiered | 第一段英文表达 |
| readmd-proofread | Gemini 3.8 Flash tiered | 说明无明显文字错误，单独标明格式调整 |
| readmd-skill-creator | Gemini；修订后另用 Claude | 发现模型虚构元数据/已执行评估，收紧真实格式与证据要求；结果仍须工作台校验 |
| readmd-style-custom | Gemini 3.8 Flash tiered | 有效 JSON，CSS 仅设置正文 font-size |
| readmd-todo | Gemini 3.8 Flash tiered | 正确提取未完成项，不捏造负责人和优先级 |
| readmd-weekly | Gemini 3.8 Flash tiered | 根据完成清单与下周任务组织内容；模型文风和长度仍可能偏离额外要求 |

另实测 Skill 工作台：真实 AI 生成禁用草案 → 无评估发布被拒绝 → 评估 → 篡改草案令牌被拒绝 → 明确确认发布到隔离目录 → 调用生成的 Skill → 导出文本完全相同 → 禁用 → 删除。不会将模型声称“已测试”当成真实执行。模型生成的文件名、措辞与建议仍需要审阅，软件校验控制可执行内容。

补全专用 Skill 保留在已有工作台检查，移出普通聊天动作下拉，避免将前后文 JSON 协议当成文档聊天使用；名称和说明补充多语言键。模型列表中 Gemini 3.5 实际返回停用公告，因此实时验收脚本新增语义失败检查，HTTP 200 也可判失败。不会把退役公告当成 AI 成功输出。

## 自动化与原生证据

| 检查 | 结果和范围 |
| --- | --- |
| Rust 全量 | 1882 个库测试、116 个主程序测试、12 个 xtask 测试通过；3 项明确忽略。最后的小改动另外复测补全 7 项、MCP 9 项 |
| 实际桌面补全 | WebView2 + 当前真实连接，Tab/撤销/上下文不落历史通过 |
| 前端补全 | 6 项通过：灰字/Tab/撤销/缓存、输入候选前缀、长候选底部可见、专用 Skill 下拉隔离与中文名、禁用/防抖/选区/IME、错误后原稿保持 |
| VS Code | TypeScript 编译和 49 项扩展测试通过；真实 Extension Host → MCP → Rust → AI 返回 54 字，原生 Tab 接受、一次撤销和磁盘未修改通过 |
| MCP | 实际 stdio 28 工具；128 并发请求中 57 完成、71 按上限返回忙碌；重复 ID、取消、超限后恢复通过。不是 128 项同时生成成功 |
| 鸿蒙内核接口 | `mobile-ffi` 生命周期测试通过：嵌入服务、真实 HTTP、重复启动/停止、参数与返回内存；419 个资源检查通过 |
| 前端与隐私门禁 | i18n 46 语言 / 2057 键，wiring、styles、assets、no-python、privacy scan 和 diff 检查通过；英文回退不等于所有语言均人工翻译 |
| 官网 | 离线构建；四语言补全页 × 明暗 8 项、手机无横向溢出、真实 4K 视频解码播放；滚动动效 43 项通过；53 个 canonical 页面、feed 和 sitemap 一致 |

真实连接测试是显式启用的脚本，不进入默认离线测试：`ui-tests/ai-completion-native.cjs`、`ai-skills-native.cjs`、`vscode-completion-native.cjs`。使用隔离配置/资料；原始日志在本机 `.cache/v005-ai`。旧原生热键测试曾因焦点尚未稳定而没有发出请求，已经通过请求记录与真实灰字重测，不把测试驱动时序问题冒充模型错误。

## 演示修正

桌面补全重录为完整 ReadMD 主窗口，3840×2160、16:9、7.2 秒。标题、工具栏、正文和底部完整可见；模型实际等待 12.999 秒被压缩，说明保留。以“灰字 → Tab → 撤销”三段清楚展示内容变化；撤销段停留在恢复后的原稿，不混入再次请求的候选。

完整宣传片 3840×2160 / 30 fps / 154.5 秒，新增补全在 130–140 秒；原转换放大、音效及所有旧镜头保留。76–125 秒七段先展示完整窗口再进入近景；安全编辑完整呈现左右差异，避免文字被画框截断。扩展演示明确标注 VS Code 与 Rust MCP，图库也单独标明环境。最终成片使用 `ReadMD-V0.0.5-promo-4K-final.mp4`；十八处新构图与音视频包核对记录在 `context-verification.json`，播放器检查十二个章节与八条独立素材。

应用补录抽样 OCR 30 帧、官网补录 40 帧通过；原七段重录 218 帧检查继续保留。只录合成哲学材料的软件窗口，不录文件夹窗口、桌面、通知和编号。人工查看最终补全三个阶段及扩展标注样本，没有逐帧人工观看全片。

媒体和宣传制作工程留在被忽略的 `showcase/updates/v0.0.5`；官网 4K 媒体另有忽略规则，不进入 GitHub 软件提交。原用户修改的两个 showcase 脚本保持原散列。

最终成片验证后，清理本轮临时视频分段和隔离测试配置的命令被自动审批策略拒绝，未给出更具体原因；没有重试，也未删除这些资产。记录在 `showcase/updates/v0.0.5/checks/cleanup-ai-2026-10-07.json`。成片、素材与复用脚本均保留；不能宣称本轮缓存清理已完成。

## 尚未满足的发布条件

Windows 本机通过不等于 macOS、Linux、UOS/Kylin/Deepin、openEuler/Anolis 或鸿蒙设备通过。Linux 修复和发布契约经脚本检查；OHOS N-API/ArkTS 源码尚缺 SDK、目标标准库、签名与设备验收，不提供未经验证的 HAP。龙芯/MIPS/申威/RISC-V、Windows ARM64 没有本轮可发布构建。

当前 Gemini 在三种 Skill 上的反复超时须保留为模型兼容性限制；不能宣传“全部场景无 Bug”。模型输出也不是确定性事实。V0.0.5 尚未打正式标签或替换公开下载，官网公开稳定下载仍指向 V0.0.4。
