//! Piglin entity.
//!
//! Vanilla parity: `net.minecraft.world.entity.monster.piglin.Piglin`. The mob
//! the Nether's economy runs on: it wears gold, hunts hoglins in packs, is
//! calmed by gold armor, and trades a gold ingot for whatever
//! `gameplay/piglin_bartering` rolls.
//!
//! Everything it does is brain-driven -- see [`super::piglin_ai`]. This file is
//! the body: the inventory it carries, the crossbow it fires, the equipment it
//! spawns with, and the clock that turns it into a
//! [`crate::entity::entities::ZombifiedPiglinEntity`] in the overworld.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::data_components::vanilla_components::KINETIC_WEAPON;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::equipment::EquipmentSlot;
use foton_registry::item_stack::ItemStack;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_entity_data::PiglinEntityData;
use foton_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_attributes, vanilla_blocks,
    vanilla_entities, vanilla_items, vanilla_mob_effects,
};
use foton_utils::locks::SyncMutex;
use foton_utils::types::InteractionHand;
use foton_utils::{BlockPos, BlockStateId, Downcast as _, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::behavior::InteractionResult;
use crate::behavior::items::{MOB_ARROW_POWER, crossbow_is_charged, perform_crossbow_attack};
use crate::entity::Enemy;
use crate::entity::LivingEntitySyncedData;
use crate::entity::ai::brain::Brain;
use crate::entity::ai::brain::behavior::{BrainContext, CrossbowAttackHooks};
use crate::entity::conversion::{ConversionParams, convert_to};
use crate::entity::damage::DamageSource;
use crate::entity::entities::ZombifiedPiglinEntity;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySpawnReason, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, MobEffectInstance, PathfinderMob, SharedEntity, SpawnGroupData,
};
use crate::inventory::container::{Container as _, SimpleContainer};
use crate::player::Player;
use crate::world::{LevelReader as _, World};

use super::abstract_piglin::{self, PiglinArmPose};
use super::piglin_ai;
use crate::entity::InventoryCarrier;
use crate::entity::conversion::ConversionReason::PiglinZombification;
use foton_registry::entity_data::EntityPose;
use foton_registry::entity_type::{EntityAttachmentPoint, EntityAttachments, EntityDimensions};
use foton_utils::Identifier;

/// Experience this mob drops.
///
/// Vanilla parity: the `this.xpReward = 5` of the `Piglin` constructor.
const XP_REWARD: i32 = 5;

/// How many stacks a piglin carries.
///
/// Vanilla parity: `Piglin.INVENTORY_SIZE`.
const INVENTORY_SIZE: usize = 8;

/// Chance a spawned adult wears each armor piece.
///
/// Vanilla parity: `Piglin.CHANCE_OF_WEARING_EACH_ARMOUR_ITEM`.
const CHANCE_OF_WEARING_EACH_ARMOUR_ITEM: f32 = 0.1;

/// Chance a naturally spawned piglin is a baby.
///
/// Vanilla parity: `Piglin.PROBABILITY_OF_SPAWNING_AS_BABY`.
const PROBABILITY_OF_SPAWNING_AS_BABY: f32 = 0.2;

/// Chance a spawned adult carries a crossbow rather than a blade.
///
/// Vanilla parity: `Piglin.PROBABILITY_OF_SPAWNING_WITH_CROSSBOW_INSTEAD_OF_SWORD`.
const PROBABILITY_OF_SPAWNING_WITH_CROSSBOW: f32 = 0.5;

/// One in this many blade-carrying piglins gets a spear instead of a sword.
///
/// Vanilla parity: the `random.nextInt(10) == 0` of `Piglin.createSpawnWeapon`.
const ONE_IN_N_BLADES_IS_A_SPEAR: i32 = 10;

/// Speed a baby piglin gains.
///
/// Vanilla parity: `Piglin.SPEED_MODIFIER_BABY`, a multiplied-base modifier.
const SPEED_MODIFIER_BABY: f64 = 0.2;

/// How long the nausea lasts after zombification.
///
/// Vanilla parity: the `new MobEffectInstance(MobEffects.NAUSEA, 200, 0)` of
/// `AbstractPiglin.finishConversion`.
const CONVERSION_NAUSEA_TICKS: i32 = 200;

