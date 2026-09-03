//! The fishing bobber (`FishingHook`).
//!
//! Vanilla parity: `net.minecraft.world.entity.projectile.FishingHook`. The hook
//! is a `Projectile` that runs its own move loop rather than the throwable one:
//! it flies until it meets water, a block or an entity, then either sticks to
//! what it hit or floats and starts fishing.
//!
//! Three states, mirroring vanilla's private `FishHookState`:
//! [`FishHookState::Flying`] raycasts its move vector, [`FishHookState::Bobbing`]
//! floats on the fluid surface and runs [`FishingHookEntity::catching_fish`], and
//! [`FishHookState::HookedInEntity`] rides whatever the hook caught.
//!
//! The bite timer has two stages: `time_until_lured` counts down to the moment a
//! fish starts swimming in (drawing the trail of `fishing` particles), then
//! `time_until_hooked` counts down to the splash that sets `biting`. Only while
//! `nibble` is running does reeling in roll `gameplay/fishing`.
//!
//! Whether that roll may produce treasure is decided by
//! [`FishingHookEntity::calculate_open_water`], which is read back out through
//! the `minecraft:type_specific/fishing_hook` loot predicate.

use std::f32::consts::PI;
use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::item_stack::ItemStack;
use foton_registry::loot_table::LootContext;
use foton_registry::particle_type::ParticleData;
use foton_registry::vanilla_entity_data::FishingBobberEntityData;
use foton_registry::{
    sound_events, vanilla_blocks, vanilla_entities, vanilla_items, vanilla_loot_tables,
    vanilla_particle_types,
};
use foton_utils::entity_events::EntityStatus;
use foton_utils::locks::SyncMutex;
use foton_utils::random::Random as _;
use foton_utils::random::legacy_random::LegacyRandom;
use foton_utils::types::InteractionHand;
use foton_utils::{BlockPos, Downcast as _, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::entity::entities::{ExperienceOrbEntity, ItemEntity};
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntityMovementEmission, EntitySyncedData,
    LivingEntity as _, MoverType, Projectile, ProjectileBase, RemovalReason, SharedEntity,
    entity_loot_ref, next_entity_id,
};
use crate::event::PlayerFishEvent;
use crate::fluid::is_water_fluid;
use crate::fluid::state::{get_fluid_state, get_height};
use crate::player::Player;
use crate::world::{ClipHitResult, World};

/// Vanilla `FishingHook.MAX_OUT_OF_WATER_TIME`.
const MAX_OUT_OF_WATER_TIME: i32 = 10;

/// Ticks a grounded hook survives before it gives up (vanilla `life >= 1200`).
const MAX_GROUNDED_LIFE: i32 = 1200;

/// Vanilla's leash length: past this the hook is reeled in for free.
const MAX_DISTANCE_TO_OWNER_SQR: f64 = 1024.0;

/// Per-tick inertia applied to the hook (vanilla's `0.92` scale).
const INERTIA: f64 = 0.92;

/// Gravity applied while airborne and unattached (vanilla `-0.03`).
const AIR_GRAVITY: f64 = -0.03;

/// Durability the rod loses when the hook is reeled in with nothing on it.
const DAMAGE_NOTHING: i32 = 0;
/// Durability lost reeling in a caught fish.
const DAMAGE_CAUGHT_FISH: i32 = 1;
/// Durability lost reeling a hook back off the ground.
const DAMAGE_FROM_GROUND: i32 = 2;
/// Durability lost dragging a hooked item entity in.
const DAMAGE_HOOKED_ITEM: i32 = 3;
/// Durability lost dragging any other hooked entity in.
const DAMAGE_HOOKED_ENTITY: i32 = 5;

/// Vanilla `FishingHook.FishHookState`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FishHookState {
    /// Still in the air, raycasting its move vector.
    Flying,
    /// Riding an entity it caught.
    HookedInEntity,
    /// Floating on water, running the bite timers.
    Bobbing,
}

/// Vanilla `FishingHook.OpenWaterType`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OpenWaterType {
    /// Air, or a lily pad lying on the surface.
    AboveWater,
    /// A full water source with no collider in it.
    InsideWater,
    /// Anything else -- which fails the whole check.
    Invalid,
}

