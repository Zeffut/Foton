//! Wolf entity.
//!
//! Vanilla parity: `Wolf`. The first tameable mob in Steel, and the one that
//! exercises every part of [`TamableAnimal`]: it is tamed with bones, it sits
//! where it is told, it wears a dyed collar, it fights whoever fights its
//! owner, and it is a [`NeutralMob`] on top of all that -- a wild wolf hit once
//! keeps hunting the attacker long after losing sight of them.

use std::str::FromStr;
use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtTag};
use steel_macros::entity_behavior;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::data_components::vanilla_components::DYE;
use steel_registry::entity_type::{
    EntityAttachmentPoint, EntityAttachments, EntityDimensions, EntityTypeRef,
};
use steel_registry::item_stack::ItemStack;
use steel_registry::item_stack_template::ItemStackTemplate;
use steel_registry::particle_type::{ItemParticleOption, ParticleData};
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_entity_data::WolfEntityData;
use steel_registry::vanilla_entity_type_tags::EntityTypeTag;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::wolf_sound_variant::{WolfAge, WolfSoundVariantRef};
use steel_registry::wolf_variant::WolfVariantRef;
use steel_registry::{
    DyeColor, REGISTRY, RegistryExt, RegistryReference, TaggedRegistryExt, sound_events,
    vanilla_attributes, vanilla_damage_type_tags, vanilla_entities, vanilla_game_events,
    vanilla_items, vanilla_particle_types,
};
use steel_utils::entity_events::EntityStatus;
use steel_utils::locks::SyncMutex;
use steel_utils::random::legacy_random::LegacyRandom;
use steel_utils::types::InteractionHand;
use steel_utils::{
    BlockPos, BlockStateId, Downcast as _, DowncastType, DowncastTypeKey, Identifier,
};
use uuid::Uuid;

use crate::behavior::InteractionResult;
use crate::entity::ai::goal::{
    AvoidEntityGoal, BegGoal, BreedGoal, FloatGoal, FollowOwnerGoal, HurtByTargetGoal,
    LeapAtTargetGoal, LookAtPlayerGoal, MeleeAttackGoal, NearestAttackableTargetGoal,
    NonTameRandomTargetGoal, OwnerHurtTargetGoal, RandomLookAroundGoal,
    ResetUniversalAngerTargetGoal, SitWhenOrderedToGoal, TamableAnimalPanicGoal,
    WaterAvoidingRandomStrollGoal,
};
use crate::entity::ai::path::PathType;
use crate::entity::damage::DamageSource;
use crate::entity::entities::SheepEntity;
use crate::entity::neutral_mob::{
    NeutralMob, PersistentAnger, read_persistent_anger, resolve_anger_target,
};
use crate::entity::{
    AgeableMob, AgeableMobBase, Animal, AnimalBase, Crackiness, Entity, EntityBase, EntityBaseLoad,
    EntityPose, EntitySpawnReason, EntitySyncedData, LivingEntity, LivingEntityBase,
    LivingEntitySyncedData, Mob, MobBase, PathfinderMob, SpawnGroupData, TamableAnimal,
    TamableAnimalBase, is_tamed,
};
use crate::inventory::equipment::EquipmentSlot;
use crate::physics::MoveResult;
use crate::player::Player;
use crate::world::{LevelReader as _, World};

/// The wolf's baby hitbox.
///
/// Vanilla parity: `Wolf.BABY_DIMENSIONS`.
const WOLF_BABY_PASSENGER_ATTACHMENTS: [EntityAttachmentPoint; 1] =
    [EntityAttachmentPoint::new(0.0, 0.4375, 0.0)];
const WOLF_BABY_DIMENSIONS: EntityDimensions = EntityDimensions::new_with_attachments(
    0.3,
    0.425,
    0.34375,
    EntityAttachments::new(&WOLF_BABY_PASSENGER_ATTACHMENTS, &[], &[], &[]),
);

/// Maximum health of a wild wolf.
///
/// Vanilla parity: `Wolf.START_HEALTH`.
const START_HEALTH: f64 = 8.0;

/// Maximum health of a tamed wolf.
///
/// Vanilla parity: `Wolf.TAME_HEALTH`. Taming quintuples it, which is why a
/// tamed wolf survives a creeper and a wild one does not.
const TAME_HEALTH: f64 = 40.0;

/// Fraction of its maximum durability one repair restores to wolf armor.
///
/// Vanilla parity: `Wolf.ARMOR_REPAIR_UNIT`.
const ARMOR_REPAIR_UNIT: f32 = 0.125;

/// Default collar color.
///
/// Vanilla parity: `Wolf.DEFAULT_COLLAR_COLOR`.
const DEFAULT_COLLAR_COLOR: DyeColor = DyeColor::Red;

