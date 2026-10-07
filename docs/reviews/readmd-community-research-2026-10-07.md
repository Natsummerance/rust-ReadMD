# Markdown 社区对照（2026-10-07）

调查独立编辑器、阅读器、知识工作区、浏览器/终端工具、VS Code 插件和编辑组件。下表均以项目官方仓库或文档为来源；属于文档与源码对照，没有声称在本机运行全部项目。社区项目持续变化，不把有限检索描述为穷尽所有仓库。

## 产品与工具

| 项目与来源 | 重点对照 | ReadMD 的处理 |
| --- | --- | --- |
| [MarkText](https://github.com/marktext/marktext) | 即时预览、专注模式、常用格式操作 | 保留纯源码与预览的明确状态，低频项折叠 |
| [Zettlr](https://www.zettlr.com/features) | 搜索、学术引用、导出 | 对照阅读/学术/转换流程，避免所有入口挤入顶栏 |
| [VS Code Markdown](https://code.visualstudio.com/docs/languages/markdown) | 预览、导航、链接检查、安全级别 | 插件补 CSP、离线资源、重复预览复用、同步滚动 |
| [VNote](https://github.com/vnotex/vnote) | 笔记管理、阅读与编辑分工 | 桌面导航与编辑保持独立，搜索必须作用于当前内容 |
| [Joplin](https://joplinapp.org/help/apps/note_history/) | 历史版本与恢复 | 沿用恢复草稿、修改保护与显式应用结果，避免额外副本污染目录 |
| [ghostwriter](https://ghostwriter.kde.org/) | 专注写作、预览、统计 | 字数继续留在底部，编辑工具栏保持一行 |
| [QOwnNotes](https://github.com/pbek/QOwnNotes) | 本地文本、搜索、预览、折叠 | 修复草稿搜索；替换全部可一次撤销 |
| [ReText](https://github.com/retext-project/retext) | 源码编辑与预览 | 优先完善当前编辑器，不再引入另一套编辑框架 |
| [MacDown](https://github.com/MacDownApp/macdown) | Markdown 编辑和预览 | 阅读主题与代码显示遵从宿主环境 |
| [Remarkable](https://github.com/jamiemcg/Remarkable) | 轻量 Markdown 编辑 | 对照常用操作的收纳，不照搬旧技术栈 |
| [StackEdit](https://github.com/benweet/stackedit) | 浏览器编辑与同步预览 | 预览脚本本地打包，不依赖外部 CDN |
| [Dillinger](https://github.com/joemccann/dillinger) | 浏览器编辑、导入/导出 | 导出传递源目录，保存选择与覆盖语义一致 |
| [HedgeDoc](https://github.com/hedgedoc/hedgedoc) | 协作 Markdown 编辑 | 对照共享流程；不引入远程协作存储来替代本地文件 |
| [CodiMD](https://github.com/hackmdio/codimd) | 编辑与即时预览 | 将实时预览与文件保存区分，保留修改保护 |
| [Markor](https://github.com/gsantner/markor) | 移动端本地 Markdown 与任务列表 | 390px 下检验搜索/替换操作可见，勾选框保持紧凑 |
| [Glow](https://github.com/charmbracelet/glow) | 终端 Markdown 阅读与文件浏览 | 键盘导航、空状态与错误反馈应完整 |
| [Frogmouth](https://github.com/Textualize/frogmouth) | 终端阅读器导航 | 对照打开与返回路径，避免功能入口死路 |
| [Markdown Viewer](https://github.com/simov/markdown-viewer) | 浏览器阅读、权限与主题 | 预览不执行文档脚本，不自动请求远程图片 |
| [Markdown All in One](https://github.com/yzhang-gh/vscode-markdown) | 编辑快捷键、目录、格式化 | 插件工具箱收纳编辑命令，不重复堆顶栏图标 |
| [Markdown Preview Enhanced](https://github.com/shd101wyy/vscode-markdown-preview-enhanced) | 增强预览、图表、演讲 | 逐项检查已提供的图表插入功能能否在预览中呈现 |
| [Foam](https://docs.foam.md/getting-started/get-started-with-vscode/) | VS Code 原生工作区、链接导航 | 尽量使用原生树、Quick Pick 与编辑撤销 |
| [SilverBullet](https://silverbullet.md/features) | 本地 Markdown 知识工作区 | 对照搜索/链接功能，保持文件格式可移植 |
| [Logseq](https://github.com/logseq/logseq) | 大纲与知识管理 | 对照图谱和导航，不改写用户 Markdown 为专属块格式 |
| [TriliumNext](https://github.com/TriliumNext/Trilium) | 层级笔记与恢复 | 对照恢复、状态与导航；不迁移用户目录至专属笔记数据库 |
| [Notesnook](https://github.com/streetwriters/notesnook) | 隐私与笔记工作流 | 不把凭据、未选中的文档内容混入 AI 请求 |
| [SiYuan](https://github.com/siyuan-note/siyuan) | 知识工作区与扩展 | 对照块操作及插件发现，保持 ReadMD 的本地文件定位 |
| [Dendron](https://github.com/dendronhq/dendron) | VS Code 中的层级知识库 | 配置应兼容多根工作区，不默认改第一个目录 |
| [AppFlowy](https://github.com/AppFlowy-IO/AppFlowy) | 文档工作区与 AI 入口 | AI 入口明确，应用结果前保护原有修改 |

## 编辑组件（与完整产品分开）

| 组件与来源 | 对照用途 | 决定 |
| --- | --- | --- |
| [Milkdown](https://github.com/Milkdown/milkdown) | 插件化 Markdown 编辑 | 对照扩展入口，不替换现有 CodeMirror |
| [Vditor](https://github.com/Vanessa219/vditor) | 编辑/即时渲染/预览模式 | 保持当前模式切换清楚，不增加第二套工具栏 |
| [Cherry Markdown](https://github.com/Tencent/cherry-markdown) | 插入工具、预览与扩展 | 对照高级功能收纳 |
| [TOAST UI Editor](https://github.com/nhn/tui.editor) | Markdown 与富文本编辑组件 | 对照工具分组；不增加依赖或双重文档模型 |

## 本轮采用的共同原则

常用操作直接可用，低频功能折叠；状态、取消与错误有真实结果；内容与工具栏分离；预览离线可用；AI 只应用明确选择的内容并保留撤销；转换和导出保留源文件，不产生无意义备份。完整实施与证据见[三端升级检查](readmd-community-upgrade-2026-10-07.md)。

MCP 按[当前规范](https://modelcontextprotocol.io/specification/2026-07-28)补充独立发现与请求元数据，同时保留旧客户端握手。VS Code 按[工作区信任规范](https://code.visualstudio.com/api/extension-guides/workspace-trust)区分安全预览与原生执行能力。未实现的可选协议扩展不宣称支持。
