# V6-36 有限切片：连续识别保持文件内的短句时间基准

日期：2026-10-27。状态：本时间基准缺陷已实现，编译后候选的真实服务与最新 boot 的界面验收均通过；未实现选段 UI。

## 真实首败与独立参照

上一切片的合成音频中，两句不同的正文都显示 `00:01`。本轮先独立合成两个 WAV，再在整数 PCM 帧边界拼接：2 秒前静音、第一句、精确 80000 帧/5 秒静音、第二句、8 秒尾静音。采样格式为 16000 Hz、16-bit、mono，共 329520 帧/20.595 秒。

这条音频没有用户素材、录音、播放或 AI。声音来自本机 System.Speech.Synthesis；识别来自已安装的英语 System.Speech。两个语音窗口由拼接帧数及原始非静音样本确定，不读取产品时间戳推算。

测试将每个原生 `RecognitionResult.Audio` 写出为 WAV，再把其 PCM 字节唯一匹配回完整源 PCM，取得独立的物理起始帧。该逐字节匹配仅用于小型合成测试，不是生产算法。

旧候选 SHA-256 为 `61442176f01797c01caed9d9504c8b5b5a85214053ca6a580ad3a5a3976c47ff`。

| 原生操作 | 第二句 AudioPosition | 保留音频匹配源帧 | 物理起点 |
| --- | --- | --- | --- |
| 重复同步 Recognize，原 WAV | 4.8 秒 | 147120 | 9.195 秒 |
| 重复同步 Recognize，FFmpeg 规范化 WAV | 4.8 秒 | 147120 | 9.195 秒 |
| 单次 RecognizeAsync(Multiple)，原 WAV | 9.26 秒 | 148160 | 9.26 秒 |
| 单次 RecognizeAsync(Multiple)，规范化 WAV | 9.26 秒 | 148160 | 9.26 秒 |

两种操作模式均识别两句相同的正文。模式不同会影响保留片段的前后切边，因此同步与连续片段的起点不必相等；判据是每个结果报告的位置能否对应它自身音频在原 PCM 中的唯一物理位置。

原 WAV 与规范化 WAV 的 PCM SHA-256 均为 `fe994b2b1a3b50039a89d1315f56c2fb0ffefb49a16cf7420e3595b8c8b05498`，每一帧完全一致。既有 `/api/transcribe` 在有、无 FFmpeg 的两条真实路径均输出 `00:02`、`00:04`，第二句落在其独立物理语音窗口之外。因此不能归因于转码，也不是把正确位置显示为整秒造成的误差。

证据均在 `.cache/capability-upgrade/v006-speech-offsets/`：

- `offsets-gqUUxQ/report.json`：原生四模式、保留音频唯一帧匹配，以及现有接口首败。
- `browser-suMXz1/report.json`：正式 CSP 的 headless Edge 通过可见 Open、真实上传/转换生成 `00:02`、`00:04`，第二句物理窗口断言失败。
- `browser-0Bpd9m/report.json`：前置测试误以为必须关闭 legacy profile；fresh profile 中它未安装，toggle 返回 400。此运行完整保留，之后直接使用默认转换入口，没有安装或切换 profile。

同时修正上一切片规格中的诊断措辞：mdcheck 会把 `**[时间戳]**` 的星号转义，严格要求未转义星号的断言失败不能解释为 profile 隐藏了时间戳。真实默认生成稿仍保留括号时间戳。

## 根因与最小实现

