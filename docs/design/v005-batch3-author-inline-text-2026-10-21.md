# 第三批 P06-B：作者行内字号实施规格

状态：限定主链已实现并实测；完整功能、外部应用与跨平台验收仍保留各自缺口。

<!-- scoped-author-inline-20261023 -->
2026-10-23：普通作者字号、共同基准与受支持 DOCX 字段／题注已完成本次限定真实链路；四项外部脚本复制差异、caption 非 DOCX 消费、任意未知 live field 等仍开放。本次证据来自 candidate-47 的 reader、author、engines、mathsection、settings、numbering、citations、notes、tex 流程，原候选首败仍保留；未用测试保护代替功能完成。[限定验证报告](../reviews/readmd-v005-batch3-author-inline-roundtrip-2026-10-23.md)。

## 内容合同

新增独立的 `AuthorTextStyle`，不扩张 CSL 的 `RichFormat`。字号采用 Word 半点的精确整数；西文 `size` 与复杂文字 `sizeCs` 独立，区分未声明、绝对点数及段落基准重置。`cs`、`rtl` 保留省略、显式关闭与开启。

```markdown
同段 <span data-readmd-text="v1" data-size-pt="30" data-size-cs-pt="10">大字、العربية 和 **粗体**</span>。
<span data-readmd-text="v1" data-size-pt="30">外层 <span data-readmd-text="v1" data-size-pt="base">回到本段</span></span>
```

普通 Markdown 不物化默认标签。标签从段首开始也必须可解析；只对真实的受控行内语法处理，代码、HTML 示例、注释、图片替代文字和 YAML 不成为格式指令。禁止额外 HTML 属性、CSS 字符串、无效单位、重复属性、跨段或跨格式边界的结构。

局部事务使用当前修订和 UTF-16 范围；不按文字匹配。清除直接格式与重置到段落基准是不同操作。保留未触及源码、BOM、CRLF、逆事务、无修改操作和重复执行检查。原生导入按 story/run 的实际节点身份映射，不凭文字和辅助旧缓存恢复。

## 尺寸与消费者

