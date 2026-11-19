# P1 Mac 专用 QA 宿主与原生限定回归

日期：2026-11-05。生产者为已推送的 [`83b4ccf7`](https://github.com/Natsummerance/rust-ReadMD/commit/83b4ccf7556e19f3d2e1f5075072fa5ff6dc3f31)。本记录只描述专用 QA 宿主与既有单栏／侧栏／修复弹窗／保存恢复的限定回归，不发布 V0.0.7，也不提升75条合同的整项状态；原账仍为 **0整项验收**。结构化生产者、摘要与边界见[同名 JSON](readmd-v007-p1-macos-guarded-quit-native-2026-11-05.json)。

clean83原生构建回执绑定源码树 `b520d8259937e7de4cbf472ad235cdb1992080537b1d0b15cde71255e5a7939f`、精确冻结的编译／签名二进制和既有boot/index，隔离专用QA bundle另通过字节／签名／隐私核验。源83变更限debug/native-qa宿主、测试和CI：只有显式owned-profile opt-in时才暂留一次真实quit，收到匹配、有效、新写入并同步的终态回执后，再放行原退出路径。普通业务退出保护和boot不变；这是测试基础设施收尾，不把先前测试失败包装成新的产品修复。

在该候选上，三次正常 LaunchServices 新进程完成 **67项**真实Cocoa/WKWebView检查（46＋11＋10），正式CSP和鉴权保持，无替身bridge、无模型调用或Node安装；真实服务保存、BOM／CRLF原文、会话flush和后续新进程恢复均核对。关闭后台标签后，再启新进程确认其保持关闭。三实例均正常launcher退出、真实进程消失且无forced cleanup；直接native子进程exit码未被观察到，不伪称该字段为0。相同clean83源树另有Rust QA **11通过／0失败／113 filtered**，以及冻结Wry形状的Node VM **22通过／0失败／0跳过**。VM不代替原生执行，三组计数分别报告，不相加冒充独立覆盖。旧unit01的head08＋4脏文件观察仍保留，主要单测证据改用clean02。

四个普通窗口输入覆盖1024×680／1160×820、light／dark与zh-CN／en／ar／de；顶栏品牌／动作排列、侧栏释放宽度与焦点、44 CSS px目标、More中的原Fix入口、48条固定缺失锚点与可滚动列表、关闭弹窗回焦均通过。保存和点击由WKWebView既有DOM入口触发真实服务，不是物理鼠标／键盘认证。

已逐一检查五帧。四个普通窗口布局可读；**五帧均来自WKSnapshotConfiguration＋AppKit标准控件绘制复合图，不是WindowServer或物理显示器截图**。201帧顶部green控件局部边缘被截断，所以视觉整体验收仍为false，不认证全屏窗口chrome的物理像素；PNG宽3840也不构成真实4K硬件证据。真实NSWindow FullScreen style/event切换、原生green target/action／菜单以及Zen协调的状态断言独立通过，不能与复合图像素结论互换。

保留01动画／焦点稳定、05缺终态文件、06对冻结Wry IPC的不可写拦截的原始失败，均按Root分类记为测试基础设施问题。此前OpenAPI原生EH因长路径触发macOS Unix-domain socket长度限制的首败也保留，缩短owned路径后8项行为通过、4图独立查看；其metadata占屏、冲突toast重叠问题仍开放，视觉整体验收为false。

此前 [`08efcc37` 的CI](https://github.com/Natsummerance/rust-ReadMD/actions/runs/41905858297)为8任务／48 artifact已核验，EH8行为及4图也明确绑定08生产者与其kernel，不挪给83。83的[首轮CI](https://github.com/Natsummerance/rust-ReadMD/actions/runs/41945332544)终态为7成功、1失败：Windows在旧发行包大小／SHA认证处拒绝输入，Rust测试已通过，防降级行为尚未执行。同期V0.0.6已按独立e7证据更换公开包；旧5d固定输入与当前公开URL不再兼容。历史CI下载未保留，不能声称逐字复核其现场内容。后续只更新已独立认证的发行输入，保留全部行为断言、认证拒绝和旧锁记录，再实际重跑；本记录不把输入更新称为通过。

物理F11／Esc、鼠标、native wheel、系统IME、原生文件选择器、真实4K显示器／DPI和全屏WindowServer chrome像素认证仍待验。专用QA为ad-hoc签名、未公证且非正式发行包，未替换日常App；Windows/Linux的原生窗口不能从本Mac结果推出。P1／P2整项和V0.0.7均未发布。
