# V0.0.7 CSS 门禁与画布／表格样式限定验证

2026-11-19。只修改 `assets/css/ink.css`、`reader.css`、`table-geometry.css` 及样式专项测试；不改样式 baseline、主题 token 定义、绘图/表格行为代码、拖拽手柄尺寸或命中声明。不构建 Rust、不生成 boot；index 资源摘要和最终原生 QA 由主任务统一处理。

## 修改与原因

- `ink.css` 的 26 处 hex 色值包括主题 token 的旧 fallback、画布白纸/透明棋盘格和主按钮白字。UI 复用现有主题 token；画布复用 `--export-layout-paper` 保持中性白纸，从其 paper/border token 派生棋盘格，不让主题改变绘画内容。主按钮使用 `--accent-fg`，并为 hover 保留 accent 背景，避免暗色主题文字对比下降。响应断点由 700px 统一至现有 640px 标准。
- `reader.css` 的 Office 原版页背景复用同一白纸 token，保持三个 UI 主题下的页面纸色。
- `table-geometry.css` 移除六处 `!important`，使用明确的阅读文章、HTML 出版文章和 AI 消息宿主选择器覆盖其单元格默认样式。明确作者 `align`/`valign` 属性的优先级保持；包括后加载的 `reader-article.css` 和数字单元格的默认右对齐。

## 实际检查

`node tools/check-styles.mjs` 全绿，`tools/styles-baseline.json` 未改。既有样式规则/主题对比单测 4 项通过，`git diff --check` 与专项测试语法检查通过。

`tools/test/v007-css-surfaces.test.cjs` 使用本机 Chrome channel、隔离 profile 与合成文档，加载真实生产 CSS；`reader-article.css` 和面板样式在 geometry CSS 后加载。三主题分别检查 640、641、700、1024px，共 12 组样本：画布在视口内、纸面恒白、棋盘格存在、`touch-action:none` 保持；三类宿主的左/中/右及上/中/下对齐正确；主按钮 hover 对比至少 4.5；主题/尺寸切换不改绘画草稿。12px 列手柄与 11px 行手柄命中区、pointer 命中及 touch-action 检查通过。

专项首败发生在合成编辑 fixture 排在三个阅读容器后，列手柄位于视口外：`y=1262.34375`，视口高 900，宽 12、高 44、touch-action 为 none、inert 为 false。将该独立测试宿主放入可见视口后，保留全部原尺寸/命中断言并通过；没有为此修改产品 CSS 或行为。

通过记录为 `.cache/v007-css-surfaces-qlzZGu/report.json`，含三个 CSS 摘要与 12 组浏览器计算样本。其 light/dark 截图已查看，画布、棋盘格、控件和正文对比正常。

既有 `tools/test/ink-controller-browser.cjs` 增加与应用一致的 tokens 样式加载后通过，覆盖合成笔压 pointer 事件、键盘绘画、图层、透明度、蒙版、撤销重做、恢复和过期插入保护；记录 `.cache/ink-controller-i4BbP1/report.json`。这不代表物理压感笔、系统 WebView 或最终原生候选已经认证。

可复核命令：

```sh
node tools/check-styles.mjs
node --test tools/test/check-styles.test.mjs
# 有独立离线依赖目录时，可设置 READMD_PLAYWRIGHT 指向其中的 playwright 模块。
node --test tools/test/v007-css-surfaces.test.cjs
node --test tools/test/ink-controller-browser.cjs
```

## 冻结摘要

| 文件 | SHA-256 |
| --- | --- |
| `assets/css/ink.css` | `52c1fc909fcf1ffe49eacdb3e3508721e18e3b36f6b23108ca0ea16772fc65a4` |
| `assets/css/reader.css` | `8edc842011e8425fd0d7d62bfe35712f36d8ad0e34594dcbaec3a024dfc1ed7f` |
| `assets/css/table-geometry.css` | `4bafe13c5e4bc8c3b042a0dc5fe731eae01c98bfbe4b637b8e17480a3ff1f5a7` |
