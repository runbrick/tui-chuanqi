//! 静态表结构。字段与 `doc/04-data-schema.md` 对齐，并带 ADR-005 的版本溯源。

use crate::ids::{ItemId, MapId, MonsterId, NpcId, SkillId};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 闭区间。RON 里写成 `(min, max)`。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "(i32, i32)", into = "(i32, i32)")]
pub struct Range {
    /// 下界（含）。
    pub min: i32,
    /// 上界（含）。
    pub max: i32,
}

impl From<(i32, i32)> for Range {
    fn from(value: (i32, i32)) -> Self {
        Self {
            min: value.0,
            max: value.1,
        }
    }
}

impl From<Range> for (i32, i32) {
    fn from(value: Range) -> Self {
        (value.min, value.max)
    }
}

impl Range {
    /// 构造区间。
    pub fn new(min: i32, max: i32) -> Self {
        Self { min, max }
    }

    /// `min <= max`。
    pub fn is_ordered(self) -> bool {
        self.min <= self.max
    }

    /// 在闭区间内均匀抽样。`min > max` 时退回 `min`，避免 panic。
    pub fn roll(self, rng: &mut impl Rng) -> i32 {
        if self.min >= self.max {
            self.min
        } else {
            rng.gen_range(self.min..=self.max)
        }
    }

    /// 区间中点，用于“打不打得过”的粗估，不参与正式伤害。
    pub fn midpoint(self) -> i32 {
        self.min.saturating_add(self.max) / 2
    }
}

/// 浮点闭区间，技能倍率用。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(from = "(f32, f32)", into = "(f32, f32)")]
pub struct FRange {
    /// 下界。
    pub min: f32,
    /// 上界。
    pub max: f32,
}

impl From<(f32, f32)> for FRange {
    fn from(value: (f32, f32)) -> Self {
        Self {
            min: value.0,
            max: value.1,
        }
    }
}

impl From<FRange> for (f32, f32) {
    fn from(value: FRange) -> Self {
        (value.min, value.max)
    }
}

impl FRange {
    /// `min <= max`。
    pub fn is_ordered(self) -> bool {
        self.min <= self.max
    }

    /// 在闭区间内抽样。
    pub fn roll(self, rng: &mut impl Rng) -> f32 {
        if self.min >= self.max {
            self.min
        } else {
            rng.gen_range(self.min..=self.max)
        }
    }
}

/// 内容版本溯源（ADR-005）。数据文件里写 `"1.67"` 或 `"1.76"`。
/// RON 枚举标识符不能带点，所以这里用字符串。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VersionOrigin {
    /// 1.67 主体。
    #[serde(rename = "1.67")]
    V167,
    /// 1.76 顶级内容（赤月 / 牛魔等）。
    #[serde(rename = "1.76")]
    V176,
}

/// 职业。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Class {
    /// 战士，主属性攻击。
    Warrior,
    /// 法师，主属性魔法。
    Mage,
    /// 道士，主属性道术。
    Taoist,
}

impl Class {
    /// 界面用中文名。
    pub fn label(self) -> &'static str {
        match self {
            Self::Warrior => "战士",
            Self::Mage => "法师",
            Self::Taoist => "道士",
        }
    }
}

/// 装备职业限制。`Any` 表示三职业都能穿。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClassReq {
    /// 不限职业。
    Any,
    /// 仅战士。
    Warrior,
    /// 仅法师。
    Mage,
    /// 仅道士。
    Taoist,
}

impl ClassReq {
    /// 该职业能否穿戴。
    pub fn allows(self, class: Class) -> bool {
        match self {
            Self::Any => true,
            Self::Warrior => class == Class::Warrior,
            Self::Mage => class == Class::Mage,
            Self::Taoist => class == Class::Taoist,
        }
    }

    /// 界面用中文。
    pub fn label(self) -> &'static str {
        match self {
            Self::Any => "通用",
            Self::Warrior => "战士",
            Self::Mage => "法师",
            Self::Taoist => "道士",
        }
    }
}

/// 角色性别（ADR-006）。创建后不可改。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Gender {
    /// 男。
    Male,
    /// 女。
    Female,
}

