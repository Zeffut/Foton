//! Goat entity.
//!
//! Vanilla parity: `Goat`. Everything a goat does on its own -- ramming, the
//! long jump, wandering, following a tempting item -- lives in `GoatAi`, which
//! is a `Brain` of behaviors rather than a goal list. Steel has no `Brain`
//! system, so this file is the goat itself: the screaming variant, the two
//! horns and where they come from, the milking, the softened fall, the sounds,
//! the breeding and the spawn rules. See the note on [`GoatEntity::drop_horn`]
//! for exactly what the missing brain would have driven.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::data_components::InstrumentComponent;
use steel_registry::data_components::vanilla_components::INSTRUMENT;
use steel_registry::entity_type::{
    EntityAttachmentPoint, EntityAttachments, EntityDimensions, EntityTypeRef,
};
use steel_registry::instrument::InstrumentRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_entity_data::GoatEntityData;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{
    REGISTRY, RegistryHolder, TaggedRegistryExt as _, sound_events, vanilla_attributes,
    vanilla_instrument_tags, vanilla_items,
};
use steel_utils::locks::SyncMutex;
use steel_utils::random::Random as _;
use steel_utils::random::legacy_random::LegacyRandom;
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, BlockStateId, Downcast as _, DowncastType, DowncastTypeKey};

use crate::behavior::InteractionResult;
use crate::entity::ai::path::PathType;
use crate::entity::damage::DamageSource;
use crate::entity::{
    AgeableMob, AgeableMobBase, Animal, AnimalBase, Entity, EntityBase, EntityBaseLoad, EntityPose,
    EntitySpawnReason, EntitySyncedData, LivingEntity, LivingEntityBase, LivingEntitySyncedData,
    Mob, MobBase, MoveResult, PathfinderMob, SpawnGroupData,
};
use crate::player::Player;
use crate::world::World;

/// Vanilla `Goat.LONG_JUMPING_DIMENSION_SCALE_FACTOR`.
const LONG_JUMPING_DIMENSION_SCALE_FACTOR: f32 = 0.7;
/// Vanilla `Goat.BABY_SCALE`.
const BABY_SCALE: f32 = 0.55;
/// Vanilla `Goat.ADULT_ATTACK_DAMAGE`.
const ADULT_ATTACK_DAMAGE: f64 = 2.0;
/// Vanilla `Goat.BABY_ATTACK_DAMAGE`.
const BABY_ATTACK_DAMAGE: f64 = 1.0;
/// Vanilla `Goat.GOAT_FALL_DAMAGE_REDUCTION`.
const GOAT_FALL_DAMAGE_REDUCTION: i32 = 10;
/// Vanilla `Goat.GOAT_SCREAMING_CHANCE`.
const GOAT_SCREAMING_CHANCE: f64 = 0.02;
/// Vanilla `Goat.UNIHORN_CHANCE`.
const UNIHORN_CHANCE: f32 = 0.1;
/// Vanilla `Goat.getMaxHeadYRot`.
const MAX_HEAD_Y_ROT: f32 = 15.0;

/// The passenger attachment of `Goat.BABY_DIMENSIONS`.
const BABY_PASSENGER_ATTACHMENTS: [EntityAttachmentPoint; 1] =
    [EntityAttachmentPoint::new(0.0, 0.531_25, 0.0)];
/// Vanilla `Goat.BABY_DIMENSIONS`.
const BABY_DIMENSIONS: EntityDimensions = EntityDimensions::new_with_attachments(
    0.45,
    0.65,
    0.593_75,
    EntityAttachments::new(&BABY_PASSENGER_ATTACHMENTS, &[], &[], &[]),
);

/// Which horn a goat still has.
///
/// Vanilla keeps these as two separate synchronized booleans; naming the side
/// keeps the drop order readable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HornSide {
    Left,
    Right,
}

/// A goat.
#[entity_behavior(class = "Goat")]
pub struct GoatEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    entity_data: SyncMutex<GoatEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `GoatEntity`.
unsafe impl DowncastType for GoatEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/goat");
}

