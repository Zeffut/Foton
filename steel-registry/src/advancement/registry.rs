//! The registry the generated advancement definitions land in.
//!
//! Vanilla parity: `ServerAdvancementManager`'s map of loaded advancements.
//! Like loot tables, advancements are not part of the registry set that is
//! synced to the client during configuration -- they travel over
//! `ClientboundUpdateAdvancementsPacket` instead -- so this is a plain
//! key-to-entry registry with no tag support.

use rustc_hash::FxHashMap;
use steel_utils::Identifier;

use super::Advancement;

/// A borrowed reference to a generated advancement.
pub type AdvancementRef = &'static Advancement;

/// Every advancement the built-in datapack defines.
pub struct AdvancementRegistry {
    advancements_by_id: Vec<AdvancementRef>,
    advancements_by_key: FxHashMap<Identifier, usize>,
    allows_registering: bool,
}

impl AdvancementRegistry {
    /// An empty registry, open for registration.
    #[must_use]
    pub fn new() -> Self {
        Self {
            advancements_by_id: Vec::new(),
            advancements_by_key: FxHashMap::default(),
            allows_registering: true,
        }
    }

    /// Adds one advancement, returning the id it took.
    ///
    /// # Panics
    /// If the registry is frozen, or two advancements share a key. A duplicate
    /// key would silently shadow one of the two definitions.
    pub fn register(&mut self, advancement: AdvancementRef) -> usize {
        assert!(
            self.allows_registering,
            "Cannot register advancements after the registry has been frozen"
        );
        assert!(
            !self.advancements_by_key.contains_key(&advancement.key),
            "Duplicate advancement key {}",
            advancement.key
        );

        let id = self.advancements_by_id.len();
        self.advancements_by_key.insert(advancement.key.clone(), id);
        self.advancements_by_id.push(advancement);
        id
    }

    /// Every advancement, in registration order.
    pub fn iter(&self) -> impl Iterator<Item = AdvancementRef> + '_ {
        self.advancements_by_id.iter().copied()
    }
}

impl Default for AdvancementRegistry {
    fn default() -> Self {
        Self::new()
    }
}

crate::impl_registry!(
    AdvancementRegistry,
    Advancement,
    advancements_by_id,
    advancements_by_key,
    advancements
);
