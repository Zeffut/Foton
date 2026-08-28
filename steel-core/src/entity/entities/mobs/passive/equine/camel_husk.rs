//! The camel husk.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.camel.CamelHusk`, which
//! extends `Camel` and adds no state at all: it swaps every sound, eats a
//! different tag, will not breed and will not be a baby, and it despawns like a
//! monster rather than sticking around like an animal. Everything else is
//! [`super::camel_common`].
//!
//! One vanilla override has no reader in Steel yet and is deliberately absent:
//! `chargeSpeedModifier`, which returns `4.0F` so a rider charging with a spear
//! is carried four times as fast. Steel has no `SpearUseGoal`, `SpearAttack`,
//! `SpearApproach` or `SpearRetreat`, so nothing would ever read the value; it
//! arrives with the spear behaviors that want it.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_registry::entity_type::{EntityDimensions, EntityTypeRef};
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::CamelHuskEntityData;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{REGISTRY, TaggedRegistryExt as _, sound_events};
use steel_utils::locks::SyncMutex;
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};

use super::camel_ai;
use super::camel_common::{
    BABY_SCALE, CamelBase, CamelLike, MAX_HEAD_Y_ROT, check_camel_spawn_rules, hooks_for,
};
use crate::behavior::InteractionResult;
use crate::entity::ai::brain::Brain;
use crate::entity::damage::DamageSource;
use crate::entity::{
    AbstractHorse, AbstractHorseBase, AgeableMob, AgeableMobBase, Animal, AnimalBase, Entity,
    EntityBase, EntityBaseLoad, EntityPose, EntitySpawnReason, EntitySyncedData, LivingEntity,
    LivingEntityBase, LivingEntitySyncedData, Mob, MobBase, MoveResult, PathfinderMob,
    SharedEntity, SpawnGroupData,
};
use crate::player::Player;
use crate::world::World;

/// A camel husk.
#[entity_behavior(class = "CamelHusk")]
pub struct CamelHuskEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    abstract_horse_base: AbstractHorseBase,
    camel_base: CamelBase,
    brain: Brain,
    entity_data: SyncMutex<CamelHuskEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `CamelHuskEntity`.
unsafe impl DowncastType for CamelHuskEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/camel_husk");
}

impl CamelHuskEntity {
    /// Creates a camel husk at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a camel husk from saved base data.
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
            // Vanilla parity: the two navigation calls the `Camel` constructor
            // makes, which `CamelHusk` inherits.
            let mut navigation = mob_base.navigation().lock();
            navigation.set_can_float(true);
            navigation.set_can_walk_over_fences(true);
        }
        let mut entity_data = CamelHuskEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            abstract_horse_base: AbstractHorseBase::new(0),
            camel_base: CamelBase::new(),
            brain: camel_ai::make_brain(hooks_for::<Self>()),
            entity_data: SyncMutex::new(entity_data),
        }
    }
}

impl Entity for CamelHuskEntity {
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

    fn tick(&self) {
        self.camel_tick();
    }

    fn dimensions_for_pose(&self, pose: EntityPose) -> EntityDimensions {
        self.camel_dimensions_for_pose(pose)
    }

    /// Vanilla parity: `CamelHusk.playStepSound`, the same tag test with the
    /// husk's own two sounds.
    fn play_step_sound(&self, _pos: BlockPos, block_state: BlockStateId) {
        self.camel_play_step_sound(block_state);
    }

    fn can_add_passenger(&self, _passenger: &dyn Entity) -> bool {
        self.camel_can_add_passenger()
    }

    fn handle_start_jump(&self, _jump_scale: i32) {
        self.camel_handle_start_jump();
    }

    fn can_jump_while_ridden(&self) -> bool {
        self.camel_can_jump_while_ridden()
    }

    fn open_custom_inventory_screen(&self, player: &Player) {
        self.open_horse_inventory_screen(player);
    }