impl GoatEntity {
    /// Creates a goat at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a goat from saved base data.
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
        {
            // Vanilla parity: the `Goat` constructor. A goat swims rather than
            // drowns, and treats powder snow as a wall instead of a road.
            mob_base.navigation().lock().set_can_float(true);
            let mut malus = mob_base.pathfinding_malus().lock();
            malus.set(PathType::PowderSnow, -1.0);
            malus.set(PathType::OnTopOfPowderSnow, -1.0);
        }
        let mut entity_data = GoatEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        // Vanilla `Goat` registers no goals at all: `GoatAi` supplies every
        // behavior through the mob's `Brain`, which Steel has not built yet.

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            entity_data: SyncMutex::new(entity_data),
        }
    }

    /// Returns vanilla `Goat.isScreamingGoat`.
    #[must_use]
    pub fn is_screaming_goat(&self) -> bool {
        *self.entity_data.lock().is_screaming_goat.get()
    }

    /// Sets vanilla `Goat.setScreamingGoat`.
    pub fn set_screaming_goat(&self, is_screaming_goat: bool) {
        self.entity_data
            .lock()
            .is_screaming_goat
            .set(is_screaming_goat);
    }

    /// Returns vanilla `Goat.hasLeftHorn`.
    #[must_use]
    pub fn has_left_horn(&self) -> bool {
        *self.entity_data.lock().has_left_horn.get()
    }

    /// Returns vanilla `Goat.hasRightHorn`.
    #[must_use]
    pub fn has_right_horn(&self) -> bool {
        *self.entity_data.lock().has_right_horn.get()
    }

    fn set_horn(&self, side: HornSide, present: bool) {
        let mut entity_data = self.entity_data.lock();
        match side {
            HornSide::Left => entity_data.has_left_horn.set(present),
            HornSide::Right => entity_data.has_right_horn.set(present),
        }
    }

    /// Returns vanilla `Goat.getMilkingSound`.
    #[must_use]
    fn milking_sound(&self) -> SoundEventRef {
        if self.is_screaming_goat() {
            &sound_events::ENTITY_GOAT_SCREAMING_MILK
        } else {
            &sound_events::ENTITY_GOAT_MILK
        }
    }

    /// Builds the horn this goat would shed.
    ///
    /// Vanilla parity: `Goat.createHorn`. The instrument is drawn from a
    /// random source seeded with the goat's UUID, so the same goat always
    /// yields the same horn no matter when it drops it.
    #[must_use]
    pub fn create_horn(&self) -> ItemStack {
        let mut stack = ItemStack::new(&vanilla_items::GOAT_HORN);
        let Some(instrument) = self.pick_horn_instrument() else {
            return stack;
        };

        stack.set(
            INSTRUMENT,
            InstrumentComponent::new(RegistryHolder::reference(instrument)),
        );
        stack
    }

    fn pick_horn_instrument(&self) -> Option<InstrumentRef> {
        let tag = if self.is_screaming_goat() {
            &vanilla_instrument_tags::InstrumentTag::SCREAMING_GOAT_HORNS
        } else {
            &vanilla_instrument_tags::InstrumentTag::REGULAR_GOAT_HORNS
        };
        let instruments = REGISTRY.instruments.get_tag(tag)?;
        if instruments.is_empty() {
            return None;
        }

        let mut random =
            LegacyRandom::from_seed(i64::from(java_uuid_hash_code(self.uuid())) as u64);
        let index = random.next_i32_bounded(instruments.len() as i32) as usize;
        instruments.get(index).copied()
    }

    /// Sheds one horn and drops it on the ground.
    ///
    /// Vanilla parity: `Goat.dropHorn`, which is called from the `RamTarget`
    /// behavior when a charging goat hits a wall or an entity. Steel has no
    /// `Brain`, so nothing drives a ram yet and this is only reachable from
    /// code that asks for it; the horn itself is faithful.
    pub fn drop_horn(&self) -> bool {
        if AgeableMob::is_baby(self) {
            return false;
        }

        let has_left = self.has_left_horn();
        let has_right = self.has_right_horn();
        let side = match (has_left, has_right) {
            (false, false) => return false,
            (false, true) => HornSide::Right,
            (true, false) => HornSide::Left,
            (true, true) => {
                if rand::random::<bool>() {
                    HornSide::Left
                } else {
                    HornSide::Right
                }
            }
        };

        self.set_horn(side, false);
        let Some(world) = self.level() else {
            return true;
        };
        let velocity = DVec3::new(
            random_between(-0.2, 0.2),
            random_between(0.3, 0.7),
            random_between(-0.2, 0.2),
        );
        world.spawn_item_with_velocity(self.position(), self.create_horn(), velocity);
        true
    }

    /// Returns whether the stack is vanilla goat food.
    #[must_use]
    pub fn is_food(item_stack: &ItemStack) -> bool {
        REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::GOAT_FOOD)
    }

    /// Vanilla parity: `Goat.checkGoatSpawnRules`.
    #[must_use]
    pub fn check_goat_spawn_rules(world: &Arc<World>, pos: BlockPos) -> bool {
        world
            .get_block_state(pos.below())
            .get_block()
            .has_tag(&BlockTag::GOATS_SPAWNABLE_ON)
            && <Self as Animal>::is_bright_enough_to_spawn(world.as_ref(), pos)
    }

    fn try_milk(&self, player: &Player, hand: InteractionHand) -> bool {
        if AgeableMob::is_baby(self) {
            return false;
        }

        let is_bucket = {
            let inventory = player.inventory.lock();
            inventory.get_item_in_hand(hand).is(&vanilla_items::BUCKET)
        };
        if !is_bucket {
            return false;
        }

        player.play_sound(self.milking_sound(), 1.0, 1.0);

        let overflow = {
            let mut inventory = player.inventory.lock();
            inventory.apply_filled_result(
                hand,
                ItemStack::new(&vanilla_items::MILK_BUCKET),
                player.has_infinite_materials(),
                true,
            )
        };

        if !overflow.is_empty() {
            let _ = player.drop_item(overflow, false, false);
        }

        true
    }
}

