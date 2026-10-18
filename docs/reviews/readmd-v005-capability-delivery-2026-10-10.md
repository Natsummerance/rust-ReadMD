# 能力按需安装、预装与独立运行依赖

这是 V0.0.5 开发增量。主程序及当前用户安装未被替换；只发布了自有能力包仓库。新增 R075，连同原 R003/R004/R005/R006/R071/R073 追踪；75 项总体为 58 进行中、17 待办、0 整项完整验收。第三批和全项目尚未结项。

## 已实现

PDF 原位编辑、Typst 学术 PDF、图片 OCR 三个入口在能力缺失时直接提示安装，显示用途、下载量、安装后空间、版本及许可证。安装成功继续原操作；支持真实下载进度、取消、失败重试、同窗口离线导入。插件面板复用同一服务，通用导入根据锁定 manifest 识别包；功能专用入口拒绝其他类型的包。

Windows 安装器新增组件页，OCR 推荐预选且可取消，PDFium/Typst 可选，核心必装。安装器使用相同 CLI 服务，失败不破坏基础阅读，允许之后重试。目录检查脚本防止显示尺寸与包脱节。

已建立公开 [Natsummerance/tool-ReadMD](https://github.com/Natsummerance/tool-ReadMD)，发布 `packs-2026-10-10`：

| 包 | 下载 | 安装后 | 实际验证 |
| --- | ---: | ---: | --- |
| PDFium 7881 | 17.3 MiB | 13.0 MiB | 页面渲染/对象最小操作、原生 PDF 打开 |
| Typst 0.15.1 | 29.9 MiB | 50.1 MiB | 实际排版 PDF、预览导出及子进程运行 |
| OCR 5.5.3-r1 | 9.4 MiB | 12.7 MiB | 简中/繁中/英文模型实际识别探测 |

远端三个资产已匿名核对长度及发布摘要。软件目录固定版本、平台、包摘要和逐文件摘要，只接受自有仓库和可信 GitHub 下载 CDN；不以第三方仓库的最新发布作为运行时入口。来源代码、模型和许可证仍保留真实上游归属。仓库构建配方须在匹配的主仓库源码中执行，不能称其独立于 ReadMD 源码即可构建。

发布工具已从临时脚本整理为 `tools/publish-capability-packs.mjs`，默认离线预检，只有 `--publish` 才访问自有仓库。它逐项检查普通/gzip 载荷、文件锁和现有发布摘要，拒绝替换不可变资产。目录生成支持显式新 release tag。7 项维护工具测试覆盖篡改、额外/缺失载荷、解压上限、任意仓库/地址和已发布包不匹配；三份真实包的离线预检及既有 release 维护另有 [补充证据](../evidence/capability-upgrade/2026-10-10/batch3-capability-delivery-maintenance.json)。这里只更新能力仓库的配方、说明及许可证，不发布主程序或上传视频。

## 依赖调整

OCR 使用离线静态构建的 Tesseract 5.5.3 / Leptonica 1.87.0，带三个锁定模型。Rust 自带 PNG/JPEG/BMP/GIF/TIFF/WebP 解码，将有界输入送入 PNM 工作进程；不调用 Office、系统 OCR 语言包、Python、系统 Tesseract 或系统 PDF 栅格器。扫描 PDF 通过管理的 PDFium 获取图像。独立识别进程最多并发 2、单次 90 秒；输入、解码像素和输出都受限。未知/损坏图片返回失败，不能把空输出当成功。

桌面 Vega/Vega-Lite 改为本地浏览器引擎加官方安全解释器，不需要系统 Node，不放宽正式 CSP。导出快照与原始源码及参数绑定。无界面 MCP Vega 和 PlantUML Java 依赖仍在剩余项中。

依赖检查发现核心、PDF 工作进程及 Typst 还会加载系统 VC 运行库，仅限制 PATH 无法发现这一点。两个经 Microsoft 签名及摘要校验的 VC DLL 现在随核心提供，共 228,728 字节。离线复制脚本接入正式打包与安装器，主程序仅在两文件存在时设置自己的 DLL 搜索目录。真实进程模块检查确认主程序两 DLL、Typst 工作进程 DLL 均来自软件目录，并完成 15,000 段 PDF。没有执行全局 VC 安装器、修改 PATH 或写系统目录。

基础 DOCX 生成使用 Rust，与本机 Word/WPS 是否能启动无关；实际 Word 往返仍是单独的兼容性验收。WebView2、GTK/WebKit、补充字体、语音及 TeX 等尚未全部独立交付，不能宣称当前软件已经清除了全部宿主依赖。

## 实测与证据

当前 Windows 测试使用独立配置、正式 CSP 和真实文件服务；未改用户现有配置/文件。当前构建及前端摘要在 [证据索引](../evidence/capability-upgrade/2026-10-10/batch3-capability-delivery-manifest.json) 固定。

- [安装入口 10 项](../evidence/capability-upgrade/2026-10-10/batch3-capability-delivery-native.json)：真实包、错包拒绝、插件通用导入、重启、真实在线下载和取消。中文/德文/阿拉伯文分别检查 1024×680 与 1160×820，44px、焦点、RTL 与主题。截图为动画完成后的实际页面。
- [OCR 8 项](../evidence/capability-upgrade/2026-10-10/batch3-capability-delivery-ocr-native.json)：六种图片格式中英文识别、扫描 PDF 两页、保存重开、非法输入、模型损坏修复、取消及重启。取消确认 6ms，工作完成 804ms，二者分开报告；不是全部 OCR 质量或手写验收。
- [真实网络失败 3 项](../evidence/capability-upgrade/2026-10-10/batch3-capability-delivery-network-native.json)：不可达代理、显式重试再次失败、同窗口离线导入后恢复 PDF。没有拦截/模拟 HTTP，也不保证任何网络都能下载。
- [随包运行库 3 项](../evidence/capability-upgrade/2026-10-10/batch3-capability-delivery-runtime-native.json)：实际加载路径及长文真实输出；不能替代干净虚拟机或其他平台测试。
- [安装命令 7 项](../evidence/capability-upgrade/2026-10-10/batch3-capability-delivery-cli-native.json)：三个包真实在线安装/探测、重复安装保持不变、非法 ID 拒绝。
- [安装器编译](../evidence/capability-upgrade/2026-10-10/batch3-capability-delivery-installer.json)：实际主程序、桌宠与资源输入；未执行用户安装/注册表变更，不冒充完整升级测试。
- [完整 Rust 库回归](../evidence/capability-upgrade/2026-10-10/batch3-capability-delivery-rust.json)：2024 通过、0 失败、3 项既有忽略，257.98 秒；忽略理由逐项保留。随后 main 的 DLL 搜索变更另有实际原生加载检查。
- [正式 CSP 界面](../evidence/capability-upgrade/2026-10-10/batch3-capability-delivery-ui.json)：壳层与科研 9 项通过、0 跳过/重试；[开发工具](../evidence/capability-upgrade/2026-10-10/batch3-capability-delivery-tools.json) 15 项通过。各套有重叠，不相加当成功能数。
- [自有仓库发布回执](../evidence/capability-upgrade/2026-10-10/batch3-capability-delivery-public-release.json)、[依赖与许可记录](../evidence/capability-upgrade/2026-10-10/batch3-capability-delivery-dependency-audit.json)、[失败尝试](../evidence/capability-upgrade/2026-10-10/batch3-capability-delivery-attempts.json)。

早期原生测试存在 CSP 不兼容等待、文件选择夹具、错误内容选择器和未授权保存路径问题，已修正测试使用真实入口，不放宽生产权限。产品确有主题过渡快照覆盖弹窗的问题，已在弹窗打开时取消过渡；网络错误也统一映射本地化代码。中途动画帧造成的视觉重影与产品过渡问题分别记录。成功的后续修正运行不抹除早期失败。

24 个新增键同步到 46 语言并验证占位符，含 22 个能力交付及 2 个 OCR 词条。`i18n-add` 使用原子写入，解决 Windows 已映射语言文件的更新错误。这不等于 46 语言全部人工语义认证。

![中文窄窗口安装](../evidence/capability-upgrade/2026-10-10/batch3-capability-delivery-install-1024-zh-CN.png)

## 尚未完成与执行顺序

1. 补齐 FFmpeg/whisper.cpp、传统 TeX 和字体的受管理离线包，继续去除无界面图表及 GUI 宿主的外部安装依赖；每项先锁定许可/平台/体积，再进行实际探测及入口安装。
2. 完成第三批剩余 OMML/DSL 标准语义、完整出版参数、真实 Word 往返及 LaTeX 模板沙箱；继续沿用本次统一能力交付。
3. 执行完整安装升级/回退、磁盘/锁文件及长时故障测试；其他平台取得原生环境前保持未验收。最终发布与宣传按完整项目门槛执行。

本次缓存按来源工具、复用离线包、隔离原生测试、NSIS 编译、运行库暂存及共享构建分类，实际占用见 [缓存清单](../evidence/capability-upgrade/2026-10-10/batch3-capability-delivery-cache-inventory.json)。这些是开发/测试占用，不是软件安装大小。

此前自动审批拒绝批量删除隔离测试/缓存目录；本次没有绕过该拒绝，缓存保留并分类列出。用户文档、现有安装、草稿和恢复记录未清理。

操作与维护方法见 [能力包指南](../guides/offline-capabilities.md)。
