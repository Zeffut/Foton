//! Lightning bolt.
//!
//! Vanilla parity: `LightningBolt`. It exists for two or three ticks and never
//! moves: everything it does happens on the tick it appears. What makes it an
//! entity rather than a one-shot effect is the flicker -- after its life runs
//! out it may reset for another flash, and each flash is another chance to set
//! the ground alight and another sweep of damage over whatever is standing in
//! the six-block column above it.
//!
//! The mob reactions hang off [`crate::entity::Entity::thunder_hit`], which
//! `Creeper`, `Pig`, `MushroomCow`, `Turtle`, `CopperGolem`, `ArmorStand` and
//! the block-attached entities each override to charge, transform or shrug the
//! strike off; [`default_thunder_hit`] below is the base-class body everything
//! else gets.
//!
//! Not implemented: `Villager.thunderHit`, the witch conversion. It needs
//! `releaseAllPois` and the villager brain, which live elsewhere.
//!
//! Not implemented either: `CriteriaTriggers.LIGHTNING_STRIKE` and
//! `CHANNELED_LIGHTNING`. Both are advancement triggers and Steel has no
//! advancement system, which is also why the `cause` player a channeling
//! trident sets and the `hitEntities` set it feeds are absent here: they have
//! no other reader in vanilla.

use std::sync::{Arc, Weak};

use glam::DVec3;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::vanilla_game_rules::FIRE_SPREAD_RADIUS_AROUND_PLAYER;
use steel_registry::{REGISTRY, level_events, vanilla_damage_types, vanilla_game_events};
use steel_utils::locks::SyncMutex;
use steel_utils::types::{Difficulty, UpdateFlags};
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, WorldAabb};

use crate::behavior::BLOCK_BEHAVIORS;
use crate::behavior::blocks::FireBlock;
use crate::behavior::waxables::get_normal_from_waxed_variant;
use crate::behavior::weathering::{get_weather_state, previous_copper_stage};
use crate::entity::damage::DamageSource;
use crate::entity::{Entity, EntityBase, EntityBaseLoad, RemovalReason};
use crate::world::{LevelReader as _, World};

/// Ticks a fresh bolt has left.
///
/// Vanilla parity: `LightningBolt.START_LIFE`. The strike work runs while this
/// is still at its starting value, so it happens exactly once per bolt.
const START_LIFE: i32 = 2;

/// How far from the bolt an entity is still hit.
///
/// Vanilla parity: `LightningBolt.DAMAGE_RADIUS`.
const DAMAGE_RADIUS: f64 = 3.0;

/// Extra height the damage box reaches above the bolt.
///
/// Vanilla parity: the `getY() + 6.0 + 3.0` of `LightningBolt.tick`. A bolt
/// reaches much further up than sideways, so a player on a one-block pillar is
/// still inside it.
const DAMAGE_BOX_HEIGHT: f64 = 6.0;

/// Damage one strike deals.
///
/// Vanilla parity: the `5.0F` of `Entity.thunderHit`.
const STRIKE_DAMAGE: f32 = 5.0;

/// How long a struck entity would burn.
///
/// Vanilla parity: the `igniteForSeconds(8.0F)` of `Entity.thunderHit`.
const STRIKE_FIRE_TICKS: i32 = 160;

/// Fires the first flash tries to light beyond the one under the bolt.
///
/// Vanilla parity: the `spawnFire(4)` of `LightningBolt.tick`. Later flashes
/// pass zero, so only the first strike scatters fire around itself.
const EXTRA_FIRES_ON_STRIKE: i32 = 4;

/// Positions the copper-cleaning walk samples per step.
///
/// Vanilla parity: the `10` limit of `BlockPos.randomInCube` in
/// `LightningBolt.randomStepCleaningCopper`.
const COPPER_STEP_SAMPLES: i32 = 10;

/// State a bolt keeps for its short life.
struct BoltState {
    /// Ticks left in the current flash.
    life: i32,
    /// Flashes still to come after this one.
    flashes: i32,
    /// Whether this bolt is scenery only.
    visual_only: bool,
    /// How many fires this bolt lit, for `LightningBolt.getBlocksSetOnFire`.
    blocks_set_on_fire: i32,
}