/// Health below which a tamed wolf whines instead of panting.
///
/// Vanilla parity: the `getHealth() < 20.0F` of `Wolf.getAmbientSound`.
const WHINE_HEALTH: f32 = 20.0;

/// One chance in this many that a calm wolf pants or whines rather than yips.
///
/// Vanilla parity: the `random.nextInt(3) == 0` of `Wolf.getAmbientSound`.
const PANT_CHANCE: i32 = 3;

/// One chance in this many that a bone tames the wolf.
///
/// Vanilla parity: the `random.nextInt(3) == 0` of `Wolf.tryToTame`.
const TAME_CHANCE: i32 = 3;

/// Shortest grudge, in ticks.
///
/// Vanilla parity: `Wolf.PERSISTENT_ANGER_TIME`, twenty to thirty-nine seconds.
const ANGER_MIN_TICKS: i64 = 20 * 20;
/// Longest grudge, in ticks.
const ANGER_MAX_TICKS: i64 = 39 * 20;

/// How far a wolf tilts its head down while sitting.
///
/// Vanilla parity: the `20` of `Wolf.getMaxHeadXRot`.
const SITTING_MAX_HEAD_X_ROT: f32 = 20.0;

/// Volume every wolf sound plays at.
///
/// Vanilla parity: `Wolf.getSoundVolume`.
const WOLF_SOUND_VOLUME: f32 = 0.4;

/// How much of the shake animation one tick advances.
///
/// Vanilla parity: the `shakeAnim += 0.05F` of `Wolf.tick`.
const SHAKE_ANIM_STEP: f32 = 0.05;

/// Where the shake animation ends.
///
/// Vanilla parity: the `shakeAnimO >= 2.0F` of `Wolf.tick`.
const SHAKE_ANIM_END: f32 = 2.0;

/// How wet a wolf is and how far through shaking it off.
///
/// Vanilla keeps four loose fields; they only ever change together, so Steel
/// bundles them behind one lock.
#[derive(Debug, Clone, Copy, Default)]
struct WolfShakeState {
    is_wet: bool,
    is_shaking: bool,
    shake_anim: f32,
    shake_anim_o: f32,
}

impl WolfShakeState {
    /// Vanilla parity: `Wolf.cancelShake`.
    const fn cancel(&mut self) {
        self.is_shaking = false;
        self.shake_anim = 0.0;
        self.shake_anim_o = 0.0;
    }
}

/// Vanilla wolf entity.
#[entity_behavior(class = "Wolf")]
pub struct WolfEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    tamable_base: TamableAnimalBase,
    anger: PersistentAnger,
    shake: SyncMutex<WolfShakeState>,
    entity_data: SyncMutex<WolfEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `WolfEntity`.
unsafe impl DowncastType for WolfEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/wolf");
}

