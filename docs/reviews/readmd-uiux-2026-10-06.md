# ReadMD 全模块 UI/UX 优化与覆盖清单

日期：2026-10-06。基于[完整功能清单](readmd-ui-function-inventory-2026-10-02.md)的 21 个层级、104 条操作流程。

沿用现有视觉风格和单行编辑工具栏。长表单采用固定标题、一个滚动主体、固定操作区；低频设置折叠，空状态、处理中、失败与取消保持可见。可见勾选框为 14px，标签保留 44px 点击范围。

## 本轮实际修复

- 重复事件绑定导致代码、图表、子文档插入两次。
- 打开表格设计器时先插入默认表格，取消仍产生修改。
- 切换代码/图表类型覆盖已输入内容。
- 插入后原生 WebView 的焦点恢复与 inert 释放时序冲突，直接撤销无效。
- OCR 多文件未完整等待、部分失败仍报告全部成功。
- 网页状态元素一直隐藏；加载模块期间可并发启动；Esc 取消后通用关闭逻辑立即隐藏进度；重新打开重置运行反馈。
- 图片编辑器关闭后仍接受迟到的加载结果，重新打开残留旧画布，无图片时部分按钮仍可点击。保存期间禁止重复提交、换图及关闭；失败保留编辑内容并可重试；结果不会插入到已经改变的文档。
- 剪贴板兜底返回 false 仍显示复制成功。

## 21 个层级的界面与体验核对

