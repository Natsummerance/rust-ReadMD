# 第三批 M07：水平 LTR 公式的起始／末尾对齐

状态：本单元限定实测通过，candidate-57；[结果、原始失败及边界](../reviews/readmd-v005-batch3-math-logical-alignment-2026-10-24.md)。不改变 C53／C54 的冻结记录，也不开放 Typst 非零正文基线。完整 M07、P04 和第三批继续保留未完成状态。

## 可交付范围

当前 `w:jc=start/end` 可以解析、按源修改及原生 DOCX 保存，但有限公式投影只接受 left/center/right，导入普通 LTR Word 公式因此可能无法预览或导出。本单元在**已证明水平 LTR 的独立正文显示公式**中，消费 start→left、end→right。复用已有物理对齐和字号／缩进几何，不增加 UI 或依赖。

原生值和投影值分开：原始 DSL／OMML、Word 枚举、稀疏声明、注释、节点及继承来源不变；`WordFiniteParagraph` 的输出副本使用物理对齐，供共享矢量、MathML、HTML、EPUB和两套PDF使用。DOCX仍保存start/end，不把源文件悄悄改为left/right。

这里“继承来源不变”指原生输入转换得到的DSL、正文事务、保存和新PID重开；不扩大为生成DOCX的原styles包往返。现有DOCX出口使用ReadMDMath并把有效轴物化为direct，选中样式链／docDefaults的原来源不会原样写回Word样式包。本单元必须记录这一既有边界：生成DOCX保留logical枚举及有效轴，再次导入按实际原生来源重新计算，报告`nativeStyleProvenanceRoundtripCertified:false`及两端来源对照。完整Word样式来源往返继续列为M07未完成项，不能据本轮对齐通过销账。

## 输入及优先级

- 沿用当前 `word.pPr`／`word.jc` 语法和原生 DOCX，无新协议。
- docDefaults→实际选中的basedOn链→direct按既有规则覆盖。缺省、空声明和显式false不混淆。
- 仅限 `dispDef=false`；局部数学jc仍优先，`dispDef=true`仍由数学默认值负责。
- bidi省略或有效false，textDirection省略或lrTb，且原有其他方向／宿主guard通过才可消费逻辑对齐。tb别名不在这两个新枚举的证明域内；原physical left/right行为不变。
- `w:ind start/end`、RTL、mirror、字符单位、首行／悬挂、分布对齐、spacing/grid/tabs和特殊宿主不是本单元交付。保留原错误和旧目标保护。

## 实现边界

1. `math_word/paragraph.rs` 增加纯输出映射，只修改finite副本；不在parse或源事务中改写枚举。
2. `omml::word_justification`只接收有效的物理三值，不把新增枚举送入unreachable分支。
3. resolver、OMML预览资格及共享布局分别验证direct／style／defaults、原origin、math jc覆盖、dispDef和方向负控。
4. 不修改native materialize的pPr顺序、rPr原插槽、源码保留或已有toolbar/4K文件。无前端改动时boot不重建。

## 实际测试合同

先固定全新候选、输入和检查器。两份native-only DOCX分别使用start/end；每份含三式相同数学内容，但有效值分别来自direct、选中样式链和docDefaults，低优先级放相反值。独立ZIP/XML检查实际来源，不伪造旧customXml协议。

真实MCP流程：转换→迁移→精确第二式属性事务→逆事务／撤销与陈旧修订拒绝→保存→正常EOF→新PID重开→空options导出。预设通过已有临时CAS控制纸张和引擎，finally恢复原摘要，复用运行库不计冷态。

换行输入在操作前明确固定：先将native转换及迁移的LF正文原样经API保存在独立`*-native-lf.md`；再以已迁移DSL建立新的全CRLF测试输入，保留原始输入字节／摘要及与LF源的语义等价依据，用实际读取API确认后才开始CRLF／astral编辑和保存。本机保存服务按文件原有换行写回，不能将混合换行请求的原摘要直接当作持久化期望。两份LF原稿和两份CRLF派生输入均在本任务owned目录内，创建它们属于测试材料准备，不认证产品的复制／另存功能；失败后不得改写当次输入或归一化其结果以追认通过。