impl BoltState {
    /// Vanilla parity: the `LightningBolt` constructor, which rolls one to
    /// three flashes.
    fn fresh() -> Self {
        Self {
            life: START_LIFE,
            flashes: rand::random_range(0..3) + 1,
            visual_only: false,
            blocks_set_on_fire: 0,
        }
    }
}

/// A lightning strike.
#[entity_behavior(class = "LightningBolt")]
pub struct LightningBoltEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    state: SyncMutex<BoltState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies
// `LightningBoltEntity`.
unsafe impl DowncastType for LightningBoltEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/lightning_bolt");
}

impl LightningBoltEntity {
    /// Creates a bolt about to strike.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            state: SyncMutex::new(BoltState::fresh()),
        }
    }

    /// Creates a bolt from saved base data.
    ///
    /// Vanilla marks the entity type `noSave`, so this only ever runs for a
    /// world that was written by something else.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            state: SyncMutex::new(BoltState::fresh()),
        }
    }

    /// Makes this bolt scenery: no damage, no fire.
    ///
    /// Vanilla parity: `LightningBolt.setVisualOnly`. Used by the trap-chest
    /// and lightning-effect paths that want the flash without the consequences.
    pub fn set_visual_only(&self, visual_only: bool) {
        self.state.lock().visual_only = visual_only;
    }

    /// Returns whether this bolt is scenery only.
    #[must_use]
    pub fn is_visual_only(&self) -> bool {
        self.state.lock().visual_only
    }

    /// Returns how many blocks this bolt has set alight.
    ///
    /// Vanilla parity: `LightningBolt.getBlocksSetOnFire`, read by the
    /// lightning-strike advancement trigger Steel does not have yet.
    #[must_use]
    pub fn blocks_set_on_fire(&self) -> i32 {
        self.state.lock().blocks_set_on_fire
    }

    /// Returns the block the bolt landed on.
    ///
    /// Vanilla parity: `LightningBolt.getStrikePosition`. The epsilon matters:
    /// a bolt spawned exactly on top of a block belongs to that block, not to
    /// the air above it.
    fn strike_position(&self) -> BlockPos {
        let position = self.position();
        BlockPos::containing(position.x, position.y - 1.0e-6, position.z)
    }

    /// Vanilla parity: `LightningBolt.powerLightningRod`.
    fn power_lightning_rod(&self, world: &Arc<World>) {
        let pos = self.strike_position();
        let state = world.get_block_state(pos);
        if let Some(rod) = BLOCK_BEHAVIORS
            .get_behavior(state.get_block())
            .as_lightning_rod()
        {
            rod.on_lightning_strike(state, world, pos);
        }
    }

    /// Vanilla parity: `LightningBolt.spawnFire`.
    fn spawn_fire(&self, world: &Arc<World>, additional_sources: i32) {
        if self.is_visual_only() {
            return;
        }
        let pos = self.block_position();
        if !can_spread_fire_around(world, pos) {
            return;
        }

        let mut lit = i32::from(try_light(world, pos));
        for _ in 0..additional_sources {
            let nearby = pos.offset(
                rand::random_range(0..3) - 1,
                rand::random_range(0..3) - 1,
                rand::random_range(0..3) - 1,
            );
            lit += i32::from(try_light(world, nearby));
        }

        if lit != 0 {
            self.state.lock().blocks_set_on_fire += lit;
        }
    }

    /// Vanilla parity: the damage sweep at the end of `LightningBolt.tick`.
    fn strike_entities(&self, world: &Arc<World>) {
        let position = self.position();
        let box_of_effect = WorldAabb::new(
            position.x - DAMAGE_RADIUS,
            position.y - DAMAGE_RADIUS,
            position.z - DAMAGE_RADIUS,
            position.x + DAMAGE_RADIUS,
            position.y + DAMAGE_BOX_HEIGHT + DAMAGE_RADIUS,
            position.z + DAMAGE_RADIUS,
        );

        let own_id = self.id();
        let struck = world.get_entities_in_aabb_matching(&box_of_effect, |entity| {
            entity.id() != own_id && entity.is_alive()
        });
        for entity in struck {
            entity.thunder_hit(world, self);
        }
    }
}