/// Mutable fishing state (vanilla's non-synced `FishingHook` fields).
struct HookState {
    /// Mirror of the synced `biting` flag, kept for the tick's own reads.
    biting: bool,
    out_of_water_time: i32,
    life: i32,
    nibble: i32,
    time_until_lured: i32,
    time_until_hooked: i32,
    fish_angle: f32,
    open_water: bool,
    hooked_in: Option<Weak<dyn Entity>>,
    current_state: FishHookState,
    /// Luck of the Sea bonus. Vanilla makes this final by passing it to the
    /// constructor; the generated entity factory has no room for extra
    /// arguments, so the rod sets it through [`FishingHookEntity::cast_from`].
    luck: i32,
    /// Lure bonus, in ticks shaved off `time_until_lured`. Final in vanilla for
    /// the same reason as `luck`.
    lure_speed: i32,
}

impl HookState {
    const fn new() -> Self {
        Self {
            biting: false,
            out_of_water_time: 0,
            life: 0,
            nibble: 0,
            time_until_lured: 0,
            time_until_hooked: 0,
            fish_angle: 0.0,
            open_water: true,
            hooked_in: None,
            current_state: FishHookState::Flying,
            luck: 0,
            lure_speed: 0,
        }
    }
}

/// A cast fishing bobber.
#[entity_behavior(class = "FishingHook")]
pub struct FishingHookEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<FishingBobberEntityData>,
    projectile_base: ProjectileBase,
    state: SyncMutex<HookState>,
    /// Vanilla's client-and-server shared `RandomSource` field on `FishingHook`
    /// (spelled with a typo upstream, so it is not quoted here). It is re-seeded
    /// every tick from the UUID and the game time, which is how the client draws
    /// the same bob the server simulates without a packet.
    synchronized_random: SyncMutex<LegacyRandom>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `FishingHookEntity`.
unsafe impl DowncastType for FishingHookEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/fishing_hook");
}

/// Mirrors vanilla `RandomSource.triangle(mode, deviation)`.
fn triangle(mode: f64, deviation: f64) -> f64 {
    mode + deviation * (rand::random::<f64>() - rand::random::<f64>())
}

/// Mirrors vanilla `Mth.nextInt(random, min, max)`, which is inclusive.
fn next_int(min: i32, max: i32) -> i32 {
    min + rand::random_range(0..=(max - min))
}

/// Mirrors vanilla `Mth.nextFloat(random, min, max)`.
fn next_float(min: f32, max: f32) -> f32 {
    min + rand::random::<f32>() * (max - min)
}

/// `java.lang.Math.signum`, which returns zero for zero.
///
/// Rust's `f64::signum` returns `1.0` for `+0.0`, which would nudge a bobber
/// that happens to sit exactly on the surface.
fn java_signum(value: f64) -> f64 {
    if value == 0.0 || value.is_nan() {
        value
    } else {
        value.signum()
    }
}

