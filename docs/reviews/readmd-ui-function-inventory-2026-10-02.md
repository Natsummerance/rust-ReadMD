# ReadMD 前端可操作功能总清单

盘点更新：2026-10-04（北京时间）；源码基线：`05ec02de5940ad59c560f07b45e212cbe914e71e`。范围为本仓库桌面阅读器、同源浏览器/LAN阅读界面、独立桌宠设置与运行窗口；不把官网/showcase、VS Code扩展、HarmonyOS独立产品混入桌面主界面。

这是一份**源码与安全界面抽查支持的功能清单**，不是所有业务都成功执行过的测试报告。每项按“入口 → 操作流程 → 前端处理 → Rust后端/存储 → 前置条件”记录。读到旧Python注释或`pywebview`名称时，以当前Rust接线为准；死代码、注释按钮、只存在API但无界面入口的能力单独标识。

本轮桌面窗口、托盘、关联、插件与更新的实机验证见[升级验收记录](readmd-window-system-upgrade-2026-10-04.md)。

## 使用与附件


| 附件 | 内容 |
| --- | --- |
| [逐控件清单 controls.md](ui-function-inventory-2026-10-02/controls.md) | 399个静态控件逐行记录，含隐藏文件输入、菜单项、复选框/输入/summary、层级、参数、操作和源码引用；数量不是同屏可见按钮总数。 |
| [动态交互清单 dynamic.md](ui-function-inventory-2026-10-02/dynamic.md) | 运行时按钮、列表、图谱、正文、编辑器与桌宠交互族；数据量可变的每项操作分别说明。 |
| [转换格式矩阵 formats.md](ui-function-inventory-2026-10-02/formats.md) | 逐扩展名核对收集目录、转换分派、底层实现、输出与限制。 |
| [插件矩阵 plugins.md](ui-function-inventory-2026-10-02/plugins.md) | 14个原生扩展目录项、实际Rust引擎、平台条件、安装/卸载/启用/持久化和真实转换语义。 |
| [导出参数 exports.md](ui-function-inventory-2026-10-02/exports.md) | 六种格式、14个配置组、每个字段及H1–H6样式，取值、默认值与后端使用路径。 |
| [命令与斜杠菜单 commands.md](ui-function-inventory-2026-10-02/commands.md) | 67条Shell注册记录、23个斜杠一级条目、快捷键、前置条件。 |
| [后端接口 api.md](ui-function-inventory-2026-10-02/api.md) | Rust路由与handler位置、前端使用位置、native bridge和仅API分支。 |
| [运行抽查与数据目录 catalogs.md](ui-function-inventory-2026-10-02/catalogs.md) | 隔离环境证据、完整provider/skill/语言/角色目录CSV、边界说明。 |
| [离线图表语法 native-diagrams.md](ui-function-inventory-2026-10-02/native-diagrams.md) | WSD、D2、ditaa新增离线渲染的实际语法、示例与限制。 |
| [逐项真实实现复查](readmd-ui-feature-verification-2026-10-03.md) | 原101项实现审计及后续104项覆盖；本轮保存与恢复变更见生命周期清单。 |
| [保存、副本、转换与AI编辑体验清单](readmd-document-lifecycle-ux-2026-10-03.md) | 全部文档生命周期细节、存储边界、前后端及测试证据。 |
| [实现与验收记录](readmd-ui-completion-2026-10-02.md) | 本轮补齐与修复项目、真实端到端/单测/离线构建结果、测试命令和平台条件。 |
| [机器可读 inventory.json](ui-function-inventory-2026-10-02/inventory.json) | 功能层级、静态控件、动态族、字段、格式、插件、命令、路由与验证元信息；CSV可在表格软件筛选。 |

## 核对数量

21个层级、104条操作流程、399个静态控件、80类动态交互；另有143行转换格式矩阵、14个插件、98个导出字段、67条Shell命令、23条斜杠菜单、100条HTTP路由及桌面IPC桥。目录数据、命令与控件可能复用，数量不能相加为功能总数。

## 界面层级树

```text
ReadMD
├─ 主窗口/主页
│  ├─ 打开/文件夹/新建/最近记录/主页卡片
│  ├─ 顶栏：标签、目录、搜索、主题、缩放、禅模式、编辑、AI、导出、命令
│  └─ 更多
│     ├─ 导入：万物转换、网页提取、扫描识别、剪贴新建
│     ├─ 互动：双向链接、演讲演示、代码运行、样式、另存、修复、共享
│     └─ 设置：语言、桌宠、文件关联、开机自启、更新
├─ 阅读区
│  ├─ 大纲/文件树/全文搜索/分页/连续阅读/阅读偏好
│  └─ 链接/图片灯箱/脚注/引用/callout/代码复制/代码运行/图表
├─ 编辑工作台
│  ├─ 文本/结构/插入工具、视图、选区工具、斜杠菜单、预览与分栏
│  ├─ 图片/表格/公式/代码/图表/子文档/元数据弹窗
│  └─ 文内AI：预览→替换/插入/放弃
├─ AI工作区
│  ├─ 对话、选区、无痕、停止、回复操作、历史会话
│  ├─ 连接设置：服务/模型/密钥/高级参数/测试
│  └─ Prompt/Skill：浏览、编辑、生成、评估发布、启停、导入导出
├─ 导入/转换
│  ├─ 万物转MD：文件/文件夹/覆盖、任务行、取消、输出目录
│  ├─ 插件中心：分类、运行信息、条件安装/重试/开关/卸载
│  ├─ OCR、网页提取、剪贴板分流
│  └─ 主窗口拖放/ZIP拆包/单文件预览草稿与另存为
├─ 导出：PDF/DOCX/EPUB/HTML/LaTeX/Reveal HTML
│  └─ 预设、排版组、AI样式、预览分页、执行/取消/结果操作
├─ 关联/呈现：图谱、正反向链接、演示播放器、LAN共享
├─ 设置：样式/语言/关联/自启/更新
├─ 桌宠设置
│  ├─ 预览与启用、角色搜索/收藏/选择/导入/删除
│  ├─ 活跃/安静、气泡、摸头/喂食/玩耍/休息/唤醒/AI
│  └─ 阅读器内/桌面、大小/透明度/置顶/锁定/声音/安装更新
├─ 独立桌宠：实体拖动、透明区穿透、点击气泡、右键菜单、文件拖入
├─ 文档保护：创建编辑副本、恢复草稿/版本历史、AI检查点、退出前确认
└─ 全局弹层：命令/快捷键、保存确认、冲突、选择、危险确认
```

## 实现状态先读

1. **音视频：**系统SAPI离线识别已接入，压缩媒体由本机FFmpeg解码；无引擎/无语音/错误明确失败。
2. **网页：**静态抓取与系统WebView动态DOM均已实现，私有页面使用隔离、任务限定授权；取消、超时、窗口关闭和撤销各有路径。
3. **插件：**14个目录项对应Rust扩展配置，可安装、互斥启用、持久化、卸载并参与真实转换。它们不等同于同名Python库或其模型质量。
4. **导出：**PDF直接预览真实产物，同输入导出复用同一份字节；DOCX显示共享内容与配置的版式参考并明确Word分页边界。
5. **ZIP/邮件：**选文件、文件夹收集和拖放的ZIP均进入安全拆包/逐项转换；EML实现MIME正文/附件，MSG实现CFB正文/附件/压缩RTF。
6. **命令：**export.presentation负责Reveal导出，view.presentation负责F5演讲，两项独立注册。
7. **验证：**完整清单保留条件入口、旧入口及明确拒绝项；实测与平台依赖详见验收记录，不把外部服务或系统引擎缺失算成入口消失。

## 前后端和状态存储关系

实际打包入口包括`assets/app.js`，它仍是主事件总线；不是只审查模块文件。顺序由 [rust/xtask/src/bundle.rs:9](../../rust/xtask/src/bundle.rs) 声明，页面装载`readmd.boot.js`，阅读增强和Shell部分按需加载。[assets/app.js:81 · bindEvents](../../assets/app.js)；[rust/readmd-kernel/src/main.rs:3210](../../rust/readmd-kernel/src/main.rs) 注入名称为`pywebview.api`的Rust桥。


| 数据 | 桌面后端/存储 | 浏览器或前端状态 |
| --- | --- | --- |
| 文档/资源 | server.rs文件接口，content.rs写读；导出由各writer处理 | 虚拟标签/编辑事务先在内存；浏览器打开上传副本 |
| 一般设置 | /api/settings + store.rs等配置层 | 浏览器readmd-settings；编辑器专属偏好单独localStorage |
| AI连接/凭据 | /api/ai/config；凭据句柄/has_key，GET不回显密钥 | 表单仅持本次新输入；选择/会话状态在state.ai |
| AI历史/Prompt/Skill | /api/ai/history、prompts、skills；Skill导入目录和元数据 | 无痕会话不按正常历史落盘；表单草稿先在内存 |
| 插件 | plugins/native扩展配置文件与manifest；安装校验、启用互斥与真实Rust执行 | 开关锁定/失败回滚/安装状态轮询 |
| 导出预设 | /api/export/presets，defaults/builtin/custom/last | options/selectedPreset/preview临时状态 |
| 桌宠 | /api/pets/configure/interact；preferences、companion、runtime/耐久命令队列 | 角色收藏、阅读器widget位置/动画、当前页签和搜索 |
| 图谱 | link_indexer + store链接索引 | Canvas物理布局/搜索/选择；特效开关localStorage |

## 01 启动、主页与全局入口

### F001 打开文档

**入口/控件：** `btn-open`、`w-open`、`file-input`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 点击打开 → 系统选择器或浏览器文件输入 → 上传/读取 → Markdown 打开，其余受支持文件分流转换/OCR → 标签与正文。取消不改变文档；选择器、上传及读取异常均反馈，异步操作被等待。浏览器保存操作作用于上传副本。

**前端实现：** [assets/js/reader/render.js:3161 · loadFileDialog](../../assets/js/reader/render.js)；[assets/js/reader/render.js:115 · loadFile](../../assets/js/reader/render.js)。

**后端与状态：** /api/dialog/choose-file、/api/upload、/api/file；server.rs 文件读取/上传 → content.rs 读取与修正；格式分流见 formats.md。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F002 打开文件夹

**入口/控件：** `btn-folder`、`w-folder`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 点击选择目录 → 系统选择器 → /api/list → 目录优先的自然排序文件树。读取失败保留当前目录；连续选择只应用最新请求结果；目录展开、文件打开和键盘导航均保留。浏览器入口说明桌面条件。

**前端实现：** [assets/js/reader/folder.js:13 · openFolder](../../assets/js/reader/folder.js)；[assets/js/reader/folder.js:105 · renderTreeNodes](../../assets/js/reader/folder.js)。

**后端与状态：** /api/dialog/choose-folder、/api/list；native_dialogs.rs + server.rs h_list。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F003 新建空白文档

**入口/控件：** `w-new`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 点击主页新建或命令“新建” → 建立未保存虚拟标签 → 进入编辑 → 输入内容 → 保存选择 .md 路径。此时标题是文档名，尚无磁盘文件。

**前端实现：** [assets/js/editor/preview.js:137 · newDocument](../../assets/js/editor/preview.js)。

**后端与状态：** 首次保存调用 /api/dialog/save-file 或 /api/dialog/save-as；后续 /api/save。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F004 主页快捷卡片

**入口/控件：** `w-convert`、`w-web`、`w-ocr`、`w-ai`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 转换、网页、OCR、AI 四张卡分别进入相应工作台；与顶栏/更多菜单复用同一处理函数，不产生另一套后台流程。主页的搜索命令按钮和快捷键说明入口由内联 onclick 转发到 Shell。

**前端实现：** [assets/js/core/history.js:430 · bindWelcomeEvents](../../assets/js/core/history.js)。

**后端与状态：** 复用转换、网页、OCR、AI 对应接口。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F005 最近记录与清空

**入口/控件：** `btn-recent`、`recent-clear`、`history-clear`、`history-close`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 最近文件列表 → 点击打开、移除单条或清空全部 → 后端存储回读与列表刷新。清空操作单次执行；HTTP 错误、业务失败和原生桥异常不会伪装成成功，也不会先清空当前列表。

**前端实现：** [assets/js/core/history.js:175 · openHistoryModal](../../assets/js/core/history.js)；[assets/js/core/history.js:80 · renderRecentList](../../assets/js/core/history.js)；[assets/js/core/history.js:190 · clearRecent](../../assets/js/core/history.js)。

**后端与状态：** /api/recent/status、add、remove、clear；store.rs 存储最近文件，server.rs 调用。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F006 返回主页

**入口/控件：** `btn-home`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 返回主页 → 正在编辑且有修改时弹出保存/放弃/取消 → 保存真正成功或明确放弃后才重置阅读区；取消保留编辑器。标签仍可再次打开。关闭标签已完成的确认通过内部参数复用，避免重复弹窗；并发主页点击不会叠加确认。