| 层级 | 本轮处理与保留的功能 | 验收集合 |
| --- | --- | --- |
| 01 启动、主页与全局入口 | 主页入口、最近记录与更多菜单保留；菜单限制在窗口内，长翻译自然换行，保持 44px 点击目标。 | interaction-regressions / recent-file-ux / ui-quality |
| 02 多标签、标题和文件管理 | 标签溢出和右键菜单采用同一滚动、焦点样式；文档关闭、保存冲突和恢复草稿继续使用原有保护流程。 | document-lifecycle / readmd-ui / window-upgrade |
| 03 目录、搜索、分页和阅读偏好 | 搜索输入允许压缩；计数与按钮不挤占输入。没有结果时禁用上一项/下一项，有结果后恢复。 | interaction-regressions / pagination-adaptive / reader-upgrade |
| 04 正文内可操作元素和学术阅读 | 正文引用、脚注、图片、表格、代码操作沿用完整交互；正文内容与工具提示继续分开。 | reader-upgrade / readmd-ui |
| 05 Markdown 编辑工作台 | 保持已批准的单行自适应工具栏：随宽度压缩，低频项进入更多，字数与恢复状态留在底部。 | editor-layout-production / language-production / panel-native-smoke |
| 06 插入图片、表格、公式和高级块 | 代码、图表、文档引用、元数据、Skill 使用单滚动主体及固定标题/操作区；表格网格展示完整十列。插入一次、撤销一步；取消不修改文件。图片空状态禁用无效操作，关闭后迟到的加载结果被忽略。 | panel-ux-production / all-panels-layout / panel-native-smoke |
| 07 代码运行与图表渲染 | 自定义代码/图表在切换类型时保留；Python 绘图选项仅在 Python 类型可用。代码插入、渲染与执行入口保留。 | readmd-ui / frontend-audit / panel-ux-production |
| 08 AI 对话与文内改写 | 沿用紧凑 AI 输入区和 14px 勾选框，维持标签点击与键盘操作；复制失败如实反馈，不再假报成功。文内 AI、历史与文档检查点保持原有保护。 | ai-output-sanitization / document-lifecycle / panel-ux-production |
| 09 AI 连接设置与会话历史 | AI 设置与会话历史共享表单、列表、长文本换行、焦点和滚动规则；真实模型、鉴权及网络故障测试继续保留。 | ai-model-discovery / panels-settings / v238-ai-config |
| 10 Prompt / Skill 工作台 | Skill 创建界面统一单主体滚动，底部生成/取消稳定可见；示例、长度反馈、验证、输出形式、草稿与保存逻辑保留。 | workspace-ux / v238-skill-workbench |
| 11 万物转 MD 与批量导入 | 批量转换新增明确空状态，覆盖与语音语言移入高级参数；结果列表可滚动，任务操作与状态不被长文件名挤走。 | v238-batch-workbench / v238-batch-a-fixes / document-lifecycle |
| 12 插件中心 | 插件搜索、安装、启停、卸载、分类与重试继续验证；长描述、列表焦点和滚动使用共享面板规则。 | plugins / panels-settings / v238-reliability |
| 13 OCR、网页提取和剪贴板 | OCR 顺序等待每个结果，按真实成功/失败计数；上传失败及没有文字计入失败。网页错误在面板内可见，模块加载阶段就锁定任务，取消过程保持面板/进度可见，重新打开不会重置运行状态。 | panel-ux-production / readmd-ui / document-lifecycle |
| 14 多格式导出、预设和预览 | 六种导出、全部预设与参数、预览分页继续验证；标题、操作区及长反馈使用共享规则，避免改动正文排版或预览/产物语义。 | panels-settings / readmd-ui / feature-verification |
| 15 演讲演示与局域网共享 | 共享采用固定标题/操作区和二维码空状态；增加真实复制认证链接入口，状态变化期间禁用复制，停止后隐藏复制入口。原生应用中实际开启、生成二维码、停止成功。演讲播放器保留现有完整流程。 | release-audit / frontend-audit / panel-native-smoke |
| 16 知识图谱和双向链接 | 图谱搜索、2D/3D、节点打开、反向链接筛选与未解析目标保留；运行时面板继续参与主题、键盘与回归验证。 | workspace-ux / ui-quality |
| 17 修复报告、样式定制和系统设置 | 样式、语言、关联、自启与修复报告采用统一的表单/反馈规则；不添加重复退出入口。中文插入工具标题去掉冗长英文括注。 | language-production / all-panels-layout / window-upgrade |
| 18 应用更新 | 更新操作区和状态文本使用共享换行、焦点规则；运行中的关闭/取消继续由自己的生命周期控制，避免被通用 Esc 兜底强制隐藏。 | window-upgrade / updater-release-notes / v238-reliability |
| 19 桌宠设置、角色库和伴读互动 | 桌宠角色预览、库、显示设置、折叠项与勾选框参与全部对话框布局检查；原有角色机制及已保存运行设置保留。 | pet-workbench / pet-bongo / all-panels-layout |
| 20 独立桌宠窗口和快捷菜单 | 独立桌面扩展真实 Rust 渲染器参加完整回归；拖动、原生透明窗口与角色动作的生产证据见前轮实机审计，浏览器截图不替代物理窗口验证。 | ai-model-discovery / pet-native-smoke（条件实测） |
| 21 命令面板、快捷键和通用对话框 | 全局菜单、命令与对话框统一窗口边界、44px 目标、焦点与键盘规则；插入弹窗同步释放 inert 后回到编辑器，原生窗口中可直接 Ctrl+Z。新增重复监听器质量门禁。 | ui-quality / interaction-regressions / panel-native-smoke |

## 104 条操作流程逐项索引

下面逐项保留已有设计，覆盖每个入口及所属验收集合。操作步骤、前端和后端细节继续以总清单及保存生命周期清单为准；“所属验收集合”表示相关测试范围，不等于每项都在本轮重新运行过真实第三方服务。

### 01 启动、主页与全局入口

| 编号 | 功能 | 可操作入口 | 验收集合 |
| --- | --- | --- | --- |
| F001 | 打开文档 | `btn-open`、`w-open`、`file-input` | interaction-regressions / recent-file-ux / ui-quality |
| F002 | 打开文件夹 | `btn-folder`、`w-folder` | interaction-regressions / recent-file-ux / ui-quality |
| F003 | 新建空白文档 | `w-new` | interaction-regressions / recent-file-ux / ui-quality |
| F004 | 主页快捷卡片 | `w-convert`、`w-web`、`w-ocr`、`w-ai` | interaction-regressions / recent-file-ux / ui-quality |
| F005 | 最近记录与清空 | `btn-recent`、`recent-clear`、`history-clear`、`history-close` | interaction-regressions / recent-file-ux / ui-quality |
| F006 | 返回主页 | `btn-home` | interaction-regressions / recent-file-ux / ui-quality |
| F007 | 更多菜单与分组 | `btn-more` | interaction-regressions / recent-file-ux / ui-quality |
| F008 | 刷新文件列表/当前文件 | `btn-reload` | interaction-regressions / recent-file-ux / ui-quality |

