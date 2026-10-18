# ReadMD 公式体验与能力规划验收

本次不调整版本号。产品方向与八类场景详见[能力规划](../design/readmd-experience-and-capabilities.md)；本页只记录本次实际实现和验证范围。没有把规划中的协作、原位 PDF、数位板或全量 OMML 认证记作现成功能。

## 已实现的用户流程

编辑正文时使用原有公式入口，或双击实时预览中的公式，打开该公式草稿。可选择可视结构预设、常用函数或搜索符号；点击渲染后的符号后直接键入替换内容，也可在目录中选择替换符号。源码默认折叠。确认后一次写入正文，Ctrl+Z 撤销整个公式改动；关闭或 Esc 不写入。

相同公式按源位置区分，行内代码与围栏示例不参与替换。保留原有公式分隔符和范围外内容；正文、标签或编辑器实例变化时，旧草稿拒绝覆盖。空公式、未知命令、无效语法和超出交互预算的草稿不能确认。源码编辑仍可在正文中进行。

目录是离线生成并签明来源的共享数据：19 个结构、27 个函数、2,432 个数学字符、446 个 TeX 别名。前端与 Rust 使用同一目录。模板显示渲染缩略图；源码可通过悬停或展开查看。没有新增常驻工具栏按钮，也没有新增 npm/crate 库依赖。

复用了现有 MathJax 3.2.2，补齐相同版本的 HTML 和 Safe 本地组件及许可证。修复冷启动时重复渲染请求丢失、SVG 字形空心区域不可点、普通金额被当作公式、窄窗口顶栏横向溢出，以及 `\cdot` 在预览与导出间字符不一致的问题。MathJax 操作串行执行，过期异步渲染不会写入其他草稿。

## 代码边界

| 层 | 文件 | 职责 |
| --- | --- | --- |
| 公式草稿 | `assets/js/editor/formula-studio.js` | 有界源范围扫描、符号映射、渲染点选、目录、确认事务与过期保护 |
| 阅读渲染 | `assets/js/reader/formula.js` | 本地 MathJax 配置、安全过滤、冷启动等待、渲染队列与金额保护 |
| 共享目录 | `assets/math-catalog.json` | 字符、别名、结构/函数预设及来源版本 |
| 目录生成 | `tools/build-math-catalog.mjs` | 离线重建与 `--check`；固定 Unicode 16.0，不随新运行时默默改目录 |
| Rust 数学 | `math_catalog.rs`、`latex2omml.rs`、`omml.rs` | 扩展别名、原生 OMML 写出与数学乘点分类 |
| 面板/文案 | `assets/index.html`、`assets/css/panels.css`、`assets/i18n/` | 现有公式面板、主题与响应式、动态文本和无障碍名称 |
| 标准差距 | `docs/design/office-math-coverage.json` | 官方固定提交的 133 元素、12 枚举、19 数学对象；不以源码出现充当兼容认证 |

另修正了两处测试可靠性问题：随机密文不能以短 `sk-` 子串断言未泄漏完整测试密钥；修改 `READMD_DATA_DIR` 的测试与依赖默认目录的测试共享互斥守卫并恢复环境。生产加密逻辑没有改变。

## 验证范围

机器为 Windows，Intel i9-13900HX、RTX 4060 Laptop GPU。原生测试使用离线构建的 Rust release 可执行文件、真实 WebView2、应用 CSP 和隔离配置；资料为专用原创统计研究笔记，未修改用户书籍或生产配置。

