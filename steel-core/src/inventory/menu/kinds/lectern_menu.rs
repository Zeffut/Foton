//! Lectern menu.
//!
//! Vanilla parity: `LecternMenu`. One number the player can change and one
//! book they cannot pick up: the buttons turn pages or take the book away.
//! Everything arrives as a button rather than a slot click, which is why this
//! menu has almost no slot handling at all.

use std::sync::Arc;

use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::item_stack::ItemStack;
use steel_registry::{vanilla_blocks, vanilla_menu_types};
use steel_utils::BlockPos;
use steel_utils::locks::{IntoShared, Shared};

use crate::behavior::blocks::{signal_lectern_page_change, take_book_from};
use crate::block_entity::entities::LecternBlockEntity;
use crate::inventory::container::{Container as _, SimpleContainer};
use crate::inventory::prelude::*;
use crate::player::player_inventory::PlayerInventory;
use crate::world::{LevelReader as _, World};

/// Vanilla parity: `LecternMenu.BUTTON_PREV_PAGE`.
const BUTTON_PREVIOUS_PAGE: i32 = 1;
/// Vanilla parity: `LecternMenu.BUTTON_NEXT_PAGE`.
const BUTTON_NEXT_PAGE: i32 = 2;
/// Vanilla parity: `LecternMenu.BUTTON_TAKE_BOOK`.
const BUTTON_TAKE_BOOK: i32 = 3;
/// Vanilla parity: `LecternMenu.BUTTON_PAGE_JUMP_RANGE_START`, above which a
/// button id is a page number rather than a command.
const BUTTON_PAGE_JUMP_RANGE_START: i32 = 100;

/// Builds the lectern menu.
#[must_use]
pub fn lectern(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    block_pos: BlockPos,
    world: &Arc<World>,
) -> Menu {
    // Vanilla parity: `LecternMenu` shows the book in one slot and carries no
    // player inventory -- the book is not something the player can reach
    // without the Take Book button. The slot exists so the client has
    // something to draw; the book itself lives on the block entity.
    let _ = inventory;

    let book_display = SimpleContainer::new(1).into_shared();
    if let Some(book) = book_on(world, block_pos) {
        book_display.lock().set_item(0, book);
    }

    let mut builder = MenuBuilder::new(&vanilla_menu_types::LECTERN, container_id);
    let _book_slot = builder.section_all(&book_display);
    let page = builder.data_slot(0);

    builder.build(LecternKind {
        page,
        block_pos,
        world: Arc::clone(world),
        book_display,
    })
}

/// Per-menu lectern state.
pub struct LecternKind {
    /// The open page, mirrored to the client.
    page: DataSlot,
    block_pos: BlockPos,
    world: Arc<World>,
    /// A copy of the book, purely so the client has something to draw.
    book_display: Shared<SimpleContainer>,
}

// SAFETY: This Steel-owned key uniquely identifies the concrete menu kind
// within the process.
unsafe impl steel_utils::DowncastType for LecternKind {
    const TYPE_KEY: steel_utils::DowncastTypeKey =
        steel_utils::DowncastTypeKey::new("steel:menu/lectern");
}

impl MenuKind for LecternKind {
    /// Vanilla parity: `LecternMenu.clickMenuButton`.
    ///
    /// The page buttons only pulse the block when the page really moved --
    /// clicking "next" on the last page of a book must not tick the redstone,
    /// which is what stops a lectern being used as a clock.
    fn on_button_click(
        &mut self,
        behavior: &mut MenuBehavior,
        _guard: &mut ContainerLockGuard,
        player: &Player,
        button: i32,
    ) -> bool {
        let current = i32::from(self.page.get(behavior));

        let target = match button {
            BUTTON_PREVIOUS_PAGE => current - 1,
            BUTTON_NEXT_PAGE => current + 1,
            BUTTON_TAKE_BOOK => {
                let book = take_book_from(&self.world, self.block_pos);
                if !book.is_empty() {
                    // Vanilla parity: the book goes to the player's inventory,
                    // or on the floor if there is no room for it.
                    player.add_item_or_drop(book);
                }
                self.book_display.lock().set_item(0, ItemStack::empty());
                return true;
            }
            jump if jump >= BUTTON_PAGE_JUMP_RANGE_START => jump - BUTTON_PAGE_JUMP_RANGE_START,
            _ => return false,
        };

        if !turn_page(&self.world, self.block_pos, target) {
            return false;
        }
        signal_lectern_page_change(&self.world, self.block_pos);
        self.page
            .set(behavior, i16::try_from(target).unwrap_or(i16::MAX));
        true
    }

    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        let world = player.get_world();
        world.get_block_state(self.block_pos).get_block() == &vanilla_blocks::LECTERN
            && player.is_within_block_interaction_range_with_buffer(self.block_pos, 4.0)
    }
}

/// Returns the book on the lectern at `pos`, if there is one.
fn book_on(world: &Arc<World>, pos: BlockPos) -> Option<ItemStack> {
    use steel_utils::Downcast as _;

    let entity = world.get_block_entity(pos)?;
    let lectern = entity.downcast_ref::<LecternBlockEntity>()?;
    Some(lectern.book())
}

/// Turns the lectern at `pos` to `page`, reporting whether it moved.
fn turn_page(world: &Arc<World>, pos: BlockPos, page: i32) -> bool {
    use steel_utils::Downcast as _;

    world
        .get_block_entity(pos)
        .and_then(|entity| {
            entity
                .downcast_ref::<LecternBlockEntity>()
                .map(|lectern| lectern.set_page(page))
        })
        .unwrap_or(false)
}