### 02 多标签、标题和文件管理

| 编号 | 功能 | 可操作入口 | 验收集合 |
| --- | --- | --- | --- |
| F009 | 切换、关闭及溢出标签 | `doc-tabs-overflow-btn` | document-lifecycle / readmd-ui / window-upgrade |
| F010 | 标签右键菜单和排序 | `tab-context-menu` | document-lifecycle / readmd-ui / window-upgrade |
| F011 | 标题重命名 | `file-title` | document-lifecycle / readmd-ui / window-upgrade |
| F012 | 另存为 Markdown | `btn-saveas` | document-lifecycle / readmd-ui / window-upgrade |
| F103 | 恢复草稿与版本历史 | `btn-document-history`、`document-history-close`、`document-history-refresh` | document-lifecycle / readmd-ui / window-upgrade |

### 03 目录、搜索、分页和阅读偏好

| 编号 | 功能 | 可操作入口 | 验收集合 |
| --- | --- | --- | --- |
| F013 | 侧栏、大纲和文件树 | `btn-toc`、`tab-toc`、`tab-files`、`side-close-btn` | interaction-regressions / pagination-adaptive / reader-upgrade |
| F014 | 文内查找 | `btn-search`、`search-input`、`search-prev`、`search-next`、`search-close` | interaction-regressions / pagination-adaptive / reader-upgrade |
| F015 | 分页跳转与连续阅读 | `pg-first-btn`、`pg-prev-btn`、`pg-page-select`、`pg-next-btn`、`pg-last-btn`、`pg-mode-toggle`、`status-pagination` | interaction-regressions / pagination-adaptive / reader-upgrade |
| F016 | 主题、缩放和禅模式 | `btn-theme`、`btn-a`、`btn-A`、`btn-zen` | interaction-regressions / pagination-adaptive / reader-upgrade |
| F017 | 浮动阅读设置 | `rd-prefs-btn`、`rd-prefs` | interaction-regressions / pagination-adaptive / reader-upgrade |
| F099 | 回到顶部 | `top-btn` | interaction-regressions / pagination-adaptive / reader-upgrade |

### 04 正文内可操作元素和学术阅读

| 编号 | 功能 | 可操作入口 | 验收集合 |
| --- | --- | --- | --- |
| F018 | 正文链接、双链和锚点 | `content` | reader-upgrade / readmd-ui |
| F019 | 代码块复制、折叠提示、脚注与图片 | `content` | reader-upgrade / readmd-ui |
| F020 | 文献引用与参考文献交互 | `content` | reader-upgrade / readmd-ui |
| F098 | 代码文件顶部四个操作 | `btn-code-to-md`、`btn-code-edit`、`btn-code-ai-explain`、`btn-code-copy` | reader-upgrade / readmd-ui |

### 05 Markdown 编辑工作台

| 编号 | 功能 | 可操作入口 | 验收集合 |
| --- | --- | --- | --- |
| F021 | 编辑、保存和取消 | `btn-edit`、`edit-area`、`edit-save`、`edit-cancel` | editor-layout-production / language-production / panel-native-smoke |
| F022 | 文字格式与结构工具 | `data-md` | editor-layout-production / language-production / panel-native-smoke |
| F023 | 编辑菜单、视图和撤销栈 | `data-menu`、`edit-view-trigger`、`edit-view-focus`、`edit-view-typewriter`、`edit-view-lines`、`edit-undo`、`edit-redo` | editor-layout-production / language-production / panel-native-smoke |
| F024 | 选区浮动工具条 | `cm-sel-ai`、`cm-sel-copy`、`cm-sel-cut`、`cm-sel-paste`、`cm-selection-toolbar` | editor-layout-production / language-production / panel-native-smoke |
| F025 | 斜杠菜单与子选择器 | `edit-slash-btn` | editor-layout-production / language-production / panel-native-smoke |
| F026 | 智能编辑、粘贴与任务勾选 | `edit-area` | editor-layout-production / language-production / panel-native-smoke |
| F027 | 预览布局与分栏拖动 | `pv-trigger`、`data-pv`、`pv-sync`、`pv-splitter` | editor-layout-production / language-production / panel-native-smoke |
| F102 | 创建编辑副本 | `btn-document-copy` | editor-layout-production / language-production / panel-native-smoke |