impl Entity for LightningBoltEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    /// Vanilla parity: `LightningBolt.getSoundSource`.
    fn sound_source(&self) -> SoundSource {
        SoundSource::Weather
    }

    /// Vanilla parity: `LightningBolt.tick`.
    ///
    /// VANILLA CLIENT-LOCAL: the `isClientSide` half plays
    /// `LIGHTNING_BOLT_THUNDER` and `LIGHTNING_BOLT_IMPACT` through
    /// `playLocalSound` and calls `setSkyFlashTime(2)`. The vanilla client
    /// ticks the bolt it was sent and runs both itself, so the server sends no
    /// sound packet -- doing so would play the thunder twice.
    fn tick(&self) {
        self.default_tick();

        let Some(world) = self.level() else {
            return;
        };

        let (life, visual_only) = {
            let state = self.state.lock();
            (state.life, state.visual_only)
        };

        if life == START_LIFE {
            // Vanilla only scatters fire on Normal and Hard; on Easy and
            // Peaceful a strike is loud and harmless to the terrain.
            if matches!(world.difficulty(), Difficulty::Normal | Difficulty::Hard) {
                self.spawn_fire(&world, EXTRA_FIRES_ON_STRIKE);
            }
            self.power_lightning_rod(&world);
            clear_copper_on_lightning_strike(&world, self.strike_position());
            self.game_event(&vanilla_game_events::LIGHTNING_STRIKE);
        }

        let life = {
            let mut state = self.state.lock();
            state.life -= 1;
            state.life
        };

        if life < 0 {
            let flashes = self.state.lock().flashes;
            if flashes == 0 {
                self.set_removed(RemovalReason::Discarded);
                return;
            }
            // The gap before the next flash is random, so a multi-flash bolt
            // stutters instead of strobing on a fixed beat.
            if life < -rand::random_range(0..10) {
                {
                    let mut state = self.state.lock();
                    state.flashes -= 1;
                    state.life = 1;
                }
                self.spawn_fire(&world, 0);
            }
        }

        if self.state.lock().life >= 0 && !visual_only {
            self.strike_entities(&world);
        }
    }

    /// Vanilla parity: `LightningBolt.hurtServer`, which always refuses.
    fn hurt(&self, _world: &World, _source: &DamageSource, _amount: f32) -> bool {
        false
    }
}

/// Applies one strike to a single entity.
///
/// Vanilla parity: the base-class body of `Entity.thunderHit`, which
/// [`crate::entity::Entity::thunder_hit`] runs for every entity that does not
/// override it.
///
/// The fire here reads oddly on purpose: vanilla bumps the counter by one and
/// then checks it against zero, so the eight-second ignition only ever fires
/// for an entity whose counter was at -1. For everything else a strike leaves
/// one tick of fire, which is the flicker seen in game.
pub fn default_thunder_hit(entity: &dyn Entity, world: &World) {
    entity.set_remaining_fire_ticks(entity.remaining_fire_ticks() + 1);
    if entity.remaining_fire_ticks() == 0 {
        entity.ignite_for_ticks(STRIKE_FIRE_TICKS);
    }
    entity.hurt(
        world,
        &DamageSource::environment(&vanilla_damage_types::LIGHTNING_BOLT),
        STRIKE_DAMAGE,
    );
}

/// Returns whether fire may spread near `pos`.
///
/// Vanilla parity: `ServerLevel.canSpreadFireAround` plus
/// `ChunkMap.anyPlayerCloseEnoughTo`. A radius of -1 turns the check off; with
/// the default 128 a strike in an empty corner of the world lights nothing,
/// which is what keeps unattended chunks from burning.
fn can_spread_fire_around(world: &Arc<World>, pos: BlockPos) -> bool {
    let radius = world.get_game_rule(&FIRE_SPREAD_RADIUS_AROUND_PLAYER);
    if radius == -1 {
        return true;
    }

    let target = DVec3::new(f64::from(pos.x()), f64::from(pos.y()), f64::from(pos.z()));
    let mut close_enough = false;
    world.players.iter_players(|_, player| {
        if player.is_spectator() {
            return true;
        }
        if player.position().distance(target) < f64::from(radius) {
            close_enough = true;
            return false;
        }
        true
    });
    close_enough
}

