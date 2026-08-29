//! Bundle item behavior.
//!
//! Vanilla parity: `BundleItem`. The client-only halves of that class -- the
//! fullness bar (`isBarVisible`, `getBarWidth`, `getBarColor`), the grid layout
//! (`getNumberOfItemsToShow`) and `getTooltipImage` -- are left out: a server
//! only ships the `minecraft:bundle_contents` component and the client draws
//! from it. Vanilla's `awardStat(Stats.ITEM_USED)` is also left out because
//! Foton has no statistics system yet.

mod contents;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use foton_macros::item_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::ItemStackTemplate;
use foton_registry::data_components::BundleContents;
use foton_registry::data_components::vanilla_components::BUNDLE_CONTENTS;
use foton_registry::item_stack::ItemStack;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::sound_events;

use crate::behavior::item::ItemUseAnimation;
use crate::behavior::item_utils::on_container_destroyed;
use crate::behavior::{InteractionResult, ItemBehavior, UseItemContext};
use crate::entity::entities::ItemEntity;
use crate::entity::{Entity, LivingEntity};
use crate::inventory::click::MouseButton;
use crate::inventory::lock::ContainerLockGuard;
use crate::inventory::slots::slot::Slot;
use crate::player::Player;
use crate::world::World;

pub use contents::{MutableBundleContents, can_item_be_in_bundle};

/// Vanilla parity: `BundleItem.TICKS_AFTER_FIRST_THROW`.
const TICKS_AFTER_FIRST_THROW: i32 = 10;
/// Vanilla parity: `BundleItem.TICKS_BETWEEN_THROWS`.
const TICKS_BETWEEN_THROWS: i32 = 2;
/// Vanilla parity: `BundleItem.TICKS_MAX_THROW_DURATION`.
const TICKS_MAX_THROW_DURATION: i32 = 200;

/// The bundle and its sixteen dyed variants.
#[item_behavior]
pub struct BundleItem;

impl BundleItem {
    /// Points a bundle's next extraction at `selected_item`.
    ///
    /// Vanilla parity: `BundleItem.toggleSelectedItem`.
    pub fn toggle_selected_item(stack: &mut ItemStack, selected_item: i32) {
        let Some(initial) = stack.get(BUNDLE_CONTENTS) else {
            return;
        };
        let mut contents = MutableBundleContents::new(initial);
        contents.toggle_selected_item(selected_item);
        stack.set(BUNDLE_CONTENTS, contents.to_immutable());
    }

    /// Vanilla parity: `BundleItem.dropContent(ItemStack, Player)`, returning
    /// whether anything left the bundle.
    fn drop_one(stack: &mut ItemStack, player: &Player) -> bool {
        let Some(initial) = stack.get(BUNDLE_CONTENTS) else {
            return false;
        };
        if initial.is_empty() {
            return false;
        }

        let mut contents = MutableBundleContents::new(initial);
        let Some(removed) = contents.remove_one() else {
            return false;
        };
        play_remove_one_sound(player);
        stack.set(BUNDLE_CONTENTS, contents.to_immutable());
        let _ = player.drop_item(removed, true, false);
        true
    }
}

impl ItemBehavior for BundleItem {
    /// Vanilla parity: `BundleItem.overrideStackedOnOther` -- a carried bundle
    /// clicked onto another slot.
    fn override_stacked_on_other(
        &self,
        stack: &mut ItemStack,
        slot: &dyn Slot,
        guard: &mut ContainerLockGuard,
        button: MouseButton,
        player: &Player,
    ) -> bool {
        let Some(initial) = stack.get(BUNDLE_CONTENTS) else {
            return false;
        };
        let mut contents = MutableBundleContents::new(initial);
        let other_is_empty = slot.get_item(guard).is_empty();

        match (button, other_is_empty) {
            (MouseButton::Left, false) => {
                if contents.try_transfer(slot, guard, player) > 0 {
                    play_insert_sound(player);
                } else {
                    play_insert_fail_sound(player);
                }
            }
            (MouseButton::Right, true) => {
                if let Some(removed) = contents.remove_one() {
                    let mut remainder = slot.safe_insert(guard, removed, i32::MAX);
                    if remainder.count() > 0 {
                        contents.try_insert(&mut remainder);
                    } else {
                        play_remove_one_sound(player);
                    }
                }
            }
            _ => return false,
        }

        stack.set(BUNDLE_CONTENTS, contents.to_immutable());
        true
    }

