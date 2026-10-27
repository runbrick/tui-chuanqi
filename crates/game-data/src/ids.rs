//! 运行时整数 ID。加载完成后 slug 只留在定义结构体里做反查。

use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! id_newtype {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub struct $name(pub u32);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

id_newtype!(
    /// 怪物表下标。
    MonsterId
);
id_newtype!(
    /// 物品表下标。
    ItemId
);
id_newtype!(
    /// 技能表下标。
    SkillId
);
id_newtype!(
    /// 地图表下标。
    MapId
);
id_newtype!(
    /// NPC 表下标。
    NpcId
);
