# 101项功能真实实现复查与面板完善记录

> 后续保存与恢复升级：总清单已扩展为104流程、391静态控件、78动态交互族、67命令、100路由。下面101项及250项UI结果是此前已完成审计的历史记录；当前存储行为、56项体验细则和最终回归请查看[文档生命周期清单](readmd-document-lifecycle-ux-2026-10-03.md)。机器可读 verification.json/csv 已补齐新增3项并刷新源码位置。

日期：2026-10-03；工作树基线`05ec02d`。按 [完整层级清单](readmd-ui-function-inventory-2026-10-02.md) 逐条核对，覆盖21层级、101流程、389静态控件、74类动态交互。每项保留操作流程、前端实现、后端与验证边界；没有因缺少外部环境而删除设计入口。源码、测试、离线包与文档更新保留在工作树，未提交或推送。

**实现入口与验证证据已全部核对；当前通过的测试不等于可以保证所有环境永不出错。** 浏览器、原生宿主、系统识别器、外部模型和在线站点的条件分别列在每项中。相关测试文件说明覆盖到的路径，不表示文件内每个测试均对应本行，或本行每个按钮都已自动点击。所有检查使用隔离临时数据，不复制密钥或remote令牌。

## 本轮修复与面板优化


| 范围 | 最终行为 |
| --- | --- |
| 文档/标签/主页 | 返回主页先处理保存/放弃/取消；保留虚拟草稿、空原文和脏状态；编辑器延迟加载不能跨标签挂错。选择器错误捕获并反馈。 |
| AI改写与修复 | 来源标签/编辑器/正文身份核对；关闭作废迟到结果；并发单次执行；来源变化保存未存盘副本；应用独立可撤销。textarea兜底读取实际草稿。 |
| 目录/设置 | 加载失败保留当前树，旧响应不能覆盖新目录；设置白名单/范围校验；原生写入串行化，失败可见。 |
| 代码/图表/表格/导入 | 拒绝空代码；围栏保护内嵌反引号；保留图表草稿；表格支持方向键/Enter。@import真实执行mode/lines，前后端一致校验。 |
| YAML元数据 | 读取既有值，正确转义；保留未知/文献/嵌套字段；支持行内映射；过期修改和复杂锚点不静默破坏。 |
| 手机共享 | 认证二维码；实际可点击目录/Markdown阅读/源下载；未保存草稿隔离快照；停止/重启按会话隔离，错误反馈/刷新/防重复请求。 |
| 样式与更新 | 加载反馈/重试和保存锁；保存中新增输入不被旧响应关闭。更新轮询不重叠、错误可恢复；已验证安装包失败可重试而不重复下载。 |
| 全体弹层 | 统一44px目标、表单滚动、勾选/聚焦风格、窄窗按钮换行；表格、URL输入和会话按钮专项修正。排除关闭details内控件，动态字段加载后恢复有效焦点。 |
| 桌宠回归 | 保留Bongo、原创精灵图与Live2D各自机制；角色预览解码、大小/锁定/置顶持久化、透明区/拖动/点击/右键输入回归。 |
| 文案与打包 | 通过i18n-add.mjs增加20条反馈键，46语言/1983keys同步；readmd.boot.js已离线重打包。 |

## 最终验证

| 检查 | 最终结果 | 证据 |
| --- | --- | --- |
| 逐项入口及覆盖 | 101流程/389控件/74动态族/99路由，全部归属和实现位置通过 | [检查器](../../tools/audit-ui-features.mjs) |
| 最终桌面UI全套 | 250通过、0失败、1个移动端专属跳过；禁用重试，10.5分钟 | [日志](<local-evidence>) |
| 移动端专属触控 | 1通过、0失败，补齐上述跳过项 | [日志](<local-evidence>) |
| 全体弹窗布局 | 28弹窗×2窗口×3语言=168组合；6个参数化用例全部通过 | [用例](../../ui-tests/all-panels-layout.spec.js)；[产物](<local-evidence>) |
| Rust library全套 | 1798通过、0失败、3条件忽略 | [日志](<local-evidence>) |
| Rust桌面宿主 | 114通过、0失败 | [日志](<local-evidence>) |
| 独立Rust桌宠 | 71通过、0失败 | [日志](<local-evidence>) |
| 离线桌宠包实际安装 | 1通过；单独执行条件忽略项 | [日志](<local-evidence>) |
| 真实WebView2 | 通过：动态DOM、隔离桥、任务授权/Cookie、输入剔除、超时/取消/窗口关闭/撤销 | [日志](<local-evidence>) |
| i18n | 46语言、1983keys通过 | [日志](<local-evidence>) |
| wiring/styles/assets/no-python | 全部通过；267份资源、1148上游provider条目 | [wiring](<local-evidence>)；[styles](<local-evidence>)；[assets](<local-evidence>)；[no-python](<local-evidence>) |
| tools检查器回归 | 26通过、0失败 | [日志](<local-evidence>) |
| 前端离线包 | boot重打包，bundle-boot --check通过 | [日志](<local-evidence>) |
| 离线Release | 通过；实际产物[readmd.exe](Z:/readmd-target/main/release/readmd.exe) | [日志](<local-evidence>) |
| 文档链接/数量/差异空白 | 全部通过；实际链接检查数量见日志 | [日志](<local-evidence>)；[日志](<local-evidence>) |

窗口为1160×820和1024×680，语言为zh-CN/en/zh-TW。Rust的3个默认忽略项分别是离线桌宠包安装、旧Python PDF探针产物及在线网络探测；离线包安装已单独通过，另两项未执行。新一轮完整UI回归前，发现并修复导出初始焦点问题，同时将AI凭据用例改为先打开文档/编辑器再调用真实入口；最终250项全套在零重试运行中全部通过。


## 逐项验证表

每项的控件名是HTML id/委托容器；完整属性、默认隐藏/禁用、选项在 [controls.md](ui-function-inventory-2026-10-02/controls.md)，动态生成交互在 [dynamic.md](ui-function-inventory-2026-10-02/dynamic.md)。真实格式分派在 [formats.md](ui-function-inventory-2026-10-02/formats.md)，插件及导出配置分别见 [plugins.md](ui-function-inventory-2026-10-02/plugins.md)、[exports.md](ui-function-inventory-2026-10-02/exports.md)。

### 01 启动、主页与全局入口