### 06 插入图片、表格、公式和高级块

| 编号 | 功能 | 可操作入口 | 验收集合 |
| --- | --- | --- | --- |
| F028 | 图片输入及 URL 直接插入 | `img-file`、`img-file-input`、`img-url-input`、`img-url-load` | panel-ux-production / all-panels-layout / panel-native-smoke |
| F029 | 裁剪、旋转、翻转、尺寸及历史 | `img-rot-l`、`img-rot-r`、`img-flip-x`、`img-flip-y`、`img-angle`、`img-angle-number`、`img-view-zoom`、`img-ratio`、`img-out-w`、`img-out-h`、`img-size-lock`、`img-undo`、`img-redo`、`img-reset` | panel-ux-production / all-panels-layout / panel-native-smoke |
| F030 | 图片生成、插入与关闭 | `img-insert`、`img-close`、`img-close-x` | panel-ux-production / all-panels-layout / panel-native-smoke |
| F031 | 表格网格选择 | `btn-insert-table`、`table-modal-close` | panel-ux-production / all-panels-layout / panel-native-smoke |
| F032 | 公式模板选择 | `formula-open`、`formula-close`、`formula-search`、`formula-mode` | panel-ux-production / all-panels-layout / panel-native-smoke |
| F033 | 交互式代码块插入 | `btn-insert-code-chunk`、`code-chunk-modal-close`、`code-chunk-lang`、`code-chunk-opt-plot`、`code-chunk-opt-hide`、`code-chunk-code`、`code-chunk-cancel`、`code-chunk-insert` | panel-ux-production / all-panels-layout / panel-native-smoke |
| F034 | 科学图表插入 | `btn-insert-diagram`、`diagram-modal-close`、`diagram-type`、`diagram-code`、`diagram-cancel`、`diagram-insert` | panel-ux-production / all-panels-layout / panel-native-smoke |
| F035 | 引用子文档 | `btn-insert-doc-import`、`doc-import-modal-close`、`doc-import-path`、`doc-import-browse`、`doc-import-mode`、`doc-import-lines`、`doc-import-cancel`、`doc-import-insert` | panel-ux-production / all-panels-layout / panel-native-smoke |
| F036 | YAML/演示元数据 | `btn-insert-frontmatter`、`frontmatter-modal-close`、`fm-input-title`、`fm-input-author`、`fm-select-theme`、`fm-select-transition`、`fm-modal-cancel`、`fm-modal-insert` | panel-ux-production / all-panels-layout / panel-native-smoke |

### 07 代码运行与图表渲染

| 编号 | 功能 | 可操作入口 | 验收集合 |
| --- | --- | --- | --- |
| F037 | 单个代码块运行、复制输出、清空 | `code-chunk-run-btn`、`code-chunk-copy-btn`、`code-chunk-clear-btn` | readmd-ui / frontend-audit / panel-ux-production |
| F038 | 运行全部代码块 | `btn-run-all-chunks` | readmd-ui / frontend-audit / panel-ux-production |
| F039 | 本地图表与失败回退 | `diagram-preview` | readmd-ui / frontend-audit / panel-ux-production |
| F040 | PlantUML 显式远程渲染 | `diagram-allow-remote-btn` | readmd-ui / frontend-audit / panel-ux-production |

### 08 AI 对话与文内改写

