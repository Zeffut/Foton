//! The ender dragon.
//!
//! Vanilla parity: `EnderDragon`. Almost nothing about it is a normal mob. It
//! has no goals -- a [phase](phases) decides everything it does. It has no
//! collision box of its own: it is [eight hitboxes](EnderDragonPart) that
//! follow its body a few ticks behind, and every hit a player lands arrives
//! addressed to one of them. It does not pathfind through the world but between
//! [twenty-four fixed points](path) in the sky. And it does not die when its
//! health runs out; it flies to the exit portal first.
//!
//! Two of those are why this file exists at all, and why the entity manager and
//! the interact handler had to learn about parts. See [`part`] for the details.
//!
//! The dragon owns none of the fight around it. The boss bar, the crystal
//! count, the exit portal and the experience all belong to
//! [`EnderDragonFight`], which the End hangs on its [`World`]; a dragon
//! summoned anywhere else has no bar and no crystals, exactly as in vanilla.
//!
//! **Gaps**, all of them things nothing in the tree can yet express:
//!
//! * `applyEffectsFromBlocks`, and the `interpolation` the client half of
//!   `aiStep` drives, are not carried.
//! * The growl and flap sounds, the death particles and the `onFlap` hook are
//!   client-local in vanilla and have no server work.

use std::f32::consts::PI;
use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_math::trig;
use foton_protocol::packets::game::SoundSource;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::vanilla_damage_type_tags::DamageTypeTag;
use foton_registry::vanilla_entity_data::EnderDragonEntityData;
use foton_registry::vanilla_game_rules::{MOB_DROPS, MOB_GRIEFING};
use foton_registry::{
    blocks::block_state_ext::BlockStateExt as _, level_events, sound_events, vanilla_damage_types,
    vanilla_entities, vanilla_game_events,
};
use foton_utils::locks::SyncMutex;
use foton_utils::{
    BlockPos, Downcast as _, DowncastType, DowncastTypeKey, WorldAabb, wrap_degrees,
};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::chunk::heightmap::HeightmapType;
use crate::dimension::end::EnderDragonFight;
use crate::entity::LivingEntitySyncedData;
use crate::entity::ai::node::Node;
use crate::entity::ai::path::Path;
use crate::entity::damage::DamageSource;
use crate::entity::entities::{EndCrystalEntity, ExperienceOrbEntity};
use crate::entity::{
    Enemy, Entity, EntityBase, EntityBaseLoad, EntitySyncedData, LivingEntity, LivingEntityBase,
    Mob, MobBase, MobEffectInstance, MoveControlKind, NavigationKind, PathfinderMob, RemovalReason,
    reserve_entity_ids,
};
use crate::physics::{MoveResult, MoverType};
use crate::player::Player;
use crate::world::World;
use crate::worldgen::feature::features::end_podium;

pub mod flight_history;
pub mod part;
pub mod path;
pub mod phases;

#[cfg(test)]
mod tests;

pub use flight_history::DragonFlightHistory;
pub use part::{DragonPartIndex, EnderDragonPart};
pub use path::DragonPathfinder;
pub use phases::{EnderDragonPhase, EnderDragonPhaseManager};

/// Entity IDs a dragon occupies: itself and its eight hitboxes.
///
/// Vanilla gets the same block for free, because each `EnderDragonPart` runs
/// `Entity`'s constructor -- and so draws the next ID -- from inside the
/// dragon's own constructor. See [`reserve_entity_ids`].
pub const ENDER_DRAGON_ID_BLOCK: u32 = 1 + part::PART_COUNT as u32;

/// The dragon's health.
///
/// Vanilla parity: the `Attributes.MAX_HEALTH, 200.0` of `createAttributes`.
pub const MAX_HEALTH: f32 = 200.0;

/// Share of its health a sitting dragon takes before it gives up and flies.
///
/// Vanilla parity: `EnderDragon.SITTING_ALLOWED_DAMAGE_PERCENTAGE`.
const SITTING_ALLOWED_DAMAGE_PERCENTAGE: f32 = 0.25;

/// Damage below which a hit is discarded entirely.
///
/// Vanilla parity: the `damage < 0.01F` of `EnderDragon.hurt`.
const MINIMUM_DAMAGE: f32 = 0.01;

/// Ticks the death animation runs for.
///
/// Vanilla parity: the `this.dragonDeathTime >= 200` of `tickDeath`.
const DEATH_TIME: i32 = 200;

/// Tick the death animation starts paying out experience on.
///
/// Vanilla parity: the `dragonDeathTime > 150` of `tickDeath`.
const DEATH_XP_START_TICK: i32 = 150;

/// How often experience is paid out during the death animation.
const DEATH_XP_INTERVAL: i32 = 5;

/// Experience a dragon that is not the first of its world is worth.
///
/// Vanilla parity: the `int xpCount = 500` of `tickDeath`. The dragon has no
/// `xpReward`: it awards orbs itself, so nothing here goes through the mob
/// experience path.
const DEATH_XP: i32 = 500;

/// Experience the first dragon of a world is worth.
///
/// Vanilla parity: the `xpCount = 12000` a fight that has not killed one before
/// substitutes.
pub const FIRST_KILL_DEATH_XP: i32 = 12_000;

/// Share of the total paid out on each tick of the death animation.
const DEATH_XP_TRICKLE_SHARE: f32 = 0.08;

/// Share of the total paid out when the animation ends.
const DEATH_XP_FINAL_SHARE: f32 = 0.2;

