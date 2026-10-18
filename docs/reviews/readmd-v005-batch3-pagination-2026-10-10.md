# 第三批：真实分页与页面内容交互

这是 V0.0.5 开发增量，主程序尚未发布，也未覆盖用户已安装版本。完整第三批及 R069 继续进行中；实际 Word 往返、全部出版属性和非 Windows 原生证据仍待验收。

## 本次行为

在已有编辑预览菜单选择“生成文档页面”，即可按当前出版设置显示真实 Typst 分页。默认保留轻量 Markdown 预览；没有增加常驻工作台。缺少 Typst 时，原入口显示用途、下载量、安装空间和许可证，安装成功后自动继续；缺少页面栅格引擎时也复用同一能力安装服务。

页面上的段落、标题、块公式和题注通过实际排版几何定位到原始源码。点击后打开既有局部属性表单，应用为一次可撤销事务；重复文字按节点和 UTF-16 范围区分。公式及引用语法保留，保存前不写磁盘。翻页、键盘操作、连续编辑、取消、过期节点保护和重启后再次编辑均使用同一 Rust 修订。

源码只在后端临时副本中插入随机作用域标记，排版时形成临时 PDF 链接。Rust 提取实际页、栏和行矩形后，清除这些链接及相关 OBJR/ParentTree 引用，保留语义内容和原有链接。真实链接覆盖的字形通过标签父链恢复定位，不以文字搜索猜测。临时标记不进入 Markdown，也不成为最终 PDF 的动作。

预览与同输入导出共用同一缓存产物。分页模式改用实际页面栅格并最多缓存三张图片；编辑或关闭预览取消旧任务。修改出版参数会刷新页面。Word 仍明确属于共享版式投影，不等于 Word 原生分页。

## 证据范围

固定构建、源码和证据摘要见[索引](../evidence/capability-upgrade/2026-10-10/batch3-publication-pagination-manifest.json)。全部材料为专用合成文档，使用独立配置、正式 CSP 和真实文件接口；未读取私人文档或替换现有安装。

- [Windows 原生十条流程](../evidence/capability-upgrade/2026-10-10/batch3-publication-pagination-native.json)：缺包取消及从自有仓库真实下载安装后继续；第二个重复段落、公式和题注精确点击；单次撤销；翻页和键盘；过期节点拒绝；普通预览恢复；连续打字；页面修改保存、实际重启及再次编辑。
- 独立 PyMuPDF 对五页双栏文档逐页比较：实际像素、正文和用户链接与未插入定位标记的直接导出一致；868 个矩形跨页、跨栏定位；没有临时动作、悬空对象引用或错误指向正文的脚注区域。缓存预览与导出字节相同。这不是 PDF/UA 或所有文档组合认证。
- [完整 Rust 库回归](../evidence/capability-upgrade/2026-10-10/batch3-publication-pagination-rust.json)：2044 通过、0 失败、3 项既有忽略，226.43 秒。忽略项及原因保留，不计为通过。
- [相关界面回归](../evidence/capability-upgrade/2026-10-10/batch3-publication-pagination-ui.json)：46 通过、0 失败/跳过/重试，覆盖公式、单行编辑栏及导出预设。与旧套件有重叠，不累加为独立功能数。
- [静态门禁](../evidence/capability-upgrade/2026-10-10/batch3-publication-pagination-static.json)及[词法边界](../evidence/capability-upgrade/2026-10-10/batch3-publication-pagination-helpers.json)：资源、接线、样式、46 语言 × 2576 键、无运行时 Python 和前端包一致性；三条源码折叠边界通过。没有新增未翻译词条，复用现有完整语言键；这不代替所有语言人工语义审校。

中文 1024×680、德文 1160×820、阿拉伯文 1024×680 的原生页面已做截图及横向溢出检查。

![中文真实分页](../evidence/capability-upgrade/2026-10-10/batch3-publication-pagination-pages-zh-CN.png)

![阿拉伯文布局](../evidence/capability-upgrade/2026-10-10/batch3-publication-pagination-pages-ar.png)

## 问题与未完成项

临时 HTML 标记曾阻断公式题注的关联，现与既有布局、数学元数据一起作为语义桥接处理，并用真实准备链路回归。真实链接的字形曾在临时链接几何中形成空洞，现通过实际标签父链定位，用户链接动作继续保留。

早期独立检查脚本把合法的空闲 PDF 对象槽当作错误；改为检查实际对象及其引用，确认没有悬空引用。一次原生布局测试还错误地重复打开已经自动关闭的菜单；测试改为按真实可见状态操作，未移除布局断言。失败尝试保留在[记录](../evidence/capability-upgrade/2026-10-10/batch3-publication-pagination-attempts.json)。

当前只验收上述顶层内容和测试组合；嵌套块、纯链接块及任意复杂组合还需扩展。实际 Word 分页和往返、全部 Office 语义、任意节/尾注/字体属性、传统 TeX 便携包及沙箱、离线补充字体仍待完成。页面栅格缺包分支已实现，本机 Windows 自带栅格可用，尚未在缺少系统栅格的非 Windows 平台取得原生证据。跨平台、完整安装升级、八小时压力及 4K 增量录制均未由本次替代。

缓存分为隔离原生配置、测试二进制、探查文件和回归日志，见[分类清单](../evidence/capability-upgrade/2026-10-10/batch3-publication-pagination-cache-inventory.json)。测试进程已结束；此前自动审批拒绝的批量清理未重试，用户安装、文件及恢复记录未清理。

排版行为参考 [Typst link](https://typst.app/docs/reference/model/link/) 的外观及断词规则；源码映射是否改变输出由实际像素对比判定。操作说明见[版式指南](../guides/publication-layout.md)。
