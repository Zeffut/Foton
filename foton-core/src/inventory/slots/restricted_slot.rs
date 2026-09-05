use std::sync::Arc;

use foton_registry::item_stack::ItemStack;
use foton_utils::{DowncastType, DowncastTypeKey};

use crate::{
    inventory::{
        lock::{ContainerLockGuard, ContainerRef},
        slots::{NormalSlot, Slot, SlotStorage},
    },
    player::Player,
};

type MayPlace = Box<dyn Fn(usize, &ItemStack) -> bool + Send + Sync>;
type MayPickup = Box<dyn Fn(usize, &ItemStack, &ContainerLockGuard, &Player) -> bool + Send + Sync>;
type MaxStackSize = Box<dyn Fn(usize) -> Option<i32> + Send + Sync>;

/// The predicates behind a [`RestrictedSlot`], shared by every slot of a
/// section so a slot costs one pointer rather than one per predicate.
pub struct RestrictedRules {
    may_place: MayPlace,
    may_pickup: Option<MayPickup>,
    /// Per-slot ceiling, applied on top of the container's and the item's.
    ///
    /// Vanilla parity: the `getMaxStackSize()` some menus override on a slot.
    /// It takes the slot index because those overrides are per slot and not per
    /// menu: the enchanting table caps its item slot at one while leaving the
    /// lapis slot alone, and capping both would make the three-lapis offer
    /// impossible to pay for.
    max_stack_size: Option<MaxStackSize>,
}

impl RestrictedRules {
    pub(crate) fn place_only(
        may_place: impl Fn(usize, &ItemStack) -> bool + Send + Sync + 'static,
    ) -> Arc<Self> {
        Arc::new(Self {
            may_place: Box::new(may_place),
            may_pickup: None,
            max_stack_size: None,
        })
    }

    /// Like [`place_only`](Self::place_only), with a ceiling on how much the
    /// slot will hold.
    pub(crate) fn capped(
        may_place: impl Fn(usize, &ItemStack) -> bool + Send + Sync + 'static,
        max_stack_size: impl Fn(usize) -> Option<i32> + Send + Sync + 'static,
    ) -> Arc<Self> {
        Arc::new(Self {
            may_place: Box::new(may_place),
            may_pickup: None,
            max_stack_size: Some(Box::new(max_stack_size)),
        })
    }

    pub(crate) fn guarded(
        may_place: impl Fn(usize, &ItemStack) -> bool + Send + Sync + 'static,
        may_pickup: impl Fn(usize, &ItemStack, &ContainerLockGuard, &Player) -> bool
        + Send
        + Sync
        + 'static,
    ) -> Arc<Self> {
        Arc::new(Self {
            may_place: Box::new(may_place),
            may_pickup: Some(Box::new(may_pickup)),
            max_stack_size: None,
        })
    }
}

/// A [`NormalSlot`] with custom place and pickup rules.
pub struct RestrictedSlot {
    base: NormalSlot,
    rules: Arc<RestrictedRules>,
}

// SAFETY: This key uniquely identifies Foton's `RestrictedSlot`.
unsafe impl DowncastType for RestrictedSlot {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:slot/restricted");
}

impl RestrictedSlot {
    /// Placement gated by `may_place`, which receives the container-local slot
    /// index; pickup stays allowed.
    pub fn new(
        container: impl Into<ContainerRef>,
        index: usize,
        may_place: impl Fn(usize, &ItemStack) -> bool + Send + Sync + 'static,
    ) -> Self {
        Self::with_rules(container, index, RestrictedRules::place_only(may_place))
    }

    /// Like [`new`](Self::new), but pickup is gated too.
    pub fn guarded(
        container: impl Into<ContainerRef>,
        index: usize,
        may_place: impl Fn(usize, &ItemStack) -> bool + Send + Sync + 'static,
        may_pickup: impl Fn(usize, &ItemStack, &ContainerLockGuard, &Player) -> bool
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self::with_rules(
            container,
            index,
            RestrictedRules::guarded(may_place, may_pickup),
        )
    }

    /// Shares one rules object across a whole section's slots.
    pub(crate) fn with_rules(
        container: impl Into<ContainerRef>,
        index: usize,
        rules: Arc<RestrictedRules>,
    ) -> Self {
        Self {
            base: NormalSlot::new(container, index),
            rules,
        }
    }
}