    /// Vanilla parity: `CamelHusk.interact`, which pins the husk in place the
    /// moment a player touches it -- a husk a player has handled is theirs and
    /// stops despawning.
    fn interact(
        &self,
        player: &Player,
        hand: InteractionHand,
        location: DVec3,
    ) -> InteractionResult {
        self.set_persistence_required();
        self.interact_mob(player, hand, location)
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_camel(nbt);
        self.brain.save(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_camel(nbt);
        self.brain.load(nbt);
    }
}

impl LivingEntity for CamelHuskEntity {
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

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(self.camel_hurt_sound())
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(self.camel_death_sound())
    }

    fn get_age_scale(&self) -> f32 {
        if AgeableMob::is_baby(self) {
            BABY_SCALE
        } else {
            1.0
        }
    }

    fn actually_hurt(&self, world: &World, source: &DamageSource, amount: f32) {
        self.camel_actually_hurt(world, source, amount);
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn travel(&self, input: DVec3) -> Option<MoveResult> {
        self.camel_travel(input)
    }

    fn tick_ridden(&self, controller: &Player, ridden_input: DVec3) {
        self.camel_tick_ridden(controller, ridden_input);
    }

    fn ridden_input(&self, controller: &Player, _self_input: DVec3) -> DVec3 {
        self.camel_ridden_input(controller)
    }

    fn ridden_speed(&self, controller: &Player) -> f32 {
        self.camel_ridden_speed(controller)
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

impl AgeableMob for CamelHuskEntity {
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

    /// Vanilla parity: `CamelHusk.canBeABaby`, which is `false`. There are no
    /// young camel husks; they are made, not born.
    fn can_be_a_baby(&self) -> bool {
        false
    }
}

impl Animal for CamelHuskEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        self.is_camel_food(item_stack)
    }

    /// Vanilla parity: `CamelHusk.canMate`, a flat `false`.
    fn can_mate(&self, _partner: &dyn Animal) -> bool {
        false
    }

    /// Vanilla parity: `CamelHusk.canFallInLove`, a flat `false`. Feeding one
    /// still heals it; it simply never breeds.
    fn can_fall_in_love(&self) -> bool {
        false
    }

    /// Vanilla parity: `CamelHusk.getBreedOffspring`, which returns null.
    fn get_breed_offspring(
        &self,
        _world: &Arc<World>,
        _partner: &dyn Animal,
    ) -> Option<SharedEntity> {
        None
    }
}

impl AbstractHorse for CamelHuskEntity {
    fn abstract_horse_base(&self) -> &AbstractHorseBase {
        &self.abstract_horse_base
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

    fn on_player_jump(&self, jump_amount: i32) {
        self.camel_on_player_jump(jump_amount);
    }

    fn execute_riders_jump(&self, amount: f32, _input: DVec3) {
        self.execute_camel_dash(amount);
    }

    fn is_tamed(&self) -> bool {
        true
    }

    fn can_perform_rearing(&self) -> bool {
        false
    }

    fn handle_eating(&self, player: &Player, item_stack: &ItemStack) -> bool {
        self.camel_handle_eating(player, item_stack)
    }

    /// Vanilla parity: `CamelHusk.isMobControlled`, which counts *any* mob in
    /// the saddle rather than only one steering it -- a parched riding a husk
    /// is what the desert patrol is.
    fn is_mob_controlled(&self) -> bool {
        self.first_passenger()
            .is_some_and(|passenger| passenger.is_mob())
    }
}

impl Mob for CamelHuskEntity {
    /// Vanilla parity: `Mob.serverAiStep` ticks the goal selector for every
    /// mob it runs, brain-driven or not. `Mob::tick_goal_selectors` has an
    /// empty default, so leaving it out is how a registered goal set never
    /// runs.
    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    /// Vanilla parity: `AbstractHorse.supportQuadLeash`.
    fn support_quad_leash(&self) -> bool {
        true
    }

    fn brain(&self) -> Option<&Brain> {
        Some(&self.brain)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    fn custom_server_ai_step(&self) {
        let Some(world) = self.level() else {
            return;
        };
        self.brain.tick(&world, self);
        camel_ai::update_activity(&self.brain);
        Animal::custom_server_ai_step_animal(self);
    }

    fn tick_move_control(&self) {
        self.camel_tick_move_control();
    }

    fn tick_look_control(&self) {
        self.camel_tick_look_control();
    }

    fn max_head_y_rot(&self) -> f32 {
        MAX_HEAD_Y_ROT
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(self.camel_ambient_sound())
    }

    /// Vanilla parity: `CamelHusk.removeWhenFarAway`, a flat `true` where an
    /// `Animal` answers `false`. A husk despawns like a monster.
    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        true
    }

