# gpui-fast 试验

日期 2026-11-06。分支 `spike/gpui-fast`，基于 `098e472`。机器 M4 Max，macOS 27，Rust 1.95（没改默认工具链）。
gpui-fast 用 crates.io 上的 0.1.13（对应仓库 HEAD `a10f393`）。原始数据和脚本在 `results/gpui-fast-spike/`。

## 结论：先等等（wait）

- **能换，而且换起来很省事**：只改了 Cargo 配置，一行 `.rs` 都没动，675 个测试全过，屏幕上看排版、编辑、换主题都正常。
- **但 Fumi 最在意的打字反而变慢了**：200KB 打字时，每帧主线程中位数从 2.2ms 变成 3.3ms；而且每次打字测试都会冒出一次 20–80ms 的卡顿，gpui-pre 一次都没有。只要关掉 gpui-fast 的两项加速之一（`GPUI_VIEW_RETENTION=0` 或 `GPUI_SCROLL_LAYERS=0`），数字就回到 gpui-pre 的水平。所以它的加速机制和我们的编辑器（一个大 `list`）合不来。
- **有提升的地方大多站不住**：第一轮里，启动时最长的一次卡顿从 73ms 降到 11ms，稳定后的内存从 427MB 降到 357MB。但第三轮（关掉 scroll layers，见第 7 节）这两项都没再出现差别，gpui-pre 也没再出现启动卡顿，所以更像是第一轮的噪声。**真正稳定的提升只有打开文档**：200KB 时 p95 从 12ms 降到 6ms，1MB 时从 21ms 降到 0.4ms，但两边都已经远低于一帧（16.7ms）。滚动没有收益。
- **窗口合成能替掉快照绕法**：原型里，Info 卡片、Search、Settings 都能画在**实时的** WKWebView 上面，网页照样能滚动，点完网页再点回编辑器也能接着打字。代价有两点：Search 和 Settings 要改成 `deferred` 绘制；有一次没看到 scrim 盖住网页，原因没确认（见第 3 节）。
- **为什么还不换**：gpui-fast 自称 experimental，现在是 0.1.x，同样需要我们给 Rust 1.95 打补丁。为了去掉一个已经能用的绕法，去换一个让编辑器变慢的 fork，不划算。
- **什么时候再看**：
  - gpui-fast 修好“编辑 list 里的一行”时的卡顿；或者
  - 窗口合成（zed#62379）合进上游 Zed 或 gpui-pre。
- **想先用合成也有办法，而且已经测过**：在 `main` 里、GPUI 启动之前设 `GPUI_SCROLL_LAYERS=0`（commit `f743949`），打字回到和 gpui-pre 持平，卡顿消失，合成照样能用（第 7 节）。这样换过去，性能上基本是“不赚不赔”，只多了打开文档的小提升。所以要不要换，只取决于值不值得为去掉快照绕法、接受一个 experimental 的 fork。我仍然建议先等。

## 1. gpui-fast 和 gpui-pre 的区别

- **同源**：两者都是 Zed GPUI 的快照。gpui-fast 跟到 Zed `decd2399`（见它的 `UPSTREAM`），自己的改动放在 `fast/` 模块里，对上游只加了很小的钩子。公开 API 基本相同。
- **Retained Mode（默认开）**：每个 view 都是可复用的子树，包括所有 `Entity<V: Render>` 和 `AnyView`，有没有 `.cached()` 都算。它依赖的东西都没变时，这一帧就不再 render、layout、prepaint、paint，直接沿用上一帧。依赖包括：读过的 entity、global、`ListState` / `ScrollHandle`、鼠标位置、修饰键。
  - **规则**：render 读到的状态如果不在 entity、global 或 list/scroll state 里（比如 `Rc<RefCell<..>>`、thread_local、当前时间），改了就要自己 `cx.notify()`，否则会一直显示旧的。上游 GPUI 对 `.cached()` view 本来就是这个要求，现在扩大到所有 view。
  - **对 Fumi 的影响**：
    - 主题 `t()` 是 thread_local，但换主题时 `root.rs` 会调 `window.refresh()`。gpui-fast 在 refresh 那一帧不复用任何东西，所以没问题。屏幕上验证过：改强调色后，编辑器、侧栏、设置页都立刻跟着变了。
    - 动画用的是 `Instant::now()` 加 `request_animation_frame()`。gpui-fast 的 `request_animation_frame` 会 notify 当前 view，也没问题。
    - 代码里还有 38 处 thread_local / `Rc<RefCell>`，没有逐个审查。
  - **排查开关**：`GPUI_VIEW_RETENTION=0` 关掉复用，每帧从头画，和上游一样。
