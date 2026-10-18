# 第三批：核心数学字体与按需出版字体

本次为 V0.0.5 开发增量。主程序未发布、未覆盖用户安装；第三批仍在实施。完成的是数学及中文 PDF 字体的独立交付链路，不是所有宿主依赖、所有 Unicode 或完整跨平台认证。

## 实际行为

小型 STIX Two Math 字体以固定原始字节随 Rust 核心提供，不再从 Windows、macOS 或 Linux 的字体目录寻找 Cambria Math。常见希腊字母、积分、根号及数学字母使用实际数学字形和 MATH 度量。生成 DOCX 不需要 Office、WPS 或 LibreOffice；这些软件仍是输出兼容性验收对象。

中文 PDF 使用 ReadMD 私有字体包，包括黑体、宋体风格的常规与粗体四个固定字重文件。下载 **38.1 MiB**，安装后 **48.5 MiB**。缺包时，真实分页、PDF 导出及预览在当前操作内说明用途、体积与许可证，支持“安装并继续”或离线导入；取消不改变输入文件。没有新增常驻界面或要求用户先去插件面板寻找组件。

缩略与放大预览均显示明确的缺包状态，停止加载动画并禁用旧翻页操作。安装完成后刷新同一预览；文件或设置已经改变的旧回调不能覆盖当前预览。字体安装、移除或损坏改变缓存身份，不能沿用不匹配的旧 PDF。

Typst 使用 `--ignore-system-fonts`，只读取随附字体及经过校验的私有字体。原生 PDF 嵌入实际字形子集，包含真实粗体轮廓和可搜索复制的中文；不是依赖查看器安装 `STSong` 的替代字形。Office 平台字体在 PDF 中映射为公开字体并提示，DOCX 保留原请求的字体名；两者不能宣称为完全相同排版。

字体保存在 ReadMD 配置下的能力目录，不注册到系统、不写入文档目录。安装与每次使用核对完整包及逐文件摘要，最小探测解析真实字形；损坏后修复、卸载保留未知用户文件，已有完整输出不被失败操作覆盖。导出作用域和安装互斥锁共同保护资源生命周期，公式保存实际所用字体对象，避免排版后换用另一套字形。

Windows 安装器现有组件页加入中文字体选项，与 PDFium、Typst、OCR 一起显示用途与真实大小。核心必装，OCR 推荐预选但可取消，其余可选；失败保留基础阅读能力。安装器以真实候选程序和资源编译验证，尚未执行覆盖用户安装、完整升级或卸载流程。

## 来源、体积与离线复现

