# V0.0.6 第三批：OpenAPI 本地跨文件引用

日期：2026-10-27。范围：V6-31 的一个可独立保存与重开的程序员流程；不表示 V6-31 整项结案。

## 用户流程与公共服务

在现有“从 OpenAPI 生成文档”入口选择根 JSON/YAML。根规范可引用其目录内的 JSON/YAML 文件，依赖还可相对自己的文件引用其他依赖。桌面、MCP `readmd_openapi_document` 和 VS Code Extension Host 继续调用 `/api/developer/openapi`。服务仅生成一次可撤销的正文事务；接受和显式保存沿用既有入口。

审阅回执 `source_revision` 对根规范以及实际读取的依赖字节共同取摘要。接受前的既有 `expected_source_revision` 复查必须拒绝“根文件未变、依赖已变／消失／由缺失变为存在”的旧预览。无依赖的单文件回执与生成区格式保持兼容。

## 解析与保护边界

- 根规范沿用 OpenAPI 3.0／3.1 验证和有预算的 YAML 解析；依赖允许是独立对象、数组或 3.1 布尔 schema，不要求包含 `openapi`、`info`、`paths`。
- 仅解析 OpenAPI 定义中的引用位置及受支持 JSON Schema 子结构；示例值、默认值、扩展字段内的字面 `$ref` 不读文件。
- URI 只支持相对文件名与 JSON Pointer 片段。UTF-8 百分号编码与 `~0`／`~1` 按标准处理；每个引用以其所属文件为基准。HTTP(S)、file URI、绝对／驱动器／UNC 路径、查询及非法编码不读取。
- 依赖 canonical 路径必须仍在所选根规范的 canonical 父目录内；符号链接或 junction 指到目录外时拒绝。允许子目录内通过 `../` 回到同一根目录。
- 目录检查不作为随后普通路径打开的授权。根规范和依赖共用保留的 `SourceReader`：Windows 逐级以 `OPEN_REPARSE_POINT` 打开并检查目录／文件句柄，目录句柄拒绝写入和删除共享；Unix 从原根目录 FD 逐级 `openat(O_NOFOLLOW)`。读取验证过的文件句柄，并在读后复核长度与 mtime；竞争变更返回冲突，不交付部分事务。原生测试安排“解析后换父目录”“旧 canonical 路径已被链接替换”“打开后再换父目录”三个确定时序，不将未能创建 fixture 静默算通过。
- 每文件最多 8 MiB、整组输入最多 16 MiB、最多 64 个依赖文件、引用链最多 32 层、投影树最多 200,000 节点、展开及最终输出最多 16 MiB，复用取消上下文。预算超限终止，无部分事务。
- 已支持的 3.0 Reference Object 忽略额外字段；3.1 Reference Object 保留 `summary`／`description` 覆盖。3.1 Schema 通过 `allOf` 承载被引用目标，兄弟约束留在原层级，使 `unevaluatedProperties`／`unevaluatedItems` 仍能看到引用目标的评估注解；不能把兄弟约束整体搬到另一个 `allOf` 分支或用覆盖对象伪造联合语义。
- `$id`、命名 anchor、`$dynamicRef` 等改变 schema URI 语义的结构仍保留原始引用并诊断；不宣称完整 JSON Schema 验证或 URI 资源注册。循环与缺失保留引用和诊断；从依赖搬入的未解析引用必须重新限定到根目录，不能误变成根规范的本地指针。
- 回执的依赖清单和来源映射只返回规范相对名称、摘要、调用点与定义的 JSON Pointer；生成正文投影选中的定义，不回传整份依赖源文件、私有 profile、连接信息或绝对依赖路径。

## 测试与验收

20 个专属 Rust 测试覆盖 JSON/YAML 混合、跨文件相对基准、指针转义、3.0／3.1 语义、字面示例、循环、缺失、大小／节点／深度预算、取消、越界与符号链接、三个文件打开竞争时序、依赖修订冲突、作者段落、局部手改保护和 UTF-16 来源映射。专属 Node 脚本用显式指定的预编译二进制启动隔离服务，通过真实 HTTP 和 MCP stdio 断言相同内容、依赖冲突、事务撤回以及现有 Rust 保存入口写入后的实际文件重开；没有自动重试或假成功。

