# ReadMD 功能演示

最新 [功能清单](../docs/reviews/readmd-ui-function-inventory-2026-10-02.md) 的 104 个功能流程、21 个分类，各对应一个真实操作短片。每段保留点击、面板状态及最终结果；连续模型/网络等待可以加速，剪辑区间记录在清单和独立字幕中。录制器运行真正的 Rust 阅读器、原生桌宠和模型请求，不替换接口响应。

视频仅采集本任务的 ReadMD、角色和原创演示网页 WebView。桌宠画面按真实窗口位置合成；系统文件夹、选择器、资源管理器、打印窗口及其他前台应用不进入录制。应用内的个人路径通过不改变业务数据的遮挡层隐藏，并以本机 OCR 复核视频和封面。`build-gallery.mjs` 拒绝缺少隐私核验或媒体哈希已变化的短片。

录制使用 `immersive-v2`：只保留软件自身画面，关闭底部浮动提示、繁忙浮层及模块状态摘要，不添加编号框、步骤说明、额外光标圆圈或等待加速标签。说明放在外部 VTT 和清单中，MP4 不烧字；网页播放默认关闭字幕，需要时可通过播放器开启。软件的功能面板、保存确认及桌宠对话仍按实际操作显示。隐藏样式仅注入隔离录制页面，每段完成后移除并验证；用户的软件设置和系统通知设置不改动。主页完整主题循环见 F016：深色、护眼、自动、浅色；每次检查实际配色和偏好保存，再演示阅读字号及禅模式。

每段记录实际录制时间、采集方式、当前前端代码 SHA256 和叠加层状态。验收拒绝旧 UI 或旧录制模式混入素材。素材转交宣传视频制作时使用 [PROMO-HANDOFF.md](PROMO-HANDOFF.md) 和 `promo-clips.json`；视频目录保持一份，不另建重复副本。

[逐项演示与保存体验对照](../docs/reviews/readmd-showcase-2026-10-03.md) 记录每段实际操作、核验结果、条件分支和 D01–D56 保存细则。失败、迟到响应、外部修改和写入冲突通过应用回归测试验证；短片不代表所有分支、全部格式和所有环境都已录制。

## 目录

| 路径 | 内容 |
| --- | --- |
| `manifest.json` | 全部功能与原清单 SHA256；实际步骤、结果断言、接口状态、媒体哈希和等待剪辑记录 |
| `promo-clips.json`、`PROMO-HANDOFF.md` | 宣传片素材索引、使用说明；说明和剪辑记录与纯画面 MP4 分离 |
| `materials/` | 原创哲学研究手册、关联笔记、演示稿、PDF、EPUB、HTML、邮件、代码、数据、图像、录音和带 MIT 许可的 Skill ZIP |
| `videos/` | 每项一个 H.264 MP4，编号 F001–F104 |
| `posters/`、`captions/` | 对应真实画面 WebP 与中文 WebVTT |
| `checks/` | 播放、布局、兼容性、验收报告及必要的网页效果截图 |
| `scripts/` | 资料构建、录制、原生窗口控制、等待剪辑、网页生成、校验与本地预览 |
| `.runtime/` | 隔离数据、帧、凭据、WebView 缓存、失败诊断；忽略 Git，完成后清理 |

网站源在 `website/public/`，最终静态网站在 `website/dist/`。本机媒体通过硬链接复用，避免三个目录重复占用空间；发布静态网站时它们仍是普通文件。旧截图拼接、营销海报、v237/v238/v239 管线与逐帧首页资产已移除。

## 离线构建与预览

复用已有 Node、Playwright、Tailwind 和 Rust 缓存，不安装新依赖。只改前端仍需重新生成 `assets/readmd.boot.js`。