impl Slot for RestrictedSlot {
    fn storage(&self) -> &SlotStorage {
        self.base.storage()
    }

    fn get_item<'a>(&self, guard: &'a ContainerLockGuard) -> &'a ItemStack {
        self.base.get_item(guard)
    }

    fn get_item_mut<'a>(&self, guard: &'a mut ContainerLockGuard) -> &'a mut ItemStack {
        self.base.get_item_mut(guard)
    }

    fn set_item(&self, guard: &mut ContainerLockGuard, stack: ItemStack) {
        self.base.set_item(guard, stack);
    }

    fn may_place(&self, stack: &ItemStack) -> bool {
        (self.rules.may_place)(self.base.get_container_slot(), stack)
    }

    fn may_pickup(&self, guard: &ContainerLockGuard, player: &Player) -> bool {
        self.rules.may_pickup.as_ref().is_none_or(|it| {
            it(
                self.base.get_container_slot(),
                self.base.get_item(guard),
                guard,
                player,
            )
        })
    }

    fn get_max_stack_size(&self, guard: &ContainerLockGuard) -> i32 {
        let base = self.base.get_max_stack_size(guard);
        self.rules
            .max_stack_size
            .as_ref()
            .and_then(|capped| capped(self.base.get_container_slot()))
            .map_or(base, |capped| base.min(capped))
    }

    fn set_changed(&self, guard: &mut ContainerLockGuard) {
        self.base.set_changed(guard);
    }

    fn get_container_slot(&self) -> usize {
        self.base.get_container_slot()
    }
}

#[cfg(test)]
mod tests {
    use std::slice;
    use std::sync::Arc;

    use super::RestrictedSlot;
    use crate::inventory::container::{Container, SimpleContainer};
    use crate::inventory::lock::{ContainerLockGuard, ContainerRef};
    use crate::inventory::slots::Slot as _;
    use foton_registry::data_components::vanilla_components::MAX_STACK_SIZE;
    use foton_registry::{init_vanilla_registry, item_stack::ItemStack, vanilla_items};
    use foton_utils::locks::{IntoShared as _, SyncMutex};
    use foton_utils::{DowncastType, DowncastTypeKey};

    struct SingleItemContainer {
        item: ItemStack,
        max_stack_size: i32,
    }

    // SAFETY: This test-only key uniquely identifies `SingleItemContainer`.
    unsafe impl DowncastType for SingleItemContainer {
        const TYPE_KEY: DowncastTypeKey =
            DowncastTypeKey::new("foton:test/container/restricted_slot_single_item");
    }

    impl Container for SingleItemContainer {
        fn items(&self) -> &[ItemStack] {
            slice::from_ref(&self.item)
        }

        fn items_mut(&mut self) -> &mut [ItemStack] {
            slice::from_mut(&mut self.item)
        }

        fn get_max_stack_size(&self) -> i32 {
            self.max_stack_size
        }

        fn set_changed(&mut self) {}
    }

    #[test]
    fn max_stack_size_delegates_to_the_container_and_item() {
        init_vanilla_registry();
        let capped = Arc::new(SyncMutex::new(SingleItemContainer {
            item: ItemStack::empty(),
            max_stack_size: 1,
        }));
        let capped_ref = ContainerRef::from(capped);
        let capped_slot = RestrictedSlot::new(capped_ref.clone(), 0, |_, _| true);
        let mut capped_guard = ContainerLockGuard::lock_all(&[capped_ref]);
        let capped_remainder = capped_slot.safe_insert(
            &mut capped_guard,
            ItemStack::with_count(&vanilla_items::STONE, 64),
            64,
        );
        assert_eq!(capped_slot.get_item(&capped_guard).count(), 1);
        assert_eq!(capped_remainder.count(), 63);

        let default = SimpleContainer::new(1).into_shared();
        let default_ref = ContainerRef::from(default);
        let default_slot = RestrictedSlot::new(default_ref.clone(), 0, |_, _| true);
        let mut default_guard = ContainerLockGuard::lock_all(&[default_ref]);
        let mut stack = ItemStack::new(&vanilla_items::STONE);
        stack.set(MAX_STACK_SIZE, 99);
        stack.set_count(99);
        let default_remainder = default_slot.safe_insert(&mut default_guard, stack, 99);

        assert!(default_remainder.is_empty());
        assert_eq!(default_slot.get_item(&default_guard).count(), 99);
    }
}