/// How far the dragon looks for a crystal to heal from.
///
/// Vanilla parity: the `inflate(32.0)` of `checkCrystals`.
const CRYSTAL_SCAN_RANGE: f64 = 32.0;

/// How often a healing dragon gains a point of health.
///
/// Vanilla parity: the `this.tickCount % 10 == 0` of `checkCrystals`.
const CRYSTAL_HEAL_INTERVAL: i32 = 10;

/// Damage the wing sweep does.
///
/// Vanilla parity: the `5.0F` of `EnderDragon.knockBack`.
const WING_DAMAGE: f32 = 5.0;

/// Damage the head and neck do to anything they pass through.
///
/// Vanilla parity: the `10.0F` of the private `EnderDragon.hurt(level, list)`.
const BITE_DAMAGE: f32 = 10.0;

/// Damage a destroyed crystal does to the dragon.
///
/// Vanilla parity: the `10.0F` of `onCrystalDestroyed`.
const CRYSTAL_EXPLOSION_DAMAGE: f32 = 10.0;

/// How far a destroyed crystal looks for someone to blame.
///
/// Vanilla parity: `EnderDragon.CRYSTAL_DESTROY_TARGETING`.
const CRYSTAL_DESTROY_RANGE: f64 = 64.0;

/// Returns where the exit portal stands for a fight based at `origin`.
///
/// Vanilla parity: `EndPodiumFeature.getLocation`.
#[must_use]
pub const fn end_podium_location(origin: BlockPos) -> BlockPos {
    end_podium::location(origin)
}

/// The ender dragon.
#[entity_behavior(class = "EnderDragon")]
pub struct EnderDragon {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<EnderDragonEntityData>,
    /// The eight hitboxes, in [`DragonPartIndex::ORDER`].
    ///
    /// Vanilla parity: `EnderDragon.subEntities`.
    parts: [Arc<EnderDragonPart>; part::PART_COUNT],
    /// Vanilla parity: `EnderDragon.flightHistory`.
    flight_history: SyncMutex<DragonFlightHistory>,
    /// Vanilla parity: `EnderDragon.phaseManager`.
    phase_manager: EnderDragonPhaseManager,
    /// Vanilla parity: `EnderDragon.nodes`, `nodeAdjacency` and `openSet`.
    pathfinder: SyncMutex<DragonPathfinder>,
    state: SyncMutex<DragonState>,
}

