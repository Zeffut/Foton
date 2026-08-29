//! The loom's three inputs and its result.
//!
//! Vanilla parity: the anonymous slots of `LoomMenu`. A loom takes a banner, a
//! dye and optionally a pattern item, and stamps one more layer onto the
//! banner. Which layer is not decided by the inputs: the player picks it from
//! the list the pattern item offers, which is why the result depends on a
//! selection as well as on the slots.

use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use foton_registry::banner_pattern::BannerPatternRef;
use foton_registry::data_components::components::{BannerPatternLayer, BannerPatternLayers};
use foton_registry::data_components::vanilla_components::{
    BANNER_PATTERNS, DYE, PROVIDES_BANNER_PATTERNS,
};
use foton_registry::item_stack::ItemStack;
use foton_registry::registry::holder::RegistryHolder;
use foton_registry::registry::holder_set::RegistryHolderSet;
use foton_registry::vanilla_banner_pattern_tags::BannerPatternTag;
use foton_registry::vanilla_item_tags::ItemTag;
use foton_registry::{REGISTRY, TaggedRegistryExt as _};
use foton_utils::locks::Shared;

use crate::inventory::container::{Container as _, ResultContainer, SimpleContainer};
use crate::inventory::lock::{ContainerId, ContainerLockGuard, ContainerRef};
use crate::inventory::slots::ResultHandler;
use crate::player::Player;

/// The banner being worked on.
pub const LOOM_BANNER: usize = 0;
/// The dye the new layer is drawn in.
pub const LOOM_DYE: usize = 1;
/// The optional pattern item, which decides what can be drawn.
pub const LOOM_PATTERN: usize = 2;

/// Vanilla parity: `LoomMenu.PATTERN_NOT_SET`.
pub const PATTERN_NOT_SET: i32 = -1;

/// How many layers a banner can carry.
///
/// Vanilla parity: the `layers().size() >= 6` of `LoomMenu.slotsChanged`.
const MAX_LAYERS: usize = 6;

/// Returns whether `stack` is a banner.
///
/// Vanilla parity: the `instanceof BannerItem` of `LoomMenu`. Foton has no
/// per-item classes, so the `banners` tag stands in -- it holds exactly the
/// sixteen colored banners and nothing else.
#[must_use]
pub fn is_banner(stack: &ItemStack) -> bool {
    REGISTRY.items.is_in_tag(stack.item(), &ItemTag::BANNERS)
}

/// Returns whether `stack` can go in the dye slot.
///
/// Vanilla parity: `LoomMenu.isDyeItem`.
#[must_use]
pub fn is_dye_item(stack: &ItemStack) -> bool {
    REGISTRY.items.is_in_tag(stack.item(), &ItemTag::LOOM_DYES) && stack.has(DYE)
}

/// Returns whether `stack` can go in the pattern slot.
///
/// Vanilla parity: `LoomMenu.isPatternItem`.
#[must_use]
pub fn is_pattern_item(stack: &ItemStack) -> bool {
    REGISTRY
        .items
        .is_in_tag(stack.item(), &ItemTag::LOOM_PATTERNS)
        && stack.has(PROVIDES_BANNER_PATTERNS)
}

/// Returns the patterns a player may choose, given what is in the pattern slot.
///
/// Vanilla parity: `LoomMenu.getSelectablePatterns`. An empty pattern slot
/// offers the patterns that need no item; a pattern item offers exactly the
/// ones it names.
#[must_use]
pub fn selectable_patterns(pattern_stack: &ItemStack) -> Vec<BannerPatternRef> {
    if pattern_stack.is_empty() {
        return REGISTRY
            .banner_patterns
            .iter_tag(&BannerPatternTag::NO_ITEM_REQUIRED)
            .collect();
    }

    match pattern_stack.get(PROVIDES_BANNER_PATTERNS) {
        Some(RegistryHolderSet::Tag(tag)) => REGISTRY.banner_patterns.iter_tag(tag).collect(),
        Some(RegistryHolderSet::Direct(entries)) => entries.clone(),
        None => Vec::new(),
    }
}

/// Keeps a loom's result in step with its inputs and the chosen pattern.
#[derive(Clone)]
pub struct LoomHandler {
    input_container: Shared<SimpleContainer>,
    result_container: Shared<ResultContainer>,
    /// Which of the selectable patterns the player picked, or
    /// [`PATTERN_NOT_SET`]. Shared with the menu, which mirrors it to the
    /// client so the right button is drawn pressed.
    selected: Arc<AtomicI32>,
}

impl LoomHandler {
    /// Creates a handler over the loom's containers.
    #[must_use]
    pub fn new(
        input_container: Shared<SimpleContainer>,
        result_container: Shared<ResultContainer>,
    ) -> Self {
        Self {
            input_container,
            result_container,
            selected: Arc::new(AtomicI32::new(PATTERN_NOT_SET)),
        }
    }

    /// The index the player picked, or [`PATTERN_NOT_SET`].
    #[must_use]
    pub fn selected(&self) -> i32 {
        self.selected.load(Ordering::Relaxed)
    }

    /// Records the index the player picked.
    pub fn select(&self, index: i32) {
        self.selected.store(index, Ordering::Relaxed);
    }

    fn input_id(&self) -> ContainerId {
        ContainerId::from_arc(&self.input_container)
    }

    fn result_id(&self) -> ContainerId {
        ContainerId::from_arc(&self.result_container)
    }

    /// Returns the banner, the dye and the pattern item.
    fn inputs(&self, guard: &ContainerLockGuard) -> Option<(ItemStack, ItemStack, ItemStack)> {
        let container = guard.get(self.input_id())?;
        Some((
            container.get_item(LOOM_BANNER).clone(),
            container.get_item(LOOM_DYE).clone(),
            container.get_item(LOOM_PATTERN).clone(),
        ))
    }

