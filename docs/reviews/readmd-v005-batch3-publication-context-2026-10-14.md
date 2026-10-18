# V0.0.5 第三批：共享出版元信息与引用上下文

日期：2026-10-14。P01/P07 核心已实施并完成下述限定 Windows 验证；R025/R026、完整第三批、其他平台及发布仍未整体验收。

## 用户入口与真实实现

共享作者／机构／基金及书籍模型、有界 YAML、稳定 ID 关系、继承／覆盖／清除和 UTF-16 正逆事务已实施。既有文档属性与导出高级区支持结构化行及稀疏用户预设；DOCX core/custom、PDF Info/XMP、EPUB OPF 和 typed 前置内容共用模型。

逐项富 affix、locator、normal/suppress-author/author-only/prose、真实 note 状态、排序折叠及消歧已实施；byte/UTF-16 字段范围、修订与 nonce 唯一归属贯通 typed AST、reader／split preview 精确定位与多格式输出。ReadMD :cite[author-only ...] 是扩展，不冒称 CSL/Pandoc 标准。

文档属性维护论文／译著材料；导出高级设置选择继承／覆盖／清除并保存用户预设。正文引用使用 BibTeX/CSL JSON 和样式资源；阅读／编辑预览点击或键盘激活准确定位重复引用。预览、取消、样式切换不回写源；显式编辑沿用撤销、保存、重开与修订保护，不增加常驻工作台。

出版元信息由 publication_metadata/metadata_formats 共用；PDF 最终有界增量 helper 在映射清理之后写入 Info/Lang/XMP，保留实际原前缀／非元信息对象与像素。publication_citations/publication_rich 和 parse_prepared 保留来源与安全富格式。锁版 Hayagriva 0.10.1、citationberg、离线构建及许可边界保持；私有官方 oracle 原件不再分发。

## 当前候选与实际测试

EXE SHA256：a231a2f545c002590454e47b8768f4ab0abc6573102265f576faf05ed54acf3c（70362624 字节）；boot SHA256：2610f804e9a5fb9114ec61b74aa7e7eec9af01e4b6baf14d15d08caf87c8908b。源码清单 SHA256：f6cbf2794aaecdb2d094d7fa142ef4f2ed4685ed5ee94c40cf9b54b48cca1754。

完整离线 Rust：2553 通过／0 失败／3 历史忽略，146.34s。仓库门槛 8 项通过。

| 套件 | 实际范围 | 证据 |
| --- | --- | --- |
| metadata | 16 组全过，单次完整执行 | [原报告](../evidence/capability-upgrade/2026-10-14/publication-context/final/metadata/report.json) |
| citations | 14 组全过，单次完整执行 | [原报告](../evidence/capability-upgrade/2026-10-14/publication-context/final/citations/report.json) |
| mcp | 7 组全过，单次完整执行 | [原报告](../evidence/capability-upgrade/2026-10-14/publication-context/final/mcp/report.json) |
| engines | 16 组全过，单次完整执行 | [原报告](../evidence/capability-upgrade/2026-10-14/publication-context/final/engines/report.json) |
| layout | 28 组全过，单次完整执行 | [原报告](../evidence/capability-upgrade/2026-10-14/publication-context/final/layout/report.json) |

P01 原生 16 组全部通过；(35 个逐字段案例＋2 个基线)×3 格式＝111 个矩阵文件，整套 121 个实际文件；40 个离线 EPUBCheck 零致命错误／错误／警告。

P07 原生 14 组全部通过，3 个实际输出；独立 oracle 20 必需通过（15 个官方 CSL test-suite＋5 个 Juris-M／citeproc-js 补充）、0 必需失败，1 个可选扩展未支持，整体 oracle passed=false。共享真实 stdio 7 组，不重复计数。

文献布局 4 个案例／20 个实际文件／12 个 PDF：reader 与 Rust、Typst、XeLaTeX 真实 PDF 几何通过；DOCX/EPUB 仅验证实际包布局参数／结构，不认证 Word／独立阅读器几何。字形 proof：typst 4 字面／72 字形；xelatex 4 字面／72 字形。

引擎实际输出 8 个；46 语言源面板词条／方向 smoke 不等同人工译校。旧候选或复合续验不混入本表。见[不可变 manifest](../evidence/capability-upgrade/2026-10-14/batch3-publication-context-manifest.json)，SHA256：111e5dc43d18309f67fe7f966a06abdc5afbda1f07e71020bcb4fa6a94bb069f。