/// The dragon's own mutable state.
struct DragonState {
    /// Vanilla parity: `EnderDragon.dragonDeathTime`.
    dragon_death_time: i32,
    /// Vanilla parity: `EnderDragon.yRotA`.
    y_rot_a: f32,
    /// Vanilla parity: `EnderDragon.sittingDamageReceived`.
    sitting_damage_received: f32,
    /// Vanilla parity: `EnderDragon.inWall`.
    in_wall: bool,
    /// Vanilla parity: `EnderDragon.fightOrigin`.
    fight_origin: BlockPos,
    /// Network ID of the crystal the dragon is currently healing from.
    ///
    /// Vanilla parity: `EnderDragon.nearestCrystal`, held as an ID for the same
    /// reason a hitbox holds its parent as one.
    nearest_crystal: Option<i32>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `EnderDragon`.
unsafe impl DowncastType for EnderDragon {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/ender_dragon");
}

impl EnderDragon {
    /// Creates a dragon at runtime.
    ///
    /// **The `id` the entity registry hands in is discarded.** A dragon needs
    /// [`ENDER_DRAGON_ID_BLOCK`] consecutive IDs, because the client derives its
    /// eight hitboxes as `dragonId + 1 ..= dragonId + 8` and has nothing else to
    /// go on; a single ID drawn by the caller cannot promise the eight after it
    /// are free. So the dragon reserves its own block and takes the first of it,
    /// and every caller reads the ID back off the entity, as they already do.
    ///
    /// Vanilla has the same shape for the same reason: an `EnderDragon`'s ID and
    /// its parts' IDs all come from inside its own constructor, one after
    /// another, never from the code that asked for a dragon.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, _id: i32, position: DVec3, world: Weak<World>) -> Self {
        let id = reserve_entity_ids(ENDER_DRAGON_ID_BLOCK);
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world.clone()),
            entity_type,
            position,
            world,
        )
    }

    /// Creates a dragon from saved base data.
    ///
    /// The loaded ID is replaced for the reason [`Self::new`] gives. Entity IDs
    /// are session-local and never persisted, so nothing is lost by it.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, mut load: EntityBaseLoad) -> Self {
        load.id = reserve_entity_ids(ENDER_DRAGON_ID_BLOCK);
        let position = load.position;
        let world = load.world.clone();
        Self::new_with_base(
            EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            position,
            world,
        )
    }

    fn new_with_base(
        base: EntityBase,
        entity_type: EntityTypeRef,
        position: DVec3,
        world: Weak<World>,
    ) -> Self {
        let id = base.id();
        let parts = DragonPartIndex::ORDER.map(|index| {
            Arc::new(EnderDragonPart::new(
                entity_type,
                id + 1 + index.slot() as i32,
                id,
                index,
                position,
                world.clone(),
            ))
        });

        let living_base = LivingEntityBase::new(entity_type);
        let mut entity_data = EnderDragonEntityData::new();
        // Vanilla parity: the `setHealth(getMaxHealth())` of the constructor.
        living_base.initialize_synced_data(&mut entity_data);

        Self {
            base,
            entity_type,
            living_base,
            mob_base: MobBase::new(),
            entity_data: SyncMutex::new(entity_data),
            parts,
            flight_history: SyncMutex::new(DragonFlightHistory::new()),
            phase_manager: EnderDragonPhaseManager::new(),
            pathfinder: SyncMutex::new(DragonPathfinder::new()),
            state: SyncMutex::new(DragonState {
                dragon_death_time: 0,
                y_rot_a: 0.0,
                sitting_damage_received: 0.0,
                in_wall: false,
                fight_origin: BlockPos::new(0, 0, 0),
                nearest_crystal: None,
            }),
        }
    }

    /// Returns the eight hitboxes.
    ///
    /// Vanilla parity: `EnderDragon.getSubEntities`. The entity manager reads
    /// this to register the dragon's parts for ID lookup.
    #[must_use]
    pub const fn sub_entities(&self) -> &[Arc<EnderDragonPart>; part::PART_COUNT] {
        &self.parts
    }

    /// Returns one hitbox.
    #[must_use]
    pub const fn part(&self, index: DragonPartIndex) -> &Arc<EnderDragonPart> {
        &self.parts[index.slot()]
    }

    /// Returns the head hitbox.
    ///
    /// Vanilla parity: the public `EnderDragon.head`, which is the only part
    /// vanilla exposes by name.
    #[must_use]
    pub const fn head(&self) -> &Arc<EnderDragonPart> {
        self.part(DragonPartIndex::Head)
    }

    /// Returns the dragon's phases.
    ///
    /// Vanilla parity: `EnderDragon.getPhaseManager`.
    #[must_use]
    pub const fn phase_manager(&self) -> &EnderDragonPhaseManager {
        &self.phase_manager
    }

    /// Writes the synced phase the client switches its own animation on.
    ///
    /// Vanilla parity: the `getEntityData().set(DATA_PHASE, ...)` of
    /// `EnderDragonPhaseManager.setPhase`.
    pub fn set_synced_phase(&self, phase: EnderDragonPhase) {
        self.entity_data.lock().phase.set(phase.id());
    }

    /// Returns where this dragon's fight is centered.
    ///
    /// Vanilla parity: `EnderDragon.getFightOrigin`.
    #[must_use]
    pub fn fight_origin(&self) -> BlockPos {
        self.state.lock().fight_origin
    }

    /// Vanilla parity: `EnderDragon.setFightOrigin`.
    pub fn set_fight_origin(&self, origin: BlockPos) {
        self.state.lock().fight_origin = origin;
    }

    /// Returns the fight this dragon is the dragon of.
    ///
    /// Vanilla parity: `EnderDragon.getDragonFight`. Vanilla caches the level's
    /// fight on the dragon the first tick its UUID matches the fight's
    /// `dragonUUID`, and `EnderDragonFight.createNewDragon` hands it over
    /// directly; both amount to "the level has a fight and this is its dragon",
    /// which is what is tested here rather than carrying a second copy of the
    /// answer. The one case the two differ in is a fight that re-targets
    /// another dragon after twenty seconds of not seeing this one: vanilla
    /// leaves the old dragon holding the fight, and this drops it at once.
    #[must_use]
    pub fn dragon_fight<'world>(
        &self,
        world: &'world Arc<World>,
    ) -> Option<&'world EnderDragonFight> {
        let fight = world.dragon_fight()?;
        (fight.dragon_uuid() == Some(self.uuid())).then_some(fight)
    }

    /// Returns whether this dragon belongs to a fight.
    ///
    /// Vanilla parity: `getDragonFight() != null`. False for a dragon summoned
    /// outside the End, which is what keeps it on the inner rings and stops it
    /// ever rolling for a landing.
    #[must_use]
    pub fn has_fight(&self) -> bool {
        let Some(world) = self.level() else {
            return false;
        };
        self.dragon_fight(&world).is_some()
    }

    /// Returns how many pillar crystals are still standing.
    ///
    /// Vanilla parity: `getDragonFight() == null ? 0 : aliveCrystals()`, the
    /// null-guarded form every caller in vanilla writes.
    #[must_use]
    pub fn alive_crystals(&self) -> i32 {
        let Some(world) = self.level() else {
            return 0;
        };
        self.dragon_fight(&world)
            .map_or(0, EnderDragonFight::alive_crystals)
    }

    /// Vanilla parity: `EnderDragon.yRotA`.
    #[must_use]
    pub fn y_rot_a(&self) -> f32 {
        self.state.lock().y_rot_a
    }

    /// Vanilla parity: assignment to `EnderDragon.yRotA`.
    pub fn set_y_rot_a(&self, y_rot_a: f32) {
        self.state.lock().y_rot_a = y_rot_a;
    }

    /// Returns where the head hitbox is.
    #[must_use]
    pub fn head_position(&self) -> DVec3 {
        self.head().position()
    }

    /// Returns which way the dragon's head is pointing.
    ///
    /// Vanilla parity: `EnderDragon.getHeadLookVector`. Vanilla writes a
    /// temporary pitch onto the entity and reads the view vector back; the
    /// pitch is substituted directly here instead, which avoids a visible
    /// rotation flicker for anything reading the dragon mid-call.
    #[must_use]
    pub fn head_look_vector(&self, _partial_tick: f32) -> DVec3 {
        let (y_rot, x_rot) = self.rotation();
        let phase = self.phase_manager.current_phase();
        let instance = self.phase_manager.current_instance();

        let pitch = match phase {
            EnderDragonPhase::Landing | EnderDragonPhase::Takeoff => {
                let Some(world) = self.level() else {
                    return self.calculate_view_vector(x_rot, y_rot);
                };
                let egg = world.heightmap_pos(
                    HeightmapType::MotionBlockingNoLeaves,
                    end_podium_location(self.fight_origin()),
                );
                let dist =
                    (phases::dist_to_center_sqr(egg, self.position()).sqrt() as f32 / 4.0).max(1.0);
                let y_offset = 6.0 / dist;
                -y_offset * 1.5 * 5.0
            }
            _ if instance.is_sitting() => -45.0,
            _ => return self.calculate_view_vector(x_rot, y_rot),
        };

        self.calculate_view_vector(pitch, y_rot)
    }

    /// Returns the ring node nearest the dragon.
    ///
    /// Vanilla parity: the no-argument `EnderDragon.findClosestNode`.
    pub fn find_closest_node_to_self(&self, world: &Arc<World>) -> usize {
        self.pathfinder.lock().find_closest_node_to_self(
            world,
            self.position(),
            self.alive_crystals(),
        )
    }

    /// Returns the ring node nearest a point.
    ///
    /// Vanilla parity: `EnderDragon.findClosestNode(double, double, double)`.
    pub fn find_closest_node(&self, world: &Arc<World>, x: f64, y: f64, z: f64) -> usize {
        self.pathfinder
            .lock()
            .find_closest_node(world, x, y, z, self.alive_crystals())
    }

    /// A*s between two ring nodes.
    ///
    /// Vanilla parity: `EnderDragon.findPath`.
    pub fn find_path(
        &self,
        world: &Arc<World>,
        start_index: usize,
        end_index: usize,
        final_node: Option<Node>,
    ) -> Option<Path> {
        self.pathfinder.lock().find_path(
            world,
            start_index,
            end_index,
            final_node,
            self.alive_crystals(),
        )
    }

    /// Resolves a hit that arrived on one of the hitboxes.
    ///
    /// Vanilla parity: `EnderDragon.hurt(ServerLevel, EnderDragonPart,
    /// DamageSource, float)`. This is where the parts pay off: anything that is
    /// not the head has its damage cut to a quarter plus a point, which is why
    /// killing a dragon means hitting it in the face.
    pub fn hurt_part(
        &self,
        world: &World,
        part: DragonPartIndex,
        source: &DamageSource,
        damage: f32,
    ) -> bool {
        if self.phase_manager.current_phase() == EnderDragonPhase::Dying {
            return false;
        }

        let mut damage = self
            .phase_manager
            .current_instance()
            .on_hurt(self, source, damage);
        if part != DragonPartIndex::Head {
            damage = damage / 4.0 + damage.min(1.0);
        }

        if damage < MINIMUM_DAMAGE {
            return false;
        }

        let from_player = source
            .causing_entity_id
            .and_then(|id| world.get_entity_by_id(id))
            .is_some_and(|causing| causing.entity_type() == &vanilla_entities::PLAYER);
        if !from_player && !source.is(&DamageTypeTag::ALWAYS_HURTS_ENDER_DRAGONS) {
            // Vanilla parity: `hurt` still reports true here. A hit from
            // something that is neither a player nor tagged is swallowed, but
            // the caller is told it landed.
            return true;
        }

        let health_before = self.get_health();
        self.living_hurt_server(world, source, damage);

        if self.phase_manager.current_instance().is_sitting() {
            let taken = health_before - self.get_health();
            let takeoff = {
                let mut state = self.state.lock();
                state.sitting_damage_received += taken;
                if state.sitting_damage_received
                    > SITTING_ALLOWED_DAMAGE_PERCENTAGE * self.get_max_health()
                {
                    state.sitting_damage_received = 0.0;
                    true
                } else {
                    false
                }
            };
            if takeoff {
                self.phase_manager
                    .set_phase(self, EnderDragonPhase::Takeoff);
            }
        }

        true
    }

    /// Reacts to one of the pillar crystals blowing up.
    ///
    /// Vanilla parity: `EnderDragon.onCrystalDestroyed`. The dragon takes the
    /// blast in the face if it was the crystal it was healing from, then lets
    /// the current phase decide whether to come after whoever did it.
    pub fn on_crystal_destroyed(
        &self,
        world: &Arc<World>,
        crystal: &EndCrystalEntity,
        pos: BlockPos,
        source: &DamageSource,
        blamed: Option<&Arc<Player>>,
    ) {
        let player = blamed.cloned().or_else(|| {
            world.nearest_player(
                DVec3::new(f64::from(pos.x()), f64::from(pos.y()), f64::from(pos.z())),
                CRYSTAL_DESTROY_RANGE,
                LivingEntity::can_be_seen_as_enemy,
            )
        });

        if self.state.lock().nearest_crystal == Some(crystal.id()) {
            let mut explosion = DamageSource::environment(&vanilla_damage_types::EXPLOSION)
                .with_direct_entity(crystal.id());
            if let Some(player) = player.as_ref() {
                explosion = explosion.with_causing_entity(player.id());
            }
            self.hurt_part(
                world,
                DragonPartIndex::Head,
                &explosion,
                CRYSTAL_EXPLOSION_DAMAGE,
            );
        }

        self.phase_manager.current_instance().on_crystal_destroyed(
            self,
            world,
            crystal,
            pos,
            source,
            player.as_ref(),
        );
    }

    /// Heals the dragon from the crystal it is tethered to.
    ///
    /// Vanilla parity: `EnderDragon.checkCrystals`. The beam is the client's
    /// business; the effect is a point of health every ten ticks.
    fn check_crystals(&self, world: &Arc<World>) {
        let nearest = self.state.lock().nearest_crystal;
        if let Some(crystal_id) = nearest {
            let alive = world
                .get_entity_by_id(crystal_id)
                .is_some_and(|crystal| !crystal.is_removed());
            if !alive {
                self.state.lock().nearest_crystal = None;
            } else if self.tick_count() % CRYSTAL_HEAL_INTERVAL == 0
                && self.get_health() < self.get_max_health()
            {
                self.set_health(self.get_health() + 1.0);
            }
        }

        if rand::random_range(0..10) != 0 {
            return;
        }

        let search = self.bounding_box().inflate(CRYSTAL_SCAN_RANGE);
        let position = self.position();
        let mut best: Option<(i32, f64)> = None;
        for entity in world.get_entities_in_aabb(&search) {
            if entity.downcast_ref::<EndCrystalEntity>().is_none() {
                continue;
            }
            let distance = entity.position().distance_squared(position);
            if best.is_none_or(|(_, current)| distance < current) {
                best = Some((entity.id(), distance));
            }
        }

        self.state.lock().nearest_crystal = best.map(|(id, _)| id);
    }

    /// The source the wings and the head hurt with.
    ///
    /// Vanilla parity: `this.damageSources().mobAttack(this)` -- the dragon
    /// itself as both the cause and the direct dealer.
    fn dragon_attack_damage_source(&self) -> DamageSource {
        DamageSource::environment(&vanilla_damage_types::MOB_ATTACK)
            .with_causing_entity(self.id())
            .with_direct_entity(self.id())
            .with_source_position(self.position())
    }

    /// Moves one hitbox to where the body says it should be.
    ///
    /// Vanilla parity: `EnderDragon.tickPart`.
    fn tick_part(&self, index: DragonPartIndex, x: f64, y: f64, z: f64) {
        let position = self.position();
        self.part(index).set_part_position(DVec3::new(
            position.x + x,
            position.y + y,
            position.z + z,
        ));
    }

    /// Returns how far above the body the head rides.
    ///
    /// Vanilla parity: `EnderDragon.getHeadYOffset`.
    fn head_y_offset(&self) -> f32 {
        if self.phase_manager.current_instance().is_sitting() {
            return -1.0;
        }

        let history = self.flight_history.lock();
        (history.get(5).y - history.get(0).y) as f32
    }

    /// Puts all eight hitboxes where this tick's body position implies.
    ///
    /// Vanilla parity: the geometry block in the middle of `EnderDragon.aiStep`.
    /// The neck, head and tail read the flight history rather than the current
    /// position, which is what makes the body flex through a turn.
    fn tick_parts(&self) {
        let (y_rot, _) = self.rotation();
        let (tilt, sample_5) = {
            let history = self.flight_history.lock();
            let tilt = (history.get(5).y - history.get(10).y) as f32 * 10.0 * PI / 180.0;
            (tilt, history.get(5))
        };
        let cc_tilt = trig::cos(f64::from(tilt));
        let ss_tilt = trig::sin(f64::from(tilt));

        let rot1 = f64::from(y_rot).to_radians();
        let ss1 = trig::sin(rot1);
        let cc1 = trig::cos(rot1);
        self.tick_part(
            DragonPartIndex::Body,
            f64::from(ss1) * 0.5,
            0.0,
            f64::from(-cc1) * 0.5,
        );
        self.tick_part(
            DragonPartIndex::Wing1,
            f64::from(cc1) * 4.5,
            2.0,
            f64::from(ss1) * 4.5,
        );
        self.tick_part(
            DragonPartIndex::Wing2,
            f64::from(cc1) * -4.5,
            2.0,
            f64::from(ss1) * -4.5,
        );

        let head_rot = f64::from(y_rot).to_radians() - f64::from(self.y_rot_a()) * 0.01;
        let ss2 = trig::sin(head_rot);
        let cc2 = trig::cos(head_rot);
        let y_offset = self.head_y_offset();
        self.tick_part(
            DragonPartIndex::Head,
            f64::from(ss2 * 6.5 * cc_tilt),
            f64::from(y_offset + ss_tilt * 6.5),
            f64::from(-cc2 * 6.5 * cc_tilt),
        );
        self.tick_part(
            DragonPartIndex::Neck,
            f64::from(ss2 * 5.5 * cc_tilt),
            f64::from(y_offset + ss_tilt * 5.5),
            f64::from(-cc2 * 5.5 * cc_tilt),
        );

        for (segment, index) in [
            DragonPartIndex::Tail1,
            DragonPartIndex::Tail2,
            DragonPartIndex::Tail3,
        ]
        .into_iter()
        .zip(0..3)
        {
            let sample = self.flight_history.lock().get(12 + index * 2);
            let rot = f64::from(y_rot).to_radians()
                + f64::from(wrap_degrees(sample.y_rot - sample_5.y_rot)).to_radians();
            let ss = trig::sin(rot);
            let cc = trig::cos(rot);
            let dd = (index + 1) as f32 * 2.0;
            self.tick_part(
                segment,
                f64::from(-(ss1 * 1.5 + ss * dd) * cc_tilt),
                sample.y - sample_5.y - f64::from((dd + 1.5) * ss_tilt) + 1.5,
                f64::from((cc1 * 1.5 + cc * dd) * cc_tilt),
            );
        }
    }

    /// Shoves and hurts anything the wings sweep through.
    ///
    /// Vanilla parity: `EnderDragon.knockBack`.
    fn knock_back(&self, world: &Arc<World>, wing: DragonPartIndex) {
        let body = self.part(DragonPartIndex::Body).bounding_box();
        let xm = f64::midpoint(body.min_x(), body.max_x());
        let zm = f64::midpoint(body.min_z(), body.max_z());

        let sweep = self
            .part(wing)
            .bounding_box()
            .inflate_xyz(4.0, 2.0, 4.0)
            .translate(DVec3::new(0.0, -2.0, 0.0));
        let sitting = self.phase_manager.current_instance().is_sitting();

        for entity in world.get_entities_in_aabb(&sweep) {
            if entity.id() == self.id() || entity.as_living_entity().is_none() {
                continue;
            }
            if is_creative_or_spectator(entity.as_ref()) {
                continue;
            }

            let position = entity.position();
            let xd = position.x - xm;
            let zd = position.z - zm;
            let dd = xd.mul_add(xd, zd * zd).max(0.1);
            entity.push_impulse(DVec3::new(xd / dd * 4.0, f64::from(0.2_f32), zd / dd * 4.0));

            if sitting {
                continue;
            }
            let Some(living) = entity.as_living_entity() else {
                continue;
            };
            if living.last_hurt_by_mob_timestamp() >= entity.tick_count() - 2 {
                continue;
            }
            entity.hurt(world, &self.dragon_attack_damage_source(), WING_DAMAGE);
        }
    }

    /// Hurts anything the head or neck passes through.
    ///
    /// Vanilla parity: the private `EnderDragon.hurt(ServerLevel, List<Entity>)`.
    fn bite(&self, world: &Arc<World>, part: DragonPartIndex) {
        let box_ = self.part(part).bounding_box().inflate(1.0);
        for entity in world.get_entities_in_aabb(&box_) {
            if entity.id() == self.id() || entity.as_living_entity().is_none() {
                continue;
            }
            if is_creative_or_spectator(entity.as_ref()) {
                continue;
            }
            entity.hurt(world, &self.dragon_attack_damage_source(), BITE_DAMAGE);
        }
    }

    /// Eats the blocks the dragon is flying through.
    ///
    /// Vanilla parity: `EnderDragon.checkWalls`. Returns whether the dragon was
    /// stopped by something it could not take, which is what makes it beat its
    /// wings harder against bedrock.
    fn check_walls(world: &Arc<World>, bb: WorldAabb) -> bool {
        let x0 = bb.min_x().floor() as i32;
        let y0 = bb.min_y().floor() as i32;
        let z0 = bb.min_z().floor() as i32;
        let x1 = bb.max_x().floor() as i32;
        let y1 = bb.max_y().floor() as i32;
        let z1 = bb.max_z().floor() as i32;

        let mob_griefing = world.get_game_rule(&MOB_GRIEFING);
        let mut hit_wall = false;
        let mut destroyed_block = false;

        for x in x0..=x1 {
            for y in y0..=y1 {
                for z in z0..=z1 {
                    let pos = BlockPos::new(x, y, z);
                    let state = world.get_block_state(pos);
                    if state.is_air() || state.get_block().has_tag(&BlockTag::DRAGON_TRANSPARENT) {
                        continue;
                    }
                    if mob_griefing && !state.get_block().has_tag(&BlockTag::DRAGON_IMMUNE) {
                        destroyed_block = world.remove_block(pos, false) || destroyed_block;
                    } else {
                        hit_wall = true;
                    }
                }
            }
        }

        if destroyed_block {
            let pos = BlockPos::new(
                x0 + rand::random_range(0..x1 - x0 + 1),
                y0 + rand::random_range(0..y1 - y0 + 1),
                z0 + rand::random_range(0..z1 - z0 + 1),
            );
            world.level_event(level_events::PARTICLES_DRAGON_BLOCK_BREAK, pos, 0, None);
        }

        hit_wall
    }

    /// Runs the death animation.
    ///
    /// Vanilla parity: `EnderDragon.tickDeath`. The dragon does not drop
    /// experience through the mob path -- it awards the orbs here, in a trickle
    /// over the last two and a half seconds and then a lump at the end.
    fn run_death_animation(&self, world: &Arc<World>) {
        if let Some(fight) = world.dragon_fight() {
            fight.update_dragon(self);
        }

        let death_time = {
            let mut state = self.state.lock();
            state.dragon_death_time += 1;
            state.dragon_death_time
        };

        // Vanilla parity: the fight substitutes twelve thousand for the first
        // dragon of a world. With no fight, every dragon is worth five hundred.
        let xp_count = if self
            .dragon_fight(world)
            .is_some_and(|fight| !fight.has_previously_killed_dragon())
        {
            FIRST_KILL_DEATH_XP
        } else {
            DEATH_XP
        };

        if world.get_game_rule(&MOB_DROPS)
            && death_time > DEATH_XP_START_TICK
            && death_time % DEATH_XP_INTERVAL == 0
        {
            ExperienceOrbEntity::award(
                world,
                self.position(),
                (xp_count as f32 * DEATH_XP_TRICKLE_SHARE).floor() as i32,
            );
        }

        if death_time == 1 && !self.is_silent() {
            world.global_level_event(level_events::SOUND_DRAGON_DEATH, self.block_position(), 0);
        }

        let death_move = DVec3::new(0.0, f64::from(0.1_f32), 0.0);
        self.move_entity(MoverType::SelfMovement, death_move);
        for part in &self.parts {
            part.set_old_position_to_current();
            part.set_part_position(part.position() + death_move);
        }

        if death_time < DEATH_TIME {
            return;
        }

        if world.get_game_rule(&MOB_DROPS) {
            ExperienceOrbEntity::award(
                world,
                self.position(),
                (xp_count as f32 * DEATH_XP_FINAL_SHARE).floor() as i32,
            );
        }

        // Vanilla parity: `tickDeath` tells the fight before it removes itself.
        // This is what opens the exit portal, drops the egg and hands out a
        // gateway, and it is the only caller any of the three have.
        if let Some(fight) = world.dragon_fight() {
            fight.set_dragon_killed(world, self);
        }

        self.set_removed(RemovalReason::Killed);
    }
}