    /// Works out what the loom would make right now.
    ///
    /// Vanilla parity: `LoomMenu.slotsChanged` together with `setupResultSlot`.
    /// The selection is clamped here rather than by the caller because a slot
    /// change can make the chosen pattern unavailable -- swapping the pattern
    /// item out from under a selection is the case that matters.
    fn compute(&self, guard: &ContainerLockGuard) -> ItemStack {
        let Some((banner, dye, pattern_item)) = self.inputs(guard) else {
            return ItemStack::empty();
        };
        if banner.is_empty() || dye.is_empty() {
            self.select(PATTERN_NOT_SET);
            return ItemStack::empty();
        }

        let patterns = selectable_patterns(&pattern_item);
        if patterns.is_empty() {
            self.select(PATTERN_NOT_SET);
            return ItemStack::empty();
        }

        // Vanilla parity: a pattern item offering exactly one pattern selects
        // it for the player, which is what makes a banner-pattern item work
        // with a single click.
        let index = if patterns.len() == 1 {
            self.select(0);
            0
        } else {
            let selected = self.selected();
            match usize::try_from(selected) {
                Ok(index) if index < patterns.len() => index,
                _ => {
                    self.select(PATTERN_NOT_SET);
                    return ItemStack::empty();
                }
            }
        };

        let layers = banner
            .get(BANNER_PATTERNS)
            .cloned()
            .unwrap_or_else(BannerPatternLayers::empty);
        if layers.layers().len() >= MAX_LAYERS {
            self.select(PATTERN_NOT_SET);
            return ItemStack::empty();
        }

        let Some(color) = dye.get(DYE).copied() else {
            return ItemStack::empty();
        };

        let mut result = banner.copy_with_count(1);
        let mut stamped = layers.layers().to_vec();
        stamped.push(BannerPatternLayer::new(
            RegistryHolder::Reference(patterns[index]),
            color,
        ));
        result.set(BANNER_PATTERNS, BannerPatternLayers::new(stamped));
        result
    }
}

impl ResultHandler for LoomHandler {
    fn result_container(&self) -> ContainerRef {
        ContainerRef::from(self.result_container.clone())
    }

    fn dependencies(&self) -> Vec<ContainerRef> {
        vec![ContainerRef::from(self.input_container.clone())]
    }

    fn update_result(&self, guard: &mut ContainerLockGuard) {
        let result = self.compute(guard);
        let container = guard
            .get_typed_mut::<ResultContainer>(self.result_id())
            .expect("result container not locked");
        container.set_item(0, result);
        container.set_changed();
    }

    /// Vanilla parity: the `onTake` of the loom's result slot, which spends one
    /// banner and one dye and leaves the pattern item alone -- a banner pattern
    /// is a stencil, not an ingredient.
    fn on_result_taken(
        &self,
        guard: &mut ContainerLockGuard,
        _player: &Player,
    ) -> Option<ItemStack> {
        {
            let container = guard
                .get_typed_mut::<SimpleContainer>(self.input_id())
                .expect("input container not locked");
            container.remove_item(LOOM_BANNER, 1);
            container.remove_item(LOOM_DYE, 1);
            container.set_changed();
        }

        let still_loaded = self
            .inputs(guard)
            .is_some_and(|(banner, dye, _)| !banner.is_empty() && !dye.is_empty());
        if !still_loaded {
            self.select(PATTERN_NOT_SET);
        }
        self.update_result(guard);
        None
    }

    fn is_result_valid(&self, guard: &ContainerLockGuard, _player: &Player) -> bool {
        guard
            .get(self.result_id())
            .is_some_and(|container| !container.get_item(0).is_empty())
    }
}

#[cfg(test)]
mod tests {
    use foton_registry::{init_vanilla_registry, vanilla_items};

    use super::*;

    fn setup() {
        init_vanilla_registry();
    }

    #[test]
    fn a_banner_is_a_banner_and_a_stick_is_not() {
        setup();
        assert!(is_banner(&ItemStack::new(&vanilla_items::WHITE_BANNER)));
        assert!(!is_banner(&ItemStack::new(&vanilla_items::STICK)));
    }

    /// The dye slot wants the component too, not just the tag: the tag is what
    /// the client filters on, the component is what the layer is drawn with.
    #[test]
    fn a_dye_is_a_dye_and_a_stick_is_not() {
        setup();
        assert!(is_dye_item(&ItemStack::new(&vanilla_items::RED_DYE)));
        assert!(!is_dye_item(&ItemStack::new(&vanilla_items::STICK)));
    }

    #[test]
    fn a_banner_pattern_item_is_a_pattern_item() {
        setup();
        assert!(is_pattern_item(&ItemStack::new(
            &vanilla_items::FLOWER_BANNER_PATTERN
        )));
        assert!(!is_pattern_item(&ItemStack::new(&vanilla_items::STICK)));
    }

    /// With nothing in the pattern slot a loom offers the patterns that need
    /// no item -- the plain stripes and crosses, and not the flower.
    #[test]
    fn an_empty_pattern_slot_offers_the_free_patterns() {
        setup();
        let patterns = selectable_patterns(&ItemStack::empty());

        assert!(!patterns.is_empty());
        assert!(
            !patterns
                .iter()
                .any(|pattern| pattern.key.path.as_ref() == "flower")
        );
    }

    /// A pattern item offers exactly what it names, which is one pattern for
    /// every vanilla banner-pattern item.
    #[test]
    fn a_pattern_item_offers_its_own_pattern() {
        setup();
        let patterns = selectable_patterns(&ItemStack::new(&vanilla_items::FLOWER_BANNER_PATTERN));

        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].key.path.as_ref(), "flower");
    }
}
