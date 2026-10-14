//! The last damage of each entity that no plugin handler heard.
//!
//! Paper records every hit that went through as the victim's `getLastDamageCause`, whether or not
//! a plugin listens to damage events, and a plugin may read it later -- in a death handler, say.
//! Building that event in Java for every hit costs a JNI crossing each; a nuke damages thousands
//! of dropped items in one tick. When no damage handler exists the facts wait here, and Java builds
//! the event only if a plugin asks for it.

use std::collections::hash_map::Entry;
use std::sync::LazyLock;

use foton_utils::locks::SyncMutex;
use rustc_hash::FxHashMap;
use uuid::Uuid;

/// The arguments `EventRelay.fireEntityDamage` takes, in its order.
pub(crate) type DamageArgs = [String; 6];

/// Paper forgets a last damage cause after this many ticks.
const KEPT_TICKS: i64 = 40;

/// How many records may wait before stale ones are swept.
const SWEEP_AT: usize = 4096;

static UNHEARD: LazyLock<SyncMutex<FxHashMap<Uuid, (i64, DamageArgs)>>> =
    LazyLock::new(|| SyncMutex::new(FxHashMap::default()));

/// Keeps a hit nobody heard as the entity's last damage.
pub(crate) fn remember(entity: Uuid, tick: i64, args: DamageArgs) {
    let mut unheard = UNHEARD.lock();
    if unheard.len() >= SWEEP_AT {
        unheard.retain(|_, (at, _)| tick - *at <= KEPT_TICKS);
    }
    unheard.insert(entity, (tick, args));
}

/// Drops what is kept for an entity: it was heard, replaced by a plugin, or is gone.
pub(crate) fn forget(entity: Uuid) {
    UNHEARD.lock().remove(&entity);
}

/// Hands over the kept hit, once: Java turns it into the event it records.
pub(crate) fn take(entity: Uuid) -> Option<(i64, DamageArgs)> {
    match UNHEARD.lock().entry(entity) {
        Entry::Occupied(entry) => Some(entry.remove()),
        Entry::Vacant(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(cause: &str) -> DamageArgs {
        [
            String::new(),
            String::new(),
            cause.to_owned(),
            "1".to_owned(),
            String::new(),
            "0".to_owned(),
        ]
    }

    #[test]
    fn a_kept_hit_is_handed_over_once_and_the_latest_wins() {
        let entity = Uuid::from_u128(0x5eed);
        remember(entity, 10, args("FIRE"));
        remember(entity, 12, args("BLOCK_EXPLOSION"));
        let (tick, kept) = take(entity).expect("kept");
        assert_eq!(tick, 12);
        assert_eq!(kept[2], "BLOCK_EXPLOSION");
        assert!(take(entity).is_none());
    }

    #[test]
    fn a_full_store_sweeps_hits_older_than_paper_keeps() {
        let base = 0x7000_0000_u128;
        for index in 0..SWEEP_AT as u128 {
            remember(Uuid::from_u128(base + index), 0, args("FIRE"));
        }
        let fresh = Uuid::from_u128(base + SWEEP_AT as u128);
        remember(fresh, KEPT_TICKS + 1, args("LAVA"));
        assert!(take(Uuid::from_u128(base)).is_none());
        assert!(take(fresh).is_some());
    }
}