/// Tries to put a fire block at `pos`, returning whether one appeared.
///
/// Vanilla parity: the inner body of `LightningBolt.spawnFire`, which is
/// `BaseFireBlock.getState` followed by an air-and-survival check.
fn try_light(world: &Arc<World>, pos: BlockPos) -> bool {
    let fire = FireBlock::get_state(world.as_ref(), pos);
    if !world.get_block_state(pos).is_air() {
        return false;
    }
    if !BLOCK_BEHAVIORS
        .get_behavior(fire.get_block())
        .can_survive(fire, world.as_ref(), pos)
    {
        return false;
    }
    world.set_block(pos, fire, UpdateFlags::UPDATE_ALL)
}

/// Strips the oxidation off copper the bolt touched.
///
/// Vanilla parity: `LightningBolt.clearCopperOnLightningStrike`. A strike on
/// waxed copper cleans its neighbors without changing the waxed block itself,
/// which is how a single rod can be used to scrub a whole copper wall.
fn clear_copper_on_lightning_strike(world: &Arc<World>, struck_pos: BlockPos) {
    let struck_state = world.get_block_state(struck_pos);
    let struck_block = struck_state.get_block();
    let is_waxed = get_normal_from_waxed_variant(struck_block).is_some();
    let is_weathering_copper = get_weather_state(struck_block).is_some();
    if !is_weathering_copper && !is_waxed {
        return;
    }

    if is_weathering_copper {
        world.set_block(
            struck_pos,
            first_copper_stage(struck_state),
            UpdateFlags::UPDATE_ALL,
        );
    }

    let strikes = rand::random_range(0..3) + 3;
    for _ in 0..strikes {
        let steps = rand::random_range(0..8) + 1;
        random_walk_cleaning_copper(world, struck_pos, steps);
    }
}

/// Vanilla parity: `WeatheringCopper.getFirst`, walking back to unaffected.
fn first_copper_stage(state: BlockStateId) -> BlockStateId {
    let mut block: BlockRef = state.get_block();
    while let Some(previous) = previous_copper_stage(block) {
        block = previous;
    }
    REGISTRY.blocks.copy_matching_properties(state, block)
}

/// Vanilla parity: `LightningBolt.randomWalkCleaningCopper`.
fn random_walk_cleaning_copper(world: &Arc<World>, origin: BlockPos, step_count: i32) {
    let mut work_pos = origin;
    for _ in 0..step_count {
        let Some(next) = random_step_cleaning_copper(world, work_pos) else {
            break;
        };
        work_pos = next;
    }
}

/// Vanilla parity: `LightningBolt.randomStepCleaningCopper`.
///
/// Samples random positions in the 3x3x3 cube around `pos` and de-oxidizes the
/// first copper it finds, returning where the walk continues from.
fn random_step_cleaning_copper(world: &Arc<World>, pos: BlockPos) -> Option<BlockPos> {
    for _ in 0..COPPER_STEP_SAMPLES {
        let candidate = BlockPos::new(
            rand::random_range(pos.x() - 1..=pos.x() + 1),
            rand::random_range(pos.y() - 1..=pos.y() + 1),
            rand::random_range(pos.z() - 1..=pos.z() + 1),
        );
        let state = world.get_block_state(candidate);
        if get_weather_state(state.get_block()).is_none() {
            continue;
        }
        if let Some(previous) = previous_copper_stage(state.get_block()) {
            world.set_block(
                candidate,
                REGISTRY.blocks.copy_matching_properties(state, previous),
                UpdateFlags::UPDATE_ALL,
            );
        }
        world.level_event(level_events::PARTICLES_ELECTRIC_SPARK, candidate, -1, None);
        return Some(candidate);
    }
    None
}

#[cfg(test)]
mod tests {
    use steel_registry::blocks::properties::{BlockStateProperties, BoolProperty};
    use steel_registry::item_stack::ItemStack;
    use steel_registry::{init_vanilla_registry, vanilla_blocks, vanilla_entities, vanilla_items};
    use steel_utils::types::InteractionHand;
    use steel_utils::{ChunkPos, Downcast as _};

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::entity::entities::{
        ArmorStandEntity, CowEntity, CreeperEntity, ItemFrameEntity, MushroomCowEntity,
        MushroomCowVariant, PigEntity, ZombifiedPiglinEntity,
    };
    use crate::entity::next_entity_id;
    use crate::entity::{LivingEntity as _, Mob as _};
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    const POWERED: &BoolProperty = &BlockStateProperties::POWERED;