impl WolfEntity {
    /// Creates a new wolf entity.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a wolf entity from saved base data.
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
        let ageable_base = AgeableMobBase::new();
        let animal_base = AnimalBase::new();
        AnimalBase::initialize_pathfinding_malus(&mob_base);
        let mut entity_data = WolfEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        let wolf = Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            tamable_base: TamableAnimalBase::new(),
            anger: PersistentAnger::new(),
            shake: SyncMutex::new(WolfShakeState::default()),
            entity_data: SyncMutex::new(entity_data),
        };

        // Vanilla parity: the constructor's `setTame(false, false)` and the two
        // powder-snow maluses that keep a wolf out of it.
        wolf.set_tame(false, false);
        wolf.set_pathfinding_malus(PathType::PowderSnow, -1.0);
        wolf.set_pathfinding_malus(PathType::OnTopOfPowderSnow, -1.0);
        wolf.register_goals();
        wolf
    }

    /// Vanilla parity: `Wolf.registerGoals`.
    fn register_goals(&self) {
        {
            let mut goals = self.mob_base.goal_selector().lock();
            goals.add_goal(1, FloatGoal::new(&self.mob_base));
            goals.add_goal(
                1,
                TamableAnimalPanicGoal::with_damage_types(
                    1.5,
                    vanilla_damage_type_tags::DamageTypeTag::PANIC_ENVIRONMENTAL_CAUSES,
                ),
            );
            goals.add_goal(2, SitWhenOrderedToGoal::new());
            goals.add_goal(3, wolf_avoid_llama_goal());
            goals.add_goal(4, LeapAtTargetGoal::new(0.4));
            goals.add_goal(5, MeleeAttackGoal::new(1.0, true));
            goals.add_goal(6, FollowOwnerGoal::new(1.0, 10.0, 2.0));
            goals.add_goal(7, BreedGoal::new(1.0));
            goals.add_goal(8, WaterAvoidingRandomStrollGoal::new(1.0));
            goals.add_goal(
                9,
                BegGoal::new(8.0, |item_stack| {
                    item_stack.is(&vanilla_items::BONE) || Self::is_wolf_food(item_stack)
                }),
            );
            goals.add_goal(10, LookAtPlayerGoal::new(8.0));
            goals.add_goal(10, RandomLookAroundGoal::new());
        }

        let mut targets = self.mob_base.target_selector().lock();
        targets.add_goal(1, OwnerHurtTargetGoal::hurt_by_owner_attacker());
        targets.add_goal(2, OwnerHurtTargetGoal::owners_current_victim());
        targets.add_goal(3, HurtByTargetGoal::new().set_alert_others([]));
        targets.add_goal(
            4,
            NearestAttackableTargetGoal::new_for_players_with_interval(
                10,
                true,
                false,
                |targeter, target, _| {
                    let Some(wolf) = targeter.and_then(|t| t.downcast_ref::<Self>()) else {
                        return false;
                    };
                    wolf.level()
                        .is_some_and(|world| wolf.is_angry_at(target, &world))
                },
            ),
        );
        targets.add_goal(
            5,
            NonTameRandomTargetGoal::new(false, |_, target, _| {
                let entity_type = target.entity_type();
                entity_type == &vanilla_entities::SHEEP
                    || entity_type == &vanilla_entities::RABBIT
                    || entity_type == &vanilla_entities::FOX
            }),
        );
        // Vanilla parity: `NonTameRandomTargetGoal<>(this, Turtle.class, false,
        // Turtle.BABY_ON_LAND_SELECTOR)` is not registered: Steel has no turtle
        // yet, and `Turtle.BABY_ON_LAND_SELECTOR` belongs to that class.
        targets.add_goal(
            7,
            NearestAttackableTargetGoal::new(false, |_, target, _| {
                REGISTRY
                    .entity_types
                    .is_in_tag(target.entity_type(), &EntityTypeTag::SKELETONS)
            }),
        );
        targets.add_goal(8, ResetUniversalAngerTargetGoal::new(true));
    }

    /// Returns the current wolf variant.
    #[must_use]
    pub fn variant(&self) -> WolfVariantRef {
        self.entity_data.lock().variant.get().value()
    }

    /// Sets the current wolf variant by registry entry.
    pub fn set_variant(&self, variant: WolfVariantRef) {
        self.entity_data
            .lock()
            .variant
            .set(RegistryReference::new(variant));
    }

    /// Returns the current wolf sound variant.
    #[must_use]
    pub fn sound_variant(&self) -> WolfSoundVariantRef {
        self.entity_data.lock().sound_variant.get().value()
    }

    /// Sets the current wolf sound variant by registry entry.
    pub fn set_sound_variant(&self, sound_variant: WolfSoundVariantRef) {
        self.entity_data
            .lock()
            .sound_variant
            .set(RegistryReference::new(sound_variant));
    }

    fn set_variant_by_key(&self, key: &Identifier) -> bool {
        let Some(variant) = REGISTRY.wolf_variants.by_key(key) else {
            return false;
        };
        self.set_variant(variant);
        true
    }

    fn set_sound_variant_by_key(&self, key: &Identifier) {
        if let Some(sound_variant) = REGISTRY.wolf_sound_variants.by_key(key) {
            self.set_sound_variant(sound_variant);
        }
    }

    /// Vanilla parity: `Wolf.getSoundSet`.
    fn sound_set(&self) -> &'static WolfAge {
        let sound_variant = self.sound_variant();
        if AgeableMob::is_baby(self) {
            &sound_variant.baby_sounds
        } else {
            &sound_variant.adult_sounds
        }
    }

    /// Returns the collar color.
    ///
    /// Vanilla parity: `Wolf.getCollarColor`.
    #[must_use]
    pub fn collar_color(&self) -> DyeColor {
        DyeColor::by_id(*self.entity_data.lock().collar_color.get())
    }

    fn set_collar_color(&self, color: DyeColor) {
        self.entity_data.lock().collar_color.set(color.id());
    }

    /// Returns vanilla `Wolf.isInterested`.
    #[must_use]
    pub fn is_interested(&self) -> bool {
        *self.entity_data.lock().interested.get()
    }

    /// Sets vanilla `Wolf.setIsInterested`.
    pub fn set_is_interested(&self, interested: bool) {
        self.entity_data.lock().interested.set(interested);
    }

    /// Returns whether the stack is vanilla wolf food.
    #[must_use]
    pub fn is_wolf_food(item_stack: &ItemStack) -> bool {
        REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::WOLF_FOOD)
    }

    /// Returns whether wolf armor would soak this damage.
    ///
    /// Vanilla parity: `Wolf.canArmorAbsorb`.
    fn can_armor_absorb(&self, source: &DamageSource) -> bool {
        if source.is(&vanilla_damage_type_tags::DamageTypeTag::BYPASSES_WOLF_ARMOR) {
            return false;
        }

        let mut wears_wolf_armor = false;
        self.with_equipment_slot(EquipmentSlot::Body, &mut |stack| {
            wears_wolf_armor = stack.is(&vanilla_items::WOLF_ARMOR);
        });
        wears_wolf_armor
    }

    /// Vanilla parity: `Wolf.tryToTame`.
    fn try_to_tame(&self, player: &Player) {
        if rand::random_range(0..TAME_CHANCE) != 0 {
            self.spawn_taming_particles(false);
            return;
        }

        self.tame(player);
        self.mob_base.navigation().lock().stop();
        self.set_target(None);
        self.set_ordered_to_sit(true);
        self.spawn_taming_particles(true);
    }

    /// Vanilla parity: the collar-dye branch of `Wolf.mobInteract`.
    fn try_dye_collar(
        &self,
        player: &Player,
        hand: InteractionHand,
        item_stack: &ItemStack,
    ) -> bool {
        let Some(color) = item_stack.get(DYE).copied() else {
            return false;
        };
        if color == self.collar_color() {
            return false;
        }

        self.set_collar_color(color);
        Mob::use_player_item(self, player, hand);
        true
    }

    /// Vanilla parity: the armor-equip branch of `Wolf.mobInteract`.
    fn try_equip_body_armor(
        &self,
        player: &Player,
        hand: InteractionHand,
        item_stack: &ItemStack,
    ) -> bool {
        if !LivingEntity::is_equippable_in_slot(self, item_stack, EquipmentSlot::Body)
            || self.has_item_in_slot(EquipmentSlot::Body)
            || !self.is_owned_by(player)
            || AgeableMob::is_baby(self)
        {
            return false;
        }

        self.living_base
            .equipment()
            .lock()
            .set(EquipmentSlot::Body, item_stack.copy_with_count(1));
        Mob::set_guaranteed_drop(self, EquipmentSlot::Body);
        Mob::use_player_item(self, player, hand);
        true
    }

    /// Vanilla parity: the armor-repair branch of `Wolf.mobInteract`.
    fn try_repair_body_armor(
        &self,
        player: &Player,
        hand: InteractionHand,
        item_stack: &ItemStack,
    ) -> bool {
        if !self.is_in_sitting_pose() || !self.is_owned_by(player) {
            return false;
        }

        let mut repaired = false;
        self.with_equipment_slot_mut(EquipmentSlot::Body, &mut |armor| {
            if armor.is_empty() || !armor.is_damaged() || !armor.is_valid_repair_item(item_stack) {
                return;
            }
            let repair_unit = (armor.get_max_damage() as f32 * ARMOR_REPAIR_UNIT) as i32;
            let repaired_damage = (armor.get_damage_value() - repair_unit).max(0);
            armor.set_damage_value(repaired_damage);
            repaired = true;
        });

        if !repaired {
            return false;
        }

        player.inventory.lock().shrink_item_in_hand(hand, 1);
        self.play_sound(&sound_events::ITEM_WOLF_ARMOR_REPAIR, 1.0, 1.0);
        true
    }

    /// Vanilla parity: the tamed half of `Wolf.mobInteract`.
    fn tamed_interact(
        &self,
        player: &Player,
        hand: InteractionHand,
        item_stack: &ItemStack,
    ) -> InteractionResult {
        if Self::is_wolf_food(item_stack) && self.get_health() < self.get_max_health() {
            self.feed(player, hand, item_stack, 2.0, 2.0);
            return InteractionResult::Success;
        }

        let is_collar_dye = REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::WOLF_COLLAR_DYES);
        if is_collar_dye && self.is_owned_by(player) {
            return if self.try_dye_collar(player, hand, item_stack) {
                InteractionResult::Success
            } else {
                Animal::mob_interact_animal(self, player, hand)
            };
        }

        if self.try_equip_body_armor(player, hand, item_stack)
            || self.try_repair_body_armor(player, hand, item_stack)
        {
            return InteractionResult::Success;
        }

        let interaction_result = Animal::mob_interact_animal(self, player, hand);
        if interaction_result.consumes_action() || !self.is_owned_by(player) {
            return interaction_result;
        }

        self.set_ordered_to_sit(!self.is_ordered_to_sit());
        self.set_jumping(false);
        self.mob_base.navigation().lock().stop();
        self.set_target(None);
        // Vanilla returns `InteractionResult.SUCCESS.withoutItem()`; Steel has
        // no without-item variant, and the sit toggle consumes nothing anyway.
        InteractionResult::Success
    }

    /// Runs the shake state machine.
    ///
    /// Vanilla parity: the `Wolf.tick` half that is not the interested-angle
    /// interpolation. The splash particles are client-local; the sound, the
    /// game event and the two entity events are not.
    fn tick_shake(&self) {
        if !Entity::is_alive(self) {
            return;
        }

        if self.is_in_water_or_rain() {
            let cancel = {
                let mut shake = self.shake.lock();
                shake.is_wet = true;
                let was_shaking = shake.is_shaking;
                if was_shaking {
                    shake.cancel();
                }
                was_shaking
            };
            if cancel {
                self.broadcast_entity_event(EntityStatus::CancelShakeWetness);
            }
            return;
        }

        let start_of_shake = {
            let shake = self.shake.lock();
            if !shake.is_shaking {
                return;
            }
            shake.shake_anim == 0.0
        };

        if start_of_shake {
            let pitch = (rand::random::<f32>() - rand::random::<f32>()).mul_add(0.2, 1.0);
            self.play_sound(&sound_events::ENTITY_WOLF_SHAKE, WOLF_SOUND_VOLUME, pitch);
            self.game_event(&vanilla_game_events::ENTITY_ACTION);
        }

        let mut shake = self.shake.lock();
        shake.shake_anim_o = shake.shake_anim;
        shake.shake_anim += SHAKE_ANIM_STEP;
        if shake.shake_anim_o >= SHAKE_ANIM_END {
            shake.is_wet = false;
            shake.cancel();
        }
        // VANILLA CLIENT-LOCAL: `Wolf.tick` spawns the splash particles with
        // `Level.addParticle` once `shakeAnim` passes 0.4.
    }

    /// Starts a shake once the wolf is out of the water and standing still.
    ///
    /// Vanilla parity: the first half of `Wolf.aiStep`.
    fn start_shake_if_dry_land(&self) {
        let should_start = {
            let shake = self.shake.lock();
            shake.is_wet && !shake.is_shaking
        };
        if !should_start || self.is_path_finding() || !self.on_ground() {
            return;
        }

        {
            let mut shake = self.shake.lock();
            shake.is_shaking = true;
            shake.shake_anim = 0.0;
            shake.shake_anim_o = 0.0;
        }
        self.broadcast_entity_event(EntityStatus::ShakeWetness);
    }

    fn update_dirty_mob_effect_entity_data(&self) {
        if !self.living_base.take_effects_dirty() {
            return;
        }

        let display = self.living_base.mob_effect_display_state();

        {
            let mut entity_data = self.entity_data.lock();
            let living = entity_data.living_entity_mut();
            living.effect_particles.set(display.particles);
            living.effect_ambience.set(display.ambient);
        }

        self.entity_data.set_base_invisible_flag(display.invisible);
        self.entity_data
            .set_base_glowing_flag(self.has_glowing_tag() || display.glowing);
    }
}