- **Scroll layers（默认开）**：`list` 和 `uniform_list` 按行缓存，只滚动时直接重放，不重建；第一次画时会把上下各约两屏的行（overscan）都画出来。`GPUI_SCROLL_LAYERS=0` 关闭。Fumi 的编辑器正文就是一个 `list`。
- **窗口合成（要主动开）**：调 `window.enable_window_composition()` 之后可以建 native surface 来放 `WKWebView`。根 view 画在它下面，deferred 和窗口级的绘制画在它上面的 overlay 层。在 macOS 上，native surface 是 GPUI view 的子视图，而 Fumi 现在的做法是把 web view 放在 GPUI view 的兄弟视图里。
- gpui-fast 自己用的工具链是 Rust 1.98.1。

## 2. 构建改动（commit `03eaea0`）

一共 3 处，没改任何 `.rs` 源码就编过，`cargo test --workspace` 675 个测试全过。

1. 根目录和 8 个 crate 的 `Cargo.toml`：`gpui = { package = "gpui-fast", version = "=0.1.13" }`，`gpui_platform = { package = "gpui-fast-platform", version = "=0.1.13", features = ["font-kit", "runtime_shaders"] }`。
2. **`ratex-gpui`（公式渲染）依赖 `gpui-pre` ^0.3**。不处理的话会同时编进两套 GPUI，类型对不上。gpui-fast 自带的 `compat/gpui-pre` 版本是 0.3.9，并且按路径依赖它自己的仓库，没法直接拿来用。所以我在 `vendor/gpui-fast-compat/gpui-pre` 放了一个 0.3.11 的替身，只有一行 `pub use gpui_fast::*;`，再用 `[patch.crates-io]` 接上。
3. **Rust 1.95 的补丁还要**：`gpui-fast-macos` 0.1.13 里有和 `gpui-pre-macos` 一样的 3 处 `Option::map_or_default`，这在 1.95 上是不稳定 API。我把它复制到 `vendor/gpui-fast-macos`，改了同样的 3 行。原来的 `vendor/gpui-pre-macos` 补丁已经用不上了（cargo 会提示 unused patch），真换的话可以删掉。

依赖图里还有 `gpui-pre-collections`、`gpui-pre-util` 等 0.3.11 的辅助 crate。它们是 gpui-fast 自己的依赖，不是第二套 GPUI。

## 3. 窗口合成能不能替掉快照绕法（commit `cc9c2f3`）

原型放在 `FUMI_COMPOSE=1` 开关后面，默认关，所以不影响上面的对比数据。改了 3 处：

- `web.rs` `WebHost`：开启合成后建一个 native surface，把装 web view 的 container 放进去。每次 `place` 时把 surface 的 bounds 设成 container 原来的位置；隐藏时一起隐藏 surface。
- `root.rs`：Search 和 Settings 原来是根 view 的普通子元素，会落在 web view 下面，所以改成 `deferred(...)`，让它们落到 overlay 层。Info 卡片本来就是 `deferred(anchored(...))`，不用改。
- `pane.rs` `set_covered`：开启合成后不再拍快照，也不隐藏 web view。

屏幕检查只在测试包 `com.fumi.gpui-prototype` 里做，用的是临时库（fixtures 里的 `typography.md`、`tricky.md`、`10k.md`）：

| 检查项 | 结果 |
|---|---|
| WKWebView 放进 native surface 后能显示，位置、圆角、窗口缩放时跟随都对 | 通过 |
| Info 卡片（含阴影）画在**实时**网页上面，盖住了标题和第一段 | 通过，这正是快照绕法要解决的情况 |
| Search、Settings 和它们的 scrim | 面板正常；第二次打开 Search 时，scrim 正确压暗了网页。第一次打开时网页没被压暗，可能是截到了渐显的中途，也可能是首次出现时的层序问题，**没确认** |
| 网页内滚轮滚动 | 通过 |
| 点网页后再点编辑器打字，字进了编辑器 | 通过 |
| 在网页里的输入框打字，看按键是否会冒泡回 GPUI | **没测**。web view 现在是 GPUI view 的子视图，网页没处理的按键会沿响应链传到 GPUIView，需要专门检查 |
| Settings 面板正好压在网页上的情况 | 窗口宽度下 Settings 没碰到网页区域，**没直接看到**；但它和 Info 卡片走的是同一层 |

结论：**在 macOS 上，合成可以替掉 `set_covered` 的快照绕法**，前提是所有压在网页上的浮层都走 `deferred`。上表里两项没测、一项没确认，真要换之前要补上。