    fn bolt_world(key: &'static str) -> Arc<World> {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world(key);
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        world
    }

    fn strike_at(world: &Arc<World>, position: DVec3) -> Arc<LightningBoltEntity> {
        let bolt = Arc::new(LightningBoltEntity::new(
            &vanilla_entities::LIGHTNING_BOLT,
            next_entity_id(),
            position,
            Arc::downgrade(world),
        ));
        world
            .try_add_entity(bolt.clone())
            .expect("the strike chunk is loaded");
        bolt
    }

    fn has_zombified_piglin(world: &Arc<World>) -> bool {
        let everywhere = WorldAabb::new(-32.0, 0.0, -32.0, 32.0, 128.0, 32.0);
        world
            .get_entities_in_aabb(&everywhere)
            .iter()
            .any(|entity| {
                entity
                    .as_ref()
                    .downcast_ref::<ZombifiedPiglinEntity>()
                    .is_some()
            })
    }

    fn zombified_piglin_weapon(world: &Arc<World>) -> Option<ItemStack> {
        let everywhere = WorldAabb::new(-32.0, 0.0, -32.0, 32.0, 128.0, 32.0);
        world
            .get_entities_in_aabb(&everywhere)
            .iter()
            .find_map(|entity| {
                let piglin = entity.as_ref().downcast_ref::<ZombifiedPiglinEntity>()?;
                assert!(
                    piglin.is_persistence_required(),
                    "vanilla marks a converted piglin persistent"
                );
                Some(piglin.get_item_in_hand(InteractionHand::MainHand))
            })
    }

    fn tick_until_gone(bolt: &Arc<LightningBoltEntity>) -> i32 {
        for ticks in 1..=200 {
            bolt.tick();
            if bolt.is_removed() {
                return ticks;
            }
        }
        panic!("the bolt never burned out");
    }

    #[test]
    fn a_bolt_burns_itself_out_after_its_last_flash() {
        let world = bolt_world("lightning_bolt_burns_out");
        let bolt = strike_at(&world, DVec3::new(4.5, 64.0, 4.5));

        // Three ticks is the floor: two of life plus the one that takes it
        // below zero. Extra flashes only ever add ticks.
        assert!(tick_until_gone(&bolt) >= 3);
    }

    #[test]
    fn a_bolt_shocks_and_singes_everything_standing_in_its_box() {
        let world = bolt_world("lightning_bolt_shocks_bystanders");
        let cow = Arc::new(CowEntity::new(
            &vanilla_entities::COW,
            next_entity_id(),
            DVec3::new(5.5, 64.0, 4.5),
            Arc::downgrade(&world),
        ));
        world
            .try_add_entity(cow.clone())
            .expect("the cow's chunk is loaded");
        let full_health = cow.get_health();

        let bolt = strike_at(&world, DVec3::new(4.5, 64.0, 4.5));
        bolt.tick();

        assert!(cow.get_health() < full_health);
        // Vanilla's `thunderHit` only bumps the counter by one, so the burn is
        // a single tick rather than the eight seconds the dead branch names.
        assert_eq!(cow.remaining_fire_ticks(), 1);
    }

    #[test]
    fn a_visual_only_bolt_leaves_bystanders_and_the_ground_alone() {
        let world = bolt_world("lightning_bolt_visual_only");
        assert!(world.set_game_rule(&FIRE_SPREAD_RADIUS_AROUND_PLAYER, -1));
        let ground = BlockPos::new(4, 63, 4);
        assert!(world.set_block(
            ground,
            vanilla_blocks::STONE.default_state(),
            UpdateFlags::UPDATE_ALL
        ));

        let cow = Arc::new(CowEntity::new(
            &vanilla_entities::COW,
            next_entity_id(),
            DVec3::new(4.5, 64.0, 4.5),
            Arc::downgrade(&world),
        ));
        world
            .try_add_entity(cow.clone())
            .expect("the cow's chunk is loaded");
        let full_health = cow.get_health();

        let bolt = strike_at(&world, DVec3::new(4.5, 64.0, 4.5));
        bolt.set_visual_only(true);
        bolt.tick();

        assert_eq!(cow.get_health().to_bits(), full_health.to_bits());
        assert_eq!(cow.remaining_fire_ticks(), 0);
        assert_eq!(bolt.blocks_set_on_fire(), 0);
        assert!(world.get_block_state(ground.above()).is_air());
    }

