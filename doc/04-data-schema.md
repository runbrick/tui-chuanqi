# 静态数据表结构

所有传奇内容走 RON 文件，**代码里不写死任何怪物 / 装备 / 技能**。这样内容修正不需要重编译，也方便你后期按照更准确的 1.67 资料整体替换。

## 通用约定

- ID 用**字符串 slug**（`"woma_guard"`、`"black_iron_sword"`），不用数字。好处：数据文件可读、增删不用重排、diff 友好。
- 加载时 slug → 索引，运行时内部用 `MonsterId(u32)` 等 newtype，查表 O(1)。
- 区间统一写成 `(min, max)`。
- 概率用 `f32`，`0.0 ~ 1.0`。
- 所有文本字段用中文（游戏内直接显示）。

## `monsters.ron`

```ron
[
    (
        id: "chicken",
        name: "鸡",
        level: 1,
        hp: 12,
        attack: (0, 2),
        defense: (0, 1),
        magic_defense: (0, 0),
        accuracy: 2,
        agility: 4,
        attack_interval_ms: 1800,
        exp: 3,
        undead: false,              // 道士的"施毒/圣言"对不死系有特殊效果
        attack_type: Melee,
        drops: [
            (item: "gold", chance: 0.6, min: 1, max: 5),
        ],
    ),
    (
        id: "woma_guard",
        name: "沃玛卫士",
        level: 28,
        hp: 700,
        attack: (28, 52),
        defense: (12, 20),
        magic_defense: (4, 10),
        accuracy: 12,
        agility: 10,
        attack_interval_ms: 1400,
        exp: 1250,
        undead: false,
        attack_type: Melee,
        drops: [
            (item: "gold",              chance: 1.0,   min: 150, max: 400),
            (item: "potion_hp_large",   chance: 0.25,  min: 1,   max: 3),
            (item: "woma_helmet",       chance: 0.015, min: 1,   max: 1),
            (item: "black_iron_ore",    chance: 0.08,  min: 1,   max: 2),
        ],
    ),
]
```

`attack_type` 枚举：`Melee` / `Ranged` / `Magic` / `Poison`。用来决定走 AC 还是 MAC 减伤。

## `items.ron`

物品用枚举区分种类，RON 的枚举变体写法让这个很干净：

```ron
[
    (
        id: "black_iron_sword",
        name: "乌木剑",
        quality: Green,                 // White / Green / Blue / Purple / Orange
        price: 8000,
        stackable: false,
        kind: Weapon((
            slot: Weapon,
            req_level: 25,
            req_class: Any,             // Any / Warrior / Mage / Taoist
            attack: (10, 25),
            magic: (0, 0),
            taoism: (0, 0),
            accuracy: 1,
            agility: 0,
            durability: 10000,
            upgradable: true,
        )),
    ),
    (
        id: "potion_hp_large",
        name: "金创药(大)",
        quality: White,
        price: 300,
        stackable: true,
        kind: Consumable((
            heal_hp: 180,
            heal_mp: 0,
            instant: true,              // 传奇的药是持续回，instant=false 时按 tick 回
            duration_ticks: 30,
        )),
    ),
    (
        id: "black_iron_ore",
        name: "黑铁矿石",
        quality: White,
        price: 50,
        stackable: true,
        kind: Material((
            purity: (1, 15),            // 黑铁纯度，武器升级用
        )),
    ),
    (
        id: "book_fire_ball",
        name: "火球术(书)",
        quality: Blue,
        price: 2000,
        stackable: false,
        kind: SkillBook((
            skill: "fire_ball",
            req_level: 7,
            req_class: Mage,
        )),
    ),
]
```

装备槽位（对齐原作）：
`Weapon`（武器）、`Armor`（衣服）、`Helmet`（头盔）、`Necklace`（项链）、`BraceletL` / `BraceletR`（手镯 ×2）、`RingL` / `RingR`（戒指 ×2）、`Belt`（腰带）、`Boots`（鞋）、`Charm`（宝石 / 符）

> ⚠️ 1.67 是否已有腰带 / 鞋 / 宝石槽位需要核实，早期版本槽位更少。见 `06-content-reference.md`。

## `skills.ron`