/// Vanilla `EntitySelector.NO_CREATIVE_OR_SPECTATOR`.
fn is_creative_or_spectator(entity: &dyn Entity) -> bool {
    if entity.is_spectator() {
        return true;
    }
    entity
        .downcast_ref::<Player>()
        .is_some_and(Player::has_infinite_materials)
}

impl Entity for EnderDragon {
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

    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    /// Vanilla parity: `EnderDragon.isPickable` returns false -- the dragon's
    /// own box is not a target. Its [hitboxes](EnderDragonPart) are.
    fn is_pickable(&self) -> bool {
        false
    }

    /// Vanilla parity: `EnderDragon.checkDespawn` is empty.
    fn check_despawn(&self) {}

    /// Vanilla parity: `EnderDragon.canRide` returns false.
    fn can_ride(&self, _vehicle: &dyn Entity) -> bool {
        false
    }

    /// Vanilla parity: `EnderDragon.canUsePortal` returns false.
    fn can_use_portal(&self, _ignore_passenger: bool) -> bool {
        false
    }

    /// Vanilla parity: `EnderDragon.kill`, which skips the death animation
    /// entirely and still closes the fight out -- `/kill` on a dragon opens the
    /// exit portal exactly as beating it does.
    fn kill(&self, _world: &World) {
        self.set_removed(RemovalReason::Killed);
        self.game_event(&vanilla_game_events::ENTITY_DIE);

        // The fight needs the owning `Arc`, which the borrowed level cannot
        // hand over, so it is read back off the dragon.
        let Some(world) = self.level() else {
            return;
        };
        if let Some(fight) = world.dragon_fight() {
            fight.update_dragon(self);
            fight.set_dragon_killed(&world, self);
        }
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        let state = self.state.lock();
        nbt.insert("DragonPhase", self.phase_manager.current_phase().id());
        nbt.insert("DragonDeathTime", state.dragon_death_time);
        nbt.insert("sitting_damage_received", state.sitting_damage_received);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        if let Some(phase_id) = nbt.int("DragonPhase") {
            self.phase_manager
                .load_phase(self, EnderDragonPhase::from_id(phase_id));
        }
        let mut state = self.state.lock();
        state.dragon_death_time = nbt.int("DragonDeathTime").unwrap_or(0);
        state.sitting_damage_received = nbt.float("sitting_damage_received").unwrap_or(0.0);
    }
}

