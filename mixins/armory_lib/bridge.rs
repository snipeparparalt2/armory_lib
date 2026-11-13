// Armory Lib's bridge into the client library crate, where the mobs tick: the entity types
// daylight must not set alight. The library's `[api]` package writes the list (it reaches
// this module as `horizon_client::armory_lib_bridge`); `sunburn.rs` reads it on the server
// thread, once for each undead mob each tick, so reads take a shared lock on a short list.
use std::sync::RwLock;

static NO_SUNBURN: RwLock<Vec<String>> = RwLock::new(Vec::new());

/// Replaces the list of entity types (`mymod:shade`) that daylight doesn't light.
pub fn set_no_sunburn(kinds: Vec<String>) {
    let mut list = NO_SUNBURN.write().unwrap_or_else(|e| e.into_inner());
    if *list != kinds {
        *list = kinds;
    }
}

/// Whether daylight leaves entity type `kind` unlit.
pub fn no_sunburn(kind: &str) -> bool {
    let list = NO_SUNBURN.read().unwrap_or_else(|e| e.into_inner());
    list.iter().any(|k| k == kind)
}