impl Gender {
    /// 界面用中文名。
    pub fn label(self) -> &'static str {
        match self {
            Self::Male => "男",
            Self::Female => "女",
        }
    }
}

/// 衣服性别限制。非衣服装备忽略此字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GenderReq {
    /// 不限性别。
    #[default]
    Any,
    /// 仅男性。
    Male,
    /// 仅女性。
    Female,
}

impl GenderReq {
    /// 该性别能否穿。
    pub fn allows(self, gender: Gender) -> bool {
        match self {
            Self::Any => true,
            Self::Male => gender == Gender::Male,
            Self::Female => gender == Gender::Female,
        }
    }

    /// 界面用中文。
    pub fn label(self) -> &'static str {
        match self {
            Self::Any => "不限",
            Self::Male => "男",
            Self::Female => "女",
        }
    }
}

/// 装备品质色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Quality {
    /// 白色普通。
    White,
    /// 绿色。
    Green,
    /// 蓝色。
    Blue,
    /// 紫色。
    Purple,
    /// 橙色顶级。
    Orange,
}

/// 怪物攻击走哪条减伤。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttackType {
    /// 物理近战，吃防御。
    Melee,
    /// 物理远程，吃防御。
    Ranged,
    /// 法术，吃魔防，且不判定命中。
    Magic,
    /// 毒素，吃魔防。
    Poison,
}

impl AttackType {
    /// 法术类不走命中判定。
    pub fn is_spell(self) -> bool {
        matches!(self, Self::Magic | Self::Poison)
    }

    /// 用魔防而不是防御减伤。
    pub fn uses_magic_defense(self) -> bool {
        self.is_spell()
    }
}

/// 技能伤害取哪条属性。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DamageSource {
    /// 攻击（DC）。
    Attack,
    /// 魔法（MC）。
    Magic,
    /// 道术（SC）。
    Taoism,
}

/// 装备槽。顺序即存档数组下标，不要重排。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EquipSlot {
    /// 武器。
    Weapon,
    /// 衣服。
    Armor,
    /// 头盔。
    Helmet,
    /// 项链。
    Necklace,
    /// 左手镯。
    BraceletL,
    /// 右手镯。
    BraceletR,
    /// 左戒指。
    RingL,
    /// 右戒指。
    RingR,
    /// 腰带。
    Belt,
    /// 靴子。
    Boots,
    /// 宝石 / 符。
    Charm,
}

impl EquipSlot {
    /// 全部槽位，界面按这个顺序画。
    pub const ALL: [EquipSlot; 11] = [
        Self::Weapon,
        Self::Armor,
        Self::Helmet,
        Self::Necklace,
        Self::BraceletL,
        Self::BraceletR,
        Self::RingL,
        Self::RingR,
        Self::Belt,
        Self::Boots,
        Self::Charm,
    ];

    /// 槽位数量。
    pub const COUNT: usize = Self::ALL.len();

    /// 中文名。
    pub fn label(self) -> &'static str {
        match self {
            Self::Weapon => "武器",
            Self::Armor => "衣服",
            Self::Helmet => "头盔",
            Self::Necklace => "项链",
            Self::BraceletL => "左手镯",
            Self::BraceletR => "右手镯",
            Self::RingL => "左戒指",
            Self::RingR => "右戒指",
            Self::Belt => "腰带",
            Self::Boots => "靴子",
            Self::Charm => "宝石",
        }
    }

    /// 数组下标。
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|slot| *slot == self).unwrap_or(0)
    }

    /// 手镯、戒指可以左右互换。
    pub fn accepts(self, item_slot: EquipSlot) -> bool {
        match (self, item_slot) {
            (Self::BraceletL | Self::BraceletR, Self::BraceletL | Self::BraceletR) => true,
            (Self::RingL | Self::RingR, Self::RingL | Self::RingR) => true,
            (a, b) => a == b,
        }
    }
}