/// The goal that keeps wild wolves away from llamas.
///
/// Vanilla parity: `Wolf.WolfAvoidEntityGoal`. Steel has no llama yet, so the
/// goal is registered with the type and tame checks it can answer and never
/// fires; the strength roll (`llama.getStrength() >= random.nextInt(5)`) and
/// the `setTarget(null)` in `start`/`tick` arrive with `Llama`.
fn wolf_avoid_llama_goal() -> AvoidEntityGoal {
    AvoidEntityGoal::with_selector(24.0, 1.5, 1.5, |targeter, target, _| {
        target.entity_type() == &vanilla_entities::LLAMA
            && !targeter.is_some_and(|wolf| is_tamed(wolf.as_entity_event_source()))
    })
}

/// Returns whether a wolf may appear at `pos`.
///
/// Vanilla parity: `Wolf.checkWolfSpawnRules`.
#[must_use]
fn check_wolf_spawn_rules(world: &Arc<World>, pos: BlockPos) -> bool {
    world
        .get_block_state(pos.below())
        .get_block()
        .has_tag(&BlockTag::WOLVES_SPAWNABLE_ON)
        && <WolfEntity as Animal>::is_bright_enough_to_spawn(world.as_ref(), pos)
}

impl Entity for WolfEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);
    }

    fn tick(&self) {
        LivingEntity::tick_living_entity(self);
        self.tick_shake();
    }

    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let scale = LivingEntity::get_scale(self);
        if AgeableMob::is_baby(self) {
            WOLF_BABY_DIMENSIONS.scale(scale)
        } else if self.entity_type.fixed {
            self.entity_type.dimensions
        } else {
            self.entity_type.dimensions.scale(scale)
        }
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn update_data_before_sync(&self) {
        self.update_dirty_mob_effect_entity_data();
    }

    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {
        self.play_sound(self.sound_set().step_sound, 0.15, 1.0);
    }

    fn is_allied_to(&self, other: &dyn Entity) -> bool {
        self.considers_entity_as_ally_tamable(other)
    }

    fn is_tame_owned_by(&self, owner: &dyn LivingEntity) -> bool {
        self.is_tame() && self.is_owned_by(owner.as_entity_event_source())
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
        self.save_tamable_animal(nbt);
        nbt.insert(
            "CollarColor",
            i8::try_from(self.collar_color().id()).unwrap_or(0),
        );
        nbt.insert("variant", self.variant().key.to_string());
        nbt.insert("sound_variant", self.sound_variant().key.to_string());
        nbt.insert("anger_end_time", self.persistent_anger_end_time());
        if let Some(target) = self.persistent_anger_target() {
            nbt.insert(
                "angry_at",
                NbtTag::IntArray(steel_utils::UuidExt::to_int_array(&target).to_vec()),
            );
        }
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
        self.load_tamable_animal(nbt);

        if let Some(variant) = nbt.string("variant")
            && let Ok(key) = Identifier::from_str(variant.to_str().as_ref())
        {
            self.set_variant_by_key(&key);
        }
        if let Some(sound_variant) = nbt.string("sound_variant")
            && let Ok(key) = Identifier::from_str(sound_variant.to_str().as_ref())
        {
            self.set_sound_variant_by_key(&key);
        }
        self.set_collar_color(
            nbt.byte("CollarColor")
                .map_or(DEFAULT_COLLAR_COLOR, |id| DyeColor::by_id(i32::from(id))),
        );

        let angry_at = nbt
            .int_array("angry_at")
            .and_then(|values| <Uuid as steel_utils::UuidExt>::from_int_array(&values));
        read_persistent_anger(
            self,
            nbt.long("anger_end_time"),
            nbt.int("AngerTime"),
            angry_at,
        );
        if let Some(world) = self.level()
            && let Some(target) = resolve_anger_target(&world, angry_at)
        {
            self.set_target(Some(&target));
        }
    }
}

