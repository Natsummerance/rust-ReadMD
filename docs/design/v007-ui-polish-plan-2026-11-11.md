# V0.0.7 界面与交互收尾计划

从 `c5f8d80d` 延续现有桌面、会话和文档服务。复用[顶栏与侧栏规格](v007-shell-navigation-spec-2026-11-05.md)的保存、关闭、键盘及窄屏验收。完成后按实际通过的候选补记结果。V0.0.7 仍按 P1、P2 的完整验收发布。

## GitHub 源码对照

- VS Code 的 `createContentArea` 把窗控留白与可操作工具分开，`updateOverflowingToolBars` 依据内容溢出调整工具组，侧栏隐藏通过工作区布局更新。ReadMD 使用单顶栏和横向页签；可收起的侧栏由常驻图标切换视图，“更多”与命令面板保留低频操作。 [顶栏实现](https://github.com/microsoft/vscode/blob/d2385e54099e74789556a3ab76ec6885b34b407d/src/vs/sessions/browser/parts/titlebarPart.ts) · [工作区布局](https://github.com/microsoft/vscode/blob/d2385e54099e74789556a3ab76ec6885b34b407d/src/vs/sessions/browser/workbench.ts)
- VS Code 的 `ContextView` 按触发元素定位并归还焦点。Floating UI 的 `size` 与 `shift` 分别限制可用尺寸和位置。编辑栏复用同一个「视图」菜单和现有事件，按正文列与窗口边界计算宽高，容纳 RTL 和长译文。 [VS Code 弹层实现](https://github.com/microsoft/vscode/blob/d2385e54099e74789556a3ab76ec6885b34b407d/src/vs/base/browser/ui/contextview/contextview.ts#L553) · [尺寸约束](https://github.com/floating-ui/floating-ui/blob/master/packages/core/src/middleware/size.ts#L54) · [位置约束](https://github.com/floating-ui/floating-ui/blob/master/packages/core/src/middleware/shift.ts#L55)
- BongoCat 以已解析语言生成菜单文本，macOS 在当前面板的局部事件路径处理右键并交给应用菜单所有者。ReadMD 复用现有桌宠 IPC 与角色模型，先核对预览的真实像素和右键事件是否到达，再按故障层修复。 [菜单文本](https://github.com/ayangweb/BongoCat/blob/b82b4750b776df1408db07d5858d37db1594c3f3/crates/bongocat-app/src/system_menu.rs) · [右键事件](https://github.com/ayangweb/BongoCat/blob/b82b4750b776df1408db07d5858d37db1594c3f3/crates/bongocat-overlay/src/macos/resize_monitor.rs)
- markdown-it 以标点和空白判断定界符可开闭，再按上下文配对。ReadMD 在 Rust 的现有修复流程中检查语法上下文，优先保护合法的标记、代码、链接、货币和公式。 [扫描实现](https://github.com/markdown-it/markdown-it/blob/14.1.0/lib/rules_inline/state_inline.mjs#L77-L109) · [强调配对](https://github.com/markdown-it/markdown-it/blob/14.1.0/lib/rules_inline/emphasis.mjs)

## 实现顺序与验收

1. 先修 Mac 菜单键名和语言切换、BongoCat 预览与桌宠右键。验证菜单首次启动和换语言、正确目标的右键、穿透及关闭。桌宠的互动和个人偏好复用已有动作及设置写入，能从工作台直接测试。
2. 收拢顶栏与侧栏，文件名放在横向页签，保留平台窗控，应用标志留在主页。常用入口使用带提示的图标。优化插件列表与自动修复弹窗，复用弹窗的标题、滚动区域、页脚和焦点管理。验收窄宽窗口、长译文、RTL、浅深色、编辑工具栏及文档保存、撤销、关闭和重开。
3. 阅读设置提供可恢复默认的标题和加粗配色，复用主题色和用户设置。自动修复继续由 Rust 生成诊断与候选，在明确操作时应用。验证未闭合 `**`、`*`、`$` 的正例，以及代码、转义、价格、列表、合法配对和多行正文的反例。更新内容只经过现有修订和持久化路径。

样式复用 `tokens.css`。顶栏与侧栏由 `shell.css` 管理，插件和修复弹窗由 `dialogs.css` 管理。阅读样式和桌宠仍分别由各自的组件维护。按组件验收，业务状态继续由原服务管理。Windows、Linux 与 macOS 分别记录实际运行的验证。

## 本批实现与证据

- 方案 A 已接入真实界面。顶栏显示侧栏开关、编辑、禅模式、横向页签和右端 AI。侧栏上方切换文件、目录、最近打开、搜索与更多，下方保留更新、导出、主题、字体、命令和主页。文档数据在展开的侧栏底部显示一行。Mac 左端使用系统红黄绿按钮，Windows 和 Linux 的界面把窗控放在 AI 右侧。编辑工具保持单行，「视图」固定在编辑栏，「保存」放在最右端。
- `editor/toolbar.js` 负责编辑栏的溢出、弹层定位和键盘导航，`editor/preview.js` 负责预览设置及状态同步。正常拖选或键盘选取仍可用，轻微误触和拖动期间收起选区工具。六种语言的 24 项浏览器测试和 27 组真实界面检查通过。首次检查发现复选框旧样式造成 1 像素偏移，统一尺寸后复测通过。
- Mac 本机安装候选的启动资源摘要前缀为 `8111fcd6aaf6`。专用 AppKit 与 WKWebView 通过 21 项检查，含三种窗口尺寸、预览菜单定位、原生红色关闭按钮、保存退出及 CRLF 写入、中文／英文／阿拉伯文的系统菜单。签名验证及隔离配置下的冷启动通过，随后替换本机应用并保留上版备份。
- 安装后的源码把确认、选择、保存冲突和全卷确认弹窗合用 `dialogs.css` 的边距、正文滚动和操作栏。`ReadMDModal` 统一弹窗层级、Esc 和焦点返回。实际服务的布局检查覆盖 1024×680、640×440、390×360 及中文、德文和阿拉伯文。长文件名曾将确认按钮挤出窗口，正文滚动后通过复测。导出与连接设置在同样尺寸下可达。
- 安装后源码的启动资源摘要前缀为 `e957432cb80b`。56 项 Chromium 回归通过，含保存、撤销、草稿恢复、外部写入冲突、演示加载与退出、插件和桌宠工作台。两条旧测试更新了启动等待条件。桌宠角色选择的实际竞态由旧状态覆盖筛选值引起，统一状态更新后通过复测。另有 26 项 Node 定向检查及 46 语言键名、占位符检查通过。

- 修复弹窗在真实分析服务上通过 5 组长列表／空状态、浅深色和 RTL 布局检查。退出后焦点回到「更多」内实际打开弹窗的按钮。当前源码还通过真实文档的定界符修复、标题与加粗配色持久化，以及原文件 BOM、CRLF 和正文逐字节核对。

以下为此前各候选的限定证据，不合并为本次候选的全量结果。
- 插件列表在矮窗保留滚动空间，修复报告的说明、诊断和操作各有稳定位置。四组真实浏览器布局验证了搜索、分类、长报告和关闭后的焦点。修复详情的 Rust 结构化记录包含行号与处理内容，现有接口的文本记录继续供原消费者使用。
- Mac 原生菜单使用同一份标签映射。词库就绪和切换语言后更新现有菜单项，原生端拒绝空白和裸键名。46 个语言包通过键名和占位符检查，42 个语言包补译了检查报告的 11 个旧词条。语言切换的三项真实浏览器竞态测试通过。少数语种仍需母语审校。
- 阅读设置可分别选择标题、加粗的默认、蓝色、绿色或紫色。真实后端保存设置后，浏览器重载恢复颜色。Markdown 阅读器确认可以闭合的 `**`、`*` 和行内 `$` 在真实服务和渲染器通过，原件 BOM、CRLF 和正文在打开及退出编辑后逐字节相同。跨行有效公式、代码、转义和货币纳入反例。遇到含义不明确的符号时保留原文，转换器及 MCP 使用原修复入口。
- BongoCat 共用原模型的透明封面。工作台可对当前角色预览问候和动作。桌宠 Rust 测试 63 项、Chromium 渲染器和工作台 7 项通过。另用专用 AppKit 窗口和本机包内渲染资源测试右键、Control＋点按、透明边缘不弹菜单及父进程退出后回收。此验证由测试进程发送自己的窗口事件，真实鼠标的系统级穿透仍待确认。
- Node 回归 2,038 项通过、3 项既有跳过。Rust 文档、修复和设置定向回归 116 项通过、3 项既有忽略。Mac 主程序离线编译通过。公式编辑与历史保护涉及的 `settings.js` 和 `panels.css` 恢复为基线精确字节，相关回归通过。

## 后续顺序

1. 在实体 Mac 上补桌宠鼠标穿透、菜单项执行和菜单栏点击。Windows、Linux 单独验收实际窗控、预览菜单及桌宠窗口焦点。
2. 按使用频次复查尚未覆盖的弹窗，补输入法、长译文和少数语种的语义审校。桌宠继续补角色偏好在真实桌面进程退出和重开后的持久化证据。
3. 按现有 P1、P2 计划推进。V0.0.7 的发布门槛仍以各平台实际证据和完整范围验收为准。

## 桌宠、编辑器与设置收尾

- BongoCat 图像本身有透明通道。实际界面的灰黑层来自预览舞台对整个图像添加的投影。移除投影与舞台底色，角色库共用一组可搜索卡片，导入、删除、收藏及 Live2D 选择均保留。运行位置移入设置页并改为两项单选。桌宠显示、运行时安装和角色状态继续经过原服务。卡片仅为进入视口的角色加载预览。
- 编辑工具栏保留常用格式，去掉重复的删除线、行内代码和链接入口。插入块与其他插入命令共用菜单，预览方向改为分栏图标。工具栏、预览设置和正文事务分别由原有模块管理。参考 BlockNote 将操作、图标、选中态和快捷提示放在同一个控件的实现。 [格式工具栏](https://github.com/TypeCellOS/BlockNote/blob/main/packages/react/src/components/FormattingToolbar/FormattingToolbar.tsx) · [格式按钮](https://github.com/TypeCellOS/BlockNote/blob/main/packages/react/src/components/FormattingToolbar/DefaultButtons/BasicTextStyleButton.tsx)
- 阅读设置的标题和加粗配色改为可键盘操作的色点。样式设置将 HTML Head 和 AI 排版折叠，导出设置在窄窗中统一滚动。AI 历史的更名、删除和清空，以及连接删除使用统一的应用内弹窗。新建连接展开高级参数并把焦点放到名称。设置按控制类型选择表单控件，借鉴 VS Code 已有行模板的做法。 [设置行实现](https://github.com/microsoft/vscode/blob/d2385e54099e74789556a3ab76ec6885b34b407d/src/vs/workbench/contrib/preferences/browser/settingsTree.ts)
- 旧版复现了空白公式草稿缺少卡片预览、窄窗宽表格缺少键盘滚动状态。公式卡片改为直接复用本地 MathJax 加载，公式目录转换等待真实结果并保护当前草稿。表格随实际宽度重新更新焦点状态。另修正了旧样式测试误折叠已展开分组，以及在 Mac 测试中误按 Control+Z。
- 本机候选的启动资源摘要前缀为 `96b975c54fea`。同一源码的 101 项定向浏览器用例通过，公式组 10 项单独复测。真实鉴权服务另走通桌宠导入与删除、会话更名与取消删除、配色重开、样式保存、窄窗导出和连接设置。Mac AppKit 与 WKWebView 通过 25 项检查，签名校验和安装候选冷启动通过。本机 V0.0.6 测试副本已替换并打开，旧副本已备份。Windows 与 Linux 原生验收仍按前述计划单独记录。