| 编号 | 功能 | 可操作入口 | 验收集合 |
| --- | --- | --- | --- |
| F041 | 打开、收起、全屏和调整 AI 面板 | `btn-ai`、`ai-close`、`ai-expand-toggle`、`ai-jump-latest`、`ai-resize-handle` | ai-output-sanitization / document-lifecycle / panel-ux-production |
| F042 | 输入、模板、选区、无痕及发送 | `ai-template`、`ai-tpl-btn`、`ai-prompt`、`ai-selection`、`ai-incognito`、`ai-run`、`ai-stop`、`ai-clear-ctx` | ai-output-sanitization / document-lifecycle / panel-ux-production |
| F043 | 消息操作、文档上下文展开和重试 | `ai-bubble-act-btn`、`ai-code-copy`、`ai-regen-btn` | ai-output-sanitization / document-lifecycle / panel-ux-production |
| F044 | 应用回复及 AI 副本文档 | `ai-apply`、`ai-save` | ai-output-sanitization / document-lifecycle / panel-ux-production |
| F045 | 编辑内 AI 操作条 | `edit-ai-close`、`edit-ai-input`、`edit-ai-submit`、`edit-ai-apply`、`edit-ai-insert`、`edit-ai-discard`、`data-ai-action` | ai-output-sanitization / document-lifecycle / panel-ux-production |

### 09 AI 连接设置与会话历史

| 编号 | 功能 | 可操作入口 | 验收集合 |
| --- | --- | --- | --- |
| F046 | 选择连接、官方预设和上游目录 | `ai-settings-close`、`ai-provider`、`ai-provider-search`、`ai-settings-open` | ai-model-discovery / panels-settings / v238-ai-config |
| F047 | 创建/删除自定义连接及模型选择 | `ai-provider-new`、`ai-provider-delete`、`ai-model`、`ai-models-btn` | ai-model-discovery / panels-settings / v238-ai-config |
| F048 | 密钥输入、显示切换和清除 | `ai-key`、`ai-key-toggle`、`ai-key-clear` | ai-model-discovery / panels-settings / v238-ai-config |
| F049 | 高级参数与连接保存 | `ai-provider-name`、`ai-base-url`、`ai-url-reset`、`ai-mode`、`ai-endpoint-mode`、`ai-headers`、`ai-stream`、`ai-save-key` | ai-model-discovery / panels-settings / v238-ai-config |
| F050 | 测试连接 | `ai-test-connection` | ai-model-discovery / panels-settings / v238-ai-config |
| F051 | 会话浏览、命名、保存、删除、复制及导出 | `ai-history-open`、`ai-history-close`、`ai-history-search`、`ai-history-copy`、`ai-history-export`、`ai-history-clear`、`ai-session`、`ai-save-session`、`ai-del-session` | ai-model-discovery / panels-settings / v238-ai-config |

### 10 Prompt / Skill 工作台

| 编号 | 功能 | 可操作入口 | 验收集合 |
| --- | --- | --- | --- |
| F052 | 搜索、浏览和技能说明 | `ai-tpl-btn`、`tpl-close`、`tpl-search`、`tpl-close-btn` | workspace-ux / v238-skill-workbench |
| F053 | 新建、编辑、复制、保存和删除 | `tpl-new`、`tpl-edit`、`tpl-copy`、`tpl-del`、`tpl-id`、`tpl-action`、`tpl-name`、`tpl-system`、`tpl-user`、`tpl-save` | workspace-ux / v238-skill-workbench |
| F054 | AI 生成草稿和示例 | `tpl-ai-generate`、`skill-create-close`、`skill-create-example`、`skill-create-name`、`skill-create-purpose`、`skill-create-format`、`skill-create-cancel`、`skill-create-go` | workspace-ux / v238-skill-workbench |
| F055 | 评估、发布、启用/停用和导出单个 | `tpl-publish`、`tpl-toggle`、`tpl-export-one` | workspace-ux / v238-skill-workbench |
| F056 | 导入 Markdown/JSON 及全部导出 | `tpl-import-btn`、`tpl-file-input`、`tpl-export-btn` | workspace-ux / v238-skill-workbench |
| F057 | GitHub、目录、ZIP来源预览和选择导入 | `tpl-import-source-github`、`tpl-import-source-folder`、`tpl-import-source-zip`、`tpl-github-url`、`tpl-github-credential`、`tpl-github-preview-btn`、`tpl-folder-input`、`tpl-zip-input`、`tpl-github-apply-btn` | workspace-ux / v238-skill-workbench |

### 11 万物转 MD 与批量导入

