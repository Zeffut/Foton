//! Sniffer entity.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.sniffer.Sniffer`. A
//! sniffer is a walking loot table with a six-step ritual in front of it: it
//! scents, sniffs, picks a patch of ground it has not dug before, walks there,
//! searches, digs, and leaves an ancient seed behind. Breeding a pair gives a
//! sniffer egg rather than a calf, and that egg is what hatches the next one --
//! the loop this closes.

mod sniffer_ai;

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_registry::entity_data::SnifferState;
use steel_registry::entity_type::{EntityDimensions, EntityTypeRef};
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_entity_data::SnifferEntityData;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_entities, vanilla_game_rules,
    vanilla_items, vanilla_loot_tables,
};
use steel_utils::locks::SyncMutex;
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, GlobalPos};

use steel_registry::blocks::block_state_ext::BlockStateExt as _;

use crate::behavior::InteractionResult;
use crate::entity::ai::brain::Brain;
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::ai::goal::land_random_pos;
use crate::entity::ai::path::PathType;
use crate::entity::damage::DamageSource;
use crate::entity::living_entity::gift_loot_items_with_rng;
use crate::entity::mob::NavigationKind;
use crate::entity::{
    AgeableMob, AgeableMobBase, Animal, AnimalBase, ENTITIES, Entity, EntityBase, EntityBaseLoad,
    EntityPose, EntitySyncedData, LivingEntity, LivingEntityBase, LivingEntitySyncedData, Mob,
    MobBase, MoveResult, PathfinderMob, next_entity_id,
};
use crate::player::Player;
use crate::world::{LevelReader as _, World};

/// Vanilla parity: `Sniffer.DIGGING_DROP_SEED_OFFSET_TICKS`.
const DIGGING_DROP_SEED_OFFSET_TICKS: i32 = 120;
/// Vanilla parity: `Sniffer.SNIFFER_BABY_START_AGE`.
const SNIFFER_BABY_START_AGE: i32 = -48_000;
/// Vanilla parity: `Sniffer.DIGGING_BB_HEIGHT_OFFSET`.
const DIGGING_BB_HEIGHT_OFFSET: f32 = 0.4;
/// Vanilla parity: the `withEyeHeight(0.81F)` of `Sniffer.DIGGING_DIMENSIONS`.
const DIGGING_EYE_HEIGHT: f32 = 0.81;
/// Vanilla parity: the `2.25` of `Sniffer.getHeadPosition`.
const HEAD_REACH: f64 = 2.25;
/// Vanilla parity: the `0.2F` of `Sniffer.getHeadBlock`.
const HEAD_BLOCK_Y_OFFSET: f64 = 0.2;
/// Vanilla parity: `Sniffer.getMaxHeadYRot`.
const MAX_HEAD_Y_ROT: f32 = 50.0;
/// Vanilla parity: the `0.15F` volume of `Sniffer.playStepSound`.
const STEP_SOUND_VOLUME: f32 = 0.15;
/// How many dig positions vanilla tries, and how much wider each try looks.
///
/// Vanilla parity: the `IntStream.range(0, 5)` and `10 + 2 * idx` of
/// `Sniffer.calculateDigPosition`.
const DIG_POSITION_TRIES: i32 = 5;
const DIG_POSITION_BASE_RANGE: i32 = 10;
const DIG_POSITION_RANGE_STEP: i32 = 2;
const DIG_POSITION_VERTICAL_RANGE: i32 = 3;
/// Vanilla parity: the `limit(20L)` of `Sniffer.storeExploredPosition`.
const MAX_EXPLORED_POSITIONS: usize = 20;
/// Vanilla parity: the `0.1F` nudge of `Sniffer.jumpFromGround`.
const JUMP_FORWARD_NUDGE: f32 = 0.1;
/// Vanilla parity: the `0.01` below which that nudge applies.
const JUMP_FORWARD_THRESHOLD: f64 = 0.01;