**前端实现：** [assets/js/core/history.js:358 · goHome](../../assets/js/core/history.js)。

**后端与状态：** 读取最近记录；返回主页本身不删除文件。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F007 更多菜单与分组

**入口/控件：** `btn-more`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 点击更多 → 展开菜单；导入、互动、设置分组标题可分别折叠/展开 → 点击具体功能。支持上下/Home/End 键移动、Esc 关闭及点击外部关闭；菜单分组属于导航，具体业务见对应层级。

**前端实现：** [assets/app.js:81 · bindEvents](../../assets/app.js)；[assets/app.js:70 · closeMoreMenu](../../assets/app.js)。

**后端与状态：** 前端处理；不调用业务后端。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F008 刷新文件列表/当前文件

**入口/控件：** `btn-reload`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 点击刷新按当前状态刷新文件夹列表或重新读取当前文件。命令面板“重新加载”调用 force:true 的 loadFile；外部文件自动重载是后台行为，无独立常驻开关按钮。编辑脏数据通过相应保护流程处理。

**前端实现：** [assets/app.js:81 · bindEvents](../../assets/app.js)；[assets/js/core/history.js:446 · startAutoReload](../../assets/js/core/history.js)。

**后端与状态：** /api/list、/api/file；以最终绑定与状态分支为准。

**实现状态：** 已接线；是否可用取决于下述前置条件。

## 02 多标签、标题和文件管理

### F009 切换、关闭及溢出标签

**入口/控件：** `doc-tabs-overflow-btn`。

**操作流程与交互：** 点击标签切换正文；×、中键或标签菜单关闭。以实际文本与保存基线比较：只打开、进入编辑、移动光标或撤销回原文均不提示保存；真实修改才提示保存/不保存/取消。切换保留独立草稿与撤销栈；关闭当前编辑标签不产生重复提示。

**前端实现：** [assets/js/core/tabs.js:59 · renderTabsBar](../../assets/js/core/tabs.js)；[assets/js/core/tabs.js:670 · closeTab](../../assets/js/core/tabs.js)。

**后端与状态：** 切换主要在内存；实际文档读取 /api/file，脏标签保存 /api/save

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F010 标签右键菜单和排序

**入口/控件：** `tab-context-menu`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 右键标签 → 左移、右移、关闭标签、关闭其他、关闭所有、重命名、复制文件路径七项。标签还可拖动重排；关闭多标签逐一经过脏数据决策。虚拟文档无实际路径时复制路径入口受限。

**前端实现：** [assets/js/core/tabs.js:426 · openTabContextMenu](../../assets/js/core/tabs.js)；[assets/js/core/tabs.js:411 · reorderTabs](../../assets/js/core/tabs.js)。

**后端与状态：** 顺序保留在前端标签数组；关闭时可能触发保存；复制使用剪贴板。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F011 标题重命名

**入口/控件：** `file-title`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 点击顶栏文件标题或双击标签/F2 → 输入名称 → Enter 或失焦提交，Esc 取消。磁盘文件经 native bridge rename_file 发送 path/new_stem，保留原目录与扩展名；虚拟标签只改显示名。目标同名或名称不合法时反馈错误。标签和顶栏存在各自提交路径，不能凭按钮可见就认定浏览器能改磁盘文件名。

**前端实现：** [assets/js/reader/render.js:34 · openFileRename](../../assets/js/reader/render.js)；[assets/js/core/tabs.js:373 · renameTab](../../assets/js/core/tabs.js)。

**后端与状态：** /api/rename → server.rs h_rename；路径/名称验证、改名与相关引用处理。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F012 另存为 Markdown

**入口/控件：** `btn-saveas`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 打开文档 → 更多/互动/另存为 → 选择目标路径 → 写出当前 Markdown；虚拟文档第一次保存也进入此类流程。桌面用系统保存对话框；浏览器可以走 Blob 下载路径，具体以 hasPy 分支为准。

**前端实现：** [assets/js/editor/preview.js:671 · saveAs](../../assets/js/editor/preview.js)。

**后端与状态：** /api/dialog/save-as、/api/save；native_dialogs.rs、server.rs 文件写入。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F103 恢复草稿与版本历史

**入口/控件：** `btn-document-history`、`document-history-close`、`document-history-refresh`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 更多菜单或命令面板 → 查看恢复草稿/保存前版本/操作检查点/已放弃草稿 → 打开为副本，或匹配原文件时应用到编辑器 → Ctrl+Z可撤销应用，Ctrl+S才改原文件；删除需确认，读取失败可刷新重试。记录在应用数据目录，30天、每文档10条、共256条/64MiB。

**前端实现：** [assets/js/features/document-history.js:168 · open](../../assets/js/features/document-history.js)；[assets/js/features/document-history.js:108 · restore](../../assets/js/features/document-history.js)；[assets/js/features/document-history.js:132 · refresh](../../assets/js/features/document-history.js)。

**后端与状态：** /api/documents/history → document_history.rs，gzip正文/SHA256校验/原子元数据/容量与孤立文件清理；恢复不直接写原文。

**实现状态：** 已接线；是否可用取决于下述前置条件。

## 03 目录、搜索、分页和阅读偏好

### F013 侧栏、大纲和文件树

**入口/控件：** `btn-toc`、`tab-toc`、`tab-files`、`side-close-btn`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 侧栏切换大纲/文件 → 标题跳转、目录展开/折叠、文件打开和方向键导航。目录 API 的状态与数据类型先验证；失败保留文件树，迟到的旧请求不能覆盖新目录。

**前端实现：** [assets/js/reader/toc.js:23 · buildToc](../../assets/js/reader/toc.js)；[assets/js/reader/folder.js:248 · showSide](../../assets/js/reader/folder.js)；[assets/js/reader/enhance.js:1057 · decorateToc](../../assets/js/reader/enhance.js)。

**后端与状态：** 大纲解析和滚动为前端；文件树 /api/list、/api/file。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F014 文内查找

**入口/控件：** `btn-search`、`search-input`、`search-prev`、`search-next`、`search-close`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 打开文档 → Ctrl+F/搜索按钮 → 输入关键词 → 正文匹配高亮并显示命中数量 → 上一项/下一项定位；Enter 下一项，Shift+Enter 上一项，Esc/× 清理高亮并关闭。没有命中时显示空结果。

**前端实现：** [assets/js/reader/search.js:120 · doSearch](../../assets/js/reader/search.js)；[assets/js/reader/search.js:196 · jumpToMark](../../assets/js/reader/search.js)。

**后端与状态：** 前端处理；不调用业务后端。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F015 分页跳转与连续阅读

**入口/控件：** `pg-first-btn`、`pg-prev-btn`、`pg-page-select`、`pg-next-btn`、`pg-last-btn`、`pg-mode-toggle`、`status-pagination`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 大文档满足分页条件后显示分页栏 → 首页/上一页/页码选择/下一页/末页 → 按页渲染；阅读模式按钮及状态栏分页状态切换分页/全卷连续。转连续前在提示框确认继续或取消；不可前进/后退时对应按钮禁用。目录定位会先切至包含标题的页。

**前端实现：** [assets/js/reader/render.js:434 · splitMdIntoPages](../../assets/js/reader/render.js)；[assets/js/reader/render.js:665 · togglePaginationMode](../../assets/js/reader/render.js)；[assets/js/reader/render.js:697 · renderPage](../../assets/js/reader/render.js)。

**后端与状态：** 分块与页切换在前端；初始内容来自 /api/file 或虚拟文档。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F016 主题、缩放和禅模式

**入口/控件：** `btn-theme`、`btn-a`、`btn-A`、`btn-zen`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 主题循环、缩放、禅模式和阅读偏好 → 即时应用 → 保存。读取设置只接受白名单字段及有效枚举，数值按范围限制；损坏配置不能覆盖文档、标签或 AI 状态。原生保存串行执行，保证最后选择持久化；保存失败显示反馈。

**前端实现：** [assets/js/core/settings.js:104 · toggleTheme](../../assets/js/core/settings.js)；[assets/js/core/settings.js:123 · zoom](../../assets/js/core/settings.js)；[assets/js/reader/render.js:2564 · toggleZenMode](../../assets/js/reader/render.js)。

**后端与状态：** 桌面 /api/settings 持久化主题与字号；浏览器 localStorage。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F017 浮动阅读设置

**入口/控件：** `rd-prefs-btn`、`rd-prefs`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 正文内浮动设置按钮 → 字号减/增、无衬线/衬线、行距紧凑/正常/宽松、宽度窄/正常/宽 → 即时调整阅读排版；显示字数/预计阅读时间。分段按钮支持方向键；点击外部或 Esc 关闭。

**前端实现：** [assets/js/reader/enhance.js:819 · buildPrefsPanel](../../assets/js/reader/enhance.js)；[assets/js/reader/enhance.js:779 · setPref](../../assets/js/reader/enhance.js)。

**后端与状态：** 桌面 /api/settings；浏览器 readmd-settings；readingFont、readingLeading、readingWidth。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F099 回到顶部

**入口/控件：** `top-btn`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 正文滚动超过600px后出现浮动上箭头 → 点击让正文区域平滑滚到顶部；作用于当前阅读容器，不重新读取文件、不改变文档。

**前端实现：** [assets/app.js:81 · bindEvents](../../assets/app.js)。

**后端与状态：** 前端处理；不调用业务后端。

**实现状态：** 已接线；是否可用取决于下述前置条件。

## 04 正文内可操作元素和学术阅读

### F018 正文链接、双链和锚点

