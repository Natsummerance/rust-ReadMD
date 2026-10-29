# V0.0.6 第三批：转写时间戳呈现修复

状态：**限定修复已实现，真实识别与页面保存重开已通过；媒体选段工作流仍未完成。** 所有材料均为自动生成的合成音频，使用隔离资料目录，不涉及用户文档。

转写器生成的行首时间戳原本是有效的 Markdown 加粗语法，通用修复器却将它当作多余标记转义；预览显示星号文本，保存后的转写稿也会携带错误字符。修复器现在仅保护行首的完整合法时间戳前缀，其后的正文继续执行原有修复。支持分钟、小时及超过 99 小时的记录，保留用户显式转义、行内／围栏代码和既有非法输入行为。此前已经保存的转义稿不会被自动改写。

## 证据

- [真实首败](../../.cache/capability-upgrade/v006-speech-emphasis-20261027/browser-SEQuGj/report.json)：真实 Windows 识别取得正确时间位置，但正式 CSP 页面中的时间戳不是加粗元素。最初的定向测试还有一处普通正文断言预期错误，见 `cargo-timestamp-first-20261027.log`；依据实际行为修正断言后，首次失败原样保留。
- [真实页面及保存重开](../../.cache/capability-upgrade/v006-speech-emphasis-20261027/browser-PmBYC7/report.json)：合成音频经过系统识别、页面打开与转换，时间戳按独立 PCM 帧范围校验；预览及显式保存、重新打开均检查实际加粗元素，原始音频字节不变。该测试采用 headless Edge 和正式 CSP，不能代替原生桌面窗口验收。
- [修复器回归](../../.cache/capability-upgrade/v006-speech-emphasis-20261027/cargo-repair-20261027.log)：相关 Rust 测试 81 项通过，包含 6 项新增的时间戳、正文、转义、代码、无效输入和幂等测试。
- [完整 Rust 库回归](../../.cache/capability-upgrade/v006-speech-emphasis-20261027/full-rust-20261027.log)：4,208 通过、0 失败、7 忽略；[前端与工具回归](../../.cache/capability-upgrade/v006-rename-safe/full-node-20261027-06.log)共 1,565 通过、0 失败、0 跳过。
- 本候选内核 SHA-256 为 `72cd138dea42b72222d5bdb8e86af02e6b24196692a62cb80892640d705cb0e6`，页面 boot SHA-256 为 `09afb2321196d9fa7f1dd9f1022db58f7b30d50edbf7f29624400665296a7dc6`。

## 仍未完成

当前验证使用本机 Windows 系统语音识别及已有媒体工具，不是跨平台受管 Whisper／FFmpeg 离线能力包。音视频起止选段、轨道选择、字幕、原始媒体偏移、长任务续作和其他平台的原生测试仍需独立实现与验收。其他格式的 Markdown 强调兼容性也不能由这个窄前缀推断为全部完成。
