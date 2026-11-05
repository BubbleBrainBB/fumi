# Fumi on GPUI：原型评估报告

日期 2026-11-05。机器 M4 Max，Xcode 27，Rust 1.95（未改默认工具链）。原型只在这台 Mac 上运行和测量过。

## 结论

- **桌面性能远好于 Fumi 和 NSH**：Release 构建下，每次操作的主线程耗时约 2–3ms，从 10KB 到 1MB 基本不变。
- **这个对比不完全对等**：它是不带应用外壳的裸编辑器，只布局可见内容。Fumi 是完整应用；NSH 也没有外壳，但开文档时耗时随文档线性增长。
- **还不能下结论的三点**：
  1. 真实拼音还没试，需要你手动验证。
  2. 辅助功能要自己做。
  3. 拿不到原生 Liquid Glass。

## 对比（中位数）

| 操作 | Fumi | NSH | GPUI |
|---|---|---|---|
| 打开 50KB | 0.6s | 0.24s | 1.7ms |
| 打开 200KB | 1.8s | 0.9s | 2.8ms |
| 打开 500KB | 未测 | 4.9s | 3.8ms |
| 打开 1MB | 9.5s | 19s | 6.3ms |
| 普通输入 200KB | 6.9ms | 5.1ms | 3.0ms |
| 普通输入 1MB | 26ms | 93ms | 3.0ms |
| 表格源码内输入 200KB | 107ms | 4.9ms | 1.8ms |
| 表格源码内输入 1MB | 685ms | 89ms（有积压） | 3.0ms |
| 回车 50KB | 10ms | 约 31ms | 2.3ms |
| 回车 200KB | 43ms | 约 116ms | 2.4ms |
| 回车 1MB | 143ms | 1.5s 内未完成 | 1.6ms |
| 滚动超 16.7ms 的帧 200KB | 4 | 0 | 0 |
| 滚动超 16.7ms 的帧 1MB | 61 | 2 | 0 |
| 内存 200KB | 约 320MB（启动） | 约 270MB | 约 290MB |
| 内存 1MB | 约 320MB（启动） | 1.2GB | 约 300MB |

GPUI 的其他数据（1MB）：

- 光标下移 2.3ms（p95 8.1ms），右移 2.0ms。
- 退格 3.0ms。
- 按住键以 30 键/秒输入，100 键全部生效，无积压。
- 从进程启动到首帧 405ms，10KB 时是 398ms。其中约 200ms 是窗口和 Metal 初始化，包含运行时编译着色器。
- 内存空闲 12 秒后回落到 46–55MB。启动瞬时峰值约 270–280MB，与文档大小基本无关。

完整数据见 `results/`，用 `python3 -I scripts/summary.py` 汇总。

## 每个数字测的是什么

- **Fumi**：完整应用的主线程时间。
- **NSH**：引擎提交时间，裸编辑器。
- **GPUI（表中数字）**：裸编辑器的主线程忙碌时间，由 CFRunLoop 观察者计时，从发出事件到“画出这次改动”的帧结束，包含事件分发、布局、绘制和 Metal 编码。
- **GPUI“打开”**：在已运行的窗口里加载文档到首帧。
- **GPUI“滚动”**：连续 6 秒、约 1800pt/s，统计绘制时间和主线程连续工作时长超过 16.7ms 的次数。另有快速甩动：每次 400pt、8 秒，1MB 时同样是 0 次；但有 3 次帧间隔超过 25ms（漏垂直同步），最长 26.9ms。
- **没测到的部分**：GPU 执行时间，以及 WindowServer 到屏幕的延迟。
- **测试条件**：显示器 60Hz。其他会话同时在编译，load average 约 24–65，所以数字偏保守。

## 驱动的真实性

事件是真实的 NSEvent，走 NSApp → NSWindow → GPUI view → `NSTextInputClient`（键盘和鼠标），滚轮则直接送到 GPUI view。

- 每一步都校验文本或光标真的变了，并等到画出这次改动的帧才计数。
- 驱动抓到过两次假结果：滚动事件发错了 view（0 帧，`scroll moved: false`）；帧计数被无关的帧放行。两者都已修复，相关数字已作废重跑。
- computer-use 发的系统级按键也验证过：`ABC` 和方向键确实生效（未用于计时）。
- 全量矩阵约 2000 次操作有 1 次失败（500KB 光标上移），**重跑未复现，原因未查明**。那次日志已被重跑覆盖，失败只记录在本报告里；下面的单元测试和保存测试不受影响。
- 字节级一致：
  - 17 个单元测试通过。
  - 28 次“撤销后字节相同”检查全部通过。
  - 5 份文件（包括混合 CRLF、BOM、U+2028、lone CR 的 `tricky.md`）用真实按键修改、撤销，再 Cmd-S 写盘，`cmp` 确认与原文相同。
  - 非 UTF-8 文件拒绝打开，不做猜测。