/// 怪物定义。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonsterDef {
    /// slug。
    pub id: String,
    /// 显示名。
    pub name: String,
    /// 版本溯源。
    pub version_origin: VersionOrigin,
    /// 等级。
    pub level: u16,
    /// 生命。
    pub hp: i32,
    /// 攻击区间。
    pub attack: Range,
    /// 防御区间。
    pub defense: Range,
    /// 魔防区间。
    pub magic_defense: Range,
    /// 准确。
    pub accuracy: i32,
    /// 敏捷。
    pub agility: i32,
    /// 攻击间隔（毫秒）。
    pub attack_interval_ms: u32,
    /// 击杀经验。
    pub exp: u64,
    /// 不死系。施毒、圣言类效果会看这个标记。
    pub undead: bool,
    /// 攻击类型。
    pub attack_type: AttackType,
    /// 独立掉落表。
    pub drops: Vec<DropEntry>,
    #[serde(skip, default)]
    pub runtime_id: MonsterId,
}

/// 一条掉落。各项独立 roll，不是互斥权重。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DropEntry {
    /// 物品 slug（加载期）。解析后见 [`Self::item_id`]。
    pub item: String,
    /// 0 到 1。
    pub chance: f32,
    /// 数量下界。
    pub min: u32,
    /// 数量上界。
    pub max: u32,
    #[serde(skip, default)]
    pub item_id: ItemId,
}

/// 物品定义。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemDef {
    /// slug。
    pub id: String,
    /// 显示名。
    pub name: String,
    /// 版本溯源。
    pub version_origin: VersionOrigin,
    /// 品质。
    pub quality: Quality,
    /// 商店基准价。
    pub price: u32,
    /// 能否堆叠。
    pub stackable: bool,
    /// 种类。
    pub kind: ItemKind,
    #[serde(skip, default)]
    pub runtime_id: ItemId,
}

/// 物品种类。RON 写成 `Weapon((...))` 这种枚举元组。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ItemKind {
    /// 武器。
    Weapon(GearDef),
    /// 衣服。会校验性别。
    Armor(GearDef),
    /// 头盔。
    Helmet(GearDef),
    /// 首饰（项链 / 手镯 / 戒指 / 腰带 / 靴 / 宝石）。
    Accessory(GearDef),
    /// 药水等消耗品。
    Consumable(ConsumableDef),
    /// 矿石等材料。
    Material(MaterialDef),
    /// 技能书。
    SkillBook(SkillBookDef),
    /// 金币。掉落直接进钱包，不进背包。
    Currency,
    /// 幸运符。武器升级失败时消耗以保住武器。
    LuckCharm,
}

impl ItemKind {
    /// 可穿戴则返回装备定义。
    pub fn gear(&self) -> Option<&GearDef> {
        match self {
            Self::Weapon(g) | Self::Armor(g) | Self::Helmet(g) | Self::Accessory(g) => Some(g),
            _ => None,
        }
    }

    /// 是不是衣服（性别校验只对衣服）。
    pub fn is_armor(&self) -> bool {
        matches!(self, Self::Armor(_))
    }
}

/// 一件装备的数值。缺省字段在 RON 里可以省略。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GearDef {
    /// 穿在哪个槽。
    pub slot: EquipSlot,
    /// 需求等级。
    #[serde(default)]
    pub req_level: u16,
    /// 需求职业。
    #[serde(default = "default_any_class")]
    pub req_class: ClassReq,
    /// 性别限制。非衣服忽略。
    #[serde(default)]
    pub gender: GenderReq,
    /// 攻击。
    #[serde(default)]
    pub attack: Range,
    /// 魔法。
    #[serde(default)]
    pub magic: Range,
    /// 道术。
    #[serde(default)]
    pub taoism: Range,
    /// 防御。
    #[serde(default)]
    pub defense: Range,
    /// 魔防。
    #[serde(default)]
    pub magic_defense: Range,
    /// 准确。
    #[serde(default)]
    pub accuracy: i32,
    /// 敏捷。
    #[serde(default)]
    pub agility: i32,
    /// 生命加成。
    #[serde(default)]
    pub hp: i32,
    /// 魔法加成。
    #[serde(default)]
    pub mp: i32,
    /// 最大耐久。0 表示不会损坏。
    #[serde(default)]
    pub durability: u32,
    /// 能否武器升级。
    #[serde(default)]
    pub upgradable: bool,
}

fn default_any_class() -> ClassReq {
    ClassReq::Any
}