/// Where a baby piglin sits when it rides a hoglin.
///
/// Vanilla parity: the `EntityAttachment.VEHICLE` of `Piglin.BABY_DIMENSIONS`.
const BABY_VEHICLE_ATTACHMENTS: [EntityAttachmentPoint; 1] =
    [EntityAttachmentPoint::new(0.0, 0.1875, 0.0)];

/// Vanilla parity: `Piglin.BABY_DIMENSIONS`.
const BABY_DIMENSIONS: EntityDimensions = EntityDimensions::new_with_attachments(
    0.49,
    0.98,
    0.78,
    EntityAttachments::new(&[], &BABY_VEHICLE_ATTACHMENTS, &[], &[]),
);

/// Fields a piglin keeps that are neither synced nor on a base.
struct PiglinState {
    /// Vanilla parity: `AbstractPiglin.timeInOverworld`.
    time_in_overworld: i32,
    /// Vanilla parity: `Piglin.cannotHunt`.
    cannot_hunt: bool,
}

/// A piglin.
#[entity_behavior(class = "Piglin")]
pub struct PiglinEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<PiglinEntityData>,
    state: SyncMutex<PiglinState>,
    /// Vanilla parity: the `SimpleContainer(8)` field of `Piglin`.
    inventory: SyncMutex<SimpleContainer>,
    brain: Brain,
}

// SAFETY: This key is owned by Foton and uniquely identifies `PiglinEntity`.
unsafe impl DowncastType for PiglinEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/piglin");
}

