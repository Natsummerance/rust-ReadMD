# 第三批下一单元：公式原生段落样式链

2026-10-24。承接六单位 C58，本规格已完成下述有限原生样式链实现与候选C61实测；全部范围及未验收边界见末尾结果，不据此认证完整M07。

## 目标与准入

当前源码已经保存选定样式链、docDefaults、direct pPr 和逐轴来源，但原生出口仅返回有效值，生成 `ReadMDMath/direct`。目标是让获准的公式真正使用导出的 pStyle/basedOn；在外部应用改样式或删除直接覆盖后，实际继承应生效。

先实现有限 pPr 样式链。原 DSL、raw 单位、源身份不改；有效布局值仅用于预检。run/character/theme、全部 styles 包和特殊宿主不随此单元扩大。

**全局默认是独立门槛。** 一个 DOCX 只有一份 docDefaults，不能首式胜出，也不能直接拼入异源默认。精确保留要求所有来源的每轴 missing/empty/explicit 相容，并与模板默认、普通正文和裸公式严格兼容。把 defaults 提升为数学基样式会改变 origin；给 Normal/BodyText 偷补0会改变缺省声明。二者都不能用来追认严格来源保真。

不兼容域保留当前源模型，原生输出继续明确记录有效轴物化的损失边界；若采用严格保真模式则提前拒绝。必须复用既有能力与导出损失报告，不增默认界面或静默承诺无损。

## 数据路径与文件

1. `math_word/native_paragraph.rs`、`math_defaults.rs`：复用 `math_word/paragraph.rs` 已验证模型提取 native 描述，包含原 direct pPr、选定 paragraph basedOn 链和 pPrDefault 三态；不由有效值反推声明。
2. `omml.rs`：在 `materialize` 之前保存描述；有限保护保持，视觉继续消费原有效值。
3. `math_word/native_paragraph.rs` 中的 DOCX 专属 registry：预收集整篇定义和模板保留 ID。依据完整结构判断相等；无冲突保留 ID，异定义冲突确定性分配 ID，同时重写 pStyle/basedOn，形成内部 alias 表。不能只凭名称或私有前缀授信，不留下悬空引用。当前公开导出仅返回统一损失警告，尚未提供结构化 alias/loss API；实际来源由独立 OPC／重导入证明，不以内部 payload 冒充用户可见报告。
4. `docx_writer.rs`：向现有 styles part 写获准链；公式外层写原稀疏 pPr。普通 Markdown、裸公式、inline 和未获准来源沿原路径；原有严格隔离回归不变。

## 先写测试，再接真实流程

- Registry：模板撞名、跨源同名异定义、结构相同／不同、输入逆序稳定、映射重导入稳定、basedOn 全链、悬空／循环和数量预算。
- 稀疏语义：direct/style/defaults；缺省、空容器、显式零；六单位；可选属性 null 与删除容器；删除 direct 后实际继承恢复。
- 隔离：普通正文／裸公式、不同来源默认、表格／引用／注释、页面格式不受影响。`docx_math_style_isolation_tests::ordinary_equivalent` 的 missing/0 区分必须保留。
- 新候选实际流程：native-only DOCX → 转换 → DSL → 第二重复式精确修改／撤销重做 → API 保存 → 正常 EOF → 新 PID → DOCX → 独立 OPC 和再导入；比较每轴真实来源与 alias，而不是从源码生成期望再回填。
- 外部包直接改生成的当前样式定义，再导入证明当前值生效；不增加 customXml 旧载荷恢复协议去覆盖用户新修改。
- 不兼容、过期、格式和预算错误均在暂存／替换前拒绝，保留旧目标和目录。源文本、实际 native 来源和物理几何分别验证。

固定准入 → registry/projection/writer → `--locked --offline` 编译及完整测试 → 新候选真实操作 → 独立审查和限量归档 → 更新子项。当前不用 GUI；真正 Word/WPS/LibreOffice 的样式操作和排版仍需另取应用证据，不能用 OPC 检查替代。

## 实施前固定的接口与验证细节

捕获的 `NativeParagraphSource` 为封闭结构，保留原 direct／链／默认状态，不从有效值回推。两阶段 registry 先收集同篇候选再决定 alias，实际输出时才登记使用；只写真正使用的链。上限为4096来源、4096定义、128层、16MiB描述（规范化键与保留的原 pPr／defaults／链容器共同计费，含注释和未登记的 mark-rPr，不含公式运行）；收集时即做累计检查，不等堆积后再拒绝。

