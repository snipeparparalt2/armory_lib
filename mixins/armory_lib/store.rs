// What Armory Lib keeps for a world, and small item helpers, added to the game as the module
// `game::armory_lib_store`. Files live in `<world>/armory_lib/` (Horizon's `native_world_dir`),
// so the world's backups carry them. A file is written whole through a temporary file and a
// rename, so a crash leaves the old one or the new one, never half of one.
use crate::game::Game;
use horizon_client::items::ItemStack;
use horizon_world::nbt::Tag;
use std::path::{Path, PathBuf};

/// `<world>/armory_lib/<rel>`; None with no world open (a client of a server).
pub(super) fn path(game: &Game<'_>, rel: &str) -> Option<PathBuf> {
    Some(game.native_world_dir()?.join("armory_lib").join(rel))
}

/// The slots `write_items` saved at `path`, `size` of them; empty ones where the file has none
/// (or there is no file).
pub(super) fn read_items(path: &Path, size: usize) -> Vec<Option<ItemStack>> {
    let tag = horizon_client::save::read_dat(path);
    horizon_items::item_nbt::decode_items(tag.as_ref().and_then(|t| t.get("Items")), size)
}

/// Saves `slots` at `path` (making its folder), as a chest saves its items.
pub(super) fn write_items(path: &Path, slots: &[Option<ItemStack>]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let tag = Tag::compound([("Items", horizon_items::item_nbt::encode_items(slots))]);
    horizon_client::save::write_dat_durable(path, tag).map_err(|e| format!("{}: {e}", path.display()))
}

/// A JSON file of the library's, or None when there is none or it doesn't parse.
#[allow(dead_code)] // used by the Batch B features
pub(super) fn read_json(path: &Path) -> Option<serde_json::Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// Writes a JSON file whole (making its folder).
#[allow(dead_code)] // used by the Batch B features
pub(super) fn write_json(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    horizon_client::save::write_durable(path, &bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// The value under `key` in the stack's `minecraft:custom_data`.
pub(super) fn custom<'a>(stack: &'a ItemStack, key: &str) -> Option<&'a Tag> {
    stack.custom_data().and_then(|data| {
        data.iter().find(|(k, _)| k.to_string_lossy() == key).map(|(_, v)| v)
    })
}

/// A text value of the stack's `minecraft:custom_data`.
pub(super) fn custom_text(stack: &ItemStack, key: &str) -> Option<String> {
    match custom(stack, key)? {
        Tag::String(s) => Some(s.to_string_lossy()),
        _ => None,
    }
}

/// An int value of the stack's `minecraft:custom_data`.
pub(super) fn custom_int(stack: &ItemStack, key: &str) -> Option<i32> {
    match custom(stack, key)? {
        Tag::Int(n) => Some(*n),
        _ => None,
    }
}

/// Sets (or with None removes) `key` in the stack's `minecraft:custom_data`; the component goes
/// when it is left empty.
pub(super) fn set_custom(stack: &mut ItemStack, key: &str, value: Option<Tag>) {
    let mut data = stack.custom_data().cloned().unwrap_or_default();
    data.retain(|k, _| k.to_string_lossy() != key);
    if let Some(value) = value {
        data.insert(key.into(), value);
    }
    let extra = stack.extra.get_or_insert_with(Default::default);
    extra.set_other(
        "minecraft:custom_data",
        (!data.is_empty()).then_some(Tag::Compound(data)),
    );
}

/// A fresh random id (16 hex digits), for something the library keeps by id.
pub(super) fn new_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos()));
    format!("{:016x}", h.finish())
}
