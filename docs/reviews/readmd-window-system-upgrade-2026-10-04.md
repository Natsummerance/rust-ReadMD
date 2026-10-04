# ReadMD 窗口、托盘、系统设置与更新升级验收

日期：2026-10-04。机器可读验收摘要见 [checks.json](window-system-upgrade-2026-10-04/checks.json)。对应功能总清单 F009、F021、F060、F062、F084、F085、F104；共 104 条流程、399 个静态控件。本轮不修改用户实际默认应用或下载安装已发布的旧版本。

## 用户操作与真实实现

| 功能 | 操作、反馈与保存方式 | 前端与 Rust 实现 |
| --- | --- | --- |
| 应用顶栏 | Windows 原生标题栏取消，使用主题一致的应用顶栏；最小化、最大化/还原、关闭，空白处拖动、双击最大化，八方向边缘缩放。按钮至少 44px，最大化状态来自宿主。 | `index.html`、`shell.js initWindowChrome`、`panels.css`；`main.rs` Wry/Tao 同源 IPC。 |
| 品牌图标 | 顶栏与软件主页直接使用用户原有蓝紫渐变 LOGO；保留原图的字形、箭头、圆点和颜色，在各主题中保持原样。图像禁止浏览器拖动，保留标题栏拖动操作。 | 共用 `assets/icon-256.png`；网站及 VSCode 图标文件与原图逐字节相同，`readmd.ico` 的 256px 图层也与原图相同。窗口既有 32px RGBA 图标与托盘资源使用同一图案，分别核对。 |
| 全局入口 | 打开、目录、最近、命令、主题、更多移到顶栏，复用原控件和绑定；文档行保留大纲、刷新、搜索、禅模式、缩放、导出、编辑、AI。 | 移动现有 DOM；浏览器隐藏原生窗口按钮；禅模式的键盘焦点可展开文档工具栏。 |
| 托盘驻留 | 默认点 × 或 Alt+F4 收起到 Windows 托盘；WebView、标签、未保存编辑器保持运行。更多菜单可关闭驻留，设置持久化；“退出 ReadMD”完全退出。 | `native_tray.rs` Win32 `Shell_NotifyIconW`；隐藏窗口回调、资源图标、Explorer 重启重建、添加失败定时重试。托盘不可用时走正常关闭保护，避免窗口失联。 |
| 托盘恢复与菜单 | 单击/双击恢复，右键菜单打开阅读器、打开文件、退出；恢复不重建编辑器。 | Win32 回调发送 Tao 事件，显示/取消最小化/聚焦；菜单文案随界面语言更新。 |
| 热启动与文件交接 | 再次启动唤回现有窗口；从资源管理器打开 Markdown 交给驻留实例，不再新建进程和窗口。 | 既有实例描述及控制 API；无文件启动也转交唤起；外部文件请求直接发送 UI 唤醒，不依赖后台 WebView 定时器。 |
| 真实脏状态 | 只打开、进入编辑、移动光标及撤销回原文，关闭无保存弹窗。真正输入、删除或应用改写后才提示保存/不保存/取消。关闭当前编辑标签不重复确认。 | `preview.js hasUnsavedEditorChanges` 比较当前文本与保存基线，规范化 LF/CRLF/CR；`tabs.js` 同步后再确认。 |
| 新内容保护 | 空白新文档、未编辑的转换/OCR/网页预览可直接关闭；AI副本、剪贴创建、编辑副本与恢复草稿包含新内容，仍提示保存。 | `renderVirtual` 区分只读导入预览和新建内容；真实修改写入集中恢复目录，原文件不自动改变。 |
| 完全退出 | 对所有真实修改逐篇确认；取消保留编辑器，保存失败留在应用，不保存先保留可恢复记录。未修改文件在编辑模式也能直接退出。 | `ReadMDRecovery.prepareClose` → `closeAllTabs` → 保存/恢复 API → 退出 IPC。 |
| 默认打开方式 | 注册 ReadMD 后打开 Windows 默认应用设置，由用户选择 .md/.markdown/.mdown/.mkd；状态接口读取系统实际默认程序。注册成功不会伪装成已设为默认。 | HKCU ProgId、Applications、Capabilities、RegisteredApplications、OpenWithProgids；引用路径正确加引号，open command 使用 REG_SZ，并带正确引用的 --assets 参数，分离安装/开发目录的冷启动不依赖工作目录。不覆盖受保护 UserChoice 或现有扩展名默认值。 |
| 插件中心 | 搜索名称/说明/能力、分类、仅看已安装、刷新；安装/启停/卸载和错误详情保留。显示真实总数、已安装数和启用数；无需安装的内置能力与可安装引擎区别计算。 | `convert.js renderPluginCards`、既有 `/api/plugins/*` 和 `plugin_manager.rs`；状态刷新保留搜索、滚动与焦点，分类可用方向键/Home/End。运行路径信息折叠。 |
| 检查更新 | 同时触发合并成一次请求；检查按钮显示忙状态。支持环境代理和 Windows 手动代理；GitHub API、发布重定向与批准镜像回退。 | `updater.rs update_agent/check_update_live`，检查整体 45 秒预算，响应体有上限；不输出代理凭据。 |
| 联网恢复 | 断网失败不循环刷弹窗，30秒/2分钟/10分钟重试；网络恢复立即重新检查；成功后每6小时检查。失败有可操作反馈。 | `updater.js` 单请求与退避，`online` 事件重试，成功后清除过时更新提示。 |
| 下载与校验 | 选择首选镜像仍保留官方源；不可达、错误内容或 SHA256 不符时换源。取消不发布文件，下载限体积；只有验证包进入 ready。 | `batch2.rs download_with` 共用代理客户端、候选循环、清理 `.part`、进度重置、流式限额、发布前再次检查取消；每源最多2小时、无数据30秒超时，允许大安装包在慢速网络继续下载。 |
| 安装与数据保护 | 安装前保存/放弃/取消真实修改；取消不启动安装。Windows安装器真实启动；便携EXE由临时 Rust 辅助程序等待退出后校验、同步、事务替换并重启，失败还原旧程序。 | `update_install.rs`；临时更新区保留一份上一版本，后继进程清理本次辅助程序/计划；失败报告仅含稳定错误码，重启显示说明。文档与恢复目录不移动。 |