    /// Vanilla parity: `BundleItem.overrideOtherStackedOnMe` -- something
    /// clicked onto a bundle sitting in a slot.
    fn override_other_stacked_on_me(
        &self,
        stack: &mut ItemStack,
        carried: &mut ItemStack,
        allow_modification: bool,
        button: MouseButton,
        player: &Player,
    ) -> bool {
        if button == MouseButton::Left && carried.is_empty() {
            Self::toggle_selected_item(stack, BundleContents::NO_SELECTED_ITEM_INDEX);
            return false;
        }

        let Some(initial) = stack.get(BUNDLE_CONTENTS) else {
            return false;
        };
        let mut contents = MutableBundleContents::new(initial);

        match (button, carried.is_empty()) {
            (MouseButton::Left, false) => {
                if allow_modification && contents.try_insert(carried) > 0 {
                    play_insert_sound(player);
                } else {
                    play_insert_fail_sound(player);
                }
            }
            (MouseButton::Right, true) => {
                if allow_modification && let Some(removed) = contents.remove_one() {
                    play_remove_one_sound(player);
                    *carried = removed;
                }
            }
            _ => {
                Self::toggle_selected_item(stack, BundleContents::NO_SELECTED_ITEM_INDEX);
                return false;
            }
        }

        stack.set(BUNDLE_CONTENTS, contents.to_immutable());
        true
    }

    /// Vanilla parity: `BundleItem.use`.
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        context.player.start_using_item(context.hand);
        InteractionResult::Success
    }

    /// Vanilla parity: `BundleItem.getUseDuration`.
    fn get_use_duration(&self, _stack: &ItemStack, _user: &dyn LivingEntity) -> i32 {
        TICKS_MAX_THROW_DURATION
    }

    /// Vanilla parity: `BundleItem.getUseAnimation`.
    fn get_use_animation(&self, _stack: &ItemStack) -> ItemUseAnimation {
        ItemUseAnimation::Bundle
    }

    /// Vanilla parity: `BundleItem.onUseTick` -- one item on the first tick,
    /// then one every other tick after a short pause.
    fn on_use_tick(
        &self,
        _world: &Arc<World>,
        user: &dyn LivingEntity,
        stack: &mut ItemStack,
        ticks_remaining: i32,
    ) {
        let Some(player) = user.as_player() else {
            return;
        };

        let use_duration = self.get_use_duration(stack, user);
        let is_first_tick = ticks_remaining == use_duration;
        let is_repeat_tick = ticks_remaining < use_duration - TICKS_AFTER_FIRST_THROW
            && ticks_remaining % TICKS_BETWEEN_THROWS == 0;
        if !is_first_tick && !is_repeat_tick {
            return;
        }

        if Self::drop_one(stack, player) {
            play_drop_contents_sound(player);
        }
    }

    /// Vanilla parity: `BundleItem.onDestroyed`.
    fn on_destroyed(&self, entity: &ItemEntity) {
        let mut stack = entity.get_item();
        let Some(contents) = stack.get(BUNDLE_CONTENTS) else {
            return;
        };
        let spilled: Vec<ItemStack> = contents
            .items()
            .iter()
            .map(ItemStackTemplate::create)
            .collect();
        stack.set(BUNDLE_CONTENTS, BundleContents::empty());
        entity.set_item(stack);
        on_container_destroyed(entity, spilled);
    }
}

/// Vanilla parity: `BundleItem.playRemoveOneSound`.
fn play_remove_one_sound(player: &Player) {
    play_bundle_sound(player, &sound_events::ITEM_BUNDLE_REMOVE_ONE);
}

/// Vanilla parity: `BundleItem.playInsertSound`.
fn play_insert_sound(player: &Player) {
    play_bundle_sound(player, &sound_events::ITEM_BUNDLE_INSERT);
}

/// Vanilla parity: `BundleItem.playInsertFailSound`, the one bundle sound with
/// a fixed volume and pitch.
fn play_insert_fail_sound(player: &Player) {
    player.play_sound(&sound_events::ITEM_BUNDLE_INSERT_FAIL, 1.0, 1.0);
}

fn play_bundle_sound(player: &Player, sound: SoundEventRef) {
    player.play_sound(sound, 0.8, 0.4f32.mul_add(rand::random::<f32>(), 0.8));
}

/// Vanilla parity: `BundleItem.playDropContentsSound`, which goes through the
/// level rather than the entity so it lands in the players sound category.
fn play_drop_contents_sound(player: &Player) {
    let world = player.get_world();
    world.play_sound(
        &sound_events::ITEM_BUNDLE_DROP_CONTENTS,
        SoundSource::Players,
        player.block_position(),
        0.8,
        0.4f32.mul_add(rand::random::<f32>(), 0.8),
        None,
    );
}
