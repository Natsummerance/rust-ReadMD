# 离线媒体运行包的维护组装

`tools/prepare-media-runtime.mjs` 将明确提供的、已逐文件固定的便携输入组装为 `stream-v2` `.readmd-pack`。工具没有下载、包管理器、编译、模型加载、引擎启动或用户安装行为；普通构建不调用它。`build` 与 `verify` 均不更改应用清单，`register` 才会显式编辑指定的应用资产。

应用资产仅信任已显式登记的平台包，未登记平台继续保留 `pending` 状态。组包流程的合成小文件测试不代表原生验收。当前本机 Homebrew FFmpeg 依赖包外动态库，不能复制两个可执行文件就视为便携交付；因此新增的 Mac ARM64 维护构建器从固定官方源码构建无外部库的音频运行包。Whisper 使用独立固定的源码、模型、许可和原生收据，不从宿主模型目录发现或加载输入。

## Mac ARM64 源码构建与真实接口验收

`tools/build-media-ffmpeg-macos.mjs` 是独立、显式调用的离线构建器，不参与普通应用构建。它只接收本地的 FFmpeg 9.0.2 官方源码归档，要求 SHA-256 为 `8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e`。构建器不下载材料、不安装工具、不查找 Homebrew、不加载模型，也不登记或发布包。

```sh
node tools/build-media-ffmpeg-macos.mjs --archive LOCAL/ffmpeg-9.0.2.tar.xz --output NEW_BUILD_DIRECTORY --jobs 1
```

只在原生 macOS ARM64 上运行，使用系统 Clang 和 Command Line Tools SDK，最低部署目标为 macOS 11.0。可用 `--jobs 2`，最大并发为 2；编译以较低优先级执行。临时文件、编译缓存、源码、日志、输入和输出全部位于指定的新目录。

此构建关闭外部库自动探测、网络、GPL/nonfree 组件、共享 FFmpeg 库、硬件加速、视频编解码和 ffplay。保留应用接受的 AAC、ASF、AVI、FLAC、Matroska/WebM、MOV/MP4、MP3、Ogg、WAV 容器与所选原生音频解码器，以及 PCM/AAC/FLAC 编码器、音频滤镜和本地文件/管道协议。它用于音轨探测、抽取、重采样和预览，不是完整功能的 FFmpeg 分发版。每个生成二进制都必须通过既有 Mach-O 架构及系统动态依赖检查。

输出中的 `prepared/` 可继续交给下述 `verify`。新目录还保留精确源码归档、`build-recipe.json`、构建日志、许可和来源说明，供对应源码交付审查。默认两项分发审批均为 `false`，不以编译成功代替登记审查。

使用已构建内核，通过实际应用接口进行隔离验收：

```sh
node tools/test/media-runtime-http.cjs --bin PREBUILT_KERNEL --prepared NEW_BUILD_DIRECTORY/prepared --lock NEW_BUILD_DIRECTORY/inputs.lock.json --evidence NEW_EVIDENCE_DIRECTORY
```

验收脚本复制应用资产并仅在复制目录中加入明确输入的包锁与目录项，启动独立配置，清空宿主工具搜索路径。运行真实分块上传、损坏归档拒绝、安装时音频探测、WAV 区间预览、AAC/MP4 双音轨编码和指定音轨解码；校验 PCM 时长与频率，重启内核验证持久安装，再经正常卸载接口删除受管对象并确认未知备注仍在。它只生成测试音频，不读取用户媒体或请求识别模型。

成功时保存 `native-report.json` 与绑定该文件摘要的 `native-receipt.json`，可供维护者审阅后用于登记。隔离资产、配置、测试音频和日志都留在证据目录；它们不会修改真实应用目录或用户配置。脚本通过表示该具体引擎包在该原生环境的接口闭环通过，不表示 Whisper 识别或其他平台通过。

Whisper 的配套维护构建器为 `tools/build-media-whisper-macos.mjs`。固定源码提交 `d1be6fde11ac6e0407606b4e42fe72d34add8037` 的归档 SHA-256 为 `645445da884df6dbd52b30f82d7be7c01d1767c938714e477e786b88d529e9f4`。它要求显式提供现有 CMake、锁定的 59.7 MB 模型和 OpenAI 模型许可；全部从本地输入读取，不会获取材料或在构建过程中加载模型。