impl LivingEntity for WolfEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    fn living_synced_data(&self) -> Option<&dyn LivingEntitySyncedData> {
        Some(&self.entity_data)
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

    fn sound_volume(&self) -> f32 {
        WOLF_SOUND_VOLUME
    }

    fn hurt_sound(&self, source: &DamageSource) -> Option<SoundEventRef> {
        if self.can_armor_absorb(source) {
            return Some(&sound_events::ITEM_WOLF_ARMOR_DAMAGE);
        }
        Some(self.sound_set().hurt_sound)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(self.sound_set().death_sound)
    }

    /// Vanilla parity: the `Wolf.hurtServer` override, which stands a sitting
    /// wolf up before anything else happens to it.
    fn hurt_server(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        if self.is_invulnerable_to(world, source) {
            return false;
        }

        self.set_ordered_to_sit(false);
        self.living_hurt_server(world, source, amount)
    }

    /// Vanilla parity: `Wolf.actuallyHurt`, where armor eats the whole hit
    /// rather than reducing it.
    fn actually_hurt(&self, world: &World, source: &DamageSource, amount: f32) {
        if !self.can_armor_absorb(source) {
            self.living_actually_hurt(world, source, amount);
            return;
        }

        let mut cracked_further = false;
        self.with_equipment_slot_mut(EquipmentSlot::Body, &mut |armor| {
            let damage_before = armor.get_damage_value();
            let max_damage = armor.get_max_damage();
            armor.hurt_and_break(amount.ceil() as i32, false);
            cracked_further = Crackiness::WOLF_ARMOR.by_damage(damage_before, max_damage)
                != Crackiness::WOLF_ARMOR.by_stack(armor);
        });

        if !cracked_further {
            return;
        }

        self.play_sound(&sound_events::ITEM_WOLF_ARMOR_CRACK, 1.0, 1.0);
        let position = self.position();
        world.send_particles(
            ParticleData::new(
                &vanilla_particle_types::ITEM,
                ItemParticleOption::new(ItemStackTemplate::new(&vanilla_items::ARMADILLO_SCUTE)),
            ),
            DVec3::new(position.x, position.y + 1.0, position.z),
            20,
            DVec3::new(0.2, 0.1, 0.2),
            0.1,
        );
    }