impl PiglinEntity {
    /// Creates a piglin at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a piglin from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self::new_with_base(
            EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
        )
    }

    fn new_with_base(base: EntityBase, entity_type: EntityTypeRef) -> Self {
        let living_base = LivingEntityBase::new(entity_type);
        let mob_base = MobBase::new();
        mob_base.set_xp_reward(XP_REWARD);
        let mut entity_data = PiglinEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        let piglin = Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            state: SyncMutex::new(PiglinState {
                time_in_overworld: 0,
                cannot_hunt: false,
            }),
            inventory: SyncMutex::new(SimpleContainer::new(INVENTORY_SIZE)),
            brain: piglin_ai::make_brain(),
        };
        abstract_piglin::apply_constructor(&piglin);
        piglin
    }

    /// The brain, without going through [`Mob::brain`].
    #[must_use]
    pub const fn brain_ref(&self) -> &Brain {
        &self.brain
    }

    /// Vanilla parity: `AbstractPiglin.isAdult`.
    #[must_use]
    pub fn is_adult(&self) -> bool {
        !self.is_baby_piglin()
    }

    fn is_baby_piglin(&self) -> bool {
        *self.entity_data.lock().baby.get()
    }

    /// Vanilla parity: `Piglin.canHunt`.
    #[must_use]
    pub fn piglin_can_hunt(&self) -> bool {
        !self.state.lock().cannot_hunt
    }

    /// Vanilla parity: the private `Piglin.setCannotHunt`.
    pub fn set_cannot_hunt(&self, cannot_hunt: bool) {
        self.state.lock().cannot_hunt = cannot_hunt;
    }

    /// Vanilla parity: `AbstractPiglin.setImmuneToZombification`.
    pub fn set_immune_to_zombification(&self, immune: bool) {
        self.entity_data
            .lock()
            .abstract_piglin_mut()
            .immune_to_zombification
            .set(immune);
    }

    /// Vanilla parity: `AbstractPiglin.isImmuneToZombification`.
    #[must_use]
    pub fn is_immune_to_zombification(&self) -> bool {
        *self
            .entity_data
            .lock()
            .abstract_piglin()
            .immune_to_zombification
            .get()
    }

    /// Vanilla parity: `AbstractPiglin.isConverting`.
    #[must_use]
    pub fn piglin_is_converting(&self) -> bool {
        let Some(world) = self.level() else {
            return false;
        };
        !self.is_immune_to_zombification()
            && !self.is_no_ai()
            && world.dimension_type.piglins_zombify
    }

    /// Sets how long this piglin has stood in the overworld.
    ///
    /// Vanilla parity: the `@VisibleForTesting AbstractPiglin.setTimeInOverworld`.
    pub fn set_time_in_overworld(&self, time_in_overworld: i32) {
        self.state.lock().time_in_overworld = time_in_overworld;
    }

    /// Returns how long this piglin has stood in the overworld.
    #[must_use]
    pub fn time_in_overworld(&self) -> i32 {
        self.state.lock().time_in_overworld
    }

    /// Vanilla parity: `Piglin.isDancing`.
    #[must_use]
    pub fn is_dancing(&self) -> bool {
        *self.entity_data.lock().is_dancing.get()
    }

    /// Vanilla parity: `Piglin.setDancing`.
    pub fn set_dancing(&self, dancing: bool) {
        self.entity_data.lock().is_dancing.set(dancing);
    }

    /// Vanilla parity: the private `Piglin.isChargingCrossbow`.
    #[must_use]
    pub fn is_charging_crossbow(&self) -> bool {
        *self.entity_data.lock().is_charging_crossbow.get()
    }

    /// Vanilla parity: `Piglin.setChargingCrossbow`.
    pub fn set_charging_crossbow(&self, charging: bool) {
        self.entity_data.lock().is_charging_crossbow.set(charging);
    }

    /// What this piglin is doing with its hands, for the client.
    ///
    /// Vanilla parity: `Piglin.getArmPose`.
    #[must_use]
    pub fn arm_pose(&self) -> PiglinArmPose {
        if self.is_dancing() {
            return PiglinArmPose::Dancing;
        }
        if piglin_ai::is_loved_item(&self.get_item_in_hand(InteractionHand::OffHand)) {
            return PiglinArmPose::AdmiringItem;
        }
        if Mob::is_aggressive(self) && abstract_piglin::is_holding_melee_weapon(self) {
            return PiglinArmPose::AttackingWithMeleeWeapon;
        }
        if self.is_charging_crossbow() {
            return PiglinArmPose::CrossbowCharge;
        }
        let main_hand = self.get_item_in_hand(InteractionHand::MainHand);
        if main_hand.is(&vanilla_items::CROSSBOW) && crossbow_is_charged(&main_hand) {
            return PiglinArmPose::CrossbowHold;
        }
        PiglinArmPose::Default
    }

    /// Adds `item_stack` to the carried inventory, returning what did not fit.
    ///
    /// Vanilla parity: `Piglin.addToInventory`, which delegates to
    /// `SimpleContainer.addItem`.
    pub fn add_to_inventory(&self, item_stack: ItemStack) -> ItemStack {
        let mut remaining = item_stack;
        self.inventory.lock().add(&mut remaining);
        remaining
    }

    /// Whether the carried inventory has room for `item_stack`.
    ///
    /// Vanilla parity: `Piglin.canAddToInventory`, which delegates to
    /// `SimpleContainer.canAddItem`.
    #[must_use]
    pub fn can_add_to_inventory(&self, item_stack: &ItemStack) -> bool {
        let inventory = self.inventory.lock();
        for slot in 0..inventory.get_container_size() {
            let existing = inventory.get_item(slot);
            if existing.is_empty() {
                return true;
            }
            if ItemStack::is_same_item_same_components(existing, item_stack)
                && existing.count() + item_stack.count() <= existing.max_stack_size()
            {
                return true;
            }
        }
        false
    }

    /// Empties the carried inventory.
    ///
    /// Vanilla parity: `SimpleContainer.removeAllItems`.
    pub fn remove_all_inventory_items(&self) -> Vec<ItemStack> {
        let mut inventory = self.inventory.lock();
        let mut taken = Vec::new();
        for slot in 0..inventory.get_container_size() {
            let item = inventory.remove_item_no_update(slot);
            if !item.is_empty() {
                taken.push(item);
            }
        }
        taken
    }

    /// Vanilla parity: `Piglin.holdInMainHand`.
    pub fn hold_in_main_hand(&self, item_stack: ItemStack) {
        Mob::set_item_slot_and_drop_when_killed(self, EquipmentSlot::MainHand, item_stack);
        Mob::set_persistence_required(self);
    }

    /// Vanilla parity: `Piglin.holdInOffHand`. A gold ingot -- the barter
    /// currency -- deliberately does not pin the piglin in place, so a traded
    /// piglin still despawns.
    pub fn hold_in_off_hand(&self, item_stack: ItemStack) {
        let is_currency = piglin_ai::is_barter_currency(&item_stack);
        Mob::set_item_slot_and_drop_when_killed(self, EquipmentSlot::OffHand, item_stack);
        if !is_currency {
            Mob::set_persistence_required(self);
        }
    }

    /// Vanilla parity: the private `Piglin.createSpawnWeapon`.
    fn create_spawn_weapon() -> ItemStack {
        if rand::random::<f32>() < PROBABILITY_OF_SPAWNING_WITH_CROSSBOW {
            return ItemStack::new(&vanilla_items::CROSSBOW);
        }
        if rand::random_range(0..ONE_IN_N_BLADES_IS_A_SPEAR) == 0 {
            return ItemStack::new(&vanilla_items::GOLDEN_SPEAR);
        }
        ItemStack::new(&vanilla_items::GOLDEN_SWORD)
    }

    /// Vanilla parity: the private `Piglin.maybeWearArmor`.
    fn maybe_wear_armor(&self, slot: EquipmentSlot, item_stack: ItemStack) {
        if rand::random::<f32>() < CHANCE_OF_WEARING_EACH_ARMOUR_ITEM {
            self.set_item_slot(slot, item_stack);
        }
    }

    /// Vanilla parity: `Piglin.populateDefaultEquipmentSlots`.
    fn populate_default_equipment_slots(&self) {
        if !self.is_adult() {
            return;
        }
        self.maybe_wear_armor(
            EquipmentSlot::Head,
            ItemStack::new(&vanilla_items::GOLDEN_HELMET),
        );
        self.maybe_wear_armor(
            EquipmentSlot::Chest,
            ItemStack::new(&vanilla_items::GOLDEN_CHESTPLATE),
        );
        self.maybe_wear_armor(
            EquipmentSlot::Legs,
            ItemStack::new(&vanilla_items::GOLDEN_LEGGINGS),
        );
        self.maybe_wear_armor(
            EquipmentSlot::Feet,
            ItemStack::new(&vanilla_items::GOLDEN_BOOTS),
        );
    }

    /// Turns this piglin into a zombified piglin.
    ///
    /// Vanilla parity: `Piglin.finishConversion`, which cancels the admiring,
    /// spills the carried inventory, and then runs `AbstractPiglin`'s own
    /// conversion with `ConversionParams.single(this, true, true)`.
    pub fn finish_conversion(&self) -> Option<Arc<ZombifiedPiglinEntity>> {
        piglin_ai::cancel_admiring(self);
        for item in self.remove_all_inventory_items() {
            let _ = self.spawn_at_location(item, 0.0);
        }

        let equipment: Vec<(EquipmentSlot, ItemStack)> = EquipmentSlot::ALL
            .into_iter()
            .map(|slot| (slot, self.get_item_by_slot(slot)))
            .filter(|(_, item)| !item.is_empty())
            .collect();

        convert_to(
            self,
            ConversionParams::single(true, true).with_reason(PiglinZombification),
            |id, position, world| {
                ZombifiedPiglinEntity::new(&vanilla_entities::ZOMBIFIED_PIGLIN, id, position, world)
            },
            |zombified| {
                // The baby flag comes across in `copy_common_state`.
                // Vanilla's `keepEquipment` moves the slots inside
                // `ConversionType.SINGLE.convert`; Foton's conversion leaves
                // that to the caller, so a piglin's gold armor comes across
                // here rather than being lost.
                for (slot, item) in equipment {
                    zombified.set_item_slot(slot, item);
                }
                zombified
                    .living_base()
                    .add_mob_effect(MobEffectInstance::with_duration(
                        vanilla_mob_effects::NAUSEA,
                        CONVERSION_NAUSEA_TICKS,
                        0,
                    ));
            },
        )
    }

    /// The hooks the brain's [`crate::entity::ai::brain::behavior::CrossbowAttack`]
    /// drives this piglin through.
    #[must_use]
    pub fn crossbow_hooks() -> CrossbowAttackHooks {
        CrossbowAttackHooks {
            set_charging_crossbow: |ctx: &BrainContext<'_>, charging: bool| {
                if let Some(piglin) = ctx.mob().downcast_ref::<Self>() {
                    piglin.set_charging_crossbow(charging);
                }
            },
            perform_ranged_attack: |ctx: &BrainContext<'_>, target: &SharedEntity, _power: f32| {
                let Some(piglin) = ctx.mob().downcast_ref::<Self>() else {
                    return;
                };
                // Vanilla parity: `Piglin.performRangedAttack` ignores the
                // power the behavior passes and fires at `MOB_ARROW_POWER`.
                perform_crossbow_attack(ctx.world(), piglin, target, MOB_ARROW_POWER);
                // Vanilla parity: `Piglin.onCrossbowAttackPerformed`.
                piglin.set_no_action_time(0);
            },
        }
    }
}