impl FishingHookEntity {
    /// Creates an unowned hook with no enchantment bonuses.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(FishingBobberEntityData::new()),
            projectile_base: ProjectileBase::new(),
            state: SyncMutex::new(HookState::new()),
            synchronized_random: SyncMutex::new(LegacyRandom::from_seed(0)),
        }
    }

    /// Creates a hook from saved base data.
    ///
    /// Vanilla marks `fishing_bobber` as not serializable, so this only exists
    /// because the generated entity factory registers a loader for every type.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(FishingBobberEntityData::new()),
            projectile_base: ProjectileBase::new(),
            state: SyncMutex::new(HookState::new()),
            synchronized_random: SyncMutex::new(LegacyRandom::from_seed(0)),
        }
    }

    /// Places and launches a freshly cast hook.
    ///
    /// Vanilla parity: the `FishingHook(Player, Level, int, int)` constructor,
    /// which offsets the spawn behind the caster's eye and throws the bobber
    /// along their look direction with a triangular speed jitter.
    pub fn cast_from(&self, player: &Player, luck: i32, lure_speed: i32) {
        {
            let mut state = self.state.lock();
            state.luck = luck.max(0);
            state.lure_speed = lure_speed.max(0);
        }

        let (y_rot, x_rot) = player.rotation();
        let y_cos = (-y_rot.to_radians() - PI).cos();
        let y_sin = (-y_rot.to_radians() - PI).sin();
        let x_cos = -(-x_rot.to_radians()).cos();
        let x_sin = (-x_rot.to_radians()).sin();

        let player_pos = player.position();
        let position = DVec3::new(
            player_pos.x - f64::from(y_sin) * 0.3,
            player.get_eye_y(),
            player_pos.z - f64::from(y_cos) * 0.3,
        );
        self.base().set_position_local(position);
        self.set_old_position_to_current();
        self.set_rotation((y_rot, x_rot));
        self.base().set_old_rotation_to_current();

        let mut movement = DVec3::new(
            f64::from(-y_sin),
            f64::from((-(x_sin / x_cos)).clamp(-5.0, 5.0)),
            f64::from(-y_cos),
        );
        let distance = movement.length();
        movement *= DVec3::new(
            0.6 / distance + triangle(0.5, 0.010_336_5),
            0.6 / distance + triangle(0.5, 0.010_336_5),
            0.6 / distance + triangle(0.5, 0.010_336_5),
        );
        self.set_velocity(movement);
        self.mark_velocity_sync();

        let yaw = movement.x.atan2(movement.z).to_degrees() as f32;
        let horizontal = movement.x.hypot(movement.z);
        let pitch = movement.y.atan2(horizontal).to_degrees() as f32;
        self.set_rotation((yaw, pitch));
        self.base().set_old_rotation_to_current();
    }

    /// Returns vanilla `FishingHook.isOpenWaterFishing`.
    #[must_use]
    pub fn is_open_water_fishing(&self) -> bool {
        self.state.lock().open_water
    }

    /// Returns the owner when it is a player (vanilla `getPlayerOwner`).
    fn player_owner(&self) -> Option<SharedEntity> {
        let owner = self.get_owner()?;
        owner.as_player()?;
        Some(owner)
    }

    /// Returns the live entity this hook has caught, if it is still there.
    fn hooked_in(&self) -> Option<SharedEntity> {
        self.state.lock().hooked_in.as_ref().and_then(Weak::upgrade)
    }

    /// Vanilla `FishingHook.setHookedEntity`.
    fn set_hooked_entity(&self, hooked: Option<&SharedEntity>) {
        self.state.lock().hooked_in = hooked.map(Arc::downgrade);
        self.entity_data
            .lock()
            .fishing_hook
            .hooked_entity
            .set(hooked.map_or(0, |entity| entity.id() + 1));
    }

    /// Sets the synced `biting` flag.
    ///
    /// Vanilla's `SynchedEntityData.set` runs `onSyncedDataUpdated` on the
    /// server too, which is where the downward tug on the bobber comes from --
    /// so it happens here rather than only on the client.
    fn set_biting(&self, biting: bool) {
        {
            let mut state = self.state.lock();
            if state.biting == biting {
                return;
            }
            state.biting = biting;
        }
        self.entity_data.lock().fishing_hook.biting.set(biting);

        if !biting {
            return;
        }
        let tug = f64::from(
            -0.4 * {
                let mut random = self.synchronized_random.lock();
                0.6 + random.next_f32() * 0.4
            },
        );
        let movement = self.velocity();
        self.set_velocity(DVec3::new(movement.x, tug, movement.z));
    }

    /// Vanilla `FishingHook.shouldStopFishing`: the hook is discarded unless the
    /// owner is still holding a rod and still within the leash.
    fn should_stop_fishing(&self, owner: &SharedEntity, player: &Player) -> bool {
        if owner.can_interact_with_level() {
            let holds_rod = {
                let inventory = player.inventory.lock();
                inventory
                    .get_item_in_hand(InteractionHand::MainHand)
                    .is(&vanilla_items::FISHING_ROD)
                    || inventory
                        .get_item_in_hand(InteractionHand::OffHand)
                        .is(&vanilla_items::FISHING_ROD)
            };
            if holds_rod
                && self.position().distance_squared(owner.position()) <= MAX_DISTANCE_TO_OWNER_SQR
            {
                return false;
            }
        }

        self.set_removed(RemovalReason::Discarded);
        true
    }

    /// Vanilla `FishingHook.checkCollision`.
    fn check_collision(&self) {
        let Some(hit) = self.get_hit_result_on_move_vector() else {
            return;
        };
        self.hit_target_or_deflect_self(&hit);
    }

    /// Vanilla `FishingHook.catchingFish`.
    ///
    /// Runs the two-stage bite timer and puts the fish trail, the tease splashes
    /// and the bite splash on screen.
    fn catching_fish(&self, world: &Arc<World>, block_pos: BlockPos) {
        let mut fishing_speed = 1;
        let above = block_pos.offset(0, 1, 0);
        if rand::random::<f32>() < 0.25 && world.is_raining_at(above) {
            fishing_speed += 1;
        }
        if rand::random::<f32>() < 0.5 && !world.can_see_sky(above) {
            fishing_speed -= 1;
        }

        let (nibble, time_until_hooked, time_until_lured) = {
            let state = self.state.lock();
            (
                state.nibble,
                state.time_until_hooked,
                state.time_until_lured,
            )
        };

        if nibble > 0 {
            let mut state = self.state.lock();
            state.nibble -= 1;
            let expired = state.nibble <= 0;
            if expired {
                state.time_until_lured = 0;
                state.time_until_hooked = 0;
            }
            drop(state);
            if expired {
                self.set_biting(false);
            }
            return;
        }

        if time_until_hooked > 0 {
            self.tick_time_until_hooked(world, fishing_speed);
            return;
        }

        if time_until_lured > 0 {
            self.tick_time_until_lured(world, fishing_speed);
            return;
        }

        let mut state = self.state.lock();
        state.time_until_lured = next_int(100, 600) - state.lure_speed;
    }

    /// The second stage of the bite timer: a fish is swimming in.
    fn tick_time_until_hooked(&self, world: &Arc<World>, fishing_speed: i32) {
        let (remaining, angle) = {
            let mut state = self.state.lock();
            state.time_until_hooked -= fishing_speed;
            if state.time_until_hooked > 0 {
                state.fish_angle += triangle(0.0, 9.188) as f32;
            }
            (state.time_until_hooked, state.fish_angle)
        };

        if remaining <= 0 {
            self.hook_the_fish(world);
            return;
        }

        let angle = angle.to_radians();
        let angle_sin = angle.sin();
        let angle_cos = angle.cos();
        let position = self.position();
        let fish_x = position.x + f64::from(angle_sin * remaining as f32) * 0.1;
        let fish_y = position.y.floor() + 1.0;
        let fish_z = position.z + f64::from(angle_cos * remaining as f32) * 0.1;
        if !Self::is_water_block(world, fish_x, fish_y - 1.0, fish_z) {
            return;
        }

        if rand::random::<f32>() < 0.15 {
            world.send_particles(
                ParticleData::simple(&vanilla_particle_types::BUBBLE),
                DVec3::new(fish_x, fish_y - 0.1, fish_z),
                1,
                DVec3::new(f64::from(angle_sin), 0.1, f64::from(angle_cos)),
                0.0,
            );
        }

        // Vanilla sends the trail twice, the second time mirrored, so the wake
        // spreads to both sides of the swimming fish.
        let drift = DVec3::new(
            f64::from(angle_cos * 0.04),
            0.01,
            f64::from(-angle_sin * 0.04),
        );
        let trail = DVec3::new(fish_x, fish_y, fish_z);
        world.send_particles(
            ParticleData::simple(&vanilla_particle_types::FISHING),
            trail,
            0,
            drift,
            1.0,
        );
        world.send_particles(
            ParticleData::simple(&vanilla_particle_types::FISHING),
            trail,
            0,
            DVec3::new(-drift.x, drift.y, -drift.z),
            1.0,
        );
    }

    /// The moment the fish takes the hook: splash, bubbles, and `biting`.
    fn hook_the_fish(&self, world: &Arc<World>) {
        let pitch = 1.0 + (rand::random::<f32>() - rand::random::<f32>()) * 0.4;
        self.play_sound(&sound_events::ENTITY_FISHING_BOBBER_SPLASH, 0.25, pitch);

        let position = self.position();
        let splash = DVec3::new(position.x, position.y + 0.5, position.z);
        let width = f64::from(self.base().dimensions().width);
        let count = (1.0 + width * 20.0) as i32;
        let spread = DVec3::new(width, 0.0, width);
        world.send_particles(
            ParticleData::simple(&vanilla_particle_types::BUBBLE),
            splash,
            count,
            spread,
            0.2,
        );
        world.send_particles(
            ParticleData::simple(&vanilla_particle_types::FISHING),
            splash,
            count,
            spread,
            0.2,
        );

        self.state.lock().nibble = next_int(20, 40);
        self.set_biting(true);
    }

    /// The first stage of the bite timer: teasing splashes, then a fish is sent.
    fn tick_time_until_lured(&self, world: &Arc<World>, fishing_speed: i32) {
        let remaining = {
            let mut state = self.state.lock();
            state.time_until_lured -= fishing_speed;
            state.time_until_lured
        };

        let mut tease_chance = 0.15_f32;
        if remaining < 20 {
            tease_chance += (20 - remaining) as f32 * 0.05;
        } else if remaining < 40 {
            tease_chance += (40 - remaining) as f32 * 0.02;
        } else if remaining < 60 {
            tease_chance += (60 - remaining) as f32 * 0.01;
        }

        if rand::random::<f32>() < tease_chance {
            let angle = next_float(0.0, 360.0).to_radians();
            let distance = next_float(25.0, 60.0);
            let position = self.position();
            let fish_x = position.x + f64::from(angle.sin() * distance) * 0.1;
            let fish_y = position.y.floor() + 1.0;
            let fish_z = position.z + f64::from(angle.cos() * distance) * 0.1;
            if Self::is_water_block(world, fish_x, fish_y - 1.0, fish_z) {
                world.send_particles(
                    ParticleData::simple(&vanilla_particle_types::SPLASH),
                    DVec3::new(fish_x, fish_y, fish_z),
                    2 + rand::random_range(0..2),
                    DVec3::new(0.1, 0.0, 0.1),
                    0.0,
                );
            }
        }

        if remaining > 0 {
            return;
        }
        let mut state = self.state.lock();
        state.fish_angle = next_float(0.0, 360.0);
        state.time_until_hooked = next_int(20, 80);
    }

    /// Whether the block containing this point is plain water.
    ///
    /// Vanilla checks `state.is(Blocks.WATER)`, the block rather than the fluid,
    /// so a waterlogged slab does not carry the fish trail.
    fn is_water_block(world: &Arc<World>, x: f64, y: f64, z: f64) -> bool {
        let pos = BlockPos::new(x.floor() as i32, y.floor() as i32, z.floor() as i32);
        world.get_block_state(pos).get_block() == &vanilla_blocks::WATER
    }

    /// Vanilla `FishingHook.calculateOpenWater`.
    ///
    /// Scans four 5x5 layers from one below the bobber to two above. Each layer
    /// has to be uniformly water or uniformly air, and air is never allowed
    /// below water, which is what rules out fishing under an overhang or from
    /// inside a one-block hole.
    fn calculate_open_water(world: &Arc<World>, block_pos: BlockPos) -> bool {
        let mut previous_layer = OpenWaterType::Invalid;

        for y in -1..=2 {
            let layer = Self::open_water_type_for_area(
                world,
                block_pos.offset(-2, y, -2),
                block_pos.offset(2, y, 2),
            );
            match layer {
                OpenWaterType::AboveWater => {
                    if previous_layer == OpenWaterType::Invalid {
                        return false;
                    }
                }
                OpenWaterType::InsideWater => {
                    if previous_layer == OpenWaterType::AboveWater {
                        return false;
                    }
                }
                OpenWaterType::Invalid => return false,
            }
            previous_layer = layer;
        }

        true
    }

    /// Vanilla `FishingHook.getOpenWaterTypeForArea`: one kind for the whole box,
    /// or `Invalid` when the box is mixed.
    fn open_water_type_for_area(world: &Arc<World>, from: BlockPos, to: BlockPos) -> OpenWaterType {
        let mut result: Option<OpenWaterType> = None;
        for x in from.x()..=to.x() {
            for y in from.y()..=to.y() {
                for z in from.z()..=to.z() {
                    let kind = Self::open_water_type_for_block(world, BlockPos::new(x, y, z));
                    result = Some(match result {
                        None => kind,
                        Some(previous) if previous == kind => kind,
                        Some(_) => return OpenWaterType::Invalid,
                    });
                }
            }
        }
        result.unwrap_or(OpenWaterType::Invalid)
    }

    /// Vanilla `FishingHook.getOpenWaterTypeForBlock`.
    fn open_water_type_for_block(world: &Arc<World>, pos: BlockPos) -> OpenWaterType {
        let state = world.get_block_state(pos);
        if state.is_air() || state.get_block() == &vanilla_blocks::LILY_PAD {
            return OpenWaterType::AboveWater;
        }

        let fluid_state = state.get_fluid_state();
        if is_water_fluid(fluid_state.fluid_id)
            && fluid_state.is_source()
            && state.get_static_collision_shape().is_empty()
        {
            OpenWaterType::InsideWater
        } else {
            OpenWaterType::Invalid
        }
    }

    /// Vanilla `FishingHook.retrieve`: returns the durability the rod loses.
    pub fn retrieve(&self, rod: &ItemStack) -> i32 {
        let Some(world) = self.level() else {
            return 0;
        };
        let Some(owner) = self.player_owner() else {
            return 0;
        };
        let Some(player) = owner.as_player() else {
            return 0;
        };
        if self.should_stop_fishing(&owner, player) {
            return 0;
        }

        let mut damage = DAMAGE_NOTHING;
        if let Some(hooked) = self.hooked_in() {
            self.pull_entity(&hooked);
            // TODO: trigger the `fishing_rod_hooked` advancement criterion once
            // Foton has an advancement system.
            self.broadcast_entity_event(EntityStatus::FishingRodReelIn);
            damage = if hooked.as_ref().downcast_ref::<ItemEntity>().is_some() {
                DAMAGE_HOOKED_ITEM
            } else {
                DAMAGE_HOOKED_ENTITY
            };
        } else if self.state.lock().nibble > 0 {
            let mut event = PlayerFishEvent::new(player.gameprofile.id, self.uuid(), "CAUGHT_FISH");
            world.fire_event(&mut event);
            if !event.is_cancelled() {
                self.drop_catch(&world, &owner, player, rod);
                damage = DAMAGE_CAUGHT_FISH;
            }
        }

        if self.on_ground() {
            damage = DAMAGE_FROM_GROUND;
        }

        self.set_removed(RemovalReason::Discarded);
        damage
    }

    /// Rolls `gameplay/fishing` and throws what it produced at the caster.
    fn drop_catch(
        &self,
        world: &Arc<World>,
        owner: &SharedEntity,
        player: &Player,
        rod: &ItemStack,
    ) {
        let position = self.position();
        let mut rng = rand::rng();
        let luck = self.state.lock().luck as f32 + player.get_luck();
        let mut context = LootContext::new(&mut rng)
            .with_world(&**world)
            .with_origin(position.x, position.y, position.z)
            .with_game_time(world.game_time())
            .with_tool(rod)
            .with_this_entity(entity_loot_ref(self))
            .with_luck(luck);
        let items = vanilla_loot_tables::GAMEPLAY_FISHING.get_random_items(&mut context);

        let owner_position = owner.position();
        for item in items {
            let delta = owner_position - position;
            let velocity = DVec3::new(
                delta.x * 0.1,
                delta.y * 0.1 + delta.length().sqrt() * 0.08,
                delta.z * 0.1,
            );
            // TODO: award the FISH_CAUGHT stat for `#minecraft:fishes` once a
            // stats system exists.
            let entity: SharedEntity = Arc::new(ItemEntity::with_item_and_velocity(
                &vanilla_entities::ITEM,
                next_entity_id(),
                position,
                item,
                velocity,
                Arc::downgrade(world),
            ));
            if let Err(error) = world.try_add_entity(entity) {
                log::debug!("failed to spawn a fishing catch: {error}");
                continue;
            }

            let orb: SharedEntity = Arc::new(ExperienceOrbEntity::with_value(
                &vanilla_entities::EXPERIENCE_ORB,
                next_entity_id(),
                DVec3::new(
                    owner_position.x,
                    owner_position.y + 0.5,
                    owner_position.z + 0.5,
                ),
                rand::random_range(0..6) + 1,
                Arc::downgrade(world),
            ));
            if let Err(error) = world.try_add_entity(orb) {
                log::debug!("failed to spawn fishing experience: {error}");
            }
        }
    }

    /// Vanilla `FishingHook.pullEntity`: a tenth of the way home, per reel.
    pub fn pull_entity(&self, entity: &SharedEntity) {
        let Some(owner) = self.get_owner() else {
            return;
        };
        let delta = (owner.position() - self.position()) * 0.1;
        entity.set_velocity(entity.velocity() + delta);
        entity.mark_velocity_sync();
    }

    /// Points the owning player's `fishing` slot at this hook, or clears it.
    ///
    /// Vanilla `FishingHook.updateOwnerInfo`.
    fn update_owner_info(&self, hook: Option<&SharedEntity>) {
        let Some(owner) = self.player_owner() else {
            return;
        };
        let Some(player) = owner.as_player() else {
            return;
        };
        player.set_fishing_hook(hook);
    }

    /// The `FLYING` branch of the tick. Returns true when the tick is over.
    fn tick_flying(&self, in_water: bool) -> bool {
        if self.hooked_in().is_some() {
            self.set_velocity(DVec3::ZERO);
            self.state.lock().current_state = FishHookState::HookedInEntity;
            return true;
        }

        if in_water {
            self.set_velocity(self.velocity() * DVec3::new(0.3, 0.2, 0.3));
            self.state.lock().current_state = FishHookState::Bobbing;
            return true;
        }

        self.check_collision();
        false
    }

    /// The `HOOKED_IN_ENTITY` branch: ride the catch, or let it go.
    fn tick_hooked_in_entity(&self, world: &Arc<World>) {
        let Some(hooked) = self.hooked_in() else {
            return;
        };

        let same_world = hooked
            .level()
            .is_some_and(|hooked_world| Arc::ptr_eq(&hooked_world, world));
        if hooked.is_removed() || !hooked.can_interact_with_level() || !same_world {
            self.set_hooked_entity(None);
            self.state.lock().current_state = FishHookState::Flying;
            return;
        }

        let position = hooked.position();
        let height = f64::from(hooked.base().dimensions().height);
        if let Err(error) = self.try_set_position(DVec3::new(
            position.x,
            position.y + height * 0.8,
            position.z,
        )) {
            log::debug!("failed to move a fishing hook onto its catch: {error}");
        }
    }

    /// The `BOBBING` branch: float on the surface and run the bite timers.
    fn tick_bobbing(
        &self,
        world: &Arc<World>,
        block_pos: BlockPos,
        liquid_height: f32,
        in_water: bool,
    ) {
        let movement = self.velocity();
        let mut force =
            self.position().y + movement.y - f64::from(block_pos.y()) - f64::from(liquid_height);
        if force.abs() < 0.01 {
            force += java_signum(force) * 0.1;
        }
        self.set_velocity(DVec3::new(
            movement.x * 0.9,
            movement.y - force * f64::from(rand::random::<f32>()) * 0.2,
            movement.z * 0.9,
        ));

        {
            let (nibble, time_until_hooked, out_of_water_time, open_water) = {
                let state = self.state.lock();
                (
                    state.nibble,
                    state.time_until_hooked,
                    state.out_of_water_time,
                    state.open_water,
                )
            };
            let next_open_water = if nibble <= 0 && time_until_hooked <= 0 {
                true
            } else {
                open_water
                    && out_of_water_time < MAX_OUT_OF_WATER_TIME
                    && Self::calculate_open_water(world, block_pos)
            };
            self.state.lock().open_water = next_open_water;
        }

        if !in_water {
            let mut state = self.state.lock();
            state.out_of_water_time = (state.out_of_water_time + 1).min(MAX_OUT_OF_WATER_TIME);
            return;
        }

        let biting = {
            let mut state = self.state.lock();
            state.out_of_water_time = (state.out_of_water_time - 1).max(0);
            state.biting
        };
        if biting {
            let bob = {
                let mut random = self.synchronized_random.lock();
                -0.1 * f64::from(random.next_f32()) * f64::from(random.next_f32())
            };
            self.set_velocity(self.velocity() + DVec3::new(0.0, bob, 0.0));
        }

        self.catching_fish(world, block_pos);
    }
}