/// Reproduces `java.util.UUID.hashCode`, which vanilla uses as the horn seed.
const fn java_uuid_hash_code(uuid: uuid::Uuid) -> i32 {
    let (most, least) = uuid.as_u64_pair();
    let hilo = most ^ least;
    ((hilo >> 32) as i32) ^ (hilo as i32)
}

/// Vanilla `Mth.randomBetween`.
fn random_between(min: f32, max: f32) -> f64 {
    f64::from(rand::random::<f32>().mul_add(max - min, min))
}

impl Entity for GoatEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);
    }

    /// Vanilla parity: `Goat.getDefaultDimensions`. A goat mid-long-jump tucks
    /// in to seven tenths of its size.
    fn dimensions_for_pose(&self, pose: EntityPose) -> EntityDimensions {
        let scale = LivingEntity::get_scale(self);
        let base = if AgeableMob::is_baby(self) {
            BABY_DIMENSIONS.scale(scale)
        } else if self.entity_type.fixed {
            self.entity_type.dimensions
        } else {
            self.entity_type.dimensions.scale(scale)
        };

        if pose == EntityPose::LongJumping {
            base.scale(LONG_JUMPING_DIMENSION_SCALE_FACTOR)
        } else {
            base
        }
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {
        self.play_sound(&sound_events::ENTITY_GOAT_STEP, 0.15, 1.0);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
        nbt.insert("IsScreamingGoat", self.is_screaming_goat());
        nbt.insert("HasLeftHorn", self.has_left_horn());
        nbt.insert("HasRightHorn", self.has_right_horn());
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
        self.set_screaming_goat(nbt.byte("IsScreamingGoat").is_some_and(|flag| flag != 0));
        self.set_horn(
            HornSide::Left,
            nbt.byte("HasLeftHorn").is_none_or(|flag| flag != 0),
        );
        self.set_horn(
            HornSide::Right,
            nbt.byte("HasRightHorn").is_none_or(|flag| flag != 0),
        );
    }
}

impl LivingEntity for GoatEntity {
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

    /// Vanilla parity: `Goat.getAgeScale`.
    fn get_age_scale(&self) -> f32 {
        if AgeableMob::is_baby(self) {
            BABY_SCALE
        } else {
            1.0
        }
    }

    /// Vanilla parity: `Goat.calculateFallDamage`. Ten blocks of any fall are
    /// free, which is what lets a goat live on a mountainside.
    fn calculate_fall_damage(&self, fall_distance: f64, damage_modifier: f32) -> i32 {
        self.default_calculate_fall_damage(fall_distance, damage_modifier)
            - GOAT_FALL_DAMAGE_REDUCTION
    }

    /// Vanilla parity: `Goat.setYHeadRot`, which clamps the head to the body
    /// rather than letting the look control turn it freely.
    fn set_y_head_rot(&self, y_head_rot: f32) {
        let y_body_rot = self.y_body_rot();
        let delta =
            degrees_difference(y_body_rot, y_head_rot).clamp(-MAX_HEAD_Y_ROT, MAX_HEAD_Y_ROT);
        self.living_base().set_y_head_rot(y_body_rot + delta);
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(if self.is_screaming_goat() {
            &sound_events::ENTITY_GOAT_SCREAMING_HURT
        } else {
            &sound_events::ENTITY_GOAT_HURT
        })
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(if self.is_screaming_goat() {
            &sound_events::ENTITY_GOAT_SCREAMING_DEATH
        } else {
            &sound_events::ENTITY_GOAT_DEATH
        })
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn ai_step(&self) -> Option<MoveResult> {
        // VANILLA CLIENT-LOCAL: `Goat.aiStep` only runs down `lowerHeadTick`,
        // which the client sets from entity events 58 and 59 to tilt the head
        // during a ram. Nothing server-side reads it.
        let result = self.default_ai_step();
        AgeableMob::tick_ageable_mob(self);
        Animal::tick_animal_love(self);
        result
    }
}

impl AgeableMob for GoatEntity {
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