/// Vanilla parity: `Piglin.SPEED_MODIFIER_BABY_ID`, the `minecraft:baby`
/// modifier every baby mob shares.
static BABY_SPEED_MODIFIER_ID: Identifier = Identifier::vanilla_static("baby");

/// Vanilla parity: `Piglin.getPreferredWeaponType`, which is `ItemTags.PIGLIN_PREFERRED_WEAPONS`.
static PIGLIN_PREFERRED_WEAPONS: Identifier =
    Identifier::vanilla_static("piglin_preferred_weapons");

/// The hooks the fight activity hands to the crossbow behavior.
#[must_use]
pub fn crossbow_attack_hooks() -> CrossbowAttackHooks {
    PiglinEntity::crossbow_hooks()
}

/// Returns whether a piglin may appear at `pos`.
///
/// Vanilla parity: `Piglin.checkPiglinSpawnRules`.
#[must_use]
fn check_piglin_spawn_rules(world: &Arc<World>, pos: BlockPos) -> bool {
    use foton_registry::blocks::block_state_ext::BlockStateExt as _;

    world.get_block_state(pos.below()).get_block() != &vanilla_blocks::NETHER_WART_BLOCK
}

impl abstract_piglin::ConvertiblePiglin for PiglinEntity {
    fn is_converting(&self) -> bool {
        self.piglin_is_converting()
    }

