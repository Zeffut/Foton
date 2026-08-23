//! The armor stand.
//!
//! Vanilla parity: `ArmorStand`. It is a `LivingEntity` with no AI at all --
//! the first one in Steel that is not a mob -- and almost everything it does is
//! in two places: swapping gear when a player right-clicks it, and the two-hit
//! rule that breaks it.
//!
//! Not implemented: the six pose rotations. They exist in the synced data with
//! vanilla's defaults, so a stand looks right, but nothing reads or writes them
//! and the `Pose` tag is neither saved nor loaded -- Steel has no NBT codec for
//! `Rotations`, and a half-written one would lose a map-maker's work silently.

use std::sync::Weak;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, Ordering};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::equipment::{EquipmentSlot, EquipmentSlotType};
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_damage_type_tags::DamageTypeTag;
use steel_registry::vanilla_entity_data::ArmorStandEntityData;
use steel_registry::{sound_events, vanilla_items};
use steel_utils::entity_events::EntityStatus;
use steel_utils::locks::SyncMutex;
use steel_utils::types::InteractionHand;
use steel_utils::{DowncastType, DowncastTypeKey};

use crate::behavior::InteractionResult;
use crate::entity::damage::DamageSource;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, LivingEntity, LivingEntityBase,
    RemovalReason,
};
use crate::player::Player;
use crate::world::World;

/// Vanilla parity: the `1` of `DATA_CLIENT_FLAGS`.
const FLAG_SMALL: i8 = 1;
/// Vanilla parity: the `4` of `DATA_CLIENT_FLAGS`.
const FLAG_SHOW_ARMS: i8 = 4;
/// Vanilla parity: the `8` of `DATA_CLIENT_FLAGS`.
const FLAG_NO_BASE_PLATE: i8 = 8;
/// Vanilla parity: the `16` of `DATA_CLIENT_FLAGS`.
const FLAG_MARKER: i8 = 16;

/// How long a first hit counts for.
///
/// Vanilla parity: the `time - this.lastHit > 5L` of `hurtServer`. Two hits
/// inside this window break the stand; one on its own only rattles it.
const DOUBLE_HIT_WINDOW_TICKS: i64 = 5;

/// The health below which a burning stand falls apart.
///
/// Vanilla parity: the `health <= 0.5F` of `causeDamage`.
const BREAK_HEALTH: f32 = 0.5;

/// Vanilla parity: the `0.15F` a burning stand loses per tick of fire.
const FIRE_TICK_DAMAGE: f32 = 0.15;

/// Vanilla parity: the `4.0F` a lava-like source does at once.
const BURN_DAMAGE: f32 = 4.0;

/// How long a stand burns once lit.
///
/// Vanilla parity: the `igniteForSeconds(5.0F)` of `hurtServer`.
const FIRE_TICKS: i32 = 100;

/// An armor stand.
#[entity_behavior(class = "ArmorStand")]
pub struct ArmorStandEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    entity_data: SyncMutex<ArmorStandEntityData>,
    /// Vanilla keeps this off the synced flags, so it is not the entity-wide
    /// invisibility: an invisible stand is immune rather than hidden.
    invisible: AtomicBool,
    /// The tick of the last hit that did not break it.
    last_hit: AtomicI64,
    /// Bit set of locked slots, keyed by slot id plus 0, 8 or 16.
    disabled_slots: AtomicI32,
}

// SAFETY: This key is owned by Steel and uniquely identifies `ArmorStandEntity`.
unsafe impl DowncastType for ArmorStandEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/armor_stand");
}

