# ReadMD MCP Server

ReadMD 的 MCP 服务内置在桌面端可执行程序中：`readmd --mcp` 在标准输入输出上提供 JSON-RPC。支持 MCP 2026-07-28 的逐请求协议发现，以及 2025-11-25 / 2025-06-18 / 2025-03-26 / 2024-11-05 的初始化握手。它调用与桌面端相同的 Rust 转换、导出、OCR、Skills 和 AI 代码，不需要 Python、窗口或监听端口。

`readmd-mcp-server-0.0.4.zip` 是文档与连接模板包；可执行内核随桌面应用提供，不在这个小型配置包中重复附带。

## 快速开始

1. 安装 ReadMD 桌面端（或把 `readmd` 放到 PATH 中）。终端运行 `readmd --version` 确认可用。
2. 在 MCP 客户端配置中填入可执行文件的绝对路径，参数为 `--mcp`，然后重启客户端。

Windows：

```json
{
  "mcpServers": {
    "readmd": {
      "command": "C:\\Program Files\\ReadMD\\ReadMD.exe",
      "args": ["--mcp"]
    }
  }
}
```

macOS / Linux：

```json
{
  "mcpServers": {
    "readmd": {
      "command": "/usr/bin/readmd",
      "args": ["--mcp"]
    }
  }
}
```

VS Code 扩展的「ReadMD: 一键配置工作区 MCP Server」会自动探测 ReadMD 并写入同样的配置。其他客户端的模板见 `mcp_config_templates.json`。可以额外传 `--data-dir <dir>` 让 MCP 使用独立的数据目录。

## 工具

以客户端返回的 `tools/list` 为准，共 22 项：

`readmd_fix_markdown`、`readmd_convert_to_markdown`、`readmd_web_to_markdown`、`readmd_ocr_to_markdown`、`readmd_export_document`、`readmd_latex_to_md`、`readmd_md_to_latex`、`readmd_parse_bibtex`、`readmd_latex_to_omml`、`readmd_ai_assistant`、`readmd_ai_providers`、`readmd_ai_chat`、`readmd_process_imports`、`readmd_generate_toc`、`readmd_export_presentation`、`readmd_export_epub`、`readmd_run_code_chunk`、`readmd_pdf_audit`、`readmd_pdf_rollback`。

新增：`readmd_ai_models` 从已保存的连接获取模型；`readmd_export_presets` 返回真实内置/自定义导出预设；`readmd_render_diagram` 离线渲染 PlantUML 或基础 WSD/D2/Ditaa，禁止自动上传图表源码。导出预设不存在时返回错误，不再静默回退。

对象结果同时提供兼容文本和 `structuredContent`，工具带有只读/副作用等注解。参数按公开 schema 校验，错误类型、未知字段、非法目录深度不会被静默转换。

`resources/list` 公开 Skills（`readmd://skills/<id>`）、Provider 目录（`readmd://providers`，不含密钥）和本地会话记录（`readmd://sessions`）；`prompts/list` 与当前 Skill Registry 一一对应。旧的 workflow id（如 `polish`、`summary`）仍可在 `prompts/get` 和 `readmd_ai_assistant` 中使用。

## 安全边界

下列工具有副作用，参数必须包含 `"confirm": true`，否则返回 `confirmation_required`：

- `readmd_web_to_markdown`（联网）
- `readmd_export_document`、`readmd_export_presentation`、`readmd_export_epub`（写文件）
- `readmd_run_code_chunk`（执行代码）
- `readmd_pdf_rollback`（改写 PDF）
- `readmd_ai_models`（连接上游）、`readmd_render_diagram`（可能调用本地渲染程序）

输出路径必须是绝对路径，扩展名须与格式一致，父目录必须存在，路径上不允许符号链接；目标已存在时只有 `"overwrite": true` 才会替换，否则返回 `output_exists`。`readmd_ai_chat` 只接受 `credential_id`，传入原始 API Key 会被拒绝。MCP 不暴露更新、托盘、开机启动、通知和窗口控制。

错误以 `{"ok": false, "error_code": "..."}` 返回（`isError: true`），协议错误使用标准 JSON-RPC 代码。同时运行的工具超过 8 个时返回 `-32001 server_busy`；客户端可以退避重试。重复的在途请求 ID 被拒绝。单行消息上限 32 MiB，超限后丢弃整行并继续处理后续请求。

提供 `progressToken` 的耗时工具每 10 秒发送活动通知，这些不是 AI 文本增量。取消后不返回迟到结果，尚未执行的操作会跳过，单文件导出在提交前检查取消；已在运行的同步网络/渲染任务可能继续到结束。已提交的写入不能撤销，TeX 带图片目录的导出以开始写入为提交点。

PDF、Word、HTML 先在独立临时目录渲染，完成后原子发布；EPUB 和演讲也原子写入。禁止覆盖时在最终发布仍检查目标冲突。TeX 主文件采用原子写入，关联的 `.assets` 目录用于图片及参考文献。

文档、EPUB 和演讲导出均可传 `base_dir` 指定源文件目录；相对图片按该目录解析。演讲将可读取的本地图片嵌入 HTML，无法嵌入的图片返回 `warnings`，不静默声称全部成功。

演讲单文件移除在线/旁置字体样式请求，使用系统字体回退；KaTeX 公式与存在的 Mermaid 围栏均使用内嵌渲染资源，断网可打开。其他工程图表的离线增强预览不等于演讲导出已嵌入所有引擎。

## 当前协议与兼容

2026-07-28 客户端可直接调用 `server/discover`，逐请求在 `params._meta` 传入 `io.modelcontextprotocol/protocolVersion: "2026-07-28"` 和 `io.modelcontextprotocol/clientCapabilities: {}`。响应含 `resultType: "complete"`、服务器信息；用户目录/资源不共享缓存。版本不支持时返回 `-32022` 和支持的版本。

旧客户端继续调用 `initialize`、发送 `notifications/initialized`，无需改变现有配置。`resources/templates/list` 返回空列表；没有宣称实现可选 Tasks、Apps 或订阅能力。

维护者可用 `test/stdio.mjs` 对真实可执行程序测试发现、模型、AI、导出、并发、取消和输入上限。须设置 `READMD_BIN`、`READMD_ASSETS_DIR` 和独立的 `READMD_MCP_TEST_ROOT`；资料和 AI 服务均为本地合成测试材料。

## 故障排查

- 客户端显示进程立即退出：在终端直接运行 `readmd --mcp`，输入一行 `{"jsonrpc":"2.0","id":1,"method":"ping"}` 应返回 `{"jsonrpc":"2.0","id":1,"result":{}}`。
- `--mcp 不能与 … 同时使用`：`--mcp` 不能与 `--browser`、`--selftest`、`--share` 等界面或自检参数混用。
- AI Provider 为空：先在同一用户账户的 ReadMD 桌面端保存 Provider 和凭据。
- OCR 返回 `ocr_no_engine`：当前平台没有系统 OCR 引擎（Windows 10+ 使用系统自带的 Windows.Media.Ocr）。

标准输出只承载协议消息，诊断信息写入标准错误。

旧的 Python 实现 `readmd_mcp_server.py` 已删除，由 `readmd --mcp` 取代。
