# 第三批本轮缓存分类与清理（2026-10-23）

已清理六份重复 EXE 压缩缓存，共 **151648711 字节（144.6 MiB）**。每份在删除前核对正式归档摘要，删除后再次确认归档未变。

| 缓存 | 大小 | 处理 |
| --- | --- | --- |
| candidate-36-readmd.exe.gz | 23.8 MiB | 摘要与正式归档一致，已删除重复缓存 |
| candidate-37-readmd.exe.gz | 24.2 MiB | 摘要与正式归档一致，已删除重复缓存 |
| candidate-38-readmd.exe.gz | 24.1 MiB | 摘要与正式归档一致，已删除重复缓存 |
| candidate-40-readmd.exe.gz | 24.2 MiB | 摘要与正式归档一致，已删除重复缓存 |
| candidate-41-readmd.exe.gz | 24.2 MiB | 摘要与正式归档一致，已删除重复缓存 |
| candidate-42-readmd.exe.gz | 24.2 MiB | 摘要与正式归档一致，已删除重复缓存 |

以下保留：

- `.cache/community-upgrade/build/`：共享离线编译目录及当前开发 EXE，继续复用。
- 锁定能力包、受管引擎和材料：后续离线验证需要，未清理。
- 当前任务首败、原始响应和源冻结：保留回归与复现依据。正式归档共有 21460 个成员、822.3 MiB；这是验收证据，不是软件安装资源。
- `docs/evidence/capability-upgrade/2026-10-23/batch3-author-inline-scoped-attempt-01-incomplete`：2351 个与正式归档逐项摘要一致的重复文件，共 98.0 MiB。自动审批拒绝批量删除，仅返回 blocked by policy；操作未执行，该目录仍保留，不能记为已清理。
- 私人文件、其他任务缓存及既有 toolbar 五文件未改动。

[机器可读清理与保留记录](evidence/batch3-author-inline-cache-cleanup-2026-10-23.json)。没有递归删除用户目录，没有提交、推送或发布。