Windows 默认应用的选择流程遵循系统设置机制。[Microsoft 官方默认应用设置文档](https://learn.microsoft.com/en-us/windows/apps/develop/launch/launch-default-apps-settings)。托盘图标在 Explorer 重启后重建遵循官方说明。[Microsoft Shell_NotifyIconW](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shell_notifyiconw)。

## 验证证据与边界

- 离线锁定构建完成，前端已重新打包 `readmd.boot.js`，未添加依赖。
- Rust 内核完整回归：1846 通过、3 项既有忽略（新增更新失败报告用例已包含）；慢速下载预算调整后的下载回退测试 2/2 通过；默认关联与分离资源目录的纯计划测试 2/2 通过。
- 原生入口回归：116 项；真实 Windows 烟测验证窗口拖动、最大化/还原、最小化、托盘恢复、驻留、再次启动唤回、文件交接、退出取消与恢复。
- 前端覆盖包括全部29个弹层、两个指定窗口尺寸与中/英/繁体语言；新顶栏和插件还覆盖 720×480。界面累计覆盖143项，最终重点35项与更新2项复测全部通过。原有 `readmd-ui.spec.js` 的两个历史失败本轮均通过。
- 真实更新网络结果见 [network.json](window-system-upgrade-2026-10-04/network.json)：当前网络 3897ms 成功取得 `v2.3.9`，本地 `2.4.0` 不推荐降级；不可用代理 10519ms 返回 `update_network_error`。测试未下载安装包。
- 默认关联只做注册计划与实际状态只读测试，未改用户系统选择。Explorer 重启恢复有实现与消息机制核对，未重启用户的 Explorer。
- 不承诺所有网络都能连接：PAC/WPAD、企业自签证书、需登录的代理，以及官方和所有镜像都被封锁时可能失败；界面会反馈并重试，仍可手动下载安装。不会关闭 HTTPS 验证来绕过网络限制。
- 安装器启动与便携EXE替换已实现；事务替换/损坏包/数据保持由隔离夹具测试。未对用户当前安装实际执行更新，非Windows及非EXE包仍使用手动安装路径。
- 新效果截图仅使用隔离夹具和实际系统 WebView：[新顶栏](window-system-upgrade-2026-10-04/titlebar-home.png)、[插件中心](window-system-upgrade-2026-10-04/plugin-center.png)。截图已更新为用户原有 LOGO；1160×820、1024×680、720×480 与浅色/深色/护眼主题共9种组合验证图片正常加载、比例正确、无裁切和额外着色。全部104段 showcase 已按最新 UI 与 LOGO 重录为沉浸式素材，并重新核验隐私；F104 演示原生退出保护和恢复。最小化、最大化和托盘等宿主分支仍由原生烟测覆盖，短片未逐分支展开全部窗口动作。

## 复测命令

```powershell
cargo build --offline --locked --release --manifest-path rust/Cargo.toml --target-dir Z:/readmd-target/main -p readmd-kernel
cargo run --offline --locked --manifest-path rust/Cargo.toml --target-dir Z:/readmd-target/main -p xtask -- bundle-boot
cargo test --offline --locked --manifest-path rust/Cargo.toml --target-dir Z:/readmd-target/main -p readmd-kernel --lib -- --quiet
cargo test --offline --locked --manifest-path rust/Cargo.toml --target-dir Z:/readmd-target/main -p readmd-kernel --bin readmd -- --quiet
$env:NODE_PATH='<existing-ui-node-modules>'
$env:READMD_BIN='Z:/readmd-target/main/release/readmd.exe'
node ui-tests/window-native-smoke.cjs
node ui-tests/native-web-test.cjs
node ui-tests/update-network-smoke.cjs
node <existing-ui-node-modules>/@playwright/test/cli.js test --config=ui-tests/playwright.config.js --project=desktop readmd-ui.spec.js window-upgrade.spec.js plugins.spec.js document-lifecycle.spec.js panels-settings.spec.js all-panels-layout.spec.js
node tools/audit-ui-features.mjs
```

本轮隔离 WebView 数据、临时预览脚本与测试输出已清理；保留验收截图、清单、既有演示和最终构建。共享离线依赖与构建目录保留，避免影响其他工作树。

启动本轮构建（使用此工作树的最新资源）：

```powershell
& 'Z:/readmd-target/main/release/readmd.exe' --assets '<repository>/assets'
```
