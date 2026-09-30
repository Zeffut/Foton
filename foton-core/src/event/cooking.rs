//! Events from blocks that cook and brew on their own: furnaces, smokers,
//! blast furnaces and brewing stands.
//!
//! All four are fired from a block entity's tick with its container unlocked,
//! because a listener reading the furnace it is told about is the normal case.

use foton_registry::item_stack::ItemStack;
use foton_utils::downcast::{DowncastType, DowncastTypeKey};
use foton_utils::{BlockPos, Identifier};

use super::Event;

/// A furnace is about to take a fuel item and light.
///
/// A listener may change how long the fuel burns, keep the fuel item, let the
/// furnace light without it burning down, or cancel so nothing happens.
pub struct FurnaceBurnEvent {
    world: String,
    position: BlockPos,
    fuel: ItemStack,
    burn_time: i32,
    burning: bool,
    consume_fuel: bool,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for FurnaceBurnEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/furnace_burn");
}

impl Event for FurnaceBurnEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl FurnaceBurnEvent {
    /// Creates the event for `fuel` about to burn for `burn_time` ticks.
    #[must_use]
    pub const fn new(world: String, position: BlockPos, fuel: ItemStack, burn_time: i32) -> Self {
        Self {
            world,
            position,
            fuel,
            burn_time,
            burning: true,
            consume_fuel: true,
            cancelled: false,
        }
    }

    /// The world the furnace is in.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }

    /// Where the furnace is.
    #[must_use]
    pub const fn position(&self) -> BlockPos {
        self.position
    }

    /// The fuel about to burn.
    #[must_use]
    pub const fn fuel(&self) -> &ItemStack {
        &self.fuel
    }

    /// How many ticks it will burn.
    #[must_use]
    pub const fn burn_time(&self) -> i32 {
        self.burn_time
    }

    /// Changes how many ticks it will burn.
    pub const fn set_burn_time(&mut self, burn_time: i32) {
        self.burn_time = burn_time;
    }

    /// Whether the furnace counts as burning fuel, which is what spends it.
    #[must_use]
    pub const fn burning(&self) -> bool {
        self.burning
    }

    /// Changes whether the furnace counts as burning fuel.
    pub const fn set_burning(&mut self, burning: bool) {
        self.burning = burning;
    }

    /// Whether the fuel item is used up.
    #[must_use]
    pub const fn consume_fuel(&self) -> bool {
        self.consume_fuel
    }

    /// Changes whether the fuel item is used up.
    pub const fn set_consume_fuel(&mut self, consume_fuel: bool) {
        self.consume_fuel = consume_fuel;
    }

    /// Leaves the furnace unlit, or lets it light again.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}

/// A furnace is starting to cook an item. A listener may change how long it
/// takes; the start itself cannot be refused.
pub struct FurnaceStartSmeltEvent {
    world: String,
    position: BlockPos,
    source: ItemStack,
    recipe: Identifier,
    total_cook_time: i32,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for FurnaceStartSmeltEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/furnace_start_smelt");
}

impl Event for FurnaceStartSmeltEvent {}

impl FurnaceStartSmeltEvent {
    /// Creates the event for `source` starting under `recipe`.
    #[must_use]
    pub const fn new(
        world: String,
        position: BlockPos,
        source: ItemStack,
        recipe: Identifier,
        total_cook_time: i32,
    ) -> Self {
        Self {
            world,
            position,
            source,
            recipe,
            total_cook_time,
        }
    }

    /// The world the furnace is in.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }

    /// Where the furnace is.
    #[must_use]
    pub const fn position(&self) -> BlockPos {
        self.position
    }

    /// What is being cooked.
    #[must_use]
    pub const fn source(&self) -> &ItemStack {
        &self.source
    }

    /// The recipe cooking it.
    #[must_use]
    pub const fn recipe(&self) -> &Identifier {
        &self.recipe
    }

    /// How many ticks the cook takes.
    #[must_use]
    pub const fn total_cook_time(&self) -> i32 {
        self.total_cook_time
    }

    /// Changes how many ticks the cook takes.
    pub const fn set_total_cook_time(&mut self, total_cook_time: i32) {
        self.total_cook_time = total_cook_time;
    }
}

/// A furnace finished cooking an item and is about to put out the result.
///
/// A listener may change the result, or cancel: then nothing is consumed and
/// the cook starts over.
pub struct FurnaceSmeltEvent {
    world: String,
    position: BlockPos,
    source: ItemStack,
    result: ItemStack,
    recipe: Identifier,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for FurnaceSmeltEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/furnace_smelt");
}

impl Event for FurnaceSmeltEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl FurnaceSmeltEvent {
    /// Creates the event for `source` about to become `result`.
    #[must_use]
    pub const fn new(
        world: String,
        position: BlockPos,
        source: ItemStack,
        result: ItemStack,
        recipe: Identifier,
    ) -> Self {
        Self {
            world,
            position,
            source,
            result,
            recipe,
            cancelled: false,
        }
    }

    /// The world the furnace is in.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }

    /// Where the furnace is.
    #[must_use]
    pub const fn position(&self) -> BlockPos {
        self.position
    }

    /// What was cooked.
    #[must_use]
    pub const fn source(&self) -> &ItemStack {
        &self.source
    }

    /// What comes out.
    #[must_use]
    pub const fn result(&self) -> &ItemStack {
        &self.result
    }

    /// Changes what comes out.
    pub fn set_result(&mut self, result: ItemStack) {
        self.result = result;
    }

    /// The recipe that cooked it.
    #[must_use]
    pub const fn recipe(&self) -> &Identifier {
        &self.recipe
    }

    /// Refuses the result, or lets it out again.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}

/// Brewing completion uses the transactional inventory event.
pub use super::inventory::BrewEvent;