## 原始失败与修复依据

早期 Typst 的 U+1F600 静默缺字由独立解析与实际字形检查发现，回退字形和缺字拒绝按实际套件范围验收，不宣称覆盖任意 Unicode。另一次 Typst 请求了锁定引擎未内嵌的 Libertinus Sans，真实 PDF 字体／字形检查发现斜体落回直立 SC Regular；后续选择实际可用的 Libertinus Serif 字面并保留中文／补充平面回退。源码存在 italic/smallcaps 指令本身不是通过证据，最终字形 proof 按实际轮廓和正常／大写对照验收。

分页 nonce 清理曾压平元信息增量，导致最终 /Prev 无法绑定真实基线；修复为先清理映射、后在同一原子 staging 内写最终元信息，保留原字节、对象及逐页像素检查。源映射另修正阅读原文与预处理显示文本的修订绑定，以及 HTML entity 分号被误当 cluster 分隔；重复同文只按 byte/UTF-16 范围定位，不按显示文字搜索。

原始失败、修复前源码和诊断按 manifest 保留，不能改写成成功或由新结果覆盖。随后 CFF 可选 FullName 读取曾造成 QA 误拒，修正仅在检查器；实际 PDF 和旧失败原件保留，最终以新 QA 版本完整执行为准。官方 RESULT 保持固定字节；历史 locale 使用可追溯的原运行时版本，不从产品输出生成 golden。

TeX 实测另发现资源校验挤占编译预算：旧候选在第 111.65 秒才创建 sandbox，120 秒 HTTP 超时，主进程 CPU 仅增加 3.17 秒。修复把重复祖先检查统一为预检、完整文件哈希、复检三个有界并发阶段，每个阶段全部 join 后才进入下一阶段；保留逐文件同句柄的长度／全字节摘要、持续 reparse 拒绝、取消和最终路径检查，不引入跨请求可信缓存或提高超时。当前输出流程按上述同一新候选实际验收；不把这一优化认证为完整并发路径防替换或正式冷态性能通过。

继续编译后暴露 core Unicode 字体路径的两个错误：全 cat12 扩展名不匹配锁定 fontspec 的外部字体识别，修正类别后完整路径又被 NFSS 当作带斜线的字体家族标识。独立开发探针保留原三个结果及失败假设，第四个新用例使用安全 token 和固定 NFSSFamily 成功生成 PDF；两个解析器及原字体轮廓确认了实际字符。随后才应用产品补丁，并以当前候选完整工作流验收；没有借系统字体、上调超时或把校准探针当成产品认证。

修复后的 XeLaTeX 已产生真实 PDF，预览与最终文件一致，但旧 QA 只从 pypdf 合并 trailer 读取 /Prev，误拒实际最终交叉引用流中的修订指针。校验器现在仅在 trailer 缺失时，按最后 startxref 指向的精确对象解析 /Type /XRef 及整数 /Prev，仍要求唯一原始结束记录、早期范围和页数一致，全部验证后才以 xb 创建基线。真实 XeLaTeX 文件和合成传统交叉引用表两种正例，以及八种损坏／伪造／越界／覆盖负例符合预期；保留原失败 PDF、旧检查器和控制记录，再以修正的 QA 完整执行同一产品候选，不将控制材料或成功前缀拼接成最终通过。