- 核心原始字体：STIX Two Math v2.13b171，固定提交和 SHA-256，**1,518,116 字节**，附原始 OFL 许可证。
- 中文来源：固定 Google Fonts 提交中的 Noto Sans SC、Noto Serif SC；使用审查锁定的 fontTools 4.63.0 离线生成 400/700 字重，衍生字体改名，保留原版权及许可证。原始字形覆盖未做裁减。
- 输入锁、静态输出锁、包内逐文件锁和整体目录分别核对。变量字体默认细字重不能当作常规/粗体；固定字重确保 Rust 子集器和 Typst 读取相同的真实轮廓。
- 准备字体是开发维护步骤；应用、普通离线构建和能力包装配不调用 Python 或 fontTools。QA 独立检查也不构成产品依赖。
- 通用字体包已经发布到 [tool-ReadMD 固定发布](https://github.com/Natsummerance/tool-ReadMD/releases/tag/packs-2026-10-10)。现有同名包不被静默替换；下一版沿用同一审查、锁定、离线装配与发布工具。

维护命令与当前边界见[离线能力指南](../guides/offline-capabilities.md)。上游依据为 [STIX 官方发行](https://github.com/stipub/stixfonts/releases/tag/v2.13b171)、[固定 Noto 来源](https://github.com/google/fonts/tree/a85815a42757630ce188fdad368c2dfc444d4773)、[fontTools 静态实例接口](https://fonttools.readthedocs.io/en/latest/varLib/instancer.html)及 [OFL 原文](https://openfontlicense.org/open-font-license-official-text/)。

## 验收范围

固定源码、候选程序、前端包与全部证据摘要见[索引](../evidence/capability-upgrade/2026-10-10/batch3-publication-fonts-manifest.json)。使用隔离配置与专用合成研究材料，未读取私人文件或替换用户安装。

- [Windows 原生离线流程](../evidence/capability-upgrade/2026-10-10/batch3-publication-fonts-native.json)和[自有仓库在线流程](../evidence/capability-upgrade/2026-10-10/batch3-publication-fonts-online.json)各覆盖同样六条检查：限定 System32 的 PATH、无字体包的核心输出、缺包取消、安装继续、两引擎输出、损坏修复与卸载、实际重启及目录无污染。两种交付方式不累计为十二个独立功能。
- [独立 PDF 检查](../evidence/capability-upgrade/2026-10-10/batch3-publication-fonts-font-inspection.json)：提取真实嵌入字体、静态常规/粗体轮廓和中文文本；检查矢量公式并栅格化视觉自检。原生 PDF 与 Typst 样张分别归档，不以两个引擎像素相同为目标。
- [完整内核库回归](../evidence/capability-upgrade/2026-10-10/batch3-publication-fonts-rust.json)：2047 通过、0 失败、3 项既有忽略，忽略原因独立保留。
- [相关界面回归](../evidence/capability-upgrade/2026-10-10/batch3-publication-fonts-ui.json)：46 通过、0 失败/跳过/重试。中文书籍 PDF 在缺包状态从放大预览真实上传安装，再检查多页输出；保留原有 EPUB、HTML、PDF 断言。
- [安装器编译](../evidence/capability-upgrade/2026-10-10/batch3-publication-fonts-installer.json)、[资源发布](../evidence/capability-upgrade/2026-10-10/batch3-publication-fonts-delivery.json)与[静态门禁](../evidence/capability-upgrade/2026-10-10/batch3-publication-fonts-static.json)分别记录；46 语言现为 2580 键。键值和布局检查不等于所有语言人工语义审校。

![原生 PDF 的真实嵌入字形](../evidence/capability-upgrade/2026-10-10/batch3-publication-fonts-native-page.png)

![Typst 的私有字体输出](../evidence/capability-upgrade/2026-10-10/batch3-publication-fonts-typst-page.png)

## 发现的问题与剩余工作

过去的字体测试借用了开发机已安装的字体，并在找不到字形时提前返回，无法代表干净安装。本次移除这类宿主成功前提，核心希腊字形测试无条件执行，中文安装正向证据由真实私有包提供。旧中文书籍预览测试在缺包后失败，还暴露了放大预览无限加载的问题，已修复。

真实字体导入超过了原测试五秒默认断言期限；现对大包完整校验设明确的有界安装时限，未删除输出断言或自动重试。早期合成材料还使用了不存在的 `text()` DSL 函数，已改为正式支持的引号文本，不修改编译器来迁就测试。失败记录见[尝试账本](../evidence/capability-upgrade/2026-10-10/batch3-publication-fonts-attempts.json)。

R012、R029、R075 仍进行中：任意缺字逐节点定位、替代字体目录、全部文字体系、实际 Word 往返和非 Windows 原生均未完成。中文包可跨平台读取，不等于所有平台的原生排版已验收。字体最小探测和本次样张不代表任意字符都已检查或完整覆盖。

FFmpeg/whisper.cpp、XeLaTeX/pdfLaTeX 便携包与沙箱，无界面 Vega、PlantUML、Windows WebView2、Linux GTK/WebKit 等交付仍待解决。完整升级回退、干净系统、性能与八小时压力仍保留，不宣称“零依赖”或“所有场景通过”。

本任务缓存按锁定资源、准备输入、测试候选、隔离配置、工具和日志分类，见[清单](../evidence/capability-upgrade/2026-10-10/batch3-publication-fonts-cache-inventory.json)。本次测试进程已结束；此前自动审批拒绝的批量删除未绕过或重试，用户安装、文档及恢复记录未清理。
