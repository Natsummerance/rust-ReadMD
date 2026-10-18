# 禅模式与全屏联动验收（2026-10-17）

已集成当前 V0.0.5 开发代码，完成离线编译；本记录不代表安装包已发布或替换用户已安装的程序。

| 操作 | 结果 |
| --- | --- |
| 左键禅模式按钮 | 只切换禅模式，保留当前窗口／全屏状态 |
| 非全屏时右键按钮或按 F11 | 进入真实窗口全屏，同时开启禅模式 |
| 全屏中左键按钮 | 可单独退出禅模式，恢复工具栏，保持全屏 |
| 全屏中右键按钮或再次按 F11 | 同时退出全屏和禅模式 |
| 全屏中按 Esc | 同时退出全屏和禅模式，保留编辑器和已打开的对话框 |

禅模式下将鼠标移到窗口顶部即可唤出按钮。窗口切换由同一控制器管理；原生 IPC 等待窗口实际状态回传，避免把请求发出当作切换成功。连续请求合并为最后目标；按住 F11 不反复切换，输入法组合输入期间保留按键。命令面板中禅模式与全屏分开，F11 归属于全屏。

## 实测结果

- `node --test tools/test/zen-fullscreen.test.cjs`：10/10，覆盖状态转换、快速取消、外部退出、IPC 失败／超时及原生回传。
- Edge UI：`zen-fullscreen.spec.js` 三项与 `shell-upgrade.spec.js` 命令搜索一项，共 4/4；无跳过或自动重试。
- Windows 原生 WebView：`node ui-tests/zen-fullscreen-native.cjs`，7/7；使用正式 CSP、独立测试配置和真实 `Focus.md`，不替换文件接口。原生窗口从 1762×1243 进入 2560×1440 显示器全屏，退出后位置与大小精确还原；编辑内容和磁盘 CRLF 原文不变。结果见 [原生证据](zen-fullscreen-native-2026-10-17.json)。
- `check-i18n`：46 种语言、2749 个键；`check-wiring` 和 `bundle-boot --check` 通过。
- 离线 `cargo build --offline --locked -p readmd-kernel --bin readmd` 通过；现有 15 条库警告保留。
- 人工查看原生纯全屏截图，确认标题栏与操作栏恢复、窗口控制隐藏且正文可见。

前次实测暴露并修复了两个问题：旧 CSS 隐藏整个全屏标题栏，导致禅模式按钮无法点击；原生桥提前返回成功，导致窗口切换与鼠标操作竞争。测试保留实际窗口几何检查；高 DPI 的 CSS 像素舍入仅用于等待 WebView 表面就绪，不放宽原生物理窗口尺寸断言。

本次为该交互的定向回归，未重新运行整个项目的所有测试，未取得 macOS／Linux 原生证据。新增提示已提供 46 种语言译文；键值检查不代表所有译文已由母语人员审校。

## 复测与产物

原生复测需要 `READMD_BIN` 指向离线构建的 `readmd.exe`、`READMD_ASSETS_DIR` 指向本仓库 `assets`、`NODE_PATH` 指向 `ui-tests/node_modules`；当前依赖布局使用 `NODE_OPTIONS=--preserve-symlinks --preserve-symlinks-main`。`READMD_QA_POWERSHELL` 指定现有 PowerShell Core 可执行文件，仅用于读取测试进程的窗口边界，不修改机器执行策略。

本轮截图、运行报告、日志及独立测试配置位于 `.cache/zen-fullscreen-20261017/`，共约 167.72 MiB。测试进程均已退出。自动审批拒绝了本轮测试配置／数据库／WebView 缓存的递归清理，返回 `blocked by policy`，未提供更具体原因；该目录因此保留，未改用其他工具绕过。离线构建复用 `.cache/community-upgrade/build/`，没有新建另一份 Rust 编译缓存。
