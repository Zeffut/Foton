//! The half of a vault the client is told about.
//!
//! Vanilla parity:
//! `net.minecraft.world.level.block.entity.vault.VaultSharedData`.

use std::sync::Arc;

use foton_registry::item_stack::ItemStack;
use foton_utils::BlockPos;
use foton_utils::nbt::NbtNumeric as _;
use foton_utils::uuid_ext::{load_uuid_list, save_uuid_list};
use simdnbt::borrow::NbtCompound as NbtCompoundView;
use simdnbt::owned::NbtCompound;
use uuid::Uuid;

use super::config::{DEFAULT_DEACTIVATION_RANGE, VaultConfig};
use super::server_data::VaultServerData;
use crate::world::World;

/// What the vault shows and who it is connected to.
pub struct VaultSharedData {
    display_item: ItemStack,
    connected_players: Vec<Uuid>,
    connected_particles_range: f64,
    pub(super) is_dirty: bool,
}

impl Default for VaultSharedData {
    fn default() -> Self {
        Self {
            display_item: ItemStack::empty(),
            connected_players: Vec::new(),
            connected_particles_range: DEFAULT_DEACTIVATION_RANGE,
            is_dirty: false,
        }
    }
}

impl VaultSharedData {
    /// Vanilla parity: `VaultSharedData.getDisplayItem`.
    #[must_use]
    pub const fn display_item(&self) -> &ItemStack {
        &self.display_item
    }

    /// Vanilla parity: `VaultSharedData.hasDisplayItem`.
    #[must_use]
    pub fn has_display_item(&self) -> bool {
        !self.display_item.is_empty()
    }

    /// Vanilla parity: `VaultSharedData.setDisplayItem`.
    pub fn set_display_item(&mut self, stack: ItemStack) {
        if ItemStack::matches(&self.display_item, &stack) {
            return;
        }
        self.display_item = stack;
        self.is_dirty = true;
    }

    /// Vanilla parity: `VaultSharedData.hasConnectedPlayers`.
    #[must_use]
    pub const fn has_connected_players(&self) -> bool {
        !self.connected_players.is_empty()
    }

    /// Vanilla parity: `VaultSharedData.getConnectedPlayers`.
    #[must_use]
    pub fn connected_players(&self) -> &[Uuid] {
        &self.connected_players
    }

    /// Refreshes who the vault is connected to.
    ///
    /// Vanilla parity: `VaultSharedData.updateConnectedPlayersWithinRange`. A
    /// player who has already been paid is filtered out here, which is what
    /// makes the vault go dark for them alone.
    pub fn update_connected_players_within_range(
        &mut self,
        world: &Arc<World>,
        pos: BlockPos,
        server_data: &VaultServerData,
        config: &VaultConfig,
        limit: f64,
    ) {
        let current: Vec<Uuid> = config
            .player_detector
            .detect(world, pos, limit, false)
            .into_iter()
            .filter(|uuid| !server_data.has_rewarded_player(*uuid))
            .collect();
        if self.connected_players == current {
            return;
        }
        self.connected_players = current;
        self.is_dirty = true;
    }

    /// Vanilla parity: `VaultSharedData.CODEC` on the way in.
    pub fn load(&mut self, nbt: &NbtCompoundView<'_, '_>) {
        self.display_item = nbt
            .compound("display_item")
            .and_then(|compound| ItemStack::from_borrowed_compound(&compound))
            .unwrap_or_else(ItemStack::empty);
        self.connected_players = load_uuid_list(nbt.list("connected_players").as_ref());
        self.connected_particles_range = nbt
            .get("connected_particles_range")
            .and_then(|tag| tag.codec_f64())
            .unwrap_or(DEFAULT_DEACTIVATION_RANGE);
    }

    /// Vanilla parity: `VaultSharedData.CODEC` on the way out.
    #[must_use]
    pub fn save(&self) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        if !self.display_item.is_empty() {
            nbt.insert("display_item", self.display_item.to_nbt_tag_ref());
        }
        nbt.insert("connected_players", save_uuid_list(&self.connected_players));
        nbt.insert("connected_particles_range", self.connected_particles_range);
        nbt
    }
}