```powershell
cargo run --offline --locked --manifest-path rust/Cargo.toml -p xtask -- bundle-boot
node showcase/scripts/build-materials.mjs
node showcase/scripts/build-material-images.mjs
powershell.exe -NoProfile -ExecutionPolicy Bypass -File showcase/scripts/build-material-audio.ps1
node showcase/scripts/finish-privacy.mjs
node showcase/scripts/build-gallery.mjs
node showcase/scripts/build-home.mjs
npm run build --prefix website --offline
node showcase/scripts/verify.mjs
node showcase/scripts/build-promo.mjs
node website/tools/validate-website.mjs --release
node showcase/scripts/serve.mjs 4173
```

打开 `http://127.0.0.1:4173/zh-cn/` 或 `/showcase/`。页面提供分类、搜索、逐项播放、字幕、操作步骤、下载和明暗主题；手机布局也可使用。需要 HTTP 服务，直接双击 HTML 不能加载目录 JSON。

## 重录

`READMD_BIN` 指向已有离线 Rust release；`READMD_UI_NODE_MODULES` 可指向本机已有 Playwright 目录。普通流程使用浏览器，原生选择器、打印、动态授权网页和桌宠使用 Windows WebView2 与真实系统输入。系统窗口只供自动化操作，视频取自自身 WebView；每段通过隐私门禁后才写入正式媒体目录。需本机已有 Windows OCR 语言包。

```powershell
node showcase/scripts/record.mjs F042 --force
node showcase/scripts/record.mjs --force
node showcase/scripts/record.mjs --native --force
node showcase/scripts/compact-waits.mjs
node showcase/scripts/finish-privacy.mjs
node showcase/scripts/build-gallery.mjs
node showcase/scripts/build-home.mjs
npm run build --prefix website --offline
node showcase/scripts/verify.mjs
node showcase/scripts/build-promo.mjs
node showcase/scripts/cleanup-runtime.mjs
```

仅重录个别功能时，可用 `node showcase/scripts/finish-privacy.mjs F016` 复核对应视频和封面；其他媒体必须与既有隐私批准的哈希完全相同，否则拒绝复用并要求复查。

AI 使用 Antigravity Manager 的真实本机反代与 `gemini-3.8-flash-high`。把 `READMD_ANTIGRAVITY_CONFIG` 设为本机 Manager 配置文件路径，录制器仅在内存读取凭据并写入演示专用加密存储；不把密钥放入命令行、日志或文档。用户原 ReadMD 配置不改动。`READMD_DEMO_PORT` 可指定隔离端口；`READMD_DEMO_MANIFEST` 支持独立录制结果，合并后再生成网页和校验。避免同时改写同一 manifest 或同一段视频。

Hermes 哲学 EPUB/PDF 仅复制到 `.runtime` 做本机兼容性测试；报告只保留格式、大小、引擎、字符数和原书未改变断言。私人书籍、凭据和失败片段不发布。公开演示使用 `materials/` 原创内容。

## 验收与清理

`verify.mjs` 对全部短片核对清单、隐私批准、哈希、分辨率、时长、网站文件一致性，并在浏览器逐个解码、定位和加载字幕；检查 1160×820、1024×680、390×844 三种尺寸及两种主题。首页沿用原有 Apple 风格：系统字体、白灰/深色背景、蓝色圆角按钮、玻璃导航、居中大标题与产品大图，仅精修排版和演示入口。隐私复核摘要见 `checks/privacy.json`；OCR 正文与原始帧不发布。应用故障分支和保存保护测试的命令、结果见验收报告。

关闭本次录制器、阅读器和选择器后，删除本任务 `.runtime`、测试临时输出及不用的构建缓存。保留最终网站、短片和可复现资料；保留 Hermes 原书、用户配置、共享依赖和公共 Cargo 缓存。递归删除前验证绝对路径位于本仓库或明确的任务临时目录，并避开目录联接。

`cleanup-runtime.mjs` 仅在全部媒体已使用当前录制模式和 UI 快照完成后清理。它先验证目录归属、每个子项和相关进程已退出，再删除隔离录制与隐私复核的临时目录；遇到未知目录或目录联接会停止。结果写入 `checks/cleanup.json`。