    /// Vanilla parity: `Goat.ageBoundaryReached`, which is where a kid's weaker
    /// butt comes from.
    ///
    /// Vanilla's own `ageBoundaryReached` only dismounts a grown goat from a
    /// boat it no longer fits in, and the hitbox is refreshed separately from
    /// `AgeableMob.onSyncedDataUpdated`. Steel folds that refresh into this
    /// hook, so the override still has to run it.
    fn age_boundary_changed(&self, baby: bool) {
        self.refresh_dimensions();
        let damage = if baby {
            BABY_ATTACK_DAMAGE
        } else {
            ADULT_ATTACK_DAMAGE
        };
        self.attributes()
            .lock()
            .set_base_value(vanilla_attributes::ATTACK_DAMAGE, damage);
    }
}

impl Animal for GoatEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        GoatEntity::is_food(item_stack)
    }

    /// Vanilla parity: `Goat.playEatingSound`.
    fn play_eating_sound(&self) {
        let Some(world) = self.level() else {
            return;
        };
        let sound = if self.is_screaming_goat() {
            &sound_events::ENTITY_GOAT_SCREAMING_EAT
        } else {
            &sound_events::ENTITY_GOAT_EAT
        };
        world.play_sound(
            sound,
            SoundSource::Neutral,
            self.block_position(),
            1.0,
            random_between(0.8, 1.2) as f32,
            None,
        );
    }

    /// Vanilla parity: `Goat.getBreedOffspring`. A kid screams if either parent
    /// does, and otherwise gets the same two percent chance a wild goat does.
    fn initialize_breed_offspring(&self, partner: &dyn Animal, offspring: &dyn Animal) {
        let Some(offspring) = offspring.downcast_ref::<Self>() else {
            log::error!("goat breeding produced a non-goat offspring");
            return;
        };

        let inherited_from = if rand::random::<bool>() {
            self.is_screaming_goat()
        } else {
            partner
                .downcast_ref::<Self>()
                .is_some_and(Self::is_screaming_goat)
        };
        let screaming = inherited_from || rand::random::<f64>() < GOAT_SCREAMING_CHANCE;
        offspring.set_screaming_goat(screaming);
    }
}

impl Mob for GoatEntity {
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

    /// Vanilla parity: `Goat.customServerAiStep` ticks the goat's `Brain` and
    /// then `GoatAi.updateActivity`. Steel has no `Brain`, so only the shared
    /// `Animal` half runs; see the module comment.
    fn custom_server_ai_step(&self) {
        Animal::custom_server_ai_step_animal(self);
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(if self.is_screaming_goat() {
            &sound_events::ENTITY_GOAT_SCREAMING_AMBIENT
        } else {
            &sound_events::ENTITY_GOAT_AMBIENT
        })
    }

    /// Vanilla parity: `Goat.getMaxHeadYRot`.
    fn max_head_y_rot(&self) -> f32 {
        MAX_HEAD_Y_ROT
    }

    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        let _ = spawn_reason;
        Self::check_goat_spawn_rules(world, pos)
    }

    /// Vanilla parity: `Goat.finalizeSpawn`. One goat in fifty screams and one
    /// adult in ten is born already missing a horn.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        // Vanilla also calls `GoatAi.initMemories`, which seeds the brain's
        // long-jump and ram cooldowns. There is no brain to seed.
        self.set_screaming_goat(rand::random::<f64>() < GOAT_SCREAMING_CHANCE);
        self.age_boundary_changed(AgeableMob::is_baby(self));

        if !AgeableMob::is_baby(self) && rand::random::<f32>() < UNIHORN_CHANCE {
            let side = if rand::random::<bool>() {
                HornSide::Left
            } else {
                HornSide::Right
            };
            self.set_horn(side, false);
        }

        self.finalize_spawn_ageable_mob(world, spawn_reason, group_data)
    }

    /// Vanilla parity: `Goat.mobInteract`. A bucket takes milk; anything else
    /// falls through to feeding, and a fed goat bleats.
    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        if self.try_milk(player, hand) {
            return InteractionResult::Success;
        }

        let interaction_result = Animal::mob_interact_animal(self, player, hand);
        // Vanilla holds a live reference to the held stack, so feeding the last
        // item leaves it empty and this second sound does not play.
        let still_holding_food = {
            let inventory = player.inventory.lock();
            Self::is_food(inventory.get_item_in_hand(hand))
        };
        if interaction_result.consumes_action() && still_holding_food {
            self.play_eating_sound();
        }

        interaction_result
    }
}

impl PathfinderMob for GoatEntity {}

/// Vanilla `Mth.degreesDifference`.
fn degrees_difference(from: f32, to: f32) -> f32 {
    wrap_degrees(to - from)
}

fn wrap_degrees(mut degrees: f32) -> f32 {
    degrees %= 360.0;
    if degrees >= 180.0 {
        degrees -= 360.0;
    }
    if degrees < -180.0 {
        degrees += 360.0;
    }
    degrees
}

#[cfg(test)]
mod tests;