    #[test]
    fn a_strike_on_solid_ground_lights_it_when_the_game_rule_allows_it() {
        let world = bolt_world("lightning_bolt_lights_the_ground");
        // The default radius wants a player nearby; -1 is vanilla's "always".
        assert!(world.set_game_rule(&FIRE_SPREAD_RADIUS_AROUND_PLAYER, -1));
        let ground = BlockPos::new(4, 63, 4);
        assert!(world.set_block(
            ground,
            vanilla_blocks::STONE.default_state(),
            UpdateFlags::UPDATE_ALL
        ));

        let bolt = strike_at(&world, DVec3::new(4.5, 64.0, 4.5));
        bolt.tick();

        assert_eq!(
            world.get_block_state(ground.above()).get_block(),
            &vanilla_blocks::FIRE
        );
        assert!(bolt.blocks_set_on_fire() >= 1);
    }

    #[test]
    fn an_empty_world_stays_unlit_because_no_player_is_close_enough() {
        let world = bolt_world("lightning_bolt_no_player_no_fire");
        let ground = BlockPos::new(4, 63, 4);
        assert!(world.set_block(
            ground,
            vanilla_blocks::STONE.default_state(),
            UpdateFlags::UPDATE_ALL
        ));

        let bolt = strike_at(&world, DVec3::new(4.5, 64.0, 4.5));
        bolt.tick();

        assert_eq!(bolt.blocks_set_on_fire(), 0);
        assert!(world.get_block_state(ground.above()).is_air());
    }

    #[test]
    fn a_bolt_landing_on_a_lightning_rod_powers_it() {
        let world = bolt_world("lightning_bolt_powers_the_rod");
        let rod_pos = BlockPos::new(4, 64, 4);
        assert!(world.set_block(
            rod_pos,
            vanilla_blocks::LIGHTNING_ROD.default_state(),
            UpdateFlags::UPDATE_ALL
        ));

        let bolt = strike_at(&world, DVec3::new(4.5, 65.0, 4.5));
        bolt.tick();

        assert!(world.get_block_state(rod_pos).get_value(POWERED));
    }

    #[test]
    fn a_bolt_scrubs_the_oxidation_off_the_copper_it_hits() {
        let world = bolt_world("lightning_bolt_cleans_copper");
        let copper_pos = BlockPos::new(4, 64, 4);
        assert!(world.set_block(
            copper_pos,
            vanilla_blocks::OXIDIZED_COPPER.default_state(),
            UpdateFlags::UPDATE_ALL
        ));

        let bolt = strike_at(&world, DVec3::new(4.5, 65.0, 4.5));
        bolt.tick();

        assert_eq!(
            world.get_block_state(copper_pos).get_block(),
            &vanilla_blocks::COPPER_BLOCK
        );
    }

    #[test]
    fn a_struck_creeper_stays_charged() {
        let world = bolt_world("lightning_bolt_charges_a_creeper");
        let creeper = Arc::new(CreeperEntity::new(
            &vanilla_entities::CREEPER,
            next_entity_id(),
            DVec3::new(4.5, 64.0, 4.5),
            Arc::downgrade(&world),
        ));
        world
            .try_add_entity(creeper.clone())
            .expect("the creeper's chunk is loaded");
        assert!(!creeper.is_powered());

        strike_at(&world, DVec3::new(4.5, 64.0, 4.5)).tick();

        assert!(creeper.is_powered());
        // The charge rides on top of the ordinary strike, not instead of it.
        assert_eq!(creeper.remaining_fire_ticks(), 1);
    }