impl ArmorStandEntity {
    /// Creates one at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates one from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self::new_with_base(
            EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
        )
    }

    fn new_with_base(base: EntityBase, entity_type: EntityTypeRef) -> Self {
        let living_base = LivingEntityBase::new(entity_type);
        let mut entity_data = ArmorStandEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        Self {
            base,
            entity_type,
            living_base,
            entity_data: SyncMutex::new(entity_data),
            invisible: AtomicBool::new(false),
            last_hit: AtomicI64::new(0),
            disabled_slots: AtomicI32::new(0),
        }
    }

    fn flags(&self) -> i8 {
        *self.entity_data.lock().armor_stand().client_flags.get()
    }

    fn has_flag(&self, flag: i8) -> bool {
        self.flags() & flag != 0
    }

    fn set_flag(&self, flag: i8, on: bool) {
        let mut data = self.entity_data.lock();
        let current = *data.armor_stand().client_flags.get();
        let next = if on { current | flag } else { current & !flag };
        if next != current {
            data.armor_stand_mut().client_flags.set(next);
        }
    }

    /// Vanilla parity: `ArmorStand.isSmall`.
    #[must_use]
    pub fn is_small(&self) -> bool {
        self.has_flag(FLAG_SMALL)
    }

    /// Vanilla parity: `ArmorStand.showArms`.
    #[must_use]
    pub fn show_arms(&self) -> bool {
        self.has_flag(FLAG_SHOW_ARMS)
    }

    /// Vanilla parity: `ArmorStand.showBasePlate`, which is stored inverted.
    #[must_use]
    pub fn show_base_plate(&self) -> bool {
        !self.has_flag(FLAG_NO_BASE_PLATE)
    }

    /// Vanilla parity: `ArmorStand.isMarker`.
    #[must_use]
    pub fn is_marker(&self) -> bool {
        self.has_flag(FLAG_MARKER)
    }

    /// Vanilla parity: `ArmorStand.isDisabled`. A hand slot on an armless stand
    /// counts as locked, which is why an ordinary stand refuses a sword.
    fn is_disabled(&self, slot: EquipmentSlot) -> bool {
        let locked = self.disabled_slots.load(Ordering::Relaxed) & (1 << filter_bit(slot, 0)) != 0;
        locked || (slot.slot_type() == EquipmentSlotType::Hand && !self.show_arms())
    }

    /// Picks the slot a click at `local_y` was aimed at.
    ///
    /// Vanilla parity: `ArmorStand.getClickedSlot`. The bands overlap and are
    /// tried in order, so the first filled one wins -- clicking the middle of a
    /// fully dressed stand takes the chestplate, not the leggings.
    fn clicked_slot(&self, local_y: f64) -> EquipmentSlot {
        let small = self.is_small();
        let feet_span = if small { 0.8 } else { 0.45 };
        let chest_low = if small { 1.2 } else { 0.9 };
        let chest_high = if small { 1.9 } else { 1.6 };
        let legs_span = if small { 1.0 } else { 0.8 };

        if (0.1..0.1 + feet_span).contains(&local_y) && self.has_item_in_slot(EquipmentSlot::Feet) {
            EquipmentSlot::Feet
        } else if (chest_low..chest_high).contains(&local_y)
            && self.has_item_in_slot(EquipmentSlot::Chest)
        {
            EquipmentSlot::Chest
        } else if (0.4..0.4 + legs_span).contains(&local_y)
            && self.has_item_in_slot(EquipmentSlot::Legs)
        {
            EquipmentSlot::Legs
        } else if local_y >= 1.6 && self.has_item_in_slot(EquipmentSlot::Head) {
            EquipmentSlot::Head
        } else if !self.has_item_in_slot(EquipmentSlot::MainHand)
            && self.has_item_in_slot(EquipmentSlot::OffHand)
        {
            EquipmentSlot::OffHand
        } else {
            EquipmentSlot::MainHand
        }
    }

    /// Trades what the player is holding for what the stand is wearing.
    ///
    /// Vanilla parity: `ArmorStand.swapItem`. The two extra bits of
    /// `disabledSlots` are what a command block sets to make a stand that can
    /// be dressed but not undressed, or the other way round.
    fn swap_item(
        &self,
        player: &Player,
        slot: EquipmentSlot,
        held: &ItemStack,
        hand: InteractionHand,
    ) -> bool {
        let locked = self.disabled_slots.load(Ordering::Relaxed);
        let mut worn = ItemStack::empty();
        self.with_equipment_slot(slot, &mut |item| worn = item.clone());

        if !worn.is_empty() && locked & (1 << filter_bit(slot, 8)) != 0 {
            return false;
        }
        if worn.is_empty() && locked & (1 << filter_bit(slot, 16)) != 0 {
            return false;
        }

        if player.has_infinite_materials() && worn.is_empty() && !held.is_empty() {
            self.set_equipment(slot, held.copy_with_count(1));
            return true;
        }

        if held.is_empty() || held.count() <= 1 {
            self.set_equipment(slot, held.clone());
            player.inventory.lock().set_item_in_hand(hand, worn);
            return true;
        }

        if !worn.is_empty() {
            return false;
        }

        let mut taken = held.clone();
        let one = taken.split(1);
        self.set_equipment(slot, one);
        player.inventory.lock().set_item_in_hand(hand, taken);
        true
    }

    fn set_equipment(&self, slot: EquipmentSlot, stack: ItemStack) {
        self.with_equipment_slot_mut(slot, &mut |item| *item = stack.clone());
    }

    /// Drops the stand itself and everything it was wearing.
    ///
    /// Vanilla parity: `brokenByPlayer` plus `brokenByAnything`. The gear pops
    /// one block up so it does not land inside whatever the stand was standing
    /// on.
    fn break_apart(&self, drop_the_stand: bool) {
        let Some(world) = self.level() else {
            return;
        };
        world.play_sound_at(
            &sound_events::ENTITY_ARMOR_STAND_BREAK,
            self.sound_source(),
            self.position(),
            1.0,
            1.0,
            None,
        );

        let pos = self.block_position();
        if drop_the_stand {
            world.pop_resource(pos, ItemStack::new(&vanilla_items::ARMOR_STAND));
        }

        // TODO: Vanilla skips a piece enchanted with PREVENT_EQUIPMENT_DROP.
        // Steel has no enchantment effect components, so every piece drops.
        let above = steel_utils::BlockPos::new(pos.x(), pos.y() + 1, pos.z());
        for slot in EquipmentSlot::ALL {
            let mut worn = ItemStack::empty();
            self.with_equipment_slot_mut(slot, &mut |item| {
                worn = item.clone();
                *item = ItemStack::empty();
            });
            if !worn.is_empty() {
                world.pop_resource(above, worn);
            }
        }
    }

    /// Wears a source down rather than breaking it outright.
    ///
    /// Vanilla parity: `ArmorStand.causeDamage`, which is how fire and lava
    /// destroy a stand: not by the two-hit rule but by running its health out.
    fn wear_down(&self, amount: f32) {
        let health = self.get_health() - amount;
        if health <= BREAK_HEALTH {
            self.break_apart(false);
            self.set_removed(RemovalReason::Killed);
        } else {
            self.set_health(health);
        }
    }
}

