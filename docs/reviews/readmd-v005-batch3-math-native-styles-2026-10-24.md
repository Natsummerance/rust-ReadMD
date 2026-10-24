# 第三批公式原生段落样式链：实现与限定实测

2026-10-24。候选 candidate-61，Windows x64，自有 Rust 内核；本次没有使用 computer use，也没有启动 Word、WPS、LibreOffice、阅读器或浏览器界面。

## 对用户实际行为的改进

此前 DOCX 公式只把段落样式的有效缩进写成 ReadMDMath/direct；在其他编辑器修改样式或删除直接覆盖后，原来的继承关系不能完整发挥作用。现在获准的有限水平 LTR 公式保留稀疏 pPr、pStyle 和 basedOn 链，真正引用导出包的原生样式。没有显式选择样式的公式会绑定对应导出样式，不改变整篇默认样式。

- 保留 missing、空容器、显式0、六种单位原词法与每个轴的来源，不由有效值反推声明。
- 模板和不同来源同名异定义的样式使用确定性别名，重写完整 basedOn 引用；输入正逆序取得相同映射。只输出实际使用的链，普通正文、完整 docDefaults 和最终 sectPr 有独立隔离检查。
- 对导出后的当前 DOCX 删除直接 left，或只修改当前 styles 中的 right，重新导入后当前值真实生效；所有其他 ZIP 成员逐字节保持，非目标有效布局不变。
- 非空 pPrDefault、未获准的段落 mark-rPr 等继续明确物化兼容输出。机器警告 math_paragraph_style_materialized 已精确登记，并接入现有导出警告区的46语言文案。不能把未知后缀或路径当作已登记代码。
- 悬空、循环、属性、作用域、冲突和数量/体积错误在替换目标前拒绝。处理上限为4096来源、4096定义、128层和16MiB描述，避免无限扩张。

## 实际验证

| 验证 | 结果与范围 |
| --- | --- |
| Rust 完整无过滤回归 | 3672通过、0失败、3项既有忽略；较C58增加24项有意义的 Rust 测试。锁定离线编译。 |
| 前端与工具完整回归 | 1097通过、0失败、0跳过。 |
| 六门槛 | assets、styles、wiring、i18n、no-python、验收账本全部通过。 |
| 独立检查器反例控制 | Node18/18、Python18/18、plan exit0；包含错误颜色、全部默认值隔离和非目标轴变更的拒绝反例。 |
| 原生样式真实 stdio | 12组检查、322次工具调用、326个匹配请求/响应；31份输出，48条几何证据，30份原目标错误后保持。 |
| 保存与重开 | 两组真实转换/迁移/编辑/撤销重做/API保存及新PID磁盘读回；实际退出PID 35016、31884 均正常EOF。源身份、修订、第二重复式定位与CRLF保持。 |
| 同一候选复杂出版回归 | 12组检查、4份输出；七节出版、公式、实际受管理XeLaTeX、取消、旧目标保护、同ID复用和独立ID显式重试。PID 19148、24412 正常EOF，遗留拥有任务为0。 |

31份输出包括9份 DOCX、16份 PDF、3份 HTML、3份 EPUB。PDF分别用 Rust 和 Typst 实际输出，检查文本/矢量/字形/绘制及位置；HTML检查所选渲染器输出和流式声明，不认证浏览器像素布局。EPUB实际读回 MathML，不认证真实阅读器界面。

外部修改对照使用同一未修改 native DOCX 的真实重新导入、API保存及解析作为基准，另有4份真实 PDF 控制文件；SVG 的全部 fill/glyph/primitive 和 PDF 的 paint/curve 断言保留，容差分别为0.00003pt与0.002pt。不能以去色、删断言或扩大容差取得通过。

## 保留的首败与审查修正