/// A sniffer.
#[entity_behavior(class = "Sniffer")]
pub struct SnifferEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    brain: Brain,
    entity_data: SyncMutex<SnifferEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `SnifferEntity`.
unsafe impl DowncastType for SnifferEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/sniffer");
}

impl SnifferEntity {
    /// Creates a sniffer at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a sniffer from saved base data.
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
            // Vanilla parity: the three `setPathfindingMalus` calls of the
            // `Sniffer` constructor, plus its `setCanFloat(true)`.
            let mut malus = mob_base.pathfinding_malus().lock();
            malus.set(PathType::Water, -1.0);
            malus.set(PathType::OnTopOfPowderSnow, -1.0);
            malus.set(PathType::DamageCautious, -1.0);
        }
        mob_base.navigation().lock().set_can_float(true);

        let mut entity_data = SnifferEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            brain: sniffer_ai::make_brain(),
            entity_data: SyncMutex::new(entity_data),
        }
    }

    /// Returns vanilla `Sniffer.getState`.
    #[must_use]
    pub fn state(&self) -> SnifferState {
        *self.entity_data.lock().state.get()
    }

    fn set_state(&self, state: SnifferState) {
        self.entity_data.lock().state.set(state);
        self.refresh_dimensions();
    }

    /// Moves the sniffer to `state`, with whatever sound that step makes.
    ///
    /// Vanilla parity: `Sniffer.transitionTo`.
    pub fn transition_to(&self, state: SnifferState) {
        match state {
            SnifferState::Idling | SnifferState::Searching => {}
            SnifferState::FeelingHappy => {
                self.play_sound(&sound_events::ENTITY_SNIFFER_HAPPY, 1.0, 1.0);
            }
            SnifferState::Scenting => {
                let pitch = if AgeableMob::is_baby(self) { 1.3 } else { 1.0 };
                self.play_sound(&sound_events::ENTITY_SNIFFER_SCENTING, 1.0, pitch);
            }
            SnifferState::Sniffing => {
                self.play_sound(&sound_events::ENTITY_SNIFFER_SNIFFING, 1.0, 1.0);
            }
            SnifferState::Digging => {
                self.entity_data
                    .lock()
                    .drop_seed_at_tick
                    .set(self.tick_count() + DIGGING_DROP_SEED_OFFSET_TICKS);
            }
            SnifferState::Rising => {
                self.play_sound(&sound_events::ENTITY_SNIFFER_DIGGING_STOP, 1.0, 1.0);
            }
        }
        self.set_state(state);
    }

    /// Returns vanilla `Sniffer.isSearching`.
    #[must_use]
    pub fn is_searching(&self) -> bool {
        self.state() == SnifferState::Searching
    }

    /// Returns vanilla `Sniffer.isTempted`.
    #[must_use]
    pub fn is_tempted(&self) -> bool {
        self.brain
            .get_memory(memory_module_types::IS_TEMPTED)
            .unwrap_or(false)
    }

    /// Returns vanilla `Sniffer.canSniff`.
    ///
    /// Everything that would interrupt the ritual is here: a sniffer being led
    /// by food, panicking, swimming, courting, riding or leashed does not sniff.
    #[must_use]
    pub fn can_sniff(&self) -> bool {
        !self.is_tempted()
            && !self.is_panicking()
            && !self.is_in_water()
            && !self.is_in_love()
            && self.on_ground()
            && !self.is_passenger()
            && !self.is_leashed()
    }

    /// Returns vanilla `Sniffer.canDig()`.
    #[must_use]
    pub fn can_dig(&self) -> bool {
        !self.is_panicking()
            && !self.is_tempted()
            && !AgeableMob::is_baby(self)
            && !self.is_in_water()
            && self.on_ground()
            && !self.is_passenger()
            && self.can_dig_at(self.head_block().below())
    }

    /// Vanilla parity: the private `Sniffer.canDig(BlockPos)`.
    fn can_dig_at(&self, position: BlockPos) -> bool {
        let Some(world) = self.level() else {
            return false;
        };
        if !world
            .get_block_state(position)
            .get_block()
            .has_tag(&BlockTag::SNIFFER_DIGGABLE_BLOCK)
        {
            return false;
        }
        if self.has_explored(position) {
            return false;
        }
        self.create_path_to(position, 1)
            .is_some_and(|path| path.can_reach())
    }

    /// Vanilla parity: `Sniffer.getHeadBlock`.
    #[must_use]
    fn head_block(&self) -> BlockPos {
        let head = self.position() + self.look_angle() * HEAD_REACH;
        BlockPos::containing(head.x, self.position().y + HEAD_BLOCK_Y_OFFSET, head.z)
    }

    /// Vanilla parity: `Sniffer.calculateDigPosition`, which widens its search
    /// five times before giving up.
    #[must_use]
    pub fn calculate_dig_position(&self) -> Option<BlockPos> {
        (0..DIG_POSITION_TRIES).find_map(|index| {
            let range = DIG_POSITION_BASE_RANGE + DIG_POSITION_RANGE_STEP * index;
            let target = land_random_pos(self, range, DIG_POSITION_VERTICAL_RANGE)?;
            let position = BlockPos::containing(target.x, target.y, target.z).below();
            self.can_dig_at(position).then_some(position)
        })
    }

    /// Vanilla parity: `Sniffer.onDiggingComplete`, which only remembers a hole
    /// it actually finished.
    pub fn on_digging_complete(&self, success: bool) {
        if success {
            self.store_explored_position(self.block_position().below());
        }
    }

    /// Vanilla parity: `Sniffer.storeExploredPosition`, newest first and capped
    /// at twenty, so a sniffer eventually forgets its oldest holes.
    fn store_explored_position(&self, position: BlockPos) {
        let Some(world) = self.level() else {
            return;
        };
        let mut explored = self
            .brain
            .get_memory(memory_module_types::SNIFFER_EXPLORED_POSITIONS)
            .unwrap_or_default();
        explored.truncate(MAX_EXPLORED_POSITIONS);
        explored.insert(0, GlobalPos::new(world.key.clone(), position));
        self.brain
            .set_memory(memory_module_types::SNIFFER_EXPLORED_POSITIONS, explored);
    }

    /// Returns whether this sniffer has already dug here.
    fn has_explored(&self, position: BlockPos) -> bool {
        let Some(world) = self.level() else {
            return false;
        };
        let here = GlobalPos::new(world.key.clone(), position);
        self.brain
            .get_memory(memory_module_types::SNIFFER_EXPLORED_POSITIONS)
            .is_some_and(|explored| explored.contains(&here))
    }

    /// Vanilla parity: `Sniffer.dropSeed`, which fires on one exact tick of the
    /// dig -- two seconds in, while the sniffer's head is still down.
    fn drop_seed(&self) {
        let Some(world) = self.level() else {
            return;
        };
        if *self.entity_data.lock().drop_seed_at_tick.get() != self.tick_count() {
            return;
        }
        if !world.get_game_rule(&vanilla_game_rules::MOB_DROPS) {
            return;
        }

        let head = self.head_block();
        let mut rng = rand::rng();
        let drops = gift_loot_items_with_rng(
            self,
            &vanilla_loot_tables::GAMEPLAY_SNIFFER_DIGGING,
            &mut rng,
        );
        for drop in drops {
            // Vanilla drops the seed at the head block itself rather than at the
            // sniffer's feet, so it lands in the hole it just dug.
            let dropped = world.spawn_item(
                DVec3::new(
                    f64::from(head.x()),
                    f64::from(head.y()),
                    f64::from(head.z()),
                ),
                drop,
            );
            if let Some(item) = dropped {
                item.set_default_pickup_delay();
            }
        }
        self.play_sound(&sound_events::ENTITY_SNIFFER_DROP_SEED, 1.0, 1.0);
    }

    /// Returns whether the stack is vanilla sniffer food.
    #[must_use]
    pub fn is_sniffer_food(item_stack: &ItemStack) -> bool {
        REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::SNIFFER_FOOD)
    }

    /// Vanilla parity: `Sniffer.DIGGING_DIMENSIONS`.
    fn digging_dimensions(entity_type: EntityTypeRef) -> EntityDimensions {
        EntityDimensions::new(
            entity_type.dimensions.width,
            entity_type.dimensions.height - DIGGING_BB_HEIGHT_OFFSET,
            DIGGING_EYE_HEIGHT,
        )
    }
}