/// 消耗品。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumableDef {
    /// 回复生命总量。
    #[serde(default)]
    pub heal_hp: i32,
    /// 回复魔法总量。
    #[serde(default)]
    pub heal_mp: i32,
    /// `true` 时立刻回满本次数值；`false` 时按 tick 缓慢回复。
    #[serde(default = "default_true")]
    pub instant: bool,
    /// 缓慢回复持续的 tick 数。`instant = true` 时忽略。
    #[serde(default)]
    pub duration_ticks: u32,
}

fn default_true() -> bool {
    true
}

/// 材料。黑铁纯度在掉落 / 购买时 roll 一次，记在物品堆上。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialDef {
    /// 纯度区间。
    pub purity: Range,
}

/// 技能书。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillBookDef {
    /// 技能 slug。
    pub skill: String,
    /// 使用需求等级。
    pub req_level: u16,
    /// 使用需求职业，必须和技能职业一致。
    pub req_class: ClassReq,
    #[serde(skip, default)]
    pub skill_id: SkillId,
}

/// 技能定义。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDef {
    /// slug。
    pub id: String,
    /// 显示名。
    pub name: String,
    /// 版本溯源。
    pub version_origin: VersionOrigin,
    /// 所属职业。
    pub class: Class,
    /// 最高等级，必须等于 `levels` 长度。
    pub max_level: u8,
    /// 每一级的消耗和倍率。
    pub levels: Vec<SkillLevelDef>,
    /// 效果。
    pub effect: SkillEffect,
    #[serde(skip, default)]
    pub runtime_id: SkillId,
}

/// 技能某一级。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillLevelDef {
    /// 学习 / 升级所需角色等级。
    pub req_level: u16,
    /// 施放耗蓝。被动技能为 0。
    pub mp_cost: i32,
    /// 冷却毫秒。0 表示每拍都能放。除了烈火这类蓄力，施法都会占掉这一拍普攻。
    pub cooldown_ms: u32,
    /// 效果倍率区间。
    pub power: FRange,
}

/// 技能效果。位移和组队技能改写成挂机用的被动替代。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SkillEffect {
    /// 单体伤害。
    Damage(DamageEffect),
    /// 范围伤害。单怪战斗里仍打当前目标，倍率用 power。
    AoeDamage(DamageEffect),
    /// 治疗。
    Heal(HealEffect),
    /// 给下一次普攻附加倍率（烈火）。
    BuffNextAttack(BuffNextAttackEffect),
    /// 一段时间内加属性。
    BuffStats(BuffStatsEffect),
    /// 降低目标防御。
    DebuffDefense(DebuffDefenseEffect),
    /// 持续伤害。
    Poison(PoisonEffect),
    /// 召唤物。
    Summon(SummonEffect),
    /// 麻痹：推迟怪物下次出手。
    Paralyze(ParalyzeEffect),
    /// 被动。命中或普攻概率加成。
    Passive(PassiveEffect),
}

/// 直接伤害。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DamageEffect {
    /// 用哪条属性 roll 伤害。
    pub source: DamageSource,
    /// 目标数量描述，目前单怪循环里只打当前怪。
    #[serde(default = "default_single")]
    pub targets: TargetCount,
    /// 为 true 时伤害再乘 power 区间。
    #[serde(default = "default_true")]
    pub multiplier_from_power: bool,
    /// 对不死系的倍率。地狱雷光这类克制不死的法术填大于 1。
    #[serde(default = "default_unit")]
    pub undead_bonus: f32,
}

fn default_unit() -> f32 {
    1.0
}

fn default_single() -> TargetCount {
    TargetCount::Single
}

/// 目标数量。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetCount {
    /// 单体。
    Single,
    /// 范围（当前地图同时只有一只怪，效果上仍是这一只）。
    Area,
}

/// 治疗。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealEffect {
    /// 治疗量基准属性。
    pub source: DamageSource,
}

/// 烈火：下一击附加。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuffNextAttackEffect {
    /// 附加伤害的属性。
    pub source: DamageSource,
}

/// 属性增益，持续若干 tick。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BuffStatsEffect {
    /// 持续 tick。
    pub duration_ticks: u32,
    /// 额外准确。
    #[serde(default)]
    pub accuracy: i32,
    /// 伤害减免比例 0..1，魔法盾用。
    #[serde(default)]
    pub damage_reduction: f32,
    /// 额外敏捷。
    #[serde(default)]
    pub agility: i32,
}

