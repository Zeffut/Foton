//! The half of a vault only the server sees.
//!
//! Vanilla parity:
//! `net.minecraft.world.level.block.entity.vault.VaultServerData`.

use simdnbt::borrow::NbtCompound as NbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_registry::item_stack::ItemStack;
use steel_utils::uuid_ext::{load_uuid_list, save_uuid_list};
use uuid::Uuid;

/// Vanilla parity: `VaultServerData.MAX_REWARD_PLAYERS`.
const MAX_REWARD_PLAYERS: usize = 128;

/// The ledger and the queue behind one vault.
#[derive(Default)]
pub struct VaultServerData {
    rewarded_players: Vec<Uuid>,
    state_updating_resumes_at: i64,
    items_to_eject: Vec<ItemStack>,
    last_insert_fail_timestamp: i64,
    total_ejections_needed: i32,
    pub(super) is_dirty: bool,
}

impl VaultServerData {
    /// Vanilla parity: `VaultServerData.getLastInsertFailTimestamp`.
    #[must_use]
    pub const fn last_insert_fail_timestamp(&self) -> i64 {
        self.last_insert_fail_timestamp
    }

    /// Vanilla parity: `VaultServerData.setLastInsertFailTimestamp`.
    pub const fn set_last_insert_fail_timestamp(&mut self, timestamp: i64) {
        self.last_insert_fail_timestamp = timestamp;
    }

    /// Vanilla parity: `VaultServerData.getRewardedPlayers`.
    #[must_use]
    pub fn rewarded_players(&self) -> &[Uuid] {
        &self.rewarded_players
    }

    /// Vanilla parity: `VaultServerData.hasRewardedPlayer`.
    #[must_use]
    pub fn has_rewarded_player(&self, player: Uuid) -> bool {
        self.rewarded_players.contains(&player)
    }

    /// Records that a player has been paid.
    ///
    /// Vanilla parity: `VaultServerData.addToRewardedPlayers`, which drops the
    /// oldest entry once the ledger passes a hundred and twenty-eight. That cap
    /// is what stops a vault in a public server growing without bound; it also
    /// means the very first looter can eventually loot it again.
    pub fn add_to_rewarded_players(&mut self, player: Uuid) {
        if !self.rewarded_players.contains(&player) {
            self.rewarded_players.push(player);
        }
        if self.rewarded_players.len() > MAX_REWARD_PLAYERS {
            self.rewarded_players.remove(0);
        }
        self.mark_changed();
    }

    /// Vanilla parity: `VaultServerData.stateUpdatingResumesAt`.
    #[must_use]
    pub const fn state_updating_resumes_at(&self) -> i64 {
        self.state_updating_resumes_at
    }

    /// Vanilla parity: `VaultServerData.pauseStateUpdatingUntil`.
    pub const fn pause_state_updating_until(&mut self, resumes_at: i64) {
        self.state_updating_resumes_at = resumes_at;
        self.mark_changed();
    }

    /// Vanilla parity: `VaultServerData.getItemsToEject`.
    #[must_use]
    pub fn items_to_eject(&self) -> &[ItemStack] {
        &self.items_to_eject
    }

    /// Vanilla parity: `VaultServerData.markEjectionFinished`.
    pub const fn mark_ejection_finished(&mut self) {
        self.total_ejections_needed = 0;
        self.mark_changed();
    }

    /// Vanilla parity: `VaultServerData.setItemsToEject`.
    pub fn set_items_to_eject(&mut self, items: Vec<ItemStack>) {
        self.items_to_eject = items;
        self.total_ejections_needed = self.items_to_eject.len() as i32;
        self.mark_changed();
    }

    /// Vanilla parity: `VaultServerData.getNextItemToEject`, which peeks at the
    /// back of the list.
    #[must_use]
    pub fn next_item_to_eject(&self) -> ItemStack {
        self.items_to_eject
            .last()
            .map_or_else(ItemStack::empty, |item| item.copy_with_count(item.count()))
    }

    /// Vanilla parity: `VaultServerData.popNextItemToEject`.
    pub fn pop_next_item_to_eject(&mut self) -> ItemStack {
        match self.items_to_eject.pop() {
            Some(item) => {
                self.mark_changed();
                item
            }
            None => ItemStack::empty(),
        }
    }