随后对象比较只多出了实际新增的交叉引用流，129 个原有普通对象、页面、资源、注释和像素均不变。检查器仅排除真实 startxref／Prev 链上、基线中不存在、位于增量区且没有普通内容引用的新索引对象；旧索引对象仍按当前有效映射的原始字节解析并比较，避免 pypdf 的旧对象缓存掩盖重定义。21 项控制包含真实 XeLaTeX／Typst 文件及链外伪索引、复用 ID、旧索引改写、内容／字体／注释和退休元信息破坏；原前缀及像素门槛保持。[结构依据](https://pdf-issues.pdfa.org/32000-2-2020/clause07.html)限 PDF 修订与交叉引用结构，不代替实际文件证据。

执行后半段前还确认 TeX 原先忽略 Block::Styled 的局部字体。产品补齐固定离线 Termes 四字面的段落、标题、嵌套内容及脚注作用域，结束段落后恢复周围字体；小型大写和正常／大写对照共用同一物理家族，其他属性及字体继续明确告警。另修正独立 TeX 源检查对原始 SMP 连续文本的错误假设：只识别保留原字符的受控 readmdunicode 序列，并核原版字体及许可，11 个正负控制通过；实际成品字形仍由最终完整原生套件独立检查。

四字体面校准进一步发现，锁定 fontspec 的粗体小型大写形状替代会落回普通粗体，而渲染正确的常规／斜体小型大写在复制时也变成大写。先用一次受管编译生成 BASE／FEATURE／ACTUAL／LOCAL 四组真实对照，再采用独立固定小型大写字体族和仅作用于当前文本的 XeTeX ActualText 标记。局部分组退出后才输出页面，核对前后普通文字、中文与数学哨兵；最终产品仍以完整原生输出的四字面、72 个原字体轮廓和原文复制检查验收。[固定上游实现](https://github.com/TeX-Live/texlive-source/blob/695d4a6d19a9567b66697b2697dccebc752b78e8/texk/web2c/xetexdir/xetex.web#L16604)用于解释节点级标记，不能代替锁定二进制实测。pypdf 6.16 忽略 ActualText，保留其原始提取结果并披露该解析器限制，不宣称它验证了小型大写复制。

实际 Xe PDF 省略空格字形，且 CID-keyed CFF 的 CID 不是子集数组下标。检查器只对选中的真实探针字体，核 Type0／Identity-H／CIDFontType0、嵌入流原字节、ROS 与实际 charset，再把 CID 映射到真实 CharString；标记边界由完整文字和同页基线、实际物理间隙确定。普通 GID 路径、唯一资源、精确原文、原字号和正常／大写／GSUB 轮廓门槛保持，正反控制与原始失败一起归档。

引擎与布局按明确顺序复用同一数据目录。root 在引擎结束后、布局启动前实际观察到所查宿主及驱动进程为零，并保留了带采样时点的记录；另一代理的较晚样本原样记录了合法布局进程，没有改成零或误报引擎失败。布局结束后的独立 CIM 再次核查两个任务目录。此流程不是同时运行两个共享数据宿主。

本轮还用当前候选生成的合成 DOCX 进行了单次、45 秒上限的真实 Word 探测。它在 COM 激活阶段返回 WORD_WORKER_TIMEOUT，尚未打开文档；没有另存或 PDF 结果，原文件不变，后验 WINWORD 为零。该环境诊断保留为未验，不混入五份通过报告，也不把 DOCX 的自包含生成能力绑定到 Office 启动状态。

## 尚未整体验收

保留完整 CSL 覆盖及 cs:intext 可选兼容、复杂多段／嵌套 display、Light、长注复杂分页、全部预设参数组合、可编辑引用来源往返、真实 Word／独立 EPUB 阅读器、其他平台与长期压力门槛。旧通用提示仍观察到中英混用；新增词条检查与46语言冒烟不能替代全界面语义审校。正式 TeX 包及独立冷态 120s 仍未认证；600s 完整性诊断或复用运行库的 ready=true 不构成性能通过。

普通注释型 CSL、近引与作者脚注隔离已有证据，不能重新列为全未实现。标准两列 display 已实现，复杂布局显式拒绝；DOCX/Typst/静态 TeX 的 Light 拒绝、核心 PDF 的损失诊断及 HTML/EPUB CSS 字重不等同物理 Light 支持。当前格式输出不认证回导后仍保留可编辑 CSL 来源。46 语言人工审校、全部复杂脚本／字体及真正冷态全流程仍单列。源码／XML／PDF 检查不代替 Word 或独立阅读器，版本未发布，整批状态由原账本判断。

归档来源补充说明：旧 n2 引擎报告对应的一份原 QA 源码，以及原计划中一份 148 字节的历史 no-Python 失败日志原字节不可用。来源缺口和当前可观察版本按各自摘要保留在 manifest 中，不替代原件、不用于认证当前候选；当前五份完整报告及对应源码、八项新检查均另行绑定。

本轮完成后已清理 60 个旧测试缓存组件，合计 6,493 文件、2,213,064,814 逻辑字节；见[缓存分类与清理记录](readmd-v005-publication-cache-cleanup-2026-10-14.md)。
