# V0.0.6 关闭保护更新：发布与本机安装

2026-11-04，[V0.0.6 正式下载](https://github.com/Natsummerance/rust-ReadMD/releases/tag/V0.0.6)的十个包已替换，总摘要同步更新；十包与 SHA256SUMS.txt 共 11 次匿名公开下载的内容摘要和大小复核通过。运行源码 `5d617aeffdb792f6fd7ef7616b7f099b4f90c3b4`，原标签 `544defc32219e4de6b85e9d07d9edc230d6639f4` 保留，不发布 V0.0.7。

修复 SQL 结果在关闭确认期间改变正文、导致第一次“不保存”失效的问题；关闭标签与退出编辑的保存、取消、不保存分别验证。结果固定页面／输入代次／修订／源位置，相同内容切页重置旧控件；外部修订冲突保留草稿和原件，保存失败不报告成功。原有关闭修订校验保持。V0.0.6 使用原有格式 6，未带入 P1 的格式 7 或批量执行。

[精确候选 CI](https://github.com/Natsummerance/rust-ReadMD/actions/runs/41738957189)的 10 个构建／质量任务通过，三平台各 10 项真实 Rust 关闭／保存用例、Windows／Linux 正式发行二进制各 30 项生命周期回归通过，产物摘要已核对。Mac 专用 debug QA 宿主 68 项真实 Cocoa/WKWebView 检查通过；四个正式 ZIP／DMG 另完成 8 次冷启动／重开，正常退出、会话恢复、中文路径、图片和原件 BOM／CRLF 均核对。桌面包的受影响源码资源、架构和配对二进制已检查，Windows 安装程序与便携包、Linux DEB 与 tar 包中的可执行文件及受影响资源一致，VS Code／MCP 集成包另核对版本及构建内容；Windows 精确 CRLF 检出变体独立记录。

本机 App 已更新为已验候选，又完成 2 次正式安装副本启动／重开；默认配置／数据库哨兵摘要保持一致，默认 Markdown 程序仍为安装的 ReadMD。没有加载模型。前一候选在 Ubuntu 26.04 上被锁定 Playwright 拒绝，保留首次失败，改用 Ubuntu 24.04 后重新构建验收，没有跳过检查。

Mac 为免费 ad-hoc 签名，没有 Developer ID／公证；首次安装见[安装说明](../../release/macos-install.md)。Intel 通过 Rosetta 运行，实体 Intel 仍待验；Windows／Linux 的浏览器记录不冒充原生窗口、选择器或 IME。P1、P2 和 75 条完整合同继续保留原有边界，全部 P1/P2 完成后才发布 V0.0.7。结构化回执见同名 JSON。