    /// Vanilla parity: `CamelHusk.canBeLeashed`, which refuses while a mob is
    /// in the saddle.
    fn can_be_leashed(&self) -> bool {
        !AbstractHorse::is_mob_controlled(self)
    }

    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        let _ = spawn_reason;
        check_camel_spawn_rules::<Self>(world, pos)
    }

    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        self.reset_last_pose_change_tick_to_full_stand(world.game_time());
        self.finalize_spawn_ageable_mob(world, spawn_reason, group_data)
    }

    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        self.camel_mob_interact(player, hand)
    }
}

impl PathfinderMob for CamelHuskEntity {}

impl CamelLike for CamelHuskEntity {
    fn camel_base(&self) -> &CamelBase {
        &self.camel_base
    }

    fn dash_flag(&self) -> bool {
        *self.entity_data.lock().camel().dash.get()
    }

    fn store_dash_flag(&self, dashing: bool) {
        self.entity_data.lock().camel_mut().dash.set(dashing);
    }

    fn stored_last_pose_change_tick(&self) -> i64 {
        *self.entity_data.lock().camel().last_pose_change_tick.get()
    }

    fn store_last_pose_change_tick(&self, tick: i64) {
        self.entity_data
            .lock()
            .camel_mut()
            .last_pose_change_tick
            .set(tick);
    }

