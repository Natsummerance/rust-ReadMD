# P03 正文分节执行合同

2026-10-15：已落共享模型、AST 和 DOCX 实现及专属测试；本文件不表示编译、原生运行或真实 Word 验收通过。candidate15 与 2026-10-14 档案保持原件。

在现有 Markdown 编辑入口使用独占一行的声明，不增加常驻工作台：

```markdown
<!-- readmd-section:v1 {"id":"main","break":"nextPage","page":{"size":"A4","orientation":"portrait","columns":2,"columnGap":8},"numbering":{"format":"decimal","start":1},"header":{"linkPrevious":false,"text":"正文"}} -->

# 正文
```

声明开始其后的顶层正文，直至下一声明或文末。`id` 是作者指定的稳定身份：以 ASCII 字母开始、后续字母/数字/`_`/`-`、最多 64 字节，全篇唯一；改变正文或移动声明不重新生成身份。禁止在段落、列表、引用、脚注、表格或局部样式容器内部创建节。围栏/缩进代码和代码跨度中的相同文字属于普通代码。

`break` 为 `continuous/nextPage/oddPage/evenPage`，缺省 `nextPage`。连续分节允许换栏，但不得更改有效纸张尺寸、方向、四边距或装订边；冲突直接失败，不悄悄换成分页。奇偶页由输出引擎计算真实物理页面，不能仅以显示页码判断。

声明严格拒绝未知键、重复键、非法类型或枚举。单声明最多 8192 UTF-8 字节，最多 128 个显式节；有分节的文档采用既有 32 MiB 源预算及 32768 AST 节点预算。无声明文档继续原输出分支。

| 字段 | 合同 |
| --- | --- |
| `page` | 复用 A3/A4/A5/B5/Letter/Legal/Custom；Custom 必须同时显式给出 width/height（80–600 mm）。orientation 为 portrait/landscape；四边距 0–60 mm，gutter 0–40 mm，columns 1–3，columnGap 1–30 mm；differentFirstPage 为布尔。可用栏宽和正文高度至少 20 mm，不进行静默 clamp。 |
| `numbering` | format 为现有 decimal/lowerRoman/upperRoman/lowerLetter/upperLetter；start 为整数 1–9999，仅本节重置。首节缺省继承全局起点，其后缺省连续；不能从累计 style.footer.start 推断后节重置。enabled/showTotal 可选；改变 footer 字段内容时生成独立 footer，显式 linkPrevious:true 冲突则失败。 |
| `header/footer` | text/evenText/firstText 为最多 500 字符、无控制字符的文字；align 为 left/center/right；distance 为 0–50 mm。text/align 声明须显式 linkPrevious:false。false 且未给文字产生真正独立空白 part；省缺或 true 在后节继承前节。evenText 需全局 differentOddEven，firstText 需本节 differentFirstPage。 |

首声明之前已有正文则保留继承全局的隐式 `_document` 节；文首声明直接配置首节。相邻声明和末尾声明产生真实空节，不合并。脚注定义保留在原 Document 中，输出器应先从完整文档收集定义。

`publication_sections::resolve(&Document, &Value)` 返回 `ResolvedSection { id, break_kind, style, block_range, page_start, header_link_previous, footer_link_previous, explicit }`。`block_range` 引用原顶层 blocks；容器不展平、正文不复制重排。`SectionStart(Section)` 不含可见文本；render JSON 为 `{"type":"section","section":<typed declaration>}`，无 `text` 字段。`analyze` 返回原文 byte range 与 UTF-16 from/to（包含 BOM），`strip_for_view` 仅用于视图投影并保留换行，不是原文写回接口。

DOCX 在每节末尾的段落 pPr 保存该节 sectPr；末节 sectPr 是 body 最后子项。sectPr 中的 type 描述该节相对前节如何开始，不将下一节 type 错写到前节。真实 pgSz/pgMar/cols/pgNumType、PAGE/NUMPAGES 域和 header/footer relationships 按节输出。省 reference 实现原生 Link to Previous，独立空白节不能借用旧页眉。稳定 bookmark 名由 section id 产生，零宽范围标记不进入正文复制。表格和图片按当前节栏宽重新约束。

自动封面/目录在显式分节文档中归首个输出节；未声明文档仍使用原先“单栏封面目录→全局正文栏数”逻辑，不添加新 bookmark 或节属性。所有专属错误必须在现有原子导出提交之前处理，不能覆盖旧产物：`publication_section_invalid`、`publication_section_budget`、`publication_section_geometry`、`publication_section_continuous_page`。

实现所有权：本单元仅 publication_sections.rs、publication_sections_tests.rs、md_ast.rs、docx_writer.rs、固定小夹具；root 负责 lib 注册、publication/mdexport 入口、其他 writer、路由、46 语言资源与正式候选。publication_rich 与 bibliography item 分支不改。

专属测试覆盖固定“罗马前置→双栏正文→横向宽表→奇数页附录”、四种 break、相同布局不同边界、空节/末节/表格结束、独立/继承/空白页眉、编号重置和连续、原生域和 OMML/脚注/链接、当前栏宽表格网格、BOM/CRLF/中文/astral UTF-16、稳定身份、严格无效输入及无声明旧布局。root 统一编译后再计实际结果；API 旧目标不变、PDF 几何/页码/奇偶空页、保存重开及真实 Word 视觉/编辑仍须各自证据。此前 Word COM 激活超时不因本实现而视为通过。