`AudioPosition` 的时间基准是当前引擎处理音频的识别流。[Microsoft 文档](https://learn.microsoft.com/en-us/dotnet/api/system.speech.recognition.recognizedaudio.audioposition) 描述该位置属于输入音频流；[官方实现的说明](https://github.com/dotnet/runtime/blob/main/src/libraries/System.Speech/src/Result/RecognizedAudio.cs) 指出引擎开始处理音频时流基准从零开始。本机实测确认，反复同步 Recognize 会在短句间重新启动操作，后续结果的位置不再对应整文件的同一基准。直接把这些值当作文件绝对起点就会产生重复或提前的时间戳。

唯一产品源码改动位于 `speech.rs` 的 PowerShell SCRIPT：

1. 同一输入调用一次 `RecognizeAsync(RecognizeMode.Multiple)`。
2. 在该独立 PowerShell 子进程注册真实 SpeechRecognized 和 RecognizeCompleted 事件，通过事件队列顺序消费结果。
3. 仍直接使用结果 AudioPosition、Text 和 Confidence，不累加前一句时长，不增加人为静音估计，不读取 wall-clock 计算媒体时间。
4. 成功、异常路径均清理事件订阅；原有 Rust 子进程超时、取消/回收与后续取消检查继续生效。
5. 前一切片的 PCM 总时长解析与未知合法 WAV 的可选 duration 合同不变。

不改变格式化、服务端路由、UI、MCP、能力包或引擎选择，没有新依赖。持续由已安装的 Windows System.Speech 识别，不能称为受管 Whisper 或便携语音运行时。

## 定向验证、所有权与复现

Rust 源冻结 SHA-256：`f2d062505f3eef0db1c31f7109543a539084e66d1784cc83203fc627d931542f`。

`source-mwIWi2/report.json` 已记录 3/3、exit 0：独立物理帧窗口、原生保留音频匹配，以及从 speech.rs 提取实际 SCRIPT 直接交给 Windows PowerShell 执行。实际生产脚本返回 2.04 秒与 9.26 秒，并保持两句原生识别文本；连续模式的每个位置都与其音频在原 PCM 的匹配帧一致。该运行明确没有执行尚未构建的新内核，不能据此冒充编译后服务或 UI 验收。

本切片独占：speech.rs、`tools/test/v006-speech-offsets.cjs`、`ui-tests/v006-speech-offsets-native.cjs`、本规格，以及上一媒体规格中上述诊断措辞的纠正。没有新增镜像实现的 Rust 单元测试；回归直接执行真实生产脚本、原生引擎和物理 PCM 参照。现有 9 项时长边界测试继续由协调者统一运行。

本切片代理没有运行 Cargo、bundle、打包、提交或 computer use。统一 Rust 回归及候选构建由协调者执行。新候选到位后，按以下顺序执行：

```powershell
$env:READMD_BIN=(Resolve-Path '.cache/community-upgrade/build/debug/readmd.exe').Path
node tools/test/v006-speech-offsets.cjs
node ui-tests/v006-speech-offsets-native.cjs
node tools/test/v006-media-duration.cjs
node ui-tests/v006-media-duration-native.cjs
```

构建前定向诊断可用 `node tools/test/v006-speech-offsets.cjs --source-only`；报告会明确区分是否执行编译后内核。每次运行新建证据目录，保留首次失败，不自动重试。

## 编译后候选与最终界面证据

协调者已离线构建候选，二进制 SHA-256 为 `a0a5a7133a8323f28fc3fe35d8a113a2cf9fe2dfbcd9add18da6124bf1572c03`，speech.rs 仍为上述冻结摘要。

四个既定脚本已依次运行，均 exit 0，共 19/19 检查：

| 报告目录 | 通过检查 | 结果 |
| --- | --- | --- |
| `v006-speech-offsets/offsets-UWpxju` | 4/4 | 原生物理音频参照、实际生产 SCRIPT、编译后服务的有/无 FFmpeg 两条路径。 |
| `v006-speech-offsets/browser-sMFk07` | 3/3 | 既有默认 Open、可见时间戳、可见 Save 下载及磁盘文件重开。 |
| `v006-media-duration/service-AP306J` | 9/9 | 既有全部时长、转码、错误、无 FFmpeg、mu-law、真实识别进程取消及保存重开回归。 |
| `v006-media-duration/browser-drC79Z` | 3/3 | 既有默认时长界面和保存重开回归。 |

表中路径位于 `.cache/capability-upgrade/`，每个目录均含冻结的 report.json。该轮前端摘要为 `0227b45b659775c78717124fadeb5e7fd8eb6c701599fa9b1c7a3d2e7dad2177`。

新服务在有、无 FFmpeg 时均输出 `00:02`、`00:09`，对照旧值 `00:02`、`00:04`。源 PCM 摘要完全相同，识别正文摘要均为 `6c4ce41d668a50529508128934a7a1f108fe443f645571906f6fbf5232016997`，没有替换或修正引擎识别出的文字。

上一切片最初出现两个 `00:01` 的原合成素材也已复验：现在是 `00:01`、`00:05`，完整时长保持 `00:17`；从新旧实际保存稿去除时间戳后，全文完全一致。该次真实取消仍观察到 owned powershell 子进程已回收、任务 canceled、无稿件写出，从发起取消至完成并确认退出约 653ms。它包含观察开销，是单次端到端结果，不是 UI 反馈延迟、P95 或 ≤200ms 达标证明。

随后前端其他修复触发重打包，媒体源码未改。每次等待协调者完成打包后，均串行重新运行两份媒体界面脚本，没有覆盖早期证据：

| boot 摘要 | 时间戳界面报告 | 原时长界面报告 |
| --- | --- | --- |
| `f002cfaa658afef9b0f4a2d8c974e402abfccce79ead290fded8d333e423ea2f` | `v006-speech-offsets/browser-lIMt2Z`，3/3 | `v006-media-duration/browser-LBkuc2`，3/3 |
| `097866dd95c0f7267faf791d41572ed203d82584565eb73ca1909f0e06dd7b93` | `v006-speech-offsets/browser-YOQo3W`，3/3 | `v006-media-duration/browser-ufOhKO`，3/3 |

最新一轮两份报告均为同一候选二进制，正式 CSP 未改变，pageErrors 为空，外部浏览器请求数为零；可见 Open、实际 System.Speech 转写、可见编辑器时间戳、Save 下载及真实文件重开均通过。两份浏览器与其 owned 服务进程已退出，报告冻结。它们认证 headless Edge 既有流程，不冒充原生窗口或扩展宿主验收。

新 speech.rs 的 Rust 全库回归由协调者在这些浏览器结束后运行；本规格暂不复用上一候选的 4102 通过计数。

## 本切片不能证明的范围

- 物理帧参照对应提供给引擎的音频流；原压缩容器的编辑列表、视频时间线、轨道选择和原始范围偏移仍需单独合同和测试。
- 合成测试中两个不同短句的音频可唯一匹配；没有在生产路径实现音频内容搜索，也不据此保证所有重复短句、噪声或全部语言的识别精度。
- 选段、播放、范围保存重开、SRT/VTT、受管 FFmpeg/模型包、非 Windows 语音链路及生成稿 46 语言本地化没有完成。
- 取消测量仍是实际进程与任务完成观测，不能宣称 UI 反馈 P95 或 ≤200ms 已达标。
