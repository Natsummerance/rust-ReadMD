# V6-36 有限切片：既有媒体转写的总时长修正

日期：2026-10-27。状态：本有限缺陷已实现并真实验收；V6-36 选段工作流整体仍未完成。

## 真实问题与用户入口

现有 `speech.rs` 的 System.Speech 脚本将最后一个识别短句的结束位置作为 `duration`。带尾静音的音频因此在生成稿中出现错误总时长。`transcribe.rs` 将该值写入 Markdown，现有单文件转换、批量转换及保存稿都会受到影响。

旧候选 `8f290f47d4dc4ddd4a31bdb9496a82693d1a58e1835189090491358d000ecb00` 的真实首败：合成英语 PCM 共 275440 帧、16000 Hz，即 17.215 秒，其中前置静音 1 秒、尾静音 8 秒；输出却为 `duration: "00:04"`，应按既有整秒显示规则为 `00:17`。

正式 CSP 下的 headless Edge 已通过可见 Open 按钮及真实 file-input 上传该文件，实际 `/api/upload` 与 `/api/convert` 均为 200，并生成含真实识别正文的转换预览。上述时长错误在该入口独立重现，没有添加后端入口或替换识别结果。

## 行为与兼容边界

1. 删除 PowerShell 的短句结束时间 `duration`；识别引擎、短句文本、置信度及 `start` 的来源保持原样。
2. Rust 检查实际交给识别器的 WAV：有 FFmpeg 时检查本次解码完成的 PCM；没有 FFmpeg 时继续使用原 WAV 路径。
3. 对经典 RIFF/WAVE 的固定帧 PCM、IEEE float，以及对应 WAVE_FORMAT_EXTENSIBLE GUID，按 `data` 的完整帧数除以采样率计算全长。包含前后静音和声道交织，不按短句数量或单声道样本数估计。
4. 非 WAV 媒体的时长描述实际解码的音频流，包含解码器裁剪与补齐；不会用更长的视频图像轨时长代替。MP3 和 MP4 的验收以独立输出的裸 PCM 帧数为预期值。
5. RF64、RIFX、未知编码、非字节对齐样本、未确认支持的 float 位宽、流式 `u32::MAX` 长度、wavl/无顶层 data、重复 fmt/data 或元数据块预算耗尽时返回 `None`。继续由原识别器决定是否可识别，只省略无法可靠求得的 duration。既有 `transcribe.rs` 的可选字段合同支持此行为。
6. 对已确认的固定帧 PCM 参数矛盾、普通结构截断、非完整采样帧等损坏情况返回既有解码错误族 `audio_decode_failed: invalid_wave_duration`。未知或非标准容器不会仅因本解析器无法判断而被新增拒绝。

解析器最多遍历 4096 个元数据块，每次最多读取 40 字节格式前缀，通过有界整数计算及 seek 跳过音频数据和未知块，处理 WORD padding。遍历前后及块间检查取消；识别完成后再次检查取消才返回结果。没有读取整段 PCM 到内存的新开销，没有新增依赖。

没有 FFmpeg 的真实兼容对照也已保留：旧候选可识别合法 mu-law WAV；新候选保持真实识别成功，只省略未证明的全长字段。此行为没有把合法 WAV 退化为引擎不可用。

## 文件所有权

- `rust/readmd-kernel/src/speech.rs`：唯一产品源码改动。
- `rust/readmd-kernel/src/speech_duration_tests.rs`：由 speech.rs 自行声明的 9 个专属单元测试。
- `tools/test/v006-media-duration.cjs`：真实服务、合成素材、独立帧数预期、取消进程观察及证据记录。
- `ui-tests/v006-media-duration-native.cjs`：正式 CSP 的 headless Edge 既有入口、保存和重开。
- 本规格文件。

未修改 lib.rs、server.rs、mcp.rs、transcribe.rs、boot、能力包目录或用户界面源码；未新增依赖。Cargo、统一回归及候选构建由协调者串行执行，本切片代理未执行 Cargo、打包、提交或 computer use。

## 验收与证据

最终候选二进制 SHA-256：`61442176f01797c01caed9d9504c8b5b5a85214053ca6a580ad3a5a3976c47ff`。

对应 speech.rs SHA-256：`540c21504bad6ecc9e1e418fae7d230375752eb950efb7c6a9383345c3777e14`；boot SHA-256：`e7d0b2a10d18f73adb6730751ecabe479c3ff18c4054625d33dee509c7ca4a55`。

所有下列报告位于 `.cache/capability-upgrade/v006-media-duration/`，每次独立运行新建目录，旧报告没有被覆盖。