**入口/控件：** `content`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 点击外部 http(s) 链接由 openPath/宿主打开；本地 Markdown 或 [[笔记#标题|别名]] 打开对应笔记并定位锚点；正文 #锚点及标题链图标用于定位。标题图标是href锚点，没有独立复制链接处理器。路径、协议和脚本内容经渲染白名单处理；未创建双链在关联列表中不可直接打开。

**前端实现：** [assets/js/reader/render.js:2940 · fixLinks](../../assets/js/reader/render.js)；[assets/js/reader/render.js:389 · navigateWikilink](../../assets/js/reader/render.js)；[assets/js/reader/enhance.js:591 · upgradeHeadings](../../assets/js/reader/enhance.js)。

**后端与状态：** /api/file、/api/system/open-path；Rust 解析路径，前端滚动到锚点。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F019 代码块复制、折叠提示、脚注与图片

**入口/控件：** `content`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 普通代码块复制按钮复制该块源码并短暂显示已复制；可折叠 callout 的标题展开/收起；脚注序号跳到文末定义，回跳箭头回到引用；点击正文图片打开大图灯箱，×/点击背景/Esc 关闭。正文表格横向滚动以保留列内容；普通任务清单阅读时为展示，编辑器里的任务标记可改源码。

**前端实现：** [assets/js/reader/enhance.js:471 · upgradeCode](../../assets/js/reader/enhance.js)；[assets/js/reader/enhance.js:106 · upgradeCallout](../../assets/js/reader/enhance.js)；[assets/js/reader/enhance.js:157 · upgradeFootnotes](../../assets/js/reader/enhance.js)；[assets/js/reader/enhance.js:723 · ensureLightbox](../../assets/js/reader/enhance.js)。

**后端与状态：** 复制与灯箱是前端；图片资源经 /raw 等资源路径加载，不把展示复选框误作后端任务系统。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F020 文献引用与参考文献交互

**入口/控件：** `content`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 文档含 citation/BibTeX 配置时 → 加载文献数据 → 引用编号/作者信息可跳参考文献；悬浮显示文献详情卡，DOI/URL 等链接可打开。引用样式依据文档内容/配置解析，界面没有独立完整文献库管理面板。

**前端实现：** [assets/js/reader/render.js:316 · loadDocCitations](../../assets/js/reader/render.js)；[assets/js/reader/render.js:1527 · processBibCitations](../../assets/js/reader/render.js)；[assets/js/reader/render.js:1603 · showBibHoverCard](../../assets/js/reader/render.js)。

**后端与状态：** /api/bibtex → bibtex.rs/引用处理；前端生成引用和悬浮卡。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F098 代码文件顶部四个操作

**入口/控件：** `btn-code-to-md`、`btn-code-edit`、`btn-code-ai-explain`、`btn-code-copy`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 打开代码文件 → 顶部四操作：转说明文档使用code_to_doc AI模板；编辑源码调用toggleEdit进入CodeMirror并保留原始代码；AI解析使用code_analysis；复制源码写剪贴板。编辑内容可撤销、保存，未保存关闭走统一确认。

**前端实现：** [assets/js/reader/render.js:1225 · renderContent](../../assets/js/reader/render.js)；[assets/js/reader/render.js:1317 · bindCodeDocActions](../../assets/js/reader/render.js)。

**后端与状态：** 说明文档/解析复用/api/ai/chat；编辑事务前端、保存/api/save；复制剪贴板。

**实现状态：** 四动作已接线；编辑按钮通过真实浏览器交互验证；AI依赖所选连接。

## 05 Markdown 编辑工作台

### F021 编辑、保存和取消

**入口/控件：** `btn-edit`、`edit-area`、`edit-save`、`edit-cancel`。

**操作流程与交互：** 进入编辑读取真实草稿；Ctrl+S仅保存提交快照，成功保留撤销栈，后续输入仍脏；Ctrl+Shift+S保存当前编辑内容并在成功后切换路径。无路径文档必须选择位置；浏览器下载不等于已落盘。保存前中央版本、编码确认、mtime+内容指纹冲突保护；放弃先留恢复记录。 脏状态根据文本内容计算，兼容 LF/CRLF/CR；进入编辑不标脏，撤销回原文立即恢复干净。

**前端实现：** [assets/js/editor/preview.js:440 · toggleEdit](../../assets/js/editor/preview.js)；[assets/js/editor/preview.js:593 · saveEdit](../../assets/js/editor/preview.js)；[assets/js/editor/editor.js:53 · createEditor](../../assets/js/editor/editor.js)。

**后端与状态：** /api/save、/api/dialog/save-as、/api/documents/history → server.rs/content.rs/document_history.rs；不创建旁边.bak

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F022 文字格式与结构工具

**入口/控件：** `data-md`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 选中文本或放置光标 → 工具栏常用项/“文本”/“结构”菜单 → 加粗、斜体、删除线、行内代码、行内公式、链接；一级/二级/三级标题、正文、引用、提示块、无序/有序/任务列表、代码块、块公式、分隔线。操作经统一 Markdown transform 更新选区和撤销栈；不是直接把 HTML 写入文档。

**前端实现：** [assets/js/editor/editor.js:701 · cmInsertSyntax](../../assets/js/editor/editor.js)；[assets/js/editor/md-transforms.js:1](../../assets/js/editor/md-transforms.js)。

**后端与状态：** 纯前端文本变换；点击保存才写磁盘。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F023 编辑菜单、视图和撤销栈

**入口/控件：** `data-menu`、`edit-view-trigger`、`edit-view-focus`、`edit-view-typewriter`、`edit-view-lines`、`edit-undo`、`edit-redo`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 点击文本/结构/插入菜单展开二级项；编辑视图菜单切专注行、打字机居中、显示行号；撤销/重做作用于编辑器事务。开关更新编辑器扩展及界面勾选状态；编辑器偏好与阅读器全局设置使用各自存储。

**前端实现：** [assets/js/editor/editor.js:2407 · bindEditBarExtras](../../assets/js/editor/editor.js)；[assets/js/editor/editor.js:635 · setEditorPref](../../assets/js/editor/editor.js)；[assets/js/editor/editor.js:167 · cmUndo](../../assets/js/editor/editor.js)。

**后端与状态：** 编辑器偏好 localStorage；文字撤销在前端，最终保存 /api/save。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F024 选区浮动工具条

**入口/控件：** `cm-sel-ai`、`cm-sel-copy`、`cm-sel-cut`、`cm-sel-paste`、`cm-selection-toolbar`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 鼠标或键盘选择非空文字 → 浮动条出现 → AI 改写、粗体、斜体、删除线、代码、链接、标题、引用、复制、剪切、粘贴。选区失效时隐藏；剪切/粘贴走编辑器事务，AI 入口使用当前选区。

**前端实现：** [assets/js/editor/editor.js:203 · updateCmSelectionToolbar](../../assets/js/editor/editor.js)；[assets/js/editor/editor.js:273 · bindCmSelectionToolbar](../../assets/js/editor/editor.js)；[assets/js/editor/editor.js:332 · cmPasteSelection](../../assets/js/editor/editor.js)。

**后端与状态：** 格式与剪贴板前端；AI /api/ai/chat；写磁盘须保存。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F025 斜杠菜单与子选择器

**入口/控件：** `edit-slash-btn`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 点击命令按钮或在合适位置输入 / → 按词搜索/点条目；23 个一级条目逐项见 commands.md。代码块进入语言子列表（纯文本、25 个预设语言、自定义输入）；表格进入尺寸选择器。鼠标、方向键、Enter、Esc 完成选择或退出；插入保持编辑历史。

**前端实现：** [assets/js/editor/editor.js:1936 · slashCatalog](../../assets/js/editor/editor.js)；[assets/js/editor/editor.js:2008 · slashLanguageItems](../../assets/js/editor/editor.js)；[assets/js/editor/editor.js:2160 · slashRenderTable](../../assets/js/editor/editor.js)。

**后端与状态：** 纯前端；生成 Markdown 后按正常保存路径持久化。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F026 智能编辑、粘贴与任务勾选

**入口/控件：** `edit-area`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 在代码围栏、Markdown 表格和列表中 Enter/Tab/Shift+Tab 自动补结构或缩进；选中文字输入强调符号可包裹；粘贴 Excel 的 HTML/制表文本转为 Markdown 表格；粘贴图片走图片保存；点击编辑器任务标记切换 [ ]/[x]。这些交互属于源码编辑，不是另一个后端表格数据库。

**前端实现：** [assets/js/editor/editor.js:1805 · cmPriorityKeymap](../../assets/js/editor/editor.js)；[assets/js/editor/editor.js:1154 · handleSmartExcelPaste](../../assets/js/editor/editor.js)；[assets/js/editor/editor.js:552 · cmTaskMarkerClick](../../assets/js/editor/editor.js)；[assets/js/editor/editor.js:1897 · cmHandlePaste](../../assets/js/editor/editor.js)。

**后端与状态：** 通常纯前端；图片 /api/image/save，文档最终 /api/save。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F027 预览布局与分栏拖动

**入口/控件：** `pv-trigger`、`data-pv`、`pv-sync`、`pv-splitter`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 编辑 → 预览菜单 → 上/左/右/下/关闭五种布局 → 实时渲染 Markdown；勾选同步滚动使源码和预览滚动联动；拖动横向/纵向分隔条调整比例。布局、滚动同步与比例保存到设置。

**前端实现：** [assets/js/editor/preview.js:60 · setPvLayout](../../assets/js/editor/preview.js)；[assets/js/editor/preview.js:173 · bindPvSplitter](../../assets/js/editor/preview.js)；[assets/js/editor/preview.js:381 · pvSyncFromPreview](../../assets/js/editor/preview.js)。

**后端与状态：** 桌面 /api/settings；浏览器 localStorage；正文预览渲染在前端。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F102 创建编辑副本

**入口/控件：** `btn-document-copy`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 更多菜单或命令面板 → 捕获当前真实草稿及图片引用 → 创建独立、未保存标签并进入编辑 → Ctrl+S首次弹系统路径选择 → 成功后副本成为当前文件；取消不落盘。原稿、原稿撤销栈和原目录不被改写。

**前端实现：** [assets/js/features/document-history.js:87 · createCopy](../../assets/js/features/document-history.js)；[assets/js/editor/preview.js:677 · saveAsSnapshot](../../assets/js/editor/preview.js)。

**后端与状态：** /api/documents/history保留恢复草稿；用户明确保存时/api/dialog/save-as → save_document_copy，版本保留/资源去重与原子写入。

**实现状态：** 已接线；是否可用取决于下述前置条件。

## 06 插入图片、表格、公式和高级块

### F028 图片输入及 URL 直接插入

**入口/控件：** `img-file`、`img-file-input`、`img-url-input`、`img-url-load`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 插入菜单/图片 → 选择本地图片加载工作台；也可粘 URL 后点插入 URL/Enter，直接生成 Markdown 图片链接。URL 直接插入和本地裁剪是两条不同路径，URL 入口不等于完整远程图片编辑。

**前端实现：** [assets/js/editor/image.js:24 · openImgModal](../../assets/js/editor/image.js)；[assets/js/editor/image.js:327 · insertImgUrl](../../assets/js/editor/image.js)；[assets/js/editor/image.js:39 · loadImgFromFile](../../assets/js/editor/image.js)。

**后端与状态：** 选择本地图片浏览器 FileReader；URL 生成源码；本地编辑后的结果 /api/image/save。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F029 裁剪、旋转、翻转、尺寸及历史

**入口/控件：** `img-rot-l`、`img-rot-r`、`img-flip-x`、`img-flip-y`、`img-angle`、`img-angle-number`、`img-view-zoom`、`img-ratio`、`img-out-w`、`img-out-h`、`img-size-lock`、`img-undo`、`img-redo`、`img-reset`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 加载图片 → 拖动裁剪框或八个方向手柄 → 自由/1:1/4:3/3:2/16:9/原图比例 → 左/右90°旋转或精确角度，水平/垂直翻转 → 预览缩放 → 输出宽/高（处理上限 16000），锁定宽高比例 → 撤销/重做/重置。缩放用于编辑视图，输出尺寸单独控制生成图片。

**前端实现：** [assets/js/editor/image.js:195 · rotateImg](../../assets/js/editor/image.js)；[assets/js/editor/image.js:209 · applyRatio](../../assets/js/editor/image.js)；[assets/js/editor/image.js:20 · undoImg](../../assets/js/editor/image.js)。

**后端与状态：** Canvas 本地变换；尚未点击插入时不改正文或原图文件。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F030 图片生成、插入与关闭

**入口/控件：** `img-insert`、`img-close`、`img-close-x`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 点插入 → Canvas 导出处理后图像 → /api/image/save 保存图片资源 → 在当前光标插 Markdown 相对路径或相应回退表示 → 标记文档未保存。关闭/×放弃当前工作台编辑；图像保存与 .md 保存是两个步骤。

**前端实现：** [assets/js/editor/image.js:346 · exportAndInsertImg](../../assets/js/editor/image.js)；[assets/js/editor/image.js:33 · closeImgModal](../../assets/js/editor/image.js)。

**后端与状态：** /api/image/save → server.rs h_image_save；资源路径与编码验证。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F031 表格网格选择

**入口/控件：** `btn-insert-table`、`table-modal-close`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 插入表格 → 10×10 尺寸网格 → 鼠标悬停/键盘焦点显示行列 → 点击或 Enter 插入对应尺寸；方向键移动，关闭不插入。每个格子是真实带名称的 44px 按钮，小窗口允许滚动访问。

**前端实现：** [assets/js/editor/editor.js:1270 · initTableGridPicker](../../assets/js/editor/editor.js)；[assets/js/editor/editor.js:1314 · insertCustomTable](../../assets/js/editor/editor.js)。

**后端与状态：** 前端处理；不调用业务后端。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F032 公式模板选择

**入口/控件：** `formula-open`、`formula-close`、`formula-search`、`formula-mode`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 公式按钮 → 搜索模板、选择分类/模板 → 查看公式预览 → 按行内/块公式模式插入 LaTeX。分类和模板按钮动态生成，具体模板目录见数据清单；正文公式由本地 MathJax/KaTeX 类资源渲染。

**前端实现：** [assets/js/editor/editor.js:1090 · renderFormulaPicker](../../assets/js/editor/editor.js)；[assets/js/editor/editor.js:1118 · insertFormula](../../assets/js/editor/editor.js)。

**后端与状态：** 前端模板插入；Rust 读取修复与导出公式另由 formula_repair.rs/formula 系列实现。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F033 交互式代码块插入

**入口/控件：** `btn-insert-code-chunk`、`code-chunk-modal-close`、`code-chunk-lang`、`code-chunk-opt-plot`、`code-chunk-opt-hide`、`code-chunk-code`、`code-chunk-cancel`、`code-chunk-insert`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 选择七种代码语言 → 填代码、图形输出与隐藏选项 → 插入带属性的围栏。空源码不插入、不关闭弹窗；围栏长度自动大于源码中的反引号，防止结构破坏。执行仍取决于已有系统解释器，插入不会自动运行。

**前端实现：** [assets/js/editor/editor.js:756 · openCodeChunkModal](../../assets/js/editor/editor.js)；[assets/js/editor/editor.js:773 · insertCodeChunkFromModal](../../assets/js/editor/editor.js)。

**后端与状态：** 插入前端处理；点击运行后 /api/code/run → parity_code.rs + code_chunk_runner.rs。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F034 科学图表插入

**入口/控件：** `btn-insert-diagram`、`diagram-modal-close`、`diagram-type`、`diagram-code`、`diagram-cancel`、`diagram-insert`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 选择科学图表类型 → 示例/用户草稿 → 编辑 → 插入。重新打开保留草稿；空代码给出反馈；嵌套反引号用更长围栏保护。渲染器、运行条件及复杂语法限制见图表附件。

**前端实现：** [assets/js/editor/editor.js:805 · openDiagramModal](../../assets/js/editor/editor.js)；[assets/js/editor/editor.js:822 · insertDiagramFromModal](../../assets/js/editor/editor.js)。

**后端与状态：** 插入前端；渲染路径详见第07层和 api.md。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F035 引用子文档

**入口/控件：** `btn-insert-doc-import`、`doc-import-modal-close`、`doc-import-path`、`doc-import-browse`、`doc-import-mode`、`doc-import-lines`、`doc-import-cancel`、`doc-import-insert`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 输入/浏览引用路径 → 选择 Markdown 递归、Code 高亮或 HTML 原样包含 → 正整数行号或由小到大的范围 → 插入 @import。前端拒绝空路径、引号/换行及非法范围；Rust 真实处理 mode 和 lines，保留预算、路径限制、循环检测及深度上限；不再忽略面板选项。

**前端实现：** [assets/js/editor/editor.js:849 · insertDocImportFromModal](../../assets/js/editor/editor.js)；[assets/js/reader/render.js:852 · processDocImports](../../assets/js/reader/render.js)。

**后端与状态：** /api/import/process → import_processor.rs；读文件并按引用模式展开。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F036 YAML/演示元数据

**入口/控件：** `btn-insert-frontmatter`、`frontmatter-modal-close`、`fm-input-title`、`fm-input-author`、`fm-select-theme`、`fm-select-transition`、`fm-modal-cancel`、`fm-modal-insert`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 打开元数据 → 读取现有标题、作者、演示主题/转场 → 编辑 → 有效 YAML 引号转义后更新受控字段。保留文献、自定义键、注释和其他演示设置；支持普通缩进及行内映射。文档在弹窗期间变化会拒绝旧修改；复杂锚点/别名等结构要求在源码编辑，不静默改写或损坏。

**前端实现：** [assets/js/editor/editor.js:982 · openFrontmatterModal](../../assets/js/editor/editor.js)；[assets/js/editor/editor.js:1019 · insertFrontmatterFromModal](../../assets/js/editor/editor.js)。

**后端与状态：** 编辑器源码修改；读取/演示编译在后端解析 frontmatter。

**实现状态：** 已接线；是否可用取决于下述前置条件。

## 07 代码运行与图表渲染

### F037 单个代码块运行、复制输出、清空

**入口/控件：** `code-chunk-run-btn`、`code-chunk-copy-btn`、`code-chunk-clear-btn`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 文档存在 cmd/可执行属性块 → 点击运行 → 按钮禁用、状态和计时显示 → 结果显示 stdout/stderr及可捕获图像 → 可再次运行、复制输出或清空显示。output=true 且处于编辑状态时才通过编辑器事务回填输出并走保存路径；普通运行仅影响预览。

**前端实现：** [assets/js/reader/render.js:1761 · renderAllCodeChunks](../../assets/js/reader/render.js)；[assets/js/reader/render.js:1902 · persistCodeChunkOutput](../../assets/js/reader/render.js)。

**后端与状态：** /api/code/run 要求 confirm:true；code_chunk_runner.rs 选择本地语言运行时或内置 SQL，限制cwd、网络/路径、超时与输出。

**实现状态：** 条件功能：多数语言需要本机解释器；源码插入和执行分开。

### F038 运行全部代码块

**入口/控件：** `btn-run-all-chunks`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 有可运行代码块时更多/互动显示运行代码 → 点击后按实现顺序执行各卡片；无代码块时入口隐藏/禁用。每个块保留独立运行状态、计时和输出，失败不是整个文档变成运行成功。

**前端实现：** [assets/js/reader/render.js:1948 · runAllCodeChunks](../../assets/js/reader/render.js)。

**后端与状态：** 复用每块 /api/code/run；并非另一种无限权限执行器。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F039 本地图表与失败回退

**入口/控件：** `diagram-preview`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** Mermaid、WaveDrom、DOT/Viz、Chart.js、BitField、TikZ 等按本地 vendor JS/WASM渲染；Vega/Vega-Lite也有 Rust/Node 后端路径。渲染失败保留源码并显示回退原因；主题变化重渲染。图表源码不是可随意执行的页面 HTML。

**前端实现：** [assets/js/reader/render.js:2438 · renderAllDiagrams](../../assets/js/reader/render.js)；[assets/js/reader/render.js:2229 · renderLocalDiagram](../../assets/js/reader/render.js)。

**后端与状态：** /api/diagram/render、capabilities → parity_diagram.rs + diagrams.rs；当前探测枚举见 api.md。

**实现状态：** 按引擎/随包资源/本机工具条件可用。

### F040 PlantUML 显式远程渲染

**入口/控件：** `diagram-allow-remote-btn`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** PlantUML 无本地 Java/JAR → 显示依赖缺失和允许远程按钮 → 点击该按钮再确认 → allow_remote:true 请求远程 SVG；取消则保留源码。WSD、D2、ditaa 的基础语法已由Rust离线输出SVG，不请求外部渲染；完整第三方高级语法不等同于本机基础实现，支持内容与拒绝规则见 native-diagrams.md。

**前端实现：** [assets/js/reader/render.js:2438 · renderAllDiagrams](../../assets/js/reader/render.js)。

**后端与状态：** /api/diagram/render → parity_diagram.rs render_plantuml；本地优先，远程必须明确 true。

**实现状态：** 本机未配置 PlantUML 时为条件联网功能。

## 08 AI 对话与文内改写

### F041 打开、收起、全屏和调整 AI 面板

**入口/控件：** `btn-ai`、`ai-close`、`ai-expand-toggle`、`ai-jump-latest`、`ai-resize-handle`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 顶栏/主页/快捷键打开 AI → 按需加载模块和配置 → 空态显示快速提问入口 → ×收起；全屏按钮切独占布局；拖左边界改宽度；跳到最新回复滚动到底部。未配置连接时提示进入 AI设置，面板本身可打开。

**前端实现：** [assets/js/features/ai.js:53 · handleTopAiButtonClick](../../assets/js/features/ai.js)；[assets/js/features/ai.js:86 · toggleAiFullscreen](../../assets/js/features/ai.js)；[assets/js/features/ai.js:2403 · bindAiResize](../../assets/js/features/ai.js)。

**后端与状态：** /api/modules/load、/api/ai/config、/api/ai/prompts；面板宽度 /api/settings。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F042 输入、模板、选区、无痕及发送

**入口/控件：** `ai-template`、`ai-tpl-btn`、`ai-prompt`、`ai-selection`、`ai-incognito`、`ai-run`、`ai-stop`、`ai-clear-ctx`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 选择 Prompt/Skill 或直接输入问题 → 可勾选仅使用选中文本，未勾选时按动作选当前文档/上下文 → 发送/键盘发送 → 请求后状态、逐字流式输出与用量 → 停止中断前端请求；新对话清空当前上下文。无痕开关抑制会话落盘，仍会把选定内容发送到所选服务。

**前端实现：** [assets/js/features/ai.js:2040 · runAi](../../assets/js/features/ai.js)；[assets/js/features/ai.js:1861 · getAiTargetText](../../assets/js/features/ai.js)；[assets/js/features/ai.js:1465 · clearAiContext](../../assets/js/features/ai.js)。

**后端与状态：** /api/ai/chat → server.rs h_ai_chat → parity_aichat.rs/ai.rs/ai_providers.rs；普通会话 /api/ai/history。

**实现状态：** 需有效连接/模型；支持本地服务时不一定要求云密钥。

### F043 消息操作、文档上下文展开和重试

**入口/控件：** `ai-bubble-act-btn`、`ai-code-copy`、`ai-regen-btn`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 回复下方四项：复制、应用到文档、另存MD、重新生成；回复代码块另有复制代码；用户文档上下文卡可展开/收起全文。请求失败显示重试按钮。重新生成删除最后一组问答后按原问题重新请求，并不是服务端“版本切换”。

**前端实现：** [assets/js/features/ai.js:1320 · renderAiBubbleActions](../../assets/js/features/ai.js)；[assets/js/features/ai.js:1333 · appendAiUserBubble](../../assets/js/features/ai.js)；[assets/js/features/ai.js:1304 · regenerateLastAnswer](../../assets/js/features/ai.js)。

**后端与状态：** 复制在前端；重新生成 /api/ai/chat；应用和另存见下一项。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F044 应用回复及 AI 副本文档

**入口/控件：** `ai-apply`、`ai-save`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** AI整体结果打开未保存副本；选区替换/续写核对来源标签、原文和版本，先保存检查点再执行单个可撤销事务；迟到或编辑变化改开安全副本。AI副本首次保存必须选择路径，不自动写原目录。

**前端实现：** [assets/js/features/ai.js:2510 · applyAi](../../assets/js/features/ai.js)；[assets/js/features/ai.js:2698 · saveAiAs](../../assets/js/features/ai.js)。

**后端与状态：** /api/ai/chat生成；应用事务在编辑器；history保留检查点；用户保存才调用/save或/dialog/save-as。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F045 编辑内 AI 操作条

**入口/控件：** `edit-ai-close`、`edit-ai-input`、`edit-ai-submit`、`edit-ai-apply`、`edit-ai-insert`、`edit-ai-discard`、`data-ai-action`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 选区/光标打开文内 AI → 一次请求 → 文本预览 → 替换、插入或放弃。关闭使旧响应失效并中止前端请求；切换标签不能应用到相同文字的另一篇文档。选区无法唯一定位或来源改变时创建未保存副本；应用是可撤销的编辑事务。

**前端实现：** [assets/js/editor/editor.js:1411 · runEditAiAction](../../assets/js/editor/editor.js)；[assets/js/editor/editor.js:1560 · applyEditAiResult](../../assets/js/editor/editor.js)；[assets/js/editor/editor.js:1606 · insertEditAiResult](../../assets/js/editor/editor.js)。

**后端与状态：** /api/ai/chat 使用共享连接和Skill；修改在编辑器，/api/save 持久化。

**实现状态：** 已接线；是否可用取决于下述前置条件。

## 09 AI 连接设置与会话历史

### F046 选择连接、官方预设和上游目录

**入口/控件：** `ai-settings-close`、`ai-provider`、`ai-provider-search`、`ai-settings-open`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** AI设置 → 选择已保存自定义连接或26个官方预设；展开服务目录，搜索/点击上游连接卡加载相应配置。目录有1148条数据，完整条目在 providers.csv。预设是配置模板，列在目录不代表模型可用、密钥已配置或服务已实测。

**前端实现：** [assets/js/features/ai.js:1515 · fillAiProviders](../../assets/js/features/ai.js)；[assets/js/features/ai.js:1652 · onAiProviderChange](../../assets/js/features/ai.js)。

**后端与状态：** /api/ai/config → server.rs h_ai_config；ai_providers.rs 提供预设，配置与凭据持久化。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F047 创建/删除自定义连接及模型选择

**入口/控件：** `ai-provider-new`、`ai-provider-delete`、`ai-model`、`ai-models-btn`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 点新建自定义连接 → 输入名称、URL、协议等 → 保存；仅自定义连接可删除并确认。模型是 select：获取模型向配置端点拉取列表，再选模型；预设/模型列表为空或鉴权失败显示对应反馈，不把模型选项描述为任意文本输入框。

**前端实现：** [assets/js/features/ai.js:1721 · newAiProvider](../../assets/js/features/ai.js)；[assets/js/features/ai.js:1737 · deleteAiProvider](../../assets/js/features/ai.js)；[assets/js/features/ai.js:1942 · loadAiModels](../../assets/js/features/ai.js)。

**后端与状态：** /api/ai/config POST 保存/删除、/api/ai/models GET/POST 拉模型目录。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F048 密钥输入、显示切换和清除

**入口/控件：** `ai-key`、`ai-key-toggle`、`ai-key-clear`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 输入新密钥 → 可切换当前输入框密码/文本显示 → 清除标记删除凭据 → 保存。已保存密钥仅显示“已配置”等状态，GET配置不回显原始密钥，打开面板不会重新填入密钥。清除按钮和显示按钮各自作用，不把“显示当前输入”写成“读回已存密钥”。

**前端实现：** [assets/js/features/ai.js:1670 · syncAiKey](../../assets/js/features/ai.js)；[assets/js/features/ai.js:1997 · toggleAiKey](../../assets/js/features/ai.js)；[assets/js/features/ai.js:2005 · clearAiKey](../../assets/js/features/ai.js)。

**后端与状态：** /api/ai/config POST 保存 key/clear_key；读取使用 has_key/credential_id 等句柄信息。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F049 高级参数与连接保存

**入口/控件：** `ai-provider-name`、`ai-base-url`、`ai-url-reset`、`ai-mode`、`ai-endpoint-mode`、`ai-headers`、`ai-stream`、`ai-save-key`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 展开高级参数 → 名称、Base URL、重置URL；协议 auto/chat/completion/responses/messages；端点模式前缀或完整URL；自定义Headers JSON；流式开关。点保存验证Headers对象和重名 → POST配置 → 重新读配置 → 已保存并关闭。保存官方预设时创建独立 custom: 连接，避免把模板当用户配置本身。高级折叠与服务目录折叠只控制显示。

**前端实现：** [assets/js/features/ai.js:1759 · saveAiSelection](../../assets/js/features/ai.js)；[assets/js/features/ai.js:1628 · readAiCustomHeaders](../../assets/js/features/ai.js)。

**后端与状态：** /api/ai/config；配置字段与 AI 请求协议归一化见 ai.rs/parity_aichat.rs。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F050 测试连接

**入口/控件：** `ai-test-connection`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 点测试 → 禁用测试、保存、获取模型按钮并显示“正在测试” → 先保存当前表单 → POST /api/ai/models → 成功显示连接状态和模型数，失败显示鉴权/网络/协议等提示 → finally恢复按钮。该测试验证模型目录请求，不等于实际完成一次聊天推理。

**前端实现：** [assets/js/features/ai.js:2288 · testAiConnection](../../assets/js/features/ai.js)。

**后端与状态：** /api/ai/config 后 /api/ai/models；后端读取凭据并调用提供商。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F051 会话浏览、命名、保存、删除、复制及导出

**入口/控件：** `ai-history-open`、`ai-history-close`、`ai-history-search`、`ai-history-copy`、`ai-history-export`、`ai-history-clear`、`ai-session`、`ai-save-session`、`ai-del-session`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 历史按钮 → 加载会话列表 → 搜索标题 → 点会话恢复问答；会话条目支持重命名及删除。保存会话写历史；删除当前/删除条目和清空所有经各自确认。复制把会话格式化成文本/Markdown；导出会话通过桌面保存/浏览器下载。静态的 ai-session/save-session/delete-session 是旧控件，当前可见性见 controls.md，不能重复当成另一套界面。

**前端实现：** [assets/js/features/ai.js:1140 · loadAiSessions](../../assets/js/features/ai.js)；[assets/js/features/ai.js:1150 · renderAiSessionSelect](../../assets/js/features/ai.js)；[assets/js/features/ai.js:2356 · exportCurrentConversation](../../assets/js/features/ai.js)。

**后端与状态：** /api/ai/history 按 action/list/get/save/delete/rename/clear；store.rs AI 会话持久化。

**实现状态：** 已接线；是否可用取决于下述前置条件。

## 10 Prompt / Skill 工作台

### F052 搜索、浏览和技能说明

**入口/控件：** `ai-tpl-btn`、`tpl-close`、`tpl-search`、`tpl-close-btn`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 打开模板工作台 → 搜索名称/内容 → 分类分组的动态 Skill 行 → 点行查看名称、描述、角色指令、许可证/来源/版本等事实 → 选作 AI 模板。内置技能和用户技能有不同编辑/删除权限；当前隔离快照25个内置Skill，不是承诺用户数据也只有25个。

**前端实现：** [assets/js/features/ai.js:339 · renderTplList](../../assets/js/features/ai.js)；[assets/js/features/ai.js:432 · renderSkillOverview](../../assets/js/features/ai.js)；[assets/js/features/ai.js:507 · selectTpl](../../assets/js/features/ai.js)。

**后端与状态：** /api/ai/prompts、/api/skills；Rust 模板/skill 目录与元信息。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F053 新建、编辑、复制、保存和删除

**入口/控件：** `tpl-new`、`tpl-edit`、`tpl-copy`、`tpl-del`、`tpl-id`、`tpl-action`、`tpl-name`、`tpl-system`、`tpl-user`、`tpl-save`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 新建空白技能/模板 → 输入名称、系统指令、用户模板（支持 {doc}/{prompt}）→ 保存；编辑进入表单，复制内置或现有项为可改副本；删除自定义项须确认，内置模板的delete分支是重置默认内容。tpl-id/tpl-action 是隐藏表单元数据，用户不能直接点击它们。

**前端实现：** [assets/js/features/ai.js:539 · copyCurrentSkill](../../assets/js/features/ai.js)；[assets/js/features/ai.js:1079 · saveTplForm](../../assets/js/features/ai.js)；[assets/js/features/ai.js:1104 · deleteCurrentTpl](../../assets/js/features/ai.js)。

**后端与状态：** /api/ai/prompts POST 维护自定义模板；/api/skills 提供 Skill 对应能力；以保存分支为准。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F054 AI 生成草稿和示例

**入口/控件：** `tpl-ai-generate`、`skill-create-close`、`skill-create-example`、`skill-create-name`、`skill-create-purpose`、`skill-create-format`、`skill-create-cancel`、`skill-create-go`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** AI生成 → 可填入精读示例 → 填技能名称/用途 → 输出 Markdown/结构化表格/行动清单 → 生成草稿 → 展示在工作台可编辑。生成需要共享AI连接；生成草稿不会直接变成已发布启用技能。取消/×保留原工作台。

**前端实现：** [assets/js/features/ai.js:639 · generateSkillDraft](../../assets/js/features/ai.js)；[assets/js/features/ai.js:559 · openSkillIdeaDialog](../../assets/js/features/ai.js)。

**后端与状态：** /api/ai/chat 使用技能生成提示；发布/保存是后续独立动作。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F055 评估、发布、启用/停用和导出单个

**入口/控件：** `tpl-publish`、`tpl-toggle`、`tpl-export-one`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 发布 → 先 /api/skills action:evaluate → 取得evaluation_token → 用户确认 → action:publish写入启用技能；启用/停用对可管理自定义项发enable/disable；导出单个Skill下载 .SKILL.md，内置项由元数据+指令合成。按钮文案“排版出版”实际对应 Skill 发布，不应误列为文件导出。

**前端实现：** [assets/js/features/ai.js:694 · publishCurrentSkill](../../assets/js/features/ai.js)；[assets/js/features/ai.js:723 · toggleCurrentSkillEnabled](../../assets/js/features/ai.js)；[assets/js/features/ai.js:742 · exportCurrentSkill](../../assets/js/features/ai.js)。

**后端与状态：** /api/skills evaluate/publish/enable/disable/export；Rust后端检查评估与确认，scripts_allowed:false。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F056 导入 Markdown/JSON 及全部导出

**入口/控件：** `tpl-import-btn`、`tpl-file-input`、`tpl-export-btn`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 工作台导入入口和隐藏文件输入支持模板文件处理：读取 MD/frontmatter或JSON模板数组 → 解析名称/system/user/action → 写自定义模板 → 重新加载列表。导出全部将当前模板列表封成version/exported_at/templates JSON下载。当前导入主菜单默认打开GitHub来源，不应把隐藏文件 input 当成一直可見按钮；其可到达路径详见 controls.md。

**前端实现：** [assets/js/features/ai.js:844 · importTemplatesFromFile](../../assets/js/features/ai.js)；[assets/js/features/ai.js:1062 · exportTemplatesAsJson](../../assets/js/features/ai.js)。

**后端与状态：** /api/ai/prompts POST；导出JSON为浏览器 Blob。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F057 GitHub、目录、ZIP来源预览和选择导入

**入口/控件：** `tpl-import-source-github`、`tpl-import-source-folder`、`tpl-import-source-zip`、`tpl-github-url`、`tpl-github-credential`、`tpl-github-preview-btn`、`tpl-folder-input`、`tpl-zip-input`、`tpl-github-apply-btn`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 导入菜单 → 三来源页签 → GitHub填写仓库/子目录URL，鉴权失败才显示token输入；桌面目录/ZIP用原生选择器，浏览器目录用webkitdirectory、ZIP先上传 → POST预览 → 动态技能复选框（无效项禁用）→ 选择 → 确认导入 → apply提交。当前前端冲突策略固定skip，无覆盖/重命名下拉；输入token使用后清空，目录源码/脚本元数据只展示和受控导入。

**前端实现：** [assets/js/features/ai.js:902 · selectSkillImportSource](../../assets/js/features/ai.js)；[assets/js/features/ai.js:977 · renderGithubSkillPreview](../../assets/js/features/ai.js)；[assets/js/features/ai.js:1028 · applyGithubSkillImport](../../assets/js/features/ai.js)。

**后端与状态：** /api/skill-imports/preview、apply → batch2.rs + skill_import.rs；源扫描、校验、导入；动态check/update/delete接口当前没有对应完整来源管理面板。

**实现状态：** 已接线；是否可用取决于下述前置条件。

## 11 万物转 MD 与批量导入

### F058 打开面板、选文件、选文件夹和覆盖策略

**入口/控件：** `btn-convert`、`w-convert`、`convert-close`、`convert-files`、`convert-folder`、`convert-overwrite`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 转换入口 → 万物转MD面板 → 选多个文件或目录。桌面多选原路径；浏览器多选先上传；选目录GET collect递归收集 → 自动开始批处理，无额外开始按钮。未勾覆盖时已有.md跳过，勾选覆盖时替换；图片分到OCR通道，其他到文档通道。处理中禁用新增文件/目录防止并行任务。

**前端实现：** [assets/js/features/convert.js:39 · pickConvertFiles](../../assets/js/features/convert.js)；[assets/js/features/convert.js:62 · pickConvertFolder](../../assets/js/features/convert.js)；[assets/js/features/batch.js:101 · enqueueBatchFiles](../../assets/js/features/batch.js)。

**后端与状态：** /api/dialog/choose-many-files、choose-folder、/api/upload、/api/convert/collect、batch；具体格式矩阵见 formats.md。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F059 任务行、进度、取消和结果目录

**入口/控件：** `convert-list`、`batch-cancel`、`convert-open-dir`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 每文件行显示排队/运行/成功/跳过/失败/取消；失败附错误说明。文档批量600ms轮询progress；图片逐张OCR并保存.md。点取消全部发送job取消并阻止后续OCR，包括首项尚未开始的OCR；正在进行的系统OCR调用不保证立即中断。成功或可打开的跳过行点开输出Markdown；打开结果目录优先用所选根目录，否则输出公共目录。关闭面板保留轮询与任务锁；重开显示同一批次，处理中禁止再提交一批。

**前端实现：** [assets/js/features/batch.js:59 · makeBatchRow](../../assets/js/features/batch.js)；[assets/js/features/batch.js:169 · pollBatchJob](../../assets/js/features/batch.js)；[assets/js/features/batch.js:335 · onBatchCancel](../../assets/js/features/batch.js)；[assets/js/features/batch.js:279 · showBatchOpenDir](../../assets/js/features/batch.js)。

**后端与状态：** /api/convert/progress、cancel；/api/ocr save=1&on_exists=skip/overwrite；/api/system/open-path。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F060 单文件导入与同名输出决策

**入口/控件：** `file-input`。

**操作流程与交互：** 从打开文件/拖放等进入单文件转换 → 检测已有输出 → 如需决策显示覆盖/改名/取消等动态选择（由具体调用路径生成）→ /api/convert返回content/engine/fixes/out/saved/skipped → 展示虚拟或实际文档，并反馈输出是否写入。批处理覆盖勾选与单文件on_exists决策是不同入口。 单文件转换/OCR/网页预览不自动写文件，未编辑关闭不提示；修改后提示，显式保存才创建 Markdown。

**前端实现：** [assets/js/features/convert.js:178 · convertOrOcr](../../assets/js/features/convert.js)；[assets/js/reader/render.js:3117 · convertFile](../../assets/js/reader/render.js)。

**后端与状态：** /api/convert?p=…&on_exists=skip|overwrite|rename；batch2.rs h_convert → convert.rs convert_triple → mdcheck与输出保存

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F061 文件拖入、目录拖入和ZIP拆包

**入口/控件：** `drag-overlay`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 拖入主阅读器或原生窗口 → 分类text/binary/folder/zip；文本直接打开，文档/图像转换或OCR，目录显示文件树；ZIP先解压到隔离临时目录、过滤条目，再把解压路径加入批量转换。浏览器无绝对路径则上传副本；桌面原生drop传完整路径；去重原生/HTML双事件。ZIP直接通过万物转换选中与拖ZIP的拆包流程不同。

**前端实现：** [assets/js/core/dragdrop.js:58 · handleDroppedEntries](../../assets/js/core/dragdrop.js)；[assets/js/core/dragdrop.js:21 · extractDroppedZip](../../assets/js/core/dragdrop.js)。

**后端与状态：** /api/batch/extract-zip、upload、file、convert/batch、ocr；convert.rs ZIP路径/大小/条数/CRC限制。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F100 音视频识别语言

**入口/控件：** `convert-speech-language`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 运行前选择自动/中文/英文/日文 → 保存在localStorage → 单文件和批次请求携带language → SAPI匹配安装的语言识别器 → 输出真实识别段落与时间戳；找不到语言模型明确失败。运行中锁定语言控件，关闭重开保持任务。

**前端实现：** [assets/js/features/convert.js:16 · initSpeechLanguage](../../assets/js/features/convert.js)；[assets/js/features/convert.js:11 · currentSpeechLanguage](../../assets/js/features/convert.js)。

**后端与状态：** /api/convert、/api/convert/batch、/api/transcribe → transcribe.rs/speech.rs，媒体解码复用本机FFmpeg。

**实现状态：** 已接线；是否可用取决于下述前置条件。

## 12 插件中心

### F062 插件搜索、分类和运行信息

**入口/控件：** `btn-open-plugins`、`plugin-close`、`plugin-search`、`plugin-installed-filter`、`plugin-refresh`。

**操作流程与交互：** 万物转MD或命令面板 → 插件中心 → 输入名称/说明/能力搜索，或切换分类、仅看已安装 → 安装、启停、卸载及查看错误。刷新重新读取真实状态；统计区按 installed 与无需安装的内置能力计算，不把可安装引擎误算为已安装。类别支持方向键/Home/End，搜索、滚动和焦点在状态刷新中保留；运行信息折叠。

**前端实现：** [assets/js/features/convert.js:209 · openPluginModal](../../assets/js/features/convert.js)；[assets/js/features/convert.js:410 · renderPluginCards](../../assets/js/features/convert.js)。

**后端与状态：** /api/plugins/list → plugin_manager.rs plugin_manifest/native_support；按实际installed/native标志渲染

**实现状态：** 插件入口在转换面板与命令面板；更多菜单注释中的 btn-plugin-menu 不可点击。

### F063 安装、重试及安装进度

**入口/控件：** `data-action=install`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 未安装扩展点安装/失败后点重试 → 确认 → POST install → 轮询进度 → 原子写plugins/native/id.json、验证内容、保存manifest并回读 → 卡片显示开关/卸载。发生写入错误恢复之前配置并显示错误详情。安装的是此Rust构建中的扩展配置，不下载或冒称安装Python第三方库。

**前端实现：** [assets/js/features/convert.js:582 · startPluginInstall](../../assets/js/features/convert.js)；[assets/js/features/convert.js:227 · refreshPluginList](../../assets/js/features/convert.js)。

**后端与状态：** /api/plugins/install → plugin_manager.rs install_plugin_async；原子安装、回读验证、manifest互斥与回滚。

**实现状态：** Rust原生扩展生命周期已实现；系统OCR/语音能力取决于宿主。

### F064 启用/停用和互斥切换

**入口/控件：** `data-action=toggle`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 检测到已安装插件才有开关 → change时锁住全部插件开关 → POST toggle → 保存manifest.enabled → 若同能力另一插件已启用则关闭它 → 重新读清单 → 已保存。失败回滚前端缓存并提示，不把UI变亮当保存成功。内置能力没有此开关，native能力不会因为旧包开关自动切换成Python引擎。

**前端实现：** [assets/js/features/convert.js:547 · setPluginToggle](../../assets/js/features/convert.js)。

**后端与状态：** /api/plugins/toggle → plugin_manager.rs set_plugin_enabled/save_manifest；同能力互斥持久化。

**实现状态：** 原生插件启停、互斥及manifest持久化已实现并实测；已有合法安装状态时可操作。

### F065 卸载及错误详情

**入口/控件：** `data-action=uninstall`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 检测到installed卡片 → 点卸载 → 危险确认 → POST uninstall → 刷新list并反馈。后端检查忙碌/安装状态、删除沙箱内对应包文件和manifest记录，不删除Rust编译内置能力。错误详情summary可展开查看已返回诊断。

**前端实现：** [assets/js/features/convert.js:616 · startPluginUninstall](../../assets/js/features/convert.js)；[assets/js/features/convert.js:369 · pluginErrorMarkup](../../assets/js/features/convert.js)。

**后端与状态：** /api/plugins/uninstall → plugin_manager.rs uninstall_plugin；沙箱文件/锁/记录。

**实现状态：** 真实原生扩展卸载已通过端到端；错误详情/运行占用保护保留。

## 13 OCR、网页提取和剪贴板

### F066 扫描识别入口

**入口/控件：** `btn-ocr`、`w-ocr`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 选择图片/PDF → 单文件任务去重与busy反馈 → 按需加载OCR模块 → GET ocr → 内容/fixes展示新文档；多图逐张处理，批处理通道可写.md。无文字返回empty/提示，失败区分模块不可用、文件/格式等。Windows使用本机OCR能力；是否可用以 pick_engine/模块状态为准。

**前端实现：** [assets/js/features/convert.js:120 · chooseFile](../../assets/js/features/convert.js)；[assets/js/features/convert.js:98 · ocrFile](../../assets/js/features/convert.js)。

**后端与状态：** /api/ocr → parity_web.rs + ocr.rs；ocr_winrt.rs WindowsOCR及PDF页渲染/布局归一化。

**实现状态：** 条件功能：系统OCR语言包及PDF栅格化资源；不是PythonOCR插件已接入。

### F067 网页表单、粘贴、抓取页数和图片

**入口/控件：** `btn-web`、`w-web`、`url-close`、`url-input`、`url-paste-btn`、`url-pages-dec`、`url-pages`、`url-pages-inc`、`url-images`、`url-private`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 网页入口 → 填URL/粘贴剪贴板网址 → 抓取页数1–30（±、输入或滚轮）→ 勾下载图片 → 可见“私有页面”选项按宿主分支授权。页数>1触发同站链接抓取，而不是任意全站爬虫；输入Enter等同智能提取。网址无协议补https。

**前端实现：** [assets/js/features/web.js:243 · openWebDialog](../../assets/js/features/web.js)；[assets/js/features/web.js:150 · webToMd](../../assets/js/features/web.js)。

**后端与状态：** /api/clipboard/read、/api/web/extract；桌面私有授权经main.rs nativeWeb IPC → desktop_web.rs隔离系统WebView；普通浏览器缺少宿主时显示能力提示。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F068 智能提取、完整动态渲染及取消

**入口/控件：** `url-go`、`url-render`、`url-cancel`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 智能提取 → 静态HTTP获取/正文清洗 → 成功显示虚拟文档、来源、资源和warning；需要JS时尝试宿主隔离系统WebView，完整动态渲染强制使用该窗口。私有页面先在隔离窗口登录并显式授权此站点，再带任务限定grant提取实际DOM；交互模式点提取此页。取消同时设置本地标记、取消HTTP任务及宿主窗口；已抓到页时保留部分成果与统计，批次结束撤销grant。超时或窗口×终止该任务；普通浏览器缺少宿主时提示能力不足。关闭面板的处理见closeWebDialog。

**前端实现：** [assets/js/features/web.js:69 · extractOneWebPage](../../assets/js/features/web.js)；[assets/js/features/web.js:135 · cancelWebTask](../../assets/js/features/web.js)。

**后端与状态：** /api/web/extract、cancel → parity_web.rs/headless_renderer.rs；desktop_web.rs隔离系统WebView窗口，DOM捕获、任务授权、取消、超时、撤销授权。

**实现状态：** 静态/动态/任务私有授权已接Rust；动态窗口需要桌面宿主。

### F069 剪贴新建与多类型分流

**入口/控件：** `btn-clipboard-new`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 点击剪贴新建 → 显式读取剪贴板 → 文件按格式打开/转换，截图OCR；富文本HTML通过Rust本地HTML解析保留标题/强调/列表并去除脚本；单一网址填网页面板，纯文本建立虚拟文档。失败或无正文有反馈，转换不执行HTML脚本、不自动抓取资源。

**前端实现：** [assets/js/features/clipboard.js:6 · createFromClipboard](../../assets/js/features/clipboard.js)。

**后端与状态：** /api/clipboard/read → native_system.rs；/api/clipboard/convert-html → headless_renderer.rs；其它分流复用file/ocr/convert。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F101 隔离网页窗口授权与提取

**入口/控件：** `readmd-capture-bar`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 动态/私有抓取打开隔离系统WebView；私有页面可登录后点授权此站点，产生限定任务、来源与时效的grant。交互抓取窗口点提取此页交回实际DOM，去除脚本和输入值。窗口×取消该job；超时终止；取消渲染与撤销授权独立处理，整批结束撤销授权。外部网页没有阅读器API桥，不继承阅读器cookies。

**前端实现：** [rust/readmd-kernel/src/desktop_web.rs:158 · capture_script](../../rust/readmd-kernel/src/desktop_web.rs)；[assets/js/features/web.js:135 · cancelWebTask](../../assets/js/features/web.js)。

**后端与状态：** main.rs nativeWeb/render_web_page/authorize_private_web → desktop_web.rs Manager，临时配置文件隔离；提取结果/api/web/extract。

**实现状态：** 已接线；是否可用取决于下述前置条件。

## 14 多格式导出、预设和预览

### F070 导出格式切换

**入口/控件：** `btn-print`、`export-close`、`export-tab-pdf`、`export-tab-docx`、`export-tab-epub`、`export-tab-html`、`export-tab-tex`、`export-tab-presentation`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 打开有内容文档 → 导出 → PDF/DOCX/EPUB/HTML/LaTeX/演示六页签 → 切换只展示适用配置组 → 内容优先取当前编辑内容而非只取旧磁盘文件。×关闭配置；导出只在点主按钮后执行。全部98个可输入排版参数与1条说明、范围及默认值见 exports.md。

**前端实现：** [assets/js/features/export.js:209 · openExportModal](../../assets/js/features/export.js)；[assets/js/features/export.js:230 · currentExportContent](../../assets/js/features/export.js)；[assets/js/features/export.js:1040 · renderExportSections](../../assets/js/features/export.js)。

**后端与状态：** /api/export/presets GET；导出后端按格式分流。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F071 预设选择、重置、自定义保存

**入口/控件：** `exp-preset`、`exp-save-preset`、`exp-reset`、`exp-save-input`、`exp-save-ok`、`exp-save-cancel`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 展开全部预设 → 选极简/经典/商务或用户预设；动态预设卡也能加载 → 合并默认参数/预设 → 重绘字段和预览。改字段变成自定义；重置恢复默认；保存当前预设展开命名表单 → 校验名称 → 写入custom预设并刷新选择。取消命名不改已有配置。浏览器缺少native预设bridge时使用对应前端回退行为，持久范围以源码为准。

**前端实现：** [assets/js/features/export.js:181 · loadExportPresets](../../assets/js/features/export.js)；[assets/js/features/export.js:1201 · renderExportPresetSelect](../../assets/js/features/export.js)；[assets/js/features/export.js:1426 · expSavePreset](../../assets/js/features/export.js)。

**后端与状态：** /api/export/presets GET/POST → export_styles.rs + server.rs h_export_presets；合并custom/last。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F072 版式设置、实时预览和大预览

**入口/控件：** `export-opts`、`export-preview-card`、`export-preview-prev-btn`、`export-preview-next-btn`、`export-preview-close`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 展开定制版式 → 改字段/预设 → 收集同一份options。PDF请求后端生成真实导出产物并显示其页面；大预览按实际PDF页数翻页。内容/设置相同的导出复用缓存产物，PDF字节一致。DOCX同时生成实际文档及共享内容/参数的PDF版式参考，页面明确提示Word分页为准；HTML保留CSS预览。请求切换与关闭会取消旧请求、丢弃过期响应。

**前端实现：** [assets/js/features/export.js:1040 · renderExportSections](../../assets/js/features/export.js)；[assets/js/features/export.js:902 · updateExportLivePreview](../../assets/js/features/export.js)；[assets/js/features/export.js:789 · requestNativeExportPreview](../../assets/js/features/export.js)。

**后端与状态：** /api/export/preview → export_preview.rs/mdexport.rs；同输入的/api/export复用产物；页面栅格化ocr_winrt.rs。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F073 AI 排版生成

**入口/控件：** `exp-ai-prompt`、`exp-ai-gen-btn`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 输入排版要求 → 生成/Enter → 使用共享AI连接和readmd-export-style技能 → 解析JSON并归一化白名单 → 合并当前options → 设为自定义 → 即时字段/预览更新。失败显示状态和Toast；不会自动导出文档。

**前端实现：** [assets/js/features/export.js:1491 · generateExportStyleWithAi](../../assets/js/features/export.js)；[assets/js/features/export.js:1169 · normalizeExportAiPayload](../../assets/js/features/export.js)。

**后端与状态：** /api/ai/chat；返回结构化排版配置而非任意代码。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F074 执行、取消、打开结果和定位

**入口/控件：** `export-run`、`export-cancel`、`export-open`、`export-reveal`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 点导出 → ReadMDTask防重复并显示忙状态 → 桌面选择输出位置 → 按格式导出 → 显示路径/警告。通用导出带task_id可取消，尽可能临时文件提交；TeX有资源目录提交点限制，EPUB不提供同级取消。完成后桌面可打开文件/资源管理器定位；路径已移动显示错误。关闭弹窗与任务取消是不同动作。

**前端实现：** [assets/js/features/export.js:1288 · runExportOnce](../../assets/js/features/export.js)。

**后端与状态：** /api/export → mdexport.rs；EPUB /api/export/epub；演示Rust经/export写离线HTML，浏览器fallback /api/export/presentation；取消 /api/task/cancel。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F075 系统浏览器打印

**入口/控件：** `export-print`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 导出面板相应入口 → 打开打印视图/调用浏览器打印 → 用户在系统对话框选打印机或另存PDF；它和Rust直接生成PDF路径不同，输出由浏览器打印引擎决定。

**前端实现：** [assets/app.js:81 · bindEvents](../../assets/app.js)。

**后端与状态：** 前端window.print/宿主打印桥按当前绑定调用；不计为第七种格式。

**实现状态：** 已接线；是否可用取决于下述前置条件。

## 15 演讲演示与局域网共享

### F076 演示开启与播放器

**入口/控件：** `btn-presentation-menu`、`presentation-modal`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 当前Markdown → 更多/互动/演示或F5 → POST编译幻灯片HTML → 模态iframe播放器。主题11项、转场6项；A−20px/A24px/A+28px；总览、全屏、退出全屏、关闭；Reveal内部还有上一页/下一页、垂直页、进度点击、Esc总览等条件控件。主题和全屏状态同步到iframe；退出恢复原阅读器。

**前端实现：** [assets/js/reader/render.js:2690 · launchPresentationMode](../../assets/js/reader/render.js)；[assets/js/reader/render.js:2609 · togglePresentationFullscreen](../../assets/js/reader/render.js)。

**后端与状态：** /api/export/presentation → mdexport.rs render_presentation_html；本地Reveal资源+presentation-bootstrap.js。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F077 开启/关闭共享与扫码访问

**入口/控件：** `btn-share`、`share-start`、`share-stop`、`share-close`、`share-refresh`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 打开共享 → 加载/刷新状态 → 开启服务 → 含访问令牌的二维码/地址 → 手机同网访问可点击目录 → Markdown 阅读、链接/图片、下载源文件与回到上级。未保存文档或编辑草稿使用隔离临时快照；磁盘文档共享所在目录。关闭共享停止该会话；重新开启不会恢复旧监听。失败不提示成功，重复点击单次执行，旧状态/二维码不能覆盖最新结果；×仅收起面板。

**前端实现：** [assets/js/features/share.js:105 · startShare](../../assets/js/features/share.js)；[assets/js/features/share.js:51 · refreshShareStatus](../../assets/js/features/share.js)。

**后端与状态：** /api/share/start、status、stop → server.rs 共享状态/监听/令牌访问控制；QR由vendor qrcode生成。

**实现状态：** 已接线；是否可用取决于下述前置条件。

## 16 知识图谱和双向链接

### F078 图谱打开与视图控制

**入口/控件：** `btn-graph`、`graph-btn-close`、`graph-btn-reset`、`graph-btn-zoom-in`、`graph-btn-zoom-out`、`graph-btn-dimension`、`graph-btn-fx`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 有当前文档 → 图谱/命令Ctrl+G → 文件文档先索引目录再读图谱，虚拟文档建立当前内容图 → Canvas显示关联网络；重置视角、放大/缩小、2D/3D切换、特效开关、关闭。特效偏好保存在readmd-graph-fx；无双链常驻按钮可能隐藏，命令可在有文档时调用。

**前端实现：** [assets/js/features/graph.js:666 · open](../../assets/js/features/graph.js)；[assets/js/features/graph.js:100 · createModal](../../assets/js/features/graph.js)。

**后端与状态：** /api/links/index、graph → link_indexer.rs + store.rs；力布局/绘图前端。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F079 图谱筛选、节点选择与手势

**入口/控件：** `graph-search`、`graph-labels`、`graph-neighbors`、`graph-canvas`、`graph-node-list`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 输入节点/别名过滤列表，Enter选首个命中；显示名称开关、仅邻近关联开关；点节点/列表选中，详情有打开笔记；双击节点打开文件。拖节点牵引、拖背景旋转、Shift拖平移、滚轮缩放；键盘方向旋转、±缩放、0重置、Enter打开选中笔记。Esc先取消选择再关闭。

**前端实现：** [assets/js/features/graph.js:243 · renderList](../../assets/js/features/graph.js)；[assets/js/features/graph.js:179 · onPointerDown](../../assets/js/features/graph.js)；[assets/js/features/graph.js:264 · select](../../assets/js/features/graph.js)。

**后端与状态：** 图交互前端；打开笔记 /api/file。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F080 反向链接抽屉

**入口/控件：** `btn-backlinks-menu`、`backlinks-btn-close`、`backlinks-search`、`data-tab=incoming`、`data-tab=outgoing`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 更多/互动/反向链接 → incoming反向引用/outgoing正向链接两页签 → 搜索过滤路径/别名/上下文 → 点有效笔记行打开；未创建目标显示deadlink并禁用 → ×关闭。文档切换刷新关联；有虚拟文档但无磁盘路径时不能生成真实文件反向索引。

**前端实现：** [assets/js/features/graph.js:696 · createDrawer](../../assets/js/features/graph.js)；[assets/js/features/graph.js:725 · refreshBacklinks](../../assets/js/features/graph.js)。

**后端与状态：** /api/links/backlinks → link_indexer.rs/store.rs。

**实现状态：** 已接线；是否可用取决于下述前置条件。

## 17 修复报告、样式定制和系统设置

### F081 自动修复报告和修复副本

**入口/控件：** `btn-fix`、`fix-save`、`fix-close`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 读取/转换后的fixes不为空或有内容时 → 修复详情 → 列每条自动修正文本及数量 → 另存修复版调用save_fixed；源文件名旁生成.readmd后缀副本，原文件保留；关闭返回阅读。修复列表是说明文本，不存在逐条“接受/拒绝”按钮。

**前端实现：** [assets/js/reader/fixes.js:8 · showFixModal](../../assets/js/reader/fixes.js)。

**后端与状态：** /api/file 或convert产生fixes；/api/file/save-fixed → server.rs；readmd_fix.rs/formula_repair.rs。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F082 AI 深度排版自愈

**入口/控件：** `fix-ai-btn`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 有文档 → AI 深度排版 → 单次请求。只有来源标签、编辑器实例和原文仍一致时才以可撤销事务应用；原文变化、换标签或阅读模式时建立未保存 AI 副本，保留原文件及当前草稿。HTTP/业务失败和空结果均明确反馈；不会修改干净基线来伪装成已保存。 保存、副本、转换输出和跨会话恢复的最终策略详见文档生命周期清单；恢复记录集中存储，不再新增旁边.bak。

**前端实现：** [assets/js/reader/fixes.js:33 · handleAiDocumentFix](../../assets/js/reader/fixes.js)。

**后端与状态：** /api/ai/chat 使用readmd-format-fix技能；最终保存 /api/save。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F083 样式预设、CSS/Head和AI生成

**入口/控件：** `btn-style-custom`、`style-modal-close`、`btn-preset-indent`、`btn-preset-table`、`btn-preset-font`、`btn-preset-print`、`style-ai-prompt`、`style-ai-gen-btn`、`style-custom-css`、`style-custom-head`、`style-modal-cancel`、`style-modal-save`、`style-load-retry`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 打开样式面板 → 加载状态/失败重试 → CSS、Head、模板及 AI 生成 → 保存后应用 CSS。加载时锁定字段，旧加载不能覆盖新窗口；保存单次执行，失败保留输入；保存中继续编辑不会被关闭或覆盖。保存是主按钮，AI 生成是辅助操作；外部 Head 资源仍取决于用户声明的资源条件。

**前端实现：** [assets/app.js:1318 · openStyleModal](../../assets/app.js)；[assets/app.js:1350 · saveStyleModal](../../assets/app.js)。

**后端与状态：** /api/style/get、save → server.rs；AI /api/ai/chat 使用readmd-style-custom。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F084 界面语言、默认关联和开机自启

**入口/控件：** `btn-lang`、`lang-modal-close`、`lang-search-input`、`btn-assoc`、`btn-autostart`。

**操作流程与交互：** 语言 → 搜索并选择语言 → 即时重翻界面。默认打开方式 → 注册用户级 ReadMD ProgId、Capabilities、OpenWith 与引用命令 → 打开 Windows 默认应用设置 → 用户选择各 Markdown 扩展名 → 只读状态接口核对系统实际打开程序；注册不代表已设为默认，不改受保护 UserChoice。开机自启开关调用原有 API 保存并回读。浏览器/非Windows说明平台条件。

**前端实现：** [assets/js/core/i18n.js:1](../../assets/js/core/i18n.js)；[assets/js/core/history.js:514 · installAssoc](../../assets/js/core/history.js)；[assets/js/core/settings.js:155 · toggleAutostart](../../assets/js/core/settings.js)。

**后端与状态：** /api/system/assoc POST {op:register|status}、/api/autostart/get、set；win_registry.rs modern_association_writes/default_executable、native_system.rs shell_open；四种扩展名 .md/.markdown/.mdown/.mkd。

**实现状态：** 已接线；是否可用取决于下述前置条件。

## 18 应用更新

### F085 检查、下载、取消、应用和浏览器查看

**入口/控件：** `btn-check-update`、`status-update-badge`、`update-close`、`update-use-mirror`、`btn-update-browser`、`btn-update-cancel`、`btn-update-start`。

**操作流程与交互：** 启动后静默检查或手动检查（并发合并） → GitHub API/发布重定向/镜像回退 → 显示新版本 → 首选镜像可切换 → 下载候选源故障或校验失败时自动尝试下一源 → SHA256 校验 → 保存/放弃/取消未保存修改 → 安装器启动或便携版事务替换 → 退出并重新打开。检查失败按30秒/2分钟/10分钟重试，联网恢复立即重查；检查整体45秒限时。取消不发布未验证包，安装失败可重试；替换失败保留原版本并给出反馈。

**前端实现：** [assets/js/features/updater.js:24 · checkUpdate](../../assets/js/features/updater.js)；[assets/js/features/updater.js:149 · startUpdateDownload](../../assets/js/features/updater.js)；[assets/js/features/updater.js:278 · cancelUpdateDownload](../../assets/js/features/updater.js)。

**后端与状态：** /api/update/check、download、status、cancel、apply → updater.rs 代理/回退/检查；batch2.rs 下载与哈希门；update_install.rs Windows安装器/事务替换/一份回滚文件/辅助程序清理；文档草稿由 document-history.js 保护。

**实现状态：** 真实网络元数据检查和受控失败回退通过；未下载或安装真实发布包，未替换用户安装。。

## 19 桌宠设置、角色库和伴读互动

### F086 桌宠工作台、预览和页签

**入口/控件：** `btn-pet`、`pet-settings-close`、`data-pet-section`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 设置/桌宠 → 加载运行状态与角色库 → 左侧完整角色预览、启用开关 → 角色/陪伴/设置三页签。切页签只切对应内容；预览使用精灵图完整帧/Live2D模型或Bongo预览资源，实际桌面窗口和设置预览不是同一个渲染实例。

**前端实现：** [assets/js/features/pet-batch.js:1116 · openPetSettings](../../assets/js/features/pet-batch.js)；[assets/js/features/pet-batch.js:963 · updateCharacterPreview](../../assets/js/features/pet-batch.js)；[assets/js/features/pet-workbench.js:98 · init](../../assets/js/features/pet-workbench.js)。

**后端与状态：** /api/pets/status、/api/pets、上游角色目录本地资源；pet_catalog.rs/manifest数据。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F087 角色搜索、分类渲染器、收藏和选择

**入口/控件：** `pet-renderer`、`pet-roster-search`、`pet-gallery`、`pet-roster`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 角色页 → 精灵图/Live2D筛选 → 搜索名称/标识 → 点击角色卡选择，星形收藏单独切换不选中角色。精灵图和Live2D各走其配置路径，选择中锁定防并发，失败恢复并显示反馈。库下拉可选伴读使者、BongoCat及当前库/目录合并项。角色目录不是每个都已经导入用户库。

**前端实现：** [assets/js/features/pet-workbench.js:258 · renderRoster](../../assets/js/features/pet-workbench.js)；[assets/js/features/pet-workbench.js:220 · choose](../../assets/js/features/pet-workbench.js)；[assets/js/features/pet-batch.js:1611 · refreshPetGallery](../../assets/js/features/pet-batch.js)。

**后端与状态：** /api/pets/active、configure；parity_pets.rs + desktop_pet.rs；收藏前端localStorage。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F088 导入/删除自定义精灵图

**入口/控件：** `pet-gallery-import`、`pet-gallery-delete`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 角色库导入PNG → 前端文件大小17MiB检查/转base64 → 后端验证图尺寸/布局和slug → 入库刷新；删除仅自定义项显示 → 危险确认 → remove → 刷新/必要时回到默认角色。内置角色不能用此删除按钮卸载；缩略图由thumb接口或本地资源加载。

**前端实现：** [assets/js/features/pet-batch.js:1205 · initPetSystem](../../assets/js/features/pet-batch.js)；[assets/js/features/pet-batch.js:1599 · updatePetDeleteButtonVisibility](../../assets/js/features/pet-batch.js)。

**后端与状态：** /api/pets/import、remove、thumb → parity_pets.rs；图资源与目录边界检查。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F089 运行位置、启用、尺寸、透明度和置顶

**入口/控件：** `pet-runtime`、`pet-enabled`、`pet-scale`、`pet-opacity`、`pet-topmost`、`pet-lock-position`、`pet-sound`、`pet-reset-pos`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 设置页 → 阅读器内/独立桌面 → 启用 → 大小8–48（默认22，逻辑尺寸再由角色渲染器计算）→ 不透明度 → 保持置顶/锁定位置/Bongo按键音效 → onchange自动保存；range input即时数字/预览，change提交。重置位置重设widget/宿主相应位置。置顶是可选项；锁定阻止拖动。音效只对Bongo机制有意义。

**前端实现：** [assets/js/features/pet-batch.js:1047 · capturePetSettings](../../assets/js/features/pet-batch.js)；[assets/js/features/pet-batch.js:1077 · applyPetSettings](../../assets/js/features/pet-batch.js)；[assets/js/features/pet-batch.js:565 · resetWidgetPosition](../../assets/js/features/pet-batch.js)。

**后端与状态：** /api/pets/configure → parity_pets.rs/desktop_pet.rs/pet_window_state.rs；保存preferences并配置独立宿主。

**实现状态：** 阅读器内/桌面能力不同；桌面需可用runtime。

### F090 安装/卸载桌宠扩展与运行时更新

**入口/控件：** `pet-install-runtime`、`pet-install`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 独立桌面模式显示运行时与扩展动作 → 一键安装从本地随包资源/配置路径准备runtime → 安装桌宠扩展；已安装按钮变卸载，卸载前确认。运行时按钮在可更新状态转检查更新 → allow_network:true → 发现版本确认 → apply_update，400ms轮询进度 → 刷新状态。桌宠扩展的生命周期和pip插件中心是两套后端。

**前端实现：** [assets/js/features/pet-batch.js:1577 · installDefaultPetRuntime](../../assets/js/features/pet-batch.js)；[assets/js/features/pet-batch.js:1205 · initPetSystem](../../assets/js/features/pet-batch.js)。

**后端与状态：** /api/pets/runtime/install、install、uninstall、check_update、apply_update、update_status → parity_pets.rs/pet_launcher.rs。

**实现状态：** 现有离线包实际安装条件测试已通过；运行时卸载/更新按平台状态，未实际升级用户安装。

### F091 陪伴风格、气泡和养成动作

**入口/控件：** `pet-mode-social`、`pet-mode-quiet`、`pet-bubble-toggle`、`pet-say-hello`、`pet-chat-open`、`data-pet-action`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 陪伴页 → 活跃/安静风格 → 开关伴读提示气泡 → 打招呼、打开AI聊天；摸摸头、喂食、玩耍、休息/唤醒五种动作 → /interact → 更新等级/体力/心情/亲密度、状态和台词。休息/唤醒按resting只显示适用项；冷却或体力不足明确反馈；安静模式不等于关闭所有交互。

**前端实现：** [assets/js/features/pet-companion-actions.js:40 · init](../../assets/js/features/pet-companion-actions.js)；[assets/js/features/pet-workbench.js:203 · quiet](../../assets/js/features/pet-workbench.js)。

**后端与状态：** /api/pets/interact、configure → parity_pets.rs 持久化companion状态/冷却/恢复。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F092 阅读器内点击、拖动和快捷条

**入口/控件：** `pet-character-wrap`、`pet-quick-settings`、`pet-quick-hide`、`pet-quick-chat`、`pet-quick-quiet`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 阅读器内角色 → 单击播放原图动作及时段/角色台词，连续戳触发组合反馈；按住超阈值拖动改变位置，锁定时受限。快捷条：设置、收起、AI、安静切换；收起保存disabled状态。气泡点击/交互、阅读进度提醒、空闲/打盹/睡眠属于有状态的陪伴机制，不强制给所有角色加键盘。

**前端实现：** [assets/js/features/pet-batch.js:576 · initPetDirectManipulation](../../assets/js/features/pet-batch.js)；[assets/js/features/pet-batch.js:706 · handlePetInteractiveClick](../../assets/js/features/pet-batch.js)；[assets/js/features/pet-batch.js:309 · initBubbleInteractions](../../assets/js/features/pet-batch.js)。

**后端与状态：** /api/pets/configure + 前端位置/状态；阅读器内输入反馈仅当前页面事件。

**实现状态：** 已接线；是否可用取决于下述前置条件。

## 20 独立桌宠窗口和快捷菜单

### F093 原生拖动、透明区域和Bongo输入动作

**入口/控件：** `native-pet-window`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 角色实体拖动超4px阈值触发原生窗口拖拽；透明外围按alpha命中区穿透；锁定位置禁拖。Bongo按键、鼠标左右键/移动触发左右手/键鼠反馈与可选声音；精灵图用自身动作帧、朝向、闲置和完成反馈，Live2D用模型视线/动作/表情，不假设每个角色都有键盤。

**前端实现：** [packages/readmd-pet-rust/renderer/bongocat.js:1](../../packages/readmd-pet-rust/renderer/bongocat.js)；[packages/readmd-hermes-pet-adapter/src/live2d/stage.ts:1](../../packages/readmd-hermes-pet-adapter/src/live2d/stage.ts)。

**后端与状态：** Rust pet_host.rs/desktop_pet.rs/native输入与窗口事件；renderer通过控制协议配置；仅实际runtime启动时有效。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F094 右键快捷菜单、角色子菜单和互动气泡

**入口/控件：** `native-pet-context-menu`、`native-pet-bubble`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 右键实体 → ReadMD、剪贴板、角色、玩耍、设置、隐藏六项；角色子菜单有返回和每个角色，标记当前选择。点击宠物打开互动气泡 → 摸头、喂食、玩耍、休息/唤醒、AI聊天、关闭。菜单/气泡打开时扩大有效点击区，关闭后恢复实体范围；取消/外部点/键盘处理按renderer事件。

**前端实现：** [packages/readmd-pet-rust/renderer/bongocat.js:1](../../packages/readmd-pet-rust/renderer/bongocat.js)；[packages/readmd-hermes-pet-adapter/src/pet-life.ts:1](../../packages/readmd-hermes-pet-adapter/src/pet-life.ts)。

**后端与状态：** pet_host.rs 控制消息open-app/toggle-app/character/interact/hide；/api/control/pet-menu、pets/interact/configure；主阅读器轮询消费。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F095 桌宠拖入文件与跨进程唤起

**入口/控件：** `native-pet-drop`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 从系统文件管理器拖文件到独立桌宠 → 原生取得完整路径 → 写入耐久命令/批次队列 → 唤起主ReadMD → 主页面控制轮询消费 → Markdown打开；图片OCR；文档/多文件批量转换。队列传路径/任务而不是仅传浏览器File.name；ZIP/目录按相应可识别分派处理。

**前端实现：** [assets/js/features/pet-batch.js:178 · receivePetBatch](../../assets/js/features/pet-batch.js)；[assets/js/features/pet-batch.js:1160 · pollPetControls](../../assets/js/features/pet-batch.js)；[packages/readmd-pet-rust/renderer/bongocat.js:1](../../packages/readmd-pet-rust/renderer/bongocat.js)。

**后端与状态：** /api/control/pet-batch、pet-menu、next + pet_queue/pet_host；复用转换/OCR/文件入口。

**实现状态：** 已接线；是否可用取决于下述前置条件。

## 21 命令面板、快捷键和通用对话框

### F096 全局命令面板及快捷键查询

**入口/控件：** `btn-palette`、`readmd-palette`、`shortcuts-modal`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 按钮/Ctrl+K打开 → 输入模糊查询 → 可点命令、最近文件或设置 → Enter执行；上下导航、Esc关闭；最近使用命令参与排序。快捷键说明可打开并搜索，展示与当前编辑/文档条件有关的命令。全部注册记录逐项见 commands.md；hidden项和条件不可用项单独注明。

**前端实现：** [assets/js/shell/shell-deferred.js:1](../../assets/js/shell/shell-deferred.js)；[assets/js/shell/shell.js:32 · onGlobalKey](../../assets/js/shell/shell.js)。

**后端与状态：** 复用各具体功能；MRU、搜索与键位显示前端localStorage。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F097 保存、冲突、选择、确认和连续模式提示

**入口/控件：** `close-confirm-save`、`close-confirm-discard`、`close-confirm-cancel`、`save-conflict-save-as`、`save-conflict-reload`、`save-conflict-cancel`、`confirm-action`、`confirm-cancel`、`continuous-confirm`、`continuous-cancel`、`choice-modal`。动态类名/委托属性和容器名是定位标识，不代表都存在同名按钮。

**操作流程与交互：** 通用弹层统一管理打开顺序、顶层 Escape、焦点循环/归还及背景 inert；保留保存、冲突、选择、确认和持续任务关闭保护。动态弹层同样登记，inert 控件不会参与循环。所有静态弹窗统一 44px 目标、滚动可达的表单和主题/焦点/滚动条样式；在两种指定窗口、三种语言逐面板检查。 保存、副本、转换输出和跨会话恢复的最终策略详见文档生命周期清单；恢复记录集中存储，不再新增旁边.bak。

**前端实现：** [assets/js/core/modal.js:1](../../assets/js/core/modal.js)；[assets/js/core/dialog.js:5 · confirmAction](../../assets/js/core/dialog.js)；[assets/js/core/tabs.js:620 · askChoice](../../assets/js/core/tabs.js)；[assets/js/editor/preview.js:629 · promptSaveConflict](../../assets/js/editor/preview.js)。

**后端与状态：** 对话框前端；确认后由调用者进入save/rename/convert/plugin/skill等对应后端。

**实现状态：** 已接线；是否可用取决于下述前置条件。

### F104 窗口控制、托盘驻留与退出恢复

**入口/控件：** `window-minimize`、`window-maximize`、`window-close`、`window-drag-region`、`btn-close-to-tray`、`btn-app-exit`、`native-window-close`、`request_quit`。

**操作流程与交互：** 应用自绘顶栏：拖动空白区移动窗口，双击最大化/还原，边缘调整大小；按钮最小化、最大化/还原、关闭。打开/目录/最近/命令/主题/更多集中在顶栏，下一行只保留文档工具。默认关闭驻留 Windows 系统托盘，标签与编辑器保持运行；托盘点击恢复，右键打开文件或完全退出。更多菜单可关闭驻留并保存偏好。重复启动恢复原窗口，文件启动直接交给已驻留实例。完全退出仅对真实修改提示保存/不保存/取消；取消保留编辑器，放弃先留恢复记录；无修改直接退出。

**前端实现：** [assets/js/shell/shell.js:99 · initWindowChrome](../../assets/js/shell/shell.js)；[assets/js/shell/shell.js:92 · syncWindowPreferences](../../assets/js/shell/shell.js)；[assets/js/features/document-history.js:192 · prepareClose](../../assets/js/features/document-history.js)；[assets/js/core/tabs.js:737 · closeAllTabs](../../assets/js/core/tabs.js)。

**后端与状态：** main.rs Wry/Tao Window IPC 与 CloseRequested；native_tray.rs Shell_NotifyIcon/TaskbarCreated/重试；server.rs 外部打开立即唤醒；settings.json closeToTray；恢复与保存 API；仅同源阅读器可调用宿主 IPC。

**实现状态：** 真实 Windows 窗口、托盘回调、热唤起、文件交接及退出保护通过；浏览器隐藏原生控制按钮。。

## 核对方法和结论边界

本次先扫描index.html实际非注释控件，再追踪app.js集中绑定、所有assets/js模块、延迟Shell/阅读增强、独立桌宠renderer与Rust路由。补充了createElement、事件委托、源码属性和运行时列表，按控件族登记数量可变项；providers/roles/skills是数据目录，和操作种类分别统计。

用独立临时数据目录运行现有Rust二进制和当前工作树assets；Chromium以1160×820和1024×680依次打开转换、插件、桌宠、网页、Skill、AI设置、语言和快捷键面板；另读取六种导出动态字段、建立临时虚拟文档、确认CodeMirror初始化和斜杠菜单目录。没有使用用户AI密钥，没有执行安装/卸载/更新/覆盖保存/发送AI/分享开服等动作。运行抽查未出现pageerror；这一结果**不等于所有按钮都点过、所有弹层像素无遮挡或所有后端业务成功**。

本次仅添加盘点文档和数据，应用实现未改动，因此没有重新打包前端或执行全套应用回归测试。各CSV/JSON、控件计数和源码行链接经过生成后校验；可按这些清单再制定逐功能人工/自动验收用例。