/// The bit `disabled_slots` uses for a slot.
///
/// Vanilla parity: `EquipmentSlot.getFilterBit`. The offset picks which of the
/// three rules is being asked about: 0 locked outright, 8 cannot be taken from,
/// 16 cannot be put into.
const fn filter_bit(slot: EquipmentSlot, offset: i32) -> i32 {
    slot.id() + offset
}

impl Entity for ArmorStandEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `ArmorStand.isPushable`, which is false -- a stand does
    /// not slide when something walks into it.
    fn is_pushable(&self) -> bool {
        false
    }

    /// Vanilla parity: `ArmorStand.isPickable`, where a marker cannot be hit.
    fn is_pickable(&self) -> bool {
        !self.is_removed() && !self.is_marker()
    }

    /// Vanilla parity: `ArmorStand.isIgnoringBlockTriggers`, which is what lets
    /// a marker sit on a pressure plate without setting it off.
    fn is_ignoring_block_triggers(&self) -> bool {
        self.is_marker()
    }

    fn is_marker_armor_stand(&self) -> bool {
        self.is_marker()
    }

    /// Vanilla parity: `ArmorStand.interact`.
    ///
    /// An empty hand takes whatever the click landed on; a full hand puts the
    /// item where it belongs. Both go through the same swap, which is why
    /// right-clicking a dressed stand with a helmet trades the two.
    fn interact(
        &self,
        player: &Player,
        hand: InteractionHand,
        location: DVec3,
    ) -> InteractionResult {
        let held = {
            let inventory = player.inventory.lock();
            inventory.get_item_in_hand(hand).clone()
        };
        if self.is_marker() || held.is(&vanilla_items::NAME_TAG) {
            return InteractionResult::Pass;
        }
        if player.is_spectator() {
            return InteractionResult::Success;
        }

        // Vanilla parity: `Entity.getEquipmentSlotForItem`, which falls back
        // to the main hand for anything that is not wearable.
        let for_item = held
            .get_equippable_slot()
            .unwrap_or(EquipmentSlot::MainHand);
        if held.is_empty() {
            let clicked = self.clicked_slot(location.y);
            let target = if self.is_disabled(clicked) {
                for_item
            } else {
                clicked
            };
            if self.has_item_in_slot(target) && self.swap_item(player, target, &held, hand) {
                return InteractionResult::SuccessServer;
            }
        } else {
            if self.is_disabled(for_item) {
                return InteractionResult::Fail;
            }
            if for_item.slot_type() == EquipmentSlotType::Hand && !self.show_arms() {
                return InteractionResult::Fail;
            }
            if self.swap_item(player, for_item, &held, hand) {
                return InteractionResult::SuccessServer;
            }
        }

        InteractionResult::Pass
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        nbt.insert(
            "Invisible",
            i8::from(self.invisible.load(Ordering::Relaxed)),
        );
        nbt.insert("Small", i8::from(self.is_small()));
        nbt.insert("ShowArms", i8::from(self.show_arms()));
        nbt.insert("DisabledSlots", self.disabled_slots.load(Ordering::Relaxed));
        nbt.insert("NoBasePlate", i8::from(!self.show_base_plate()));
        if self.is_marker() {
            nbt.insert("Marker", 1i8);
        }
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        let flag = |name: &str| nbt.byte(name).is_some_and(|value| value != 0);
        self.invisible.store(flag("Invisible"), Ordering::Relaxed);
        self.set_flag(FLAG_SMALL, flag("Small"));
        self.set_flag(FLAG_SHOW_ARMS, flag("ShowArms"));
        self.set_flag(FLAG_NO_BASE_PLATE, flag("NoBasePlate"));
        self.set_flag(FLAG_MARKER, flag("Marker"));
        if let Some(disabled) = nbt.int("DisabledSlots") {
            self.disabled_slots.store(disabled, Ordering::Relaxed);
        }
    }
}