截图都由我检查过，在 `shots/`：表格和公式折叠与展开、图片、代码、列表、选区、1MB 滚动后、毛玻璃。

## GPUI 版本和依赖

- `gpui` 0.2.2 是 2025-10 的旧版，只有单行输入示例，没有 AccessKit，**已不适合**。
- `gpui-component` 0.9.0（Apache-2.0）依赖 `gpui-pre` =0.3.11。这是第三方（Longbridge 的 huacnlee）发布的 Zed 快照，大约每周一版，本原型也用它。
- 它自带的编辑器是代码编辑器（行高统一：`top = line_height * row`），不能隐藏语法、也不能让标题有不同行高，所以用不上。
- `gpui-pre-macos` 0.3.11 需要比 1.95 更新的 Rust。我在 `vendor/` 改了 3 行，避免下载 1.99。正式项目应该跟最新 stable。
- 用 `runtime_shaders` 特性，省去了 Metal Toolchain 下载。
- 依赖共 420 个包，没有 GPL/AGPL；有 2 个 MPL-2.0，已确认其一是 `cbindgen`，另一个未逐一确认；未用 Zed `editor` 的代码。
- `zorite-editor` 0.11.3（MIT）是现成的 GPUI 实时预览编辑器，共 1.6 万行，单独测过：
  - 每键耗时：10KB 1.9ms，200KB 194ms。
  - 1MB：打开 9.8 秒，90 秒内没跑完输入测试。
  - 正文存为 `String`；加载时会改写部分 `$$` 写法（已用夹具确认），**不是字节级一致**。可以参考，不能直接用。

## CJK 排版（Core Text）

**可用：**

- 中英混排和 PingFang SC 回退，简中字形正确（门、关、直、骨）。
- 罕用字、日文、韩文、emoji、组合字符都能显示。
- 避头尾：折行用 ICU 的 UAX#14，检查夹具未见标点出现在行首。这只是目测，没有穷举。
- 固定行高可实现：28px 的行距正常。

**需要绕过或做不到：**

- 中文行和拉丁行的基线不同，因为 Core Text 按整行字体报 ascent。我加了 5 行修正，效果在截图里已确认。
- `shape_text` 一行只有一个字号，所以行内代码不能比正文小一号。Fumi 的规范要求小一号，这点目前做不到（除非自己分段布局）。
- 没有标点挤压和两端对齐。
- 中文按词移动（Option+方向键）目前按字走，要自己接 CFStringTokenizer。
- 引号“”走的是 SF 的西文字形。Fumi 的默认字体是否也如此，还没有并排对比。

## 辅助功能和窗口效果

**辅助功能**

- `gpui-pre` 已接入 AccessKit（macOS、Windows、Linux 都有），框架是好的。
- 但自定义文本元素要自己提供 role、逐行 TextRun 和选区。
- 原型没做，AX 树里只看得到窗口按钮，**VoiceOver 目前读不到正文**。这与 NSTextView 免费获得是实质差距。

**窗口效果**

- `Blurred` 可用（截图已确认）：`NSVisualEffectView` 并去掉了染色，适合“中性、不染色”的偏好。
- 它是磨砂，**不是真正透视的玻璃**。`gpui-pre` 代码里没有 `NSGlassEffectView`，Liquid Glass 需要自己接原生视图，这条路未验证。

## Windows 和 Linux（只读了源码和 issue，未编译、未运行）

| | Windows | Linux |
|---|---|---|
| 渲染 | DirectX 11（发布构建用 fxc 编译 HLSL） | wgpu |
| 文字 | DirectWrite | cosmic-text |
| 输入法 | IMM32（`WM_IME_COMPOSITION`） | Wayland text-input-v3，X11 XIM |
| 辅助功能 | AccessKit | AccessKit |
| 背景 | Mica、MicaAlt、Blurred | 模糊依赖合成器（ext-background-effect） |

