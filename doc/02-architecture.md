# 架构设计

## 核心原则

> **`game-core` 不知道终端的存在，也不知道文件系统的存在。**

它是一个纯粹的状态机：`(状态, 静态数据, 玩家指令) → 新状态 + 事件列表`。所有 IO（读键盘、画屏幕、读写存档、加载数据表）都在外层。

这一条带来的好处是全局性的：
- 可以写快照测试，锁住数值平衡；
- 离线收益就是"快速跑 N 个 tick"，不需要第二套公式；
- 以后想加 Web 前端 / 自动平衡脚本 / 无头 benchmark，核心一行不改。

## Crate 划分

```
tui-game/
├── Cargo.toml              # workspace
├── crates/
│   ├── game-core/          # 纯逻辑：状态、tick、战斗、掉落、升级
│   ├── game-data/          # 静态数据类型定义 + RON 加载器 + 校验
│   └── game-tui/           # ratatui 前端 + 存档 IO + main()
├── data/                   # RON 内容文件（随二进制分发）
│   ├── maps.ron
│   ├── monsters.ron
│   ├── items.ron
│   ├── skills.ron
│   ├── npcs.ron
│   └── balance.ron         # 经验表、公式系数等可调参数
└── doc/
```

**依赖方向严格单向**：`game-tui → game-core → game-data`。
`game-core` 的 `Cargo.toml` 里不允许出现 `ratatui`、`crossterm`、`std::fs`。建议在 CI 里加一条检查。

### 各 crate 职责

**`game-data`** — 只管"数据长什么样"和"怎么把它读进来"。
- 定义 `MonsterDef`、`ItemDef`、`SkillDef`、`MapDef`、`NpcDef`、`BalanceConfig`
- `GameData::load(dir) -> Result<GameData>`
- **校验**：所有 ID 引用必须能解析（地图引用的怪物存在、掉落表引用的物品存在、技能书对应的技能存在）。启动时一次性校验完，核心层之后可以无条件 unwrap。
- ID 用 newtype（`MonsterId(u32)`）而不是裸 `u32`，避免传错参数。

**`game-core`** — 游戏规则的全部。
- `GameState`：玩家属性、背包、装备、技能等级、当前地图、战斗状态、统计数据、RNG 状态
- `fn tick(&mut self, data: &GameData) -> Vec<GameEvent>`
- `fn apply(&mut self, cmd: Command, data: &GameData) -> Result<Vec<GameEvent>>`
- `fn simulate_offline(&mut self, data: &GameData, elapsed: Duration) -> OfflineReport`

**`game-tui`** — 渲染 + 输入 + 持久化。
- 游戏主循环、面板布局、键位映射、存档读写、日志初始化

## Tick 模型

```
逻辑 tick = 100ms（固定）
渲染      = 事件驱动 + 最多 20 FPS 上限
```

逻辑和渲染解耦，用累加器（accumulator）追帧：

```rust
let tick_dur = Duration::from_millis(100);
let mut acc = Duration::ZERO;
let mut last = Instant::now();

loop {
    let now = Instant::now();
    acc += now - last;
    last = now;

    // 处理输入（非阻塞，超时等于一帧预算）
    while event::poll(Duration::from_millis(0))? {
        if let Some(cmd) = input::map(event::read()?) { pending.push(cmd); }
    }

    // 追平逻辑 tick。设上限，避免系统休眠后一次追几万帧卡住 UI
    let mut steps = 0;
    while acc >= tick_dur && steps < MAX_CATCHUP {
        for cmd in pending.drain(..) { state.apply(cmd, &data)?; }
        let events = state.tick(&data);
        log.extend(events);
        acc -= tick_dur;
        steps += 1;
    }
    if acc > tick_dur * MAX_CATCHUP as u32 { acc = Duration::ZERO; } // 丢弃积压

    // 渲染（脏标记 + 限流）
    if dirty || last_draw.elapsed() > Duration::from_millis(50) {
        terminal.draw(|f| ui::render(f, &state, &log, &data))?;
    }
}
```

**为什么 100ms**：传奇的攻击间隔大约 0.5–1.4 秒量级，100ms 的粒度足够表达攻击速度差异，又能让离线推演保持在合理数量级（1 小时 = 36000 tick，一天 = 864000 tick，Rust 里跑完都在毫秒级）。

## 确定性与 RNG

RNG 状态**存在 `GameState` 里**，随存档一起序列化：

```rust
pub struct GameState {
    rng: Pcg64,   // 实现 Serialize/Deserialize
    // ...
}
```

这样做的收益：
- **可复现 bug**：玩家给你一个存档，你能跑出完全一样的后续。
- **离线 = 在线**：离线推演不是另一套公式，就是循环调 `tick()`，结果和玩家挂着不关机完全一致。
- **快照测试可行**：固定种子 → 跑 10000 tick → 断言最终属性。改动公式时能立刻看到平衡被破坏了多少。

> ⚠️ 代价：不能在 `tick` 里用 `HashMap` 迭代顺序做任何影响状态的决策（迭代顺序不确定）。涉及顺序的地方统一用 `Vec` 或 `BTreeMap`。

## 离线收益

玩家关掉游戏再打开，算法：

1. 读存档里的 `last_saved_at`（Unix 秒）
2. `elapsed = now - last_saved_at`，按配置截断（比如最多累计 12 小时，防止改系统时间刷）
3. `simulate_offline` 直接跑 `elapsed / 100ms` 个 tick
4. 产出 `OfflineReport`（经验、金币、掉落、升级次数、死亡次数），开局弹窗展示

