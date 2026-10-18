# ReadMD HarmonyOS / OpenHarmony · V0.0.5 原生接入候选

这是 ArkUI/ArkWeb 原生宿主，复用 Rust 内核；当前尚未完成 OHOS SDK 构建、签名或设备验收，不提供未经验证的 HAP，也不属于已验证正式平台。

架构：Rust mobile-ffi 导出 start / stop / free C ABI；系统 N-API 的 C++ 薄层加载 Rust .so；ArkTS 启动绑定 127.0.0.1 随机端口的同一个 HTTP 内核；ArkWeb 加载真实内核首页。前端渲染、AI、保存保护、文档分析、导入/导出继续使用共享内核，不再加载一个没有后端的 rawfile 首页。原生选择器将外部文档导入私有 documents 区，避免无提示覆盖；另存为经系统选择器。剪贴板仅提供纯文本。

离线构建：准备已有 DevEco / OpenHarmony SDK、Rust OHOS std 和签名环境。设置 OHOS_NDK_HOME 及目标链接器后运行 scripts/build-rust.ps1，再用 DevEco 构建 entry。脚本使用 --offline --locked；不执行 npm install、下载 Rust target 或添加依赖。系统 N-API 是 SDK 能力，唯一新增的模块引用是本工程自己的 libreadmd_bridge.so。

node scripts/sync-web-assets.mjs --check 只校验资源及内容哈希，不生成资源。正式同步会将 assets 放入 rawfile/kernel-assets，生成 kernel-manifest.json。此生成目录和 entry/libs 不应提交。SDK 构建前应先生成二进制与资源；缺少任何一步应停止，而不是发布源码压缩包充当 HAP。

本机已验证 Rust ABI 输入拒绝、嵌入 HTTP 启动、访问、重复启动拒绝、停止、内存归还。ArkTS、C++ SDK 编译、外部 URI 权限、多窗口、软键盘、前后台切换和真机离线/联网流程仍需设备验收。桌面托盘、桌宠、原生 OCR 等功能不会因共有 Rust 代码而自动变成已验证的鸿蒙能力。
