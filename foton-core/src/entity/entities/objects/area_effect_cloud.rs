//! Area effect cloud.
//!
//! Vanilla parity: `AreaEffectCloud`. The lingering half of alchemy: a patch of
//! ground that keeps working after the bottle is gone. What makes it a distinct
//! thing rather than a slow splash is that it charges for use -- every entity it
//! catches shrinks it -- so one cloud can serve a crowd once or one target
//! several times, but not both.

use uuid::Uuid;

use std::sync::{Arc, Weak};

use crate::entity::living_base::{
    MobEffectInstance, apply_instantaneous_mob_effect, is_instantaneous,
};
use foton_macros::entity_behavior;
use foton_registry::RegistryExt;
use foton_registry::entity_data::ParticleData;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::particle_type::PowerParticleOption;
use foton_registry::potion::PotionRef;
use foton_registry::vanilla_entity_data::AreaEffectCloudEntityData;
use foton_registry::{vanilla_mob_effects, vanilla_particle_types};
use foton_utils::UuidExt;
use foton_utils::locks::SyncMutex;
use foton_utils::{DowncastType, DowncastTypeKey, WorldAabb};
use glam::DVec3;
use rustc_hash::FxHashMap;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};

use crate::entity::{Entity, EntityBase, EntityBaseLoad, EntitySyncedData, RemovalReason};
use crate::world::World;
use foton_registry::entity_data::EntityPose;
use foton_registry::entity_type::EntityDimensions;
use foton_registry::mob_effect::MobEffectRef;

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

/// Radius of the cloud a creeper carrying effects leaves behind.
///
/// Vanilla parity: the `setRadius(2.5F)` of `Creeper.spawnLingeringCloud`.
const CREEPER_CLOUD_RADIUS: f32 = 2.5;

/// How long that cloud lasts: fifteen seconds, half a potion's.
const CREEPER_CLOUD_DURATION: i32 = 300;

/// Ticks before it starts working.
const CREEPER_CLOUD_WAIT_TIME: i32 = 10;

/// How much each victim costs it.
const CREEPER_CLOUD_RADIUS_ON_USE: f32 = -0.5;

/// What a creeper's cloud divides each effect's duration by.
///
/// Vanilla parity: the `setPotionDurationScale(0.25F)` of the same method.
pub const CREEPER_CLOUD_DURATION_SCALE: i32 = 4;

/// Ticks before the same entity may be dosed again.
///
/// Vanilla parity: `DEFAULT_REAPPLICATION_DELAY`.
const DEFAULT_REAPPLICATION_DELAY: i32 = 20;

/// How much of an instant effect one dose from a cloud is worth.
///
/// Vanilla parity: the `0.5` scale of the `applyInstantaneousEffect` call in
/// `AreaEffectCloud.tick`. A cloud of harming costs three points a dose where
/// the bottle costs six.
const CLOUD_INSTANT_SCALE: f64 = 0.5;

/// How long a dragon fireball's cloud lasts.
///
/// Vanilla parity: the `setDuration(600)` of `DragonFireball.onHit`.
const DRAGON_BREATH_DURATION: i32 = 600;

/// Radius a dragon fireball's cloud starts at.
///
/// Vanilla parity: the `setRadius(3.0F)` of `DragonFireball.onHit`.
const DRAGON_BREATH_RADIUS: f32 = 3.0;

/// Radius a dragon fireball's cloud grows to over its lifetime.
///
/// Vanilla parity: the `7.0F` of the `setRadiusPerTick` in `DragonFireball.onHit`.
/// A dragon's breath spreads as it burns down, where a lingering potion shrinks.
const DRAGON_BREATH_FINAL_RADIUS: f32 = 7.0;

/// Amplifier of the harming effect a dragon's breath carries.
///
/// Vanilla parity: the `new MobEffectInstance(INSTANT_DAMAGE, 1, 1)` of
/// `DragonFireball.onHit`.
const DRAGON_BREATH_AMPLIFIER: i32 = 1;

/// Radius of the breath a sitting dragon lays down.
///
/// Vanilla parity: the `this.flame.setRadius(5.0F)` of
/// `DragonSittingFlamingPhase.doServerTick`.
pub const DRAGON_SITTING_FLAME_RADIUS: f32 = 5.0;