需要在 `tick` 里加一个"离线模式"标记供少数行为分支使用（例如离线不触发 UI 动画类事件、不产生详细战斗日志，只做聚合统计 —— 否则 12 小时的日志会吃掉几百 MB 内存）。

> **性能约束（写进 benchmark）**：12 小时离线推演必须在 **500ms** 内完成。超了就要优化，不要降级成近似公式。

## 事件流与日志

`tick` 返回 `Vec<GameEvent>`，而不是直接改 UI：

```rust
pub enum GameEvent {
    Attacked   { target: MonsterId, damage: i32, crit: bool },
    Blocked    { target: MonsterId },              // 传奇特色：伤害被防御吃掉
    MonsterSlain { id: MonsterId, exp: u64 },
    ItemDropped  { id: ItemId, count: u32 },
    LevelUp      { level: u16 },
    SkillLevelUp { id: SkillId, level: u8 },
    PlayerDied   { lost_exp: u64 },
    PotionUsed   { id: ItemId },
    OutOfPotions,
    // ...
}
```

UI 层把事件格式化成滚动日志（环形缓冲，容量固定，比如 500 条），顺便驱动数字跳动之类的表现。

## 渲染注意事项

1. **中文宽度**：中文字符占 2 个终端列。ratatui 的 `Span` 不会自动处理，所有涉及对齐 / 截断 / 进度条填充的地方都要用 `unicode_width::UnicodeWidthStr::width()`。建议早期就封一个 `pad_to(s, width)` 工具函数，全项目只用它。
2. **颜色**：传奇的装备有品质色（白 / 绿 / 蓝 / 紫 / 橙之类）。定一张 `theme.rs`，不要在 widget 里散落硬编码颜色。
3. **最小终端尺寸**：定一个底线（建议 80×24）。小于它就画一句"请放大窗口"，而不是让布局 panic。
4. **不要每帧重算布局数据**：派生数据（总攻击力、暴击率等）缓存在 `GameState` 里，装备 / 技能变更时失效重算。

## 建议的屏幕布局

```
┌─ 角色 ──────────────┬─ 战斗 ─────────────────────────┐
│ 战士 Lv.23          │ 当前地图：沃玛寺庙 三层         │
│ HP ████████░░ 412/500│                                │
│ MP ███░░░░░░░  38/120│ 敌人：沃玛卫士  HP ███░░ 230/700│
│ 攻击 42-68          │ 你：第 3 下 / 攻速 1.2s        │
│ 防御 18-24          │                                │
│ 魔防 5-9            ├─ 日志 ─────────────────────────┤
│ 经验 ██████░░ 62%   │ 命中沃玛卫士 54 点              │
├─ 装备 ──────────────┤ 沃玛卫士抵挡了你的攻击          │
│ 武器 乌木剑 +3      │ 击杀沃玛卫士，获得 1250 经验     │
│ 衣服 重盔甲(男)     │ 沃玛卫士掉落 金创药(大)×2       │
│ 头盔 铁头盔         │ 喝下 金创药(大)                 │
│ ...                 │ 沃玛卫士掉落 乌木剑 ★           │
└─────────────────────┴────────────────────────────────┘
 [1]角色 [2]背包 [3]技能 [4]地图 [5]商店 [6]统计   [Q]退出
```

主界面常驻，`1–6` 切换右侧 / 弹层面板。挂机游戏的核心是"看着数字涨"，所以**战斗面板和日志必须一直可见**。

## 存档（多槽位）

按 ADR-001，**角色即存档槽**，同职业可以开多个。

### 目录结构

`directories::ProjectDirs` 下（Windows：`%APPDATA%\tui-mir\`）：

```
tui-mir/
├── characters.json          # 槽位索引：名字/职业/等级/最后游玩时间/高风险标记
├── slots/
│   ├── 01J.../                # 槽位目录名 = 生成的唯一 ID（不用角色名，避开重名和非法文件名）
│   │   ├── save.json
│   │   └── save.json.bak
│   └── 01K.../
└── logs/
```

`characters.json` 是**派生缓存**，只用于快速渲染角色列表（不用加载所有存档）。它如果丢了或损坏，应该能通过扇描 `slots/` 重建 —— **不要把它当成权威数据源**。

### 启动流程

```
启动 → 角色列表
        ├─ 选择已有角色 → 加载 → 离线收益结算 → 进入游戏
        ├─ 创建新角色 → 职业 / 性别 / 名字 / 高风险模式 → 进入游戏
        └─ 删除角色 → 二次确认（输入角色名）→ 删除槽位目录
```

角色创建时要锁定三个不可变属性：**职业、性别（ADR-006）、高风险模式（ADR-002）**。

### 离线收益的关键约束

> ⚠️ **只对载入的角色结算离线收益。** 其他槽位的 `last_saved_at` 不推进、不计算收益。
>
> 否则玩家开十个角色挂一夜，等于十倍挂机收益，平衡直接崩。具体做法：**加载时**才读 `last_saved_at` 算 elapsed，算完立即更新为当前时间。未加载的槽位存档文件根本不被打开。

### 写入策略

- 格式：JSON（带 `version` 字段）
- 频率：每 30 秒 + 退出时 + 重大事件后（升级、拿到稀有装备）
- **原子写**：先写 `save.json.tmp`，`fsync`，再 `rename` 覆盖。挂机游戏开着好几个小时，崩溃 / 断电时写坏存档是真实风险。
- 保留 `save.json.bak`，加载失败时自动回退
- `version` 不匹配时走迁移函数链，不要直接拒绍加载
- 每个槽位有自己独立的 RNG 状态