    fn hurt_armor(&self, source: &DamageSource, damage: f32) {
        self.do_hurt_equipment(source, damage, &[EquipmentSlot::Body]);
    }

    fn can_use_slot(&self, slot: EquipmentSlot) -> bool {
        slot != EquipmentSlot::Body || (Entity::is_alive(self) && !AgeableMob::is_baby(self))
    }

    fn die(&self, source: &DamageSource) {
        self.notify_owner_of_death(source);
        self.shake.lock().cancel();
        self.shake.lock().is_wet = false;
        self.living_die(source);
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn ai_step(&self) -> Option<MoveResult> {
        self.start_shake_if_dry_land();
        let result = self.default_ai_step();
        AgeableMob::tick_ageable_mob(self);
        Animal::tick_animal_love(self);
        if let Some(world) = self.level() {
            self.update_persistent_anger(&world, true);
        }
        result
    }
}

impl AgeableMob for WolfEntity {
    fn ageable_base(&self) -> &AgeableMobBase {
        &self.ageable_base
    }

    fn is_age_locked(&self) -> bool {
        *self.entity_data.lock().ageable_mob().age_locked.get()
    }

    fn set_age_locked(&self, age_locked: bool) {
        self.entity_data
            .lock()
            .ageable_mob_mut()
            .age_locked
            .set(age_locked);
    }

