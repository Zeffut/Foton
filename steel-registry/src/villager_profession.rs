use rustc_hash::FxHashMap;
use steel_utils::Identifier;

use crate::items::ItemRef;
use crate::sound_event::SoundEventRef;
use crate::{vanilla_items, vanilla_villager_professions};

#[derive(Debug)]
pub struct VillagerProfession {
    pub key: Identifier,
    pub work_sound: Option<SoundEventRef>,
}

/// Vanilla parity: the `requestedItems` of `VillagerProfession.FARMER`, the one
/// profession `VillagerProfession.bootstrap` hands a non-empty set.
fn farmer_requested_items() -> [ItemRef; 4] {
    [
        &vanilla_items::WHEAT,
        &vanilla_items::WHEAT_SEEDS,
        &vanilla_items::BEETROOT_SEEDS,
        &vanilla_items::BONE_MEAL,
    ]
}

impl VillagerProfession {
    /// Whether this profession has its villager pick `item` up off the ground.
    ///
    /// Vanilla parity: `VillagerProfession.requestedItems().contains(item)`.
    ///
    /// `requestedItems` is a literal `ImmutableSet` written into
    /// `VillagerProfession.bootstrap`. No datapack can change it, no packet
    /// carries it and `SteelExtractor` emits nothing for it, so the vanilla
    /// source is mirrored here the way [`crate::fuel`] mirrors the equally
    /// hardcoded `FuelValues.vanillaBurnTimes`.
    #[must_use]
    pub fn requests_item(&self, item: ItemRef) -> bool {
        self.key == vanilla_villager_professions::FARMER.key
            && farmer_requested_items()
                .iter()
                .any(|requested| requested.key == item.key)
    }
}

pub type VillagerProfessionRef = &'static VillagerProfession;

pub struct VillagerProfessionRegistry {
    villager_professions_by_id: Vec<VillagerProfessionRef>,
    villager_professions_by_key: FxHashMap<Identifier, usize>,
    allows_registering: bool,
}

impl VillagerProfessionRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            villager_professions_by_id: Vec::new(),
            villager_professions_by_key: FxHashMap::default(),
            allows_registering: true,
        }
    }
}

crate::impl_standard_methods!(
    VillagerProfessionRegistry,
    VillagerProfessionRef,
    villager_professions_by_id,
    villager_professions_by_key,
    allows_registering
);

crate::impl_registry!(
    VillagerProfessionRegistry,
    VillagerProfession,
    villager_professions_by_id,
    villager_professions_by_key,
    villager_professions
);