| 编号 | 功能 | 可操作入口 | 验收集合 |
| --- | --- | --- | --- |
| F058 | 打开面板、选文件、选文件夹和覆盖策略 | `btn-convert`、`w-convert`、`convert-close`、`convert-files`、`convert-folder`、`convert-overwrite` | v238-batch-workbench / v238-batch-a-fixes / document-lifecycle |
| F059 | 任务行、进度、取消和结果目录 | `convert-list`、`batch-cancel`、`convert-open-dir` | v238-batch-workbench / v238-batch-a-fixes / document-lifecycle |
| F060 | 单文件导入与同名输出决策 | `file-input` | v238-batch-workbench / v238-batch-a-fixes / document-lifecycle |
| F061 | 文件拖入、目录拖入和ZIP拆包 | `drag-overlay` | v238-batch-workbench / v238-batch-a-fixes / document-lifecycle |
| F100 | 音视频识别语言 | `convert-speech-language` | v238-batch-workbench / v238-batch-a-fixes / document-lifecycle |

### 12 插件中心

| 编号 | 功能 | 可操作入口 | 验收集合 |
| --- | --- | --- | --- |
| F062 | 插件搜索、分类和运行信息 | `btn-open-plugins`、`plugin-close`、`plugin-search`、`plugin-installed-filter`、`plugin-refresh` | plugins / panels-settings / v238-reliability |
| F063 | 安装、重试及安装进度 | `data-action=install` | plugins / panels-settings / v238-reliability |
| F064 | 启用/停用和互斥切换 | `data-action=toggle` | plugins / panels-settings / v238-reliability |
| F065 | 卸载及错误详情 | `data-action=uninstall` | plugins / panels-settings / v238-reliability |

### 13 OCR、网页提取和剪贴板

| 编号 | 功能 | 可操作入口 | 验收集合 |
| --- | --- | --- | --- |
| F066 | 扫描识别入口 | `btn-ocr`、`w-ocr` | panel-ux-production / readmd-ui / document-lifecycle |
| F067 | 网页表单、粘贴、抓取页数和图片 | `btn-web`、`w-web`、`url-close`、`url-input`、`url-paste-btn`、`url-pages-dec`、`url-pages`、`url-pages-inc`、`url-images`、`url-private` | panel-ux-production / readmd-ui / document-lifecycle |
| F068 | 智能提取、完整动态渲染及取消 | `url-go`、`url-render`、`url-cancel` | panel-ux-production / readmd-ui / document-lifecycle |
| F069 | 剪贴新建与多类型分流 | `btn-clipboard-new` | panel-ux-production / readmd-ui / document-lifecycle |
| F101 | 隔离网页窗口授权与提取 | `readmd-capture-bar` | panel-ux-production / readmd-ui / document-lifecycle |

### 14 多格式导出、预设和预览

| 编号 | 功能 | 可操作入口 | 验收集合 |
| --- | --- | --- | --- |
| F070 | 导出格式切换 | `btn-print`、`export-close`、`export-tab-pdf`、`export-tab-docx`、`export-tab-epub`、`export-tab-html`、`export-tab-tex`、`export-tab-presentation` | panels-settings / readmd-ui / feature-verification |
| F071 | 预设选择、重置、自定义保存 | `exp-preset`、`exp-save-preset`、`exp-reset`、`exp-save-input`、`exp-save-ok`、`exp-save-cancel` | panels-settings / readmd-ui / feature-verification |
| F072 | 版式设置、实时预览和大预览 | `export-opts`、`export-preview-card`、`export-preview-prev-btn`、`export-preview-next-btn`、`export-preview-close` | panels-settings / readmd-ui / feature-verification |
| F073 | AI 排版生成 | `exp-ai-prompt`、`exp-ai-gen-btn` | panels-settings / readmd-ui / feature-verification |
| F074 | 执行、取消、打开结果和定位 | `export-run`、`export-cancel`、`export-open`、`export-reveal` | panels-settings / readmd-ui / feature-verification |
| F075 | 系统浏览器打印 | `export-print` | panels-settings / readmd-ui / feature-verification |

### 15 演讲演示与局域网共享

