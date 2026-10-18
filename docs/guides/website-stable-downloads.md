# 官网正式版下载同步

两个官网的下载入口统一使用 `Natsummerance/rust-ReadMD` 的正式发布，不再提供旧架构安装包。2026-10-08 实测旧域名下载入口重定向至 `rust.readmd.asia`；两个仓库的 Pages 部署均成功。源码中的 `VERSION` 可以是开发中的 V0.0.5，不能用来推断已经发布的下载版本。

## 发布与构建

1. 软件通过验收后，完成正式 GitHub Release 的全部安装包、VS Code、MCP 和校验清单上传。
2. 发布检查单独运行 `node website/tools/published-release.mjs --refresh`，读取 GitHub 最新正式发布。草稿、预发布、缺失包、未完成上传、空包、非官方 URL 均不接受；标签大小写原样保留。
3. `website/published-release.json` 保存经过验证的发布快照，`website/release.json` 保存正式版与候选版的区别。刷新发布清单是联网操作；`sync-version.mjs` 和网页构建只使用本地快照，不在构建中下载资源。
4. 构建同步可见版本、元数据、实际下载 URL、发布日期和 CSP 中对应的结构化数据摘要。历史发布说明及候选能力介绍保留自己的版本语义。
5. GitHub Pages 工作流通过真实网站校验后再部署。正式发布事件派发到 main 分支构建，避免 tag 环境无法部署 Pages。两个站点另有每小时的同步检查，旧站也能跟进 Rust 仓库的新版。

浏览器先读取同源 `version.json`，再查询正式发布。最新发布必须拥有完整包集合才会替换页面。GitHub 被阻断、限流或网络失败时，继续使用已经部署的快照和下载链接。不会把候选版显示成正式版，也不会将新版本降回较旧的 API 缓存结果。

镜像切换作用于经过验证的官方 URL，包括校验清单；不会重新生成小写标签或切回旧仓库。不存在的旧包不继续作为下载提供。未提供的架构保留源码构建入口，不能冒充可下载安装包。

## 验证

```powershell
node --test website/tools/published-release.test.mjs
node website/tools/validate-website.mjs --release
node website/tools/release-smoke.cjs
```

浏览器检查使用构建好的 `website/dist`，需已存在的 Playwright。覆盖四种语言、正式版升级、网络回退、不完整发布拒绝、镜像与校验链接、390–1440px 布局和正式 CSP。测试中的 V0.0.5 响应是受控的升级场景，不能作为 V0.0.5 已经正式发布的证据。

2026-10-08 的上线记录放在 `docs/evidence/capability-upgrade/2026-10-08/website-stable-downloads-production.json`；线上实测包括两个域名的八个下载页和十一项真实发布资产。该目录为本机证据，不随软件发布。

本次只向两个仓库提交网站修复，没有提交软件候选改动、视频制作或社交资产。V0.0.5 后续合入时必须保留官网修复提交，正常合并最新 main，禁止覆盖发布分支或强制推送。
