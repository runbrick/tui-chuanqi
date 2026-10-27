//! 静态数据类型、RON 加载与交叉引用校验。
//!
//! 运行时内部使用 [`id`] 模块里的 newtype，数据文件里写字符串 slug。

mod error;
mod ids;
mod load;
mod model;
mod validate;

pub use error::{DataError, NamedSource};
pub use ids::{ItemId, MapId, MonsterId, NpcId, SkillId};
pub use model::*;