impl Entity for FishingHookEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn tick(&self) {
        let Some(world) = self.level() else {
            return;
        };

        // Vanilla re-seeds the shared random from the UUID and the game time so
        // client and server agree on the bob without sending anything.
        let (_, least_significant) = self.uuid().as_u64_pair();
        self.synchronized_random
            .lock()
            .set_seed(least_significant as i64 ^ world.game_time());

        // VANILLA CLIENT-LOCAL: `getInterpolation().interpolate()` smooths the
        // rendered position between the server's position packets.
        self.projectile_base_tick();

        let Some(owner) = self.player_owner() else {
            self.set_removed(RemovalReason::Discarded);
            return;
        };
        let Some(player) = owner.as_player() else {
            self.set_removed(RemovalReason::Discarded);
            return;
        };
        if self.should_stop_fishing(&owner, player) {
            return;
        }

        if self.on_ground() {
            let mut state = self.state.lock();
            state.life += 1;
            let expired = state.life >= MAX_GROUNDED_LIFE;
            drop(state);
            if expired {
                self.set_removed(RemovalReason::Discarded);
                return;
            }
        } else {
            self.state.lock().life = 0;
        }

        let block_pos = self.block_position();
        let fluid_state = get_fluid_state(&world, block_pos);
        let fluid_is_water = is_water_fluid(fluid_state.fluid_id);
        let liquid_height = if fluid_is_water {
            get_height(&world, block_pos, fluid_state)
        } else {
            0.0
        };
        let in_water = liquid_height > 0.0;