    fn set_synced_baby(&self, baby: bool) {
        self.entity_data.lock().ageable_mob_mut().baby.set(baby);
    }
}

impl Animal for WolfEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        Self::is_wolf_food(item_stack)
    }

    /// Vanilla parity: `Wolf.canMate`. Two tamed wolves breed, and neither may
    /// be sitting.
    fn can_mate(&self, partner: &dyn Animal) -> bool {
        if self.uuid() == partner.uuid() || !self.is_tame() {
            return false;
        }
        let Some(other) = partner.as_entity_event_source().downcast_ref::<Self>() else {
            return false;
        };
        if !other.is_tame() || other.is_in_sitting_pose() {
            return false;
        }

        self.is_in_love() && other.is_in_love()
    }

    fn breed_variant_key(&self) -> Option<&Identifier> {
        Some(&self.variant().key)
    }

    fn set_breed_variant_key(&self, key: &Identifier) -> bool {
        self.set_variant_by_key(key)
    }

    /// Vanilla parity: `Wolf.getBreedOffspring`. A pup inherits one parent's
    /// coat, the owner, and the two collars mixed as dyes.
    fn initialize_breed_offspring(&self, partner: &dyn Animal, offspring: &dyn Animal) {
        let partner_wolf = partner.as_entity_event_source().downcast_ref::<Self>();
        let Some(pup) = offspring.as_entity_event_source().downcast_ref::<Self>() else {
            return;
        };

        let inherit_from_self = rand::random::<bool>();
        let variant = match (inherit_from_self, partner_wolf) {
            (false, Some(partner_wolf)) => partner_wolf.variant(),
            _ => self.variant(),
        };
        pup.set_variant(variant);

        if self.is_tame() {
            pup.set_owner_uuid(self.owner_uuid());
            pup.set_tame(true, true);
            if let Some(partner_wolf) = partner_wolf {
                pup.set_collar_color(mixed_collar_color(
                    self.collar_color(),
                    partner_wolf.collar_color(),
                ));
            }
        }

        if let Some(sound_variant) = REGISTRY
            .wolf_sound_variants
            .pick_random(&mut LegacyRandom::from_seed(rand::random()))
        {
            pup.set_sound_variant(sound_variant);
        }
    }
}

impl TamableAnimal for WolfEntity {
    fn tamable_base(&self) -> &TamableAnimalBase {
        &self.tamable_base
    }

    fn tamable_flags(&self) -> i8 {
        *self.entity_data.lock().tamable_animal().flags.get()
    }

    fn set_tamable_flags(&self, flags: i8) {
        self.entity_data
            .lock()
            .tamable_animal_mut()
            .flags
            .set(flags);
    }

    fn owner_uuid(&self) -> Option<Uuid> {
        *self.entity_data.lock().tamable_animal().owneruuid.get()
    }

    fn set_owner_uuid(&self, owner: Option<Uuid>) {
        self.entity_data
            .lock()
            .tamable_animal_mut()
            .owneruuid
            .set(owner);
    }

    /// Vanilla parity: `Wolf.applyTamingSideEffects`.
    fn apply_taming_side_effects(&self) {
        if self.is_tame() {
            self.attributes()
                .lock()
                .set_base_value(vanilla_attributes::MAX_HEALTH, TAME_HEALTH);
            self.set_health(TAME_HEALTH as f32);
        } else {
            self.attributes()
                .lock()
                .set_base_value(vanilla_attributes::MAX_HEALTH, START_HEALTH);
        }
    }