impl LivingEntity for ArmorStandEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    fn get_health(&self) -> f32 {
        *self.entity_data.lock().living_entity().health.get()
    }

    fn set_health(&self, health: f32) {
        let clamped = health.clamp(0.0, self.get_max_health());
        self.entity_data
            .lock()
            .living_entity_mut()
            .health
            .set(clamped);
    }

    /// Vanilla parity: `ArmorStand.canUseSlot`.
    fn can_use_slot(&self, slot: EquipmentSlot) -> bool {
        slot != EquipmentSlot::Body && slot != EquipmentSlot::Saddle && !self.is_disabled(slot)
    }

    /// Vanilla parity: `ArmorStand.isAffectedByPotions`.
    fn is_affected_by_potions(&self) -> bool {
        false
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ARMOR_STAND_HIT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ARMOR_STAND_BREAK)
    }

    /// Vanilla parity: `ArmorStand.hurtServer`.
    ///
    /// The order matters and is vanilla's: a stand shrugs almost everything
    /// off, and the one thing a player can do to it needs two hits inside five
    /// ticks -- which is what stops a stray click from destroying somebody's
    /// display.
    fn hurt_server(&self, world: &World, source: &DamageSource, _amount: f32) -> bool {
        if self.is_removed() {
            return false;
        }
        // Not implemented: vanilla also refuses damage from a mob while
        // `mobGriefing` is off. Deciding whether a damage source came from a
        // mob needs a lookup from an entity id back to the entity, which
        // `DamageSource` does not offer here.
        if source.is(&DamageTypeTag::BYPASSES_INVULNERABILITY) {
            self.set_removed(RemovalReason::Killed);
            return false;
        }
        if self.invisible.load(Ordering::Relaxed) || self.is_marker() {
            return false;
        }

        if source.is(&DamageTypeTag::IS_EXPLOSION) {
            self.break_apart(false);
            self.set_removed(RemovalReason::Killed);
            return false;
        }
        if source.is(&DamageTypeTag::IGNITES_ARMOR_STANDS) {
            if self.is_on_fire() {
                self.wear_down(FIRE_TICK_DAMAGE);
            } else {
                self.ignite_for_ticks(FIRE_TICKS);
            }
            return false;
        }
        if source.is(&DamageTypeTag::BURNS_ARMOR_STANDS) && self.get_health() > BREAK_HEALTH {
            self.wear_down(BURN_DAMAGE);
            return false;
        }

        let breakable = source.is(&DamageTypeTag::CAN_BREAK_ARMOR_STAND);
        let always_kills = source.is(&DamageTypeTag::ALWAYS_KILLS_ARMOR_STANDS);
        if !breakable && !always_kills {
            return false;
        }

        let now = world.game_time();
        if now - self.last_hit.load(Ordering::Relaxed) > DOUBLE_HIT_WINDOW_TICKS && !always_kills {
            // Vanilla parity: the first hit only rattles the stand. The client
            // plays the knock from this event.
            self.broadcast_entity_event(EntityStatus::ArmorstandWobble);
            self.last_hit.store(now, Ordering::Relaxed);
        } else {
            self.break_apart(true);
            self.set_removed(RemovalReason::Killed);
        }
        true
    }

    /// Vanilla parity: `ArmorStand.pushEntities`, which is inverted -- a stand
    /// is not pushed, it pushes minecarts out of its own square.
    ///
    /// Not implemented: Steel has no rideable-minecart predicate reachable from
    /// here, and a stand that shoved every entity would be worse than one that
    /// shoves none.
    fn push_entities(&self) {}

    fn server_ai_step(&self) {}
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use steel_registry::{init_vanilla_registry, vanilla_entities, vanilla_items};
    use steel_utils::ChunkPos;

    use super::*;
    use crate::entity::next_entity_id;
    use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};

    fn stand(name: &'static str) -> ArmorStandEntity {
        init_vanilla_registry();
        let world = fresh_test_world(name);
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        ArmorStandEntity::new(
            &vanilla_entities::ARMOR_STAND,
            next_entity_id(),
            DVec3::new(8.5, 64.0, 8.5),
            Arc::downgrade(&world),
        )
    }

    /// The four client flags share one byte, so setting one must not disturb
    /// the others -- a stand set to `marker` that quietly stopped being `small`
    /// would be a hard bug to see.
    #[test]
    fn the_client_flags_are_independent() {
        let stand = stand("armor_stand_flags");

        stand.set_flag(FLAG_SMALL, true);
        stand.set_flag(FLAG_MARKER, true);
        assert!(stand.is_small() && stand.is_marker());

        stand.set_flag(FLAG_SMALL, false);
        assert!(!stand.is_small(), "clearing small cleared the wrong bit");
        assert!(stand.is_marker(), "clearing small also cleared marker");
    }

    /// Vanilla stores the base plate inverted, so the default -- no flag set --
    /// has to mean the plate is shown.
    #[test]
    fn a_fresh_stand_shows_its_base_plate() {
        let stand = stand("armor_stand_base_plate");
        assert!(stand.show_base_plate());

        stand.set_flag(FLAG_NO_BASE_PLATE, true);
        assert!(!stand.show_base_plate());
    }

    /// The three rules `disabled_slots` encodes sit eight bits apart, so a slot
    /// locked against taking must not read as locked outright.
    #[test]
    fn the_three_lock_rules_do_not_overlap() {
        assert_eq!(filter_bit(EquipmentSlot::Head, 0), EquipmentSlot::Head.id());
        assert_eq!(
            filter_bit(EquipmentSlot::Head, 8),
            EquipmentSlot::Head.id() + 8
        );
        assert_eq!(
            filter_bit(EquipmentSlot::Head, 16),
            EquipmentSlot::Head.id() + 16
        );
    }

    /// Vanilla parity: `isDisabled` treats a hand slot on an armless stand as
    /// locked, which is why an ordinary stand refuses a sword but takes a
    /// helmet.
    #[test]
    fn an_armless_stand_refuses_a_hand_slot_but_not_a_helmet() {
        let stand = stand("armor_stand_armless");

        assert!(stand.is_disabled(EquipmentSlot::MainHand));
        assert!(!stand.is_disabled(EquipmentSlot::Head));

        stand.set_flag(FLAG_SHOW_ARMS, true);
        assert!(!stand.is_disabled(EquipmentSlot::MainHand));
    }

    /// A click only picks a slot that has something in it. An empty stand hands
    /// every height back to the main hand, which is what lets a player put the
    /// first piece on wherever they clicked.
    #[test]
    fn a_click_on_an_empty_stand_always_lands_on_the_main_hand() {
        let stand = stand("armor_stand_empty_click");

        for height in [0.2, 0.5, 1.0, 1.8] {
            assert_eq!(stand.clicked_slot(height), EquipmentSlot::MainHand);
        }
    }

    /// Vanilla parity: the bands of `getClickedSlot`. Each one only answers for
    /// the slot it belongs to, so a stand wearing only boots gives up its boots
    /// low down and nothing higher.
    #[test]
    fn a_click_low_on_a_booted_stand_takes_the_boots() {
        let stand = stand("armor_stand_boots");
        stand.set_equipment(
            EquipmentSlot::Feet,
            ItemStack::new(&vanilla_items::IRON_BOOTS),
        );

        assert_eq!(stand.clicked_slot(0.2), EquipmentSlot::Feet);
        assert_eq!(
            stand.clicked_slot(1.8),
            EquipmentSlot::MainHand,
            "a high click should not reach the boots"
        );
    }

    #[test]
    fn a_click_high_on_a_helmeted_stand_takes_the_helmet() {
        let stand = stand("armor_stand_helmet");
        stand.set_equipment(
            EquipmentSlot::Head,
            ItemStack::new(&vanilla_items::IRON_HELMET),
        );

        assert_eq!(stand.clicked_slot(1.8), EquipmentSlot::Head);
    }

    /// With nothing in the main hand but something in the off hand, vanilla
    /// falls through to the off hand rather than answering with an empty slot.
    #[test]
    fn an_empty_main_hand_falls_through_to_the_off_hand() {
        let stand = stand("armor_stand_offhand");
        stand.set_equipment(
            EquipmentSlot::OffHand,
            ItemStack::new(&vanilla_items::STICK),
        );

        assert_eq!(stand.clicked_slot(1.0), EquipmentSlot::OffHand);
    }

    /// Vanilla parity: `canUseSlot`, which is what stops a stand wearing a
    /// saddle or a body slot it has no place for.
    #[test]
    fn a_stand_has_no_body_or_saddle_slot() {
        let stand = stand("armor_stand_slots");

        assert!(!stand.can_use_slot(EquipmentSlot::Body));
        assert!(!stand.can_use_slot(EquipmentSlot::Saddle));
        assert!(stand.can_use_slot(EquipmentSlot::Head));
    }

    /// A marker is the invisible anchor map-makers use: it must not be
    /// clickable, and it must not set off anything it stands on.
    #[test]
    fn a_marker_is_neither_pickable_nor_a_trigger() {
        let stand = stand("armor_stand_marker");
        stand.set_flag(FLAG_MARKER, true);

        assert!(!stand.is_pickable());
        assert!(stand.is_ignoring_block_triggers());
        assert!(stand.is_marker_armor_stand());
    }
    /// Right-clicking a stand with a helmet puts the helmet on its head, not
    /// wherever the click happened to land. This is the one interaction the
    /// whole entity exists for.
    #[test]
    fn a_helmet_goes_on_the_head() {
        init_vanilla_registry();
        let world = fresh_test_world("armor_stand_interact_helmet");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let stand = ArmorStandEntity::new(
            &vanilla_entities::ARMOR_STAND,
            next_entity_id(),
            DVec3::new(8.5, 64.0, 8.5),
            Arc::downgrade(&world),
        );
        let player = TestPlayerBuilder::new(Arc::clone(&world), "StandTester", 1).build();
        player.inventory.lock().set_item_in_hand(
            InteractionHand::MainHand,
            ItemStack::new(&vanilla_items::IRON_HELMET),
        );

        let outcome = stand.interact(&player, InteractionHand::MainHand, DVec3::ZERO);

        assert_eq!(outcome, InteractionResult::SuccessServer);
        let mut worn = ItemStack::empty();
        stand.with_equipment_slot(EquipmentSlot::Head, &mut |item| worn = item.clone());
        assert!(
            worn.is(&vanilla_items::IRON_HELMET),
            "the helmet did not land on the head; it went somewhere else"
        );
    }

    /// An armless stand refuses a sword outright rather than putting it
    /// somewhere else.
    #[test]
    fn an_armless_stand_refuses_a_sword() {
        init_vanilla_registry();
        let world = fresh_test_world("armor_stand_interact_sword");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let stand = ArmorStandEntity::new(
            &vanilla_entities::ARMOR_STAND,
            next_entity_id(),
            DVec3::new(8.5, 64.0, 8.5),
            Arc::downgrade(&world),
        );
        let player = TestPlayerBuilder::new(Arc::clone(&world), "SwordTester", 1).build();
        player.inventory.lock().set_item_in_hand(
            InteractionHand::MainHand,
            ItemStack::new(&vanilla_items::IRON_SWORD),
        );

        let outcome = stand.interact(&player, InteractionHand::MainHand, DVec3::ZERO);

        assert_eq!(outcome, InteractionResult::Fail);
        let mut held = ItemStack::empty();
        stand.with_equipment_slot(EquipmentSlot::MainHand, &mut |item| held = item.clone());
        assert!(held.is_empty(), "an armless stand took the sword anyway");
    }
}