/// How long the breath a sitting dragon lays down lasts.
///
/// Vanilla parity: the `this.flame.setDuration(200)` of the same method, which
/// is also how long the phase itself runs.
pub const DRAGON_SITTING_FLAME_DURATION: i32 = 200;

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
    /// Effects this cloud carries.
    ///
    /// Keep complete `MobEffectInstance` metadata so cloud dosing preserves
    /// vanilla ambient, particle and icon visibility flags.
    effects: Vec<MobEffectInstance>,
    /// Entity id to the tick it may be dosed again on.
    victims: FxHashMap<i32, i32>,
    /// UUID of the projectile owner, when this cloud came from a projectile.
    owner_uuid: Option<Uuid>,
    /// Base potion carried by the cloud, when created from a potion item.
    base_potion: Option<PotionRef>,
}

/// A lingering cloud of potion effects.
#[entity_behavior(class = "AreaEffectCloud")]
pub struct AreaEffectCloudEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<AreaEffectCloudEntityData>,
    state: SyncMutex<CloudState>,
}

// SAFETY: This key is owned by Foton and uniquely identifies
// `AreaEffectCloudEntity`.
unsafe impl DowncastType for AreaEffectCloudEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/area_effect_cloud");
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
                owner_uuid: None,
                base_potion: None,
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
                owner_uuid: None,
                base_potion: None,
            }),
        }
    }

    /// Sets the entity that created this cloud.
    pub fn set_owner_uuid(&self, owner: Option<Uuid>) {
        self.state.lock().owner_uuid = owner;
    }

    /// Returns the UUID of the entity that created this cloud, if available.
    /// Sets the base potion represented by this cloud.
    pub fn set_base_potion(&self, potion: Option<PotionRef>) {
        self.state.lock().base_potion = potion;
    }

    /// Returns the base potion represented by this cloud.
    pub fn base_potion(&self) -> Option<PotionRef> {
        self.state.lock().base_potion
    }

    pub fn owner_uuid(&self) -> Option<Uuid> {
        self.state.lock().owner_uuid
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
        state.effects = effects
            .into_iter()
            .map(|(effect, duration, amplifier)| {
                MobEffectInstance::with_duration(effect, duration, amplifier)
            })
            .collect();
    }

    /// Sets this cloud up the way a creeper's death does.
    ///
    /// Vanilla parity: `Creeper.spawnLingeringCloud`. It is smaller and
    /// shorter-lived than a lingering potion's, and it hands out a quarter of
    /// each effect it carries.
    ///
    /// Deviation: vanilla keeps that quarter on the cloud as
    /// `potionDurationScale` and applies it per victim. Foton stores plain
    /// durations, so the quarter is taken when the cloud is built -- which
    /// afflicts a victim for the same length of time either way.
    pub fn configure_as_creeper_cloud(&self, effects: Vec<(MobEffectRef, i32, i32)>) {
        self.set_radius(CREEPER_CLOUD_RADIUS);

        let mut state = self.state.lock();
        state.duration = CREEPER_CLOUD_DURATION;
        state.wait_time = CREEPER_CLOUD_WAIT_TIME;
        state.radius_on_use = CREEPER_CLOUD_RADIUS_ON_USE;
        #[expect(
            clippy::cast_precision_loss,
            reason = "the creeper cloud's duration is three hundred ticks"
        )]
        let per_tick = -CREEPER_CLOUD_RADIUS / CREEPER_CLOUD_DURATION as f32;
        state.radius_per_tick = per_tick;
        state.effects = effects
            .into_iter()
            .map(|(effect, duration, amplifier)| {
                MobEffectInstance::with_duration(effect, duration, amplifier)
            })
            .collect();
    }

    /// Sets this cloud up the way a dragon fireball does.
    ///
    /// Vanilla parity: the cloud built by `DragonFireball.onHit`. Unlike a
    /// lingering potion's cloud this one grows, from three blocks to seven over
    /// its half-minute, and costs nothing per victim, so it keeps working on
    /// everything standing in it for the whole duration.
    ///
    /// Two vanilla settings are dropped because Foton has nowhere to put them.
    /// `cloud.setOwner(livingEntity)` has no equivalent -- Foton's cloud has no
    /// owner, so its damage is credited to nobody. `setPotionDurationScale(0.25F)`
    /// likewise: Foton stores plain durations on the cloud, and the scale only
    /// matters for effects that last, which a dragon's breath does not carry.
    pub fn configure_as_dragon_breath(&self) {
        self.set_radius(DRAGON_BREATH_RADIUS);
        self.entity_data.lock().particle.set(ParticleData::new(
            &vanilla_particle_types::DRAGON_BREATH,
            PowerParticleOption::new(1.0),
        ));

        let mut state = self.state.lock();
        state.duration = DRAGON_BREATH_DURATION;
        state.wait_time = 10;
        state.radius_on_use = 0.0;
        state.radius_per_tick =
            (DRAGON_BREATH_FINAL_RADIUS - DRAGON_BREATH_RADIUS) / DRAGON_BREATH_DURATION as f32;
        state.effects = vec![MobEffectInstance::with_duration(
            vanilla_mob_effects::INSTANT_DAMAGE,
            1,
            DRAGON_BREATH_AMPLIFIER,
        )];
    }

    /// Sets this cloud up the way a sitting dragon's breath does.
    ///
    /// Vanilla parity: the cloud built by `DragonSittingFlamingPhase`. It is
    /// not the fireball's cloud: it starts at its full five blocks and neither
    /// grows nor shrinks, and it lasts ten seconds rather than thirty.
    ///
    /// The same two settings are dropped as for the fireball's cloud, for the
    /// same reasons: `setOwner` has nowhere to go, and `setPotionDurationScale`
    /// only matters to effects that last.
    pub fn configure_as_dragon_sitting_flame(&self) {
        self.set_radius(DRAGON_SITTING_FLAME_RADIUS);
        self.entity_data.lock().particle.set(ParticleData::new(
            &vanilla_particle_types::DRAGON_BREATH,
            PowerParticleOption::new(1.0),
        ));

        let mut state = self.state.lock();
        state.duration = DRAGON_SITTING_FLAME_DURATION;
        // Vanilla's `new MobEffectInstance(MobEffects.INSTANT_DAMAGE)`, whose
        // one-argument constructor is duration zero. That never matters: a
        // cloud applies an instant effect on the spot rather than afflicting
        // anyone with it, so the duration is never read.
        state.effects = vec![MobEffectInstance::with_duration(
            vanilla_mob_effects::INSTANT_DAMAGE,
            0,
            0,
        )];
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

    /// Returns the active lifetime in ticks after the initial wait.
    #[must_use]
    pub fn duration(&self) -> i32 {
        self.state.lock().duration
    }

    pub fn set_duration(&self, duration: i32) {
        self.state.lock().duration = duration;
    }

    /// Returns the initial wait time in ticks.
    #[must_use]
    pub fn wait_time(&self) -> i32 {
        self.state.lock().wait_time
    }

    pub fn set_wait_time(&self, wait_time: i32) {
        self.state.lock().wait_time = wait_time;
    }

    /// Returns the per-entity reapplication cooldown in ticks.
    #[must_use]
    pub fn reapplication_delay(&self) -> i32 {
        self.state.lock().reapplication_delay
    }

    pub fn set_reapplication_delay(&self, delay: i32) {
        self.state.lock().reapplication_delay = delay;
    }

    #[must_use]
    pub fn radius_per_tick(&self) -> f32 {
        self.state.lock().radius_per_tick
    }

    pub fn set_radius_per_tick(&self, radius: f32) {
        self.state.lock().radius_per_tick = radius;
    }

    #[must_use]
    pub fn radius_on_use(&self) -> f32 {
        self.state.lock().radius_on_use
    }

    pub fn set_radius_on_use(&self, radius: f32) {
        self.state.lock().radius_on_use = radius;
    }

    /// Returns the complete custom effects carried by this cloud.
    #[must_use]
    pub fn effects(&self) -> Vec<MobEffectInstance> {
        self.state.lock().effects.clone()
    }

    /// Adds a custom effect, following Bukkit override semantics.
    pub fn add_custom_effect(&self, effect: MobEffectInstance, override_existing: bool) -> bool {
        let mut state = self.state.lock();
        if let Some(existing) = state
            .effects
            .iter_mut()
            .find(|existing| existing.effect() == effect.effect())
        {
            if !override_existing {
                return false;
            }
            *existing = effect;
            return true;
        }
        state.effects.push(effect);
        true
    }

    /// Removes all custom effects from this cloud.
    pub fn clear_custom_effects(&self) {
        self.state.lock().effects.clear();
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
            for effect_instance in &effects {
                let effect = effect_instance.effect();
                // Vanilla parity: the `isInstantaneous` branch of
                // `AreaEffectCloud.tick`. A cloud does not afflict anyone with
                // an instant effect -- it applies it on the spot, and at half
                // strength, which is why standing in a dragon's breath costs
                // three points a dose rather than six.
                if is_instantaneous(effect) {
                    apply_instantaneous_mob_effect(
                        world,
                        living,
                        effect,
                        effect_instance.amplifier(),
                        CLOUD_INSTANT_SCALE,
                    );
                    dosed = true;
                    continue;
                }
                let instance = effect_instance.clone();
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
        if !state.effects.is_empty() {
            nbt.insert(
                "Effects",
                NbtList::Compound(
                    state
                        .effects
                        .iter()
                        .map(MobEffectInstance::to_vanilla_nbt)
                        .collect(),
                ),
            );
        }
        if let Some(potion) = state.base_potion {
            nbt.insert("Potion", potion.key.to_string());
        }
        if let Some(owner) = state.owner_uuid {
            nbt.insert("Owner", NbtTag::IntArray(owner.to_int_array().to_vec()));
        }
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

        state.effects = nbt
            .list("Effects")
            .and_then(|list| list.compounds())
            .map(|effects| {
                effects
                    .into_iter()
                    .filter_map(|effect| MobEffectInstance::from_vanilla_nbt(&effect))
                    .collect()
            })
            .unwrap_or_default();
        state.base_potion = nbt
            .string("Potion")
            .and_then(|key| key.to_str().parse().ok())
            .and_then(|key| foton_registry::REGISTRY.potions.by_key(&key));
        state.owner_uuid = nbt
            .int_array("Owner")
            .and_then(|values| Uuid::from_int_array(&values));
        drop(state);
        self.set_radius(nbt.float("Radius").unwrap_or(DEFAULT_LINGERING_RADIUS));
    }
}

#[cfg(test)]
mod tests {
    use foton_registry::{init_vanilla_registry, vanilla_entities};
    use foton_utils::ChunkPos;

    use super::*;
    use crate::entity::entities::CowEntity;
    use crate::entity::{LivingEntity as _, next_entity_id};
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    /// A dragon's breath carries `INSTANT_DAMAGE`, which a cloud applies on the
    /// spot rather than afflicting anyone with. Before that branch existed the
    /// cloud handed out an effect instance nothing ever ticked, and standing in
    /// a dragon's breath was free.
    #[test]
    fn a_dragon_breath_cloud_hurts_what_stands_in_it() {
        init_vanilla_registry();
        let world = fresh_test_world("area_effect_cloud_dragon_breath");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

        let cow = Arc::new(CowEntity::new(
            &vanilla_entities::COW,
            next_entity_id(),
            DVec3::new(8.5, 64.0, 8.5),
            Arc::downgrade(&world),
        ));
        world
            .try_add_entity(cow.clone())
            .expect("the cow's chunk is loaded");
        let full_health = cow.get_health();

        let cloud = Arc::new(AreaEffectCloudEntity::new(
            &vanilla_entities::AREA_EFFECT_CLOUD,
            next_entity_id(),
            DVec3::new(8.5, 64.0, 8.5),
            Arc::downgrade(&world),
        ));
        world
            .try_add_entity(cloud.clone())
            .expect("the cloud's chunk is loaded");
        cloud.configure_as_dragon_breath();

        // The cloud waits ten ticks before it starts, and only sweeps every
        // fifth tick after that.
        for _ in 0..=(DEFAULT_LINGERING_WAIT_TIME + TIME_BETWEEN_APPLICATIONS) {
            cloud.advance_tick_count();
            cloud.tick();
        }

        // Amplifier one at the cloud's half scale: `(0.5 * (6 << 1) + 0.5)`.
        assert_f32_close(full_health - cow.get_health(), 6.0);
    }

    fn assert_f32_close(left: f32, right: f32) {
        assert!(
            (left - right).abs() <= f32::EPSILON,
            "expected {left} to equal {right}"
        );
    }
}
