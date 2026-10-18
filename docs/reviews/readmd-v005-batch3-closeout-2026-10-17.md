# 第三批本轮收尾与剩余任务（2026-10-17）

本轮只收尾已启动的字距、公式字面语义／算子、电子书导航与 PDF 字体工作；不把第三批整体或 V0.0.5 发布标为完成。完整历史及 47 项边界保留在[剩余审计](readmd-v005-batch3-remaining-audit-2026-10-15.md)，下列最新结果优先于历史时点描述。

## 已归档的真实结果

| 范围 | 实际结果 | 明确边界 |
| --- | --- | --- |
| 普通段落字距 | 8 组 Windows 流程，6 份 DOCX；重复段落定位、撤销、BOM/CRLF 保存、重启；两种窗口明暗截图已复核 | 不认证 Word 字形、其它格式字距或全部排版参数 |
| 公式字面字符 | 9 组 Windows 流程，4 份 PDF，真实 stdio MCP；第二重复公式内指定字符、取消、一次撤销／重做、保存重启 | 不认证全部 OMML／Word；重开截图的瞬时通知只作为 QA 证据 |
| TeX 行距与几何错误 | 8 组、56 次请求，7 份新 PDF，21 项旧目标保护及重启；实际 18pt 基线、中文和 emoji | 18 份旧 PDF 不算当前候选复跑；warm 运行不认证冷态性能 |

三组均绑定候选 `adf6928dac2f2e8d8544ad8d65fe83463562a5e9fa0e29e49963d96db31640e6`，完整离线库 2876 通过、0 失败、3 项历史忽略。证据分别见[字距](../evidence/capability-upgrade/2026-10-16/publication-tracking-reader-scoped/manifest.json)、[字面公式](../evidence/capability-upgrade/2026-10-16/math-literal-character-editing-scoped/manifest.json)、[TeX](../evidence/capability-upgrade/2026-10-16/publication-tex-geometry-continuation-scoped/manifest.json)。之后修改的候选必须重新绑定，不能直接继承通过。

## 本轮已完成的限定增量

| 增量 | 首次验证与处理 | 退出条件 |
| --- | --- | --- |
| M05 算子模拟与可编辑 TeX | 11 项库回归通过；8 组 Windows 原生 API／PDF／MCP 通过，4 份矢量 PDF 和 11 次 stdio 工具调用；已逐页看过 4 张实际渲染图 | 只认证限定 Bin／Rel 与源槽、失败保护，不认证整个编辑器 UI、全部 Office 语义或 Word 往返 |
| P13 电子书导航 | 8 组 Windows 原生流程、5 份实际 EPUB 与 EPUBCheck 全通过；目录／地标／参考页、跨章实际跳转、撤销、BOM/CRLF 保存重启、失败保旧目标；两种尺寸明暗截图已复核 | 仅当前材料与 ReadMD 预览；独立阅读器、长章策略及字体子集另验 |
| P15 常规／粗体字体 | 21 项专项库测试及全库通过；8 组 Windows 原生流程、5 份真实 PDF 的原字体／实际 ToUnicode／轮廓／文字／像素证明通过，五页已视觉复核 | 限定 WinAnsi 常规／粗体；复杂脚本、其它字体角色、无字体包原生及 Word 仍单列 |

导航的 3 条错误文案已正确写入 46 种语言文件，总键数 2747；这是资源完整性检查，不是 46 语言人工语义审校。首轮曾误把 locale→key 数据直接交给要求 key→locale 的脚本，未落入新键；现已转置并逐语言断言实际键存在。

本轮集成首跑为 **2923 通过、7 失败、3 项历史忽略**，没有新增忽略项。7 项中 3 项涉及导航 schema、YAML 原始范围及标题事务断言；4 项旧 PDF 排版断言仍绑定原 Helvetica，而当前正文使用内置字体。后者按原始 DejaVu 字体独立度量补充逐字、行宽与边界断言，不扩大容差或删去原文字验证。该首跑不是候选通过证明，后续结果单独记录。