- 旧整段 `readmd:layout text.size` 继续共同覆盖两轴；没有整段覆盖时，注释模板两轴保持独立。
- 源码和 DOCX 保留原生合法半点值；版面渲染另设 4096pt 上限，超限明确拒绝，禁止钳制后报告成功。
- `cs=true` 或 `rtl=true` 对整段使用复杂文字尺寸。自然文字按已有锁定的 Unicode script 属性区分，不能将所有非 ASCII 字符归到复杂文字。
- 公共 itemizer 只做尺寸轴与逻辑文本分段，不冒充 bidi 或字形整形。附加符、连接符和变体不能被丢弃或改变源文。分类规则参考 [Word 字号](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.fontsizecomplexscript?view=openxml-3.0.1)、[cs](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.complexscript?view=openxml-3.0.1)、[rtl](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.righttolefttext?view=openxml-3.0.1) 及 [LibreOffice script 类别实现](https://github.com/LibreOffice/core/blob/master/i18nutil/source/utility/unicode.cxx)。这是互操作参考，不能代替 Word 实际编辑认证。
- DOCX 写原生 `w:sz/w:szCs/w:cs/w:rtl`；已有段落后处理必须保护作者显式轴，不能将同段所有 run 重写为同一字号。最终包不泄露内部定位标记。
- 阅读与编辑预览共用 Rust 生成的有源绑定投影。前端只恢复白名单数值 CSS，旧源码内的任意样式不能取得权限。嵌套重置使用实际段落字号，字体/阅读设置更新后重新计算。
- HTML 共用同一投影与运行时；EPUB 写安全 XHTML 和物理点数。数字字号、链接、粗体、空格、脚注及纯文本必须保存。
- 实测材料准备时发现既有 HTML 链路可能将脚注当成链接定义而隐藏正文。真实脚注改由共享 Markdown 事件生成完整 HTML，重复引用对应同一注释和独立返回链接；源示例、未引用定义、Unicode 标签和数学不得丢失。运行时恢复字号后再执行公式排版；独立检查读取实际渲染分支，不能只检查隐藏的源码 JSON。
- Rust PDF 测量与绘制共用真实字号，段落重定向时只重算继承和基准重置轴。现有引擎无完整 GSUB/GPOS/bidi 整形；新增作者尺寸下无法正确处理的文字在创建输出前精确拒绝，不算正向支持。Typst/TeX 的整形、字体与方向能力分别实测。
- TeX 作者字号与新作者块基准采用物理 `bp`，Typst 采用 `pt`；不以 TeX 自身较小的 `pt` 混用测量。旧未标记 TeX 输出不在本增量内重写。
- `Block::Code { text: String }` 尚不能表达任意混合原生 run。无局部作者属性、整组均匀且只继承文档默认值的普通代码，保留原有文本／分节转换；这不认证原生代码字号。混合代码和未建模的活动字段仍须准确保护。

## 原生字段与基准的补充合同

实测发现裸 writer 默认 11pt，真实 MCP 省略预设时使用简约预设 10.5pt。导入不能用一个固定默认值误拒正常字段，也不能省略必要基准，让保存后重开悄悄换字号。

- 原生 `docDefaults` 明确声明共同西文／复杂文字字号、且可由既有全局样式精确表达时，将实际消费的基准写入 `readmd.publication.typography.size`。11pt 同样必须保存；不从当前段落／字符样式反推旧设置，不新增旁文件。
- 该源叶只接受 8–20pt 的精确半点值；源 patch 的 null 删除、空对象保持、精确逆事务及显式预设／请求覆盖沿用既有规则。独立作者 run 的合法半点范围不因此收紧。无法共同表达时保留各已知轴，不能把缺失复杂文字轴补成西文字号。
- 已识别的交叉引用和引用控制，在当前原生结果 run 的作者轴均匀且可表达时，以既有 `AuthorStyled` 源码保留，字段身份仍保持。新字号如 30pt 并非必须拒绝；真正无法等价表达的混合字段、未知活动字段和非法属性才拒绝。
- 自动题注的前缀、编号与标题共用既有局部声明 `readmd:layout`，增加 caption-only 的 `text.author`，直接复用 `AuthorTextStyle`。只写声明的轴；标题自己的行内作者格式优先，显式 false 也必须保存。未知轴不伪造；不增加来源 mask 或另一个黑盒。
- `text.author` 首先实现 DOCX 原生闭环。其他导出及阅读投影在真正消费前须给出准确的不支持诊断，不能忽略属性后报告保真成功。对其余消费者的支持单列待办，不计入本次正向覆盖。
- 私有作者定位标记只存在于受 nonce 保护的写包过程，最终 DOCX 中必须移除；不能授予用户源码内部权限。新声明预算不得给无该声明的普通长文附加限制。
- 验收包含缺少 Caption 样式及缺少复杂文字默认值的真实原生材料、10.5／11／14pt 保存重开后空 options 重导、当前值修改、再次原生导入、字段身份、标题覆盖、混合格式反例和旧目标保护。

## 实施与验证

1. 模型、受控语法、源映射、范围事务及 AST/纯文本/JSON 遍历。
2. 正文、标题、表格、脚注/尾注和链接的原生导入、当前值优先及 DOCX 原生输出。
3. 阅读/编辑投影、HTML/EPUB、Rust PDF、Typst、TeX 的真实尺寸和继承消费；MCP 与桌面 HTTP 使用相同事务服务。复用原有局部格式面板，不增加常驻工具栏。
4. 锁定新候选和独立材料后，实际导入 → 定点修改 → 撤销 → 保存 → 正常退出 → 新进程重开 → 空 options 导出 → 再导入。独立核对原生半点值、字符顺序、实体、链接、粗体、重复文字及段落边界。
5. 正反用例包含：30pt/10pt、只改一轴、嵌套继承/清除/重置、强制复杂轴的 Latin、真实 Arabic/Hebrew/Indic/Thai、CJK、空格、注释双 story、代码/图片/YAML 示例、无效标签、过期修订、重复请求、已有输出保护和资源预算。支持与拒绝分别计数；不能把 guard 用例计为排版正向证据。
6. 完整 Rust/前端/仓库检查、离线 boot/EXE 构建、真实 stdio/HTTP 保存重启与输出检查。保留首次失败；真实 Office/阅读器和其他平台仍未验收。

必须先有实际候选与产物证据，才能将本子范围从实施中更新为已验证。
