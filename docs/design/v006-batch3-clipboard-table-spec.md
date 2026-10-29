# V0.0.6 第三批：剪贴板表格转换

日期：2026-10-27。范围：V6-32 表格／办公中的一个有限前端切片。它改进已有 Markdown 编辑器的表格粘贴，不代表 V6-32 整项、全部办公场景或电子表格工作簿已经完成。

## 已实现的用户流程

在 CodeMirror 编辑器中粘贴多行、多列 TSV，或带显式 `text/csv` 类型的 CSV，生成一张 Markdown 表格，替换当前单个选区。第一条逻辑记录作为表头，后续记录作为表体；整个转换只派发一次 `input.paste` 文档事务。工具栏撤销／重做、既有审阅历史、显式保存及文件重开沿用现有入口。

真实入口是 `cmHandlePaste → handleSmartExcelPaste`，解析及单元格序列化位于 `assets/js/editor/editor.js` 的 `clipboardTableRows`／`clipboardTableCell`。源码不再先按换行和 Tab 拆分，也不再对整段及各单元格执行 `trim()`。

编辑器降级为 textarea 时，同文件中的委托 `paste` 监听器接通相同转换。插入使用 `execCommand('insertText')`，以保留宿主的原生历史；插入返回失败或抛错时，不消费事件，仍可由宿主执行普通粘贴。该降级路径目前有独立 Node 控制器验证，尚未据此宣称所有真实 WebView 的 textarea 撤销行为通过。

## 输入与序列化合同

- 普通 `text/plain` 只自动识别 TSV；也接受显式 `text/tab-separated-values`。CSV 必须由剪贴板明确提供 `text/csv`，避免把普通含逗号的多行文字误判成表格。纯文本 CSV 保留普通粘贴行为。
- 支持单元格起始双引号、引号内的分隔符／换行，以及以 `""` 表示的字面双引号。闭合引号之后出现额外正文、引号未闭合，或 CSV 的未引号字段中出现双引号时，不自动转换。未引号 TSV 字段内部的双引号作为字面文字保留。
- 支持输入起始 BOM，CRLF／CR／LF 记录分隔符和引号内换行；解析时统一为 LF。最终文档行尾仍由已有 `getEditContent` 按原文件保留 CRLF 等格式。单元格中的换行序列化为 `<br>`。
- 至少有两条逻辑记录，且表头至少两列。TSV 按所有记录的最大列数补齐短行，不丢弃空单元格；CSV 要求各逻辑记录与表头等宽，否则保留普通粘贴。
- 保留空表头、首尾空单元格、ASCII 边缘空格，以及真实的尾部空记录。一个末尾记录分隔符仅结束当前记录，不额外制造虚空记录；额外的末尾空记录仍形成空表体行。
- 单元格按办公纯文本处理：转义 `&`、尖括号、反斜杠、管道、Tab、常见 Markdown 标记及公式／自动链接触发字符。字面 HTML、Markdown、实体、公式文本不会因转换而成为对应格式。管道和反斜杠使用实体序列化，避免连续反斜杠使 GFM 管道再次成为列分隔符。
- 继续使用现有表格 Tab／Enter 编辑入口；专属回归覆盖转换后的管道、反斜杠、标记、实体、Unicode 和单元格换行经过格式对齐及加行后的内容保留，不代表既有任意手写 Markdown 表格的全部边界均获重新认证。
- CodeMirror 的多选区、代码／HTML 代码目标和只读协作状态不执行自动表格转换。损坏、歧义或超预算输入均不拦截普通粘贴。
- 图片检测、暂存保存及异步文档身份检查仍优先于表格转换。剪贴板同时含图片与表格文本时，处理图片；异步解码完成时已切换文档，不向另一份文档插入图片。

## 自动转换预算

预算是前端自动转换的上限，不是原生普通粘贴的输入限制。超限返回不转换，不产生部分表格、不发成功提示，也不调用 `preventDefault()`。

| 项目 | 包含上限 | 检查位置 |
| --- | --- | --- |
| 输入 UTF-8 字节数 | 1 MiB，即 1,048,576 字节 | 正规化和构造记录前；逐码点计数，不分配完整编码副本 |
| 逻辑记录 | 1,000 行，含表头 | 解析过程中 |
| 每条记录 | 100 列 | 解析过程中 |
| 最终矩形单元格 | 20,000 格，含补齐的空格 | 序列化和编辑事务前 |

因此，UTF-16 长度小于 1 MiB、UTF-8 字节数超过上限的 Unicode 数据也会拒绝自动转换；100 列表头加 200 条短行产生 20,100 个补齐单元格，同样会拒绝。恰好达到行、列、矩形格数和输入字节上限的正例有回归覆盖。

## 先失败再实现

所有证据位于独立的 `.cache/v006-table-clipboard/`，仅使用合成数据和新建配置目录。本工作者及以下脚本没有读取用户连接配置、输出应用令牌、启动可见窗口、编译、提交或发布；统一候选由集成负责人构建。

