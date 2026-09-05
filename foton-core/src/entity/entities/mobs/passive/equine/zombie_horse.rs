//! The vanilla zombie horse.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.equine.ZombieHorse`. An
//! undead horse: it never falls in love, never grows up, and rolls its speed and
//! jump from its own narrower tables.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_registry::entity_type::{
    EntityAttachmentPoint, EntityAttachments, EntityDimensions, EntityTypeRef,
};
use foton_registry::item_stack::ItemStack;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_entity_data::ZombieHorseEntityData;
use foton_registry::vanilla_item_tags::ItemTag;
use foton_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_attributes, vanilla_entities,
};
use foton_utils::locks::SyncMutex;
use foton_utils::types::InteractionHand;
use foton_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::behavior::InteractionResult;
use crate::entity::ai::goal::{FloatGoal, TemptGoal};
use crate::entity::damage::DamageSource;
use crate::entity::entities::mobs::passive::equine::{
    add_abstract_horse_goals, sync_mob_effect_entity_data,
};
use crate::entity::spawn_rules::check_monster_spawn_rules;
use crate::entity::{
    AbstractHorse, AbstractHorseBase, AgeableMob, AgeableMobBase, Animal, AnimalBase, BABY_SCALE,
    ENTITIES, Entity, EntityBase, EntityBaseLoad, EntityPose, EntitySpawnReason, EntitySyncedData,
    LivingEntity, LivingEntityBase, LivingEntitySyncedData, Mob, MobBase, MoveResult,
    PathfinderMob, SharedEntity, SpawnGroupData, next_entity_id,
};
use crate::inventory::equipment::EquipmentSlot;
use crate::player::Player;
use crate::world::World;

/// Where an undead foal carries its rider.
///
/// Vanilla parity: `ZombieHorse.BABY_DIMENSIONS`, attached at
/// `ZOMBIE_HORSE.getHeight() - 0.25F` and scaled by `0.7F`.
const ZOMBIE_HORSE_BABY_PASSENGER_ATTACHMENTS: [EntityAttachmentPoint; 1] =
    [EntityAttachmentPoint::new(
        0.0,
        (1.6 - 0.25) * BABY_SCALE as f64,
        0.0,
    )];

/// An undead foal's hitbox.
const ZOMBIE_HORSE_BABY_DIMENSIONS: EntityDimensions = EntityDimensions::new_with_attachments(
    1.396_484_4 * BABY_SCALE,
    1.6 * BABY_SCALE,
    1.52 * BABY_SCALE,
    EntityAttachments::new(&ZOMBIE_HORSE_BABY_PASSENGER_ATTACHMENTS, &[], &[], &[]),
);

/// Divisor that turns the zombie horse's raw speed roll into an attribute.
///
/// Vanilla parity: `ZombieHorse.SPEED_FACTOR`.
const SPEED_FACTOR: f64 = 42.16;

/// Vanilla parity: `ZombieHorse.BASE_JUMP_STRENGTH`.
const BASE_JUMP_STRENGTH: f64 = 0.5;

/// Vanilla parity: `ZombieHorse.PER_RANDOM_JUMP_STRENGTH`.
const PER_RANDOM_JUMP_STRENGTH: f64 = 0.066_666_666_666_666_67;

/// Vanilla parity: `ZombieHorse.BASE_SPEED`.
const BASE_SPEED: f64 = 9.0;

/// Vanilla parity: `ZombieHorse.PER_RANDOM_SPEED`.
const PER_RANDOM_SPEED: f64 = 1.0;

/// Vanilla parity: `ZombieHorse.generateZombieHorseJumpStrength`.
fn generate_zombie_horse_jump_strength(probability: &mut dyn FnMut() -> f64) -> f64 {
    BASE_JUMP_STRENGTH
        + probability() * PER_RANDOM_JUMP_STRENGTH
        + probability() * PER_RANDOM_JUMP_STRENGTH
        + probability() * PER_RANDOM_JUMP_STRENGTH
}

/// Vanilla parity: `ZombieHorse.generateZombieHorseSpeed`.
fn generate_zombie_horse_speed(probability: &mut dyn FnMut() -> f64) -> f64 {
    (BASE_SPEED
        + probability() * PER_RANDOM_SPEED
        + probability() * PER_RANDOM_SPEED
        + probability() * PER_RANDOM_SPEED)
        / f64::from(SPEED_FACTOR as f32)
}

/// A vanilla zombie horse.
#[entity_behavior(class = "ZombieHorse")]
pub struct ZombieHorseEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    horse_base: AbstractHorseBase,
    entity_data: SyncMutex<ZombieHorseEntityData>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `ZombieHorseEntity`.
unsafe impl DowncastType for ZombieHorseEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/zombie_horse");
}