    fn camel_ambient_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_CAMEL_HUSK_AMBIENT
    }

    fn camel_death_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_CAMEL_HUSK_DEATH
    }

    fn camel_hurt_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_CAMEL_HUSK_HURT
    }

    fn camel_step_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_CAMEL_HUSK_STEP
    }

    fn camel_sand_step_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_CAMEL_HUSK_STEP_SAND
    }

    fn camel_dashing_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_CAMEL_HUSK_DASH
    }

    fn camel_dash_ready_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_CAMEL_HUSK_DASH_READY
    }

    fn camel_eating_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_CAMEL_HUSK_EAT
    }

    fn camel_stand_up_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_CAMEL_HUSK_STAND
    }

    fn camel_sit_down_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_CAMEL_HUSK_SIT
    }

    /// Vanilla parity: `CamelHusk.isFood`, which reads `#camel_husk_food`
    /// rather than `#camel_food`.
    fn is_camel_food(&self, item_stack: &ItemStack) -> bool {
        REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::CAMEL_HUSK_FOOD)
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::{init_vanilla_registry, vanilla_entities, vanilla_items};
    use steel_utils::{ChunkPos, Downcast as _};

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::entity::entities::{CamelEntity, PigEntity};
    use crate::entity::{next_entity_id, start_riding_entities};
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    const SPAWN: DVec3 = DVec3::new(8.5, 64.0, 8.5);

    fn detached_husk() -> CamelHuskEntity {
        init_vanilla_registry();
        CamelHuskEntity::new(
            &vanilla_entities::CAMEL_HUSK,
            next_entity_id(),
            SPAWN,
            Weak::new(),
        )
    }

    /// A game time far enough from zero that a sitting husk reads as sitting.
    ///
    /// Vanilla stores the pose as the negated tick the change started on, so at
    /// game time zero `-0` is indistinguishable from standing.
    const TEST_GAME_TIME: i64 = 1000;

    fn husk_world(key: &'static str) -> Arc<World> {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world(key);
        world.level_data.write().set_game_time(TEST_GAME_TIME);
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        world
    }

    fn spawn_husk(world: &Arc<World>) -> SharedEntity {
        let husk: SharedEntity = Arc::new(CamelHuskEntity::new(
            &vanilla_entities::CAMEL_HUSK,
            next_entity_id(),
            SPAWN,
            Arc::downgrade(world),
        ));
        world
            .try_add_entity(Arc::clone(&husk))
            .expect("the test chunk is loaded");
        husk
    }

    /// The husk and the camel eat from different tags. Sharing one base is only
    /// safe if the hook the base reads is the subclass's, so this is the check
    /// that the override is reached rather than shadowed by the camel's.
    #[test]
    fn a_camel_husk_eats_from_its_own_tag_and_a_camel_from_the_camels() {
        let husk = detached_husk();
        let camel = CamelEntity::new(
            &vanilla_entities::CAMEL,
            next_entity_id(),
            SPAWN,
            Weak::new(),
        );

        let cactus = ItemStack::new(&vanilla_items::CACTUS);
        let rabbit_foot = ItemStack::new(&vanilla_items::RABBIT_FOOT);

        assert!(
            Animal::is_food(&camel, &cactus),
            "`#camel_food` is the cactus"
        );
        assert!(
            !Animal::is_food(&camel, &rabbit_foot),
            "and a camel wants nothing to do with a rabbit's foot"
        );
        assert!(
            Animal::is_food(&husk, &rabbit_foot),
            "`#camel_husk_food` is the rabbit's foot"
        );
        assert!(
            !Animal::is_food(&husk, &cactus),
            "a husk does not eat what a camel eats"
        );
    }

    /// Vanilla's `CamelHusk.canBeABaby` is `false`, and `AgeableMob.setBaby` is
    /// gated on it. A husk that could be set to a baby would ask a client for a
    /// model that does not exist.
    #[test]
    fn a_camel_husk_refuses_to_be_a_baby() {
        let husk = detached_husk();

        Mob::set_baby(&husk, true);

        assert!(!AgeableMob::is_baby(&husk));
        assert_eq!(husk.get_age(), 0, "its age was never touched");
    }

    /// Vanilla's `CamelHusk.removeWhenFarAway` is `true` where `Animal` answers
    /// `false`: a husk is desert scenery that cleans itself up, not livestock.
    #[test]
    fn a_camel_husk_despawns_far_from_a_player_where_a_camel_stays() {
        let husk = detached_husk();
        let camel = CamelEntity::new(
            &vanilla_entities::CAMEL,
            next_entity_id(),
            SPAWN,
            Weak::new(),
        );

        assert!(Mob::remove_when_far_away(&husk, 16_384.0));
        assert!(!Mob::remove_when_far_away(&camel, 16_384.0));
    }

    /// Vanilla's `CamelHusk.canBeLeashed` refuses while a mob is in the saddle,
    /// which is what stops a player leading away the mount another mob is on.
    #[test]
    fn a_camel_husk_a_mob_is_riding_cannot_be_leashed() {
        let world = husk_world("camel_husk_mob_controlled");
        let husk = spawn_husk(&world);
        let husk_mob = husk.as_mob().expect("a camel husk is a mob");

        assert!(
            husk_mob.can_be_leashed(),
            "an unridden husk takes a lead like any other mount"
        );

        let rider: SharedEntity = Arc::new(PigEntity::new(
            &vanilla_entities::PIG,
            next_entity_id(),
            SPAWN,
            Arc::downgrade(&world),
        ));
        world
            .try_add_entity(Arc::clone(&rider))
            .expect("the test chunk is loaded");
        assert!(start_riding_entities(&rider, &husk));

        assert!(
            !husk_mob.can_be_leashed(),
            "a husk with a mob in the saddle refuses the lead"
        );
    }

    /// The shared base has to still work for the husk: the pose clock, the sit
    /// and the stand are the camel's and the husk inherits every one of them.
    #[test]
    fn a_camel_husk_sits_and_stands_on_the_shared_camel_clock() {
        let world = husk_world("camel_husk_pose");
        let husk = spawn_husk(&world);
        let husk = husk
            .downcast_ref::<CamelHuskEntity>()
            .expect("a camel husk");

        assert!(!husk.is_camel_sitting());

        husk.sit_down();
        assert!(husk.is_camel_sitting());
        assert!(
            husk.refuse_to_move(),
            "a sitting camel husk will not walk anywhere"
        );

        husk.stand_up_instantly();
        assert!(!husk.is_camel_sitting());
        assert!(!husk.refuse_to_move());
    }
}