    fn bump_time_in_overworld(&self, converting: bool) -> i32 {
        let mut state = self.state.lock();
        if converting {
            state.time_in_overworld += 1;
        } else {
            state.time_in_overworld = 0;
        }
        state.time_in_overworld
    }

    /// Vanilla parity: `Piglin.playConvertedSound`.
    fn play_converted_sound(&self) {
        self.make_sound(Some(&sound_events::ENTITY_PIGLIN_CONVERTED_TO_ZOMBIFIED));
    }

    fn convert_to_zombified(&self) {
        self.finish_conversion();
    }
}

impl Entity for PiglinEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);
    }

    /// Vanilla parity: `Piglin.getDefaultDimensions`.
    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let scale = LivingEntity::get_scale(self);
        if self.is_baby_piglin() {
            BABY_DIMENSIONS.scale(scale)
        } else if self.entity_type.fixed {
            self.entity_type.dimensions
        } else {
            self.entity_type.dimensions.scale(scale)
        }
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    /// Vanilla parity: `Piglin.playStepSound`.
    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {
        self.play_sound(&sound_events::ENTITY_PIGLIN_STEP, 0.15, 1.0);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        let state = self.state.lock();
        nbt.insert("IsImmuneToZombification", self.is_immune_to_zombification());
        nbt.insert("TimeInOverworld", state.time_in_overworld);
        nbt.insert("IsBaby", self.is_baby_piglin());
        nbt.insert("CannotHunt", state.cannot_hunt);
        drop(state);
        abstract_piglin::save_inventory(&self.inventory.lock(), nbt);
        self.brain.save(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        Mob::set_can_pick_up_loot(self, nbt.byte("CanPickUpLoot").is_none_or(|flag| flag != 0));
        self.set_immune_to_zombification(nbt.byte("IsImmuneToZombification").unwrap_or(0) != 0);
        self.set_baby(nbt.byte("IsBaby").unwrap_or(0) != 0);
        {
            let mut state = self.state.lock();
            state.time_in_overworld = nbt.int("TimeInOverworld").unwrap_or(0);
            state.cannot_hunt = nbt.byte("CannotHunt").unwrap_or(0) != 0;
        }
        abstract_piglin::load_inventory(&mut self.inventory.lock(), nbt);
        self.brain.load(nbt);
    }
}