        let current_state = self.state.lock().current_state;
        match current_state {
            FishHookState::Flying => {
                if self.tick_flying(in_water) {
                    return;
                }
            }
            FishHookState::HookedInEntity => {
                self.tick_hooked_in_entity(&world);
                return;
            }
            FishHookState::Bobbing => {
                self.tick_bobbing(&world, block_pos, liquid_height, in_water);
            }
        }

        if !fluid_is_water && !self.on_ground() && self.hooked_in().is_none() {
            self.set_velocity(self.velocity() + DVec3::new(0.0, AIR_GRAVITY, 0.0));
        }

        self.move_entity(MoverType::SelfMovement, self.velocity());
        self.apply_effects_from_blocks();
        self.update_rotation();
        if self.state.lock().current_state == FishHookState::Flying
            && (self.on_ground() || self.horizontal_collision())
        {
            self.set_velocity(DVec3::ZERO);
        }

        self.set_velocity(self.velocity() * INERTIA);
        // Vanilla ends with `reapplyPosition()`, which only refreshes the
        // bounding box from the position. Foton keeps the two in step inside
        // `try_set_position`, so there is nothing left to reapply.
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Neutral
    }

    /// Vanilla `FishingHook.getAddEntityPacket` puts the owner id in the spawn
    /// packet's data field; the client needs it to draw the line.
    fn spawn_data(&self) -> i32 {
        self.get_owner()
            .map_or_else(|| self.id(), |owner| owner.id())
    }

    fn restore_owner_reference(&self, owner: &SharedEntity) {
        self.cache_owner_entity(owner);
    }

    fn projectile_owner_uuid(&self) -> Option<uuid::Uuid> {
        self.owner_uuid()
    }

    fn projectile_owner(&self) -> Option<SharedEntity> {
        self.get_owner()
    }

    fn movement_emission(&self) -> EntityMovementEmission {
        EntityMovementEmission::None
    }

    fn fishing_hook_loot_open_water(&self) -> Option<bool> {
        Some(self.is_open_water_fishing())
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla `FishingHook.remove` clears the caster's `fishing` slot first.
    fn set_removed(&self, reason: RemovalReason) {
        self.update_owner_info(None);
        self.base().set_removed(reason);
    }

    /// Vanilla `FishingHook.addAdditionalSaveData` is empty, and
    /// `fishing_bobber` is not serializable at all.
    fn save_additional(&self, _nbt: &mut NbtCompound) {}

    fn load_additional(&self, _nbt: BorrowedNbtCompoundView<'_, '_>) {}
}

impl Projectile for FishingHookEntity {
    fn projectile_base(&self) -> &ProjectileBase {
        &self.projectile_base
    }

    fn should_bounce_on_world_border(&self) -> bool {
        true
    }

    /// Vanilla `FishingHook.canHitEntity`: the base test, plus item entities,
    /// which are not pickable and would otherwise never be caught.
    fn can_hit_entity(&self, entity: &dyn Entity) -> bool {
        self.projectile_can_hit_entity(entity)
            || entity.is_alive() && entity.downcast_ref::<ItemEntity>().is_some()
    }

    fn on_hit_entity(&self, entity: &SharedEntity, _location: DVec3) {
        self.set_hooked_entity(Some(entity));
    }

    fn on_hit_block(&self, hit: &ClipHitResult) {
        self.projectile_on_hit_block(hit);
        let distance = self.position().distance(hit.location);
        self.set_velocity(self.velocity().normalize_or_zero() * distance);
    }
}

#[cfg(test)]
mod tests;