```sh
node tools/build-media-whisper-macos.mjs --archive LOCAL/whisper-d1be6fde11ac6e0407606b4e42fe72d34add8037.tar.gz --model LOCAL/ggml-base-q5_1.bin --model-license LOCAL/openai-whisper-LICENSE --cmake EXISTING_CMAKE_BINARY --output NEW_WHISPER_DIRECTORY --jobs 1
```

该配置关闭共享库、动态后端、Metal、CUDA、Vulkan、CoreML、系统 GGML、OpenMP、CURL 和依赖下载，只构建静态 CPU `whisper-cli`。最终二进制必须只有系统动态依赖且不得残留私有源码路径。完整对应源码、编译配置、MIT/MIT-0 许可和模型固定信息随输出保留；分发审批仍默认关闭。

`tools/test/whisper-runtime-http.cjs` 接收 `--bin`、`--prepared`、`--lock`、`--ffmpeg-prepared`、`--ffmpeg-lock`、`--fixture` 和 `--evidence` 七项明确输入。`--fixture` 只接受固定源码中的官方 `samples/jfk.wav`，SHA-256 为 `59dfb9a4acb36fe2a2affc14bacbee2920ff435cb13cc314a08c13f66ba7860e`。它经受管入口安装两个包，运行短静音、3 秒真实语音和完整样本识别，检查实际文字、时间范围、VTT、跨进程读取及卸载，并记录所属 Whisper 进程的 RSS；超出 1 GiB 测试预算即失败。它只加载本包的小型 ASR 模型，不访问正在运行的本地 LLM。

锁定模型对 250 毫秒静音可能返回一个带 10 秒填充上下文时间戳的 `[BLANK_AUDIO]` 标记。运行时仅将这个精确无语音标记恢复为空结果，短静音选段保持既有的 `speech_no_result` 语义；真实文字的越界时间戳、混入该标记的文字与无关起点仍被拒绝。安装探针继续使用真实的 250 毫秒 PCM，没有扩大探测时长来掩盖短输入问题。

## 输入锁

维护者独立审查上游二进制、对应源码提供方式、编译配置、依赖许可及模型许可，准备一个**不包含私有路径**的 JSON 输入锁。工具不会通过扫描输入目录生成或授权锁，不会以新摘要修正失败的文件。

```json
{
  "schema": 1,
  "id": "media-ffmpeg",
  "version": "9.0.2",
  "platform": "macos-arm64",
  "files": [
    {"path":"bin/ffmpeg","input":"ffmpeg","size":123,"sha256":"<reviewed SHA-256>","executable":true},
    {"path":"bin/ffprobe","input":"ffprobe","size":123,"sha256":"<reviewed SHA-256>","executable":true},
    {"path":"LICENSE","input":"LICENSE","size":123,"sha256":"<reviewed SHA-256>","executable":false},
    {"path":"SOURCE","input":"SOURCE","size":123,"sha256":"<reviewed SHA-256>","executable":false}
  ],
  "review": {
    "license": "<actual build license and third-party notices>",
    "minimumPlatform": "<reviewed minimum OS>",
    "portable": true,
    "redistributionApproved": false,
    "sourceDeliveryReviewed": false,
    "sources": [{"component":"FFmpeg","url":"https://<immutable reviewed source archive>","sha256":"<reviewed source archive SHA-256>"}],
    "buildConfiguration": ["<actual compiler, version and complete configure flags>"]
  }
}
```

这是格式说明，含占位符，不能执行。实际 `size` 和 `sha256` 必须来自维护者已经审查的固定材料。`SOURCE` 说明**该具体构建**的完整对应源码获取/提供方式；字段和文件存在不等于工具已经完成法律审查。GPL、LGPL、启用的外部库和静态链接方式都必须按实际构建分别审核；不能仅按 FFmpeg 项目名称写一个许可证。使用非公开输入进行 QA 时保留两个审批字段为 `false`。

支持 `macos-arm64`、`macos-x64`、`linux-arm64`、`linux-x64`、`windows-arm64`、`windows-x64` 的**格式检查**，不表示六平台已构建或验收。macOS 要求与目标一致的单架构 Mach-O，只允许系统路径下的共享库；Linux 当前要求无外部解释器/共享依赖的 ELF；Windows 要求匹配架构的 PE，仅允许工具白名单内的系统 DLL，禁止包外 MSVC/OpenMP 等运行库。这个保守检查可能拒绝其他本来有效的构建；应审查后改代码/测试，不能删除依赖文件来绕过。原生安装操作仍是最终运行验证。