1. 第一次完整 Rust 回归3670/1/3的日志保留。旧断言要求把继承轴物化成直接值；修正为独立检查稀疏 pPr 与原生 basedOn 的真实轴，未减弱有效布局和原词法检查。
2. C59测试准备阶段缺少报告 limitations 数组，未启动产品；修正共享测试报告契约并加反例，保留原候选、原日志和退出状态。
3. C60先通过14份输出，随后把原MD自动颜色和已导native模板的 rPrDefault/color=262626 当成同一渲染上下文而首败。产品字形和坐标没有因此修改；编码前补定同包控制规格，新候选纳入31份总量。原→native的默认变化单独记录，originalToNativePaintIdentityCertified=false；不宣称已解决run/theme保真。
4. 独立审查曾发现只比较 pPrDefault 会漏掉 rPrDefault，以及新警告可能进入 unknown/other；本轮已经补齐完整默认树/sectPr检查和精确API代码/前端翻译测试。最终审查原文与原始 wire、OPC、PDF 检查一并归档。

## 当前边界及下一步

此结果只认证有限段落样式链；全M07、P04、I10、全部第三批和发布均未认证。其余事项见[下一执行路径](../design/v005-batch3-after-native-styles-2026-10-24.md)：

- 默认裸LaTeX与导入的显式 dispDef=false 混排仍按现有全局策略明确拒绝，不以匹配设置控制冒充兼容。下一单元从缺省与显式状态及全部消费者的一致策略入手。
- 非空全局段落默认、run/character/theme、完整styles包、全部Word属性、任意宿主及结构化alias/loss公开接口仍开放。内部别名不是公开用户报告。
- 8份 Typst PDF保留字体替代警告；12pt STIX矢量几何通过不能代替 Office/MATH 字体保真。非零普通正文Typst基线仍关闭。
- 真实Word/WPS/LibreOffice与阅读器操作、完整CSL、剩余出版参数、正式可分发TeX材料/离线包、受控冷启动、全故障和八小时压力、46语言语义审校与其他平台原生证据仍未取得。仅Windows本机，不用交叉编译冒充其他平台验收。
- 本轮取消经过真实wire与旧目标保护验证；记录的总耗时包含显式重试，不认证≤200ms取消或全部子进程≤2s停止。

## 来源与资源

源冻结1116项；Rust/assets同输入644项。最终EXE SHA256 `6bb2cabff11f4fe39e4673eb1b1005e3bc0bb06e23113ff6e3efe9d187d711f7`，boot SHA256 `1ef8af7c3d031e9c3bc5c6f6bb854b0a1c66e88ed47ec5a769871a5b0ec7caaa`；toolbar、首绘、4K及现有boot保护均保持。C61仅修订QA对照，复用同输入Rust03编译/完整回归/最终EXE；完整前端及六门槛重新绑定all05，没有为了候选编号重复编译。

[限定证据](../evidence/capability-upgrade/2026-10-24/batch3-m07-native-styles-scoped/manifest.json) SHA256 `d5e89f0d9f10d4d9fb2fd0769da80fb6d614456edcc380204051fabe5ffdc1bd`；[独立读回](../evidence/capability-upgrade/2026-10-24/batch3-m07-native-styles-readback.json)核对2771文件，物理52.15 MiB、展开155.10 MiB。C59/C60/C61各1116项源码可恢复；三个QA候选共享一份最终EXE压缩载体。完整证据逐路径保存，46份语言源码使用钉定C58的可恢复字节增量，其余新源码保存内容载体；重建均核对完整长度/SHA。未复制运行库/profile/构建目录或私人资料，未删除共享缓存。

第一次归档预检因展开体积超过160MiB而明确失败，未创建计划文件或半成品归档；[失败记录及原脚本](../evidence/capability-upgrade/2026-10-24/batch3-m07-native-styles-scoped/evidence/root/archive-preflight-failure-01.json)保留。资源优化采用相对钉定C58的语言源码字节增量，保留全部验收证据及完整重建校验，不调高4096文件／80MiB物理／160MiB展开门槛。

第二次预检查出归档脚本把meta.json索引误计为语言；[首败和四份原脚本](../evidence/capability-upgrade/2026-10-24/batch3-m07-native-styles-scoped/evidence/root/archive-preflight-failure-02.json)保留。修复后用真实钉定C58清单验证47个JSON路径中仅46份是语言，meta保持旧完整格式、伪造meta增量明确拒绝；缓存恢复控制20/20通过，首次17项结果仍保留。此处是证据存储测试，未计入产品前端1097项。

依赖和能力运行库复用，不在线安装、不新增产品依赖。Python仅为已列明的QA独立检查器，不进入产品功能。未提交、发布、安装替换，也未清理共享缓存。