## 4. 性能对比

完整应用，`BENCH_SCENARIO`，用合成的 1000 篇笔记库（`bench10k` 到 `bench1m`），每次运行前重新复制一份。gpui-pre 和 gpui-fast 交替运行，取中位数。数字是主线程忙碌片段（大于 0.2ms 的才计入），单位 ms。机器上同时有别的会话在编译，load average 约 7–50。

**第一轮**（3 次；锁屏后的 3 次运行作废，放在 `pass1/invalid/`）：

| 场景 | gpui-pre | gpui-fast |
|---|---|---|
| 启动：notesLoaded / 稳定 | 563 / 1362 ms | 550 / 1160 ms |
| 启动：最长卡顿 | 72.6 | 11.1 |
| 启动：稳定后内存 | 427MB | 357MB |
| 打开 200KB：p50 / p95 | 1.30 / 19.3 | 0.63 / 11.5 |
| 打开 1MB：p50 / p95 | 0.86 / 24.2 | 0.27 / 0.40 |
| 打字 200KB：p50 / p95 / 最长 | 2.16 / 4.20 / 4.6 | **3.25 / 5.50 / 37.7** |
| 打字 1MB：p50 / p95 / 最长 | 2.58 / 4.03 / 4.5 | **3.07 / 4.93 / 21.1** |
| 打字时超过 16.7ms 的次数 | 每次 0 | 每次 1 |
| 滚动 1MB：p50 / p95 / 最长 | 2.60 / 4.00 / 19.6 | 2.41 / 3.69 / 266 |
| 空闲 CPU | 0.44% | 0.07% |
| 搜索“性能” | 3.2 | 1.3 |

- **滚动**：gpui-fast 有 2 次有效运行，其中一次出现 502ms 的长卡顿。
- **空闲 CPU**：gpui-pre 的 0.44% 是两次的中位数，其中一次 0.80%、一次 0.08%，波动大。

**第二轮**（2 次，加两个对照组：gpui-fast 关掉 retention、gpui-fast 关掉 scroll layers）：

| 场景 | gpui-pre | gpui-fast | 关 retention | 关 scroll layers |
|---|---|---|---|---|
| 打字 200KB：p50 / p95 / 最长 | 2.30 / 3.48 / 4.2 | 3.50 / 4.66 / 24.5 | 2.26 / 3.25 / 3.8 | 2.36 / 3.48 / 4.0 |
| 滚动 1MB：p50 / p95 / 最长 | 2.58 / 3.79 / 8.5 | 3.87 / 6.27 / 29.9 | 3.01 / 5.45 / 26.7 | 3.28 / 5.54 / 23.6 |

- **打字**：两个开关任关一个，打字的回退和卡顿都消失。所以问题出在 retention 和 scroll layers 处理编辑器 `list` 的那段逻辑上，不在 gpui-fast 的其他部分。
- **滚动**：这一轮 gpui-fast 比 gpui-pre 慢，两个对照组也稍慢；第一轮却持平。噪声比较大，只能说滚动没看到收益。
- **没跑 `scripts/matrix.sh`**：裸编辑器矩阵要窗口一直在前台，跑约一小时。第一轮中途锁屏，之后你又在用这台机器，所以只跑了上面这些完整应用的场景，打字和滚动都已经覆盖。

## 5. 屏幕检查（`com.fumi.gpui-prototype`，开着 `FUMI_COMPOSE=1`）

- **文字排版**：避头尾、中英混排基线、罕用字、日韩文、emoji、行内代码、列表悬挂缩进、引用竖线、代码块、表格、图片、frontmatter 都和 gpui-pre 一样正常。
- **编辑**：输入、折行重排、光标移动后插入都正常。光标层本身没看到，因为后台控制时窗口不是 key window；正常情况下这时本来就不画光标，所以这项不能算验证过。
- **缓存的 view**（sidebar、toolbar、pages、caret）：切换笔记时侧栏高亮跟着变，工具栏按钮状态对；改强调色后所有 view 都更新了，没有看到旧画面残留。
- **原生 web view**：见第 3 节。

## 6. 坏了什么 / 要注意的

