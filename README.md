# 热血传奇 TUI 挂机（r-chuanqi）

终端里的挂机游戏。职业、怪物、地图和装备来自传奇 1.67，赤月和牛魔这段顶级内容来自 1.76。数值是按成长曲线估的，不是客户端原始表，改 `data/*.ron` 即可，不用改代码。

## 运行

需要 Windows Terminal，或者别的支持 Unicode 和真彩色的终端。旧版 `cmd.exe` 里中文宽度和颜色会错。

```bash
cargo run -p r-chuanqi
cargo run -p r-chuanqi --bin sim -- --class warrior --ticks 36000 --seed 42
cargo run -p r-chuanqi --bin check-content
```

发布包是安装程序，不是解压即用的压缩包。下载 `r-chuanqi-setup` 运行一次：Windows 装到 `%LOCALAPPDATA%\tui-game` 并写入用户 PATH，其它系统装到 `~/.local/share/tui-game`，命令放在 `~/.local/bin/r-chuanqi`。关掉终端再开，直接运行 `r-chuanqi`。

推送 `vX.Y.Z` 标签会跑 `.github/workflows/release.yml`，为 Windows、Linux 和 macOS 打包并发布到 GitHub Releases。标签已经存在时，也可以在 Actions 里手动跑 release，填同一个标签。

`sim` 不画界面，用来看等级、金币和击杀。`--class` 可以是 `warrior`、`mage`、`taoist`。

存档默认在 `%APPDATA%\tui-mir\`（其它系统用 `directories` 的数据目录）。每个角色一个槽位目录，同名可以同时存在。离线收益只结算正在进入的那个角色。

## 操作

启动后是角色列表。`N` 创建，`D` 删除（要再输入一次名字），Enter 进入。

| 键 | 作用 |
| --- | --- |
| 1–6 | 战斗、背包、技能、地图、商店、统计 |
| ↑↓ Enter | 选择、使用、穿戴、进图 |
| L | 锁定装备。高风险死亡不掉锁定的那件 |
| X / R / F | 卖白装、修理、把药补到 20 瓶 |
| A | 自动换图。4 级之后才能开 |
| U | 武器升级。先看成功率，再按一次才执行。有新版本时改为下载并重启 |
| ? | 帮助 |
| F2 | 伪装成时钟，再按一次恢复 |
| Q | 保存并退出 |

正式版启动后会后台查 GitHub Releases。有更新的版本时底部会提示，按 `U` 下载对应平台的安装包并重启。存档不会被替换。`cargo run` 的调试构建不检查。仓库默认是 `runbrick/tui-chuanqi`，编译时可以用 `TUI_GAME_REPO=owner/name` 覆盖。

职业、性别、高风险在创建时选定，之后不能改。高风险掉落更高，死亡会掉一件没锁定的装备。

窗口至少 80×24。再小就只提示放大，不会把布局撑崩。

## 目录

- `crates/game-data`：RON 类型、加载和校验
- `crates/game-core`：状态、战斗、掉落、存档结构。这里不能依赖终端
- `crates/game-tui`：界面和槽位存档
- `data/`：内容表。怪物、物品、技能、地图、NPC、平衡系数都在这里
- `doc/`：设计文档

逻辑 tick 是 100 毫秒。离线就是快放同一套 tick，最多累计 12 小时。