首个实际输出队列上限14份：start/end各DOCX、HTML、EPUB、Rust PDF、Typst PDF，共10份；对应physical left/right控制各两套PDF，共4份。第一次失败停止，后续修正必须新attempt，原报告不覆盖。

独立检查：

- DOCX：唯一合法pPr、完整公式和正确有效轴，native枚举仍为start/end；再次导入／迁移按当前原生来源计算，不粘住错误旧属性。原DSL稀疏来源另行严格核对，不用生成DOCX的direct物化冒充原style来源保留。
- 源事务：只改当前第二式的真实UTF-16范围；邻式、注释和其余属性不变；删除direct后恢复继承。
- SVG／PDF：start/left与end/right的实际字形、advance、字号、纵坐标及页面数量相同，横向位置由纸框、物理缩进和独立intrinsic advance计算，沿既有0.25pt容差；不从输出墨迹反推期望值。
- HTML／EPUB：检查实际选择的SVG／MathML及全部spine中的公式，物理wrapper对应left/right；来源script存在不算可见输出通过。
- 负向：继承bidi=true、mirror、vertical／未证明方向、逻辑缩进、其他jc、缺值、错URI、坏pPr顺序和预算仍准确拒绝；已有输出不变。全局／局部非零Typst baseline的既有guard不松动。
- 完整Rust库、前端／工具、六门槛绑定新源码和EXE；不以构造对象或XML合法代替真实产品文件。

Windows仅采集真实CLI／MCP和实际引擎产物。本单元不认证Office应用、独立EPUB阅读器、TeX原生OfficeMath、完整M07／P04／第三批或发布。

## 2026-10-24 实测结果与下一单元

C57冻结1103源文件；640个Rust/assets输入逐行对应已完成的锁定离线编译，最终EXE摘要一致。完整Rust库3634通过、0失败、3既有忽略；前端/工具1055通过、0失败/跳过，六门槛通过。真实stdio完成7组检查、192次工具调用、196对线协议请求/响应、158次独立检查器调用，得到14份产品输出和28条几何记录；14个拒绝场景保留已有真实PDF。两个不同PID正常EOF，源修改/逆事务/陈旧拒绝/保存重开及DOCX当前逻辑值再导入均通过。

C56首败是QA的混合换行预期与原文件EOL保存合同不符，原报告、请求、响应和磁盘未改。C57另建明确的CRLF材料，保留LF中间稿、完整原文、模型/来源对照及严格字节摘要，未归一化旧失败文件来追认通过。生成DOCX的ReadMDMath/direct物化边界保持明确。

下一有限单元为left/right单位字面量中**可精确换为当前0..31680整数twip**的子集：pt/in/mm/cm使用有界checked有理数，整除后才消费；源字符串和每轴来源不变。例如36pt/0.5in/1.27cm等于720twip，18pt/0.25in/6.35mm等于360twip。非整数twip、负值、其他缩进轴及原有方向/宿主保护继续独立跟踪，不能四舍五入或用0代替。

先补纯转换和来源/稀疏/极值控制，再使用两个native-only输入、真实第二式单位修改和删除恢复、LF/CRLF保存重开、14份输出及固定整数物理A/B对照。独立检查器使用Decimal/Fraction，不从产品结果反推期望，也不放宽0.25pt容差。单位词法/原样式包的生成DOCX往返仍需分别声明；未开发前不计此单元完成。详细只读设计随本轮证据保留在`evidence/root/next-plan-01.md`。

## 依据与复用

复用现有math_word、omml、共享矢量、DOCX/HTML/EPUB writer及已锁定Typst，不新增组件。微软列明[start/end为Office 2010起的合法枚举](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.justificationvalues?view=openxml-3.0.1)，[段落jc沿样式层级继承](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.justification?view=openxml-3.0.1)。本单元只在明确LTR条件作物理投影，不从这两项文档推断完整RTL支持。