impl LivingEntity for PiglinEntity {
    /// Returns synchronized data declared by vanilla `LivingEntity`.
    fn living_synced_data(&self) -> Option<&dyn LivingEntitySyncedData> {
        Some(&self.entity_data)
    }

    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    /// Vanilla parity: the `Mob.serverAiStep` a piglin inherits, which is the
    /// only path to [`Mob::custom_server_ai_step`] and so to the brain.
    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn get_health(&self) -> f32 {
        *self.entity_data.lock().living_entity().health.get()
    }

    fn set_health(&self, health: f32) {
        let max_health = self.get_max_health();
        let clamped = health.clamp(0.0, max_health);
        self.entity_data
            .lock()
            .living_entity_mut()
            .health
            .set(clamped);
    }

    fn is_baby(&self) -> bool {
        self.is_baby_piglin()
    }

    /// Vanilla parity: `Piglin.hurtServer`, which routes the hit into the brain.
    fn hurt_server(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        let was_hurt = self.living_hurt_server(world, source, amount);
        if !was_hurt {
            return false;
        }
        let Some(world) = self.level() else {
            return true;
        };
        let Some(attacker) = source
            .causing_entity_id
            .and_then(|id| world.get_entity_by_id(id))
        else {
            return true;
        };
        if let Some(living) = attacker.as_living_entity() {
            piglin_ai::was_hurt_by(&world, self, &attacker, living);
        }
        true
    }

    /// Vanilla parity: `Piglin.getBaseExperienceReward`, which returns the flat
    /// `xpReward` -- so a piglin in full gold armor is still worth five, not
    /// five plus a bonus for every piece.
    fn base_experience_reward(&self) -> i32 {
        Mob::xp_reward(self)
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_PIGLIN_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_PIGLIN_DEATH)
    }
}

impl Mob for PiglinEntity {
    /// Vanilla parity: `Piglin.setBaby`, which also carries the speed modifier
    /// that is the only reason a baby piglin can run you down.
    fn set_baby(&self, baby: bool) {
        use crate::entity::attribute::{AttributeModifier, AttributeModifierOperation};

        self.entity_data.lock().baby.set(baby);
        {
            let mut attributes = self.attributes().lock();
            attributes.remove_modifier(vanilla_attributes::MOVEMENT_SPEED, &BABY_SPEED_MODIFIER_ID);
            if baby {
                attributes.add_modifier(
                    vanilla_attributes::MOVEMENT_SPEED,
                    AttributeModifier {
                        id: BABY_SPEED_MODIFIER_ID.clone(),
                        amount: SPEED_MODIFIER_BABY,
                        operation: AttributeModifierOperation::AddMultipliedBase,
                    },
                    false,
                );
            }
        }
        self.refresh_dimensions();
    }

    /// Vanilla parity: `Piglin` derives from `AbstractPiglin`, which is a
    /// `Monster`.
    fn is_monster(&self) -> bool {
        true
    }

    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn brain(&self) -> Option<&Brain> {
        Some(&self.brain)
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    /// Vanilla parity: `Piglin.canUseNonMeleeWeapon`, which is what keeps a
    /// crossbow piglin at range instead of closing to punch.
    fn can_use_non_melee_weapon(&self, item_stack: &ItemStack) -> bool {
        item_stack.is(&vanilla_items::CROSSBOW) || item_stack.has(KINETIC_WEAPON)
    }

    /// Vanilla parity: `Piglin.canHunt`, reached through `AbstractPiglin`.
    fn can_hunt(&self) -> bool {
        self.piglin_can_hunt()
    }

    /// Vanilla parity: `Piglin.getPreferredWeaponType`, the tag that makes a
    /// piglin swap a sword for a crossbow but never the other way.
    fn preferred_weapon_type(&self) -> Option<&'static Identifier> {
        (!self.is_baby_piglin()).then_some(&PIGLIN_PREFERRED_WEAPONS)
    }

