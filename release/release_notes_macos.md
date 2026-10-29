# ReadMD V0.0.5 for macOS

本轮修复完整应用的内外层签名、资源封装和原生启动就绪检查；补充对应架构的离线 SVG 工作进程，并保持签名后资源清单与实际字节一致。

- Apple Silicon：ReadMD-macos-arm64.dmg / ReadMD-macos-arm64.zip
- Intel：ReadMD-macos-x64.dmg / ReadMD-macos-x64.zip
- 完整性校验：SHA256SUMS.txt

采用免费的本地 ad-hoc 签名，未作 Developer ID 签名或 Apple 公证。首次打开通常需要在系统设置中仅对 ReadMD 点“仍要打开”，详见 [安装说明](macos-install.md)。无需付费开发者账号。

Mac 原生读写、恢复和冲突检查的结果及候选摘要在本轮验证记录中分别登记。Intel 在 Apple Silicon 上的 Rosetta 运行不等于实体 Intel Mac 验收；声明的最低系统版本也不等于所有旧系统都已实测。

V0.0.5 标签、Windows/Linux 下载和开发中的 V0.0.6 迁移线分别保留。本轮 Mac 下载必须明确关联其修复源码提交，不移动旧标签来掩盖原始发布来源。