impl Entity for SnifferEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `Sniffer.getDefaultDimensions`, which crouches the hitbox
    /// while the sniffer's nose is in the ground.
    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let scale = LivingEntity::get_scale(self);
        if self.state() == SnifferState::Digging {
            return Self::digging_dimensions(self.entity_type).scale(self.get_age_scale() * scale);
        }
        if self.entity_type.fixed {
            self.entity_type.dimensions
        } else {
            self.entity_type.dimensions.scale(scale)
        }
    }

    /// Vanilla parity: `Sniffer.tick`, whose server half is the seed the dig
    /// drops. The digging particles and the searching sound are client-local.
    fn tick(&self) {
        if self.state() == SnifferState::Digging {
            self.drop_seed();
        }
        self.default_tick();
    }

    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {
        self.play_sound(&sound_events::ENTITY_SNIFFER_STEP, STEP_SOUND_VOLUME, 1.0);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
        self.brain.save(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
        self.brain.load(nbt);
    }
}

impl LivingEntity for SnifferEntity {
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
        Some(&sound_events::ENTITY_SNIFFER_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_SNIFFER_DEATH)
    }

    /// Vanilla parity: `Sniffer.die`, which stands the sniffer up first so it
    /// does not die halfway into the ground.
    fn die(&self, source: &DamageSource) {
        self.transition_to(SnifferState::Idling);
        self.living_die(source);
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn ai_step(&self) -> Option<MoveResult> {
        let result = self.default_ai_step();
        AgeableMob::tick_ageable_mob(self);
        Animal::tick_animal_love(self);
        result
    }

    /// Vanilla parity: `Sniffer.jumpFromGround`, which gives a sniffer that
    /// jumps on the spot a shove forward so it clears what it is standing on.
    fn jump_from_ground(&self) {
        self.default_jump_from_ground();

        let speed_modifier = self
            .mob_base()
            .controls()
            .lock()
            .move_control
            .speed_modifier();
        if speed_modifier <= 0.0 {
            return;
        }
        let velocity = self.velocity();
        if velocity.x.mul_add(velocity.x, velocity.z * velocity.z) >= JUMP_FORWARD_THRESHOLD {
            return;
        }
        self.move_relative(JUMP_FORWARD_NUDGE, DVec3::new(0.0, 0.0, 1.0));
    }
}