impl LivingEntity for EnderDragon {
    /// Returns synchronized data declared by vanilla `LivingEntity`.
    fn living_synced_data(&self) -> Option<&dyn LivingEntitySyncedData> {
        Some(&self.entity_data)
    }

    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    /// Vanilla parity: `EnderDragon.hurtServer` routes everything that arrives
    /// on the dragon itself -- a fall, a potion, a command -- through the body
    /// hitbox, so it takes the same quarter damage a body hit takes.
    fn hurt_server(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        self.hurt_part(world, DragonPartIndex::Body, source, amount)
    }

    /// Vanilla parity: `EnderDragon.handleKillingBlow`, which vanilla calls
    /// from the middle of `die`. The dragon does not die when its health runs
    /// out: it is put back on one point of health and sent to the exit portal,
    /// and only `tickDeath` removes it, three hundred ticks later.
    ///
    /// Foton has no separate killing-blow hook, so the override sits on `die`
    /// ahead of the shared body -- the same order vanilla runs them in.
    fn die(&self, source: &DamageSource) {
        if !self.phase_manager.current_instance().is_sitting() {
            self.set_health(1.0);
            self.phase_manager.set_phase(self, EnderDragonPhase::Dying);
        }
        self.living_die(source);
    }

    /// Vanilla parity: `EnderDragon.knockback` -- a sitting dragon does not move.
    fn knockback(&self, power: f64, xd: f64, zd: f64) {
        if self.phase_manager.current_instance().is_sitting() {
            return;
        }
        self.default_knockback(power, xd, zd);
    }

