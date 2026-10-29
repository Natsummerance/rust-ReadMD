# V0.0.5 Mac 修复已发布 · 2026-10-29

[正式发布页](https://github.com/Natsummerance/rust-ReadMD/releases/tag/V0.0.5) 已替换四个 Mac 包及合并后的 SHA256SUMS。Mac 包源码为 bf731860347f24b9946c71ab66d3588303345ec7，版本号仍为 0.0.5；原标签仍指向 3cbb7a39ad889a2a485105acb4950a6cee995649。没有发布 V0.0.6，也没有移动旧标签。

全部五个文件已从匿名公开下载链接逐字节计算 SHA-256，与本机已验证候选和 GitHub 资产摘要一致。Windows、Linux、VS Code、MCP 的原资产 ID 和摘要逐项保持。替换前保存了五个原始 Mac/校验文件的恢复副本；上传、认证、正式名称切换、说明核对和旧资产清理均有本地持久回执。

Apple Silicon 原生与 Intel/Rosetta 各 36 项 Cocoa WKWebView 核心读写检查，以及四个优化 ZIP/DMG 的安装副本、签名、冷启动和受管 SVG 输出已经通过。采用免费 ad-hoc 签名；未使用 Developer ID 或 Apple 公证。首次打开仍需要按 [安装说明](../../release/macos-install.md)逐应用允许。实体 Intel、旧系统、人工首次授权、真实键盘/IME、人工视觉和其他平台集成仍分别待验。

已从 GitHub 删除 4 个没有独有提交、没有开放 PR、未受保护的旧分支：codex/v005-upgrade、codex/website-stable-downloads-20261008 及其 r2/r3。删除前复核祖先关系并建立可验证恢复 bundle；实际 pre-push 钩子检查服务器广告中的旧提交，服务器以原子组提交删除，之后复查保留分支。没有使用强制推送或跳过现有钩子。

V0.0.5 修复线和 V0.0.6 迁移线已分别推送。[PR #2](https://github.com/Natsummerance/rust-ReadMD/pull/2)保留从正式 V0.0.5 起步的有限修复。第一轮 Linux CI 揭示原测试把创建顺序误当遍历顺序；生产逻辑与原版本一致，测试现从独立目录枚举建立期望，Mac 相关 9 项已通过。首次失败日志保留；新提交的远端 CI 仍待完成，此记录不冒称全部 CI 已通过或 PR 已合并。

官网主页与下载页的四语言源码已跟随真实 V0.0.5 发布元数据，增加 Mac 首次打开说明；两条源码线的完整网站构建和校验通过。此时尚未验证线上部署。源数据、原生候选和发布后的公网结果见同名 JSON，完整能力合同验收状态未上调。
