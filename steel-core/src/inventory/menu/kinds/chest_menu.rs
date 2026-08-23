//! Chest menu for chest-like containers (chests, barrels, ender chests, shulker boxes).
//!
//! 1-6 rows of 9 slots. Layout:
//! - Slots 0 to `rows * 9 - 1`: Container
//! - Slots `rows * 9` to `rows * 9 + 26`: Main inventory (27)
//! - Slots `rows * 9 + 27` to `rows * 9 + 35`: Hotbar (9)

use steel_registry::menu_type::MenuTypeRef;
use steel_registry::vanilla_menu_types;

use std::iter;

use crate::block_entity::BlockEntityBase;
use crate::entity::Entity as _;
use crate::inventory::prelude::*;
use crate::player::player_inventory::PlayerInventory;

/// Builds a chest-like menu with `rows` rows of 9 slots plus the player inventory.
///
/// # Panics
/// Panics if `rows` is 0 or greater than 6.
#[must_use]
pub fn chest(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    container: impl Into<ContainerRef>,
    rows: usize,
) -> Menu {
    let container = container.into();
    assert!(
        (1..=6).contains(&rows),
        "Chest rows must be between 1 and 6"
    );

    let mut builder = MenuBuilder::new(menu_type_for_rows(rows), container_id);
    let chest = builder.section(&container, rows * 9);
    let player = builder.player_inventory(&inventory);

    builder.route(chest, player.all(), FillDirection::Backward);
    builder.route(player.all(), chest, FillDirection::Forward);

    builder.build(ChestKind {
        container,
        second_container: None,
    })
}

/// Builds a double chest menu: two 27-slot halves presented as one 54-slot
/// container, plus the player inventory.
///
/// Vanilla parity: `ChestBlock` combines two `ChestBlockEntity` halves through
/// a `CompoundContainer`. Steel keeps the halves as two independently lockable
/// containers and joins them at the menu layer instead, which preserves the
/// per-container locking contract while exposing the same 54 logical slots.
#[must_use]
pub fn double_chest(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    first: impl Into<ContainerRef>,
    second: impl Into<ContainerRef>,
) -> Menu {
    let first = first.into();
    let second = second.into();

    let mut builder = MenuBuilder::new(&vanilla_menu_types::GENERIC_9X6, container_id);
    let upper = builder.section(&first, CHEST_HALF_SLOTS);
    let lower = builder.section(&second, CHEST_HALF_SLOTS);
    let player = builder.player_inventory(&inventory);

    builder.route([upper, lower], player.all(), FillDirection::Backward);
    builder.route(player.all(), [upper, lower], FillDirection::Forward);

    builder.build(ChestKind {
        container: first,
        second_container: Some(second),
    })
}

/// Slots in a single chest half.
const CHEST_HALF_SLOTS: usize = 27;

/// Menu type for a chest of `rows` rows.
///
/// # Panics
/// Panics if `rows` is 0 or greater than 6.
#[must_use]
pub fn menu_type_for_rows(rows: usize) -> MenuTypeRef {
    match rows {
        1 => &vanilla_menu_types::GENERIC_9X1,
        2 => &vanilla_menu_types::GENERIC_9X2,
        3 => &vanilla_menu_types::GENERIC_9X3,
        4 => &vanilla_menu_types::GENERIC_9X4,
        5 => &vanilla_menu_types::GENERIC_9X5,
        6 => &vanilla_menu_types::GENERIC_9X6,
        _ => panic!("Invalid row count: {rows}"),
    }
}

/// Per-menu chest state: the backing container(s) for the validity check.
pub struct ChestKind {
    /// The backing container. For a double chest, this is the upper half.
    container: ContainerRef,
    /// The lower half of a double chest, if any.
    second_container: Option<ContainerRef>,
}

// SAFETY: This Steel-owned key uniquely identifies the concrete menu kind
// within the process.
unsafe impl steel_utils::DowncastType for ChestKind {
    const TYPE_KEY: steel_utils::DowncastTypeKey =
        steel_utils::DowncastTypeKey::new("steel:menu/chest");
}

impl ChestKind {
    /// Runs `f` on the block entity behind each half, if there is one.
    ///
    /// A double chest counts as one opener on each half, which is what makes
    /// both lids rise together.
    fn for_each_owner(&self, f: impl Fn(&BlockEntityBase)) {
        for container in iter::once(&self.container).chain(self.second_container.as_ref()) {
            if let Some(owner) = container.owner_block_entity() {
                f(&owner);
            }
        }
    }
}

impl MenuKind for ChestKind {
    /// Vanilla parity: `ChestBlockEntity.startOpen`, which ignores spectators
    /// -- a spectator looking into a chest must not raise its lid or power a
    /// trapped chest, because nobody can see them do it.
    fn on_open(
        &mut self,
        _behavior: &mut MenuBehavior,
        _guard: &mut ContainerLockGuard,
        player: &Player,
    ) {
        if player.is_spectator() {
            return;
        }
        self.for_each_owner(BlockEntityBase::increment_openers);
    }

    /// Vanilla parity: `ChestBlockEntity.stopOpen`.
    fn removed(&mut self, _behavior: &mut MenuBehavior, player: &Player) {
        if player.is_spectator() {
            return;
        }
        self.for_each_owner(BlockEntityBase::decrement_openers);
    }

    /// Returns true if every backing container is still valid for the player.
    ///
    /// Vanilla parity: a double chest closes as soon as either half becomes
    /// invalid.
    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        if !self.container.still_valid(player) {
            return false;
        }
        self.second_container
            .as_ref()
            .is_none_or(|second| second.still_valid(player))
    }
}

#[cfg(test)]
mod tests {
    use steel_utils::locks::IntoShared as _;

    use super::*;
    use crate::inventory::container::SimpleContainer;

    #[test]
    fn chest_uses_exactly_the_rows_requested_from_oversized_container() {
        let inventory = PlayerInventory::new().into_shared();
        let container = SimpleContainer::new(18).into_shared();

        let menu = chest(inventory, 1, container, 1);

        assert_eq!(menu.behavior().slot_count(), 9 + 36);
    }
}