/// 降防。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebuffDefenseEffect {
    /// 持续 tick。
    pub duration_ticks: u32,
    /// 防御上下限各减少的比例。
    pub defense_pct: f32,
}

/// 毒和火墙这类持续伤害。每秒（10 tick）跳一次，不吃防御。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoisonEffect {
    /// 持续 tick。按 10 tick 一跳，所以 80 就是 8 跳。
    pub duration_ticks: u32,
    /// 每跳伤害 = 来源属性中值 * ratio * power。
    pub tick_ratio: f32,
    /// 对不死系额外倍率。
    #[serde(default = "default_undead_bonus")]
    pub undead_bonus: f32,
    /// 按哪条属性算。施毒术吃道术，火墙吃魔法。
    #[serde(default = "default_poison_source")]
    pub source: DamageSource,
}

fn default_undead_bonus() -> f32 {
    1.5
}

fn default_poison_source() -> DamageSource {
    DamageSource::Taoism
}

/// 召唤。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummonEffect {
    /// 宝宝显示名 slug，日志里用技能名。
    pub pet: String,
    /// 同时存在上限。
    pub max_count: u8,
    /// 宝宝强度是否吃道术。
    pub scale_from_taoism: bool,
    /// 宝宝基础生命。
    pub base_hp: i32,
    /// 宝宝基础攻击。
    pub base_attack: Range,
    /// 宝宝攻击间隔毫秒。
    pub attack_interval_ms: u32,
    /// 存活 tick，到点消失。
    pub duration_ticks: u64,
}

/// 麻痹。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParalyzeEffect {
    /// 推迟怪物出手的 tick。
    pub delay_ticks: u64,
}

/// 被动。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PassiveEffect {
    /// 常驻准确。
    #[serde(default)]
    pub accuracy: i32,
    /// 普攻时额外一段伤害的概率（攻杀）。
    #[serde(default)]
    pub proc_chance: f32,
    /// 触发时的额外倍率。
    #[serde(default)]
    pub proc_power: f32,
}

/// 地图。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapDef {
    /// slug。
    pub id: String,
    /// 显示名。
    pub name: String,
    /// 版本溯源。
    pub version_origin: VersionOrigin,
    /// 进入等级。
    pub req_level: u16,
    /// 前置地图击杀数。`None` 表示没有前置。
    pub req_map_kills: Option<(String, u64)>,
    /// 刷怪间隔毫秒。安全区可为 0。
    pub spawn_interval_ms: u32,
    /// 安全区不刷怪，用来休息和购物。
    pub safe_zone: bool,
    /// 刷怪权重表。
    pub spawns: Vec<SpawnEntry>,
    /// 可选 BOSS。
    pub boss: Option<BossDef>,
    #[serde(skip, default)]
    pub runtime_id: MapId,
    #[serde(skip, default)]
    pub req_map_id: Option<MapId>,
}

/// 普通怪权重。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnEntry {
    /// 怪物 slug。
    pub monster: String,
    /// 权重，必须 > 0。
    pub weight: u32,
    #[serde(skip, default)]
    pub monster_id: MonsterId,
}

/// BOSS 刷新规则。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BossDef {
    /// 怪物 slug。
    pub monster: String,
    /// 击杀后再次出现的毫秒。
    pub respawn_ms: u32,
    /// 本地图小怪击杀数达到后才出现。
    pub kills_to_unlock: u64,
    #[serde(skip, default)]
    pub monster_id: MonsterId,
}

/// NPC。挂机里是功能入口。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpcDef {
    /// slug。
    pub id: String,
    /// 显示名。
    pub name: String,
    /// 版本溯源。
    pub version_origin: VersionOrigin,
    /// 所在地图 slug。
    pub map: String,
    /// 提供的服务。
    pub services: Vec<NpcService>,
    /// 打招呼。
    pub greeting: String,
    #[serde(skip, default)]
    pub runtime_id: NpcId,
    #[serde(skip, default)]
    pub map_id: MapId,
}

/// NPC 服务。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NpcService {
    /// 商店。
    Shop(ShopService),
    /// 修理。
    Repair(RepairService),
    /// 仓库。
    Storage(StorageService),
}