```ron
[
    (
        id: "fire_ball",
        name: "火球术",
        class: Mage,
        max_level: 3,
        levels: [
            (req_level: 7,  mp_cost: 3, cooldown_ms: 1600, power: (1.0, 1.3)),
            (req_level: 10, mp_cost: 4, cooldown_ms: 1500, power: (1.2, 1.5)),
            (req_level: 13, mp_cost: 5, cooldown_ms: 1400, power: (1.4, 1.8)),
        ],
        effect: Damage((
            source: Magic,              // 用 MC 算伤害
            targets: Single,
            multiplier_from_power: true,
        )),
    ),
    (
        id: "fire_sword",
        name: "烈火剑法",
        class: Warrior,
        max_level: 3,
        levels: [
            (req_level: 19, mp_cost: 7, cooldown_ms: 0, power: (2.0, 2.4)),
            (req_level: 22, mp_cost: 6, cooldown_ms: 0, power: (2.4, 2.8)),
            (req_level: 25, mp_cost: 5, cooldown_ms: 0, power: (2.8, 3.2)),
        ],
        effect: BuffNextAttack((   // 原作是"蓄力后下一击附加烈火伤害"
            source: Attack,
        )),
    ),
    (
        id: "summon_skeleton",
        name: "召唤骷髅",
        class: Taoist,
        max_level: 3,
        levels: [
            (req_level: 19, mp_cost: 20, cooldown_ms: 30000, power: (1.0, 1.0)),
            (req_level: 22, mp_cost: 20, cooldown_ms: 30000, power: (1.3, 1.3)),
            (req_level: 25, mp_cost: 20, cooldown_ms: 30000, power: (1.6, 1.6)),
        ],
        effect: Summon((
            pet: "skeleton",
            max_count: 1,
            scale_from_taoism: true,
        )),
    ),
]
```

`effect` 枚举需要覆盖的类型：
`Damage` / `AoeDamage` / `Heal` / `BuffNextAttack` / `BuffStats` / `DebuffDefense` / `Poison` / `Summon` / `Paralyze`

## `maps.ron`

```ron
[
    (
        id: "woma_temple_3",
        name: "沃玛寺庙 三层",
        req_level: 28,
        req_map_kills: ("woma_temple_2", 300),   // 前置：二层击杀 300
        spawn_interval_ms: 2000,
        safe_zone: false,                         // 安全区不刷怪，可用于休息
        spawns: [
            (monster: "woma_guard",   weight: 70),
            (monster: "woma_warrior", weight: 25),
            (monster: "woma_leader",  weight: 5),   // 精英
        ],
        boss: Some((
            monster: "woma_lord",
            respawn_ms: 600000,        // BOSS 刷新间隔
            kills_to_unlock: 500,      // 击杀多少小怪后 BOSS 才出现
        )),
    ),
]
```

## `npcs.ron`

NPC 在挂机游戏里退化成"功能入口"（商店 / 仓库 / 技能书 / 修理），但保留原作名字和位置提升代入感：

```ron
[
    (
        id: "bichen_weapon_shop",
        name: "武器店老板",
        map: "bichen_city",
        services: [
            Shop(( stock: ["wooden_sword", "short_sword", "iron_sword"], buy_markup: 1.0, sell_rate: 0.3 )),
            Repair(( cost_per_durability: 2 )),
        ],
        greeting: "想买把好剑？",
    ),
    (
        id: "bichen_storage",
        name: "仓库管理员",
        map: "bichen_city",
        services: [ Storage(( slots: 50 )) ],
        greeting: "东西放我这里，丢不了。",
    ),
]
```

## `balance.ron`

见 `03-game-design.md` 末尾。所有公式系数、经验曲线、离线上限、升级成功率都在这里。

## 加载时校验（`game-data` 必须实现）

启动时一次性全查，有任何问题直接报错退出 —— 把数据错误挡在运行时之外：

- [ ] 所有 ID 唯一
- [ ] 掉落表引用的 `item` 存在
- [ ] 地图 `spawns` / `boss` 引用的 `monster` 存在
- [ ] 地图 `req_map_kills` 引用的地图存在，且**地图依赖图无环**
- [ ] 技能书引用的 `skill` 存在，且 `req_class` 与技能的 `class` 一致
- [ ] NPC 商店 `stock` 引用的物品存在
- [ ] 技能 `levels` 长度 == `max_level`，且 `req_level` 单调递增
- [ ] 所有区间 `min <= max`
- [ ] 所有概率在 `[0, 1]`
- [ ] 存在至少一张 `req_level: 1` 且无前置的起始地图

错误信息要带上下文：`"monsters.ron: 'woma_guard' 的掉落表引用了不存在的物品 'woma_helmt'（是否想写 'woma_helmet'？）"`。数据文件会越来越大，好的错误信息能省掉大量调试时间。
