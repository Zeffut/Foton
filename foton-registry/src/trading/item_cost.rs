//! What a merchant asks for on one side of a trade.

use std::io::{Cursor, Result};

use foton_utils::codec::VarInt;
use foton_utils::serial::{ReadFrom, WriteTo};

use crate::REGISTRY;
use crate::data_component_predicate::DataComponentExactPredicate;
use crate::item_stack::ItemStack;
use crate::items::ItemRef;
use crate::registry::{RegistryEntry as _, RegistryExt as _};

/// One side of a trade's price: an item, a count, and the components it must carry.
///
/// Vanilla parity: `net.minecraft.world.item.trading.ItemCost`. Vanilla's record
/// carries a fourth field, a ready-made `ItemStack` for the trade screen to draw;
/// it is derived from the other three and cached exactly the same way here,
/// because [`Self::cost_stack`] is read once per menu sync.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemCost {
    item: ItemRef,
    count: i32,
    components: DataComponentExactPredicate,
    item_stack: ItemStack,
}

impl ItemCost {
    /// A cost of `count` of `item` with no component requirements.
    #[must_use]
    pub fn new(item: ItemRef, count: i32) -> Self {
        Self::with_components(item, count, DataComponentExactPredicate::EMPTY)
    }

    /// A cost that also requires the paid stack to carry `components` exactly.
    ///
    /// Vanilla parity: the canonical `ItemCost(Holder<Item>, int, DataComponentExactPredicate)`
    /// constructor, which builds the display stack through `createStack`.
    #[must_use]
    pub fn with_components(
        item: ItemRef,
        count: i32,
        components: DataComponentExactPredicate,
    ) -> Self {
        let item_stack = ItemStack::with_count_and_patch(item, count, components.as_patch());
        Self {
            item,
            count,
            components,
            item_stack,
        }
    }

    /// The item this cost is paid in.
    #[must_use]
    pub const fn item(&self) -> ItemRef {
        self.item
    }

    /// How many of [`Self::item`] the trade asks for before any adjustment.
    #[must_use]
    pub const fn count(&self) -> i32 {
        self.count
    }

    /// The components the paid stack must carry.
    #[must_use]
    pub const fn components(&self) -> &DataComponentExactPredicate {
        &self.components
    }

    /// The stack the trade screen draws for this side of the price.
    ///
    /// Vanilla parity: the record's `itemStack` component.
    #[must_use]
    pub const fn cost_stack(&self) -> &ItemStack {
        &self.item_stack
    }

    /// Returns `true` if `stack` is the right item carrying the right components.
    ///
    /// Vanilla parity: `ItemCost.test`. Note what it does *not* check: the count.
    /// Callers compare that themselves, because the count a trade actually wants
    /// moves with demand and reputation.
    #[must_use]
    pub fn test(&self, stack: &ItemStack) -> bool {
        stack.is(self.item) && self.components.test(stack)
    }
}

impl WriteTo for ItemCost {
    /// Vanilla parity: `ItemCost.STREAM_CODEC`.
    fn write(&self, writer: &mut impl std::io::Write) -> Result<()> {
        VarInt(self.item.id() as i32).write(writer)?;
        VarInt(self.count).write(writer)?;
        self.components.write(writer)
    }
}

impl ReadFrom for ItemCost {
    fn read(data: &mut Cursor<&[u8]>) -> Result<Self> {
        let item_id = VarInt::read(data)?.0;
        let item_id = usize::try_from(item_id)
            .map_err(|_| std::io::Error::other(format!("Negative item id: {item_id}")))?;
        let item = REGISTRY
            .items
            .by_id(item_id)
            .ok_or_else(|| std::io::Error::other(format!("Unknown item id: {item_id}")))?;
        let count = VarInt::read(data)?.0;
        let components = DataComponentExactPredicate::read(data)?;
        Ok(Self::with_components(item, count, components))
    }
}