| 编号 | 功能 | 可操作入口 | 验收集合 |
| --- | --- | --- | --- |
| F076 | 演示开启与播放器 | `btn-presentation-menu`、`presentation-modal` | release-audit / frontend-audit / panel-native-smoke |
| F077 | 开启/关闭共享与扫码访问 | `btn-share`、`share-start`、`share-stop`、`share-close`、`share-refresh`、`share-copy` | release-audit / frontend-audit / panel-native-smoke |

### 16 知识图谱和双向链接

| 编号 | 功能 | 可操作入口 | 验收集合 |
| --- | --- | --- | --- |
| F078 | 图谱打开与视图控制 | `btn-graph`、`graph-btn-close`、`graph-btn-reset`、`graph-btn-zoom-in`、`graph-btn-zoom-out`、`graph-btn-dimension`、`graph-btn-fx` | workspace-ux / ui-quality |
| F079 | 图谱筛选、节点选择与手势 | `graph-search`、`graph-labels`、`graph-neighbors`、`graph-canvas`、`graph-node-list` | workspace-ux / ui-quality |
| F080 | 反向链接抽屉 | `btn-backlinks-menu`、`backlinks-btn-close`、`backlinks-search`、`data-tab=incoming`、`data-tab=outgoing` | workspace-ux / ui-quality |

### 17 修复报告、样式定制和系统设置

| 编号 | 功能 | 可操作入口 | 验收集合 |
| --- | --- | --- | --- |
| F081 | 自动修复报告和修复副本 | `btn-fix`、`fix-save`、`fix-close` | language-production / all-panels-layout / window-upgrade |
| F082 | AI 深度排版自愈 | `fix-ai-btn` | language-production / all-panels-layout / window-upgrade |
| F083 | 样式预设、CSS/Head和AI生成 | `btn-style-custom`、`style-modal-close`、`btn-preset-indent`、`btn-preset-table`、`btn-preset-font`、`btn-preset-print`、`style-ai-prompt`、`style-ai-gen-btn`、`style-custom-css`、`style-custom-head`、`style-modal-cancel`、`style-modal-save`、`style-load-retry` | language-production / all-panels-layout / window-upgrade |
| F084 | 界面语言、默认关联和开机自启 | `btn-lang`、`lang-modal-close`、`lang-search-input`、`btn-assoc`、`btn-autostart` | language-production / all-panels-layout / window-upgrade |

### 18 应用更新

| 编号 | 功能 | 可操作入口 | 验收集合 |
| --- | --- | --- | --- |
| F085 | 检查、下载、取消、应用和浏览器查看 | `btn-check-update`、`status-update-badge`、`update-close`、`update-use-mirror`、`btn-update-browser`、`btn-update-cancel`、`btn-update-start` | window-upgrade / updater-release-notes / v238-reliability |

### 19 桌宠设置、角色库和伴读互动

| 编号 | 功能 | 可操作入口 | 验收集合 |
| --- | --- | --- | --- |
| F086 | 桌宠工作台、预览和页签 | `btn-pet`、`pet-settings-close`、`data-pet-section` | pet-workbench / pet-bongo / all-panels-layout |
| F087 | 角色搜索、分类渲染器、收藏和选择 | `pet-renderer`、`pet-roster-search`、`pet-gallery`、`pet-roster` | pet-workbench / pet-bongo / all-panels-layout |
| F088 | 导入/删除自定义精灵图 | `pet-gallery-import`、`pet-gallery-delete` | pet-workbench / pet-bongo / all-panels-layout |
| F089 | 运行位置、启用、尺寸、透明度和置顶 | `pet-runtime`、`pet-enabled`、`pet-scale`、`pet-opacity`、`pet-topmost`、`pet-lock-position`、`pet-sound`、`pet-reset-pos` | pet-workbench / pet-bongo / all-panels-layout |
| F090 | 安装/卸载桌宠扩展与运行时更新 | `pet-install-runtime`、`pet-install` | pet-workbench / pet-bongo / all-panels-layout |
| F091 | 陪伴风格、气泡和养成动作 | `pet-mode-social`、`pet-mode-quiet`、`pet-bubble-toggle`、`pet-say-hello`、`pet-chat-open`、`data-pet-action` | pet-workbench / pet-bongo / all-panels-layout |
| F092 | 阅读器内点击、拖动和快捷条 | `pet-character-wrap`、`pet-quick-settings`、`pet-quick-hide`、`pet-quick-chat`、`pet-quick-quiet` | pet-workbench / pet-bongo / all-panels-layout |