    /// Vanilla parity: `Wolf.wantsToAttack`. This is the rule that stops a
    /// wolf pack from turning on the owner's other pets or on a creeper.
    fn wants_to_attack(&self, target: &dyn LivingEntity, owner: &dyn Entity) -> bool {
        let target_entity = target.as_entity_event_source();
        let target_type = target.entity_type();
        if target_type == &vanilla_entities::CREEPER
            || target_type == &vanilla_entities::GHAST
            || target_type == &vanilla_entities::ARMOR_STAND
        {
            return false;
        }

        if let Some(other_wolf) = target_entity.downcast_ref::<Self>() {
            return !other_wolf.is_tame() || !other_wolf.is_owned_by(owner);
        }

        // Vanilla parity gap: the `owner.canHarmPlayer(target)` branch spares a
        // teammate of the owner. Steel has no scoreboard teams, and vanilla's
        // check returns true for a teamless player, so it is a no-op today.

        !is_tamed(target_entity)
    }
}

impl Mob for WolfEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    fn custom_server_ai_step(&self) {
        Animal::custom_server_ai_step_animal(self);
    }

    fn can_attack(&self, target: &dyn LivingEntity) -> bool {
        self.can_attack_tamable(target)
    }

    fn max_head_x_rot(&self) -> f32 {
        if self.is_in_sitting_pose() {
            SITTING_MAX_HEAD_X_ROT
        } else {
            30.0
        }
    }

    fn can_be_leashed(&self) -> bool {
        !self.is_angry()
    }

    fn can_shear_equipment(&self, player: &Player) -> bool {
        self.is_owned_by(player)
    }

    /// Vanilla parity: `Wolf.getAmbientSound`.
    fn ambient_sound(&self) -> Option<SoundEventRef> {
        let sound_set = self.sound_set();
        if self.is_angry() {
            return Some(sound_set.growl_sound);
        }
        if rand::random_range(0..PANT_CHANCE) != 0 {
            return Some(sound_set.ambient_sound);
        }

        if self.is_tame() && self.get_health() < WHINE_HEALTH {
            Some(sound_set.whine_sound)
        } else {
            Some(sound_set.pant_sound)
        }
    }

    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        _spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        check_wolf_spawn_rules(world, pos)
    }

    /// Vanilla parity: `Wolf.finalizeSpawn`, which keeps a whole pack in one
    /// coat by carrying the picked variant in the group data.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        let mut random = LegacyRandom::from_seed(rand::random());
        if let Some(variant) = world.biome_at(self.block_position()).and_then(|biome| {
            REGISTRY
                .wolf_variants
                .select_spawn_variant(biome, &mut random)
        }) {
            self.set_variant(variant);
        }
        // Vanilla parity gap: `Wolf.WolfPackData` carries the pack's variant so
        // every wolf of one spawn group matches. Steel's `SpawnGroupData` has
        // no wolf case yet, so each wolf re-rolls; the biome conditions make
        // that identical in every biome with a single matching variant.

        if let Some(sound_variant) = REGISTRY.wolf_sound_variants.pick_random(&mut random) {
            self.set_sound_variant(sound_variant);
        }

        self.finalize_spawn_ageable_mob(world, spawn_reason, group_data)
    }

    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        let item_stack = {
            let inventory = player.inventory.lock();
            let item_stack = inventory.get_item_in_hand(hand);
            item_stack.copy_with_count(item_stack.count())
        };

        if self.is_tame() {
            return self.tamed_interact(player, hand, &item_stack);
        }

        if item_stack.is(&vanilla_items::BONE) && !self.is_angry() {
            Mob::use_player_item(self, player, hand);
            self.try_to_tame(player);
            return InteractionResult::SuccessServer;
        }

        Animal::mob_interact_animal(self, player, hand)
    }
}

impl PathfinderMob for WolfEntity {}

impl NeutralMob for WolfEntity {
    fn persistent_anger(&self) -> &PersistentAnger {
        &self.anger
    }

    /// Vanilla parity: `Wolf.getPersistentAngerEndTime`, which reads the
    /// synchronized field so the client can draw an angry coat.
    fn persistent_anger_end_time(&self) -> i64 {
        *self.entity_data.lock().anger_end_time.get()
    }

    fn set_persistent_anger_end_time(&self, end_time: i64) {
        self.entity_data.lock().anger_end_time.set(end_time);
    }

    /// Vanilla parity: `Wolf.startPersistentAngerTimer`.
    fn start_persistent_anger_timer(&self) {
        self.set_time_to_remain_angry(rand::random_range(ANGER_MIN_TICKS..=ANGER_MAX_TICKS));
    }
}

/// Mixes two collar colors the way vanilla mixes two dyes.
///
/// Vanilla parity: `DyeColor.getMixedColor`, which looks the pair up in the
/// crafting recipes. [`crate::entity::entities::SheepEntity`] already owns that
/// lookup, so the wolf borrows it rather than duplicating the recipe search.
fn mixed_collar_color(first: DyeColor, second: DyeColor) -> DyeColor {
    SheepEntity::get_mixed_color(first, second)
}

#[cfg(test)]
mod tests;
