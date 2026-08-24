//! The cartography table's two inputs and its result.
//!
//! Vanilla parity: the anonymous slots of `CartographyTableMenu`. A map plus
//! paper zooms out, a map plus a glass pane locks, and a map plus a blank map
//! copies -- and the first two do not happen here. They are recorded on the
//! result as a `minecraft:map_post_processing` marker and only turn into a new
//! map once the player takes it, which is what stops a hovering player from
//! allocating a map id per click.

use std::sync::Arc;

use steel_registry::data_components::components::MapPostProcessing;
use steel_registry::data_components::vanilla_components::{MAP_ID, MAP_POST_PROCESSING};
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_items;
use steel_utils::locks::Shared;

use crate::inventory::container::{Container as _, ResultContainer, SimpleContainer};
use crate::inventory::lock::{ContainerId, ContainerLockGuard, ContainerRef};
use crate::inventory::slots::ResultHandler;
use crate::map::MAX_SCALE;
use crate::map::storage::MapStorage;
use crate::player::Player;

/// The slot holding the map being worked on.
pub const CARTOGRAPHY_MAP: usize = 0;
/// The slot holding the paper, blank map or glass pane.
pub const CARTOGRAPHY_ADDITIONAL: usize = 1;

/// Returns whether a stack may go in the map slot.
///
/// Vanilla parity: the `mayPlace` of `CartographyTableMenu`'s first slot.
#[must_use]
pub fn is_filled_map(stack: &ItemStack) -> bool {
    stack.has(MAP_ID)
}

/// Returns whether a stack may go in the second slot.
///
/// Vanilla parity: the `mayPlace` of `CartographyTableMenu`'s second slot.
#[must_use]
pub fn is_cartography_material(stack: &ItemStack) -> bool {
    stack.is(&vanilla_items::PAPER)
        || stack.is(&vanilla_items::MAP)
        || stack.is(&vanilla_items::GLASS_PANE)
}

/// Keeps a cartography table's result in step with its two inputs.
#[derive(Clone)]
pub struct CartographyHandler {
    input_container: Shared<SimpleContainer>,
    result_container: Shared<ResultContainer>,
    maps: Arc<MapStorage>,
}

impl CartographyHandler {
    /// Creates a handler over the table's containers and its domain's maps.
    #[must_use]
    pub const fn new(
        input_container: Shared<SimpleContainer>,
        result_container: Shared<ResultContainer>,
        maps: Arc<MapStorage>,
    ) -> Self {
        Self {
            input_container,
            result_container,
            maps,
        }
    }

    fn input_id(&self) -> ContainerId {
        ContainerId::from_arc(&self.input_container)
    }

    fn result_id(&self) -> ContainerId {
        ContainerId::from_arc(&self.result_container)
    }

    fn inputs(&self, guard: &ContainerLockGuard) -> Option<(ItemStack, ItemStack)> {
        let container = guard.get(self.input_id())?;
        Some((
            container.get_item(CARTOGRAPHY_MAP).clone(),
            container.get_item(CARTOGRAPHY_ADDITIONAL).clone(),
        ))
    }

    /// Returns whether there is nothing to take.
    #[must_use]
    pub fn result_is_empty(&self, guard: &ContainerLockGuard) -> bool {
        guard
            .get(self.result_id())
            .is_none_or(|container| container.get_item(0).is_empty())
    }

    fn computed_result(&self, guard: &ContainerLockGuard) -> ItemStack {
        self.inputs(guard)
            .map_or_else(ItemStack::empty, |(map, additional)| {
                self.cartography_result(&map, &additional)
            })
    }

    /// Returns what the table would make of these two inputs.
    ///
    /// Vanilla parity: `CartographyTableMenu.setupResultSlot`.
    #[must_use]
    pub fn cartography_result(&self, map: &ItemStack, additional: &ItemStack) -> ItemStack {
        if map.is_empty() || additional.is_empty() {
            return ItemStack::empty();
        }
        let Some(map_id) = map.get(MAP_ID).copied() else {
            return ItemStack::empty();
        };
        let Some(data) = self.maps.get(map_id) else {
            return ItemStack::empty();
        };
        let (locked, scale) = {
            let data = data.lock();
            (data.locked, data.scale)
        };

        if additional.is(&vanilla_items::PAPER) && !locked && scale < MAX_SCALE {
            let mut result = map.copy_with_count(1);
            result.set(MAP_POST_PROCESSING, MapPostProcessing::Scale);
            return result;
        }
        if additional.is(&vanilla_items::GLASS_PANE) && !locked {
            let mut result = map.copy_with_count(1);
            result.set(MAP_POST_PROCESSING, MapPostProcessing::Lock);
            return result;
        }
        if additional.is(&vanilla_items::MAP) {
            return map.copy_with_count(2);
        }
        ItemStack::empty()
    }
}

impl ResultHandler for CartographyHandler {
    fn result_container(&self) -> ContainerRef {
        ContainerRef::from(self.result_container.clone())
    }

    fn dependencies(&self) -> Vec<ContainerRef> {
        vec![ContainerRef::from(self.input_container.clone())]
    }

    fn update_result(&self, guard: &mut ContainerLockGuard) {
        let result = self.computed_result(guard);
        let result_id = self.result_id();
        let Some(container) = guard.get_typed_mut::<ResultContainer>(result_id) else {
            return;
        };
        container.set_item(0, result);
        container.set_changed();
    }

    /// Vanilla parity: the `onTake` of the result slot, which spends one of
    /// each input -- the copy keeps its original, which is why a copy costs one
    /// blank map and gives two filled ones back.
    fn on_result_taken(
        &self,
        guard: &mut ContainerLockGuard,
        _player: &Player,
    ) -> Option<ItemStack> {
        let input_id = self.input_id();
        if let Some(container) = guard.get_mut(input_id) {
            container.remove_item(CARTOGRAPHY_MAP, 1);
            container.remove_item(CARTOGRAPHY_ADDITIONAL, 1);
            container.set_changed();
        }
        self.update_result(guard);
        None
    }

    fn is_result_valid(&self, guard: &ContainerLockGuard, _player: &Player) -> bool {
        !self.computed_result(guard).is_empty()
    }
}