执行命令（由集成负责人使用统一构建目录执行 Rust；本工作者只做静态检查）：

```powershell
cargo test --manifest-path rust/Cargo.toml -p readmd-kernel openapi_document:: --offline
node --check tools/test/v006-openapi-local-refs.cjs
node tools/test/v006-openapi-local-refs.cjs --plan
# READMD_BIN 指向本次已构建并记录摘要的候选，不能把旧二进制结果当本次实现通过。
node tools/test/v006-openapi-local-refs.cjs
```

2026-10-27 集成结果：安全句柄改造前的 OpenAPI 定向 22／22 通过；句柄改造后新增三个竞争测试，已进入完整 Rust 回归并通过，全集为 4093 通过／0 失败／3 既有忽略。`rustfmt`、Node 语法／计划及定向差异空白检查已通过。原生 GUI 和 Extension Host 的本次跨文件材料仍待单独验收。

旧候选对照首败已保留：[run-iFq0hB/report.json](../../.cache/capability-upgrade/v006-batch3-openapi/run-iFq0hB/report.json)。旧二进制 SHA-256 为 `f784e5492f17bbb720926f1570445649a0c48e4d4063446d109a0ce68f1c7555`，真实服务检查 0／10，首败在 `mixed local references and maps`：生成正文未包含外部 schema 的 `minLength`。该失败证明旧行为，不能记为新实现验收；新候选结果须另存。HTTP 隔离 host 和 MCP 均已结束，没有运行 GUI。

新候选一次通过真实服务 10／10：[run-ATjlmr/report.json](../../.cache/capability-upgrade/v006-batch3-openapi/run-ATjlmr/report.json)。二进制 SHA-256 为 `8f290f47d4dc4ddd4a31bdb9496a82693d1a58e1835189090491358d000ecb00`，模块 SHA-256 为 `efe388d11f413da60f94a57b069fe94126735c92fd9f758447c8478539338904`。HTTP 与真实 MCP stdio 投影相等，逆事务恢复原文；既有 Rust 保存入口写入后的实际磁盘文件重新读取且幂等。依赖独改的旧预览在两种传输均被 409 拒绝；更新、手改保护、缺失依赖出现、原生 junction 越界、循环、超限字节与 3.0 单文件兼容均实际通过。MCP 正常 EOF 结束；HTTP host 按测试设计终止，此项不作正常退出生命周期认证。未启动 GUI 或调用模型，不能据此替代桌面、Extension Host 或长期压力验收。

真实内核与正式 CSP 下的 headless 浏览器另验证了跨文件内容进入审阅、接受后一次撤销／重做、显式保存并重开，以及依赖变更后拒绝旧结果。首轮暴露界面显示未翻译的内部错误键；修复后以英文、简体中文及阿拉伯语复测，并检查使用的提示在 46 种语言中有本地文案。该结果不等同于原生窗口或 Extension Host 验收。

原生界面验收：包含子目录依赖的示例仓库 → 桌面真实审阅／取消 → 接受／一次撤销重做 → 显式保存／重开 → 修改依赖再次更新 → 审阅期间再改依赖后接受被拒绝；再以 VS Code 真实 diff／接受复核同一链。已有 `ui-tests/openapi-native.cjs` 与 Extension Host 脚本保留，不把 HTTP、stdio 或模拟编辑器结果记作原生 GUI 通过。

本工作者不运行 Cargo、编译、构建缓存操作或 computer use；编译及原生 GUI 验收由集成负责人串行执行，首败必须保留。正式版本仍为 V0.0.5，不提交、不发布，不改现有宣传素材。

依据：[OpenAPI 3.0.4](https://spec.openapis.org/oas/v3.0.4.html)、[OpenAPI 3.1.1](https://spec.openapis.org/oas/v3.1.1.html)。