    /// Vanilla parity: `Piglin.canReplaceCurrentItem`, which puts wanting an
    /// item ahead of the armor and damage comparisons the base makes.
    fn can_replace_current_item(
        &self,
        new_item_stack: &ItemStack,
        current_item_stack: &ItemStack,
        slot: EquipmentSlot,
    ) -> bool {
        use foton_registry::enchantment_effect::EnchantmentEffectComponent;

        if current_item_stack.has_enchantment_effect(EnchantmentEffectComponent::PreventArmorChange)
        {
            return false;
        }

        let preferred = self.preferred_weapon_type();
        let wanted = |stack: &ItemStack| {
            piglin_ai::is_loved_item(stack)
                || preferred.is_some_and(|tag| REGISTRY.items.is_in_tag(stack.item(), tag))
        };
        let new_wanted = wanted(new_item_stack);
        let current_wanted = wanted(current_item_stack);
        if new_wanted && !current_wanted {
            return true;
        }
        if !new_wanted && current_wanted {
            return false;
        }
        self.mob_can_replace_current_item(new_item_stack, current_item_stack, slot)
    }

    /// Vanilla parity: `Piglin.wantsToPickUp`.
    fn wants_to_pick_up(&self, world: &World, item_stack: &ItemStack) -> bool {
        use foton_registry::vanilla_game_rules::MOB_GRIEFING;

        world.get_game_rule(&MOB_GRIEFING)
            && Mob::can_pick_up_loot(self)
            && piglin_ai::wants_to_pickup(self, item_stack)
    }

    /// Vanilla parity: `Piglin.pickUpItem`. Vanilla also calls
    /// `onItemPickup`, which only fires an advancement trigger Foton has no
    /// advancements for yet.
    fn pick_up_item(&self, _world: &Arc<World>, item_entity: &SharedEntity) {
        piglin_ai::pick_up_item(self, item_entity);
    }

    /// Vanilla parity: `Piglin.customServerAiStep`, which is the brain tick,
    /// the activity update, and then `AbstractPiglin`'s conversion clock.
    fn custom_server_ai_step(&self) {
        let Some(world) = self.level() else {
            return;
        };
        self.brain.tick(&world, self);
        if let Some(sound) = piglin_ai::update_activity(self) {
            self.make_sound(Some(sound));
        }
        abstract_piglin::tick_conversion(self);
    }

    /// Vanilla parity: `Piglin.getAmbientSound` through
    /// `AbstractPiglin.playAmbientSound`, which only speaks while idle.
    fn ambient_sound(&self) -> Option<SoundEventRef> {
        if !piglin_ai::is_idle(&self.brain) {
            return None;
        }
        piglin_ai::sound_for_current_activity(self)
    }

    /// Vanilla parity: `Piglin.removeWhenFarAway`, which keeps a piglin that
    /// has been handed something.
    fn remove_when_far_away(&self, _distance_squared: f64) -> bool {
        !self.is_persistence_required()
    }

    /// Vanilla parity: `Piglin.mobInteract`, the hand-feeding that starts a
    /// barter.
    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        piglin_ai::mob_interact(self, player, hand)
    }

    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        _spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        check_piglin_spawn_rules(world, pos)
    }

    /// Vanilla parity: `Piglin.finalizeSpawn`.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        // Vanilla parity: a structure-placed piglin keeps whatever the
        // structure gave it, so neither the baby roll nor the weapon runs.
        if spawn_reason != EntitySpawnReason::Structure {
            if rand::random::<f32>() < PROBABILITY_OF_SPAWNING_AS_BABY {
                self.set_baby(true);
            } else if self.is_adult() {
                self.set_item_slot(EquipmentSlot::MainHand, Self::create_spawn_weapon());
            }
        }

        piglin_ai::init_memories(&self.brain);
        self.populate_default_equipment_slots();
        self.finalize_spawn_mob_base(world, spawn_reason, group_data)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for PiglinEntity {}

impl InventoryCarrier for PiglinEntity {
    fn carried_inventory(&self) -> &SyncMutex<SimpleContainer> {
        &self.inventory
    }
}

impl Enemy for PiglinEntity {}
