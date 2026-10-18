# B3-55：上下限与矩阵、方程组行距

第三批仍在实施。本增量修复明确属性未参与绘制的问题，不关闭全 OMML、出版、资源或互操作验收。

## 语义和实现

`limLoc` 明确指定上下位置时，行内公式和脚本也遵守该指定；省略时，普通大算子在块级使用上下限、行内使用侧标，积分族默认使用侧标。MathML 的 `movablelimits` 区分自动位置和明确位置，Rust 矢量排版使用同一判断。文档级 `mathPr/intLim/naryLim` 尚未接通，不计为已完成。

LaTeX 转换器传递实际块级／行内及子槽样式，支持 `\limits`、`\nolimits`；求和、乘积、积分、分式、上标、根指数和矩阵内算子的预期分别检查。旧递归参考实现仍用于核对迭代解析器；生产解析的样式差异另有明确预期，不能把参考实现当作数学标准。五个行内求和快照和原先将 `\prod\limits_{i}` 当普通文字的快照随行为修正更新。结构包裹回归仅允许上下限位置不同，其余结构必须相同。

真实属性面板还发现回写缺陷：新建属性没有携带命名空间映射，导致高级矩阵 DSL 不能重新编译；简单矩阵的快捷属性回写也可能追加不支持的参数。Rust 补齐新属性的数学命名空间，矩阵改走可保留其他源码的局部结构编译。新增公开服务回归分别覆盖简单矩阵和已有高级属性的矩阵，保留外层注释及相邻矩阵。

矩阵与方程组共用行距规则：常规、一点五倍、两倍、整点绝对长度和半行倍数；绝对行距与列间距的 twip 单位分开。实际 Rust 绘制、可交互 SVG 和 EPUB MathML 共用转换规则。方程组的顶部／底部基线属性接入实际排版。MathML 导入保留可表示且统一的行距；不相等、负值、超范围及无法用 OMML 精度表达的值继续报告投影损失。

依据：[Microsoft limLoc](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.limitlocation?view=openxml-3.0.1)、[行距](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.rowspacing?view=openxml-3.0.1)、[行距规则](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.rowspacingrule?view=openxml-3.0.1)和 [W3C MathML 表格间距](https://www.w3.org/TR/MathML3/chapter3.html#presm.mtable.attrs)。规范参数已进入实际绘制；与本机 Word 的逐属性视觉比较尚未完成，尤其不能把排版内核的一致性当作 Office 应用等价认证。

## 验证记录

[不可变归档](../evidence/capability-upgrade/2026-10-12/batch3-math-row-limits-manifest.json)：55 文件、3,416,255 字节，清单 SHA-256 为 `93f3a47b27e575fa6960663fce8c29d9a8106ecc2140d788eee584db5a01cc2a`，Rust 源摘要为 `455bbb9098604ee9c839c7114c0ab298f228ac44fc4c71a71daf999a5c8d6e31`。最终二进制为 `1db3717c0347b920ab4d8327204f94b7edc9b3f0a4b5fa03a2859596f44c8b85`；前端 boot 未改，仍为 `8814885d57ed04d3db5224d8ae5a1feedfceb8f49ddd0b5cc2602db43420f053`。

- 最终完整内核 **2221 通过、0 失败、3 项既有忽略**，146.26 秒；数学专项 112 项通过，锁定离线构建完成。
- 正式 CSP、真实 Windows 原生六条流程通过：行内上下限实际字形差异、六种 LaTeX 服务请求、矩阵／方程组实际行距、高级属性修改及一次撤销重做、原子保存重开、真实 DOCX／EPUB／PDF 输出。七组实际绘制坐标记录保留，12pt 行距增加 16 CSS 像素，保存后 24pt 增加 32 像素。
- 相同最终二进制的前序括号／算子四组原生几何回归通过，严格 CSP、已安装 Edge、零重试的 DSL 八项通过。截图抽查包含 1160×820 亮色、1024×680 暗色和就地编辑。
- 独立检查确认八个可编辑 DOCX 公式、八个 EPUB MathML、可搜索标题的一页 PDF、24 个矢量绘制对象且没有栅格图。它不替代真实 Word 重编辑或 EPUB 阅读器。
- 资源、接线及 46 语言／2588 键集合检查通过，未新增前端文案；不认证所有翻译语义、辅助技术或长期性能。

此前首次完整内核为 2219 通过、0 失败、3 项既有忽略，早于 LaTeX 样式修正；随后测试编译的私有函数错误、2217／3／3 的旧预期失败、2220／0／3 的属性修复前构建分别保留。初次原生测试按选择层 DOM 顺序推断行序而失败，改按真实字形坐标排序后，又暴露生产属性回写缺陷。修复命名空间及矩阵回写后重新完成上述原生流程和全部回归；未跳过或重试取得通过。

初次证据复制遇到 PDF 库的弃用警告混入 JSON，未生成验收清单；保留该次未封存目录。检查脚本改用正式模块名后，在新的 `batch3-math-row-limits-verified/` 目录生成最终清单，未覆盖历史归档。

## 剩余

继续文档默认参数的导入、可读 DSL 保真与 DOCX 设置输出；完整标准对象及关联 Word 内容、旧 LaTeX 保真阅读、任意嵌套和辅助技术；完整出版参数、实际 Word 重编辑和 EPUB 阅读器。B3-51–54 的生命周期故障、正式 TeX 材料与许可、冷态预算继续保留。没有替换用户安装、提交或发布版本。