    /// Vanilla parity: `EnderDragon.addEffect` returns false for everything.
    fn can_be_affected(&self, _effect: &MobEffectInstance) -> bool {
        false
    }

    /// Vanilla parity: `EnderDragon.canAttack`.
    fn can_attack(&self, target: &dyn LivingEntity) -> bool {
        target.can_be_seen_as_enemy()
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
        Some(&sound_events::ENTITY_ENDER_DRAGON_HURT)
    }

    /// Vanilla parity: `EnderDragon.tickDeath`.
    fn tick_death(&self) {
        let Some(world) = self.level() else {
            return;
        };
        self.run_death_animation(&world);
    }

    /// Vanilla parity: the whole of `EnderDragon.aiStep`, minus the client half.
    fn ai_step(&self) -> Option<MoveResult> {
        let world = self.level()?;

        if self.is_dead_or_dying() {
            return None;
        }

        self.check_crystals(&world);

        // Vanilla parity: the flap-time bookkeeping is client-side animation
        // state; only `flightHistory.record` matters to the server, because the
        // hitboxes read it.
        self.set_rotation((wrap_degrees(self.rotation().0), self.rotation().1));
        if self.is_no_ai() {
            return None;
        }

        self.flight_history
            .lock()
            .record(self.position().y, self.rotation().0);

        let mut result = None;
        let current = self.phase_manager.current_instance();
        current.do_server_tick(self, &world);
        let current = if self.phase_manager.current_phase() == current.phase() {
            current
        } else {
            let switched = self.phase_manager.current_instance();
            switched.do_server_tick(self, &world);
            switched
        };

        if let Some(target_location) = current.fly_target_location() {
            result = self.fly_towards(&world, target_location, current.as_ref());
        }

        self.set_y_body_rot(self.rotation().0);
        let old_positions = self.parts.each_ref().map(|part| part.position());
        self.tick_parts();

        // Vanilla parity: `this.hurtTime == 0`. Foton carries only
        // `invulnerableTime`, which is written on the same hit and decrements in
        // the same tick, so ten ticks in is exactly vanilla's `hurtTime` running out.
        if self.living_base().invulnerable_time() <= 10 {
            self.knock_back(&world, DragonPartIndex::Wing1);
            self.knock_back(&world, DragonPartIndex::Wing2);
            self.bite(&world, DragonPartIndex::Head);
            self.bite(&world, DragonPartIndex::Neck);
        }

        let in_wall = Self::check_walls(&world, self.part(DragonPartIndex::Head).bounding_box())
            | Self::check_walls(&world, self.part(DragonPartIndex::Neck).bounding_box())
            | Self::check_walls(&world, self.part(DragonPartIndex::Body).bounding_box());
        self.state.lock().in_wall = in_wall;
        if let Some(fight) = world.dragon_fight() {
            fight.update_dragon(self);
        }

        for (part, old) in self.parts.iter().zip(old_positions) {
            part.set_old_position(old);
        }

        result
    }
}