    /// How far through the ejection the vault is, for the eject sound's pitch.
    ///
    /// Vanilla parity: `VaultServerData.ejectionProgress`.
    #[must_use]
    pub fn ejection_progress(&self) -> f32 {
        if self.total_ejections_needed == 1 {
            return 1.0;
        }
        let remaining = self.items_to_eject.len() as f32;
        let total = self.total_ejections_needed as f32;
        // Vanilla parity: `Mth.inverseLerp(remaining, 1.0F, total)`.
        1.0 - (remaining - 1.0) / (total - 1.0)
    }

    const fn mark_changed(&mut self) {
        self.is_dirty = true;
    }

    /// Vanilla parity: `VaultServerData.CODEC` on the way in.
    pub fn load(&mut self, nbt: &NbtCompoundView<'_, '_>) {
        self.rewarded_players = load_uuid_list(nbt.list("rewarded_players").as_ref());
        self.state_updating_resumes_at = nbt.long("state_updating_resumes_at").unwrap_or(0);
        self.items_to_eject = nbt
            .list("items_to_eject")
            .and_then(|list| list.compounds())
            .map(|compounds| {
                compounds
                    .into_iter()
                    .filter_map(|compound| ItemStack::from_borrowed_compound(&compound))
                    .collect()
            })
            .unwrap_or_default();
        self.total_ejections_needed = nbt.int("total_ejections_needed").unwrap_or(0);
    }

    /// Vanilla parity: `VaultServerData.CODEC` on the way out.
    #[must_use]
    pub fn save(&self) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.insert("rewarded_players", save_uuid_list(&self.rewarded_players));
        nbt.insert("state_updating_resumes_at", self.state_updating_resumes_at);
        nbt.insert(
            "items_to_eject",
            NbtList::Compound(
                self.items_to_eject
                    .iter()
                    .filter_map(|item| match item.to_nbt_tag_ref() {
                        NbtTag::Compound(compound) => Some(compound),
                        _ => None,
                    })
                    .collect(),
            ),
        );
        nbt.insert("total_ejections_needed", self.total_ejections_needed);
        nbt
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::{init_vanilla_registry, vanilla_items};

    use super::*;

    /// The ledger is the whole point of the vault: one reward per player. It
    /// also has to forget its oldest entry past a hundred and twenty-eight, or
    /// a public server's vault would grow without bound.
    #[test]
    fn the_reward_ledger_remembers_players_and_forgets_the_oldest() {
        let mut data = VaultServerData::default();
        let first = Uuid::from_u128(1);

        data.add_to_rewarded_players(first);
        assert!(data.has_rewarded_player(first));
        data.add_to_rewarded_players(first);
        assert_eq!(data.rewarded_players().len(), 1, "no double entry");

        for id in 2..=(MAX_REWARD_PLAYERS as u128 + 1) {
            data.add_to_rewarded_players(Uuid::from_u128(id));
        }
        assert_eq!(data.rewarded_players().len(), MAX_REWARD_PLAYERS);
        assert!(
            !data.has_rewarded_player(first),
            "the oldest looter is the one forgotten"
        );
    }

    /// The eject queue is drained from the back, and the pitch of each eject
    /// sound rides the progress. A single-item reward has to report one rather
    /// than divide by zero.
    #[test]
    fn ejection_progress_runs_from_start_to_finish() {
        init_vanilla_registry();
        let mut data = VaultServerData::default();
        data.set_items_to_eject(vec![
            ItemStack::new(&vanilla_items::DIAMOND),
            ItemStack::new(&vanilla_items::EMERALD),
            ItemStack::new(&vanilla_items::IRON_INGOT),
        ]);

        assert!(data.next_item_to_eject().is(&vanilla_items::IRON_INGOT));
        assert!((data.ejection_progress() - 0.0).abs() < f32::EPSILON);
        data.pop_next_item_to_eject();
        assert!((data.ejection_progress() - 0.5).abs() < f32::EPSILON);
        data.pop_next_item_to_eject();
        assert!((data.ejection_progress() - 1.0).abs() < f32::EPSILON);

        let mut single = VaultServerData::default();
        single.set_items_to_eject(vec![ItemStack::new(&vanilla_items::DIAMOND)]);
        assert!((single.ejection_progress() - 1.0).abs() < f32::EPSILON);
    }
}
