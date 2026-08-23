//! Area effect cloud.
//!
//! Vanilla parity: `AreaEffectCloud`. The lingering half of alchemy: a patch of
//! ground that keeps working after the bottle is gone. What makes it a distinct
//! thing rather than a slow splash is that it charges for use -- every entity it
//! catches shrinks it -- so one cloud can serve a crowd once or one target
//! several times, but not both.

use std::sync::{Arc, Weak};

use glam::DVec3;
use rustc_hash::FxHashMap;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::vanilla_entity_data::AreaEffectCloudEntityData;
use steel_utils::locks::SyncMutex;
use steel_utils::{DowncastType, DowncastTypeKey, WorldAabb};

use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, MobEffectInstance, RemovalReason,
};
use crate::world::World;
use steel_registry::entity_data::EntityPose;
use steel_registry::entity_type::EntityDimensions;
use steel_registry::mob_effect::MobEffectRef;

/// Ticks between two sweeps for victims.
///
/// Vanilla parity: `TIME_BETWEEN_APPLICATIONS`.
const TIME_BETWEEN_APPLICATIONS: i32 = 5;

/// Radius below which the cloud is spent.
///
/// Vanilla parity: `MINIMAL_RADIUS`.
const MINIMAL_RADIUS: f32 = 0.5;

/// Largest a cloud may be.
///
/// Vanilla parity: `MAX_RADIUS`.
const MAX_RADIUS: f32 = 32.0;

/// How long a lingering potion's cloud lasts.
///
/// Vanilla parity: `DEFAULT_LINGERING_DURATION`, thirty seconds.
pub const DEFAULT_LINGERING_DURATION: i32 = 600;

/// Radius a lingering potion's cloud starts at.
pub const DEFAULT_LINGERING_RADIUS: f32 = 3.0;

/// How much each victim costs the cloud.
///
/// Vanilla parity: the `setRadiusOnUse(-0.5F)` of `ThrownLingeringPotion`. This
/// is the rule that makes a cloud a limited resource rather than a free zone.
pub const DEFAULT_LINGERING_RADIUS_ON_USE: f32 = -0.5;

/// Ticks before a lingering cloud starts working.
pub const DEFAULT_LINGERING_WAIT_TIME: i32 = 10;

/// Ticks before the same entity may be dosed again.
///
/// Vanilla parity: `DEFAULT_REAPPLICATION_DELAY`.
const DEFAULT_REAPPLICATION_DELAY: i32 = 20;

/// State a cloud keeps that is not synced.
struct CloudState {
    /// Ticks the cloud runs for once it starts, or -1 for forever.
    duration: i32,
    /// Ticks before it starts.
    wait_time: i32,
    /// Ticks before one entity may be dosed twice.
    reapplication_delay: i32,
    /// How the radius changes each tick.
    radius_per_tick: f32,
    /// How the radius changes each time somebody is dosed.
    radius_on_use: f32,
    /// Effects this cloud carries, as (effect, duration, amplifier).
    effects: Vec<(MobEffectRef, i32, i32)>,
    /// Entity id to the tick it may be dosed again on.
    victims: FxHashMap<i32, i32>,
}

/// A lingering cloud of potion effects.
#[entity_behavior(class = "AreaEffectCloud")]
pub struct AreaEffectCloudEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<AreaEffectCloudEntityData>,
    state: SyncMutex<CloudState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies
// `AreaEffectCloudEntity`.
unsafe impl DowncastType for AreaEffectCloudEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/area_effect_cloud");
}