| 检查 | 结果与范围 |
| --- | --- |
| Rust workspace | 2,027 通过、3 忽略、0 失败；包含数学深层嵌套及 8,000 层分式检查 |
| 目录一致性 | 2,432 个字符保留；全部 446 别名逐项核对输出字符，不能用视觉相似替代字符身份 |
| 相关 UI 回归 | 53 通过、0 重试、0 跳过；公式、阅读、编辑、保存恢复、Shell 和复选框 |
| 公式专项 | 10 用例，包括全部 46 个预设、真实指针点选、单次撤销、重复公式、代码排除、取消、过期保护、安全输入和资源预算 |
| 原生交互 | 真正点击 SVG 字形；确认前原文件不变；撤销/重做、显式保存且无相邻备份 |
| 原生导出 | 实际调用 Rust 保存与 DOCX/PDF/EPUB 导出，检查 DOCX 原生数学对象、PDF 页面与正文、EPUB MathML 内容；具体最终结果见证据 JSON |
| 布局与语言 | 原生 1024×680、1160×820，简中/繁中/英/日；公式专项另覆盖 390px 浏览器布局，检查确认按钮与横向溢出 |
| i18n | 46 语言文件、2,114 键完整；本轮自然翻译覆盖四种主语言，其他语言按既定回退政策处理 |
| 静态门禁 | i18n、wiring、styles、assets、no-python、boot 同步、数学目录重现、功能清单与网站校验 |

完整证据汇总见 [formula-experience-2026-10-07.json](evidence/formula-experience-2026-10-07.json)。53 项是本次相关界面回归，不能表述成整个产品每个业务分支都已重跑。没有本次 macOS、Linux、鸿蒙真机认证，也没有任意公式的 Word 原生往返认证。

## 增量演示与清单

纯软件原生 4K 演示位于 `showcase/updates/v0.0.5/formula-workspace/formula-visual-4K.mp4`，约 11 秒，展示总体方差草稿改为样本方差、确认、撤销和重做。资料原创，未加速；逐采集画面扫描可见文本，另做媒体抽帧 OCR 隐私检查。源码、素材和视频保留在现有忽略目录中，没有上传或发布。

现有 F032 和相关静态/动态控件清单已更新；目录统计为 46 预设与 2,432 字符，不能把 2,478 条目录数据称为 2,478 个产品功能。原 104 条基础演示保留为历史画面，新公式流程有独立增量审核与视频。

录制中间帧已由录制器逐文件清理；最终视频、海报、字幕与隐私报告保留。后续删除 `.cache/math-ux` 临时 WebView/浏览器配置和旧测试可执行副本的命令被自动审批以 `blocked by policy` 拒绝，未换路径或换工具重试；这些配置仍保留。离线构建缓存也保留以供复用。

## 尚未完成的目标

当前支持“从预览打开对应公式、在草稿的渲染结果中点选替换”，尚未实现预览内原位结构光标、Tab 参数槽、矩阵行列编辑、19 类对象的全部属性及未知对象无损往返。完整 OMML 必须按官方清单和 Word 创建的真实样例逐项闭环后才能宣称完成。收录字符也不代表每种平台字体具备全部字形。

下一阶段应先实现 Rust `MathDocument` 源映射树与结构补丁，再增加槽位/结构编辑及 Word 往返样例；之后统一论文版式、文献交叉引用和出版预检。实时协作、PDF 原位编辑、媒体精确选段与数位板按规划中各自的基础设施和验收门槛推进，默认界面保持现有阅读页和一行编辑工具栏。

## 重现入口

所有 Rust 构建均使用 `--offline`，不安装依赖。

```powershell
node tools/build-math-catalog.mjs --check
node --test tools/test/formula-source.test.mjs tools/test/ui-inventory-identity.test.mjs
cargo test --offline --manifest-path rust/Cargo.toml --workspace
cargo run --offline --manifest-path rust/Cargo.toml -p xtask -- bundle-boot --check
```

Playwright 使用项目现有环境，运行 `formula-studio.spec.js`、`reader-upgrade.spec.js`、`editor-upgrade.spec.js`、`document-lifecycle.spec.js`、`shell-polish.spec.js`、`checkbox-consistency.spec.js` 的 desktop 项目，关闭重试。原生脚本为 `ui-tests/formula-native.cjs`；需要显式设置 `READMD_BIN` 和已有 QA 运行时 `READMD_QA_PYTHON`，避免碰到用户安装配置。录制开关 `READMD_RECORD_FORMULA=1` 只在本地素材目录存在时使用。