`whisper` 固定版本为 `1.9.5`，二进制为 `bin/whisper-cli`（Windows 为 `.exe`）。另外必须包含 `licenses/WHISPER-MODEL.txt` 和 `models/ggml-base-q5_1.bin`：59,707,625 字节，SHA-256 `422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898`。模型来源绑定运行时同一上游提交 `5359861c739e955e79d9a303bcbc70fb988958b1`；工具不会获取模型。改变模型或版本必须连同 Rust 运行时一起单独审查。

清单最多 32 文件，除固定二进制/模型，只接受 `LICENSE`、`NOTICE`、`SOURCE` 和 `licenses/`。所有输入使用明确的相对路径，不追随符号链接，不收集目录中的额外文件、DLL、dylib、脚本或凭据。

## 组包与校验

```sh
node tools/prepare-media-runtime.mjs build --lock REVIEWED_INPUTS.json --inputs LOCAL_INPUT_DIR --output NEW_PREPARED_DIR
node tools/prepare-media-runtime.mjs verify --lock REVIEWED_INPUTS.json --prepared NEW_PREPARED_DIR
```

首次写出前校验全部输入摘要和可执行文件架构/依赖，再复制到新目录；输入目录、已有输出及应用锁不覆盖。输出包含运行树、原始输入锁、应用 `*.lock.json`、最终包和 `prepared.json`。中断后的目录保留供检查，必须使用新目录重新准备，不能把残留目录视为成功。

归档使用既有有界两遍流式构建器，固定 gzip OS 标记为 255，并将实际 Node/zlib 版本写入清单。相同输入与工具链可以得到相同包；任意工具链的字节相同不做保证。独立 `verify` 不信任收据中的新总摘要：它将原始输入锁与显式可信锁比较，再检查归档头、每个压缩段展开后的长度/摘要和末尾，流式读取以免把模型/整个包载入内存。

## 原生验收后登记

先在隔离资产/配置中使用真实应用的能力安装接口导入该包，保留完整安装探测、实际媒体处理和卸载原始证据。Whisper 需要真实模型推理；仅运行 `--version`、检查 JSON 或头部不是验收。每个平台分别产生维护者审查的原生收据，至少包含：

```json
{
  "schema":1,
  "id":"media-ffmpeg",
  "version":"9.0.2",
  "platform":"macos-arm64",
  "sha256":"<exact pack SHA-256>",
  "manifestSha256":"<exact compact application manifest SHA-256>",
  "install":true,
  "engineOperation":true,
  "uninstall":true,
  "synthetic":false,
  "evidenceFile":"native-report.json",
  "evidenceSha256":"<SHA-256 of retained native-report.json bytes>"
}
```

证据文件相对于收据目录，必须是普通文件且与摘要一致。此处验证证据绑定关系，原生收据中的结果仍需维护者实际审阅；工具没有代替原生环境执行。确认输入锁中两项分发审批通过后运行：

```sh
node tools/prepare-media-runtime.mjs register --lock REVIEWED_INPUTS.json --prepared NEW_PREPARED_DIR --assets REVIEWED_APPLICATION_ASSETS --native-receipt REVIEWED_NATIVE.json
```

每次只登记实际验收的平台。登记保留其他平台/Node 资产、正式在线能力包、未完成平台的 `pending` 状态。已有相同版本锁和目录身份不可静默替换；摘要不符必须审查新版本，不能重写旧权威文件。

登记结果仍为 `offline-only`，没有下载 URL。分发时交付该明确命名的 `.readmd-pack`、SHA-256、适用平台、许可和对应源码材料；应用仅从显式离线导入接收它。现有 `publish-capability-packs.mjs` 发布器只处理正式 `packs`，不因登记离线包自动上传它。本次没有执行发布、修改远程仓库或安装用户资源。

`prepare-capability-catalog.mjs` 重建原有四个在线条目时保留离线条目和待办，防止一次旧式重建丢掉独立审查的媒体或 Node 元数据。