样式 ID 与 UI 名称分开处理。ID无冲突保留，冲突按完整祖先结构决定稳定映射；UI名称同时避开模板和本次新名称，不承诺原 primary-name／aliases 元数据保真。此区分参照 [Microsoft 的样式 ID、名称及 aliases 说明](https://learn.microsoft.com/en-us/office/open-xml/word/how-to-create-and-add-a-paragraph-style-to-a-word-processing-document)。[python-docx 的样式设计说明](https://github.com/python-openxml/python-docx/blob/master/docs/user/styles-understanding.rst)用于检查“实际引用已有定义才生效”的工作流；产品仍为自有 Rust 实现，不引入 Python 或该库。

属性中的tab/CR/LF以数字实体输出，避免 XML 再解析改原词法。直接段落 mark rPr 尚未递归认证，原本可走新样式链的该组合保留有效值物化并提示，不盲目复制；无链的旧直接格式路径不伪称新增来源保真。

固定新增提示 `math_paragraph_style_materialized`，经现有导出警告区显示，46语言都有对应文案，不新增控件。非空pPrDefault的全局兼容仍未证明，不修改普通正文默认来追认通过。

公共 `api_codes` 必须精确登记该机器码，`warnings`／`warn_items`／前端翻译共用它；后缀、路径或诊断文本不能冒充已登记警告。普通正文隔离除逐样式／段落外，还要比较完整 `docDefaults`（包含 `rPrDefault` 及缺省／空）和最终 `sectPr`，只检查段落默认值投影不够。

真实测试以A无pPrDefault、B空pPrDefault、F非空默认三种原生输入起步；A/B分别覆盖普通链、模板Normal/ReadMDMath撞名、跨来源同Equation异定义、空ind/显式0/默认选择，正逆组合的alias需稳定。实际修改已导DOCX的当前style或删direct后再导入，变化必须生效，其他ZIP成员保持。

普通正文隔离使用**无关联Word上下文、但mathPr相同**的控制式。真正无配置LaTeX默认为dispDef=true，A/B需dispDef=false才能消费Word物理轴；现有共享全局设置合同拒绝两者混合。本单元不得将它们归一为等价来取通过：增加真实DOCX负控保护旧文件，保留该组合缺口，不将匹配设置控制冒充默认LaTeX兼容。

P04 其余参数、完整 CSL、正式 TeX 材料／离线包、受控冷态、全故障／八小时和其他平台继续开放，完整第三批不因增加设计文档结项。

## 真实流程后的对照修订（编码前固定）

C59在测试准备阶段因共享runtime报告缺少limitations数组失败，未启动产品；保留原候选和首败。C60两组主流程14份输出通过，随后external-mutation对照首败：原MD自动色与已导DOCX的模板`rPrDefault/color=262626`被混作同一渲染上下文。未改的native重导入与current-a第0、2式OMML逐字节一致，第1式只删除直接left；故不能删除fill断言或改产品去伪造缺省值。

下一候选先保留同一未修改native DOCX的真实重导入内容，API保存并读回，逐式取得实际预览，作为current mutation的唯一SVG基准。另增加A/B各Rust／Typst两份未改native-readback PDF控制，使PDF差分也处于同一默认上下文。**在实施前将正向产物上限27修订为31，几何记录38修订为48；仍12组检查、30份旧目标保护、每份16MiB／总128MiB。** 原主流程输出及原阈值保留；不把控制文件藏在计数之外。

原MD→native的默认颜色变化单列来源报告，记录原始run声明、实际输出docDefaults、重导入上下文和文件摘要；`originalToNativePaintIdentityCertified=false`，run/theme保真仍开放。native mutation前后完整rPrDefault相等；A的非目标公式XML相同、目标仅left被移除；B仅实际styles中的选定right改变，document成员原字节不变、被直接覆盖及不引用该链的公式有效值保持。严格SVG fill／glyph／全部primitive断言和PDF曲线／paint断言、.00003pt／.002pt阈值全部保持。

## 本轮实施结果

C61有限段落原生样式链已实现并通过实际流程：Rust3672/0/3既有忽略、前端工具1097/0/0、六门槛；12检查/31输出/48几何/30旧目标保护，同一二进制另12组复杂出版回归。C59准备错误、C60跨上下文对照首败及最初Rust旧断言首败全部保留，未删除颜色断言或放宽阈值。见[完整结果](../reviews/readmd-v005-batch3-math-native-styles-2026-10-24.md)和[下一执行单元](v005-batch3-after-native-styles-2026-10-24.md)。非空全局段落默认、默认LaTeX混排、run/theme、公开结构化alias/loss及真实Word互操作继续开放。