修复后完整离线复测 **2932 通过、0 失败、3 项历史忽略**，146.42 秒；前端离线打包及桌面编译成功。新候选 EXE `fc90a94f15d1eb0ea4c6f942d091af10583a0a96daeac02ab244e10449c892b8`，源码冻结包含 647 项，复用 572 份未改变的源码副本，只新增约 12 MB。M05 原生证据见[限定算子归档](../evidence/capability-upgrade/2026-10-17/math-operator-emulator-scoped/manifest.json)与[实际页面复核](../evidence/capability-upgrade/2026-10-17/math-operator-emulator-scoped/visual-review.json)。原生运行与另一独立 profile 的导航 QA 有短暂重叠，只作为功能证据，不作性能认证。

P15 见[字体归档](../evidence/capability-upgrade/2026-10-17/publication-owned-font-scoped/manifest.json)与[五页复核](../evidence/capability-upgrade/2026-10-17/publication-owned-font-scoped/visual-review.json)。首轮停在检查器对缺少 `name` 表的字体子集抛出的 `KeyError`；没有跳过字形检查，改为严格绑定 PDF 资源、descriptor、FontFile2、CID 和实际轮廓。15 项正反例与原 16 项回归控制通过后，完整五文件复测通过。首败和同一 Paper 字节仍保留；缺少名称表不能单独判定 PDF 字体无效，必需表以 [Adobe PDF Reference §5.8](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/pdfreference1.4.pdf#page=387) 为依据。本次不认证 PDF/A 或全部字体嵌入规范。

### 电子书语言小修

真实预览发现：未操作语言选择框时，首项 `zh-CN` 被收集为显式覆盖，替换了正文的 `language: en-US`。已增加复用现有 46 语言 `pm.inherit` 的“继承”项；缺省预设复位选择框，继承时不写 EPUB 覆盖值，保留已存的地区标签，并移除旧导出入口硬填中文的逻辑。共同出版元信息保持原有优先级。

当前 11 项前端行为测试通过；原始源码在相同测试中 2 通过／9 失败。覆盖两个导出入口、实际预览请求构造、显式 en／zh-CN／en-US、切换旧预设与元信息保留。该 Node 测试使用实际产品函数，但不替代原生 UI 或 EPUB 字节验证。

小修已重新离线打包及构建。新前端 boot 为 `d19d426c231f2598c2f15ed8f114460386bf4a541223d062dd5d7a90787c7ef3`；EXE 不变。272 份 Rust 源逐一确认字节未变，复用上文 2932／0／3 的真实后端测试结果，未重复运行或冒称重跑。源码索引复用 645／647 项，只增加约 1.54 MB。资产、接线、无 Python 产品依赖及 46 语言键检查通过。

[原生语言验证](../evidence/capability-upgrade/2026-10-17/epub-language-inherit-scoped/manifest.json) `epub-language-inherit-native-MXArHu` **7 组、4 份真实 EPUB 全部通过**：默认继承 en-US、显式 en、显式 zh-CN、切回缺省预设恢复 en-US；真实可见控件→实际预览请求→实际导出 API，OPF、全部 XHTML 的 lang／xml:lang 和英／中文导航分别断言，四文件 EPUBCheck 通过。没有调用系统保存对话框，不据此认证保存按钮、重启持久化或全部出版语言。原图是折回高级区的整体 QA 面板，部分有打开文件提示，不作为语言控件视觉证据或宣传素材。

P13 完整限定证据见[电子书导航归档](../evidence/capability-upgrade/2026-10-17/publication-navigation-scoped/manifest.json)与[两帧复核](../evidence/capability-upgrade/2026-10-17/publication-navigation-scoped/visual-review.json)。五次原失败保持不变：相同 href 的不同语义链接被误当唯一链接、srcdoc 生命周期观察、真实默认语言缺陷、测试未展开高级区，以及短材料没有滚动距离。最后一项用原 HTML 独立量得正文约390px／视口517px／最大滚动0，补为16段可读正文后真实跨章移动通过，未降低滚动断言。

**新发现、未修的 P12 参数继承问题：** 缺省预设没有 EPUB 专属字号／行距时，空数字控件会收集为最小值8pt／1.2，而非正文 typography（minimal 为10.5pt／1.55）。margin 的空控件也会物化0，后端又将0回退为5／8；清除与显式0语义需要统一。这不影响本轮语言和导航的限定通过，但阻止宣称整个预设体验完成。下一轮优先修复并验证默认、显式、清除、切预设、保存重启及真实排版。

## 第三批仍须完成

1. **公式语义与编辑**：M05 剩余属性组合、完整 mathPr／UnicodeMath／MathML、未知扩展的身份与摘要保护、完整重复嵌套源码回写、输入法／触控／读屏及真实 Word 可编辑往返。
2. **完整出版参数**：基线偏移、伸缩、边框底纹、制表位；按章多级编号、每节／每页注释重启、续注／子图题注；完整 CSL 字重及可编辑引用回导、跨页命中。
3. **电子书与版式**：先修缺省字号／行距与零边距的继承；再补全预设材料矩阵、字体子集、长章策略、表格跨行跨列／小数点对齐、图表固定输出资源。导航已限定通过，仍需 Thorium／Calibre 的完整独立操作证据。
4. **字体与输出保真**：复杂脚本整形、RTL／混排、完整字体角色、缺字源定位、tagged PDF；本轮普通 WinAnsi 常规／粗体不能替代这些范围。
5. **互操作与交付**：Word／WPS／LibreOffice 真正编辑重存、Word 修订回流；VS Code Extension Host；最终候选 MCP 全链。当前 Word 启动失败记录保留，不能用 XML 合法替代。
6. **资源、压力与平台**：正式 TeX 包许可／源码闭包、冷态 120 秒内至少三次独立验证；磁盘满／锁／取消／崩溃恢复及安装峰值；八小时混合压力。仅有 Windows 原生环境，其他平台保持未验。

## 第四批路径

第三批下一轮按以下顺序继续，不重新开发已经有对应证据的行距、字距、字面字符、间距、限定算子和 WinAnsi 常规／粗体：

| 顺序 | 具体余项 | 完成判据 |
| --- | --- | --- |
| 1 | P12／P02：电子书缺省数字参数继承及语言持久化续验 | 空值继承、显式0与清除分开；默认／覆盖／切预设／重启；实际 CSS、版面、OPF及导航语言一致 |
| 2 | I06／I07：同一本修后 EPUB 的独立阅读器验证 | Thorium 与 Calibre 实际导航、跨章回跳、注释、公式、主题／字号重排、关闭重开；源书不变 |
| 3 | M13 → M12：未知扩展保真、重复嵌套公式回写 | 身份＋摘要绑定；邻近符号修改保留扩展，失配进入恢复；第二处独立修改、撤销、保存重启及过期拒绝 |
| 4 | P04：基线偏移一个完整出版参数 | 实际字形位移、完整文字与相邻内容不变；继承／覆盖／清除、非法值、预设重启；逐输出引擎声明能力 |
| 并行准备 | D04 → D05 → D06：正式 TeX 包闭包与冷态验证 | 八份精确版本 source/doc 和许可关联完成后，走正式包安装全周期，再做三次独立冷态各 ≤120 秒 |

Word 的 I01／I02／M15 及 VS Code 的 I09 保留各自独立应用门槛；未取得环境证据时不阻止上述实现。最后在选定候选上执行 I08／I10 完整链，不把单项 MCP 通过当作全部互操作通过。

先完成 **持久逐块审阅 → 固定导出快照 → Rust 多轮任务** 三个公共环节，再并行接表格协作、仓库工具、PDF／OCR，随后媒体、图片绘画、教学与长篇出版。第一条验收流程固定为：真实 AI 修改两个块 → 手改第三处 → 接受一块并保存 → 重启 → 只撤回另一块，保留手改。

复用现有 Yrs／Yjs 与 SQLite；参考 Continue 的工具适配层次，PDFium、FFmpeg、whisper.cpp 走受管理离线能力包。许可证与锁定边界见[第四批执行规格](../design/v005-batch4-execution-spec.md)、[开源路径研究](../design/v005-batch4-path-research-2026-10-15.md)及[首条流程规格](../design/v005-batch4-first-workflow-spec.md)。尚未实施第四批，不新增场景工作台。