- 打字回退（见第 4 节）。这是不建议现在换的主要原因。
- 第一轮滚动出现过一次 502ms 的卡顿，只出现一次，原因没查。
- 第一轮里，每次 gpui-fast 的运行在墙钟上都比 gpui-pre 多 60–70 秒。单独测启动后退出，两者都只要 1–2 秒，所以这段时间花在场景中间，具体在哪没查到。
- gpui-fast 仍要我们自己给 Rust 1.95 打补丁，`ratex-gpui` 也要一个 gpui-pre 替身。以后每次升级都要同步检查这两处。
- gpui-fast 的 `compat/` 跟的是 GPUI Kit 锁定的 gpui-pre 版本（现在是 0.3.9），比我们的 0.3.11 旧。真换的话，自己维护替身更稳。

## 7. 关掉 scroll layers 后，其他提升还在吗（第三轮）

**一句话：打字回到和 gpui-pre 持平，打开文档的提升还在；启动卡顿、内存、搜索、空闲 CPU 都没有提升。**

做法：

- 只比两组：gpui-pre（`098e472`），以及 gpui-fast 关掉 scroll layers、retention 保持开启。
- 关闭方式和真实构建一样：在 `main` 开头、GPUI 启动之前调用 `std::env::set_var("GPUI_SCROLL_LAYERS", "0")`（commit `f743949`；环境里已经设了这个变量时不覆盖）。gpui-fast 在每个窗口创建时读取这个变量，所以这样设一定生效。打字卡顿消失也印证了这一点。
- 场景和第一轮一样：启动和搜索、打开（10K/50K/200K/1M）、打字 200K/1M、滚动 1M、空闲。
- 跑 3 轮，两组交替，每次运行前重新复制一份 bench 库，没用你真实的笔记库。
- 10:19 到 10:44 之间屏幕一直没锁（每次运行前后都检查了），没有作废的运行。
- load average：第 1 轮开始时约 40–50（刚编译完），之后在 7–12 之间，第 3 轮打字前有一次升到 31。

| 场景 | gpui-pre | gpui-fast（关 scroll layers） | 提升还在吗 |
|---|---|---|---|
| 打字 200KB：p50 / p95 / 最长 | 2.12 / 4.40 / 5.1 | 2.25 / 4.06 / 4.9 | 持平，卡顿消失 |
| 打字 1MB：p50 / p95 / 最长 | 2.34 / 4.69 / 5.5 | 2.12 / 4.66 / 5.3 | 持平，卡顿消失 |
| 启动：最长卡顿 | 7.1 | 6.3 | **不在**：这一轮 gpui-pre 也没有第一轮那次 73ms 的卡顿 |
| 启动：notesLoaded / 稳定 | 468 / 1048 ms | 472 / 980 ms | 基本持平 |
| 启动：稳定后内存 | 379MB | 381MB | **不在** |
| 打开 200KB：p50 / p95 | 1.58 / 12.1 | 0.78 / 6.3 | **在** |
| 打开 1MB：p50 / p95 | 1.66 / 20.9 | 0.30 / 0.41 | **在** |
| 搜索 “plain” / “zzzqqq” / “性能” | 24.6 / 10.6 / 3.6 | 21.5 / 9.8 / 4.4 | **不在**：搜索是纯 Rust 计算，本来就和 GPUI 无关 |
| 空闲 CPU | 0.98% | 0.81% | **不在**：gpui-fast 三次分别是 4.55%、0.05%、0.81%，波动大于差值 |
| 滚动 1MB：p50 / p95 / 最长 | 2.87 / 4.50 / 20.9 | 2.85 / 4.15 / 19.3 | 持平 |

- **启动和内存**：第一轮的差别在这一轮没有重现，两组都约 380MB，启动卡顿也都在 7ms 左右。所以第一轮那两项更可能是噪声，而不是 gpui-fast 的效果。也可能和 scroll layers 有关，但这次没有单独测。
- **空闲 CPU**：两组都比第一轮高（约 1% 对 0.1%），说明这台机器当时有别的负载，这一项这轮看不出差别。
- **打开文档**：这项提升来自 retention，和 scroll layers 无关，所以关掉之后还在。不过两边都远低于一帧，用户感觉不到。
- **对建议的影响**：关掉 scroll layers 后，gpui-fast 在性能上和 gpui-pre 基本持平，没有变差，也几乎没有可感知的提升。所以换不换只取决于窗口合成值不值得：能去掉快照绕法，代价是依赖一个 experimental、0.1.x 的 fork，还要自己维护 Rust 1.95 补丁和 gpui-pre 替身。**建议不变，仍然是先等**。如果快照绕法以后出现用户能感觉到的问题，可以用这个配置（关 scroll layers 加 `FUMI_COMPOSE`）先切过去。

原始数据：`results/gpui-fast-spike/pass3/`、`pass3-summary.txt`、`run3.sh`。