impl AreaEffectCloudEntity {
    /// Creates a cloud with vanilla's defaults.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(AreaEffectCloudEntityData::new()),
            state: SyncMutex::new(CloudState {
                duration: DEFAULT_LINGERING_DURATION,
                wait_time: 20,
                reapplication_delay: DEFAULT_REAPPLICATION_DELAY,
                radius_per_tick: 0.0,
                radius_on_use: 0.0,
                effects: Vec::new(),
                victims: FxHashMap::default(),
            }),
        }
    }

    /// Creates a cloud from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self::new_from_base(
            EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
        )
    }

    fn new_from_base(base: EntityBase, entity_type: EntityTypeRef) -> Self {
        Self {
            base,
            entity_type,
            entity_data: SyncMutex::new(AreaEffectCloudEntityData::new()),
            state: SyncMutex::new(CloudState {
                duration: DEFAULT_LINGERING_DURATION,
                wait_time: 20,
                reapplication_delay: DEFAULT_REAPPLICATION_DELAY,
                radius_per_tick: 0.0,
                radius_on_use: 0.0,
                effects: Vec::new(),
                victims: FxHashMap::default(),
            }),
        }
    }

    /// Sets this cloud up the way a lingering potion does.
    ///
    /// Vanilla parity: `ThrownLingeringPotion.onHitAsPotion`. The per-tick
    /// shrink is derived from the duration, so a cloud fades out exactly as it
    /// runs out rather than vanishing at full size.
    pub fn configure_as_lingering(&self, effects: Vec<(MobEffectRef, i32, i32)>) {
        self.set_radius(DEFAULT_LINGERING_RADIUS);

        let mut state = self.state.lock();
        state.duration = DEFAULT_LINGERING_DURATION;
        state.wait_time = DEFAULT_LINGERING_WAIT_TIME;
        state.radius_on_use = DEFAULT_LINGERING_RADIUS_ON_USE;
        #[expect(
            clippy::cast_precision_loss,
            reason = "the lingering duration is six hundred ticks"
        )]
        let per_tick = -DEFAULT_LINGERING_RADIUS / DEFAULT_LINGERING_DURATION as f32;
        state.radius_per_tick = per_tick;
        state.effects = effects;
    }

    /// Returns how far the cloud reaches.
    #[must_use]
    pub fn radius(&self) -> f32 {
        *self.entity_data.lock().radius.get()
    }

    /// Resizes the cloud and its hitbox.
    pub fn set_radius(&self, radius: f32) {
        let radius = radius.clamp(0.0, MAX_RADIUS);
        self.entity_data.lock().radius.set(radius);
        self.refresh_dimensions();
    }

    /// Returns whether the cloud is still settling.
    #[must_use]
    fn is_waiting(&self) -> bool {
        *self.entity_data.lock().waiting.get()
    }

    fn set_waiting(&self, waiting: bool) {
        self.entity_data.lock().waiting.set(waiting);
    }

    /// Doses everything standing in the cloud that is not on cooldown.
    ///
    /// Vanilla parity: the victim sweep of `AreaEffectCloud.serverTick`.
    /// Returns how much the radius should change for the entities it dosed.
    fn dose_victims(&self, world: &Arc<World>, radius: f32) -> f32 {
        let tick = self.tick_count();
        let position = self.position();

        let (effects, reapplication_delay, radius_on_use) = {
            let mut state = self.state.lock();
            state.victims.retain(|_, ready_at| tick < *ready_at);
            if state.effects.is_empty() {
                state.victims.clear();
                return 0.0;
            }
            (
                state.effects.clone(),
                state.reapplication_delay,
                state.radius_on_use,
            )
        };

        let aabb = WorldAabb::new(
            position.x - f64::from(radius),
            position.y - 0.5,
            position.z - f64::from(radius),
            position.x + f64::from(radius),
            position.y + 0.5,
            position.z + f64::from(radius),
        );

        let mut radius_change = 0.0;
        for entity in world.get_entities_in_aabb(&aabb) {
            let Some(living) = entity.as_living_entity() else {
                continue;
            };
            if self.state.lock().victims.contains_key(&entity.id()) {
                continue;
            }

            // Vanilla measures the horizontal distance only, so a cloud reaches
            // across a floor but not up a ladder.
            let entity_position = entity.position();
            let dx = entity_position.x - position.x;
            let dz = entity_position.z - position.z;
            if dx.mul_add(dx, dz * dz) > f64::from(radius * radius) {
                continue;
            }

            let mut dosed = false;
            for (effect, duration, amplifier) in &effects {
                let instance = MobEffectInstance::with_duration(effect, *duration, *amplifier);
                if !living.can_be_affected(&instance) {
                    continue;
                }
                living.add_mob_effect(instance);
                dosed = true;
            }

            if dosed {
                self.state
                    .lock()
                    .victims
                    .insert(entity.id(), tick + reapplication_delay);
                radius_change += radius_on_use;
            }
        }

        radius_change
    }
}

impl Entity for AreaEffectCloudEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `AreaEffectCloud.serverTick`.
    fn tick(&self) {
        let Some(world) = self.level() else {
            return;
        };
        let tick = self.tick_count();

        let (duration, wait_time, radius_per_tick) = {
            let state = self.state.lock();
            (state.duration, state.wait_time, state.radius_per_tick)
        };

        if duration != -1 && tick - wait_time >= duration {
            self.set_removed(RemovalReason::Discarded);
            return;
        }

        let should_wait = tick < wait_time;
        if self.is_waiting() != should_wait {
            self.set_waiting(should_wait);
        }
        if should_wait {
            return;
        }

        let mut radius = self.radius();
        if radius_per_tick != 0.0 {
            radius += radius_per_tick;
            if radius < MINIMAL_RADIUS {
                self.set_removed(RemovalReason::Discarded);
                return;
            }
            self.set_radius(radius);
        }

        if tick % TIME_BETWEEN_APPLICATIONS != 0 {
            return;
        }

        let radius_change = self.dose_victims(&world, radius);
        if radius_change != 0.0 {
            radius += radius_change;
            if radius < MINIMAL_RADIUS {
                self.set_removed(RemovalReason::Discarded);
                return;
            }
            self.set_radius(radius);
        }
    }

    /// The cloud is a flat disc, so its hitbox follows its radius.
    ///
    /// Vanilla parity: `AreaEffectCloud.makeBoundingBox`, height 0.5.
    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        EntityDimensions::new(self.radius() * 2.0, 0.5, 0.25)
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let state = self.state.lock();
        nbt.insert("Duration", state.duration);
        nbt.insert("WaitTime", state.wait_time);
        nbt.insert("ReapplicationDelay", state.reapplication_delay);
        nbt.insert("RadiusPerTick", state.radius_per_tick);
        nbt.insert("RadiusOnUse", state.radius_on_use);
        nbt.insert("Radius", self.radius());
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        let mut state = self.state.lock();
        state.duration = nbt.int("Duration").unwrap_or(DEFAULT_LINGERING_DURATION);
        state.wait_time = nbt.int("WaitTime").unwrap_or(20);
        state.reapplication_delay = nbt
            .int("ReapplicationDelay")
            .unwrap_or(DEFAULT_REAPPLICATION_DELAY);
        state.radius_per_tick = nbt.float("RadiusPerTick").unwrap_or(0.0);
        state.radius_on_use = nbt.float("RadiusOnUse").unwrap_or(0.0);
        drop(state);
        self.set_radius(nbt.float("Radius").unwrap_or(DEFAULT_LINGERING_RADIUS));

        // TODO: the effects a saved cloud carried are not restored yet; a world
        // reloaded mid-cloud leaves a shrinking patch that doses nobody.
    }
}
