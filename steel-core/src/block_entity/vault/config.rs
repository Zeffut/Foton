//! What one vault wants and what it pays.
//!
//! Vanilla parity:
//! `net.minecraft.world.level.block.entity.vault.VaultConfig`.

use simdnbt::borrow::NbtCompound as NbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_registry::item_stack::ItemStack;
use steel_registry::loot_table::LootTableRef;
use steel_registry::{REGISTRY, RegistryExt as _, vanilla_items, vanilla_loot_tables};
use steel_utils::Identifier;
use steel_utils::nbt::NbtNumeric as _;

use crate::block_entity::trialspawner::PlayerDetector;

/// Vanilla parity: the `4.0` of the private `VaultConfig` constructor.
pub const DEFAULT_ACTIVATION_RANGE: f64 = 4.0;
/// Vanilla parity: the `4.5` of the same constructor.
pub const DEFAULT_DEACTIVATION_RANGE: f64 = 4.5;

/// One vault's tuning.
///
/// Vanilla parity: `VaultConfig`. Its `entitySelector` field is left out for
/// the same reason as the trial spawner's: it exists so a game test can hand
/// the detector a fixed list, and Steel has no such harness.
#[derive(Clone, Debug)]
pub struct VaultConfig {
    /// The table a successful unlock rolls.
    pub loot_table: LootTableRef,
    /// How close a player has to come for the vault to wake up.
    pub activation_range: f64,
    /// And how far they have to go for it to sleep again.
    pub deactivation_range: f64,
    /// What the vault takes -- an empty stack means it takes nothing.
    pub key_item: ItemStack,
    /// A table to show off instead of the one it pays from.
    pub override_loot_table_to_display: Option<LootTableRef>,
    /// Which players the vault counts as standing nearby.
    pub player_detector: PlayerDetector,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            loot_table: &vanilla_loot_tables::CHESTS_TRIAL_CHAMBERS_REWARD,
            activation_range: DEFAULT_ACTIVATION_RANGE,
            deactivation_range: DEFAULT_DEACTIVATION_RANGE,
            key_item: ItemStack::new(&vanilla_items::TRIAL_KEY),
            override_loot_table_to_display: None,
            player_detector: PlayerDetector::IncludingCreativePlayers,
        }
    }
}

impl VaultConfig {
    /// Reads a saved configuration.
    ///
    /// Vanilla parity: `VaultConfig.CODEC`, whose validator refuses an
    /// activation range above the deactivation range. A saved vault that fails
    /// that check keeps the default pair rather than being dropped, because
    /// dropping it would take the reward ledger with it.
    #[must_use]
    pub fn load(nbt: &NbtCompoundView<'_, '_>) -> Self {
        let default = Self::default();
        let loot_table = loot_table_by_name(nbt, "loot_table").unwrap_or(default.loot_table);
        let activation_range = nbt
            .get("activation_range")
            .and_then(|tag| tag.codec_f64())
            .unwrap_or(default.activation_range);
        let deactivation_range = nbt
            .get("deactivation_range")
            .and_then(|tag| tag.codec_f64())
            .unwrap_or(default.deactivation_range);
        let key_item = nbt
            .compound("key_item")
            .and_then(|compound| ItemStack::from_borrowed_compound(&compound))
            .unwrap_or(default.key_item);

        let (activation_range, deactivation_range) = if activation_range > deactivation_range {
            log::warn!(
                "vault activation range {activation_range} is above its deactivation range \
                 {deactivation_range}; using the defaults"
            );
            (
                Self::default().activation_range,
                Self::default().deactivation_range,
            )
        } else {
            (activation_range, deactivation_range)
        };

        Self {
            loot_table,
            activation_range,
            deactivation_range,
            key_item,
            override_loot_table_to_display: loot_table_by_name(
                nbt,
                "override_loot_table_to_display",
            ),
            player_detector: default.player_detector,
        }
    }

    /// Writes this configuration.
    #[must_use]
    pub fn save(&self) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.insert("loot_table", self.loot_table.key.to_string());
        nbt.insert("activation_range", self.activation_range);
        nbt.insert("deactivation_range", self.deactivation_range);
        nbt.insert("key_item", self.key_item.to_nbt_tag_ref());
        if let Some(display) = self.override_loot_table_to_display {
            nbt.insert("override_loot_table_to_display", display.key.to_string());
        }
        nbt
    }
}

fn loot_table_by_name(nbt: &NbtCompoundView<'_, '_>, name: &str) -> Option<LootTableRef> {
    let key: Identifier = nbt.string(name)?.to_str().parse().ok()?;
    REGISTRY.loot_tables.by_key(&key)
}