- Zed 的 Windows 版已于 2025-10 发布稳定版，这是 Windows 后端可用的旁证（来源是二手报道）。
- **排版每个平台各走一套**，只有折行规则共用 ICU，上面的 CJK 结论不能照搬，要分别验证。
- **中文输入是这几端的已知问题**。Zed 仍未关闭的 issue：
  - Windows：[#59882](https://github.com/zed-industries/zed/issues/59882)、[#56327](https://github.com/zed-industries/zed/issues/56327)、[#61724](https://github.com/zed-industries/zed/issues/61724)、[#64832](https://github.com/zed-industries/zed/issues/64832)、[#65973](https://github.com/zed-industries/zed/issues/65973)
  - Linux：[#60578](https://github.com/zed-industries/zed/issues/60578)、[#65302](https://github.com/zed-industries/zed/issues/65302)
  - macOS：[#61937](https://github.com/zed-industries/zed/issues/61937)、[#65868](https://github.com/zed-industries/zed/issues/65868)、[#60747](https://github.com/zed-industries/zed/issues/60747)
- 原型移植要改的地方：字体名（现在写死 PingFang SC 和 Menlo）；`driver.rs` 依赖 cocoa，要隔离。

## 移动端（按新优先级，未构建、未测试）

之前“没有官方 iOS 支持”的判断已经过时，但 Android 仍然成立。

- **iOS**：上游 [PR #63068](https://github.com/zed-industries/zed/pull/63068) 于 2026-10-14 合入 `gpui_ios`，明确标为实验性。Return 键和硬件键盘快捷键还没有作为 GPUI 按键事件送达，也不是完整的功能对等。[触摸 API](https://github.com/zed-industries/zed/pull/60496) 已合入。`gpui-pre-ios` 已上 crates.io。
- **Android**：**没有官方后端**。维护者的[路线图](https://github.com/mikayla-maki/gpui-projects/blob/main/projects/mobile.md)把它放在“Later”，说核心设计已为 Android 留好；[#12](https://github.com/mikayla-maki/gpui-projects/issues/12) 带 `someday` 标签，没有负责人。
- **社区方案**：[longbridge/gpui-mobile](https://github.com/longbridge/gpui-mobile) 支持 iOS 和 Android，自述实验性，移动端输入法、辅助功能和生命周期都未完成。crate 声明 GPL/AGPL/Apache-2.0 三选一，可选 Apache-2.0。
- **桌面优先的后续成本**：
  - `doc.rs` 和 `markdown.rs`（约 440 行）不依赖 GPUI，可以直接复用。
  - 输入走同一个 `EntityInputHandler`，iOS 通过它桥接 `UITextInput`。
  - 要新做：触摸选区和手柄、软键盘避让、生命周期、移动端布局。
  - Android 的文字走 cosmic-text，CJK 要重新验证。
  - **最大风险是 Android 没有官方路径**，届时可能要依赖社区后端或自己写一个。

## 代码量和完成度

编辑器约 2400 行（不含空行和测试）：`doc` 73、`markdown` 362、`display` 388、`element` 246、`editor` 1155、`blocks` 183。另有驱动和计时约 800 行，不属于产品。作为对照，Fumi 的 `Views/Editor` 约 7575 行。

**已做：**

- 字节级一致的 rope 文档和增量块解析。
- 只在光标行显示语法。
- 标题字号和行高、粗体、斜体、行内代码、删除线、链接、wiki 链接、列表、任务、引用、代码块。
- 表格、公式（RaTeX，MIT）、图片折叠，光标进入时显示源码。
- 虚拟化（GPUI 的 `list`）。
- 方向键、回车、退格、撤销和重做、选区、复制粘贴、保存、输入法路径。

**没做或没测：**

- 真实拼音（需要你试）。程序化调用 `setMarkedText`/`insertText` 已通过，一次撤销能移除整段组词。候选框位置只验证了坐标返回值，没看到真实候选框。
- 拖选自动滚动、可拖动滚动条（现在按条目比例估算，不是真实高度）。
- 表格单元格内编辑、窄窗口下的表格折行。
- callout、frontmatter 卡片、代码高亮、斜杠菜单、大纲、深色模式。
- 窗口缩放的性能、GPU 耗时。
- 软折行处的光标归属。
- 公式在主线程同步渲染，目前没造成卡顿，但应移到后台。

**发现的 GPUI 坑：** `list` 不处理水平内边距；宽度变化时会清空所有高度提示；对没量过的条目，`scroll_to_reveal_item` 会算错位置（我已绕开）。

**完成工作量（粗估，把握不大）：** 编辑器达到 Fumi 水平 6–10 周，应用外壳（侧栏、设置、搜索、版本历史）另需 2–3 个月，都按一人全职算。辅助功能和三端输入法打磨是其中最不确定的部分。

## 建议

1. 先试拼音。这是目前唯一可能直接否决的一项。
2. 拼音没问题的话，下一步在 Windows 上跑一次原型，因为那里的 CJK 和输入法问题最多。
3. 决定前，权衡 pre-1.0 快照（每周可能有破坏性变更）、自己做辅助功能、没有 Liquid Glass 这三项。

## 来源

- [crates.io: gpui](https://crates.io/crates/gpui)、[gpui-pre](https://crates.io/crates/gpui-pre)、[gpui-component](https://crates.io/crates/gpui-component)、[zorite-editor](https://crates.io/crates/zorite-editor)、[ratex-gpui](https://crates.io/crates/ratex-gpui)
- [zed#43206 gpui: On iOS](https://github.com/zed-industries/zed/issues/43206)、[zed#43207 gpui: On Android](https://github.com/zed-industries/zed/issues/43207)
- [Zed for Windows 报道](https://alternativeto.net/news/2025/10/zed-editor-launches-native-windows-version-with-wsl-integration-extensions-and-ai-features)
