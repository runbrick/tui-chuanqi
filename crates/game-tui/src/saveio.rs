//! 多槽位存档。角色身份是槽位目录，不是名字。
//!
//! 离线收益只在 `load` 时对这一个槽位结算，其它槽位的文件不会被打开。

use game_core::{state_from_json, state_to_json, GameState, OfflineReport};
use game_data::{Class, GameData, Gender};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static SLOT_SEQ: AtomicU64 = AtomicU64::new(1);

/// 角色列表上的一行。索引损坏时可以从槽位重建。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CharacterMeta {
    /// 槽位目录名。
    pub id: String,
    /// 名字，允许重名。
    pub name: String,
    /// warrior / mage / taoist。
    pub class: String,
    /// male / female。
    pub gender: String,
    /// 等级。
    pub level: u16,
    /// 高风险。
    pub high_risk: bool,
    /// 上次保存的 Unix 秒。未载入的角色保持不动。
    pub last_saved_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct IndexFile {
    characters: Vec<CharacterMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Envelope {
    version: u32,
    last_saved_at: u64,
    state: serde_json::Value,
}

/// 一个存档根目录。
pub struct SaveStore {
    root: PathBuf,
}

impl SaveStore {
    /// 打开或创建存档根目录。
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, String> {
        let root = root.into();
        fs::create_dir_all(root.join("slots")).map_err(|err| err.to_string())?;
        Ok(Self { root })
    }

    /// 根目录。
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 角色列表。索引坏了就扫 `slots/`。
    pub fn list(&self) -> Result<Vec<CharacterMeta>, String> {
        let path = self.root.join("characters.json");
        if path.is_file() {
            if let Ok(text) = fs::read_to_string(&path) {
                if let Ok(index) = serde_json::from_str::<IndexFile>(&text) {
                    return Ok(index.characters);
                }
            }
        }
        self.rebuild_index()
    }

    /// 创建角色。职业、性别、高风险之后不能改。
    pub fn create(
        &self,
        data: &GameData,
        name: &str,
        class: Class,
        gender: Gender,
        high_risk: bool,
    ) -> Result<String, String> {
        game_core::validate_name(name)?;
        let id = new_slot_id();
        let seed = unix_now() ^ id_seed(&id);
        let state = GameState::new(name, class, gender, high_risk, data, seed);
        self.write_state(&id, &state, data, unix_now())?;
        Ok(id)
    }

    /// 载入并结算离线。其它槽位不读、不写。
    pub fn load(&self, id: &str, data: &GameData) -> Result<(GameState, OfflineReport), String> {
        let (mut state, last) = self.read_state(id, data)?;
        let now = unix_now();
        let elapsed = now.saturating_sub(last);
        let report = state.simulate_offline(data, Duration::from_secs(elapsed));
        self.write_state(id, &state, data, now)?;
        Ok((state, report))
    }

    /// 保存当前角色，并把它的最后游玩时间改成现在。
    pub fn save(&self, id: &str, state: &GameState, data: &GameData) -> Result<(), String> {
        self.write_state(id, state, data, unix_now())
    }

    /// 删除槽位目录。不可恢复。
    pub fn delete(&self, id: &str) -> Result<(), String> {
        let dir = self.slot_dir(id);
        if dir.is_dir() {
            fs::remove_dir_all(&dir).map_err(|err| err.to_string())?;
        }
        self.rebuild_index()?;
        Ok(())
    }

    fn write_state(
        &self,
        id: &str,
        state: &GameState,
        data: &GameData,
        now: u64,
    ) -> Result<(), String> {
        let json = state_to_json(state, data)?;
        let state_value: serde_json::Value =
            serde_json::from_str(&json).map_err(|err| err.to_string())?;
        let envelope = Envelope {
            version: 1,
            last_saved_at: now,
            state: state_value,
        };
        let text = serde_json::to_string_pretty(&envelope).map_err(|err| err.to_string())?;
        let dir = self.slot_dir(id);
        fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        atomic_write(&dir.join("save.json"), text.as_bytes())?;
        self.upsert_meta(&meta_from(id, state, now))?;
        Ok(())
    }

    fn read_state(&self, id: &str, data: &GameData) -> Result<(GameState, u64), String> {
        let dir = self.slot_dir(id);
        let main = dir.join("save.json");
        let bak = dir.join("save.json.bak");
        let text = fs::read_to_string(&main)
            .or_else(|_| fs::read_to_string(&bak))
            .map_err(|err| err.to_string())?;
        parse_envelope(&text, data).or_else(|err| {
            let backup = fs::read_to_string(&bak).map_err(|_| err.clone())?;
            parse_envelope(&backup, data)
        })
    }

    fn upsert_meta(&self, meta: &CharacterMeta) -> Result<(), String> {
        let mut list = self.list().unwrap_or_default();
        if let Some(slot) = list.iter_mut().find(|item| item.id == meta.id) {
            *slot = meta.clone();
        } else {
            list.push(meta.clone());
        }
        self.write_index(&list)
    }

    fn rebuild_index(&self) -> Result<Vec<CharacterMeta>, String> {
        let mut list = Vec::new();
        let slots = self.root.join("slots");
        if slots.is_dir() {
            let mut entries: Vec<_> = fs::read_dir(&slots)
                .map_err(|err| err.to_string())?
                .collect();
            entries.sort_by_key(|entry| entry.as_ref().ok().map(|e| e.file_name()));
            for entry in entries.into_iter().flatten() {
                let id = entry.file_name().to_string_lossy().to_string();
                let main = entry.path().join("save.json");
                let bak = entry.path().join("save.json.bak");
                let text = fs::read_to_string(&main)
                    .or_else(|_| fs::read_to_string(&bak))
                    .ok();
                if let Some(text) = text {
                    if let Ok(envelope) = serde_json::from_str::<Envelope>(&text) {
                        if let Some(meta) = meta_from_value(&id, &envelope) {
                            list.push(meta);
                        }
                    }
                }
            }
        }
        self.write_index(&list)?;
        Ok(list)
    }

    fn write_index(&self, characters: &[CharacterMeta]) -> Result<(), String> {
        let index = IndexFile {
            characters: characters.to_vec(),
        };
        let text = serde_json::to_string_pretty(&index).map_err(|err| err.to_string())?;
        atomic_write(&self.root.join("characters.json"), text.as_bytes())
    }

    fn slot_dir(&self, id: &str) -> PathBuf {
        self.root.join("slots").join(id)
    }
}

fn parse_envelope(text: &str, data: &GameData) -> Result<(GameState, u64), String> {
    let envelope: Envelope = serde_json::from_str(text).map_err(|err| err.to_string())?;
    let state_text = serde_json::to_string(&envelope.state).map_err(|err| err.to_string())?;
    let state = state_from_json(&state_text, data)?;
    Ok((state, envelope.last_saved_at))
}

fn meta_from(id: &str, state: &GameState, now: u64) -> CharacterMeta {
    CharacterMeta {
        id: id.to_string(),
        name: state.name.clone(),
        class: match state.class {
            Class::Warrior => "warrior",
            Class::Mage => "mage",
            Class::Taoist => "taoist",
        }
        .to_string(),
        gender: match state.gender {
            Gender::Male => "male",
            Gender::Female => "female",
        }
        .to_string(),
        level: state.level,
        high_risk: state.high_risk,
        last_saved_at: now,
    }
}

fn meta_from_value(id: &str, envelope: &Envelope) -> Option<CharacterMeta> {
    let state = &envelope.state;
    Some(CharacterMeta {
        id: id.to_string(),
        name: state.get("name")?.as_str()?.to_string(),
        class: state.get("class")?.as_str()?.to_string(),
        gender: state.get("gender")?.as_str()?.to_string(),
        level: state.get("level")?.as_u64()? as u16,
        high_risk: state.get("high_risk")?.as_bool()?,
        last_saved_at: envelope.last_saved_at,
    })
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    {
        let mut file = File::create(&tmp).map_err(|err| err.to_string())?;
        file.write_all(bytes).map_err(|err| err.to_string())?;
        file.sync_all().map_err(|err| err.to_string())?;
    }
    if path.exists() {
        let bak = path.with_extension("json.bak");
        let _ = fs::copy(path, &bak);
        fs::remove_file(path).map_err(|err| err.to_string())?;
    }
    fs::rename(&tmp, path).map_err(|err| err.to_string())?;
    Ok(())
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn new_slot_id() -> String {
    let seq = SLOT_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{:x}-{:x}", unix_now(), seq)
}

fn id_seed(id: &str) -> u64 {
    id.bytes().fold(0u64, |acc, byte| {
        acc.wrapping_mul(131).wrapping_add(u64::from(byte))
    })
}