| 证据 | 真实结果 |
| --- | --- |
| `service-3rnyyo/report.json` | 旧候选服务首败，17.215 秒音频错误显示 00:04。 |
| `browser-8jlbme/report.json` | 旧候选既有 Open/上传/转换入口首个产品缺陷断言失败，00:04 对照 00:17；页面无脚本错误。 |
| `service-MZl4nG/report.json` | 同一旧候选另存识别正文及时间戳摘要对照，仍为 00:04。 |
| `cancellation-baseline-jeZBDs/report.json` | 旧候选真实识别子进程取消与回收对照 1/1。 |
| `mulaw-baseline-6W4C56/report.json` | 旧候选在无 FFmpeg 路径识别合法 mu-law WAV 的对照 1/1。 |
| `service-iCjKRg/report.json` | 新候选 WAV 已正确显示 00:17；随后脚本错误要求转换响应含 ok:true 而失败。该接口既有合同为 content/saved 等，修正测试断言后另存新运行。 |
| `service-wDQYIt/report.json` | 最终真实服务 9/9，exit 0。 |
| `browser-yJxPUV/report.json` | 最终正式 CSP + headless Edge 3/3，exit 0；pageErrors 为空，外部浏览器请求数为 0。 |

前置脚本适配失败同样保留：`browser-qfmpkg` 为等待函数触发 CSP EvalError；`browser-9rZAPo` 为未打开文档的等待超时；`browser-ooMabN` 记录测试开启强制鉴权却未提供对应头时上传 403；`browser-Bw1VER` 为错误要求转换输出仍用未转义的 `**[时间戳]**`。随后核对真实保存稿确认，默认转换经过 mdcheck 后保留的是转义星号与括号时间戳，不应把这次严格模式断言失败解释为 profile 隐藏。之后使用普通 evaluate 轮询、内存中的测试鉴权头及真实转换输出合同。没有修改产品 CSP、注入伪识别结果或将这些运行算作成功。

最终服务检查覆盖 PCM 全长、既有转换预览不落盘、MP3、21 秒图像轨但约 17.216 秒音轨的 MP4、损坏输入、真实识别进程取消、显式受保护保存与实际重开、无 FFmpeg 直接 PCM、合法 mu-law 兼容。无 FFmpeg 的压缩输入仍明确返回 `ffmpeg_unavailable`，没有以说明文档冒充成功。

取消检查只观察测试自建内核的实际 powershell 子进程，不收集其他进程命令行。在观察到该子进程后发起取消，确认任务 canceled、没有写出转写稿，并按 PID 与创建时间确认原工作进程已退出。本次从发起取消到任务完成且工作进程被观察为退出约 **849ms**；该值包含任务轮询与退出观察开销，是一次端到端观测。它不是独立测得的 UI 取消反馈延迟或 P95，不能宣称 UI 反馈 ≤200ms 已达标。取消控制路径沿用既有流程。

最终浏览器检查通过可见 Open 和真实 file-input 转写，进入现有编辑器并验证可见的 00:17，再点击可见 Save 按钮下载，保存该真实下载文件并从磁盘重新打开。它验证 headless Edge 和本机内核，不认证原生窗口、扩展宿主或本轮未执行的 MCP 媒体验收。

同一合成 WAV 在旧、新候选中去除 duration 行后的完整正文 SHA-256 均为 `d5874c9f048c4f060ad9d77a80994c4b60d1344ba83d48817f13d8ca05348b5d`，两个可见短句起点均保持旧值 `00:01`。因此有证据证明本例仅改变时长；重复起点本身仍需另行调查，不能据此宣称原始偏移已正确。

协调者已报告最终 Rust 全库回归 **4102 passed / 0 failed / 3 既有 ignored**，包含新增的 9 项时长测试：立体声与静音、单帧/零帧、奇数 padding 与块顺序、扩展 fmt/float、未知容器与编码、超大声明和截断、非整帧、参数矛盾/溢出、预算与重复块、两种取消检查。

复现命令（先由协调者提供已构建候选；两个脚本串行）：

```powershell
$env:READMD_BIN=(Resolve-Path '.cache/community-upgrade/build/debug/readmd.exe').Path
node tools/test/v006-media-duration.cjs
node ui-tests/v006-media-duration-native.cjs
```

脚本仅使用独立配置目录及合成素材；需要本机已安装英语 System.Speech 识别器、英语合成声音和宿主 FFmpeg，浏览器脚本还需已有 Playwright 与 Edge。缺少能力会真实失败，不以跳过或空结果计成功。没有使用用户媒体、麦克风、播放、云模型、私人连接或素材下载。

## 未完成项

- V6-36 的播放/选择入口、时间范围与轨道、原始偏移正确性、SRT/VTT 及其保存重开合同仍待设计和实现；本修复不等于媒体选段完成。
- 当前真实路径依赖 Windows 已安装的 System.Speech 与宿主 FFmpeg。受管 FFmpeg、便携语音运行时、模型、授权/校验与离线交付包没有实现；能力目录中的 speech 仍为 `portable-runtime-and-models-not-packaged`。不能称为受管 Whisper 插件。
- 非 Windows、没有安装识别器及不支持语言的现有失败行为没有改动，本轮也没有实际跨平台验收。
- `transcribe.rs` 生成稿的标题与识别语言提示仍固定中文；非中文界面下按语言或文档要求输出的工作未完成，不能把媒体的 46 语言本地化算作完成。
- 没有 UI 取消反馈 P95/≤200ms、长时压力、原生窗口或全部格式矩阵认证。