    #[test]
    fn a_struck_pig_becomes_an_armed_zombified_piglin() {
        let world = bolt_world("lightning_bolt_zombifies_a_pig");
        let pig = Arc::new(PigEntity::new(
            &vanilla_entities::PIG,
            next_entity_id(),
            DVec3::new(4.5, 64.0, 4.5),
            Arc::downgrade(&world),
        ));
        world
            .try_add_entity(pig.clone())
            .expect("the pig's chunk is loaded");

        strike_at(&world, DVec3::new(4.5, 64.0, 4.5)).tick();

        assert!(pig.is_removed());
        // Vanilla arms it from `populateDefaultEquipmentSlots`, so a bare hand
        // means the conversion callback never ran.
        let weapon = zombified_piglin_weapon(&world).expect("the pig zombified");
        assert!(
            weapon.is(&vanilla_items::GOLDEN_SWORD) || weapon.is(&vanilla_items::GOLDEN_SPEAR),
            "a zombified piglin spawns holding gold"
        );
    }

    #[test]
    fn a_struck_pig_on_peaceful_stays_a_pig() {
        let world = bolt_world("lightning_bolt_spares_a_peaceful_pig");
        world.set_difficulty(Difficulty::Peaceful);
        let pig = Arc::new(PigEntity::new(
            &vanilla_entities::PIG,
            next_entity_id(),
            DVec3::new(4.5, 64.0, 4.5),
            Arc::downgrade(&world),
        ));
        world
            .try_add_entity(pig.clone())
            .expect("the pig's chunk is loaded");

        strike_at(&world, DVec3::new(4.5, 64.0, 4.5)).tick();

        assert!(!pig.is_removed());
        assert!(!has_zombified_piglin(&world));
        // Peaceful falls through to the base body, which still singes.
        assert_eq!(pig.remaining_fire_ticks(), 1);
    }

    #[test]
    fn one_bolt_flips_a_mooshroom_once_however_long_it_flashes() {
        let world = bolt_world("lightning_bolt_flips_a_mooshroom");
        let mooshroom = Arc::new(MushroomCowEntity::new(
            &vanilla_entities::MOOSHROOM,
            next_entity_id(),
            DVec3::new(4.5, 64.0, 4.5),
            Arc::downgrade(&world),
        ));
        world
            .try_add_entity(mooshroom.clone())
            .expect("the mooshroom's chunk is loaded");
        assert_eq!(mooshroom.variant(), MushroomCowVariant::Red);
        let full_health = mooshroom.get_health();

        // A bolt sweeps for entities on every tick it is alive, so ticking it
        // out is what proves the per-bolt guard rather than a lucky single hit.
        let bolt = strike_at(&world, DVec3::new(4.5, 64.0, 4.5));
        tick_until_gone(&bolt);

        assert_eq!(mooshroom.variant(), MushroomCowVariant::Brown);
        // Vanilla's override has no `super` call: the flip is the whole effect.
        assert_eq!(mooshroom.get_health().to_bits(), full_health.to_bits());
        assert_eq!(mooshroom.remaining_fire_ticks(), 0);
    }

    #[test]
    fn a_strike_leaves_an_armor_stand_and_an_item_frame_untouched() {
        let world = bolt_world("lightning_bolt_spares_decoration");
        let stand = Arc::new(ArmorStandEntity::new(
            &vanilla_entities::ARMOR_STAND,
            next_entity_id(),
            DVec3::new(4.5, 64.0, 4.5),
            Arc::downgrade(&world),
        ));
        world
            .try_add_entity(stand.clone())
            .expect("the stand's chunk is loaded");
        let stand_health = stand.get_health();

        let frame = Arc::new(ItemFrameEntity::new(
            &vanilla_entities::ITEM_FRAME,
            next_entity_id(),
            DVec3::new(5.5, 64.0, 4.5),
            Arc::downgrade(&world),
        ));
        world
            .try_add_entity(frame.clone())
            .expect("the frame's chunk is loaded");

        strike_at(&world, DVec3::new(4.5, 64.0, 4.5)).tick();

        assert_eq!(stand.get_health().to_bits(), stand_health.to_bits());
        assert_eq!(stand.remaining_fire_ticks(), 0);
        assert!(!frame.is_removed());
        assert_eq!(frame.remaining_fire_ticks(), 0);
    }
}
