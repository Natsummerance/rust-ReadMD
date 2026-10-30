# ReadMD · macOS 安装

1. 从 [GitHub 正式发布页](https://github.com/Natsummerance/rust-ReadMD/releases/latest) 下载适合芯片的包：Apple Silicon 用 `arm64`，Intel 用 `x64`。同页提供 `SHA256SUMS.txt`。
2. 打开 DMG，把 `ReadMD.app` 拖入“应用程序”；也可以完整解压 ZIP 后移动 `ReadMD.app`。先退出旧版，再替换应用；个人文档无需移动。
3. 首次打开若提示无法验证开发者或 Apple 无法检查恶意软件，先关闭提示，再进入“系统设置 → 隐私与安全性”，找到 ReadMD 并点“仍要打开”，按系统提示确认。此许可只针对 ReadMD。
4. 要将 Markdown 默认交给 ReadMD 打开，在 Finder 中右键一份 `.md` 文档 →“显示简介”→“打开方式”选择 ReadMD，再点“全部更改”。应用声明了 Markdown 类型；默认应用由你选择。

本次使用免费的本地 ad-hoc 签名，封装完整性可检查；未使用 Developer ID，也没有 Apple 公证。默认 Gatekeeper 仍会阻止首次打开。请先核对下载来源与摘要，遵循 [Apple 的逐应用放行说明](https://support.apple.com/zh-cn/102445)。无需关闭全局安全保护。受组织策略管理的 Mac 可能需要管理员允许。

若仍出现“已损坏”，重新下载并核对同名摘要；不要先删除隔离属性。可在终端运行 `codesign --verify --deep --strict`，将 `ReadMD.app` 拖入终端补上路径，再回车。签名验证失败时请保留失败信息并重新下载，避免放行不完整的包。

## Install in English

Download the `arm64` package for Apple Silicon or `x64` for Intel from the release page above. Open the DMG and drag `ReadMD.app` to Applications, or extract the entire ZIP and move the app. Quit an older copy before replacing it.

To use ReadMD as the default Markdown editor, select a `.md` file in Finder → Get Info → Open with → ReadMD → Change All. The app declares Markdown support and leaves the default-handler choice to you.

These packages use free ad-hoc signatures and are not Apple-notarized. After the first blocked launch, open System Settings → Privacy & Security → Open Anyway for ReadMD, following [Apple's instructions](https://support.apple.com/en-us/102445). Verify the source and SHA-256 first. Organization-managed Macs may require administrator approval.