### 20 独立桌宠窗口和快捷菜单

| 编号 | 功能 | 可操作入口 | 验收集合 |
| --- | --- | --- | --- |
| F093 | 原生拖动、透明区域和Bongo输入动作 | `native-pet-window` | ai-model-discovery / pet-native-smoke（条件实测） |
| F094 | 右键快捷菜单、角色子菜单和互动气泡 | `native-pet-context-menu`、`native-pet-bubble` | ai-model-discovery / pet-native-smoke（条件实测） |
| F095 | 桌宠拖入文件与跨进程唤起 | `native-pet-drop` | ai-model-discovery / pet-native-smoke（条件实测） |

### 21 命令面板、快捷键和通用对话框

| 编号 | 功能 | 可操作入口 | 验收集合 |
| --- | --- | --- | --- |
| F096 | 全局命令面板及快捷键查询 | `btn-palette`、`readmd-palette`、`shortcuts-modal` | ui-quality / interaction-regressions / panel-native-smoke |
| F097 | 保存、冲突、选择、确认和连续模式提示 | `close-confirm-save`、`close-confirm-discard`、`close-confirm-cancel`、`save-conflict-save-as`、`save-conflict-reload`、`save-conflict-cancel`、`confirm-action`、`confirm-cancel`、`continuous-confirm`、`continuous-cancel`、`choice-modal` | ui-quality / interaction-regressions / panel-native-smoke |
| F104 | 窗口控制、托盘驻留与退出恢复 | `window-minimize`、`window-maximize`、`window-close`、`window-drag-region`、`btn-close-to-tray`、`btn-app-exit`、`native-window-close`、`request_quit` | ui-quality / interaction-regressions / panel-native-smoke |

## 验证证据与边界

- 全部 29 个对话框：1160×820、1024×680，浅色/深色/护眼，简体中文/繁体中文/英语/德语/阿拉伯语，共 30 个组合全部通过。检查所有展开选项、44px 控件、窗口边界和点击遮挡。
- 完整桌面回归：319 通过，1 个触屏条件用例跳过；该轮之后的焦点、搜索可用性调整另运行相关最终回归。
- 最终相关前端/触屏回归 68 项通过；图片保存边界专项桌面/触屏回归 32 项通过；原生 Windows WebView（保留生产 CSP）7 项实测通过，46 种语言切换保留编辑草稿和单行工具栏，3 种主题验证，原始合成文件保持不变。
- i18n：46 个词库、每个 2034 个键齐全；本轮复用已有翻译，短标题通过 i18n-add 更新并保留其他语言现有译文。
- Node 测试 47 项通过；离线 boot 构建、语法、接线、样式/对比度、资源、隐私与无 Python 门禁均通过。
- [前轮生产实测](readmd-production-audit-2026-10-06.md)继续提供真实文件转换、六种导出、真实模型、14 个插件、长文档、并发保存、取消与物理桌宠的证据。本轮不把受控 OCR/网页故障夹具写成真实识别引擎或公网网站质量证明，也不承诺未知服务、任意损坏文件和所有操作系统永远无错误。

新增图片保存压力用例初轮抓到了 Esc 在关闭按钮被禁用时触发通用兜底关闭的缺口（桌面/触屏各 1 项失败），已增加运行中关闭守卫并重跑。完整记录见 [结构化验证结果](readmd-uiux-2026-10-06-results.json)。

## 可复用验证

新增 panel-ux-production.spec.js 覆盖单次插入/一步撤销、取消不修改、自定义内容保留、固定操作区、表格网格、空图片/迟到加载、图片保存失败/重试/异步文档变化、OCR 部分失败、网页取消、剪贴板失败和无结果搜索。panel-native-smoke.cjs 可指定隔离数据目录及正式安装资源，实测生产 CSP 和原生交互。发布质量流程纳入上述交互测试及三主题全对话框检查。

历史静态控件清单是 2026-10-04 的 399 项快照；本轮新增 share-copy 按钮和两项高级参数 summary，现有入口均保留，移动布局不删功能。