impl ZombieHorseEntity {
    /// Creates a new zombie horse.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a zombie horse from saved base data.
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
        let mut entity_data = ZombieHorseEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            let mut goals = mob_base.goal_selector().lock();
            add_abstract_horse_goals(&mut goals, true);
            // Vanilla parity: `ZombieHorse.addBehaviourGoals` keeps the float and
            // the tempt goal but drops the mount panic goal.
            goals.add_goal(0, FloatGoal::new(&mob_base));
            goals.add_goal(
                3,
                TemptGoal::new(
                    1.25,
                    |item_stack| {
                        REGISTRY
                            .items
                            .is_in_tag(item_stack.item(), &ItemTag::ZOMBIE_HORSE_FOOD)
                    },
                    false,
                ),
            );
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            horse_base: AbstractHorseBase::new(0),
            entity_data: SyncMutex::new(entity_data),
        }
    }
}

impl Entity for ZombieHorseEntity {
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
        self.tick_abstract_horse();
    }

    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let scale = LivingEntity::get_scale(self);
        if AgeableMob::is_baby(self) {
            ZOMBIE_HORSE_BABY_DIMENSIONS.scale(scale)
        } else if self.entity_type.fixed {
            self.entity_type.dimensions
        } else {
            self.entity_type.dimensions.scale(scale)
        }
    }

    fn controlling_passenger(&self) -> Option<SharedEntity> {
        if Mob::is_saddled(self)
            && let Some(passenger) = self.first_passenger()
            && passenger.as_player().is_some()
        {
            return Some(passenger);
        }
        self.controlling_passenger_mob()
    }

    fn passenger_attachment_point(&self, passenger: &dyn Entity) -> DVec3 {
        let scale = LivingEntity::get_scale(self);
        self.default_passenger_attachment_point(passenger)
            + self.abstract_horse_rearing_rider_offset(scale)
    }

    fn on_climbable(&self) -> bool {
        false
    }

    fn is_pushable(&self) -> bool {
        !self.is_vehicle()
    }

    fn can_jump_while_ridden(&self) -> bool {
        Mob::is_saddled(self)
    }

    fn handle_start_jump(&self, _jump_scale: i32) {
        self.handle_start_jump_abstract_horse();
    }

    fn open_custom_inventory_screen(&self, player: &Player) {
        if (self.is_vehicle() && !self.has_passenger(player)) || !self.is_tamed() {
            return;
        }
        self.open_horse_inventory_screen(player);
    }

    /// Vanilla parity: `ZombieHorse.interact`, which anchors the horse the moment
    /// a player touches it so it stops despawning.
    fn interact(
        &self,
        player: &Player,
        hand: InteractionHand,
        location: DVec3,
    ) -> InteractionResult {
        self.set_persistence_required();
        Mob::interact_mob(self, player, hand, location)
    }

    fn cause_fall_damage(
        &self,
        fall_distance: f64,
        damage_modifier: f32,
        source: &DamageSource,
    ) -> bool {
        self.abstract_horse_cause_fall_damage(fall_distance, damage_modifier, source)
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn update_data_before_sync(&self) {
        sync_mob_effect_entity_data(self, &self.entity_data);
    }

    fn play_step_sound(&self, pos: BlockPos, block_state: BlockStateId) {
        self.abstract_horse_play_step_sound(pos, block_state);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
        self.save_abstract_horse(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
        self.load_abstract_horse(nbt);
    }
}

impl LivingEntity for ZombieHorseEntity {
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
        0.8
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ZOMBIE_HORSE_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ZOMBIE_HORSE_DEATH)
    }

    fn hurt_server(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        let was_hurt = self.living_hurt_server(world, source, amount);
        self.abstract_horse_react_to_hurt(was_hurt)
    }

    /// Vanilla parity: `ZombieHorse.canUseSlot`.
    fn can_use_slot(&self, _slot: EquipmentSlot) -> bool {
        true
    }

    fn can_dispenser_equip_into_slot(&self, slot: EquipmentSlot) -> bool {
        self.abstract_horse_can_dispenser_equip_into_slot(slot)
    }

    fn equip_sound(&self, slot: EquipmentSlot, stack: &ItemStack) -> Option<SoundEventRef> {
        if slot == EquipmentSlot::Saddle {
            return Some(&sound_events::ENTITY_HORSE_SADDLE);
        }
        LivingEntity::default_equip_sound(self, slot, stack)
    }

    fn is_immobile(&self) -> bool {
        self.abstract_horse_is_immobile()
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn tick_ridden(&self, controller: &Player, ridden_input: DVec3) {
        self.tick_ridden_abstract_horse(controller, ridden_input);
    }

    fn ridden_input(&self, controller: &Player, _self_input: DVec3) -> DVec3 {
        self.abstract_horse_ridden_input(controller)
    }

    fn ridden_speed(&self, _controller: &Player) -> f32 {
        self.abstract_horse_ridden_speed()
    }

    fn drop_custom_death_loot(&self, _source: &DamageSource, _killed_by_player: bool) {
        self.drop_abstract_horse_inventory();
    }

    fn ai_step(&self) -> Option<MoveResult> {
        self.ai_step_abstract_horse();
        let result = self.default_ai_step();
        AgeableMob::tick_ageable_mob(self);
        Animal::tick_animal_love(self);
        self.server_ai_step_abstract_horse();
        result
    }
}