/// 商店。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShopService {
    /// 出售的物品 slug。
    pub stock: Vec<String>,
    /// 买入倍率（相对 price）。
    pub buy_markup: f32,
    /// 卖出倍率。
    pub sell_rate: f32,
    #[serde(skip, default)]
    pub stock_ids: Vec<ItemId>,
}

/// 修理。
///
/// 填了 `price_ratio` 时，修满一件的价钱 = 物品售价 * 比例 * 损失比例，至少 1 金币。
/// 没填就退回旧规则：损失耐久 * `cost_per_durability`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairService {
    /// 每点耐久的金币。只在没填 `price_ratio` 时用。
    pub cost_per_durability: u32,
    /// 修满一件相当于售价的多少。
    #[serde(default)]
    pub price_ratio: Option<f32>,
}

/// 仓库。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageService {
    /// 格子数。
    pub slots: u16,
}

/// 经验曲线 `round(base * level^exponent)`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpCurve {
    /// 基数。
    pub base: f64,
    /// 指数。
    pub exponent: f64,
}

/// 命中公式系数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HitChanceConfig {
    /// 基础命中。
    pub base: f64,
    /// 每点准确差。
    pub per_point: f64,
    /// 下限。
    pub min: f64,
    /// 上限。
    pub max: f64,
}

/// 自然回复。每隔 `interval_ms` 回一次最大值的百分比，休息时不叠加。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegenConfig {
    /// 间隔毫秒。0 表示关闭。
    pub interval_ms: u32,
    /// 每次回复最大生命的比例。
    pub hp_ratio: f64,
    /// 每次回复最大魔法的比例。
    pub mp_ratio: f64,
}

/// 新角色自带的东西。装备能穿就直接穿上，穿不上（比如性别不符）就跳过；
/// 其它物品放进背包。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartingItem {
    /// 物品 slug。
    pub item: String,
    /// 数量。
    #[serde(default = "default_one")]
    pub count: u32,
}

fn default_one() -> u32 {
    1
}

/// 一个职业的 1 级裸属性和每级成长。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassBaseline {
    /// 1 级生命。
    pub hp: i32,
    /// 1 级魔法。
    pub mp: i32,
    /// 1 级攻击。
    pub attack: Range,
    /// 1 级魔法。
    pub magic: Range,
    /// 1 级道术。
    pub taoism: Range,
    /// 1 级防御。
    pub defense: Range,
    /// 1 级魔防。
    pub magic_defense: Range,
    /// 1 级准确。
    pub accuracy: i32,
    /// 1 级敏捷。
    pub agility: i32,
    /// 普攻间隔毫秒。
    pub attack_interval_ms: u32,
    /// 每级成长，乘以 `level - 1`。
    pub growth: ClassGrowth,
}

/// 每级增量。攻击上下限各加这么多。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassGrowth {
    /// 生命。
    pub hp: i32,
    /// 魔法。
    pub mp: i32,
    /// 攻击下 / 上限。
    pub attack: Range,
    /// 魔法下 / 上限。
    pub magic: Range,
    /// 道术下 / 上限。
    pub taoism: Range,
    /// 防御下 / 上限。
    pub defense: Range,
    /// 魔防下 / 上限。
    pub magic_defense: Range,
    /// 准确。
    pub accuracy: i32,
    /// 敏捷。
    pub agility: i32,
}

/// 三职业表。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassTable {
    /// 战士。
    pub warrior: ClassBaseline,
    /// 法师。
    pub mage: ClassBaseline,
    /// 道士。
    pub taoist: ClassBaseline,
}

impl ClassTable {
    /// 按职业取表。
    pub fn get(&self, class: Class) -> &ClassBaseline {
        match class {
            Class::Warrior => &self.warrior,
            Class::Mage => &self.mage,
            Class::Taoist => &self.taoist,
        }
    }
}

