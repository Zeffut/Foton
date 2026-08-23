//! The lectern block entity.
//!
//! Vanilla parity: `LecternBlockEntity`. It holds one book and remembers which
//! page is open, and that page is the whole redstone story: turning it pulses
//! the block and a comparator reads how far through the book the reader is.

use std::mem;
use std::sync::Weak;

use simdnbt::ToNbtTag as _;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtTag};
use steel_registry::data_components::vanilla_components::{
    WRITABLE_BOOK_CONTENT, WRITTEN_BOOK_CONTENT,
};
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_block_entity_types;
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::world::World;

/// The strongest signal a lectern's comparator gives.
///
/// Vanilla parity: the `pageProgress * 14 + 1` of
/// `LecternBlockEntity.getRedstoneSignal` -- a book open at its first page
/// already reads 1, and only the last page reads 15.
const SIGNAL_RANGE: f32 = 14.0;

/// The lectern.
pub struct LecternBlockEntity {
    base: BlockEntityBase,
    state: SyncMutex<LecternState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies the block entity.
unsafe impl DowncastType for LecternBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/lectern");
}

struct LecternState {
    book: ItemStack,
    page: i32,
    page_count: i32,
}

impl LecternBlockEntity {
    /// Creates a lectern block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: BlockEntityBase::new(&vanilla_block_entity_types::LECTERN, level, pos, state),
            state: SyncMutex::new(LecternState {
                book: ItemStack::empty(),
                page: 0,
                page_count: 0,
            }),
        }
    }

    /// Returns a copy of the book on the stand.
    #[must_use]
    pub fn book(&self) -> ItemStack {
        self.state.lock().book.clone()
    }

    /// Returns whether there is a readable book on the stand.
    ///
    /// Vanilla parity: `LecternBlockEntity.hasBook`, which asks whether the
    /// item has book *content* rather than whether the slot is occupied -- an
    /// empty writable book still counts.
    #[must_use]
    pub fn has_book(&self) -> bool {
        let state = self.state.lock();
        state.book.has(WRITABLE_BOOK_CONTENT) || state.book.has(WRITTEN_BOOK_CONTENT)
    }

    /// Puts a book on the stand, open at the first page.
    pub fn set_book(&self, book: ItemStack) {
        let page_count = page_count_of(&book);
        let mut state = self.state.lock();
        state.book = book;
        state.page = 0;
        state.page_count = page_count;
        drop(state);
        self.base.set_changed();
    }

    /// Takes the book off the stand and returns it.
    #[must_use]
    pub fn take_book(&self) -> ItemStack {
        let mut state = self.state.lock();
        let book = mem::replace(&mut state.book, ItemStack::empty());
        state.page = 0;
        state.page_count = 0;
        drop(state);
        self.base.set_changed();
        book
    }

    /// Returns the open page.
    #[must_use]
    pub fn page(&self) -> i32 {
        self.state.lock().page
    }

    /// Turns to `page`, and reports whether it actually moved.
    ///
    /// Vanilla parity: `LecternBlockEntity.setPage`, which clamps to the book
    /// and only signals when the page really changed -- clicking "next" on the
    /// last page must not pulse the redstone.
    pub fn set_page(&self, page: i32) -> bool {
        let mut state = self.state.lock();
        let clamped = page.clamp(0, (state.page_count - 1).max(0));
        if clamped == state.page {
            return false;
        }
        state.page = clamped;
        drop(state);
        self.base.set_changed();
        true
    }

    /// Returns what a comparator reads off the lectern.
    ///
    /// Vanilla parity: `LecternBlockEntity.getRedstoneSignal`.
    #[must_use]
    pub fn redstone_signal(&self) -> i32 {
        let has_book = self.has_book();
        let state = self.state.lock();
        let progress = if state.page_count > 1 {
            state.page as f32 / (state.page_count - 1) as f32
        } else {
            1.0
        };
        (progress * SIGNAL_RANGE).floor() as i32 + i32::from(has_book)
    }
}

/// Returns how many pages a book has.
///
/// Vanilla parity: `LecternBlockEntity.getPageCount`.
fn page_count_of(book: &ItemStack) -> i32 {
    if let Some(written) = book.get(WRITTEN_BOOK_CONTENT) {
        return written.pages().len() as i32;
    }
    if let Some(writable) = book.get(WRITABLE_BOOK_CONTENT) {
        return writable.pages().len() as i32;
    }
    0
}