impl AgeableMob for ZombieHorseEntity {
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

    /// Vanilla parity: `ZombieHorse.canAgeUp`.
    fn can_age_up(&self) -> bool {
        false
    }
}

impl Animal for ZombieHorseEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    /// Vanilla parity: `ZombieHorse.isFood`, its own tag rather than the horse one.
    fn is_food(&self, item_stack: &ItemStack) -> bool {
        REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::ZOMBIE_HORSE_FOOD)
    }

    /// Vanilla parity: `ZombieHorse.canFallInLove`.
    fn can_fall_in_love(&self) -> bool {
        false
    }

    /// Vanilla parity: `ZombieHorse.getBreedOffspring`.
    fn get_breed_offspring(
        &self,
        world: &Arc<World>,
        _partner: &dyn Animal,
    ) -> Option<SharedEntity> {
        ENTITIES.create(
            &vanilla_entities::ZOMBIE_HORSE,
            next_entity_id(),
            self.position(),
            Arc::downgrade(world),
        )
    }
}

impl AbstractHorse for ZombieHorseEntity {
    fn abstract_horse_base(&self) -> &AbstractHorseBase {
        &self.horse_base
    }

    fn horse_flags(&self) -> i8 {
        *self.entity_data.lock().abstract_horse().id_flags.get()
    }

    fn set_horse_flags(&self, flags: i8) {
        self.entity_data
            .lock()
            .abstract_horse_mut()
            .id_flags
            .set(flags);
    }

    /// Vanilla parity: `ZombieHorse.isMobControlled`, which is what keeps the
    /// mount panic goal and the bucking goal quiet under a zombie jockey.
    fn is_mob_controlled(&self) -> bool {
        self.first_passenger()
            .is_some_and(|passenger| passenger.is_mob())
    }

    fn eating_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ZOMBIE_HORSE_EAT)
    }

    fn angry_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ZOMBIE_HORSE_ANGRY)
    }

    /// Vanilla parity: `ZombieHorse.randomizeAttributes`, its own two tables.
    fn randomize_attributes(&self) {
        let mut attributes = self.attributes().lock();
        attributes.set_base_value(
            vanilla_attributes::JUMP_STRENGTH,
            generate_zombie_horse_jump_strength(&mut || rand::random::<f64>()),
        );
        attributes.set_base_value(
            vanilla_attributes::MOVEMENT_SPEED,
            generate_zombie_horse_speed(&mut || rand::random::<f64>()),
        );
    }
}

impl Mob for ZombieHorseEntity {
    /// Vanilla parity: `AbstractHorse.getMaxSpawnClusterSize`.
    fn max_spawn_cluster_size(&self) -> i32 {
        6
    }

    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    /// Vanilla parity: `AbstractHorse.supportQuadLeash`.
    fn support_quad_leash(&self) -> bool {
        true
    }

    /// Vanilla parity: `SpawnPlacements` registers the zombie horse with
    /// `Monster::checkMonsterSpawnRules`, not the animal rule its horse cousins
    /// use -- it is a `MONSTER` and wants the dark.
    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        check_monster_spawn_rules(world, spawn_reason, pos)
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

    fn ambient_sound_interval(&self) -> i32 {
        400
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ZOMBIE_HORSE_AMBIENT)
    }

    /// Vanilla parity: `ZombieHorse.removeWhenFarAway`, which is always true.
    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        true
    }

    /// Vanilla parity: `ZombieHorse.canBeLeashed`.
    fn can_be_leashed(&self) -> bool {
        self.is_tamed() || !self.is_mob_controlled()
    }

    /// Vanilla parity: `ZombieHorse.finalizeSpawn`.
    ///
    /// MISSING FOUNDATION: vanilla also seats a spear-carrying zombie on a
    /// naturally spawned zombie horse. `Entity.startRiding` in Foton resolves both
    /// entities through the world, and `finalizeSpawn` runs before the mob joins
    /// it, so the jockey cannot be mounted here. Zombie horses have no natural
    /// spawn entry in vanilla, so the branch is unreachable in normal play.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        self.finalize_spawn_abstract_horse(world, spawn_reason, group_data)
    }

    /// Vanilla parity: `ZombieHorse.mobInteract`, which has no foal bypass item.
    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        if self.skips_feeding_interact(player, None) {
            return self.abstract_horse_mob_interact(player, hand);
        }
        if let Some(result) = self.try_feed_or_anger(player, hand) {
            return result;
        }
        self.abstract_horse_mob_interact(player, hand)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for ZombieHorseEntity {}