1. `first-failure.tap`：对实现前的 editor.js 执行专属行为测试，9 项中 8 项失败，实际暴露引号内 Tab／换行拆错、空头与尾部空行丢失、CSV 缺失、文字被解释为格式、代码区被误转换和 textarea 未接通。`first-failure-source.json` 保留当时源码 SHA-256。另存的 `harness-first-failure.tap` 是最初测试宿主缺少 DOM 能力的错误，不能算产品行为首败。
2. `limits-first-failure.tap`：增加预算前，独立的 ASCII 字节、Unicode 字节、行数、列数和补齐放大 5 个负例全部失败。加入预算后，完整专属测试为 16／16 通过，记录于 `passed.tap`。
3. `browser-before-boot-final/report.json`：使用指定的既有内核及旧前端 boot，在正式 CSP 和 Edge 143 headless 中，通过 Clipboard API 写入合成 TSV 并按 Control+V。记录 `nativeTsvPaste.trusted=true`、`loadedClipboardParser=false`，逐单元格断言重现引号、空头和跨行数据损坏。它是浏览器真实粘贴入口的产品首败。

早期 `http-1`／`http-2` 使用仓库根目录旧可执行文件或不正确的读取断言，不能作为本次候选的验收证据；浏览器启动／CSP 读取／图片夹具调试产生的其他失败也不替代上面的行为首败。

## 可执行验证与实际证据

在仓库根目录执行；验收脚本要求证据目录不存在，不覆盖旧证据。它们仅使用显式指定的预编译二进制，没有 Cargo 或浏览器安装回退。

```powershell
node --test tools/test/v006-table-clipboard.test.cjs

node tools/test/v006-table-clipboard-http.cjs `
  --bin .cache/community-upgrade/build/debug/readmd.exe `
  --evidence .cache/v006-table-clipboard/http-final

node tools/test/v006-table-clipboard-browser.cjs `
  --bin .cache/community-upgrade/build/debug/readmd.exe `
  --evidence .cache/v006-table-clipboard/browser-final
```

| 层次 | 实际调用路径及通过范围 | 本次已保留的记录 |
| --- | --- | --- |
| 独立 Node | 实际仓库 CodeMirror 状态、语法树和历史，以及 marked 渲染；经 `cmHandlePaste` 调用产品函数。验证 16 项，包括边界、单事务、保存控制器、预算及图片异步身份保护；几何宿主和 API 是受控替身 | `passed.tap` |
| 无窗口 HTTP | 在 Node VM 调用实际 paste／saveEdit 函数，再由真实 `/api/save` 写入磁盘；比较 CRLF 和 SHA-256，验证保存前后一次撤销／重做、旧修订冲突。停止内核，用新 PID 和新配置重开 TSV／CSV，检查原始源码及单元格 | 最终统一候选 `http-final/report.json`，2／2；此前既有内核结果另存于 `http-current-1` |
| 正式 CSP 浏览器 | 实际加载重打包 boot；TSV 经 Clipboard API 和 Control+V 产生可信原生事件，只观察到 1 个 `input.paste` 事务；点击工具栏 Undo／Redo／保存，以真实 `loadFile` 重开并读取实际 reader 表格，同时等待真实 `/api/file` 200 回包并逐字比对原始源码。还验证普通逗号文本、代码区 TSV，以及 PNG＋表格文本时图片优先、真实保存和重开可解码 | 最新统一 boot 绑定记录 `browser-pdf-fix-boot/report.json`，5／5，CSP violation 和 page error 均为 0；先前统一候选 `browser-final`、`browser-final-boot`、`browser-final-boot-2` 及既有内核 `browser-after-boot-3` 的结果保留。`native-tsv-reopened.png` 已目视检查 |

浏览器 CSV 正例使用带 `text/csv` 的 `DataTransfer` 和 DOM `ClipboardEvent`，明确记录 `nativePasteTrusted=false`；它验证真实编辑器 DOM 处理入口，但不能声称浏览器已支持原生 CSV 剪贴板格式。最终 Edge 实测 `ClipboardItem.supports('text/csv')` 为 `false`。浏览器 TSV、普通文字、代码区和 PNG 的粘贴事件则实际记录 `isTrusted=true`。

最终复测分别重新启动统一新候选，HTTP 与浏览器报告均验证二进制 SHA-256 为 `8f290f47d4dc4ddd4a31bdb9496a82693d1a58e1835189090491358d000ecb00`；没有沿用旧候选的通过结果。editor.js 源码 SHA-256 仍为 `27390a0df6d2546e7a89d17c73c7ea2e4c3e852a9c5fc3bd9e6850e9cada9b2c`。

随后集成负责人重打包共享错误提示与 PDF 图片选择修复。最新 `browser-pdf-fix-boot/report.json` 再次完整通过 5 项，实际 HTTP 加载的 boot SHA-256 与磁盘一致，均为 `e7d0b2a10d18f73adb6730751ecabe479c3ff18c4054625d33dee509c7ca4a55`；该轮没有重复与 boot 无关的 HTTP 磁盘用例。

## 尚未实现与未认证范围

尚未实现表格排序／筛选、合并／拆分单元格、可视化表格设计器编辑已有数据、公式计算、单元格样式、工作表／工作簿及原生 Office 文件中的格式往返。本切片未增加 HTML 剪贴板表格解析、分号或其他方言 CSV 的自动探测，也不把普通纯文本 CSV 自动转换。

现有证据不认证 Excel／WPS／Word 的实际应用复制过程、原生可见桌面窗口、所有 WebView／浏览器／操作系统、所有 Unicode 空白字符、所有手写 GFM 表格、超预算大表操作或全部办公场景。正式版本仍沿用当前发布版本，本规格不改变总账本或发布状态。