impl AgeableMob for SnifferEntity {
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

    /// Vanilla parity: `Sniffer.getBabyStartAge`, which is twice every other
    /// animal's -- a sniffer calf takes forty minutes to grow up.
    fn get_baby_start_age(&self) -> i32 {
        SNIFFER_BABY_START_AGE
    }

    fn age_boundary_changed(&self, _baby: bool) {
        self.refresh_dimensions();
    }
}

impl Animal for SnifferEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        Self::is_sniffer_food(item_stack)
    }

    /// Vanilla parity: `Sniffer.canMate`, which refuses a partner mid-ritual so
    /// a dig is never interrupted by courting.
    fn can_mate(&self, partner: &dyn Animal) -> bool {
        use steel_utils::Downcast as _;

        let Some(other) = partner.downcast_ref::<Self>() else {
            return false;
        };
        let calm = |state| {
            matches!(
                state,
                SnifferState::Idling | SnifferState::Scenting | SnifferState::FeelingHappy
            )
        };
        calm(self.state()) && calm(other.state()) && self.default_can_mate(partner)
    }

    /// Vanilla parity: `Sniffer.spawnChildFromBreeding`, which lays an egg on
    /// the ground rather than producing a calf. That egg is the near end of the
    /// sniffer loop.
    fn spawn_child_from_breeding(&self, world: &Arc<World>, partner: &dyn Animal) {
        let egg = ItemStack::new(&vanilla_items::SNIFFER_EGG);
        self.finalize_spawn_child_from_breeding(world, partner, None);
        let pitch = (rand::random::<f32>() - rand::random::<f32>()).mul_add(0.2, 0.5);
        self.play_sound(&sound_events::BLOCK_SNIFFER_EGG_PLOP, 1.0, pitch);
        if let Some(item) = world.spawn_item(self.position(), egg) {
            item.set_default_pickup_delay();
        }
    }

    /// Vanilla parity: `Sniffer.playEatingSound`.
    fn play_eating_sound(&self) {
        self.play_sound(
            &sound_events::ENTITY_SNIFFER_EAT,
            1.0,
            rand::random_range(0.8..1.2),
        );
    }
}

