# V7-11：JSON 聊天取消与提交回执

JSON 聊天现在与 SSE 共用任务登记、可取消的请求连接、对端断开检查和提交边界。修复前，实际鉴权 HTTP 请求已经到达本轮 loopback 提供商夹具，停止路由仍返回 `unknown`；首败完整保留。修复后，JSON 聚合的响应头前停滞与 SSE 中途停滞均返回 `409 cancelled`，关闭拥有的上游连接，且不写入局部聊天历史。

本轮基于 `ecf7f9e2` 的未提交工作副本，仅认证 macOS ARM64 的限定链路。使用共享 target、`--locked --offline` 和 `CARGO_INCREMENTAL=0`；没有调用模型、读取用户聊天、提交或发布。[结构化记录](readmd-v007-p1-chat-json-cancel-2026-11-19.json)

## 实际检查

27 项 Rust 定向测试通过：JSON／聊天 9 项、SSE 11 项、拥有连接的传输 7 项。传输测试分别保留对端 EOF、worker 和请求槽回收的断言，覆盖队列满且消费者不轮询的取消。

正式鉴权 HTTP 回归使用两个实际内核进程和 22 次合成协议请求，验证 CSP、尾斜杠鉴权、原 JSON 信封、分段 UTF-8、错误／截断保护、两类取消、下游断开、隐私聊天与进程重开。原 Markdown 的 BOM／CRLF 字节保持一致。

| 停止阶段 | 取消 HTTP 回执 | 拥有的上游关闭 |
| --- | ---: | ---: |
| `json_cancel_headers` | 1.10ms | 75.41ms |
| `json_cancel_stream` | 1.03ms | 75.06ms |
| `cancel_headers` | 0.72ms | 29.55ms |
| `cancel_stream` | 0.78ms | 38.45ms |

JSON 取消仍在运行时返回 `cancelling`；原请求已经返回取消结果并注销后为 `unknown`。`unknown` 只表示没有活动登记，不能独立作为成功、取消完成或资源回收证据。测试分别核对了原请求结果、历史、对端关闭与后续可用请求。

JSON 和 SSE 使用同一提交边界：取消先取得边界时，不写入成功历史；提交边界先取得时，后续取消返回 `finished`，继续完成整轮原子写入。新增测试验证 SSE 在提交后、最终用量发送时丢失对端，完整两条历史仍保留。数据库失败仍报告原失败并回滚整轮，`finished` 不是数据库成功收据。

## Windows 原失败的边界

2026-11-05 的 `10060` 属于 `review_transport::network` 的真实停滞 AI 转发测试：取消后，对端读取未及时见到 EOF；之后的 worker 归零断言尚未执行。它不是代码运行器的子进程退出失败。原运行 `41955138374` 与单项诊断运行 `41965242001` 分列；后者通过没有解释原首败。

当前代码已经含 `889afc23` 的连接克隆排空和队列 worker 退出保护，本轮没有重复修改。此次修复关闭了新复现的 JSON 取消缺口，不能据此声称 Windows 原 `10060` 根因已解决。

同候选的 Windows／Linux、原生桌面 UI、实际提供商推理和系统 DNS 故障仍待验。MCP／Extension Host 的运行中取消未在本切片实测；继承的外部取消由 Rust 定向测试覆盖。代码运行器、受管运行环境及检查点继续按 V7-05 独立验收。P1／P2 与 V0.0.7 发布门槛保持开放。

本地原始记录：`.cache/v007-json-chat-cancel-first-20261119/report.json`、`.cache/v007-json-chat-cancel-20261119/http/report.json`、`.cache/v007-json-chat-cancel-20261119/rust-final.log`；其摘要以及源码、二进制和 boot 摘要均在结构化记录内。