impl EnderDragon {
    /// Steers and moves the dragon towards this tick's flight target.
    ///
    /// Vanilla parity: the `targetLocation != null` block of `aiStep`.
    fn fly_towards(
        &self,
        world: &Arc<World>,
        target: DVec3,
        phase: &dyn phases::DragonPhaseInstance,
    ) -> Option<MoveResult> {
        let _ = world;
        let position = self.position();
        let xdd = target.x - position.x;
        let ydd = target.y - position.y;
        let zdd = target.z - position.z;
        let dist_to_target = xdd.mul_add(xdd, ydd.mul_add(ydd, zdd * zdd));
        let max = f64::from(phase.fly_speed());
        let horizontal_dist = xdd.hypot(zdd);
        let ydd = if horizontal_dist > 0.0 {
            (ydd / horizontal_dist).clamp(-max, max)
        } else {
            ydd
        };

        self.set_velocity(self.velocity() + DVec3::new(0.0, ydd * 0.01, 0.0));
        self.set_rotation((wrap_degrees(self.rotation().0), self.rotation().1));

        let aim = (target - position).normalize_or_zero();
        let y_rot = f64::from(self.rotation().0).to_radians();
        let dir = DVec3::new(
            f64::from(trig::sin(y_rot)),
            self.velocity().y,
            f64::from(-trig::cos(y_rot)),
        )
        .normalize_or_zero();
        let dot = ((dir.dot(aim) as f32 + 0.5) / 1.5).max(0.0);

        if xdd.abs() > f64::from(1.0e-5_f32) || zdd.abs() > f64::from(1.0e-5_f32) {
            let y_rot_d =
                wrap_degrees(180.0 - xdd.atan2(zdd).to_degrees() as f32 - self.rotation().0)
                    .clamp(-50.0, 50.0);
            let mut y_rot_a = self.y_rot_a() * 0.8;
            y_rot_a += y_rot_d * phase.turn_speed(self);
            self.set_y_rot_a(y_rot_a);
            self.set_rotation((self.rotation().0 + y_rot_a * 0.1, self.rotation().1));
        }

        let span = (2.0 / (dist_to_target + 1.0)) as f32;
        self.move_relative(
            0.06 * dot.mul_add(span, 1.0 - span),
            DVec3::new(0.0, 0.0, -1.0),
        );

        let in_wall = self.state.lock().in_wall;
        let movement = if in_wall {
            self.velocity() * f64::from(0.8_f32)
        } else {
            self.velocity()
        };
        let result = self.move_entity(MoverType::SelfMovement, movement);

        let actual = self.velocity().normalize_or_zero();
        let slide = 0.8 + 0.15 * (actual.dot(dir) + 1.0) / 2.0;
        self.set_velocity(self.velocity() * DVec3::new(slide, f64::from(0.91_f32), slide));

        result
    }
}

impl Mob for EnderDragon {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }

    /// Vanilla parity: the dragon has no goals at all -- the phases are its AI.
    fn tick_goal_selectors(&self) {}

    /// Vanilla parity: the dragon has no navigation; it flies the node ring.
    fn tick_path_navigation(&self) {}

    fn move_control_kind(&self) -> MoveControlKind {
        MoveControlKind::Ground
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ENDER_DRAGON_AMBIENT)
    }
}

impl PathfinderMob for EnderDragon {
    fn navigation_kind(&self) -> NavigationKind {
        NavigationKind::Flying
    }
}

impl Enemy for EnderDragon {}
