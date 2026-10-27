//! 两遍加载：先读入建 slug 表，再解析交叉引用并校验。

use crate::error::{DataError, NamedSource};
use crate::model::{
    BalanceConfig, GameData, ItemDef, ItemKind, MapDef, MonsterDef, NpcDef, NpcService, SkillDef,
};
use crate::validate::{resolve_refs, validate};
use std::fs;
use std::path::Path;

const FILES: [&str; 6] = [
    "monsters.ron",
    "items.ron",
    "skills.ron",
    "maps.ron",
    "npcs.ron",
    "balance.ron",
];

/// 从目录加载六张表。
pub fn load_dir(dir: &Path) -> Result<GameData, DataError> {
    let mut sources = Vec::with_capacity(FILES.len());
    for name in FILES {
        let path = dir.join(name);
        let text = fs::read_to_string(&path).map_err(|source| DataError::Io { path, source })?;
        sources.push(NamedSource::new(name, text));
    }
    load_sources(&sources)
}

/// 从命名文本加载。单元测试走这里，不碰文件系统。
pub fn load_sources(sources: &[NamedSource]) -> Result<GameData, DataError> {
    let text = |name: &str| -> Result<&str, DataError> {
        sources
            .iter()
            .find(|source| source.name == name)
            .map(|source| source.text.as_str())
            .ok_or_else(|| {
                DataError::validation(name, "*", format!("缺少数据文件 {name}"), "补上该文件")
            })
    };
    let monsters: Vec<MonsterDef> = parse("monsters.ron", text("monsters.ron")?)?;
    let items: Vec<ItemDef> = parse("items.ron", text("items.ron")?)?;
    let skills: Vec<SkillDef> = parse("skills.ron", text("skills.ron")?)?;
    let maps: Vec<MapDef> = parse("maps.ron", text("maps.ron")?)?;
    let npcs: Vec<NpcDef> = parse("npcs.ron", text("npcs.ron")?)?;
    let balance: BalanceConfig = parse("balance.ron", text("balance.ron")?)?;
    validate(&monsters, &items, &skills, &maps, &npcs, &balance)?;
    let mut data = GameData::finish(monsters, items, skills, maps, npcs, balance);
    resolve_refs(&mut data)?;
    Ok(data)
}

fn parse<T: serde::de::DeserializeOwned>(file: &str, text: &str) -> Result<T, DataError> {
    ron::from_str(text).map_err(|err| DataError::Parse {
        file: file.to_string(),
        message: err.to_string(),
    })
}

impl GameData {
    /// 从目录加载全部静态数据。
    pub fn load(dir: impl AsRef<Path>) -> Result<Self, DataError> {
        load_dir(dir.as_ref())
    }

    /// 从命名 RON 文本加载。测试专用。
    pub fn load_sources(sources: &[NamedSource]) -> Result<Self, DataError> {
        load_sources(sources)
    }
}

/// 供校验器枚举物品种类，避免循环依赖扩散。
pub(crate) fn skill_book_ref(kind: &ItemKind) -> Option<(&str, crate::model::ClassReq)> {
    match kind {
        ItemKind::SkillBook(book) => Some((book.skill.as_str(), book.req_class)),
        _ => None,
    }
}

/// 商店存货 slug。
pub(crate) fn shop_stock(service: &NpcService) -> Option<&[String]> {
    match service {
        NpcService::Shop(shop) => Some(&shop.stock),
        _ => None,
    }
}
