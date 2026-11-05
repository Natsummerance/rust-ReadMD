# V0.0.6 会话提交热修复：发布与本机安装

2026-11-05，[V0.0.6 正式下载](https://github.com/Natsummerance/rust-ReadMD/releases/tag/V0.0.6)的十个安装／集成包及 SHA256SUMS.txt 已更新，11项匿名公开下载的实际大小与SHA256全部核对。运行源码 `e7b1bb97fc4ab5f5b43e6f0800512b10c0988fd9`；原标签 `544defc32219e4de6b85e9d07d9edc230d6639f4` 保留，不发布 V0.0.7。

本次修复 Windows 共享 DELETE 读句柄仍可能导致 MoveFileEx 返回 ACCESS_DENIED 的边界：仅对此精确错误采用 MSRV 支持的 POSIX 安全 rename，并同步原 staged 文件句柄。deny-delete 与不支持的路径仍失败并保留原件。既有单赢家会话比较更新、关闭确认、保存失败、原文编码与恢复保护继续保留。

[精确候选CI](https://github.com/Natsummerance/rust-ReadMD/actions/runs/41905970855)的十个构建／质量任务通过，Publish任务按build-only跳过；13份必需产物的GitHub digest、大小、ZIP CRC及源码绑定已核对。三平台CAS真实双进程竞争分别为Windows128轮及4类Windows控制、Mac/Linux各16轮；三平台各10项关闭／保存回归、Windows/Linux正式发行二进制各30项生命周期HTTP／headless检查通过。Mac/Linux CAS属于CLI进程证据，不冒充桌面交互。

本次Mac专用QA宿主68项Cocoa/WKWebView检查通过。四个正式arm64／x64 ZIP／DMG另完成8次正常系统启动／重开，8张原始系统窗口截图独立检查中文正文、本地SVG及无空白画面。QA宿主与CI正式包为不同二进制，分别绑定验证；Windows安装程序与ZIP、Linux DEB与tar中的配对二进制和受影响资源已核对，集成包另核对构建内容及版本。

本机已安装正式arm64候选，又完成2次正常启动／重开及2张独立截图检查；默认配置／数据库哨兵摘要保持一致，默认Markdown程序仍为安装的ReadMD。旧App保留作回退，未改写用户数据，也未加载模型。发布事务备份的实际清理及本次上传副本／已知工具缓存清理均有真实回执；十个最终安装包与原始验收证据继续保留，逻辑字节数不当作实际释放磁盘空间。

Mac保持免费ad-hoc签名，没有Developer ID／Apple公证；安装见[说明](../../release/macos-install.md)。Intel在Apple Silicon上经Rosetta运行，实体Intel仍待验；Mac脚本UI不等于物理键鼠、系统IME或原生文件选择器验收，Windows/Linux的CLI及headless结果也不等于原生窗口验收。原账仍为**0整项验收**，本限定热修复不提升75条完整合同的整项状态；P1、P2与V0.0.7未发布。摘要及结构化回执见[同名JSON](readmd-v006-session-refresh-published-2026-11-05.json)。
