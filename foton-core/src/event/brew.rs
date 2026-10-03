//! The cancellable completion of a brewing stand's brewing cycle.

use foton_registry::item_stack::ItemStack;
use foton_utils::{BlockPos, DowncastType, DowncastTypeKey};

use super::Event;

/// Paper's `BrewEvent`: inputs remain in the stand until listeners return.
/// Fuel was already spent at the start of the cycle, even when cancelled.
pub struct BrewEvent {
    world: String,
    position: BlockPos,
    fuel: i32,
    results: Vec<ItemStack>,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for BrewEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/brew");
}

impl Event for BrewEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl BrewEvent {
    /// Creates a completion event with the three candidate bottle results.
    #[must_use]
    pub const fn new(
        world: String,
        position: BlockPos,
        fuel: i32,
        results: Vec<ItemStack>,
    ) -> Self {
        Self {
            world,
            position,
            fuel,
            results,
            cancelled: false,
        }
    }

    /// World containing the brewing stand.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }

    /// Brewing stand position.
    #[must_use]
    pub const fn position(&self) -> BlockPos {
        self.position
    }

    /// Remaining brewing uses after the current cycle's fuel was spent.
    #[must_use]
    pub const fn fuel(&self) -> i32 {
        self.fuel
    }

    /// Candidate results, before they replace the bottle slots.
    #[must_use]
    pub fn results(&self) -> &[ItemStack] {
        &self.results
    }

    /// Replaces candidate results. Missing slots become empty; extras are ignored.
    pub fn set_results(&mut self, results: Vec<ItemStack>) {
        self.results = results;
    }

    /// Cancels or uncancels the completion without refunding the cycle's fuel.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }

    /// Takes the accepted result list, or no results when cancelled.
    #[must_use]
    pub fn into_results(self) -> Option<Vec<ItemStack>> {
        (!self.cancelled).then_some(self.results)
    }
}