impl BlockEntity for LecternBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let state = self.state.lock();
        if !state.book.is_empty()
            && let NbtTag::Compound(book_nbt) = state.book.clone().to_nbt_tag()
        {
            nbt.insert("Book", book_nbt);
            nbt.insert("Page", NbtTag::Int(state.page));
        }
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let view: NbtCompoundView<'_, '_> = nbt.into();
        let book = view
            .compound("Book")
            .and_then(|compound| ItemStack::from_borrowed_compound(&compound))
            .unwrap_or_else(ItemStack::empty);
        let page_count = page_count_of(&book);
        let page = view.int("Page").unwrap_or(0);

        let mut state = self.state.lock();
        state.page = page.clamp(0, (page_count - 1).max(0));
        state.page_count = page_count;
        state.book = book;
    }

    fn get_update_tag(&self) -> Option<NbtCompound> {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Weak;

    use steel_registry::data_components::components::{Filterable, WritableBookContent};
    use steel_registry::{init_vanilla_registry, vanilla_blocks, vanilla_items};

    use super::*;

    /// Builds a lectern with no world behind it.
    fn detached_lectern() -> LecternBlockEntity {
        init_vanilla_registry();
        LecternBlockEntity::new(
            Weak::new(),
            BlockPos::new(0, 0, 0),
            vanilla_blocks::LECTERN.default_state(),
        )
    }

    /// Builds a writable book with `pages` blank pages.
    fn book_with_pages(pages: usize) -> ItemStack {
        let mut book = ItemStack::new(&vanilla_items::WRITABLE_BOOK);
        let content = WritableBookContent::new(
            (0..pages)
                .map(|index| Filterable::new(format!("page {index}"), None))
                .collect(),
        )
        .expect("a handful of short pages is a valid book");
        book.set(WRITABLE_BOOK_CONTENT, content);
        book
    }

    /// An empty lectern has no book on it.
    ///
    /// Its raw `redstone_signal` is *not* zero -- vanilla's formula treats a
    /// bookless lectern as being at the end of a one-page book and returns 14.
    /// Nothing ever sees that: `LecternBlock.getAnalogOutputSignal` checks
    /// `has_book` first and answers zero. The guard belongs to the block, so
    /// this asserts what the block entity is actually asked.
    #[test]
    fn an_empty_lectern_has_no_book() {
        let lectern = detached_lectern();
        assert!(!lectern.has_book());
    }

    /// A book open at its first page already reads 1.
    ///
    /// Vanilla parity: the `+ (hasBook ? 1 : 0)` of `getRedstoneSignal`. It is
    /// what lets a comparator tell an occupied lectern from an empty one even
    /// before a page is turned.
    #[test]
    fn the_first_page_of_a_book_reads_one() {
        let lectern = detached_lectern();
        lectern.set_book(book_with_pages(8));

        assert!(lectern.has_book());
        assert_eq!(lectern.page(), 0);
        assert_eq!(lectern.redstone_signal(), 1);
    }

    /// The last page reads full.
    #[test]
    fn the_last_page_reads_fifteen() {
        let lectern = detached_lectern();
        lectern.set_book(book_with_pages(8));

        assert!(lectern.set_page(7));
        assert_eq!(lectern.redstone_signal(), 15);
    }

    /// Turning past the end does not move, and so must not report a change.
    ///
    /// Vanilla parity: the guard in `setPage`. Without it, holding "next" on
    /// the last page would pulse the redstone forever -- a lectern would be a
    /// clock.
    #[test]
    fn turning_past_the_last_page_changes_nothing() {
        let lectern = detached_lectern();
        lectern.set_book(book_with_pages(3));
        assert!(lectern.set_page(2));

        assert!(
            !lectern.set_page(3),
            "there is no fourth page, so nothing moved"
        );
        assert_eq!(lectern.page(), 2);
    }

    /// Taking the book leaves the stand empty and forgets the page.
    #[test]
    fn taking_the_book_resets_the_stand() {
        let lectern = detached_lectern();
        lectern.set_book(book_with_pages(5));
        lectern.set_page(3);

        let taken = lectern.take_book();

        assert!(taken.is(&vanilla_items::WRITABLE_BOOK));
        assert!(!lectern.has_book());
        assert_eq!(lectern.page(), 0);
    }

    /// A one-page book reads full on its only page.
    ///
    /// Vanilla parity: the `pageCount > 1` guard, which avoids dividing by
    /// zero and treats a single page as being at the end.
    #[test]
    fn a_one_page_book_reads_fifteen() {
        let lectern = detached_lectern();
        lectern.set_book(book_with_pages(1));
        assert_eq!(lectern.redstone_signal(), 15);
    }
}
