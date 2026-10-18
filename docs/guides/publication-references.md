# 稳定编号、引用与注释

图、表和公式的标签写进 Markdown，编号由当前正文顺序计算。移动对象后引用保持同一 ID，阅读、编辑预览和重新导出自动使用新编号，不把计算结果写回文件。无需打开新面板。

````markdown
---
title: 证据与推理
lang: zh-CN
---

参见 :ref[eq:mean]、:ref[fig:route] 和 :ref[tbl:results]。

```readmd-math v1
frac(sum(x_i, from: i = 1, to: n), n)
```

:label[eq:mean] 样本均值

![研究路线](readmd-assets/route.svg)

:label[fig:route] 从观察到检验

| 样本 | 均值 |
|---|---:|
| A | 0.84 |
| B | 0.76 |

:label[tbl:results] 复核记录
````

- 标签必须单独成段，紧跟对应块公式、图片或表格。`eq:`、`fig:`、`tbl:` 分别独立编号。行内公式不占块公式序号。
- ID 为 1–128 个 Unicode 字母、数字或 `_ : . -` 字符；同一文档须唯一，也不能与显式标题 ID 冲突。
- 编辑时输入 `:ref[` 可补全当前未保存正文的有效标签；Tab 接受，已有 `]` 不会重复插入，撤销按一次修改处理。
- 标签标题可使用强调、链接等行内格式。显示名称使用文档 `lang`，不能以界面语言改写作者正文。
- 缺失、重复、孤立或类型不符时保留标记并提示，禁止创建歧义目标。代码、公式源码和转义示例保持原样。

Word 输出原生书签和可更新的 `SEQ`/`REF` 字段；更新字段可刷新号码。PDF 包含实际内部目的地；EPUB 引用定位到正确章节；TeX 保留精确标签。Word 字段的包内容已验证，实际 Word 导入、修改、重开仍待原生验收；TeX 显示号码来自本次 Markdown 投影，修改 `.tex` 后的自动重编号尚未认证。

## 文献和脚注

```markdown
---
bibliography: references.bib
csl: chicago-notes-bibliography
lang: en-US
---

作者说明[^author]。引用 [@kant, p. 42]，再次引用 [@kant, p. 43]。

[^author]: 保留原始说明。
```

`bibliography` 可使用 BibTeX 或 CSL JSON；`csl` 可用内置名称或官方样式 ID，也可指定当前资料夹内的 CSL 文件。注释型样式生成真实脚注，保留作者已有脚注与定位符；正文仍保存引用键，样式投影不写进源文件。缺项和暂不能准确表达的格式应查看导出警告，当前不能宣称全部 CSL 版式无损。

## SVG 图片输出

Windows 的静态、自包含 SVG 在 Word/PDF 导出时由独立、锁定的离线 Rust 渲染器生成最长边不超过 2400px 的等比 PNG，保留透明度；原图不改写，输入目录不产生转换副本。EPUB 保留 SVG。此兼容输出不是 Word/PDF 原生矢量 SVG；公式 PDF 仍使用独立矢量数学输出。

外部文件/网络资源、脚本、动画及嵌入 HTML 不支持；失败走现有图片警告。可使用内嵌 PNG/JPEG/GIF/WebP 或 SVG 内部片段引用。引擎构建和许可证见 [SVG 引擎说明](../design/svg-renderer.md)。
