# ReadMD V0.0.7 发布与证据范围

2026-11-19。[V0.0.7 正式下载](https://github.com/Natsummerance/rust-ReadMD/releases/tag/V0.0.7)提供十份桌面／集成包及 SHA256SUMS.txt。公开下载流式复核回执核对了全部 11 个文件的实际大小、GitHub SHA-256 digest 与校验清单；本记录由完成后的本地回执生成。

发布源码：`76c7e22921fdfb926c5a0c678d65e0dc666d5e2e`。元数据维护提交会在该构建源码之后；已发布安装包继续归属此源码。

独立匿名公开下载审计：[44569993234](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44569993234)。审计实际读取全部 11 个公开文件，并核对 GitHub API digest、实际字节数及 SHA256SUMS.txt；回执明确绑定本次源码与发行运行。审计脚本 SHA-256：`111837ac31ae59294a9e11487131035b3979e817cc21c8d0455a48f683b11366`。

## CI 与平台

[最终 CI](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44559480050)，attempt 1；四个构建和所有非发布质量任务均为 success。Publish GitHub Release 的回执为 **success**。发布由工作流完成。

| 平台 | 本轮证据 | 原生界面范围 |
| --- | --- | --- |
| Windows x64 | CI 源码构建、安装版／便携包、实际发行二进制 HTTP／headless 生命周期、图片导出与离线 PDFium 检查 | CI 结果不代表完整原生窗口／键鼠验收 |
| Linux x64 | CI 源码构建、DEB／tar.gz、实际发行二进制 HTTP／headless 生命周期、图片导出与离线 PDFium 检查 | 各发行版原生桌面与输入设备单独记录 |
| macOS ARM64 | CI 源码构建与 ZIP／DMG；正式下载 ZIP 冷启动／重开；另保留 244 项候选检查 | 正式 ZIP 启动流程与旧候选六组检查分别绑定摘要 |
| macOS Intel | CI 交叉目标构建与 ZIP／DMG | 本轮不据 ARM64 记录推定实体 Intel 原生交互 |

共享质量门槛涵盖 Windows／Linux／Mac 内核测试、前端、Playwright、VS Code／MCP 和 Windows／Linux 小卷空间消费者。成功任务完整列于下表，构建及门槛成功不提升全部产品功能的整项状态。

| CI 任务 | 结果 |
| --- | --- |
| [Build · Linux x64](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44559480050/job/135684515143) | success |
| [Build · Windows x64](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44559480050/job/135684515613) | success |
| [Release quality / VS Code and MCP quality](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44559480050/job/135684515768) | success |
| [Build · macOS ARM64](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44559480050/job/135684515782) | success |
| [Release quality / Kernel tests · macos-latest](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44559480050/job/135684515787) | success |
| [Release quality / Kernel tests · ubuntu-latest](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44559480050/job/135684515820) | success |
| [Release quality / Frontend gates](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44559480050/job/135684515842) | success |
| [Release quality / UI quality (Playwright)](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44559480050/job/135684515852) | success |
| [Release quality / Kernel tests · windows-latest](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44559480050/job/135684515881) | success |
| [Release quality / native-space-consumers / Owned small volume · ubuntu-latest](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44559480050/job/135684516234) | success |
| [Build · macOS Intel](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44559480050/job/135684516326) | success |
| [Release quality / native-space-consumers / Owned small volume · windows-latest](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44559480050/job/135684516669) | success |
| [Publish GitHub Release](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44559480050/job/135718420544) | success |

### 三平台内核主测试集合

下表各取对应任务日志中的最大 Rust 主测试集合，均为零失败；这是各平台的一组回执，不是全部测试集合累计。ignored 与 filtered 分别列出，未计入通过数。

| 任务 | passed | failed | ignored | filtered |
| --- | ---: | ---: | ---: | ---: |
| [Release quality / Kernel tests · macos-latest](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44559480050/job/135684515787) | 4449 | 0 | 12 | 83 |
| [Release quality / Kernel tests · windows-latest](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44559480050/job/135684515881) | 4525 | 0 | 12 | 82 |
| [Release quality / Kernel tests · ubuntu-latest](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44559480050/job/135684515820) | 4440 | 0 | 13 | 81 |

## 打包首败与回归

[首次运行](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44538228206)，源码 `f63197aeb57bb0ec9695f626aed6bb190de5c067`，Windows 任务在 `Assemble the bundled PDFium capability pack` 失败。保留日志表明 Git Bash 的 GNU tar 将 `D:` 开头的绝对归档路径当作远程主机，报 `Cannot connect to D: resolve failed`。PDFium 改为在归档目录使用相对文件名；Office 归档检查与解包同样采用相对路径及明确工作目录。最终运行的四平台构建与质量门槛重新通过。首轮其他 cancelled／未完成项目不记为通过。

首次失败日志 SHA-256：`fdef68014037115143bcb198b44398b74fb1ca6261a3607cce0410a08b2426cc`。

## ARM64 本机候选

[原生记录](readmd-v007-macos-rc-2026-11-19.md)及[结构化回执](readmd-v007-macos-rc-2026-11-19.json)包含六组共 244 项检查、六格式导出、隔离会话升级、ZIP 冷启动／重开与 46 语言保存编码流程。QA 二进制：`1533ce9c15a8049dbc60f69e615eca8108be63114fddc909e9959027d14aa62e`；本机归档生产二进制：`57b33ec274cb15e5c69f6b7895995cafe1c2cde089847603582f7b5fec6cef79`。五份源资源摘要与本次 public 源码逐一相符。

| 本机候选归档 | 与公开 CI 包的大小及 SHA-256 一致 |
| --- | --- |
| ReadMD-macos-arm64.zip | 否，保留独立候选身份 |
| ReadMD-macos-arm64.dmg | 否，保留独立候选身份 |

保留的本机首败包括一次导出预览等待超时，以及一次 WebKit 内容进程／GPU 辅助进程失败导致的空白窗口。同包、同资源稍后通过冷启动、重开、升级和六格式导出复测；底层系统原因未确定。该本机候选结果不自动构成最终 CI 二进制的原生认证。

## 正式 ARM64 下载包原生复核

本轮从已核对的正式下载 ZIP 提取生产应用，使用正常系统启动完成 cold／reopen 两阶段。两次均实际显示原生文档、正常退出，重开保留会话，原测试文档字节保持；未启用 QA 二进制入口、未请求模型、未强行终止进程。该证据属于本次正式下载包，范围为这两个启动流程。

ZIP SHA-256：`24e8de546509547dc115825de241ab345029f685f6ca1bd3fd08f9b77ac22102`；生产二进制：`fd6ca415d03b253a7d6063c220805c4ce4e251aa4e36e7f6214476c4edc5dab0`；boot：`a472cf0764a2ce7c5c4ac0badef1c60e5d25c7dd6702ffd9b77e9170886038b6`。原生复核回执 SHA-256：`9b18974a8f1ecaee21669cbbeb5e3cdbf134abac66b58203cb46e08c5b80bf3a`。

该复核绑定同一源码与 CI artifact `14552850558`，启动前后签名及封装资源均通过验证。绑定回执 SHA-256：`74569ba5adcad065a6bebb6bb5f43569c91d436d1e94eb32f574883300e60b4b`。

| 阶段 | 原生窗口截图 SHA-256 |
| --- | --- |
| cold | `4a237fe651e474deda760bc01a65aced8930bb8d4a5ee216718b74b742c18bdb` |
| reopen | `af2620720bd364d90179bd1a1b038bdc61e06d31514301a5606a428cfd429630` |

## Windows 首败与修复

[第二次发行构建](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44540087947)暴露出两处独立故障。

- Office 排版组件编译成功后，一份依赖许可证因检出时自动转换换行，从 1023 字节变为 1046 字节，打包因此拒绝通过。锁定文件现保留上游原始字节，继续按原大小和摘要验证。
- 批量转换时，进度读取器持有旧回执。Windows 替换该文件返回错误 5，工作线程退出后，三项输入均显示已中断。此时正在处理第一份输入，重开的步骤尚未开始。新的原子写入允许已共享删除权限的读取器继续读旧内容，并在新回执发布前清除临时文件属性。拒绝共享删除、同步失败及禁止覆盖任务文件的保护继续生效。

Windows 独立运行器复现了[三项中断](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44544232360)及[实际系统错误](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44545605103)。最终候选同时覆盖读取旧回执、保存后的文件属性、失败保留目标及原临时文件名被重新使用。

## SSE 取消与 Office notices 收尾

[后续候选](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44548006880)的 Mac 内核测试暴露了慢速下游写入取消超时；Windows Office 打包另因 zune-core 的独立许可通知缺失而停止。失败日志与中断状态保留，未作为成功计数。

SSE 发送与对端监听改用可取消的非阻塞等待，并在退出时恢复连接模式。测试先观察实际写入背压，再发取消并核对所属写入退出；最终三系统内核门槛重新通过。真实 loopback HTTP 协议检查另覆盖取消、断连、失败和隐私聊天的历史边界，未调用真实模型。该协议回执二进制为 `70c8af3fda544668f0c0d683d36e5ade8edbcebcad9a66f1266360ca2e2737df`，该回执按独立本机候选保留，与最终源码的字节差异文件为 `rust/readmd-kernel/src/server/chat_stream.rs`，不据此将 HTTP 协议结果转为最终 CI 二进制认证。回执 SHA-256 为 `20a710d58e9e5347884a7a0574089a3c86b182797832c8dfdddfe18926531978`。

Office notices 按目标平台实际启用的依赖闭包规划；缺失项先汇总并拒绝打包，全部匹配后才提交目录。补充文件绑定 Cargo.lock crate 校验值和发布源码提交，保留官方原始字节。Windows 的 accesskit_windows 0.34.0、zune-core 0.4.12、zune-jpeg 0.4.21 通知见[固定来源清单](../../tools/office/third-party/notices.lock.json)与[来源说明](../../tools/office/third-party/README.md)。Mac、Linux 和 Windows 的运行时清单及最终构建分别检查。

SSE 首败日志 SHA-256：`48edbd0e8ae246cec28093bf00a4104493c62c666cf35b988c4edd8864349585`；Office 缺失通知首败日志 SHA-256：`cfe9a9f5458e3e6d4b5b98102cd14e54858ad83c07fbb34ae3a67ed450acc542`。

## 公开资产

| 文件 | 字节数 | SHA-256 |
| --- | ---: | --- |
| [ReadMD-linux-x86_64.deb](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-linux-x86_64.deb) | 99217120 | `294cc981b92adb6efb91d224d00c986443b3443e440461eef30f89ad340f160f` |
| [ReadMD-linux-x86_64.tar.gz](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-linux-x86_64.tar.gz) | 124981058 | `7897989ee2f2fede169fa519f7aca3ba9ded44b18f75e4375f664520b899a432` |
| [ReadMD-macos-arm64.dmg](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-macos-arm64.dmg) | 131767467 | `28e954cccd5c91951dc41cc5320c2c1e747f51a1e1ac89bf86d8b05b6eaa7c0e` |
| [ReadMD-macos-arm64.zip](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-macos-arm64.zip) | 114032188 | `24e8de546509547dc115825de241ab345029f685f6ca1bd3fd08f9b77ac22102` |
| [ReadMD-macos-x64.dmg](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-macos-x64.dmg) | 137512607 | `a905dc1273ad32a6655a50314480e3a8667b22dc02c64464503e5aa51ed300a4` |
| [ReadMD-macos-x64.zip](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-macos-x64.zip) | 118848299 | `1de00e6129458354c17272a9dca19c42b4388bd804b61f9611983b6874291a82` |
| [readmd-mcp-server-0.0.7.zip](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/readmd-mcp-server-0.0.7.zip) | 7055 | `eb1dd216c5e616eff9c45c7ae90fe784ef3a90f689b373d3bc000f04823b2595` |
| [readmd-vscode-0.0.7.vsix](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/readmd-vscode-0.0.7.vsix) | 14043728 | `ecb857c9ad6b3154abfc4355339af620e1b1c2b97bd4ba32fb1f3a5a88af25b6` |
| [ReadMD-windows-x64.zip](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-windows-x64.zip) | 118153354 | `11d45895e89363926ba92990788b62bfd60a1ade284c419e94262bf83059e9ff` |
| [ReadMDSetup-windows-x64.exe](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMDSetup-windows-x64.exe) | 111370516 | `1c61d48d62c2311c3caa31548916ff8c4f158a54c80c6320a3259daa80bd1102` |
| [SHA256SUMS.txt](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/SHA256SUMS.txt) | 903 | `84a6e7ee2eb63faab618f96a1a8d1a6c3e29c07900084ae6c943cebee8220b28` |

## 支持范围

Mac 使用免费 ad hoc 签名，没有 Developer ID 或 Apple 公证；首次打开见[安装说明](../../release/macos-install.md)。桌面包随平台提供 Office 阅读运行时及离线 PDFium 包。FFmpeg／Whisper 为独立离线能力包，未包含在本次十份桌面／集成资产中；本轮媒体能力包实际操作覆盖 ARM64，其他平台按可取得资源与具体操作记录。

平台矩阵保留原有候选平台、计划安装格式和功能条目；本版实际下载为上述四个目标，其他条目的计划格式不代表本次提供下载。功能单元格默认仍为 pending-native-evidence，原有 overrides 和完整能力认证规则保持。scoped_release.deferred_version 设为 null，表示未指定后续版本，不再把已发布的 V0.0.7 自身标作延期目标。完整 Office 往返、真实多人长期运行、生产 AI／语言矩阵与物理 IME／压感笔等仍按各自证据处理；本记录没有把全部产品功能记为通过。

## 文档与官网收尾检查

发布说明、平台合同、官网元数据、四语言 AI 页面及历史演示合同共 54 项测试通过。官网完成完整离线构建及 release 校验，覆盖四语言、53 个 canonical 页面与 104 项历史演示清单；public／dist 的下载与 CSP 同步检查通过。仓库及站点分发目录隐私检查、文档本地链接、旧版本发布说明字节保持及差异空白检查均通过。