impl Mob for SnifferEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
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

    /// Vanilla parity: `Sniffer.customServerAiStep`.
    fn custom_server_ai_step(&self) {
        let Some(world) = self.level() else {
            return;
        };
        self.brain.tick(&world, self);
        sniffer_ai::update_activity(&self.brain);
        Animal::custom_server_ai_step_animal(self);
    }

    /// Vanilla parity: `Sniffer.getAmbientSound`, which goes quiet while the
    /// sniffer's head is in the ground.
    fn ambient_sound(&self) -> Option<SoundEventRef> {
        if matches!(
            self.state(),
            SnifferState::Digging | SnifferState::Searching
        ) {
            return None;
        }
        Some(&sound_events::ENTITY_SNIFFER_IDLE)
    }

    /// Vanilla parity: `Sniffer.getMaxHeadYRot`.
    fn max_head_y_rot(&self) -> f32 {
        MAX_HEAD_Y_ROT
    }

    /// Vanilla parity: `Sniffer.mobInteract`, which adds the eating sound on top
    /// of the shared animal interaction.
    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        let is_food = {
            let inventory = player.inventory.lock();
            Self::is_sniffer_food(inventory.get_item_in_hand(hand))
        };

        let result = Animal::mob_interact_animal(self, player, hand);
        if result.consumes_action() && is_food {
            self.play_eating_sound();
        }
        result
    }
}

impl PathfinderMob for SnifferEntity {
    fn navigation_kind(&self) -> NavigationKind {
        NavigationKind::Ground
    }

    // MISSING FOUNDATION: vanilla's `Sniffer.onPathfindingStart` clears the
    // water malus while the sniffer is on fire or already wet, and
    // `onPathfindingDone` puts it back. Steel's navigation has no start/done
    // hook to hang those on, so a burning sniffer will not path into water to
    // put itself out -- it walks around the pond instead.
}

/// Hatches a baby sniffer out of a `SnifferEggBlock`.
///
/// Vanilla parity: the spawn half of `SnifferEggBlock.tick`, which lives here so
/// the entity owns its own construction.
pub fn hatch_sniffer_from_egg(world: &Arc<World>, pos: BlockPos) {
    let (x, y, z) = pos.get_center();
    let Some(entity) = ENTITIES.create(
        &vanilla_entities::SNIFFER,
        next_entity_id(),
        DVec3::new(x, y, z),
        Arc::downgrade(world),
    ) else {
        return;
    };

    if let Some(mob) = entity.as_mob() {
        mob.set_baby(true);
    }
    // Vanilla parity: `Mth.wrapDegrees(random.nextFloat() * 360.0F)`.
    entity.set_rotation((rand::random::<f32>().mul_add(360.0, -180.0), 0.0));
    entity.set_old_position_to_current();
    let _added = world.try_add_entity(entity);
}

#[cfg(test)]
mod tests;