/// 可调平衡参数。公式系数都在这里，不散落在代码里。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceConfig {
    /// 经验曲线。
    pub exp_curve: ExpCurve,
    /// 命中。
    pub hit_chance: HitChanceConfig,
    /// 死亡损失当前等级经验条的比例。
    pub death_penalty_exp_pct: f64,
    /// 死亡后休息 tick。
    pub death_rest_ticks: u64,
    /// 离线收益累计上限（小时）。
    pub offline_cap_hours: u32,
    /// 武器升级成功率，下标 = 当前强化等级。超出取最后一档。
    pub upgrade_success_rates: Vec<f64>,
    /// 第 n 次升级（从 1 计）的金币 = base * n。
    pub upgrade_gold_base: u32,
    /// 自动喝药：生命低于此比例。
    pub potion_hp_ratio: f64,
    /// 自动喝药：魔法低于此比例。
    pub potion_mp_ratio: f64,
    /// 无药且生命低于此比例则收工。
    pub retreat_hp_ratio: f64,
    /// 高风险模式掉率倍率（ADR-002）。
    pub high_risk_drop_multiplier: f64,
    /// 背包格数。
    pub inventory_slots: u16,
    /// 默认仓库格数（没有仓库 NPC 时）。
    pub storage_slots: u16,
    /// 新建角色金币。
    pub starting_gold: u64,
    /// 技能升到下一级所需熟练度 = base * 当前等级。
    pub skill_exp_base: u32,
    /// 等级达到后自动喝药可粗调（25/50/75）。
    pub auto_coarse_level: u16,
    /// 等级达到后自动喝药可 1% 精调。
    pub auto_fine_level: u16,
    /// 等级达到后解锁自动推进地图。
    pub auto_advance_level: u16,
    /// 职业成长。
    pub classes: ClassTable,
    /// 等级上限。
    pub max_level: u16,
    /// 自然回复。缺省关闭。
    #[serde(default)]
    pub regen: RegenConfig,
    /// 新角色自带物品。只影响新建角色，不改旧存档。
    #[serde(default)]
    pub starting_items: Vec<StartingItem>,
    /// 同一级里连死这么多次就退回前置地图，升级后再自动往前推。0 表示不退。
    #[serde(default = "default_fallback_deaths")]
    pub fallback_after_deaths: u8,
}

fn default_fallback_deaths() -> u8 {
    3
}

impl BalanceConfig {
    /// `level` 升到下一级所需经验。
    pub fn exp_to_next(&self, level: u16) -> u64 {
        if level >= self.max_level {
            return u64::MAX / 4;
        }
        let raw = self.exp_curve.base * (level as f64).powf(self.exp_curve.exponent);
        raw.round().max(1.0) as u64
    }

    /// 升级成功率。
    pub fn upgrade_rate(&self, current_plus: u8) -> f64 {
        if self.upgrade_success_rates.is_empty() {
            return 0.0;
        }
        let idx = (current_plus as usize).min(self.upgrade_success_rates.len() - 1);
        self.upgrade_success_rates[idx]
    }

    /// 保底阈值：期望次数的 3 倍，仅 `chance < 0.01` 启用。
    pub fn pity_threshold(chance: f32) -> Option<u32> {
        if chance <= 0.0 || chance >= 0.01 {
            return None;
        }
        Some((3.0 / f64::from(chance)).ceil() as u32)
    }
}

/// 全部静态数据。校验通过后核心层可以按 ID 直接索引。
#[derive(Debug, Clone)]
pub struct GameData {
    /// 怪物，下标即 [`MonsterId`]。
    pub monsters: Vec<MonsterDef>,
    /// 物品。
    pub items: Vec<ItemDef>,
    /// 技能。
    pub skills: Vec<SkillDef>,
    /// 地图。
    pub maps: Vec<MapDef>,
    /// NPC。
    pub npcs: Vec<NpcDef>,
    /// 平衡。
    pub balance: BalanceConfig,
    pub(crate) monster_index: HashMap<String, MonsterId>,
    pub(crate) item_index: HashMap<String, ItemId>,
    pub(crate) skill_index: HashMap<String, SkillId>,
    pub(crate) map_index: HashMap<String, MapId>,
    pub(crate) npc_index: HashMap<String, NpcId>,
}

impl GameData {
    /// 按 slug 查怪。
    pub fn monster_id(&self, slug: &str) -> Option<MonsterId> {
        self.monster_index.get(slug).copied()
    }

