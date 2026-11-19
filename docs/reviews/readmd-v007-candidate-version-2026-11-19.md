# V0.0.7 候选版本与官网发布边界核对

2026-11-19。本轮只准备 `0.0.7` 开发候选并修正版本检查，不发布、打包、创建标签、提交或推送。P1/P2 及最终原生验收门槛保持。

## 应用版本

`VERSION`、Rust workspace 与两个内部 Cargo.lock 包、`.env` 派生字段、应用页面、NSIS 默认值、Linux/玲珑、HarmonyOS 清单与桥接、VS Code 和 UI 测试包及 npm 锁文件根版本同步为 `0.0.7`。锁文件依赖版本保持原样。Rust 运行时从 `CARGO_PKG_VERSION` 读取，Mac 打包和发行 CI 从 `VERSION` 读取。三个发行说明文件明确“开发候选，未发布”，主发行说明完整保留旧版本正文。

## 官网独立权威源

官网继续使用现有本地 `website/published-release.json` 和 `website/release.json` 中的 V0.0.6，由 `website/public/assets/release-contract.mjs` 与 `website/tools/published-release.mjs` 处理下载语义、历史页面及各语言 JSON-LD 的 CSP。应用候选同步不再改写 `website/public` 或 `_headers`。

旧 xtask 在 V0.0.7 检查时列出的 9 个官网文件分别为：

- 四个 download 页面：正文 MCP 文件名仍为 `readmd-mcp-server-0.0.5.zip`，属于真实陈旧内容。官网自己的规则此前只修复无版本文件名，现已补齐带版本文件名并离线同步为 0.0.6。
- 四个 release-notes 页面：`artifactSection: v0.0.5` 是历史文章身份，保留；通用下载按钮原来指向 V0.0.5，现改为各语言的官网 `/download/`。文章正文与历史语义未被改为新候选。
- `_headers`：原有四语言首页 hash 均正确。旧 xtask 会把全部 13 个 hash 替换为英文首页的一个 hash，这是错误的跨界改写。现 `_headers` 字节与原仓库一致，保留全部 13 个 hash。

另发现 `website/public/version.json` 的八个桌面资产大小落后于本地发布快照，已通过官网离线同步对齐。**这些数值只取自现有本地 `published-release.json`；本轮没有重新核验远端，也没有调用 `--refresh` 或任何发布入口。** 发布标签、版本、日期与下载 URL 仍为本地快照中的 V0.0.6。

## 门禁与实际验证

官网新增只读 `--check` 与 `--public-only`。检查先规划预期 HTML 与元数据，再核对各语言首页对应的 CSP；缺少 CSP 的 self 策略或首页 JSON-LD 会拒绝通过。检查模式不写文件，也不能与联网刷新参数组合。测试实际保留候选 `VERSION=0.0.7`，验证官网仍使用其自身 stable=0.0.6、陈旧输出被拒绝、四语言 hash 与既有策略保留。

CI 的 Frontend gates 增加官网发布单测与源码独立检查；网站构建后再检查 public/dist。应用版本工具的单测另验证历史网站内容与多 hash CSP 不进入候选改写计划。

| 实际执行 | 结果 |
| --- | --- |
| `cargo test --locked --offline --manifest-path rust/Cargo.toml -p xtask` | 17 通过 |
| `cargo run --locked --offline --quiet --manifest-path rust/Cargo.toml -p xtask -- sync-version 0.0.7 --check` | 应用平台版本全部同步 |
| `node --test website/tools/published-release.test.mjs tools/test/release-contract.test.mjs` | 32 通过 |
| `node website/tools/sync-version.mjs --check --public-only` | 本地 V0.0.6 页面、元数据、下载及 CSP 通过 |
| `node website/tools/validate-website.mjs` | 通过；四语言、53 个 canonical 页面、104 项历史演示清单 |
| `git diff --check`、修改脚本的语法检查 | 通过 |

Rust 使用已有唯一共享 target 与 cargo-home，`CARGO_INCREMENTAL=0`、`CARGO_PROFILE_DEV_DEBUG=0`、`CARGO_PROFILE_TEST_DEBUG=0`；临时目录限定于本工作区 `.cache/v007-rc-version-tmp`。本轮没有构建新应用二进制、生成 boot 或构建网站发行包。上述静态与工具检查不替代最终应用、安装包和平台原生验收。
