# 技术选型

## 结论

**Rust + ratatui + crossterm**，数据用 RON，不引入异步运行时。

## 为什么是 Rust / ratatui

挂机游戏的特点是：**UI 简单但模拟逻辑重**。界面无非是几个面板 + 滚动日志，但背后要跑长时间的数值模拟，还要算离线收益（一次性推演几万到几百万个 tick）。所以瓶颈在计算，不在渲染。

- `ratatui` 是目前 TUI 生态里最成熟的（immediate-mode 渲染、布局约束系统、widget 齐全），社区活跃度和文档都好。
- Rust 的枚举 + 模式匹配特别适合表达"物品类型 / 技能效果 / 掉落规则"这类代数数据类型，比在动态语言里用字典硬凑要清晰得多。
- 离线推演这种纯数值 loop，Rust 能做到毫秒级完成几十万 tick，不用为了性能写特殊的近似公式（Python 大概率要）。
- 单文件二进制分发，玩家不用装运行时。

### 备选方案与取舍

| 方案 | 优点 | 为什么不选 |
| --- | --- | --- |
| Go + Bubble Tea | 上手快、并发模型舒服 | Elm 架构在"大状态 + 高频 tick"下 `Update` 会变成巨型 switch；数值建模表达力弱于 Rust 枚举 |
| Python + Textual | 开发速度最快、CSS 式样式很香 | 离线推演性能是硬伤；分发需要打包 |
| C# + Terminal.Gui / Spectre.Console | .NET 生态、LINQ 处理数据表方便 | TUI 生态不如 ratatui；跨平台终端细节坑多 |

如果你**更看重开发速度而不是性能和分发**，Python + Textual 是唯一值得重新考虑的选项 —— 但那样离线收益必须用封闭公式近似，不能用 tick 推演。本规划其余部分假定 Rust。

## 依赖清单

```toml
# 核心（game-core，务必保持这里依赖最少）
serde       = { version = "1", features = ["derive"] }
rand        = "0.8"       # 需要可播种、可重现的 RNG
rand_pcg    = "0.3"       # PCG64：确定性、跨平台一致、比 StdRng 稳定
thiserror   = "2"

# 数据加载
ron         = "0.8"       # 静态数据表格式
serde_json  = "1"         # 存档（易于手改调试）

# TUI 前端
ratatui     = "0.29"
crossterm   = "0.28"

# 工具
directories = "5"         # 跨平台存档路径
tracing            = "0.1"
tracing-appender   = "0.2"  # 日志写文件，不能写 stdout（会污染 TUI）

# 开发期
insta       = "1"         # 快照测试：锁定数值平衡，防止改公式时意外跑偏
criterion   = "0.5"       # 基准测试：盯住离线推演耗时
proptest    = "1"         # 属性测试：验证"模拟永不 panic / 数值不溢出"
```

### 几个关键依赖的理由

- **`rand_pcg` 而不是默认 `StdRng`**：`StdRng` 的算法在 rand 版本间可能变化，而我们要求「同一存档 + 同一种子 → 同样结果」在升级后依然成立。PCG64 有稳定性保证。
- **不用 `tokio`**：游戏循环仍然是固定 tick。自动更新只在后台线程里用一次阻塞的 HTTPS 请求，不把游戏状态拖进异步运行时。
- **RON 而不是 JSON 做静态数据**：RON 支持枚举变体字面量（`Effect::Poison(green, 3.0)`）、尾逗号、注释。写内容表时体验差别很大。
- **存档用 JSON**：方便出问题时直接手改，也便于你肉眼 review 平衡性。
- **日志绝不能进 stdout**：TUI 独占终端。统一 `tracing` + `tracing-appender` 写到文件。

## 开发工具链

```bash
rustup component add clippy rustfmt
cargo install cargo-nextest   # 测试跑得更快、输出更清楚
cargo install cargo-watch     # cargo watch -x "nextest run -p game-core"
```

> **Windows 注意**：你在 Windows 上开发。ratatui + crossterm 在 Windows Terminal 下表现良好，但在旧版 `conhost.exe`（cmd.exe 默认宿主）里 Unicode 和颜色会有问题。开发和测试请固定用 **Windows Terminal**，并在 README 里向玩家注明。另外中文字符是双宽度，布局计算时必须用 `unicode-width` 而不是 `str::len()`（见 `02-architecture.md` 的渲染注意事项）。