#### F001 打开文档


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-open`、`w-open`、`file-input` |
| 操作与交互 | 点击打开 → 系统选择器或浏览器文件输入 → 上传/读取 → Markdown 打开，其余受支持文件分流转换/OCR → 标签与正文。取消不改变文档；选择器、上传及读取异常均反馈，异步操作被等待。浏览器保存操作作用于上传副本。 |
| 前端实现（已核对） | [loadFileDialog](../../assets/js/reader/render.js)；[loadFile](../../assets/js/reader/render.js) |
| 后端实现 | /api/dialog/choose-file、/api/upload、/api/file；server.rs 文件读取/上传 → content.rs 读取与修正；格式分流见 formats.md<br>[/api/file → h_file](../../rust/readmd-kernel/src/server.rs)；[/api/upload → h_upload](../../rust/readmd-kernel/src/server.rs)；[/api/dialog/choose-file → h_dialog_choose_file](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 真实本地上传、编辑、保存和磁盘内容回读；取消、空文档、空原文、草稿重开、延迟编辑器加载及失败恢复通过。 |
| 证据 | [ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/feature-verification.spec.js](../../ui-tests/feature-verification.spec.js)；[ui-tests/panels-upgrade.spec.js](../../ui-tests/panels-upgrade.spec.js)；[ui-tests/v238-reliability.spec.js](../../ui-tests/v238-reliability.spec.js) |
| 运行条件/实测边界 | 浏览器编辑上传副本；系统文件选择与原路径保存需要桌面宿主。 |

#### F002 打开文件夹


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-folder`、`w-folder` |
| 操作与交互 | 点击选择目录 → 系统选择器 → /api/list → 目录优先的自然排序文件树。读取失败保留当前目录；连续选择只应用最新请求结果；目录展开、文件打开和键盘导航均保留。浏览器入口说明桌面条件。 |
| 前端实现（已核对） | [openFolder](../../assets/js/reader/folder.js)；[renderTreeNodes](../../assets/js/reader/folder.js) |
| 后端实现 | /api/dialog/choose-folder、/api/list；native_dialogs.rs + server.rs h_list<br>[/api/list → h_list](../../rust/readmd-kernel/src/server.rs)；[/api/dialog/choose-folder → h_dialog_choose_folder](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 文件树键盘交互、目录响应失败保留原树、连续请求只应用最新结果通过；目录选择调用链已核对。 |
| 证据 | [ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/feature-verification.spec.js](../../ui-tests/feature-verification.spec.js) |
| 运行条件/实测边界 | 真实系统目录选择需要桌面宿主；自动测试为文件树提供受控目录响应。 |

#### F003 新建空白文档


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `w-new` |
| 操作与交互 | 点击主页新建或命令“新建” → 建立未保存虚拟标签 → 进入编辑 → 输入内容 → 保存选择 .md 路径。此时标题是文档名，尚无磁盘文件。 |
| 前端实现（已核对） | [newDocument](../../assets/js/editor/preview.js) |
| 后端实现 | 首次保存调用 /api/dialog/save-file 或 /api/dialog/save-as；后续 /api/save<br>[/api/save → h_save](../../rust/readmd-kernel/src/server.rs)；[/api/dialog/save-file → h_dialog_save_file](../../rust/readmd-kernel/src/server.rs)；[/api/dialog/save-as → h_dialog_save_as](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 真实本地上传、编辑、保存和磁盘内容回读；取消、空文档、空原文、草稿重开、延迟编辑器加载及失败恢复通过。 |
| 证据 | [ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/feature-verification.spec.js](../../ui-tests/feature-verification.spec.js)；[ui-tests/panels-upgrade.spec.js](../../ui-tests/panels-upgrade.spec.js)；[ui-tests/v238-reliability.spec.js](../../ui-tests/v238-reliability.spec.js) |
| 运行条件/实测边界 | 浏览器编辑上传副本；系统文件选择与原路径保存需要桌面宿主。 |

#### F004 主页快捷卡片


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `w-convert`、`w-web`、`w-ocr`、`w-ai` |
| 操作与交互 | 转换、网页、OCR、AI 四张卡分别进入相应工作台；与顶栏/更多菜单复用同一处理函数，不产生另一套后台流程。主页的搜索命令按钮和快捷键说明入口由内联 onclick 转发到 Shell。 |
| 前端实现（已核对） | [bindWelcomeEvents](../../assets/js/core/history.js) |
| 后端实现 | 复用转换、网页、OCR、AI 对应接口 |
| 实际验证 | 主页卡片、文档相关禁用状态、更多菜单分组和实际面板打开通过。 |
| 证据 | [ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/shell-upgrade.spec.js](../../ui-tests/shell-upgrade.spec.js)；[ui-tests/ui-quality.spec.js](../../ui-tests/ui-quality.spec.js) |
| 运行条件/实测边界 | 功能卡片复用下方对应业务流程；相关服务条件也随之适用。 |

#### F005 最近记录与清空


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-recent`、`recent-clear`、`history-clear`、`history-close` |
| 操作与交互 | 最近文件列表 → 点击打开、移除单条或清空全部 → 后端存储回读与列表刷新。清空操作单次执行；HTTP 错误、业务失败和原生桥异常不会伪装成成功，也不会先清空当前列表。 |
| 前端实现（已核对） | [openHistoryModal](../../assets/js/core/history.js)；[renderRecentList](../../assets/js/core/history.js)；[clearRecent](../../assets/js/core/history.js) |
| 后端实现 | /api/recent/status、add、remove、clear；store.rs 存储最近文件，server.rs 调用<br>[/api/recent/status → h_recent_status](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 最近记录读写路径已核对；原生桥清空/选择器失败被捕获，失败不清空当前列表。 |
| 证据 | [ui-tests/feature-verification.spec.js](../../ui-tests/feature-verification.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js) |
| 运行条件/实测边界 | 自动回归覆盖错误与异步反馈；历史条目取决于本地实际文件是否仍存在。 |

#### F006 返回主页


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-home` |
| 操作与交互 | 返回主页 → 正在编辑且有修改时弹出保存/放弃/取消 → 保存真正成功或明确放弃后才重置阅读区；取消保留编辑器。标签仍可再次打开。关闭标签已完成的确认通过内部参数复用，避免重复弹窗；并发主页点击不会叠加确认。 |
| 前端实现（已核对） | [goHome](../../assets/js/core/history.js) |
| 后端实现 | 读取最近记录；返回主页本身不删除文件 |
| 实际验证 | 真实文件先保存再返回主页；取消保留草稿、放弃后重置、重复确认防护通过。 |
| 证据 | [ui-tests/feature-verification.spec.js](../../ui-tests/feature-verification.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js) |
| 运行条件/实测边界 | 保存失败时保留编辑状态。虚拟标签仍需首次保存路径。 |

#### F007 更多菜单与分组


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-more` |
| 操作与交互 | 点击更多 → 展开菜单；导入、互动、设置分组标题可分别折叠/展开 → 点击具体功能。支持上下/Home/End 键移动、Esc 关闭及点击外部关闭；菜单分组属于导航，具体业务见对应层级。 |
| 前端实现（已核对） | [bindEvents](../../assets/app.js)；[closeMoreMenu](../../assets/app.js) |
| 后端实现 | 前端处理；不调用业务后端 |
| 实际验证 | 主页卡片、文档相关禁用状态、更多菜单分组和实际面板打开通过。 |
| 证据 | [ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/shell-upgrade.spec.js](../../ui-tests/shell-upgrade.spec.js)；[ui-tests/ui-quality.spec.js](../../ui-tests/ui-quality.spec.js) |
| 运行条件/实测边界 | 功能卡片复用下方对应业务流程；相关服务条件也随之适用。 |

#### F008 刷新文件列表/当前文件


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-reload` |
| 操作与交互 | 点击刷新按当前状态刷新文件夹列表或重新读取当前文件。命令面板“重新加载”调用 force:true 的 loadFile；外部文件自动重载是后台行为，无独立常驻开关按钮。编辑脏数据通过相应保护流程处理。 |
| 前端实现（已核对） | [bindEvents](../../assets/app.js)；[startAutoReload](../../assets/js/core/history.js) |
| 后端实现 | /api/list、/api/file；以最终绑定与状态分支为准<br>[/api/file → h_file](../../rust/readmd-kernel/src/server.rs)；[/api/list → h_list](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 实际磁盘重载、编辑脏数据保护、保留阅读页码及后台重载不覆盖编辑器通过。 |
| 证据 | [ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/v238-reliability.spec.js](../../ui-tests/v238-reliability.spec.js) |
| 运行条件/实测边界 | 外部文件修改与读取权限依赖本地文件；非保存草稿不会被强制重载覆盖。 |

### 02 多标签、标题和文件管理

#### F009 切换、关闭及溢出标签


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `doc-tabs-overflow-btn` |
| 操作与交互 | 点击标签标题切换正文；× 或鼠标中键关闭；标签数超过空间后点溢出按钮列出被折叠标签，再点条目切换。标签带未保存标识，关闭脏标签先弹保存/放弃/取消。切换保存各自正文、光标/滚动及编辑状态。 |
| 前端实现（已核对） | [renderTabsBar](../../assets/js/core/tabs.js)；[closeTab](../../assets/js/core/tabs.js) |
| 后端实现 | 切换主要在内存；实际文档读取 /api/file，脏标签保存 /api/save<br>[/api/file → h_file](../../rust/readmd-kernel/src/server.rs)；[/api/save → h_save](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 标签切换、拖动重排、键盘关闭、溢出入口、脏后台标签保存与独立内容/阅读位置通过。 |
| 证据 | [ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/shell-upgrade.spec.js](../../ui-tests/shell-upgrade.spec.js)；[ui-tests/frontend-audit.spec.js](../../ui-tests/frontend-audit.spec.js) |
| 运行条件/实测边界 | 虚拟文档没有磁盘路径；上下文动作按状态启用。 |

#### F010 标签右键菜单和排序


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `tab-context-menu` |
| 操作与交互 | 右键标签 → 左移、右移、关闭标签、关闭其他、关闭所有、重命名、复制文件路径七项。标签还可拖动重排；关闭多标签逐一经过脏数据决策。虚拟文档无实际路径时复制路径入口受限。 |
| 前端实现（已核对） | [openTabContextMenu](../../assets/js/core/tabs.js)；[reorderTabs](../../assets/js/core/tabs.js) |
| 后端实现 | 顺序保留在前端标签数组；关闭时可能触发保存；复制使用剪贴板 |
| 实际验证 | 标签切换、拖动重排、键盘关闭、溢出入口、脏后台标签保存与独立内容/阅读位置通过。 |
| 证据 | [ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/shell-upgrade.spec.js](../../ui-tests/shell-upgrade.spec.js)；[ui-tests/frontend-audit.spec.js](../../ui-tests/frontend-audit.spec.js) |
| 运行条件/实测边界 | 虚拟文档没有磁盘路径；上下文动作按状态启用。 |

#### F011 标题重命名


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `file-title` |
| 操作与交互 | 点击顶栏文件标题或双击标签/F2 → 输入名称 → Enter 或失焦提交，Esc 取消。磁盘文件经 native bridge rename_file 发送 path/new_stem，保留原目录与扩展名；虚拟标签只改显示名。目标同名或名称不合法时反馈错误。标签和顶栏存在各自提交路径，不能凭按钮可见就认定浏览器能改磁盘文件名。 |
| 前端实现（已核对） | [openFileRename](../../assets/js/reader/render.js)；[renameTab](../../assets/js/core/tabs.js) |
| 后端实现 | /api/rename → server.rs h_rename；路径/名称验证、改名与相关引用处理<br>[/api/rename → h_rename](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 标题/标签重命名交互、扩展名保留和内容注入防护通过；Rust名称和路径检查通过。 |
| 证据 | [ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js) |
| 运行条件/实测边界 | 物理重命名需要桌面桥和目录权限；浏览器/虚拟标签条件在入口反馈。 |

#### F012 另存为 Markdown


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-saveas` |
| 操作与交互 | 打开文档 → 更多/互动/另存为 → 选择目标路径 → 写出当前 Markdown；虚拟文档第一次保存也进入此类流程。桌面用系统保存对话框；浏览器可以走 Blob 下载路径，具体以 hasPy 分支为准。 |
| 前端实现（已核对） | [saveAs](../../assets/js/editor/preview.js) |
| 后端实现 | /api/dialog/save-as、/api/save；native_dialogs.rs、server.rs 文件写入<br>[/api/save → h_save](../../rust/readmd-kernel/src/server.rs)；[/api/dialog/save-as → h_dialog_save_as](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 真实本地上传、编辑、保存和磁盘内容回读；取消、空文档、空原文、草稿重开、延迟编辑器加载及失败恢复通过。 |
| 证据 | [ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/feature-verification.spec.js](../../ui-tests/feature-verification.spec.js)；[ui-tests/panels-upgrade.spec.js](../../ui-tests/panels-upgrade.spec.js)；[ui-tests/v238-reliability.spec.js](../../ui-tests/v238-reliability.spec.js) |
| 运行条件/实测边界 | 浏览器编辑上传副本；系统文件选择与原路径保存需要桌面宿主。 |

### 03 目录、搜索、分页和阅读偏好

#### F013 侧栏、大纲和文件树


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-toc`、`tab-toc`、`tab-files`、`side-close-btn` |
| 操作与交互 | 侧栏切换大纲/文件 → 标题跳转、目录展开/折叠、文件打开和方向键导航。目录 API 的状态与数据类型先验证；失败保留文件树，迟到的旧请求不能覆盖新目录。 |
| 前端实现（已核对） | [buildToc](../../assets/js/reader/toc.js)；[showSide](../../assets/js/reader/folder.js)；[decorateToc](../../assets/js/reader/enhance.js) |
| 后端实现 | 大纲解析和滚动为前端；文件树 /api/list、/api/file<br>[/api/file → h_file](../../rust/readmd-kernel/src/server.rs)；[/api/list → h_list](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 文件树键盘交互、目录响应失败保留原树、连续请求只应用最新结果通过；目录选择调用链已核对。 |
| 证据 | [ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/feature-verification.spec.js](../../ui-tests/feature-verification.spec.js) |
| 运行条件/实测边界 | 真实系统目录选择需要桌面宿主；自动测试为文件树提供受控目录响应。 |

#### F014 文内查找


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-search`、`search-input`、`search-prev`、`search-next`、`search-close` |
| 操作与交互 | 打开文档 → Ctrl+F/搜索按钮 → 输入关键词 → 正文匹配高亮并显示命中数量 → 上一项/下一项定位；Enter 下一项，Shift+Enter 上一项，Esc/× 清理高亮并关闭。没有命中时显示空结果。 |
| 前端实现（已核对） | [doSearch](../../assets/js/reader/search.js)；[jumpToMark](../../assets/js/reader/search.js) |
| 后端实现 | 前端处理；不调用业务后端 |
| 实际验证 | 搜索跨相邻内联节点、计数、立即Enter、上一/下一、清理和跳过阅读工具通过。 |
| 证据 | [ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/interaction-regressions.spec.js](../../ui-tests/interaction-regressions.spec.js)；[ui-tests/reader-upgrade.spec.js](../../ui-tests/reader-upgrade.spec.js) |
| 运行条件/实测边界 | 只搜索当前渲染阅读内容；分页文档按现有分页行为定位。 |

#### F015 分页跳转与连续阅读


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `pg-first-btn`、`pg-prev-btn`、`pg-page-select`、`pg-next-btn`、`pg-last-btn`、`pg-mode-toggle`、`status-pagination` |
| 操作与交互 | 大文档满足分页条件后显示分页栏 → 首页/上一页/页码选择/下一页/末页 → 按页渲染；阅读模式按钮及状态栏分页状态切换分页/全卷连续。转连续前在提示框确认继续或取消；不可前进/后退时对应按钮禁用。目录定位会先切至包含标题的页。 |
| 前端实现（已核对） | [splitMdIntoPages](../../assets/js/reader/render.js)；[togglePaginationMode](../../assets/js/reader/render.js)；[renderPage](../../assets/js/reader/render.js) |
| 后端实现 | 分块与页切换在前端；初始内容来自 /api/file 或虚拟文档<br>[/api/file → h_file](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 分页/连续切换确认、重复标题目录定位、标签阅读位置以及窄栏分页控件通过。 |
| 证据 | [ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/pagination-adaptive.spec.js](../../ui-tests/pagination-adaptive.spec.js)；[ui-tests/readmd-performance.spec.js](../../ui-tests/readmd-performance.spec.js)；[ui-tests/interaction-regressions.spec.js](../../ui-tests/interaction-regressions.spec.js) |
| 运行条件/实测边界 | 分页由文档长度触发；连续模式的大文档性能通过1k至50k行回归。 |

#### F016 主题、缩放和禅模式


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-theme`、`btn-a`、`btn-A`、`btn-zen` |
| 操作与交互 | 主题循环、缩放、禅模式和阅读偏好 → 即时应用 → 保存。读取设置只接受白名单字段及有效枚举，数值按范围限制；损坏配置不能覆盖文档、标签或 AI 状态。原生保存串行执行，保证最后选择持久化；保存失败显示反馈。 |
| 前端实现（已核对） | [toggleTheme](../../assets/js/core/settings.js)；[zoom](../../assets/js/core/settings.js)；[toggleZenMode](../../assets/js/reader/render.js) |
| 后端实现 | 桌面 /api/settings 持久化主题与字号；浏览器 localStorage<br>[/api/settings → h_settings](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 主题切换、缩放、禅模式、损坏配置字段隔离及设置合法值过滤通过；持久化写入串行化。 |
| 证据 | [ui-tests/feature-verification.spec.js](../../ui-tests/feature-verification.spec.js)；[ui-tests/shell-upgrade.spec.js](../../ui-tests/shell-upgrade.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/frontend-audit.spec.js](../../ui-tests/frontend-audit.spec.js) |
| 运行条件/实测边界 | 原生设置写入依赖本地目录权限；失败有反馈。 |

#### F017 浮动阅读设置


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `rd-prefs-btn`、`rd-prefs` |
| 操作与交互 | 正文内浮动设置按钮 → 字号减/增、无衬线/衬线、行距紧凑/正常/宽松、宽度窄/正常/宽 → 即时调整阅读排版；显示字数/预计阅读时间。分段按钮支持方向键；点击外部或 Esc 关闭。 |
| 前端实现（已核对） | [buildPrefsPanel](../../assets/js/reader/enhance.js)；[setPref](../../assets/js/reader/enhance.js) |
| 后端实现 | 桌面 /api/settings；浏览器 readmd-settings；readingFont、readingLeading、readingWidth<br>[/api/settings → h_settings](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 浮动阅读偏好、字号/行距/宽度与回到顶部的入口和响应函数核对；阅读面板相关UI回归通过。 |
| 证据 | [ui-tests/reader-upgrade.spec.js](../../ui-tests/reader-upgrade.spec.js)；[ui-tests/ui-quality.spec.js](../../ui-tests/ui-quality.spec.js) |
| 运行条件/实测边界 | 回到顶部无需业务后端；偏好保存依赖浏览器存储或原生设置。 |

#### F099 回到顶部


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `top-btn` |
| 操作与交互 | 正文滚动超过600px后出现浮动上箭头 → 点击让正文区域平滑滚到顶部；作用于当前阅读容器，不重新读取文件、不改变文档。 |
| 前端实现（已核对） | [bindEvents](../../assets/app.js) |
| 后端实现 | 前端处理；不调用业务后端 |
| 实际验证 | 浮动阅读偏好、字号/行距/宽度与回到顶部的入口和响应函数核对；阅读面板相关UI回归通过。 |
| 证据 | [ui-tests/reader-upgrade.spec.js](../../ui-tests/reader-upgrade.spec.js)；[ui-tests/ui-quality.spec.js](../../ui-tests/ui-quality.spec.js) |
| 运行条件/实测边界 | 回到顶部无需业务后端；偏好保存依赖浏览器存储或原生设置。 |

### 04 正文内可操作元素和学术阅读

#### F018 正文链接、双链和锚点


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `content` |
| 操作与交互 | 点击外部 http(s) 链接由 openPath/宿主打开；本地 Markdown 或 [[笔记#标题\|别名]] 打开对应笔记并定位锚点；正文 #锚点及标题链图标用于定位。标题图标是href锚点，没有独立复制链接处理器。路径、协议和脚本内容经渲染白名单处理；未创建双链在关联列表中不可直接打开。 |
| 前端实现（已核对） | [fixLinks](../../assets/js/reader/render.js)；[navigateWikilink](../../assets/js/reader/render.js)；[upgradeHeadings](../../assets/js/reader/enhance.js) |
| 后端实现 | /api/file、/api/system/open-path；Rust 解析路径，前端滚动到锚点<br>[/api/file → h_file](../../rust/readmd-kernel/src/server.rs)；[/api/system/open-path → h_system_open_path](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 真实Markdown渲染、锚点/目录跳转、代码复制、脚注、图片及不可信协议过滤相关回归通过。 |
| 证据 | [ui-tests/reader-upgrade.spec.js](../../ui-tests/reader-upgrade.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js) |
| 运行条件/实测边界 | 外链由宿主/浏览器打开；本地链接需实际文件存在，未创建笔记保持明确状态。 |

#### F019 代码块复制、折叠提示、脚注与图片


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `content` |
| 操作与交互 | 普通代码块复制按钮复制该块源码并短暂显示已复制；可折叠 callout 的标题展开/收起；脚注序号跳到文末定义，回跳箭头回到引用；点击正文图片打开大图灯箱，×/点击背景/Esc 关闭。正文表格横向滚动以保留列内容；普通任务清单阅读时为展示，编辑器里的任务标记可改源码。 |
| 前端实现（已核对） | [upgradeCode](../../assets/js/reader/enhance.js)；[upgradeCallout](../../assets/js/reader/enhance.js)；[upgradeFootnotes](../../assets/js/reader/enhance.js)；[ensureLightbox](../../assets/js/reader/enhance.js) |
| 后端实现 | 复制与灯箱是前端；图片资源经 /raw 等资源路径加载，不把展示复选框误作后端任务系统<br>[/raw → h_raw](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 真实Markdown渲染、锚点/目录跳转、代码复制、脚注、图片及不可信协议过滤相关回归通过。 |
| 证据 | [ui-tests/reader-upgrade.spec.js](../../ui-tests/reader-upgrade.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js) |
| 运行条件/实测边界 | 外链由宿主/浏览器打开；本地链接需实际文件存在，未创建笔记保持明确状态。 |

#### F020 文献引用与参考文献交互


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `content` |
| 操作与交互 | 文档含 citation/BibTeX 配置时 → 加载文献数据 → 引用编号/作者信息可跳参考文献；悬浮显示文献详情卡，DOI/URL 等链接可打开。引用样式依据文档内容/配置解析，界面没有独立完整文献库管理面板。 |
| 前端实现（已核对） | [loadDocCitations](../../assets/js/reader/render.js)；[processBibCitations](../../assets/js/reader/render.js)；[showBibHoverCard](../../assets/js/reader/render.js) |
| 后端实现 | /api/bibtex → bibtex.rs/引用处理；前端生成引用和悬浮卡<br>[/api/bibtex → h_bibtex](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 文献加载、链接/浮层入口核对；BibTeX解析和TeX文献后端单测、元数据注入防护通过。 |
| 证据 | [ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js) |
| 运行条件/实测边界 | 本地.bib必须存在且可读；此轮没有逐条人工点击每个文献浮层动作。 |

#### F098 代码文件顶部四个操作


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-code-to-md`、`btn-code-edit`、`btn-code-ai-explain`、`btn-code-copy` |
| 操作与交互 | 打开代码文件 → 顶部四操作：转说明文档使用code_to_doc AI模板；编辑源码调用toggleEdit进入CodeMirror并保留原始代码；AI解析使用code_analysis；复制源码写剪贴板。编辑内容可撤销、保存，未保存关闭走统一确认。 |
| 前端实现（已核对） | [renderContent](../../assets/js/reader/render.js)；[bindCodeDocActions](../../assets/js/reader/render.js) |
| 后端实现 | 说明文档/解析复用/api/ai/chat；编辑事务前端、保存/api/save；复制剪贴板<br>[/api/save → h_save](../../rust/readmd-kernel/src/server.rs)；[/api/ai/chat → h_ai_chat](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 真实代码文档的顶部编辑按钮进入编辑器且保留源码；复制/运行/AI函数及条件核对。 |
| 证据 | [ui-tests/function-completion.spec.js](../../ui-tests/function-completion.spec.js)；[ui-tests/v238-ai-config.spec.js](../../ui-tests/v238-ai-config.spec.js) |
| 运行条件/实测边界 | 运行依赖对应解释器，AI依赖已配置连接；不因展示按钮自动执行代码。 |

### 05 Markdown 编辑工作台

#### F021 编辑、保存和取消


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-edit`、`edit-area`、`edit-save`、`edit-cancel` |
| 操作与交互 | 进入编辑 → 加载编辑器或 textarea 兜底 → 修改、预览、保存/取消与撤销。恢复标签时未保存草稿和空原文分别保存；剪贴板/AI 标签不会因撤销而伪装成已保存。编辑器加载中切换文档会取消旧挂载，不能在新文档上出现旧编辑器。保存与丢弃走实际文件/副本流程和确认。 |
| 前端实现（已核对） | [toggleEdit](../../assets/js/editor/preview.js)；[saveEdit](../../assets/js/editor/preview.js)；[createEditor](../../assets/js/editor/editor.js) |
| 后端实现 | /api/save；server.rs 版本冲突/文件写入；新虚拟文档先选保存路径<br>[/api/save → h_save](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 真实本地上传、编辑、保存和磁盘内容回读；取消、空文档、空原文、草稿重开、延迟编辑器加载及失败恢复通过。 |
| 证据 | [ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/feature-verification.spec.js](../../ui-tests/feature-verification.spec.js)；[ui-tests/panels-upgrade.spec.js](../../ui-tests/panels-upgrade.spec.js)；[ui-tests/v238-reliability.spec.js](../../ui-tests/v238-reliability.spec.js) |
| 运行条件/实测边界 | 浏览器编辑上传副本；系统文件选择与原路径保存需要桌面宿主。 |

#### F022 文字格式与结构工具


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `data-md` |
| 操作与交互 | 选中文本或放置光标 → 工具栏常用项/“文本”/“结构”菜单 → 加粗、斜体、删除线、行内代码、行内公式、链接；一级/二级/三级标题、正文、引用、提示块、无序/有序/任务列表、代码块、块公式、分隔线。操作经统一 Markdown transform 更新选区和撤销栈；不是直接把 HTML 写入文档。 |
| 前端实现（已核对） | [cmInsertSyntax](../../assets/js/editor/editor.js)；[assets/js/editor/md-transforms.js](../../assets/js/editor/md-transforms.js) |
| 后端实现 | 纯前端文本变换；点击保存才写磁盘 |
| 实际验证 | 真实CodeMirror格式、单次撤销、智能列表/括号/表格、粘贴、选区工具、斜杠菜单、预览同步及窄窗退化通过。 |
| 证据 | [ui-tests/editor-upgrade.spec.js](../../ui-tests/editor-upgrade.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/v238-reliability.spec.js](../../ui-tests/v238-reliability.spec.js) |
| 运行条件/实测边界 | 编辑器加载失败提供textarea兜底；CM专用功能在可用编辑器中启用。 |

#### F023 编辑菜单、视图和撤销栈


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `data-menu`、`edit-view-trigger`、`edit-view-focus`、`edit-view-typewriter`、`edit-view-lines`、`edit-undo`、`edit-redo` |
| 操作与交互 | 点击文本/结构/插入菜单展开二级项；编辑视图菜单切专注行、打字机居中、显示行号；撤销/重做作用于编辑器事务。开关更新编辑器扩展及界面勾选状态；编辑器偏好与阅读器全局设置使用各自存储。 |
| 前端实现（已核对） | [bindEditBarExtras](../../assets/js/editor/editor.js)；[setEditorPref](../../assets/js/editor/editor.js)；[cmUndo](../../assets/js/editor/editor.js) |
| 后端实现 | 编辑器偏好 localStorage；文字撤销在前端，最终保存 /api/save<br>[/api/save → h_save](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 真实CodeMirror格式、单次撤销、智能列表/括号/表格、粘贴、选区工具、斜杠菜单、预览同步及窄窗退化通过。 |
| 证据 | [ui-tests/editor-upgrade.spec.js](../../ui-tests/editor-upgrade.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/v238-reliability.spec.js](../../ui-tests/v238-reliability.spec.js) |
| 运行条件/实测边界 | 编辑器加载失败提供textarea兜底；CM专用功能在可用编辑器中启用。 |

#### F024 选区浮动工具条


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `cm-sel-ai`、`cm-sel-copy`、`cm-sel-cut`、`cm-sel-paste`、`cm-selection-toolbar` |
| 操作与交互 | 鼠标或键盘选择非空文字 → 浮动条出现 → AI 改写、粗体、斜体、删除线、代码、链接、标题、引用、复制、剪切、粘贴。选区失效时隐藏；剪切/粘贴走编辑器事务，AI 入口使用当前选区。 |
| 前端实现（已核对） | [updateCmSelectionToolbar](../../assets/js/editor/editor.js)；[bindCmSelectionToolbar](../../assets/js/editor/editor.js)；[cmPasteSelection](../../assets/js/editor/editor.js) |
| 后端实现 | 格式与剪贴板前端；AI /api/ai/chat；写磁盘须保存<br>[/api/ai/chat → h_ai_chat](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 真实CodeMirror格式、单次撤销、智能列表/括号/表格、粘贴、选区工具、斜杠菜单、预览同步及窄窗退化通过。 |
| 证据 | [ui-tests/editor-upgrade.spec.js](../../ui-tests/editor-upgrade.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/v238-reliability.spec.js](../../ui-tests/v238-reliability.spec.js) |
| 运行条件/实测边界 | 编辑器加载失败提供textarea兜底；CM专用功能在可用编辑器中启用。 |

#### F025 斜杠菜单与子选择器


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `edit-slash-btn` |
| 操作与交互 | 点击命令按钮或在合适位置输入 / → 按词搜索/点条目；23 个一级条目逐项见 commands.md。代码块进入语言子列表（纯文本、25 个预设语言、自定义输入）；表格进入尺寸选择器。鼠标、方向键、Enter、Esc 完成选择或退出；插入保持编辑历史。 |
| 前端实现（已核对） | [slashCatalog](../../assets/js/editor/editor.js)；[slashLanguageItems](../../assets/js/editor/editor.js)；[slashRenderTable](../../assets/js/editor/editor.js) |
| 后端实现 | 纯前端；生成 Markdown 后按正常保存路径持久化 |
| 实际验证 | 真实CodeMirror格式、单次撤销、智能列表/括号/表格、粘贴、选区工具、斜杠菜单、预览同步及窄窗退化通过。 |
| 证据 | [ui-tests/editor-upgrade.spec.js](../../ui-tests/editor-upgrade.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/v238-reliability.spec.js](../../ui-tests/v238-reliability.spec.js) |
| 运行条件/实测边界 | 编辑器加载失败提供textarea兜底；CM专用功能在可用编辑器中启用。 |

#### F026 智能编辑、粘贴与任务勾选


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `edit-area` |
| 操作与交互 | 在代码围栏、Markdown 表格和列表中 Enter/Tab/Shift+Tab 自动补结构或缩进；选中文字输入强调符号可包裹；粘贴 Excel 的 HTML/制表文本转为 Markdown 表格；粘贴图片走图片保存；点击编辑器任务标记切换 [ ]/[x]。这些交互属于源码编辑，不是另一个后端表格数据库。 |
| 前端实现（已核对） | [cmPriorityKeymap](../../assets/js/editor/editor.js)；[handleSmartExcelPaste](../../assets/js/editor/editor.js)；[cmTaskMarkerClick](../../assets/js/editor/editor.js)；[cmHandlePaste](../../assets/js/editor/editor.js) |
| 后端实现 | 通常纯前端；图片 /api/image/save，文档最终 /api/save<br>[/api/save → h_save](../../rust/readmd-kernel/src/server.rs)；[/api/image/save → h_image_save](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 真实CodeMirror格式、单次撤销、智能列表/括号/表格、粘贴、选区工具、斜杠菜单、预览同步及窄窗退化通过。 |
| 证据 | [ui-tests/editor-upgrade.spec.js](../../ui-tests/editor-upgrade.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/v238-reliability.spec.js](../../ui-tests/v238-reliability.spec.js) |
| 运行条件/实测边界 | 编辑器加载失败提供textarea兜底；CM专用功能在可用编辑器中启用。 |

#### F027 预览布局与分栏拖动


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `pv-trigger`、`data-pv`、`pv-sync`、`pv-splitter` |
| 操作与交互 | 编辑 → 预览菜单 → 上/左/右/下/关闭五种布局 → 实时渲染 Markdown；勾选同步滚动使源码和预览滚动联动；拖动横向/纵向分隔条调整比例。布局、滚动同步与比例保存到设置。 |
| 前端实现（已核对） | [setPvLayout](../../assets/js/editor/preview.js)；[bindPvSplitter](../../assets/js/editor/preview.js)；[pvSyncFromPreview](../../assets/js/editor/preview.js) |
| 后端实现 | 桌面 /api/settings；浏览器 localStorage；正文预览渲染在前端<br>[/api/settings → h_settings](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 真实CodeMirror格式、单次撤销、智能列表/括号/表格、粘贴、选区工具、斜杠菜单、预览同步及窄窗退化通过。 |
| 证据 | [ui-tests/editor-upgrade.spec.js](../../ui-tests/editor-upgrade.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/v238-reliability.spec.js](../../ui-tests/v238-reliability.spec.js) |
| 运行条件/实测边界 | 编辑器加载失败提供textarea兜底；CM专用功能在可用编辑器中启用。 |

### 06 插入图片、表格、公式和高级块

#### F028 图片输入及 URL 直接插入


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `img-file`、`img-file-input`、`img-url-input`、`img-url-load` |
| 操作与交互 | 插入菜单/图片 → 选择本地图片加载工作台；也可粘 URL 后点插入 URL/Enter，直接生成 Markdown 图片链接。URL 直接插入和本地裁剪是两条不同路径，URL 入口不等于完整远程图片编辑。 |
| 前端实现（已核对） | [openImgModal](../../assets/js/editor/image.js)；[insertImgUrl](../../assets/js/editor/image.js)；[loadImgFromFile](../../assets/js/editor/image.js) |
| 后端实现 | 选择本地图片浏览器 FileReader；URL 生成源码；本地编辑后的结果 /api/image/save<br>[/api/image/save → h_image_save](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 图片编辑面板的控制项与处理函数核对；轻量图片控制、比率/翻转/撤销入口及六种窗口语言组合的可访问布局通过。 |
| 证据 | [ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/all-panels-layout.spec.js](../../ui-tests/all-panels-layout.spec.js) |
| 运行条件/实测边界 | 文件读取、Canvas导出和网络图片受格式/浏览器跨域限制；此轮没有穷举所有图片编码及裁剪组合。 |

#### F029 裁剪、旋转、翻转、尺寸及历史


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `img-rot-l`、`img-rot-r`、`img-flip-x`、`img-flip-y`、`img-angle`、`img-angle-number`、`img-view-zoom`、`img-ratio`、`img-out-w`、`img-out-h`、`img-size-lock`、`img-undo`、`img-redo`、`img-reset` |
| 操作与交互 | 加载图片 → 拖动裁剪框或八个方向手柄 → 自由/1:1/4:3/3:2/16:9/原图比例 → 左/右90°旋转或精确角度，水平/垂直翻转 → 预览缩放 → 输出宽/高（处理上限 16000），锁定宽高比例 → 撤销/重做/重置。缩放用于编辑视图，输出尺寸单独控制生成图片。 |
| 前端实现（已核对） | [rotateImg](../../assets/js/editor/image.js)；[applyRatio](../../assets/js/editor/image.js)；[undoImg](../../assets/js/editor/image.js) |
| 后端实现 | Canvas 本地变换；尚未点击插入时不改正文或原图文件 |
| 实际验证 | 图片编辑面板的控制项与处理函数核对；轻量图片控制、比率/翻转/撤销入口及六种窗口语言组合的可访问布局通过。 |
| 证据 | [ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/all-panels-layout.spec.js](../../ui-tests/all-panels-layout.spec.js) |
| 运行条件/实测边界 | 文件读取、Canvas导出和网络图片受格式/浏览器跨域限制；此轮没有穷举所有图片编码及裁剪组合。 |

#### F030 图片生成、插入与关闭


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `img-insert`、`img-close`、`img-close-x` |
| 操作与交互 | 点插入 → Canvas 导出处理后图像 → /api/image/save 保存图片资源 → 在当前光标插 Markdown 相对路径或相应回退表示 → 标记文档未保存。关闭/×放弃当前工作台编辑；图像保存与 .md 保存是两个步骤。 |
| 前端实现（已核对） | [exportAndInsertImg](../../assets/js/editor/image.js)；[closeImgModal](../../assets/js/editor/image.js) |
| 后端实现 | /api/image/save → server.rs h_image_save；资源路径与编码验证<br>[/api/image/save → h_image_save](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 图片编辑面板的控制项与处理函数核对；轻量图片控制、比率/翻转/撤销入口及六种窗口语言组合的可访问布局通过。 |
| 证据 | [ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/all-panels-layout.spec.js](../../ui-tests/all-panels-layout.spec.js) |
| 运行条件/实测边界 | 文件读取、Canvas导出和网络图片受格式/浏览器跨域限制；此轮没有穷举所有图片编码及裁剪组合。 |

#### F031 表格网格选择


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-insert-table`、`table-modal-close` |
| 操作与交互 | 插入表格 → 10×10 尺寸网格 → 鼠标悬停/键盘焦点显示行列 → 点击或 Enter 插入对应尺寸；方向键移动，关闭不插入。每个格子是真实带名称的 44px 按钮，小窗口允许滚动访问。 |
| 前端实现（已核对） | [initTableGridPicker](../../assets/js/editor/editor.js)；[insertCustomTable](../../assets/js/editor/editor.js) |
| 后端实现 | 前端处理；不调用业务后端 |
| 实际验证 | 鼠标/键盘尺寸选取、方向键移动、Enter插入正确行列；10×10网格44px且小窗可滚动访问通过。 |
| 证据 | [ui-tests/feature-verification.spec.js](../../ui-tests/feature-verification.spec.js)；[ui-tests/editor-upgrade.spec.js](../../ui-tests/editor-upgrade.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/all-panels-layout.spec.js](../../ui-tests/all-panels-layout.spec.js) |
| 运行条件/实测边界 | 仅生成Markdown表格；大表格后续由编辑器修改。 |

#### F032 公式模板选择


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `formula-open`、`formula-close`、`formula-search`、`formula-mode` |
| 操作与交互 | 公式按钮 → 搜索模板、选择分类/模板 → 查看公式预览 → 按行内/块公式模式插入 LaTeX。分类和模板按钮动态生成，具体模板目录见数据清单；正文公式由本地 MathJax/KaTeX 类资源渲染。 |
| 前端实现（已核对） | [renderFormulaPicker](../../assets/js/editor/editor.js)；[insertFormula](../../assets/js/editor/editor.js) |
| 后端实现 | 前端模板插入；Rust 读取修复与导出公式另由 formula_repair.rs/formula 系列实现 |
| 实际验证 | 28个公式模板目录、分类/搜索/插入函数核对；公式面板和响应式工具回归通过。 |
| 证据 | [ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/all-panels-layout.spec.js](../../ui-tests/all-panels-layout.spec.js) |
| 运行条件/实测边界 | 数学渲染使用随包资源；自动回归未对每个模板各执行一次人工插入。 |

#### F033 交互式代码块插入


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-insert-code-chunk`、`code-chunk-modal-close`、`code-chunk-lang`、`code-chunk-opt-plot`、`code-chunk-opt-hide`、`code-chunk-code`、`code-chunk-cancel`、`code-chunk-insert` |
| 操作与交互 | 选择七种代码语言 → 填代码、图形输出与隐藏选项 → 插入带属性的围栏。空源码不插入、不关闭弹窗；围栏长度自动大于源码中的反引号，防止结构破坏。执行仍取决于已有系统解释器，插入不会自动运行。 |
| 前端实现（已核对） | [openCodeChunkModal](../../assets/js/editor/editor.js)；[insertCodeChunkFromModal](../../assets/js/editor/editor.js) |
| 后端实现 | 插入前端处理；点击运行后 /api/code/run → parity_code.rs + code_chunk_runner.rs<br>[/api/code/run → parity_code::h_code_run](../../rust/readmd-kernel/src/parity_code.rs) |
| 实际验证 | 拒绝空源码、较长围栏保护内嵌反引号、图表草稿重开保留通过；实际离线图表渲染通过。 |
| 证据 | [ui-tests/feature-verification.spec.js](../../ui-tests/feature-verification.spec.js)；[ui-tests/function-completion.spec.js](../../ui-tests/function-completion.spec.js)；[ui-tests/v238-batch-a-fixes.spec.js](../../ui-tests/v238-batch-a-fixes.spec.js) |
| 运行条件/实测边界 | 代码执行需本机解释器；复杂图表语法按各引擎能力，基础WSD/D2/ditaa范围见附件。 |

#### F034 科学图表插入


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-insert-diagram`、`diagram-modal-close`、`diagram-type`、`diagram-code`、`diagram-cancel`、`diagram-insert` |
| 操作与交互 | 选择科学图表类型 → 示例/用户草稿 → 编辑 → 插入。重新打开保留草稿；空代码给出反馈；嵌套反引号用更长围栏保护。渲染器、运行条件及复杂语法限制见图表附件。 |
| 前端实现（已核对） | [openDiagramModal](../../assets/js/editor/editor.js)；[insertDiagramFromModal](../../assets/js/editor/editor.js) |
| 后端实现 | 插入前端；渲染路径详见第07层和 api.md |
| 实际验证 | 拒绝空源码、较长围栏保护内嵌反引号、图表草稿重开保留通过；实际离线图表渲染通过。 |
| 证据 | [ui-tests/feature-verification.spec.js](../../ui-tests/feature-verification.spec.js)；[ui-tests/function-completion.spec.js](../../ui-tests/function-completion.spec.js)；[ui-tests/v238-batch-a-fixes.spec.js](../../ui-tests/v238-batch-a-fixes.spec.js) |
| 运行条件/实测边界 | 代码执行需本机解释器；复杂图表语法按各引擎能力，基础WSD/D2/ditaa范围见附件。 |

#### F035 引用子文档


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-insert-doc-import`、`doc-import-modal-close`、`doc-import-path`、`doc-import-browse`、`doc-import-mode`、`doc-import-lines`、`doc-import-cancel`、`doc-import-insert` |
| 操作与交互 | 输入/浏览引用路径 → 选择 Markdown 递归、Code 高亮或 HTML 原样包含 → 正整数行号或由小到大的范围 → 插入 @import。前端拒绝空路径、引号/换行及非法范围；Rust 真实处理 mode 和 lines，保留预算、路径限制、循环检测及深度上限；不再忽略面板选项。 |
| 前端实现（已核对） | [insertDocImportFromModal](../../assets/js/editor/editor.js)；[processDocImports](../../assets/js/reader/render.js) |
| 后端实现 | /api/import/process → import_processor.rs；读文件并按引用模式展开<br>[/api/import/process → h_import_process](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 前端拒绝非法路径/范围；Rust以真实文件验证递归、code/html和lines选项，不再忽略面板设置。 |
| 证据 | [ui-tests/feature-verification.spec.js](../../ui-tests/feature-verification.spec.js)；[ui-tests/v238-batch-a-fixes.spec.js](../../ui-tests/v238-batch-a-fixes.spec.js) |
| 运行条件/实测边界 | 引用文件须存在；路径授权、递归深度和预算限制仍生效。PDF引用需Windows PDF组件。 |

#### F036 YAML/演示元数据


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-insert-frontmatter`、`frontmatter-modal-close`、`fm-input-title`、`fm-input-author`、`fm-select-theme`、`fm-select-transition`、`fm-modal-cancel`、`fm-modal-insert` |
| 操作与交互 | 打开元数据 → 读取现有标题、作者、演示主题/转场 → 编辑 → 有效 YAML 引号转义后更新受控字段。保留文献、自定义键、注释和其他演示设置；支持普通缩进及行内映射。文档在弹窗期间变化会拒绝旧修改；复杂锚点/别名等结构要求在源码编辑，不静默改写或损坏。 |
| 前端实现（已核对） | [openFrontmatterModal](../../assets/js/editor/editor.js)；[insertFrontmatterFromModal](../../assets/js/editor/editor.js) |
| 后端实现 | 编辑器源码修改；读取/演示编译在后端解析 frontmatter |
| 实际验证 | 已有元数据读取、引号/反斜杠转义、无关字段/嵌套映射保留、水平线区分及过期修改保护通过。 |
| 证据 | [ui-tests/feature-verification.spec.js](../../ui-tests/feature-verification.spec.js) |
| 运行条件/实测边界 | 演示区域含复杂YAML锚点/别名时要求编辑源码；拒绝静默改坏元数据。 |

### 07 代码运行与图表渲染

#### F037 单个代码块运行、复制输出、清空


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `code-chunk-run-btn`、`code-chunk-copy-btn`、`code-chunk-clear-btn` |
| 操作与交互 | 文档存在 cmd/可执行属性块 → 点击运行 → 按钮禁用、状态和计时显示 → 结果显示 stdout/stderr及可捕获图像 → 可再次运行、复制输出或清空显示。output=true 且处于编辑状态时才通过编辑器事务回填输出并走保存路径；普通运行仅影响预览。 |
| 前端实现（已核对） | [renderAllCodeChunks](../../assets/js/reader/render.js)；[persistCodeChunkOutput](../../assets/js/reader/render.js) |
| 后端实现 | /api/code/run 要求 confirm:true；code_chunk_runner.rs 选择本地语言运行时或内置 SQL，限制cwd、网络/路径、超时与输出<br>[/api/code/run → parity_code::h_code_run](../../rust/readmd-kernel/src/parity_code.rs) |
| 实际验证 | 单块/全部运行入口及输出保存、复制、清空调用链核对；Rust代码执行参数/超时等单测通过。 |
| 证据 | [ui-tests/function-completion.spec.js](../../ui-tests/function-completion.spec.js) |
| 运行条件/实测边界 | 七种代码语言依赖已安装解释器；本轮未安装依赖或在本机执行每种语言。 |

#### F038 运行全部代码块


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-run-all-chunks` |
| 操作与交互 | 有可运行代码块时更多/互动显示运行代码 → 点击后按实现顺序执行各卡片；无代码块时入口隐藏/禁用。每个块保留独立运行状态、计时和输出，失败不是整个文档变成运行成功。 |
| 前端实现（已核对） | [runAllCodeChunks](../../assets/js/reader/render.js) |
| 后端实现 | 复用每块 /api/code/run；并非另一种无限权限执行器<br>[/api/code/run → parity_code::h_code_run](../../rust/readmd-kernel/src/parity_code.rs) |
| 实际验证 | 单块/全部运行入口及输出保存、复制、清空调用链核对；Rust代码执行参数/超时等单测通过。 |
| 证据 | [ui-tests/function-completion.spec.js](../../ui-tests/function-completion.spec.js) |
| 运行条件/实测边界 | 七种代码语言依赖已安装解释器；本轮未安装依赖或在本机执行每种语言。 |

#### F039 本地图表与失败回退


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `diagram-preview` |
| 操作与交互 | Mermaid、WaveDrom、DOT/Viz、Chart.js、BitField、TikZ 等按本地 vendor JS/WASM渲染；Vega/Vega-Lite也有 Rust/Node 后端路径。渲染失败保留源码并显示回退原因；主题变化重渲染。图表源码不是可随意执行的页面 HTML。 |
| 前端实现（已核对） | [renderAllDiagrams](../../assets/js/reader/render.js)；[renderLocalDiagram](../../assets/js/reader/render.js) |
| 后端实现 | /api/diagram/render、capabilities → parity_diagram.rs + diagrams.rs；当前探测枚举见 api.md<br>[/api/diagram/render → parity_diagram::h_diagram_render](../../rust/readmd-kernel/src/parity_diagram.rs) |
| 实际验证 | 真实Mermaid/WaveDrom/Bitfield/TikZ/Chart.js及Rust基础WSD/D2/ditaa离线渲染通过；失败保留源码/反馈。 |
| 证据 | [ui-tests/function-completion.spec.js](../../ui-tests/function-completion.spec.js)；[ui-tests/v238-batch-a-fixes.spec.js](../../ui-tests/v238-batch-a-fixes.spec.js) |
| 运行条件/实测边界 | 其他引擎按随包资源/本机工具能力；支持边界在native-diagrams.md。 |

#### F040 PlantUML 显式远程渲染


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `diagram-allow-remote-btn` |
| 操作与交互 | PlantUML 无本地 Java/JAR → 显示依赖缺失和允许远程按钮 → 点击该按钮再确认 → allow_remote:true 请求远程 SVG；取消则保留源码。WSD、D2、ditaa 的基础语法已由Rust离线输出SVG，不请求外部渲染；完整第三方高级语法不等同于本机基础实现，支持内容与拒绝规则见 native-diagrams.md。 |
| 前端实现（已核对） | [renderAllDiagrams](../../assets/js/reader/render.js) |
| 后端实现 | /api/diagram/render → parity_diagram.rs render_plantuml；本地优先，远程必须明确 true<br>[/api/diagram/render → parity_diagram::h_diagram_render](../../rust/readmd-kernel/src/parity_diagram.rs) |
| 实际验证 | 显式PlantUML远程按钮、请求和回退路径核对；Rust相关渲染/路径校验单测通过。 |
| 证据 | [ui-tests/v238-batch-a-fixes.spec.js](../../ui-tests/v238-batch-a-fixes.spec.js) |
| 运行条件/实测边界 | 没有向外部PlantUML服务发送用户文档；联网可用性和复杂语法未作在线保证。 |

### 08 AI 对话与文内改写

#### F041 打开、收起、全屏和调整 AI 面板


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-ai`、`ai-close`、`ai-expand-toggle`、`ai-jump-latest`、`ai-resize-handle` |
| 操作与交互 | 顶栏/主页/快捷键打开 AI → 按需加载模块和配置 → 空态显示快速提问入口 → ×收起；全屏按钮切独占布局；拖左边界改宽度；跳到最新回复滚动到底部。未配置连接时提示进入 AI设置，面板本身可打开。 |
| 前端实现（已核对） | [handleTopAiButtonClick](../../assets/js/features/ai.js)；[toggleAiFullscreen](../../assets/js/features/ai.js)；[bindAiResize](../../assets/js/features/ai.js) |
| 后端实现 | /api/modules/load、/api/ai/config、/api/ai/prompts；面板宽度 /api/settings<br>[/api/settings → h_settings](../../rust/readmd-kernel/src/server.rs)；[/api/modules → h_modules](../../rust/readmd-kernel/src/server.rs)；[/api/ai/config → h_ai_config](../../rust/readmd-kernel/src/server.rs)；[/api/ai/prompts → h_ai_prompts](../../rust/readmd-kernel/src/server.rs)；[/api/modules/load → h_modules_load](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | AI面板、选区/文档上下文、流式反馈、回答富文本/复制/重试、无痕历史及副本应用通过；请求使用受控响应。 |
| 证据 | [ui-tests/panels-upgrade.spec.js](../../ui-tests/panels-upgrade.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/ai-output-sanitization.spec.js](../../ui-tests/ai-output-sanitization.spec.js)；[ui-tests/v238-batch-a-fixes.spec.js](../../ui-tests/v238-batch-a-fixes.spec.js) |
| 运行条件/实测边界 | 真实模型效果/在线状态取决于用户配置；本轮没有使用用户密钥调用外部AI。 |

#### F042 输入、模板、选区、无痕及发送


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `ai-template`、`ai-tpl-btn`、`ai-prompt`、`ai-selection`、`ai-incognito`、`ai-run`、`ai-stop`、`ai-clear-ctx` |
| 操作与交互 | 选择 Prompt/Skill 或直接输入问题 → 可勾选仅使用选中文本，未勾选时按动作选当前文档/上下文 → 发送/键盘发送 → 请求后状态、逐字流式输出与用量 → 停止中断前端请求；新对话清空当前上下文。无痕开关抑制会话落盘，仍会把选定内容发送到所选服务。 |
| 前端实现（已核对） | [runAi](../../assets/js/features/ai.js)；[getAiTargetText](../../assets/js/features/ai.js)；[clearAiContext](../../assets/js/features/ai.js) |
| 后端实现 | /api/ai/chat → server.rs h_ai_chat → parity_aichat.rs/ai.rs/ai_providers.rs；普通会话 /api/ai/history<br>[/api/ai/chat → h_ai_chat](../../rust/readmd-kernel/src/server.rs)；[/api/ai/history → h_ai_history](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | AI面板、选区/文档上下文、流式反馈、回答富文本/复制/重试、无痕历史及副本应用通过；请求使用受控响应。 |
| 证据 | [ui-tests/panels-upgrade.spec.js](../../ui-tests/panels-upgrade.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/ai-output-sanitization.spec.js](../../ui-tests/ai-output-sanitization.spec.js)；[ui-tests/v238-batch-a-fixes.spec.js](../../ui-tests/v238-batch-a-fixes.spec.js) |
| 运行条件/实测边界 | 真实模型效果/在线状态取决于用户配置；本轮没有使用用户密钥调用外部AI。 |

#### F043 消息操作、文档上下文展开和重试


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `ai-bubble-act-btn`、`ai-code-copy`、`ai-regen-btn` |
| 操作与交互 | 回复下方四项：复制、应用到文档、另存MD、重新生成；回复代码块另有复制代码；用户文档上下文卡可展开/收起全文。请求失败显示重试按钮。重新生成删除最后一组问答后按原问题重新请求，并不是服务端“版本切换”。 |
| 前端实现（已核对） | [renderAiBubbleActions](../../assets/js/features/ai.js)；[appendAiUserBubble](../../assets/js/features/ai.js)；[regenerateLastAnswer](../../assets/js/features/ai.js) |
| 后端实现 | 复制在前端；重新生成 /api/ai/chat；应用和另存见下一项<br>[/api/ai/chat → h_ai_chat](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | AI面板、选区/文档上下文、流式反馈、回答富文本/复制/重试、无痕历史及副本应用通过；请求使用受控响应。 |
| 证据 | [ui-tests/panels-upgrade.spec.js](../../ui-tests/panels-upgrade.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/ai-output-sanitization.spec.js](../../ui-tests/ai-output-sanitization.spec.js)；[ui-tests/v238-batch-a-fixes.spec.js](../../ui-tests/v238-batch-a-fixes.spec.js) |
| 运行条件/实测边界 | 真实模型效果/在线状态取决于用户配置；本轮没有使用用户密钥调用外部AI。 |

#### F044 应用回复及 AI 副本文档


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `ai-apply`、`ai-save` |
| 操作与交互 | 应用回复：有可定位选区时改选区；续写时插在选区后/文末；无选区且不是续写则创建 AI 副本标签。选区变动或多次重复无法唯一定位时回退创建副本。编辑器事务可撤销；只有随后保存才写磁盘。另存MD可桌面选路径或浏览器下载。 |
| 前端实现（已核对） | [applyAi](../../assets/js/features/ai.js)；[saveAiAs](../../assets/js/features/ai.js) |
| 后端实现 | /api/dialog/save-file、/api/save；前端先建立可复核内容，不直接覆盖原文<br>[/api/save → h_save](../../rust/readmd-kernel/src/server.rs)；[/api/dialog/save-file → h_dialog_save_file](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | AI面板、选区/文档上下文、流式反馈、回答富文本/复制/重试、无痕历史及副本应用通过；请求使用受控响应。 |
| 证据 | [ui-tests/panels-upgrade.spec.js](../../ui-tests/panels-upgrade.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/ai-output-sanitization.spec.js](../../ui-tests/ai-output-sanitization.spec.js)；[ui-tests/v238-batch-a-fixes.spec.js](../../ui-tests/v238-batch-a-fixes.spec.js) |
| 运行条件/实测边界 | 真实模型效果/在线状态取决于用户配置；本轮没有使用用户密钥调用外部AI。 |

#### F045 编辑内 AI 操作条


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `edit-ai-close`、`edit-ai-input`、`edit-ai-submit`、`edit-ai-apply`、`edit-ai-insert`、`edit-ai-discard`、`data-ai-action` |
| 操作与交互 | 选区/光标打开文内 AI → 一次请求 → 文本预览 → 替换、插入或放弃。关闭使旧响应失效并中止前端请求；切换标签不能应用到相同文字的另一篇文档。选区无法唯一定位或来源改变时创建未保存副本；应用是可撤销的编辑事务。 |
| 前端实现（已核对） | [runEditAiAction](../../assets/js/editor/editor.js)；[applyEditAiResult](../../assets/js/editor/editor.js)；[insertEditAiResult](../../assets/js/editor/editor.js) |
| 后端实现 | /api/ai/chat 使用共享连接和Skill；修改在编辑器，/api/save 持久化<br>[/api/save → h_save](../../rust/readmd-kernel/src/server.rs)；[/api/ai/chat → h_ai_chat](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 关闭后忽略迟到响应、同文字不同标签隔离、单次请求、范围检查与安全副本通过。 |
| 证据 | [ui-tests/feature-verification.spec.js](../../ui-tests/feature-verification.spec.js)；[ui-tests/editor-upgrade.spec.js](../../ui-tests/editor-upgrade.spec.js)；[ui-tests/v238-ai-config.spec.js](../../ui-tests/v238-ai-config.spec.js) |
| 运行条件/实测边界 | AI响应为测试夹具；实际联网需有效连接。 |

### 09 AI 连接设置与会话历史

#### F046 选择连接、官方预设和上游目录


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `ai-settings-close`、`ai-provider`、`ai-provider-search`、`ai-settings-open` |
| 操作与交互 | AI设置 → 选择已保存自定义连接或26个官方预设；展开服务目录，搜索/点击上游连接卡加载相应配置。目录有1148条数据，完整条目在 providers.csv。预设是配置模板，列在目录不代表模型可用、密钥已配置或服务已实测。 |
| 前端实现（已核对） | [fillAiProviders](../../assets/js/features/ai.js)；[onAiProviderChange](../../assets/js/features/ai.js) |
| 后端实现 | /api/ai/config → server.rs h_ai_config；ai_providers.rs 提供预设，配置与凭据持久化<br>[/api/ai/config → h_ai_config](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 公开目录、连接/模型选择、保存、高级参数、密钥不回显、opaque凭据复用、连接测试成功/失败及失败重试通过。 |
| 证据 | [ui-tests/panels-settings.spec.js](../../ui-tests/panels-settings.spec.js)；[ui-tests/panels-upgrade.spec.js](../../ui-tests/panels-upgrade.spec.js)；[ui-tests/v238-ai-config.spec.js](../../ui-tests/v238-ai-config.spec.js)；[ui-tests/v238-batch-a-fixes.spec.js](../../ui-tests/v238-batch-a-fixes.spec.js) |
| 运行条件/实测边界 | 连接测试的成功/失败使用受控HTTP响应；没有在线探测1,174个供应商条目。 |

#### F047 创建/删除自定义连接及模型选择


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `ai-provider-new`、`ai-provider-delete`、`ai-model`、`ai-models-btn` |
| 操作与交互 | 点新建自定义连接 → 输入名称、URL、协议等 → 保存；仅自定义连接可删除并确认。模型是 select：获取模型向配置端点拉取列表，再选模型；预设/模型列表为空或鉴权失败显示对应反馈，不把模型选项描述为任意文本输入框。 |
| 前端实现（已核对） | [newAiProvider](../../assets/js/features/ai.js)；[deleteAiProvider](../../assets/js/features/ai.js)；[loadAiModels](../../assets/js/features/ai.js) |
| 后端实现 | /api/ai/config POST 保存/删除、/api/ai/models GET/POST 拉模型目录<br>[/api/ai/config → h_ai_config](../../rust/readmd-kernel/src/server.rs)；[/api/ai/models → h_ai_models](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 公开目录、连接/模型选择、保存、高级参数、密钥不回显、opaque凭据复用、连接测试成功/失败及失败重试通过。 |
| 证据 | [ui-tests/panels-settings.spec.js](../../ui-tests/panels-settings.spec.js)；[ui-tests/panels-upgrade.spec.js](../../ui-tests/panels-upgrade.spec.js)；[ui-tests/v238-ai-config.spec.js](../../ui-tests/v238-ai-config.spec.js)；[ui-tests/v238-batch-a-fixes.spec.js](../../ui-tests/v238-batch-a-fixes.spec.js) |
| 运行条件/实测边界 | 连接测试的成功/失败使用受控HTTP响应；没有在线探测1,174个供应商条目。 |

#### F048 密钥输入、显示切换和清除


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `ai-key`、`ai-key-toggle`、`ai-key-clear` |
| 操作与交互 | 输入新密钥 → 可切换当前输入框密码/文本显示 → 清除标记删除凭据 → 保存。已保存密钥仅显示“已配置”等状态，GET配置不回显原始密钥，打开面板不会重新填入密钥。清除按钮和显示按钮各自作用，不把“显示当前输入”写成“读回已存密钥”。 |
| 前端实现（已核对） | [syncAiKey](../../assets/js/features/ai.js)；[toggleAiKey](../../assets/js/features/ai.js)；[clearAiKey](../../assets/js/features/ai.js) |
| 后端实现 | /api/ai/config POST 保存 key/clear_key；读取使用 has_key/credential_id 等句柄信息<br>[/api/ai/config → h_ai_config](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 公开目录、连接/模型选择、保存、高级参数、密钥不回显、opaque凭据复用、连接测试成功/失败及失败重试通过。 |
| 证据 | [ui-tests/panels-settings.spec.js](../../ui-tests/panels-settings.spec.js)；[ui-tests/panels-upgrade.spec.js](../../ui-tests/panels-upgrade.spec.js)；[ui-tests/v238-ai-config.spec.js](../../ui-tests/v238-ai-config.spec.js)；[ui-tests/v238-batch-a-fixes.spec.js](../../ui-tests/v238-batch-a-fixes.spec.js) |
| 运行条件/实测边界 | 连接测试的成功/失败使用受控HTTP响应；没有在线探测1,174个供应商条目。 |

#### F049 高级参数与连接保存


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `ai-provider-name`、`ai-base-url`、`ai-url-reset`、`ai-mode`、`ai-endpoint-mode`、`ai-headers`、`ai-stream`、`ai-save-key` |
| 操作与交互 | 展开高级参数 → 名称、Base URL、重置URL；协议 auto/chat/completion/responses/messages；端点模式前缀或完整URL；自定义Headers JSON；流式开关。点保存验证Headers对象和重名 → POST配置 → 重新读配置 → 已保存并关闭。保存官方预设时创建独立 custom: 连接，避免把模板当用户配置本身。高级折叠与服务目录折叠只控制显示。 |
| 前端实现（已核对） | [saveAiSelection](../../assets/js/features/ai.js)；[readAiCustomHeaders](../../assets/js/features/ai.js) |
| 后端实现 | /api/ai/config；配置字段与 AI 请求协议归一化见 ai.rs/parity_aichat.rs<br>[/api/ai/config → h_ai_config](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 公开目录、连接/模型选择、保存、高级参数、密钥不回显、opaque凭据复用、连接测试成功/失败及失败重试通过。 |
| 证据 | [ui-tests/panels-settings.spec.js](../../ui-tests/panels-settings.spec.js)；[ui-tests/panels-upgrade.spec.js](../../ui-tests/panels-upgrade.spec.js)；[ui-tests/v238-ai-config.spec.js](../../ui-tests/v238-ai-config.spec.js)；[ui-tests/v238-batch-a-fixes.spec.js](../../ui-tests/v238-batch-a-fixes.spec.js) |
| 运行条件/实测边界 | 连接测试的成功/失败使用受控HTTP响应；没有在线探测1,174个供应商条目。 |

#### F050 测试连接


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `ai-test-connection` |
| 操作与交互 | 点测试 → 禁用测试、保存、获取模型按钮并显示“正在测试” → 先保存当前表单 → POST /api/ai/models → 成功显示连接状态和模型数，失败显示鉴权/网络/协议等提示 → finally恢复按钮。该测试验证模型目录请求，不等于实际完成一次聊天推理。 |
| 前端实现（已核对） | [testAiConnection](../../assets/js/features/ai.js) |
| 后端实现 | /api/ai/config 后 /api/ai/models；后端读取凭据并调用提供商<br>[/api/ai/config → h_ai_config](../../rust/readmd-kernel/src/server.rs)；[/api/ai/models → h_ai_models](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 公开目录、连接/模型选择、保存、高级参数、密钥不回显、opaque凭据复用、连接测试成功/失败及失败重试通过。 |
| 证据 | [ui-tests/panels-settings.spec.js](../../ui-tests/panels-settings.spec.js)；[ui-tests/panels-upgrade.spec.js](../../ui-tests/panels-upgrade.spec.js)；[ui-tests/v238-ai-config.spec.js](../../ui-tests/v238-ai-config.spec.js)；[ui-tests/v238-batch-a-fixes.spec.js](../../ui-tests/v238-batch-a-fixes.spec.js) |
| 运行条件/实测边界 | 连接测试的成功/失败使用受控HTTP响应；没有在线探测1,174个供应商条目。 |

#### F051 会话浏览、命名、保存、删除、复制及导出


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `ai-history-open`、`ai-history-close`、`ai-history-search`、`ai-history-copy`、`ai-history-export`、`ai-history-clear`、`ai-session`、`ai-save-session`、`ai-del-session` |
| 操作与交互 | 历史按钮 → 加载会话列表 → 搜索标题 → 点会话恢复问答；会话条目支持重命名及删除。保存会话写历史；删除当前/删除条目和清空所有经各自确认。复制把会话格式化成文本/Markdown；导出会话通过桌面保存/浏览器下载。静态的 ai-session/save-session/delete-session 是旧控件，当前可见性见 controls.md，不能重复当成另一套界面。 |
| 前端实现（已核对） | [loadAiSessions](../../assets/js/features/ai.js)；[renderAiSessionSelect](../../assets/js/features/ai.js)；[exportCurrentConversation](../../assets/js/features/ai.js) |
| 后端实现 | /api/ai/history 按 action/list/get/save/delete/rename/clear；store.rs AI 会话持久化<br>[/api/ai/history → h_ai_history](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 会话列表、无痕不落盘、加载/保存/删除/复制/导出调用链核对；历史渲染与AI面板回归通过。 |
| 证据 | [ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/ai-output-sanitization.spec.js](../../ui-tests/ai-output-sanitization.spec.js)；[ui-tests/all-panels-layout.spec.js](../../ui-tests/all-panels-layout.spec.js) |
| 运行条件/实测边界 | 此轮未以真实外部AI会话穷举所有会话操作组合；历史后端读写由Rust单测覆盖。 |

### 10 Prompt / Skill 工作台

#### F052 搜索、浏览和技能说明


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `ai-tpl-btn`、`tpl-close`、`tpl-search`、`tpl-close-btn` |
| 操作与交互 | 打开模板工作台 → 搜索名称/内容 → 分类分组的动态 Skill 行 → 点行查看名称、描述、角色指令、许可证/来源/版本等事实 → 选作 AI 模板。内置技能和用户技能有不同编辑/删除权限；当前隔离快照25个内置Skill，不是承诺用户数据也只有25个。 |
| 前端实现（已核对） | [renderTplList](../../assets/js/features/ai.js)；[renderSkillOverview](../../assets/js/features/ai.js)；[selectTpl](../../assets/js/features/ai.js) |
| 后端实现 | /api/ai/prompts、/api/skills；Rust 模板/skill 目录与元信息<br>[/api/skills → h_skills](../../rust/readmd-kernel/src/server.rs)；[/api/ai/prompts → h_ai_prompts](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | Skill目录、详情/编辑分离、创建校验、导入来源选择、认证失败反馈、凭据句柄及Rust技能增删/导入校验通过。 |
| 证据 | [ui-tests/v238-skill-workbench.spec.js](../../ui-tests/v238-skill-workbench.spec.js)；[ui-tests/v238-batch-a-fixes.spec.js](../../ui-tests/v238-batch-a-fixes.spec.js)；[ui-tests/all-panels-layout.spec.js](../../ui-tests/all-panels-layout.spec.js) |
| 运行条件/实测边界 | AI生成与GitHub返回使用受控响应；文件夹/ZIP需要相应本地资源，发布是本地技能发布状态。 |

#### F053 新建、编辑、复制、保存和删除


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `tpl-new`、`tpl-edit`、`tpl-copy`、`tpl-del`、`tpl-id`、`tpl-action`、`tpl-name`、`tpl-system`、`tpl-user`、`tpl-save` |
| 操作与交互 | 新建空白技能/模板 → 输入名称、系统指令、用户模板（支持 {doc}/{prompt}）→ 保存；编辑进入表单，复制内置或现有项为可改副本；删除自定义项须确认，内置模板的delete分支是重置默认内容。tpl-id/tpl-action 是隐藏表单元数据，用户不能直接点击它们。 |
| 前端实现（已核对） | [copyCurrentSkill](../../assets/js/features/ai.js)；[saveTplForm](../../assets/js/features/ai.js)；[deleteCurrentTpl](../../assets/js/features/ai.js) |
| 后端实现 | /api/ai/prompts POST 维护自定义模板；/api/skills 提供 Skill 对应能力；以保存分支为准<br>[/api/skills → h_skills](../../rust/readmd-kernel/src/server.rs)；[/api/ai/prompts → h_ai_prompts](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | Skill目录、详情/编辑分离、创建校验、导入来源选择、认证失败反馈、凭据句柄及Rust技能增删/导入校验通过。 |
| 证据 | [ui-tests/v238-skill-workbench.spec.js](../../ui-tests/v238-skill-workbench.spec.js)；[ui-tests/v238-batch-a-fixes.spec.js](../../ui-tests/v238-batch-a-fixes.spec.js)；[ui-tests/all-panels-layout.spec.js](../../ui-tests/all-panels-layout.spec.js) |
| 运行条件/实测边界 | AI生成与GitHub返回使用受控响应；文件夹/ZIP需要相应本地资源，发布是本地技能发布状态。 |

#### F054 AI 生成草稿和示例


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `tpl-ai-generate`、`skill-create-close`、`skill-create-example`、`skill-create-name`、`skill-create-purpose`、`skill-create-format`、`skill-create-cancel`、`skill-create-go` |
| 操作与交互 | AI生成 → 可填入精读示例 → 填技能名称/用途 → 输出 Markdown/结构化表格/行动清单 → 生成草稿 → 展示在工作台可编辑。生成需要共享AI连接；生成草稿不会直接变成已发布启用技能。取消/×保留原工作台。 |
| 前端实现（已核对） | [generateSkillDraft](../../assets/js/features/ai.js)；[openSkillIdeaDialog](../../assets/js/features/ai.js) |
| 后端实现 | /api/ai/chat 使用技能生成提示；发布/保存是后续独立动作<br>[/api/ai/chat → h_ai_chat](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | Skill目录、详情/编辑分离、创建校验、导入来源选择、认证失败反馈、凭据句柄及Rust技能增删/导入校验通过。 |
| 证据 | [ui-tests/v238-skill-workbench.spec.js](../../ui-tests/v238-skill-workbench.spec.js)；[ui-tests/v238-batch-a-fixes.spec.js](../../ui-tests/v238-batch-a-fixes.spec.js)；[ui-tests/all-panels-layout.spec.js](../../ui-tests/all-panels-layout.spec.js) |
| 运行条件/实测边界 | AI生成与GitHub返回使用受控响应；文件夹/ZIP需要相应本地资源，发布是本地技能发布状态。 |

#### F055 评估、发布、启用/停用和导出单个


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `tpl-publish`、`tpl-toggle`、`tpl-export-one` |
| 操作与交互 | 发布 → 先 /api/skills action:evaluate → 取得evaluation_token → 用户确认 → action:publish写入启用技能；启用/停用对可管理自定义项发enable/disable；导出单个Skill下载 .SKILL.md，内置项由元数据+指令合成。按钮文案“排版出版”实际对应 Skill 发布，不应误列为文件导出。 |
| 前端实现（已核对） | [publishCurrentSkill](../../assets/js/features/ai.js)；[toggleCurrentSkillEnabled](../../assets/js/features/ai.js)；[exportCurrentSkill](../../assets/js/features/ai.js) |
| 后端实现 | /api/skills evaluate/publish/enable/disable/export；Rust后端检查评估与确认，scripts_allowed:false<br>[/api/skills → h_skills](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | Skill目录、详情/编辑分离、创建校验、导入来源选择、认证失败反馈、凭据句柄及Rust技能增删/导入校验通过。 |
| 证据 | [ui-tests/v238-skill-workbench.spec.js](../../ui-tests/v238-skill-workbench.spec.js)；[ui-tests/v238-batch-a-fixes.spec.js](../../ui-tests/v238-batch-a-fixes.spec.js)；[ui-tests/all-panels-layout.spec.js](../../ui-tests/all-panels-layout.spec.js) |
| 运行条件/实测边界 | AI生成与GitHub返回使用受控响应；文件夹/ZIP需要相应本地资源，发布是本地技能发布状态。 |

#### F056 导入 Markdown/JSON 及全部导出


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `tpl-import-btn`、`tpl-file-input`、`tpl-export-btn` |
| 操作与交互 | 工作台导入入口和隐藏文件输入支持模板文件处理：读取 MD/frontmatter或JSON模板数组 → 解析名称/system/user/action → 写自定义模板 → 重新加载列表。导出全部将当前模板列表封成version/exported_at/templates JSON下载。当前导入主菜单默认打开GitHub来源，不应把隐藏文件 input 当成一直可見按钮；其可到达路径详见 controls.md。 |
| 前端实现（已核对） | [importTemplatesFromFile](../../assets/js/features/ai.js)；[exportTemplatesAsJson](../../assets/js/features/ai.js) |
| 后端实现 | /api/ai/prompts POST；导出JSON为浏览器 Blob<br>[/api/ai/prompts → h_ai_prompts](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | Skill目录、详情/编辑分离、创建校验、导入来源选择、认证失败反馈、凭据句柄及Rust技能增删/导入校验通过。 |
| 证据 | [ui-tests/v238-skill-workbench.spec.js](../../ui-tests/v238-skill-workbench.spec.js)；[ui-tests/v238-batch-a-fixes.spec.js](../../ui-tests/v238-batch-a-fixes.spec.js)；[ui-tests/all-panels-layout.spec.js](../../ui-tests/all-panels-layout.spec.js) |
| 运行条件/实测边界 | AI生成与GitHub返回使用受控响应；文件夹/ZIP需要相应本地资源，发布是本地技能发布状态。 |

#### F057 GitHub、目录、ZIP来源预览和选择导入


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `tpl-import-source-github`、`tpl-import-source-folder`、`tpl-import-source-zip`、`tpl-github-url`、`tpl-github-credential`、`tpl-github-preview-btn`、`tpl-folder-input`、`tpl-zip-input`、`tpl-github-apply-btn` |
| 操作与交互 | 导入菜单 → 三来源页签 → GitHub填写仓库/子目录URL，鉴权失败才显示token输入；桌面目录/ZIP用原生选择器，浏览器目录用webkitdirectory、ZIP先上传 → POST预览 → 动态技能复选框（无效项禁用）→ 选择 → 确认导入 → apply提交。当前前端冲突策略固定skip，无覆盖/重命名下拉；输入token使用后清空，目录源码/脚本元数据只展示和受控导入。 |
| 前端实现（已核对） | [selectSkillImportSource](../../assets/js/features/ai.js)；[renderGithubSkillPreview](../../assets/js/features/ai.js)；[applyGithubSkillImport](../../assets/js/features/ai.js) |
| 后端实现 | /api/skill-imports/preview、apply → batch2.rs + skill_import.rs；源扫描、校验、导入；动态check/update/delete接口当前没有对应完整来源管理面板<br>[/api/skill-imports → batch2::h_skill_imports_list](../../rust/readmd-kernel/src/batch2.rs)；[/api/skill-imports/preview → batch2::h_skill_imports_preview](../../rust/readmd-kernel/src/batch2.rs) |
| 实际验证 | Skill目录、详情/编辑分离、创建校验、导入来源选择、认证失败反馈、凭据句柄及Rust技能增删/导入校验通过。 |
| 证据 | [ui-tests/v238-skill-workbench.spec.js](../../ui-tests/v238-skill-workbench.spec.js)；[ui-tests/v238-batch-a-fixes.spec.js](../../ui-tests/v238-batch-a-fixes.spec.js)；[ui-tests/all-panels-layout.spec.js](../../ui-tests/all-panels-layout.spec.js) |
| 运行条件/实测边界 | AI生成与GitHub返回使用受控响应；文件夹/ZIP需要相应本地资源，发布是本地技能发布状态。 |

### 11 万物转 MD 与批量导入

#### F058 打开面板、选文件、选文件夹和覆盖策略


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-convert`、`w-convert`、`convert-close`、`convert-files`、`convert-folder`、`convert-overwrite` |
| 操作与交互 | 转换入口 → 万物转MD面板 → 选多个文件或目录。桌面多选原路径；浏览器多选先上传；选目录GET collect递归收集 → 自动开始批处理，无额外开始按钮。未勾覆盖时已有.md跳过，勾选覆盖时替换；图片分到OCR通道，其他到文档通道。处理中禁用新增文件/目录防止并行任务。 |
| 前端实现（已核对） | [pickConvertFiles](../../assets/js/features/convert.js)；[pickConvertFolder](../../assets/js/features/convert.js)；[enqueueBatchFiles](../../assets/js/features/batch.js) |
| 后端实现 | /api/dialog/choose-many-files、choose-folder、/api/upload、/api/convert/collect、batch；具体格式矩阵见 formats.md<br>[/api/upload → h_upload](../../rust/readmd-kernel/src/server.rs)；[/api/convert/collect → batch2::h_convert_collect](../../rust/readmd-kernel/src/batch2.rs)；[/api/convert → batch2::h_convert](../../rust/readmd-kernel/src/batch2.rs)；[/api/dialog/choose-many-files → h_dialog_choose_many_files](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 真实ZIP选择→拆包→子文件转MD；转换队列进度/错误/取消、关闭重开保持、重复提交与覆盖决策通过；143格式分派表核对。 |
| 证据 | [ui-tests/function-completion.spec.js](../../ui-tests/function-completion.spec.js)；[ui-tests/v238-batch-workbench.spec.js](../../ui-tests/v238-batch-workbench.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js) |
| 运行条件/实测边界 | 系统选择目录仅桌面可用；插件/本机工具条件见formats.md，143行不是143种均已实机穷举。 |

#### F059 任务行、进度、取消和结果目录


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `convert-list`、`batch-cancel`、`convert-open-dir` |
| 操作与交互 | 每文件行显示排队/运行/成功/跳过/失败/取消；失败附错误说明。文档批量600ms轮询progress；图片逐张OCR并保存.md。点取消全部发送job取消并阻止后续OCR，包括首项尚未开始的OCR；正在进行的系统OCR调用不保证立即中断。成功或可打开的跳过行点开输出Markdown；打开结果目录优先用所选根目录，否则输出公共目录。关闭面板保留轮询与任务锁；重开显示同一批次，处理中禁止再提交一批。 |
| 前端实现（已核对） | [makeBatchRow](../../assets/js/features/batch.js)；[pollBatchJob](../../assets/js/features/batch.js)；[onBatchCancel](../../assets/js/features/batch.js)；[showBatchOpenDir](../../assets/js/features/batch.js) |
| 后端实现 | /api/convert/progress、cancel；/api/ocr save=1&on_exists=skip/overwrite；/api/system/open-path<br>[/api/convert/progress → batch2::h_convert_progress](../../rust/readmd-kernel/src/batch2.rs)；[/api/ocr → h_ocr_parity](../../rust/readmd-kernel/src/server.rs)；[/api/convert → batch2::h_convert](../../rust/readmd-kernel/src/batch2.rs)；[/api/system/open-path → h_system_open_path](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 真实ZIP选择→拆包→子文件转MD；转换队列进度/错误/取消、关闭重开保持、重复提交与覆盖决策通过；143格式分派表核对。 |
| 证据 | [ui-tests/function-completion.spec.js](../../ui-tests/function-completion.spec.js)；[ui-tests/v238-batch-workbench.spec.js](../../ui-tests/v238-batch-workbench.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js) |
| 运行条件/实测边界 | 系统选择目录仅桌面可用；插件/本机工具条件见formats.md，143行不是143种均已实机穷举。 |

#### F060 单文件导入与同名输出决策


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `file-input` |
| 操作与交互 | 从打开文件/拖放等进入单文件转换 → 检测已有输出 → 如需决策显示覆盖/改名/取消等动态选择（由具体调用路径生成）→ /api/convert返回content/engine/fixes/out/saved/skipped → 展示虚拟或实际文档，并反馈输出是否写入。批处理覆盖勾选与单文件on_exists决策是不同入口。 |
| 前端实现（已核对） | [convertOrOcr](../../assets/js/features/convert.js)；[convertFile](../../assets/js/reader/render.js) |
| 后端实现 | /api/convert?p=…&on_exists=skip\|overwrite\|rename；batch2.rs h_convert → convert.rs convert_triple → mdcheck与输出保存<br>[/api/convert → batch2::h_convert](../../rust/readmd-kernel/src/batch2.rs) |
| 实际验证 | 真实ZIP选择→拆包→子文件转MD；转换队列进度/错误/取消、关闭重开保持、重复提交与覆盖决策通过；143格式分派表核对。 |
| 证据 | [ui-tests/function-completion.spec.js](../../ui-tests/function-completion.spec.js)；[ui-tests/v238-batch-workbench.spec.js](../../ui-tests/v238-batch-workbench.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js) |
| 运行条件/实测边界 | 系统选择目录仅桌面可用；插件/本机工具条件见formats.md，143行不是143种均已实机穷举。 |

#### F061 文件拖入、目录拖入和ZIP拆包


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `drag-overlay` |
| 操作与交互 | 拖入主阅读器或原生窗口 → 分类text/binary/folder/zip；文本直接打开，文档/图像转换或OCR，目录显示文件树；ZIP先解压到隔离临时目录、过滤条目，再把解压路径加入批量转换。浏览器无绝对路径则上传副本；桌面原生drop传完整路径；去重原生/HTML双事件。ZIP直接通过万物转换选中与拖ZIP的拆包流程不同。 |
| 前端实现（已核对） | [handleDroppedEntries](../../assets/js/core/dragdrop.js)；[extractDroppedZip](../../assets/js/core/dragdrop.js) |
| 后端实现 | /api/batch/extract-zip、upload、file、convert/batch、ocr；convert.rs ZIP路径/大小/条数/CRC限制<br>[/api/batch/extract-zip → batch2::h_batch_extract_zip](../../rust/readmd-kernel/src/batch2.rs) |
| 实际验证 | 真实ZIP选择→拆包→子文件转MD；转换队列进度/错误/取消、关闭重开保持、重复提交与覆盖决策通过；143格式分派表核对。 |
| 证据 | [ui-tests/function-completion.spec.js](../../ui-tests/function-completion.spec.js)；[ui-tests/v238-batch-workbench.spec.js](../../ui-tests/v238-batch-workbench.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js) |
| 运行条件/实测边界 | 系统选择目录仅桌面可用；插件/本机工具条件见formats.md，143行不是143种均已实机穷举。 |

#### F100 音视频识别语言


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `convert-speech-language` |
| 操作与交互 | 运行前选择自动/中文/英文/日文 → 保存在localStorage → 单文件和批次请求携带language → SAPI匹配安装的语言识别器 → 输出真实识别段落与时间戳；找不到语言模型明确失败。运行中锁定语言控件，关闭重开保持任务。 |
| 前端实现（已核对） | [initSpeechLanguage](../../assets/js/features/convert.js)；[currentSpeechLanguage](../../assets/js/features/convert.js) |
| 后端实现 | /api/convert、/api/convert/batch、/api/transcribe → transcribe.rs/speech.rs，媒体解码复用本机FFmpeg<br>[/api/transcribe → h_transcribe_parity](../../rust/readmd-kernel/src/server.rs)；[/api/convert → batch2::h_convert](../../rust/readmd-kernel/src/batch2.rs)；[/api/convert/batch → batch2::h_convert_batch](../../rust/readmd-kernel/src/batch2.rs) |
| 实际验证 | 语言设置进入单文件/拖放/批次；Rust离线语音、压缩媒体、无语音/超时/取消与绝对时间戳验证通过。 |
| 证据 | [ui-tests/all-panels-layout.spec.js](../../ui-tests/all-panels-layout.spec.js) |
| 运行条件/实测边界 | Windows已安装SAPI识别器；压缩音视频需要现有FFmpeg。识别质量依赖系统引擎。 |

### 12 插件中心

#### F062 打开、分类和运行信息


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-open-plugins`、`plugin-close` |
| 操作与交互 | 万物转MD → 插件管理；或命令面板“插件” → /api/plugins/list → 卡片。分类全部、复杂OCR、学术与文档、语音识别、网页提取、工具链桥接；运行信息折叠查看ffmpeg状态及沙箱路径。14种插件目录见 plugins.md；标题说明、体量、模型缓存、互斥能力、错误详情可展开。 |
| 前端实现（已核对） | [openPluginModal](../../assets/js/features/convert.js)；[renderPluginCards](../../assets/js/features/convert.js) |
| 后端实现 | /api/plugins/list → plugin_manager.rs plugin_manifest/native_support；按实际installed/native标志渲染<br>[/api/plugins/list → h_plugins_list](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 真实原生扩展安装→执行转换→启停持久化→重载→卸载；互斥/失败回滚/加载错误/进度与重试通过。 |
| 证据 | [ui-tests/function-completion.spec.js](../../ui-tests/function-completion.spec.js)；[ui-tests/panels-settings.spec.js](../../ui-tests/panels-settings.spec.js) |
| 运行条件/实测边界 | 14个目录项是Rust扩展配置；系统OCR/语音/FFmpeg能力需要本机已有组件。 |

#### F063 安装、重试及安装进度


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `data-action=install` |
| 操作与交互 | 未安装扩展点安装/失败后点重试 → 确认 → POST install → 轮询进度 → 原子写plugins/native/id.json、验证内容、保存manifest并回读 → 卡片显示开关/卸载。发生写入错误恢复之前配置并显示错误详情。安装的是此Rust构建中的扩展配置，不下载或冒称安装Python第三方库。 |
| 前端实现（已核对） | [startPluginInstall](../../assets/js/features/convert.js)；[refreshPluginList](../../assets/js/features/convert.js) |
| 后端实现 | /api/plugins/install → plugin_manager.rs install_plugin_async；原子安装、回读验证、manifest互斥与回滚<br>[/api/plugins/install → crate::plugin_manager::h_plugins_install](../../rust/readmd-kernel/src/plugin_manager.rs) |
| 实际验证 | 真实原生扩展安装→执行转换→启停持久化→重载→卸载；互斥/失败回滚/加载错误/进度与重试通过。 |
| 证据 | [ui-tests/function-completion.spec.js](../../ui-tests/function-completion.spec.js)；[ui-tests/panels-settings.spec.js](../../ui-tests/panels-settings.spec.js) |
| 运行条件/实测边界 | 14个目录项是Rust扩展配置；系统OCR/语音/FFmpeg能力需要本机已有组件。 |

#### F064 启用/停用和互斥切换


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `data-action=toggle` |
| 操作与交互 | 检测到已安装插件才有开关 → change时锁住全部插件开关 → POST toggle → 保存manifest.enabled → 若同能力另一插件已启用则关闭它 → 重新读清单 → 已保存。失败回滚前端缓存并提示，不把UI变亮当保存成功。内置能力没有此开关，native能力不会因为旧包开关自动切换成Python引擎。 |
| 前端实现（已核对） | [setPluginToggle](../../assets/js/features/convert.js) |
| 后端实现 | /api/plugins/toggle → plugin_manager.rs set_plugin_enabled/save_manifest；同能力互斥持久化<br>[/api/plugins/toggle → crate::plugin_manager::h_plugins_toggle](../../rust/readmd-kernel/src/plugin_manager.rs) |
| 实际验证 | 真实原生扩展安装→执行转换→启停持久化→重载→卸载；互斥/失败回滚/加载错误/进度与重试通过。 |
| 证据 | [ui-tests/function-completion.spec.js](../../ui-tests/function-completion.spec.js)；[ui-tests/panels-settings.spec.js](../../ui-tests/panels-settings.spec.js) |
| 运行条件/实测边界 | 14个目录项是Rust扩展配置；系统OCR/语音/FFmpeg能力需要本机已有组件。 |

#### F065 卸载及错误详情


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `data-action=uninstall` |
| 操作与交互 | 检测到installed卡片 → 点卸载 → 危险确认 → POST uninstall → 刷新list并反馈。后端检查忙碌/安装状态、删除沙箱内对应包文件和manifest记录，不删除Rust编译内置能力。错误详情summary可展开查看已返回诊断。 |
| 前端实现（已核对） | [startPluginUninstall](../../assets/js/features/convert.js)；[pluginErrorMarkup](../../assets/js/features/convert.js) |
| 后端实现 | /api/plugins/uninstall → plugin_manager.rs uninstall_plugin；沙箱文件/锁/记录<br>[/api/plugins/uninstall → crate::plugin_manager::h_plugins_uninstall](../../rust/readmd-kernel/src/plugin_manager.rs) |
| 实际验证 | 真实原生扩展安装→执行转换→启停持久化→重载→卸载；互斥/失败回滚/加载错误/进度与重试通过。 |
| 证据 | [ui-tests/function-completion.spec.js](../../ui-tests/function-completion.spec.js)；[ui-tests/panels-settings.spec.js](../../ui-tests/panels-settings.spec.js) |
| 运行条件/实测边界 | 14个目录项是Rust扩展配置；系统OCR/语音/FFmpeg能力需要本机已有组件。 |

### 13 OCR、网页提取和剪贴板

#### F066 扫描识别入口


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-ocr`、`w-ocr` |
| 操作与交互 | 选择图片/PDF → 单文件任务去重与busy反馈 → 按需加载OCR模块 → GET ocr → 内容/fixes展示新文档；多图逐张处理，批处理通道可写.md。无文字返回empty/提示，失败区分模块不可用、文件/格式等。Windows使用本机OCR能力；是否可用以 pick_engine/模块状态为准。 |
| 前端实现（已核对） | [chooseFile](../../assets/js/features/convert.js)；[ocrFile](../../assets/js/features/convert.js) |
| 后端实现 | /api/ocr → parity_web.rs + ocr.rs；ocr_winrt.rs WindowsOCR及PDF页渲染/布局归一化<br>[/api/ocr → h_ocr_parity](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | OCR选择/转换入口、语言/图片/PDF分流核对；Rust OCR/PDF真实文件与错误/页码/预算单测通过。 |
| 证据 | [ui-tests/v238-batch-workbench.spec.js](../../ui-tests/v238-batch-workbench.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js) |
| 运行条件/实测边界 | 需Windows OCR语言包及PDF组件；不同扫描质量/语言组合未逐一实机穷举。 |

#### F067 网页表单、粘贴、抓取页数和图片


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-web`、`w-web`、`url-close`、`url-input`、`url-paste-btn`、`url-pages-dec`、`url-pages`、`url-pages-inc`、`url-images`、`url-private` |
| 操作与交互 | 网页入口 → 填URL/粘贴剪贴板网址 → 抓取页数1–30（±、输入或滚轮）→ 勾下载图片 → 可见“私有页面”选项按宿主分支授权。页数>1触发同站链接抓取，而不是任意全站爬虫；输入Enter等同智能提取。网址无协议补https。 |
| 前端实现（已核对） | [openWebDialog](../../assets/js/features/web.js)；[webToMd](../../assets/js/features/web.js) |
| 后端实现 | /api/clipboard/read、/api/web/extract；桌面私有授权经main.rs nativeWeb IPC → desktop_web.rs隔离系统WebView；普通浏览器缺少宿主时显示能力提示<br>[/api/web/extract → h_web_extract_parity](../../rust/readmd-kernel/src/server.rs)；[/api/clipboard/read → h_clipboard_read](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 真实隔离WebView动态DOM、Cookie授权、独立桥、输入值剔除、超时/取消/关闭/撤销通过；前端批次取消保留已提取页。 |
| 证据 | [ui-tests/native-web-test.cjs](../../ui-tests/native-web-test.cjs)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js) |
| 运行条件/实测边界 | 实际验证使用本地页面；外部站点反爬、登录和网络条件不能由本机回归保证。 |

#### F068 智能提取、完整动态渲染及取消


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `url-go`、`url-render`、`url-cancel` |
| 操作与交互 | 智能提取 → 静态HTTP获取/正文清洗 → 成功显示虚拟文档、来源、资源和warning；需要JS时尝试宿主隔离系统WebView，完整动态渲染强制使用该窗口。私有页面先在隔离窗口登录并显式授权此站点，再带任务限定grant提取实际DOM；交互模式点提取此页。取消同时设置本地标记、取消HTTP任务及宿主窗口；已抓到页时保留部分成果与统计，批次结束撤销grant。超时或窗口×终止该任务；普通浏览器缺少宿主时提示能力不足。关闭面板的处理见closeWebDialog。 |
| 前端实现（已核对） | [extractOneWebPage](../../assets/js/features/web.js)；[cancelWebTask](../../assets/js/features/web.js) |
| 后端实现 | /api/web/extract、cancel → parity_web.rs/headless_renderer.rs；desktop_web.rs隔离系统WebView窗口，DOM捕获、任务授权、取消、超时、撤销授权<br>[/api/web/extract → h_web_extract_parity](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 真实隔离WebView动态DOM、Cookie授权、独立桥、输入值剔除、超时/取消/关闭/撤销通过；前端批次取消保留已提取页。 |
| 证据 | [ui-tests/native-web-test.cjs](../../ui-tests/native-web-test.cjs)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js) |
| 运行条件/实测边界 | 实际验证使用本地页面；外部站点反爬、登录和网络条件不能由本机回归保证。 |

#### F069 剪贴新建与多类型分流


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-clipboard-new` |
| 操作与交互 | 点击剪贴新建 → 显式读取剪贴板 → 文件按格式打开/转换，截图OCR；富文本HTML通过Rust本地HTML解析保留标题/强调/列表并去除脚本；单一网址填网页面板，纯文本建立虚拟文档。失败或无正文有反馈，转换不执行HTML脚本、不自动抓取资源。 |
| 前端实现（已核对） | [createFromClipboard](../../assets/js/features/clipboard.js) |
| 后端实现 | /api/clipboard/read → native_system.rs；/api/clipboard/convert-html → headless_renderer.rs；其它分流复用file/ocr/convert<br>[/api/clipboard/read → h_clipboard_read](../../rust/readmd-kernel/src/server.rs)；[/api/clipboard/convert-html → h_clipboard_convert_html](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 实际Rust HTML→Markdown保留标题/强调/列表；剪贴新建与原生桥兜底通过。 |
| 证据 | [ui-tests/function-completion.spec.js](../../ui-tests/function-completion.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js) |
| 运行条件/实测边界 | 浏览器读取剪贴板受权限/焦点限制；拒绝时反馈而不产生空白成功。 |

#### F101 隔离网页窗口授权与提取


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `readmd-capture-bar` |
| 操作与交互 | 动态/私有抓取打开隔离系统WebView；私有页面可登录后点授权此站点，产生限定任务、来源与时效的grant。交互抓取窗口点提取此页交回实际DOM，去除脚本和输入值。窗口×取消该job；超时终止；取消渲染与撤销授权独立处理，整批结束撤销授权。外部网页没有阅读器API桥，不继承阅读器cookies。 |
| 前端实现（已核对） | [capture_script](../../rust/readmd-kernel/src/desktop_web.rs)；[cancelWebTask](../../assets/js/features/web.js) |
| 后端实现 | main.rs nativeWeb/render_web_page/authorize_private_web → desktop_web.rs Manager，临时配置文件隔离；提取结果/api/web/extract<br>[/api/web/extract → h_web_extract_parity](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 真实隔离WebView动态DOM、Cookie授权、独立桥、输入值剔除、超时/取消/关闭/撤销通过；前端批次取消保留已提取页。 |
| 证据 | [ui-tests/native-web-test.cjs](../../ui-tests/native-web-test.cjs)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js) |
| 运行条件/实测边界 | 实际验证使用本地页面；外部站点反爬、登录和网络条件不能由本机回归保证。 |

### 14 多格式导出、预设和预览

#### F070 导出格式切换


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-print`、`export-close`、`export-tab-pdf`、`export-tab-docx`、`export-tab-epub`、`export-tab-html`、`export-tab-tex`、`export-tab-presentation` |
| 操作与交互 | 打开有内容文档 → 导出 → PDF/DOCX/EPUB/HTML/LaTeX/演示六页签 → 切换只展示适用配置组 → 内容优先取当前编辑内容而非只取旧磁盘文件。×关闭配置；导出只在点主按钮后执行。全部98个可输入排版参数与1条说明、范围及默认值见 exports.md。 |
| 前端实现（已核对） | [openExportModal](../../assets/js/features/export.js)；[currentExportContent](../../assets/js/features/export.js)；[renderExportSections](../../assets/js/features/export.js) |
| 后端实现 | /api/export/presets GET；导出后端按格式分流<br>[/api/export → h_export](../../rust/readmd-kernel/src/server.rs)；[/api/export/presets → h_export_presets](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 真实HTML/PDF/DOCX输出、PDF页面PNG与导出逐字节匹配、预设加载/保存/失败重试/设置持久化、空内容、取消和单次执行通过。 |
| 证据 | [ui-tests/function-completion.spec.js](../../ui-tests/function-completion.spec.js)；[ui-tests/panels-settings.spec.js](../../ui-tests/panels-settings.spec.js)；[ui-tests/ui-quality.spec.js](../../ui-tests/ui-quality.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js) |
| 运行条件/实测边界 | DOCX为共享AST/设置的版式参考，Word最终分页可能不同；文件写入需权限。 |

#### F071 预设选择、重置、自定义保存


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `exp-preset`、`exp-save-preset`、`exp-reset`、`exp-save-input`、`exp-save-ok`、`exp-save-cancel` |
| 操作与交互 | 展开全部预设 → 选极简/经典/商务或用户预设；动态预设卡也能加载 → 合并默认参数/预设 → 重绘字段和预览。改字段变成自定义；重置恢复默认；保存当前预设展开命名表单 → 校验名称 → 写入custom预设并刷新选择。取消命名不改已有配置。浏览器缺少native预设bridge时使用对应前端回退行为，持久范围以源码为准。 |
| 前端实现（已核对） | [loadExportPresets](../../assets/js/features/export.js)；[renderExportPresetSelect](../../assets/js/features/export.js)；[expSavePreset](../../assets/js/features/export.js) |
| 后端实现 | /api/export/presets GET/POST → export_styles.rs + server.rs h_export_presets；合并custom/last<br>[/api/export → h_export](../../rust/readmd-kernel/src/server.rs)；[/api/export/presets → h_export_presets](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 真实HTML/PDF/DOCX输出、PDF页面PNG与导出逐字节匹配、预设加载/保存/失败重试/设置持久化、空内容、取消和单次执行通过。 |
| 证据 | [ui-tests/function-completion.spec.js](../../ui-tests/function-completion.spec.js)；[ui-tests/panels-settings.spec.js](../../ui-tests/panels-settings.spec.js)；[ui-tests/ui-quality.spec.js](../../ui-tests/ui-quality.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js) |
| 运行条件/实测边界 | DOCX为共享AST/设置的版式参考，Word最终分页可能不同；文件写入需权限。 |

#### F072 版式设置、实时预览和大预览


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `export-opts`、`export-preview-card`、`export-preview-prev-btn`、`export-preview-next-btn`、`export-preview-close` |
| 操作与交互 | 展开定制版式 → 改字段/预设 → 收集同一份options。PDF请求后端生成真实导出产物并显示其页面；大预览按实际PDF页数翻页。内容/设置相同的导出复用缓存产物，PDF字节一致。DOCX同时生成实际文档及共享内容/参数的PDF版式参考，页面明确提示Word分页为准；HTML保留CSS预览。请求切换与关闭会取消旧请求、丢弃过期响应。 |
| 前端实现（已核对） | [renderExportSections](../../assets/js/features/export.js)；[updateExportLivePreview](../../assets/js/features/export.js)；[requestNativeExportPreview](../../assets/js/features/export.js) |
| 后端实现 | /api/export/preview → export_preview.rs/mdexport.rs；同输入的/api/export复用产物；页面栅格化ocr_winrt.rs<br>[/api/export → h_export](../../rust/readmd-kernel/src/server.rs)；[/api/export/preview → h_export_preview](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 真实HTML/PDF/DOCX输出、PDF页面PNG与导出逐字节匹配、预设加载/保存/失败重试/设置持久化、空内容、取消和单次执行通过。 |
| 证据 | [ui-tests/function-completion.spec.js](../../ui-tests/function-completion.spec.js)；[ui-tests/panels-settings.spec.js](../../ui-tests/panels-settings.spec.js)；[ui-tests/ui-quality.spec.js](../../ui-tests/ui-quality.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js) |
| 运行条件/实测边界 | DOCX为共享AST/设置的版式参考，Word最终分页可能不同；文件写入需权限。 |

#### F073 AI 排版生成


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `exp-ai-prompt`、`exp-ai-gen-btn` |
| 操作与交互 | 输入排版要求 → 生成/Enter → 使用共享AI连接和readmd-export-style技能 → 解析JSON并归一化白名单 → 合并当前options → 设为自定义 → 即时字段/预览更新。失败显示状态和Toast；不会自动导出文档。 |
| 前端实现（已核对） | [generateExportStyleWithAi](../../assets/js/features/export.js)；[normalizeExportAiPayload](../../assets/js/features/export.js) |
| 后端实现 | /api/ai/chat；返回结构化排版配置而非任意代码<br>[/api/ai/chat → h_ai_chat](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | AI排版请求、返回JSON规范化、合法配置过滤和失败反馈函数核对；导出面板布局/预设通路回归通过。 |
| 证据 | [ui-tests/panels-settings.spec.js](../../ui-tests/panels-settings.spec.js)；[ui-tests/all-panels-layout.spec.js](../../ui-tests/all-panels-layout.spec.js) |
| 运行条件/实测边界 | 没有调用外部AI生成真实排版；有效返回仍需实际连接/模型。 |

#### F074 执行、取消、打开结果和定位


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `export-run`、`export-cancel`、`export-open`、`export-reveal` |
| 操作与交互 | 点导出 → ReadMDTask防重复并显示忙状态 → 桌面选择输出位置 → 按格式导出 → 显示路径/警告。通用导出带task_id可取消，尽可能临时文件提交；TeX有资源目录提交点限制，EPUB不提供同级取消。完成后桌面可打开文件/资源管理器定位；路径已移动显示错误。关闭弹窗与任务取消是不同动作。 |
| 前端实现（已核对） | [runExportOnce](../../assets/js/features/export.js) |
| 后端实现 | /api/export → mdexport.rs；EPUB /api/export/epub；演示Rust经/export写离线HTML，浏览器fallback /api/export/presentation；取消 /api/task/cancel<br>[/api/export → h_export](../../rust/readmd-kernel/src/server.rs)；[/api/task/cancel → h_task_cancel](../../rust/readmd-kernel/src/server.rs)；[/api/export/epub → batch2::h_export_epub](../../rust/readmd-kernel/src/batch2.rs)；[/api/export/presentation → batch2::h_export_presentation](../../rust/readmd-kernel/src/batch2.rs) |
| 实际验证 | 真实HTML/PDF/DOCX输出、PDF页面PNG与导出逐字节匹配、预设加载/保存/失败重试/设置持久化、空内容、取消和单次执行通过。 |
| 证据 | [ui-tests/function-completion.spec.js](../../ui-tests/function-completion.spec.js)；[ui-tests/panels-settings.spec.js](../../ui-tests/panels-settings.spec.js)；[ui-tests/ui-quality.spec.js](../../ui-tests/ui-quality.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js) |
| 运行条件/实测边界 | DOCX为共享AST/设置的版式参考，Word最终分页可能不同；文件写入需权限。 |

#### F075 系统浏览器打印


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `export-print` |
| 操作与交互 | 导出面板相应入口 → 打开打印视图/调用浏览器打印 → 用户在系统对话框选打印机或另存PDF；它和Rust直接生成PDF路径不同，输出由浏览器打印引擎决定。 |
| 前端实现（已核对） | [bindEvents](../../assets/app.js) |
| 后端实现 | 前端window.print/宿主打印桥按当前绑定调用；不计为第七种格式 |
| 实际验证 | 打印入口调用window.print及文档可用性条件核对；打印CSS与阅读/导出路径回归通过。 |
| 证据 | [ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js) |
| 运行条件/实测边界 | 未触发实体打印机或在OS打印对话框中写出文件；由用户选择打印目标。 |

### 15 演讲演示与局域网共享

#### F076 演示开启与播放器


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-presentation-menu`、`presentation-modal` |
| 操作与交互 | 当前Markdown → 更多/互动/演示或F5 → POST编译幻灯片HTML → 模态iframe播放器。主题11项、转场6项；A−20px/A24px/A+28px；总览、全屏、退出全屏、关闭；Reveal内部还有上一页/下一页、垂直页、进度点击、Esc总览等条件控件。主题和全屏状态同步到iframe；退出恢复原阅读器。 |
| 前端实现（已核对） | [launchPresentationMode](../../assets/js/reader/render.js)；[togglePresentationFullscreen](../../assets/js/reader/render.js) |
| 后端实现 | /api/export/presentation → mdexport.rs render_presentation_html；本地Reveal资源+presentation-bootstrap.js<br>[/api/export → h_export](../../rust/readmd-kernel/src/server.rs)；[/api/export/presentation → batch2::h_export_presentation](../../rust/readmd-kernel/src/batch2.rs) |
| 实际验证 | 实际Reveal播放、演示工具栏、主题/字号/全屏/Esc及CSP下加载通过；演示导出命令与播放命令独立。 |
| 证据 | [ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/frontend-audit.spec.js](../../ui-tests/frontend-audit.spec.js)；[ui-tests/function-completion.spec.js](../../ui-tests/function-completion.spec.js) |
| 运行条件/实测边界 | 系统全屏受宿主权限/用户手势约束；PDF演示输出另按导出条件。 |

#### F077 开启/关闭共享与扫码访问


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-share`、`share-start`、`share-stop`、`share-close`、`share-refresh` |
| 操作与交互 | 打开共享 → 加载/刷新状态 → 开启服务 → 含访问令牌的二维码/地址 → 手机同网访问可点击目录 → Markdown 阅读、链接/图片、下载源文件与回到上级。未保存文档或编辑草稿使用隔离临时快照；磁盘文档共享所在目录。关闭共享停止该会话；重新开启不会恢复旧监听。失败不提示成功，重复点击单次执行，旧状态/二维码不能覆盖最新结果；×仅收起面板。 |
| 前端实现（已核对） | [startShare](../../assets/js/features/share.js)；[refreshShareStatus](../../assets/js/features/share.js) |
| 后端实现 | /api/share/start、status、stop → server.rs 共享状态/监听/令牌访问控制；QR由vendor qrcode生成<br>[/api/share/start → h_share_start](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 真实Rust LAN启动→拒绝匿名→认证目录/Markdown/源下载/HEAD→关闭；草稿快照、重启不恢复旧监听及失败重试通过。 |
| 证据 | [ui-tests/feature-verification.spec.js](../../ui-tests/feature-verification.spec.js) |
| 运行条件/实测边界 | 跨设备需同网络和防火墙允许；自动测试在本机实际HTTP访问，没有拿实体手机扫码。 |

### 16 知识图谱和双向链接

#### F078 图谱打开与视图控制


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-graph`、`graph-btn-close`、`graph-btn-reset`、`graph-btn-zoom-in`、`graph-btn-zoom-out`、`graph-btn-dimension`、`graph-btn-fx` |
| 操作与交互 | 有当前文档 → 图谱/命令Ctrl+G → 文件文档先索引目录再读图谱，虚拟文档建立当前内容图 → Canvas显示关联网络；重置视角、放大/缩小、2D/3D切换、特效开关、关闭。特效偏好保存在readmd-graph-fx；无双链常驻按钮可能隐藏，命令可在有文档时调用。 |
| 前端实现（已核对） | [open](../../assets/js/features/graph.js)；[createModal](../../assets/js/features/graph.js) |
| 后端实现 | /api/links/index、graph → link_indexer.rs + store.rs；力布局/绘图前端<br>[/api/links/index → h_links_index](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 3D图谱旋转/维度切换/搜索、文件夹先索引及空目录回退、反向链接过滤/页签/未创建目标实际UI回归通过；Rust知识图谱单测通过。 |
| 证据 | [ui-tests/workspace-ux.spec.js](../../ui-tests/workspace-ux.spec.js)；[ui-tests/all-panels-layout.spec.js](../../ui-tests/all-panels-layout.spec.js) |
| 运行条件/实测边界 | 此轮没有对任意大型用户知识库逐个节点操作；不存在目标文件时保持明确不可用状态。 |

#### F079 图谱筛选、节点选择与手势


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `graph-search`、`graph-labels`、`graph-neighbors`、`graph-canvas`、`graph-node-list` |
| 操作与交互 | 输入节点/别名过滤列表，Enter选首个命中；显示名称开关、仅邻近关联开关；点节点/列表选中，详情有打开笔记；双击节点打开文件。拖节点牵引、拖背景旋转、Shift拖平移、滚轮缩放；键盘方向旋转、±缩放、0重置、Enter打开选中笔记。Esc先取消选择再关闭。 |
| 前端实现（已核对） | [renderList](../../assets/js/features/graph.js)；[onPointerDown](../../assets/js/features/graph.js)；[select](../../assets/js/features/graph.js) |
| 后端实现 | 图交互前端；打开笔记 /api/file<br>[/api/file → h_file](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 3D图谱旋转/维度切换/搜索、文件夹先索引及空目录回退、反向链接过滤/页签/未创建目标实际UI回归通过；Rust知识图谱单测通过。 |
| 证据 | [ui-tests/workspace-ux.spec.js](../../ui-tests/workspace-ux.spec.js)；[ui-tests/all-panels-layout.spec.js](../../ui-tests/all-panels-layout.spec.js) |
| 运行条件/实测边界 | 此轮没有对任意大型用户知识库逐个节点操作；不存在目标文件时保持明确不可用状态。 |

#### F080 反向链接抽屉


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-backlinks-menu`、`backlinks-btn-close`、`backlinks-search`、`data-tab=incoming`、`data-tab=outgoing` |
| 操作与交互 | 更多/互动/反向链接 → incoming反向引用/outgoing正向链接两页签 → 搜索过滤路径/别名/上下文 → 点有效笔记行打开；未创建目标显示deadlink并禁用 → ×关闭。文档切换刷新关联；有虚拟文档但无磁盘路径时不能生成真实文件反向索引。 |
| 前端实现（已核对） | [createDrawer](../../assets/js/features/graph.js)；[refreshBacklinks](../../assets/js/features/graph.js) |
| 后端实现 | /api/links/backlinks → link_indexer.rs/store.rs<br>[/api/links/backlinks → h_links_backlinks](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 3D图谱旋转/维度切换/搜索、文件夹先索引及空目录回退、反向链接过滤/页签/未创建目标实际UI回归通过；Rust知识图谱单测通过。 |
| 证据 | [ui-tests/workspace-ux.spec.js](../../ui-tests/workspace-ux.spec.js)；[ui-tests/all-panels-layout.spec.js](../../ui-tests/all-panels-layout.spec.js) |
| 运行条件/实测边界 | 此轮没有对任意大型用户知识库逐个节点操作；不存在目标文件时保持明确不可用状态。 |

### 17 修复报告、样式定制和系统设置

#### F081 自动修复报告和修复副本


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-fix`、`fix-save`、`fix-close` |
| 操作与交互 | 读取/转换后的fixes不为空或有内容时 → 修复详情 → 列每条自动修正文本及数量 → 另存修复版调用save_fixed；源文件名旁生成.readmd后缀副本，原文件保留；关闭返回阅读。修复列表是说明文本，不存在逐条“接受/拒绝”按钮。 |
| 前端实现（已核对） | [showFixModal](../../assets/js/reader/fixes.js) |
| 后端实现 | /api/file 或convert产生fixes；/api/file/save-fixed → server.rs；readmd_fix.rs/formula_repair.rs<br>[/api/file → h_file](../../rust/readmd-kernel/src/server.rs)；[/api/file/save-fixed → h_file_save_fixed](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 修复报告/副本入口、修复项目和文件读取链核对；Rust内容修复/副本/损坏文本单测通过。 |
| 证据 | [ui-tests/all-panels-layout.spec.js](../../ui-tests/all-panels-layout.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js) |
| 运行条件/实测边界 | 自动修复改变的是可识别结构问题；用户仍可比较源码并保留原文件。 |

#### F082 AI 深度排版自愈


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `fix-ai-btn` |
| 操作与交互 | 有文档 → AI 深度排版 → 单次请求。只有来源标签、编辑器实例和原文仍一致时才以可撤销事务应用；原文变化、换标签或阅读模式时建立未保存 AI 副本，保留原文件及当前草稿。HTTP/业务失败和空结果均明确反馈；不会修改干净基线来伪装成已保存。 |
| 前端实现（已核对） | [handleAiDocumentFix](../../assets/js/reader/fixes.js) |
| 后端实现 | /api/ai/chat 使用readmd-format-fix技能；最终保存 /api/save<br>[/api/save → h_save](../../rust/readmd-kernel/src/server.rs)；[/api/ai/chat → h_ai_chat](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 来源/编辑器/正文三重身份校验；阅读态、换标签、新编辑、textarea兜底生成安全副本；同编辑器可独立撤销通过。 |
| 证据 | [ui-tests/feature-verification.spec.js](../../ui-tests/feature-verification.spec.js)；[ui-tests/v238-ai-config.spec.js](../../ui-tests/v238-ai-config.spec.js) |
| 运行条件/实测边界 | 受控AI响应验证前端事务；没有以用户凭据在线修复文档。 |

#### F083 样式预设、CSS/Head和AI生成


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-style-custom`、`style-modal-close`、`btn-preset-indent`、`btn-preset-table`、`btn-preset-font`、`btn-preset-print`、`style-ai-prompt`、`style-ai-gen-btn`、`style-custom-css`、`style-custom-head`、`style-modal-cancel`、`style-modal-save`、`style-load-retry` |
| 操作与交互 | 打开样式面板 → 加载状态/失败重试 → CSS、Head、模板及 AI 生成 → 保存后应用 CSS。加载时锁定字段，旧加载不能覆盖新窗口；保存单次执行，失败保留输入；保存中继续编辑不会被关闭或覆盖。保存是主按钮，AI 生成是辅助操作；外部 Head 资源仍取决于用户声明的资源条件。 |
| 前端实现（已核对） | [openStyleModal](../../assets/app.js)；[saveStyleModal](../../assets/app.js) |
| 后端实现 | /api/style/get、save → server.rs；AI /api/ai/chat 使用readmd-style-custom<br>[/api/style/get → h_style_get](../../rust/readmd-kernel/src/server.rs)；[/api/ai/chat → h_ai_chat](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 样式加载锁、加载错误重试、保存单次执行、失败保留表单、保存中后续输入不被关闭覆盖通过。 |
| 证据 | [ui-tests/feature-verification.spec.js](../../ui-tests/feature-verification.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/all-panels-layout.spec.js](../../ui-tests/all-panels-layout.spec.js) |
| 运行条件/实测边界 | 自定义CSS/Head按已有安全策略渲染；AI生成需有效连接。 |

#### F084 界面语言、默认关联和开机自启


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-lang`、`lang-modal-close`、`lang-search-input`、`btn-assoc`、`btn-autostart` |
| 操作与交互 | 语言 → 搜索语言名/代码 → 点语言按钮即时重翻界面并保存locale；选项以本地语言清单为准。默认打开方式 → 确认/系统注册动作 → 用户级.md/.markdown/.mdown/.mkd关联；开机自启开关 → POST启用/关闭并回读。浏览器/LAN或非支持平台禁用/说明各自能力。 |
| 前端实现（已核对） | [assets/js/core/i18n.js](../../assets/js/core/i18n.js)；[installAssoc](../../assets/js/core/history.js)；[toggleAutostart](../../assets/js/core/settings.js) |
| 后端实现 | /api/system/assoc、/api/autostart/get、set；native_system.rs/win_registry.rs；语言localStorage/系统language读取<br>[/api/autostart/get → h_autostart_get](../../rust/readmd-kernel/src/server.rs)；[/api/system/assoc → h_system_assoc](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 46语言目录/keys一致、语言切换UI通过；关联/自启Rust注册表计划/边界与桥错误处理通过。 |
| 证据 | [ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/feature-verification.spec.js](../../ui-tests/feature-verification.spec.js) |
| 运行条件/实测边界 | 本轮未修改用户默认应用关联或实际开机启动项；只测受控调用及注册表计划。 |

### 18 应用更新

#### F085 检查、下载、取消、应用和浏览器查看


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-check-update`、`status-update-badge`、`update-close`、`update-use-mirror`、`btn-update-browser`、`btn-update-cancel`、`btn-update-start` |
| 操作与交互 | 检查更新 → 可验证安装包和镜像选项 → 启动下载 → 不重叠的状态轮询 → 哈希校验 → 应用安装。状态服务失败显示并重试，连续失败允许收起面板；取消失败有反馈。安装启动失败保留已校验包并提供重试安装，不重复下载。测试使用受控响应和无效包验证，未替换用户当前安装。 |
| 前端实现（已核对） | [checkUpdate](../../assets/js/features/updater.js)；[startUpdateDownload](../../assets/js/features/updater.js)；[cancelUpdateDownload](../../assets/js/features/updater.js) |
| 后端实现 | /api/update/check、download、status、cancel、apply → batch2.rs/updater.rs/native_system.rs<br>[/api/update/check → batch2::h_update_check](../../rust/readmd-kernel/src/batch2.rs) |
| 实际验证 | 更新校验要求、轮询不重叠、状态错误反馈、取消失败及安装失败可重试且不再下载通过；Rust更新验证单测通过。 |
| 证据 | [ui-tests/feature-verification.spec.js](../../ui-tests/feature-verification.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/all-panels-layout.spec.js](../../ui-tests/all-panels-layout.spec.js) |
| 运行条件/实测边界 | 更新端点/安装响应为夹具；没有实际下载外部更新或替换当前安装。 |

### 19 桌宠设置、角色库和伴读互动

#### F086 桌宠工作台、预览和页签


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-pet`、`pet-settings-close`、`data-pet-section` |
| 操作与交互 | 设置/桌宠 → 加载运行状态与角色库 → 左侧完整角色预览、启用开关 → 角色/陪伴/设置三页签。切页签只切对应内容；预览使用精灵图完整帧/Live2D模型或Bongo预览资源，实际桌面窗口和设置预览不是同一个渲染实例。 |
| 前端实现（已核对） | [openPetSettings](../../assets/js/features/pet-batch.js)；[updateCharacterPreview](../../assets/js/features/pet-batch.js)；[init](../../assets/js/features/pet-workbench.js) |
| 后端实现 | /api/pets/status、/api/pets、上游角色目录本地资源；pet_catalog.rs/manifest数据<br>[/api/pets → parity_pets::h_pets](../../rust/readmd-kernel/src/parity_pets.rs)；[/api/pets/status → parity_pets::h_pets_status](../../rust/readmd-kernel/src/parity_pets.rs) |
| 实际验证 | 角色预览实际解码、伴读使者图片、搜索/收藏/选择回滚、紧凑尺寸/透明度/锁定/置顶及真实Rust偏好持久化通过。 |
| 证据 | [ui-tests/pet-workbench.spec.js](../../ui-tests/pet-workbench.spec.js)；[ui-tests/pet-window-settings.spec.js](../../ui-tests/pet-window-settings.spec.js)；[ui-tests/pet-runtime.spec.js](../../ui-tests/pet-runtime.spec.js)；[ui-tests/pet-overhaul.spec.js](../../ui-tests/pet-overhaul.spec.js) |
| 运行条件/实测边界 | 新增角色需合法精灵资源；桌面角色运行需独立运行时。外部图库在线下载未穷举。 |

#### F087 角色搜索、分类渲染器、收藏和选择


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `pet-renderer`、`pet-roster-search`、`pet-gallery`、`pet-roster` |
| 操作与交互 | 角色页 → 精灵图/Live2D筛选 → 搜索名称/标识 → 点击角色卡选择，星形收藏单独切换不选中角色。精灵图和Live2D各走其配置路径，选择中锁定防并发，失败恢复并显示反馈。库下拉可选伴读使者、BongoCat及当前库/目录合并项。角色目录不是每个都已经导入用户库。 |
| 前端实现（已核对） | [renderRoster](../../assets/js/features/pet-workbench.js)；[choose](../../assets/js/features/pet-workbench.js)；[refreshPetGallery](../../assets/js/features/pet-batch.js) |
| 后端实现 | /api/pets/active、configure；parity_pets.rs + desktop_pet.rs；收藏前端localStorage<br>[/api/pets → parity_pets::h_pets](../../rust/readmd-kernel/src/parity_pets.rs)；[/api/pets/active → parity_pets::h_pet_active](../../rust/readmd-kernel/src/parity_pets.rs) |
| 实际验证 | 角色预览实际解码、伴读使者图片、搜索/收藏/选择回滚、紧凑尺寸/透明度/锁定/置顶及真实Rust偏好持久化通过。 |
| 证据 | [ui-tests/pet-workbench.spec.js](../../ui-tests/pet-workbench.spec.js)；[ui-tests/pet-window-settings.spec.js](../../ui-tests/pet-window-settings.spec.js)；[ui-tests/pet-runtime.spec.js](../../ui-tests/pet-runtime.spec.js)；[ui-tests/pet-overhaul.spec.js](../../ui-tests/pet-overhaul.spec.js) |
| 运行条件/实测边界 | 新增角色需合法精灵资源；桌面角色运行需独立运行时。外部图库在线下载未穷举。 |

#### F088 导入/删除自定义精灵图


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `pet-gallery-import`、`pet-gallery-delete` |
| 操作与交互 | 角色库导入PNG → 前端文件大小17MiB检查/转base64 → 后端验证图尺寸/布局和slug → 入库刷新；删除仅自定义项显示 → 危险确认 → remove → 刷新/必要时回到默认角色。内置角色不能用此删除按钮卸载；缩略图由thumb接口或本地资源加载。 |
| 前端实现（已核对） | [initPetSystem](../../assets/js/features/pet-batch.js)；[updatePetDeleteButtonVisibility](../../assets/js/features/pet-batch.js) |
| 后端实现 | /api/pets/import、remove、thumb → parity_pets.rs；图资源与目录边界检查<br>[/api/pets → parity_pets::h_pets](../../rust/readmd-kernel/src/parity_pets.rs)；[/api/pets/import → parity_pets::h_pet_import](../../rust/readmd-kernel/src/parity_pets.rs) |
| 实际验证 | 角色预览实际解码、伴读使者图片、搜索/收藏/选择回滚、紧凑尺寸/透明度/锁定/置顶及真实Rust偏好持久化通过。 |
| 证据 | [ui-tests/pet-workbench.spec.js](../../ui-tests/pet-workbench.spec.js)；[ui-tests/pet-window-settings.spec.js](../../ui-tests/pet-window-settings.spec.js)；[ui-tests/pet-runtime.spec.js](../../ui-tests/pet-runtime.spec.js)；[ui-tests/pet-overhaul.spec.js](../../ui-tests/pet-overhaul.spec.js) |
| 运行条件/实测边界 | 新增角色需合法精灵资源；桌面角色运行需独立运行时。外部图库在线下载未穷举。 |

#### F089 运行位置、启用、尺寸、透明度和置顶


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `pet-runtime`、`pet-enabled`、`pet-scale`、`pet-opacity`、`pet-topmost`、`pet-lock-position`、`pet-sound`、`pet-reset-pos` |
| 操作与交互 | 设置页 → 阅读器内/独立桌面 → 启用 → 大小8–48（默认22，逻辑尺寸再由角色渲染器计算）→ 不透明度 → 保持置顶/锁定位置/Bongo按键音效 → onchange自动保存；range input即时数字/预览，change提交。重置位置重设widget/宿主相应位置。置顶是可选项；锁定阻止拖动。音效只对Bongo机制有意义。 |
| 前端实现（已核对） | [capturePetSettings](../../assets/js/features/pet-batch.js)；[applyPetSettings](../../assets/js/features/pet-batch.js)；[resetWidgetPosition](../../assets/js/features/pet-batch.js) |
| 后端实现 | /api/pets/configure → parity_pets.rs/desktop_pet.rs/pet_window_state.rs；保存preferences并配置独立宿主<br>[/api/pets → parity_pets::h_pets](../../rust/readmd-kernel/src/parity_pets.rs)；[/api/pets/configure → parity_pets::h_pet_configure](../../rust/readmd-kernel/src/parity_pets.rs) |
| 实际验证 | 角色预览实际解码、伴读使者图片、搜索/收藏/选择回滚、紧凑尺寸/透明度/锁定/置顶及真实Rust偏好持久化通过。 |
| 证据 | [ui-tests/pet-workbench.spec.js](../../ui-tests/pet-workbench.spec.js)；[ui-tests/pet-window-settings.spec.js](../../ui-tests/pet-window-settings.spec.js)；[ui-tests/pet-runtime.spec.js](../../ui-tests/pet-runtime.spec.js)；[ui-tests/pet-overhaul.spec.js](../../ui-tests/pet-overhaul.spec.js) |
| 运行条件/实测边界 | 新增角色需合法精灵资源；桌面角色运行需独立运行时。外部图库在线下载未穷举。 |

#### F090 安装/卸载桌宠扩展与运行时更新


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `pet-install-runtime`、`pet-install` |
| 操作与交互 | 独立桌面模式显示运行时与扩展动作 → 一键安装从本地随包资源/配置路径准备runtime → 安装桌宠扩展；已安装按钮变卸载，卸载前确认。运行时按钮在可更新状态转检查更新 → allow_network:true → 发现版本确认 → apply_update，400ms轮询进度 → 刷新状态。桌宠扩展的生命周期和pip插件中心是两套后端。 |
| 前端实现（已核对） | [installDefaultPetRuntime](../../assets/js/features/pet-batch.js)；[initPetSystem](../../assets/js/features/pet-batch.js) |
| 后端实现 | /api/pets/runtime/install、install、uninstall、check_update、apply_update、update_status → parity_pets.rs/pet_launcher.rs<br>[/api/pets → parity_pets::h_pets](../../rust/readmd-kernel/src/parity_pets.rs)；[/api/pets/runtime/install → parity_pets::h_pet_runtime_install](../../rust/readmd-kernel/src/parity_pets.rs) |
| 实际验证 | 实际现有离线桌宠包安装条件测试通过；安装入口、运行时状态、卸载/更新调用链和错误反馈核对。 |
| 证据 | [ui-tests/pet-setup.spec.js](../../ui-tests/pet-setup.spec.js)；[ui-tests/pet-runtime.spec.js](../../ui-tests/pet-runtime.spec.js) |
| 运行条件/实测边界 | UI自动安装测试使用受控运行时返回；没有升级用户全局桌宠安装。 |

#### F091 陪伴风格、气泡和养成动作


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `pet-mode-social`、`pet-mode-quiet`、`pet-bubble-toggle`、`pet-say-hello`、`pet-chat-open`、`data-pet-action` |
| 操作与交互 | 陪伴页 → 活跃/安静风格 → 开关伴读提示气泡 → 打招呼、打开AI聊天；摸摸头、喂食、玩耍、休息/唤醒五种动作 → /interact → 更新等级/体力/心情/亲密度、状态和台词。休息/唤醒按resting只显示适用项；冷却或体力不足明确反馈；安静模式不等于关闭所有交互。 |
| 前端实现（已核对） | [init](../../assets/js/features/pet-companion-actions.js)；[quiet](../../assets/js/features/pet-workbench.js) |
| 后端实现 | /api/pets/interact、configure → parity_pets.rs 持久化companion状态/冷却/恢复<br>[/api/pets → parity_pets::h_pets](../../rust/readmd-kernel/src/parity_pets.rs)；[/api/pets/interact → parity_pets::h_pet_interact](../../rust/readmd-kernel/src/parity_pets.rs) |
| 实际验证 | 主动问候/安静模式、点击台词/动作、养成命令、拖动锁定及快捷条回归通过。 |
| 证据 | [ui-tests/pet-workbench.spec.js](../../ui-tests/pet-workbench.spec.js)；[ui-tests/pet-overhaul.spec.js](../../ui-tests/pet-overhaul.spec.js) |
| 运行条件/实测边界 | 阅读器内与桌面渲染器机制不同；有意交互仍可在安静模式使用。 |

#### F092 阅读器内点击、拖动和快捷条


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `pet-character-wrap`、`pet-quick-settings`、`pet-quick-hide`、`pet-quick-chat`、`pet-quick-quiet` |
| 操作与交互 | 阅读器内角色 → 单击播放原图动作及时段/角色台词，连续戳触发组合反馈；按住超阈值拖动改变位置，锁定时受限。快捷条：设置、收起、AI、安静切换；收起保存disabled状态。气泡点击/交互、阅读进度提醒、空闲/打盹/睡眠属于有状态的陪伴机制，不强制给所有角色加键盘。 |
| 前端实现（已核对） | [initPetDirectManipulation](../../assets/js/features/pet-batch.js)；[handlePetInteractiveClick](../../assets/js/features/pet-batch.js)；[initBubbleInteractions](../../assets/js/features/pet-batch.js) |
| 后端实现 | /api/pets/configure + 前端位置/状态；阅读器内输入反馈仅当前页面事件<br>[/api/pets → parity_pets::h_pets](../../rust/readmd-kernel/src/parity_pets.rs)；[/api/pets/configure → parity_pets::h_pet_configure](../../rust/readmd-kernel/src/parity_pets.rs) |
| 实际验证 | 主动问候/安静模式、点击台词/动作、养成命令、拖动锁定及快捷条回归通过。 |
| 证据 | [ui-tests/pet-workbench.spec.js](../../ui-tests/pet-workbench.spec.js)；[ui-tests/pet-overhaul.spec.js](../../ui-tests/pet-overhaul.spec.js) |
| 运行条件/实测边界 | 阅读器内与桌面渲染器机制不同；有意交互仍可在安静模式使用。 |

### 20 独立桌宠窗口和快捷菜单

#### F093 原生拖动、透明区域和Bongo输入动作


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `native-pet-window` |
| 操作与交互 | 角色实体拖动超4px阈值触发原生窗口拖拽；透明外围按alpha命中区穿透；锁定位置禁拖。Bongo按键、鼠标左右键/移动触发左右手/键鼠反馈与可选声音；精灵图用自身动作帧、朝向、闲置和完成反馈，Live2D用模型视线/动作/表情，不假设每个角色都有键盤。 |
| 前端实现（已核对） | [packages/readmd-pet-rust/renderer/bongocat.js](../../packages/readmd-pet-rust/renderer/bongocat.js)；[packages/readmd-hermes-pet-adapter/src/live2d/stage.ts](../../packages/readmd-hermes-pet-adapter/src/live2d/stage.ts) |
| 后端实现 | Rust pet_host.rs/desktop_pet.rs/native输入与窗口事件；renderer通过控制协议配置；仅实际runtime启动时有效 |
| 实际验证 | 真实离线Bongo/Cubism、原创精灵图和Live2D渲染，按各自机制验证；输入按下/释放/100次快敲、拖动阈值、透明区域和右键菜单通过。 |
| 证据 | [ui-tests/pet-bongo.spec.js](../../ui-tests/pet-bongo.spec.js)；[ui-tests/pet-overhaul.spec.js](../../ui-tests/pet-overhaul.spec.js)；[ui-tests/pet-window-settings.spec.js](../../ui-tests/pet-window-settings.spec.js) |
| 运行条件/实测边界 | 浏览器驱动实际渲染器并使用宿主桥夹具；Rust宿主单测覆盖原生命中/拖动/输入，未在所有多屏/DPI硬件实机穷举。 |

#### F094 右键快捷菜单、角色子菜单和互动气泡


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `native-pet-context-menu`、`native-pet-bubble` |
| 操作与交互 | 右键实体 → ReadMD、剪贴板、角色、玩耍、设置、隐藏六项；角色子菜单有返回和每个角色，标记当前选择。点击宠物打开互动气泡 → 摸头、喂食、玩耍、休息/唤醒、AI聊天、关闭。菜单/气泡打开时扩大有效点击区，关闭后恢复实体范围；取消/外部点/键盘处理按renderer事件。 |
| 前端实现（已核对） | [packages/readmd-pet-rust/renderer/bongocat.js](../../packages/readmd-pet-rust/renderer/bongocat.js)；[packages/readmd-hermes-pet-adapter/src/pet-life.ts](../../packages/readmd-hermes-pet-adapter/src/pet-life.ts) |
| 后端实现 | pet_host.rs 控制消息open-app/toggle-app/character/interact/hide；/api/control/pet-menu、pets/interact/configure；主阅读器轮询消费<br>[/api/control/pet-menu → h_control_pet_menu](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 真实离线Bongo/Cubism、原创精灵图和Live2D渲染，按各自机制验证；输入按下/释放/100次快敲、拖动阈值、透明区域和右键菜单通过。 |
| 证据 | [ui-tests/pet-bongo.spec.js](../../ui-tests/pet-bongo.spec.js)；[ui-tests/pet-overhaul.spec.js](../../ui-tests/pet-overhaul.spec.js)；[ui-tests/pet-window-settings.spec.js](../../ui-tests/pet-window-settings.spec.js) |
| 运行条件/实测边界 | 浏览器驱动实际渲染器并使用宿主桥夹具；Rust宿主单测覆盖原生命中/拖动/输入，未在所有多屏/DPI硬件实机穷举。 |

#### F095 桌宠拖入文件与跨进程唤起


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `native-pet-drop` |
| 操作与交互 | 从系统文件管理器拖文件到独立桌宠 → 原生取得完整路径 → 写入耐久命令/批次队列 → 唤起主ReadMD → 主页面控制轮询消费 → Markdown打开；图片OCR；文档/多文件批量转换。队列传路径/任务而不是仅传浏览器File.name；ZIP/目录按相应可识别分派处理。 |
| 前端实现（已核对） | [receivePetBatch](../../assets/js/features/pet-batch.js)；[pollPetControls](../../assets/js/features/pet-batch.js)；[packages/readmd-pet-rust/renderer/bongocat.js](../../packages/readmd-pet-rust/renderer/bongocat.js) |
| 后端实现 | /api/control/pet-batch、pet-menu、next + pet_queue/pet_host；复用转换/OCR/文件入口<br>[/api/control/pet-batch → h_control_pet_batch](../../rust/readmd-kernel/src/server.rs) |
| 实际验证 | 桌宠文件Drop桥、批次接收/跨进程控制队列入口核对；Rust队列/启动/文件投递单测和转换端到端通过。 |
| 证据 | [ui-tests/function-completion.spec.js](../../ui-tests/function-completion.spec.js)；[ui-tests/v238-batch-workbench.spec.js](../../ui-tests/v238-batch-workbench.spec.js) |
| 运行条件/实测边界 | OS拖放需桌面宿主；未在本轮手动从Windows资源管理器穷举各类拖入组合。 |

### 21 命令面板、快捷键和通用对话框

#### F096 全局命令面板及快捷键查询


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `btn-palette`、`readmd-palette`、`shortcuts-modal` |
| 操作与交互 | 按钮/Ctrl+K打开 → 输入模糊查询 → 可点命令、最近文件或设置 → Enter执行；上下导航、Esc关闭；最近使用命令参与排序。快捷键说明可打开并搜索，展示与当前编辑/文档条件有关的命令。全部注册记录逐项见 commands.md；hidden项和条件不可用项单独注明。 |
| 前端实现（已核对） | [assets/js/shell/shell-deferred.js](../../assets/js/shell/shell-deferred.js)；[onGlobalKey](../../assets/js/shell/shell.js) |
| 后端实现 | 复用各具体功能；MRU、搜索与键位显示前端localStorage |
| 实际验证 | 命令注册、搜索排序、实际执行/MRU、列表键盘、快捷键查询和输入框保护通过。 |
| 证据 | [ui-tests/shell-upgrade.spec.js](../../ui-tests/shell-upgrade.spec.js)；[ui-tests/ui-quality.spec.js](../../ui-tests/ui-quality.spec.js) |
| 运行条件/实测边界 | 按当前文档和平台状态启用命令；每项复用对应主功能处理器。 |

#### F097 保存、冲突、选择、确认和连续模式提示


| 核对项 | 内容 |
| --- | --- |
| 可操作入口 | `close-confirm-save`、`close-confirm-discard`、`close-confirm-cancel`、`save-conflict-save-as`、`save-conflict-reload`、`save-conflict-cancel`、`confirm-action`、`confirm-cancel`、`continuous-confirm`、`continuous-cancel`、`choice-modal` |
| 操作与交互 | 通用弹层统一管理打开顺序、顶层 Escape、焦点循环/归还及背景 inert；保留保存、冲突、选择、确认和持续任务关闭保护。动态弹层同样登记，inert 控件不会参与循环。所有静态弹窗统一 44px 目标、滚动可达的表单和主题/焦点/滚动条样式；在两种指定窗口、三种语言逐面板检查。 |
| 前端实现（已核对） | [assets/js/core/modal.js](../../assets/js/core/modal.js)；[confirmAction](../../assets/js/core/dialog.js)；[askChoice](../../assets/js/core/tabs.js)；[promptSaveConflict](../../assets/js/editor/preview.js) |
| 后端实现 | 对话框前端；确认后由调用者进入save/rename/convert/plugin/skill等对应后端 |
| 实际验证 | 保存/放弃/取消、冲突恢复、叠层焦点、隐藏折叠项排除、IME/Esc、触控目标及28个弹窗168种组合可访问性通过。 |
| 证据 | [ui-tests/feature-verification.spec.js](../../ui-tests/feature-verification.spec.js)；[ui-tests/ui-quality.spec.js](../../ui-tests/ui-quality.spec.js)；[ui-tests/readmd-ui.spec.js](../../ui-tests/readmd-ui.spec.js)；[ui-tests/all-panels-layout.spec.js](../../ui-tests/all-panels-layout.spec.js) |
| 运行条件/实测边界 | 44px是交互目标；单个表格网格/长内容允许内部滚动，不把隐藏控件计作初始焦点。 |

## 可复核文件与命令

[verification.json](ui-function-inventory-2026-10-02/verification.json) 与 [verification.csv](ui-function-inventory-2026-10-02/verification.csv) 含全部101行，便于按功能ID、层级及运行条件筛选。[tools/audit-ui-features.mjs](../../tools/audit-ui-features.mjs) 检查101个实现入口、389个控件归属、来源行号及每项证据，不用源码存在冒充业务测试。

```powershell
node tools/audit-ui-features.mjs
node tools/check-wiring.mjs
node tools/check-styles.mjs
node tools/check-i18n.mjs
node tools/check-assets.mjs
node tools/check-no-python.mjs
node --test tools/test/*.test.mjs
# 复用已安装测试依赖；不安装新依赖
$env:NODE_PATH='<existing-ui-node-modules>'
$env:READMD_BIN='<local-evidence>
$env:READMD_PET_RENDERER_BUILD='<local-evidence>
$env:READMD_UI_PORT='28675'
node <existing-ui-node-modules>/@playwright/test/cli.js test --config ui-tests/playwright.config.js --project desktop --retries=0 --reporter=line
node <existing-ui-node-modules>/@playwright/test/cli.js test touch-targets.spec.js --config ui-tests/playwright.config.js --project mobile --retries=0
node ui-tests/native-web-test.cjs
cargo test --offline --locked --manifest-path rust/Cargo.toml --target-dir Z:/readmd-target/main -p readmd-kernel --lib
cargo test --offline --locked --manifest-path rust/Cargo.toml --target-dir Z:/readmd-target/main -p readmd-kernel --bin readmd
cargo test --offline --locked --manifest-path packages/readmd-pet-rust/Cargo.toml --target-dir Z:/readmd-target/pet
# 在 rust 目录执行：
cargo xtask bundle-boot --check
cargo build --offline --locked --release --target-dir Z:/readmd-target/main -p readmd-kernel --bin readmd
```

明确保留的条件：DOCX预览是版式参考；WSD/D2/ditaa只实现附件列出的基础语法；代码解释器、Windows OCR/SAPI、FFmpeg与字体使用现有安装；外部AI/GitHub/网站/更新需实际联网配置。真实运行不足的项目在逐项表中标出；没有修改用户关联、自启或执行实际升级。