    /// 按 slug 查物品。
    pub fn item_id(&self, slug: &str) -> Option<ItemId> {
        self.item_index.get(slug).copied()
    }

    /// 按 slug 查技能。
    pub fn skill_id(&self, slug: &str) -> Option<SkillId> {
        self.skill_index.get(slug).copied()
    }

    /// 按 slug 查地图。
    pub fn map_id(&self, slug: &str) -> Option<MapId> {
        self.map_index.get(slug).copied()
    }

    /// 按 slug 查 NPC。
    pub fn npc_id(&self, slug: &str) -> Option<NpcId> {
        self.npc_index.get(slug).copied()
    }

    /// 怪物定义。
    pub fn monster(&self, id: MonsterId) -> &MonsterDef {
        &self.monsters[id.0 as usize]
    }

    /// 物品定义。
    pub fn item(&self, id: ItemId) -> &ItemDef {
        &self.items[id.0 as usize]
    }

    /// 技能定义。
    pub fn skill(&self, id: SkillId) -> &SkillDef {
        &self.skills[id.0 as usize]
    }

    /// 地图定义。
    pub fn map(&self, id: MapId) -> &MapDef {
        &self.maps[id.0 as usize]
    }

    /// NPC 定义。
    pub fn npc(&self, id: NpcId) -> &NpcDef {
        &self.npcs[id.0 as usize]
    }

    /// 金币物品。没有则掉落金币项会被跳过并在校验阶段报错。
    pub fn gold_item(&self) -> Option<ItemId> {
        self.items
            .iter()
            .find(|item| matches!(item.kind, ItemKind::Currency))
            .map(|item| item.runtime_id)
    }

    /// 起始挂机地图：等级 1、无前置、非安全区、有刷怪。
    pub fn starting_map(&self) -> Option<MapId> {
        self.maps
            .iter()
            .find(|map| {
                map.req_level <= 1
                    && map.req_map_id.is_none()
                    && !map.safe_zone
                    && !map.spawns.is_empty()
            })
            .map(|map| map.runtime_id)
            .or_else(|| {
                self.maps
                    .iter()
                    .find(|map| map.req_level <= 1 && map.req_map_id.is_none())
                    .map(|map| map.runtime_id)
            })
    }

    /// 安全区，死亡和收工传送到这里。优先选比奇城。
    pub fn safe_map(&self) -> Option<MapId> {
        self.maps
            .iter()
            .find(|map| map.safe_zone && map.id == "bichen_city")
            .or_else(|| self.maps.iter().find(|map| map.safe_zone))
            .map(|map| map.runtime_id)
    }

    pub(crate) fn finish(
        mut monsters: Vec<MonsterDef>,
        mut items: Vec<ItemDef>,
        mut skills: Vec<SkillDef>,
        mut maps: Vec<MapDef>,
        mut npcs: Vec<NpcDef>,
        balance: BalanceConfig,
    ) -> Self {
        let mut monster_index = HashMap::new();
        let mut item_index = HashMap::new();
        let mut skill_index = HashMap::new();
        let mut map_index = HashMap::new();
        let mut npc_index = HashMap::new();
        for (i, monster) in monsters.iter_mut().enumerate() {
            monster.runtime_id = MonsterId(i as u32);
            monster_index.insert(monster.id.clone(), monster.runtime_id);
        }
        for (i, item) in items.iter_mut().enumerate() {
            item.runtime_id = ItemId(i as u32);
            item_index.insert(item.id.clone(), item.runtime_id);
        }
        for (i, skill) in skills.iter_mut().enumerate() {
            skill.runtime_id = SkillId(i as u32);
            skill_index.insert(skill.id.clone(), skill.runtime_id);
        }
        for (i, map) in maps.iter_mut().enumerate() {
            map.runtime_id = MapId(i as u32);
            map_index.insert(map.id.clone(), map.runtime_id);
        }
        for (i, npc) in npcs.iter_mut().enumerate() {
            npc.runtime_id = NpcId(i as u32);
            npc_index.insert(npc.id.clone(), npc.runtime_id);
        }
        Self {
            monsters,
            items,
            skills,
            maps,
            npcs,
            balance,
            monster_index,
            item_index,
            skill_index,
            map_index,
            npc_index,
        }
    }
}
