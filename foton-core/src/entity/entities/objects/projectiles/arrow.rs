//! Arrow entity.
//!
//! Vanilla parity: `AbstractArrow` and `Arrow`. An arrow flies under gravity and
//! drag, damages what it hits in proportion to its speed, and sticks into the
//! first block it meets until it despawns.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::{CGameEvent, GameEventType};
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::item_stack::ItemStack;
use foton_registry::vanilla_entity_data::ArrowEntityData;
use foton_registry::{sound_events, vanilla_damage_types, vanilla_entities, vanilla_items};
use foton_utils::locks::SyncMutex;
use foton_utils::{DowncastType, DowncastTypeKey};
use glam::DVec3;

use crate::enchantment_helper::{self, EnchantmentDamageContext};
use crate::entity::damage::DamageSource;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, MobEffectInstance, Projectile,
    ProjectileBase, ProjectileDeflection, RemovalReason, SharedEntity, next_entity_id,
};
use crate::event::Event as _;
use crate::inventory::container::Container as _;
use crate::inventory::slot_ranges::CONTENTS_SLOT;
use crate::physics::MoverType;
use crate::player::Player;
use crate::world::{ClipHitResult, World};

/// Damage an arrow carries before speed is taken into account.
///
/// Vanilla parity: `AbstractArrow.baseDamage`, 2.0 for a plain arrow.
const DEFAULT_BASE_DAMAGE: f64 = 2.0;

/// Vanilla parity: `AbstractArrow.getDefaultGravity`.
const DEFAULT_GRAVITY: f64 = 0.05;

/// Velocity kept each tick in air.
///
/// Vanilla parity: `AbstractArrow.getAirDrag`.
const AIR_DRAG: f64 = 0.99;

/// Ticks a stuck arrow survives before it disappears.
///
/// Vanilla parity: the 1200-tick limit in `AbstractArrow.tickDespawn`.
const DESPAWN_TICKS: i32 = 1200;

/// Ticks a freshly landed arrow wobbles for.
///
/// Vanilla parity: `AbstractArrow.SHAKE_TIME`. It is also what keeps a player
/// from snatching an arrow out of the air the instant it lands.
const SHAKE_TIME: i32 = 7;

/// Vanilla parity: `AbstractArrow.FLAG_CRIT`.
const FLAG_CRIT: i8 = 1;

/// Seconds a burning arrow sets its target alight for.
///
/// Vanilla parity: the `igniteForSeconds(5.0F)` of `AbstractArrow.onHitEntity`.
const IGNITE_TICKS: i32 = 100;

/// How far back along its own travel an arrow is pulled when it lands.
///
/// Vanilla parity: the `offsetDirection.scale(0.05F)` of
/// `AbstractArrow.onHitBlock`.
const BLOCK_HIT_STEP_BACK: f64 = 0.05;

/// Speed kept by an arrow a target refused.
///
/// Vanilla parity: the `getDeltaMovement().scale(0.2)` of the failed-hit branch
/// of `AbstractArrow.onHitEntity`.
const DEFLECTED_SPEED_SCALE: f64 = 0.2;

/// Below this squared speed a deflected arrow is treated as stopped.
///
/// Vanilla parity: the `lengthSqr() < 1.0E-7` of the same branch.
const STOPPED_SPEED_SQUARED: f64 = 1.0e-7;

/// Vanilla parity: the `knockback * 0.6` of `AbstractArrow.doKnockback`.
const KNOCKBACK_SCALE: f64 = 0.6;

/// Vanilla parity: the vertical component of the same push.
const KNOCKBACK_LIFT: f64 = 0.1;

/// Who is allowed to pick an arrow back up.
///
/// Vanilla parity: `AbstractArrow.Pickup`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowPickup {
    /// Nobody -- a skeleton's arrows, and anything an ominous spawner fires.
    Disallowed,
    /// Anyone who walks into it.
    Allowed,
    /// Only a player who does not pay for their arrows, which is what an
    /// Infinity or creative shot leaves behind.
    CreativeOnly,
}

/// State that is not mirrored to clients.
struct ArrowState {
    /// Damage before the speed multiplier.
    base_damage: f64,
    /// Ticks spent stuck in a block.
    life: i32,
    /// Ticks left of the landing wobble.
    ///
    /// Vanilla parity: `AbstractArrow.shakeTime`.
    shake_time: i32,
    /// Who may collect this arrow once it lands.
    pickup: ArrowPickup,
    /// The weapon that loosed this arrow, whose enchantments it carries.
    ///
    /// Vanilla parity: `AbstractArrow.firedFromWeapon`.
    fired_from_weapon: Option<ItemStack>,
    /// The ammunition item, including PotionContents for tipped arrows.
    fired_from_ammo: Option<ItemStack>,
    /// Effects the arrow hands to whatever it hits.
    effects: Vec<MobEffectInstance>,
}

impl ArrowState {
    const fn new() -> Self {
        Self {
            base_damage: DEFAULT_BASE_DAMAGE,
            life: 0,
            shake_time: 0,
            pickup: ArrowPickup::Disallowed,
            fired_from_weapon: None,
            fired_from_ammo: None,
            effects: Vec::new(),
        }
    }
}

/// A flying arrow.
#[entity_behavior(class = "Arrow")]
pub struct ArrowEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<ArrowEntityData>,
    projectile_base: ProjectileBase,
    state: SyncMutex<ArrowState>,
}

// SAFETY: This Foton-owned key uniquely identifies `ArrowEntity`.
unsafe impl DowncastType for ArrowEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/arrow");
}

impl ArrowEntity {
    /// Creates an arrow at `position`.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(ArrowEntityData::new()),
            projectile_base: ProjectileBase::new(),
            state: SyncMutex::new(ArrowState::new()),
        }
    }

    /// Creates an arrow from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(ArrowEntityData::new()),
            projectile_base: ProjectileBase::new(),
            state: SyncMutex::new(ArrowState::new()),
        }
    }

    /// Spawns an arrow shot by `shooter` and adds it to the world.
    pub fn shoot_from(
        world: &Arc<World>,
        shooter: &dyn Entity,
        entity_type: EntityTypeRef,
        power: f32,
        uncertainty: f32,
    ) -> Option<Arc<Self>> {
        let position = shooter.position().with_y(shooter.get_eye_y() - 0.1);
        let arrow = Arc::new(Self::new(
            entity_type,
            next_entity_id(),
            position,
            Arc::downgrade(world),
        ));
        arrow.set_owner_uuid(Some(shooter.uuid()));
        arrow.apply_owner_pickup(shooter);
        let (yaw, pitch) = shooter.rotation();
        arrow.shoot_from_rotation(shooter, pitch, yaw, 0.0, power, uncertainty);

        let mut event = crate::event::ProjectileLaunchEvent::new(shooter.uuid(), arrow.uuid());
        world.fire_event(&mut event);
        if event.is_cancelled() {
            return None;
        }
        if let Err(error) = world.try_add_entity(Arc::clone(&arrow) as Arc<dyn Entity>) {
            log::error!("failed to add arrow entity: {error}");
            return None;
        }
        Some(arrow)
    }

    /// Spawns an arrow aimed at `target` rather than along the shooter's look.
    ///
    /// Vanilla parity: the aiming maths of `AbstractSkeleton.performRangedAttack`,
    /// which lifts the shot by a fifth of the horizontal distance so the arc lands
    /// on target.
    pub fn shoot_at(
        world: &Arc<World>,
        shooter: &dyn Entity,
        target: DVec3,
        power: f32,
        uncertainty: f32,
    ) -> Arc<Self> {
        let position = shooter.position().with_y(shooter.get_eye_y() - 0.1);
        let arrow = Arc::new(Self::new(
            &vanilla_entities::ARROW,
            next_entity_id(),
            position,
            Arc::downgrade(world),
        ));
        arrow.set_owner_uuid(Some(shooter.uuid()));
        arrow.apply_owner_pickup(shooter);

        let dx = target.x - position.x;
        let dz = target.z - position.z;
        let horizontal = dx.hypot(dz);
        let dy = horizontal.mul_add(0.2, target.y - position.y);
        arrow.shoot(DVec3::new(dx, dy, dz), power, uncertainty);

        if let Err(error) = world.try_add_entity(Arc::clone(&arrow) as Arc<dyn Entity>) {
            log::error!("failed to add arrow entity: {error}");
        }
        arrow
    }

    /// Adds an effect the arrow will apply to whatever it hits.
    ///
    /// Vanilla parity: `Arrow.addEffect`. Vanilla keeps the effect in the
    /// arrow's `PotionContents` component and scales its duration by
    /// `POTION_DURATION_SCALE`; Foton has no potion component on projectiles
    /// yet, so effects are held on the entity and applied at full duration.
    /// That matches the mobs that add effects directly, such as a stray's
    /// slowness, and only diverges for the tipped arrows Foton cannot fire yet.
    pub fn add_effect(&self, effect: MobEffectInstance) {
        self.state.lock().effects.push(effect);
    }

    /// Returns the effects this arrow applies on impact.
    pub fn effects(&self) -> Vec<MobEffectInstance> {
        self.state.lock().effects.clone()
    }

    /// Hands the arrow's effects to a living target.
    ///
    /// Vanilla parity: `Arrow.doPostHurtEffects`.
    fn do_post_hurt_effects(&self, target: &SharedEntity) {
        let Some(living) = target.as_living_entity() else {
            return;
        };
        // Cloned out of the lock: applying an effect reaches back into the world.
        let effects = self.state.lock().effects.clone();
        for effect in effects {
            living.add_mob_effect(effect);
        }
    }

    /// Returns whether this arrow was loosed from a fully drawn bow.
    ///
    /// Vanilla parity: `AbstractArrow.isCritArrow`.
    #[must_use]
    pub fn is_crit_arrow(&self) -> bool {
        *self.entity_data.lock().abstract_arrow.id_flags.get() & FLAG_CRIT != 0
    }

    /// Marks the arrow as critical.
    ///
    /// Vanilla parity: `AbstractArrow.setCritArrow`.
    pub fn set_crit_arrow(&self, crit_arrow: bool) {
        let mut data = self.entity_data.lock();
        let flags = *data.abstract_arrow.id_flags.get();
        data.abstract_arrow.id_flags.set(if crit_arrow {
            flags | FLAG_CRIT
        } else {
            flags & !FLAG_CRIT
        });
    }

    /// Returns how many entities this arrow punches through.
    ///
    /// Vanilla parity: `AbstractArrow.getPierceLevel`.
    #[must_use]
    pub fn pierce_level(&self) -> i8 {
        *self.entity_data.lock().abstract_arrow.pierce_level.get()
    }

    fn set_pierce_level(&self, pierce_level: i8) {
        self.entity_data
            .lock()
            .abstract_arrow
            .pierce_level
            .set(pierce_level);
    }

    /// The stack a player picking this arrow up receives.
    ///
    /// Vanilla parity: `Arrow.getDefaultPickupItem`, which is what
    /// `AbstractArrow.getPickupItemStackOrigin` answers with until something
    /// overrides it. Vanilla keeps that stack per arrow, so a tipped one comes
    /// back tipped; Foton has no potion component on projectiles yet -- the
    /// same gap `on_hit_entity` documents -- and this is the one place that
    /// changes when it does.
    #[must_use]
    #[expect(
        clippy::unused_self,
        reason = "vanilla reads the per-arrow origin stack through this receiver"
    )]
    pub fn pickup_item(&self) -> ItemStack {
        ItemStack::new(&vanilla_items::ARROW)
    }

    /// Returns who may collect this arrow.
    #[must_use]
    pub fn pickup(&self) -> ArrowPickup {
        self.state.lock().pickup
    }

    /// Sets who may collect this arrow.
    pub fn set_pickup(&self, pickup: ArrowPickup) {
        self.state.lock().pickup = pickup;
    }

    /// Records the weapon this arrow was loosed from.
    ///
    /// Vanilla parity: the `firedFromWeapon` an `AbstractArrow` is built with.
    /// Its enchantments are read at impact, which is where Power and Punch live.
    ///
    /// Piercing is the exception: the constructor reads it off the weapon the
    /// moment it is handed over and raises the pierce level once, because the
    /// bolt has to know how many mobs to pass through before it hits the first.
    pub fn set_fired_from_weapon(&self, weapon: Option<ItemStack>) {
        if let Some(weapon) = &weapon {
            let piercing = enchantment_helper::get_piercing_count(weapon, &self.pickup_item());
            if piercing > 0 {
                self.set_pierce_level(i8::try_from(piercing).unwrap_or(i8::MAX));
            }
        }
        self.state.lock().fired_from_weapon = weapon;
    }

    /// Returns the weapon this arrow was loosed from.
    ///
    /// Vanilla parity: `AbstractArrow.getWeaponItem`.
    #[must_use]
    pub fn weapon_item(&self) -> Option<ItemStack> {
        self.state.lock().fired_from_weapon.clone()
    }

    /// Returns the ammunition item used to create this arrow.
    #[must_use]
    pub fn ammo_item(&self) -> Option<ItemStack> {
        self.state.lock().fired_from_ammo.clone()
    }

    /// Returns the potion contents carried by the ammunition, when tipped.
    #[must_use]
    pub fn ammo_potion_contents(
        &self,
    ) -> Option<foton_registry::data_components::components::PotionContents> {
        self.ammo_item()?
            .get(foton_registry::data_components::vanilla_components::POTION_CONTENTS)
            .cloned()
    }

    /// Computes the vanilla potion display color, including custom effects.
    #[must_use]
    pub fn ammo_potion_color(&self) -> Option<i32> {
        let contents = self.ammo_potion_contents()?;
        if let Some(color) = contents.custom_color() {
            return Some(color);
        }
        let mut red = 0i64;
        let mut green = 0i64;
        let mut blue = 0i64;
        let mut weight = 0i64;
        if let Some(potion) = contents.potion() {
            for effect in potion.value().effects {
                let color = effect.effect.color;
                let effect_weight = i64::from(effect.amplifier + 1);
                red += i64::from(color.red()) * effect_weight;
                green += i64::from(color.green()) * effect_weight;
                blue += i64::from(color.blue()) * effect_weight;
                weight += effect_weight;
            }
        }
        for effect in contents.custom_effects() {
            let color = effect.effect().color;
            let effect_weight = i64::from(effect.amplifier() + 1);
            red += i64::from(color.red()) * effect_weight;
            green += i64::from(color.green()) * effect_weight;
            blue += i64::from(color.blue()) * effect_weight;
            weight += effect_weight;
        }
        (weight > 0)
            .then(|| ((red / weight) << 16 | (green / weight) << 8 | (blue / weight)) as i32)
    }

    /// Records the ammunition item used to create this arrow.
    pub fn set_ammo_item(&self, ammo: ItemStack) {
        self.state.lock().fired_from_ammo = Some(ammo);
    }

    /// Applies vanilla's owner-dependent pickup rule.
    ///
    /// Vanilla parity: `AbstractArrow.setOwner`, where an arrow a player shot
    /// becomes collectable unless the ammunition already marked it otherwise.
    fn apply_owner_pickup(&self, shooter: &dyn Entity) {
        let mut state = self.state.lock();
        if shooter.entity_type() == &vanilla_entities::PLAYER
            && state.pickup == ArrowPickup::Disallowed
        {
            state.pickup = ArrowPickup::Allowed;
        }
    }

    /// Pushes a target back the way Punch asks.
    ///
    /// Vanilla parity: `AbstractArrow.doKnockback`. With no weapon, or an
    /// unenchanted one, the modified knockback is zero and nothing moves --
    /// which is why a plain arrow does not shove.
    fn do_knockback(&self, target: &dyn Entity, damage_source: &DamageSource) {
        let Some(weapon) = self.weapon_item() else {
            return;
        };
        let Some(living) = target.as_living_entity() else {
            return;
        };
        let owner_type = self.get_owner().map(|owner| owner.entity_type());
        let context = EnchantmentDamageContext::new(
            target.entity_type(),
            owner_type,
            Some(self.entity_type()),
            damage_source,
        );
        let knockback = f64::from(enchantment_helper::modify_knockback(&weapon, &context, 0.0));
        if knockback <= 0.0 {
            return;
        }

        let resistance = (1.0 - living.knockback_resistance()).max(0.0);
        let movement = (self.velocity() * DVec3::new(1.0, 0.0, 1.0)).normalize_or_zero()
            * (knockback * KNOCKBACK_SCALE * resistance);
        if movement.length_squared() > 0.0 {
            target.push_impulse(DVec3::new(movement.x, KNOCKBACK_LIFT, movement.z));
        }
    }

    /// Returns how many ticks of landing wobble are left.
    ///
    /// Vanilla parity: `AbstractArrow.shakeTime`.
    #[must_use]
    pub fn shake_time(&self) -> i32 {
        self.state.lock().shake_time
    }

    /// Returns whether the arrow is stuck in a block.
    #[must_use]
    pub fn is_in_ground(&self) -> bool {
        *self.entity_data.lock().abstract_arrow.in_ground.get()
    }

    fn set_in_ground(&self, in_ground: bool) {
        self.entity_data
            .lock()
            .abstract_arrow
            .in_ground
            .set(in_ground);
    }

    /// Returns the damage this arrow deals before the speed multiplier.
    #[must_use]
    pub fn base_damage(&self) -> f64 {
        self.state.lock().base_damage
    }

    /// Sets the damage this arrow deals before the speed multiplier.
    pub fn set_base_damage(&self, damage: f64) {
        self.state.lock().base_damage = damage;
    }

    /// Points the arrow along its current velocity.
    ///
    /// Vanilla parity: the rotation block of `AbstractArrow.tick`.
    fn face_velocity(&self) {
        let movement = self.velocity();
        let horizontal = movement.x.hypot(movement.z);
        let yaw = movement.x.atan2(movement.z).to_degrees() as f32;
        let pitch = movement.y.atan2(horizontal).to_degrees() as f32;
        self.set_rotation((yaw, pitch));
    }

    /// Counts down the lifetime of a stuck arrow.
    ///
    /// Vanilla parity: `AbstractArrow.tickDespawn`.
    fn tick_despawn(&self) {
        let expired = {
            let mut state = self.state.lock();
            state.life += 1;
            state.life >= DESPAWN_TICKS
        };
        if expired {
            self.set_removed(RemovalReason::Discarded);
        }
    }
}

impl Entity for ArrowEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    /// Vanilla parity: `AbstractArrow.getSlot`, whose one slot is what a player picking it up receives.
    fn slot_item(&self, slot: i32) -> Option<ItemStack> {
        if slot == CONTENTS_SLOT {
            return Some(self.pickup_item());
        }
        self.entity_slot_item(slot)
    }

    fn tick(&self) {
        // Vanilla parity: the `if (this.shakeTime > 0) this.shakeTime--;` of
        // `AbstractArrow.tick`, which runs whether or not the arrow has landed.
        {
            let mut state = self.state.lock();
            state.shake_time = (state.shake_time - 1).max(0);
        }

        if self.is_in_ground() {
            self.tick_despawn();
            return;
        }

        self.check_left_owner();
        self.face_velocity();

        if let Some(hit) = self.get_hit_result_on_move_vector() {
            self.hit_target_or_deflect_self(&hit);
            if !self.is_alive() {
                return;
            }
        }

        let _ = self.move_entity(MoverType::SelfMovement, self.velocity());
        self.apply_effects_from_blocks();
        self.set_velocity(self.velocity() * AIR_DRAG);
        self.apply_gravity();
    }

    fn get_default_gravity(&self) -> f64 {
        DEFAULT_GRAVITY
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn is_pickable(&self) -> bool {
        true
    }

    /// Lets a player collect an arrow that has landed.
    ///
    /// Vanilla parity: `AbstractArrow.playerTouch`. Only a stuck arrow can be
    /// picked up, which is why one still in flight passes straight through.
    fn player_touch(self: Arc<Self>, player: &Arc<Player>) {
        // Vanilla parity: `AbstractArrow.playerTouch` also waits out `shakeTime`,
        // so an arrow cannot be collected the tick it lands.
        if !self.is_in_ground() || self.state.lock().shake_time > 0 {
            return;
        }

        // Vanilla parity: `AbstractArrow.tryPickup`.
        match self.pickup() {
            ArrowPickup::Disallowed => return,
            ArrowPickup::CreativeOnly => {
                if !player.has_infinite_materials() {
                    return;
                }
            }
            ArrowPickup::Allowed => {
                let mut stack = self.pickup_item();
                let before = stack.count();
                player.inventory.lock().add(&mut stack);
                if stack.count() == before {
                    return;
                }
            }
        }

        self.set_removed(RemovalReason::Discarded);
    }
}

impl Projectile for ArrowEntity {
    fn projectile_base(&self) -> &ProjectileBase {
        &self.projectile_base
    }

    /// Damages the entity in proportion to the arrow's speed.
    ///
    /// Vanilla parity: `AbstractArrow.onHitEntity`. Damage is
    /// `ceil(speed * base_damage)`, so a fully drawn bow hurts far more than a
    /// spent arrow drifting to the ground.
    fn on_hit_entity(&self, entity: &SharedEntity, _location: DVec3) {
        let Some(world) = self.level() else {
            return;
        };
        let target = entity.as_ref();
        let speed = self.velocity().length();

        let owner = self.get_owner();
        let mut source = DamageSource::environment(&vanilla_damage_types::ARROW)
            .with_direct_entity(self.id())
            .with_source_position(self.position());
        if let Some(owner) = owner.as_ref() {
            source = source.with_causing_entity(owner.id());
        }

        // Vanilla parity: the firing weapon's `modifyDamage` runs on the base
        // damage, before the speed multiplier. That is where Power lives.
        let mut arrow_damage = self.base_damage();
        if let Some(weapon) = self.weapon_item() {
            let context = EnchantmentDamageContext::new(
                target.entity_type(),
                owner.as_ref().map(|owner| owner.entity_type()),
                Some(self.entity_type()),
                &source,
            );
            arrow_damage = f64::from(enchantment_helper::modify_damage(
                &weapon,
                &context,
                arrow_damage as f32,
            ));
        }

        let mut damage = (speed * arrow_damage)
            .clamp(0.0, f64::from(i32::MAX))
            .ceil() as i32;
        // Vanilla parity: a critical arrow rolls a bonus in `[0, damage/2 + 2)`.
        if self.is_crit_arrow() {
            damage = damage.saturating_add(rand::random_range(0..damage / 2 + 2));
        }

        if let Some(owner_living) = owner.as_ref().and_then(|owner| owner.as_living_entity()) {
            owner_living.set_last_hurt_mob(Some(entity));
        }

        // Vanilla parity: an endermen teleports away instead of catching fire,
        // and takes no arrow with it.
        let is_enderman = target.entity_type() == &vanilla_entities::ENDERMAN;
        let fire_ticks_before = target.remaining_fire_ticks();
        if self.is_on_fire() && !is_enderman {
            target.ignite_for_ticks(IGNITE_TICKS);
        }

        if !target.hurt(&world, &source, damage as f32) {
            // Vanilla parity: a refused hit gives the target its fire back and
            // bounces the arrow off.
            target.set_remaining_fire_ticks(fire_ticks_before);
            self.deflect(
                ProjectileDeflection::Reverse,
                Some(target),
                self.owner_uuid(),
                owner.as_ref(),
                false,
            );
            self.set_velocity(self.velocity() * DEFLECTED_SPEED_SCALE);
            if self.velocity().length_squared() < STOPPED_SPEED_SQUARED {
                self.set_removed(RemovalReason::Discarded);
            }
            return;
        }

        if is_enderman {
            return;
        }

        if let Some(living) = target.as_living_entity() {
            // Vanilla parity: the arrow stays stuck in the mob it hit, unless it
            // is a piercing shot that is meant to carry on through.
            if self.pierce_level() <= 0 {
                living.set_arrow_count(living.arrow_count() + 1);
            }
            self.do_knockback(target, &source);
            self.do_post_hurt_effects(entity);

            // Vanilla parity: the `PLAY_ARROW_HIT_SOUND` branch of
            // `AbstractArrow.onHitEntity`. The hit marker is a client-side
            // sound with no packet of its own -- the game event is the only
            // thing that fires it -- so a shooter whose server knew perfectly
            // well that the arrow landed still heard nothing.
            if let Some(shooter) = owner.as_ref().and_then(|owner| owner.as_player())
                && target.as_player().is_some()
                && !self.is_silent()
                && target.id() != shooter.id()
            {
                shooter.send_packet(CGameEvent {
                    event: GameEventType::PlayArrowHitSound,
                    data: 0.0,
                });
            }
        }

        self.play_sound(
            &sound_events::ENTITY_ARROW_HIT,
            1.0,
            1.2 / 0.2f32.mul_add(rand::random::<f32>(), 0.9),
        );

        // TODO: vanilla also runs the firing weapon's post-attack enchantment
        // effects and its knockback here; Foton's arrows carry no weapon yet.
        if self.pierce_level() <= 0 {
            self.set_removed(RemovalReason::Discarded);
        }
    }

    /// Sticks the arrow into the block it hit.
    ///
    /// Vanilla parity: `AbstractArrow.onHitBlock`.
    fn on_hit_block(&self, hit: &ClipHitResult) {
        self.projectile_on_hit_block(hit);

        // Vanilla parity: `AbstractArrow.stepMoveAndHit` puts the arrow on the
        // hit point, and `onHitBlock` then backs it off by a twentieth of a
        // block along the axis signs of its travel, which is what leaves the
        // shaft showing rather than the head. Foton's shared projectile engine
        // does not advance a projectile to its hit point at all -- it stops
        // where the tick began, which can be blocks short -- so both halves are
        // done here off the hit result.
        let movement = self.velocity();
        let offset_direction = DVec3::new(
            movement.x.signum(),
            movement.y.signum(),
            movement.z.signum(),
        );
        let _ = self.try_set_position(hit.location - offset_direction * BLOCK_HIT_STEP_BACK);

        self.set_velocity(DVec3::ZERO);
        self.play_sound(
            &sound_events::ENTITY_ARROW_HIT,
            1.0,
            1.2 / 0.2f32.mul_add(rand::random::<f32>(), 0.9),
        );
        self.set_in_ground(true);
        self.set_crit_arrow(false);
        self.set_pierce_level(0);
        {
            let mut state = self.state.lock();
            state.life = 0;
            state.shake_time = SHAKE_TIME;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use foton_registry::{init_vanilla_registry, vanilla_blocks, vanilla_entities};
    use foton_utils::types::UpdateFlags;
    use foton_utils::{BlockPos, ChunkPos};
    use glam::DVec3;

    use crate::behavior::init_behaviors;
    use crate::entity::entities::PigEntity;
    use crate::entity::{Entity, LivingEntity, RemovalReason, SharedEntity, next_entity_id};
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
    use crate::world::World;

    use super::{ArrowEntity, SHAKE_TIME};

    /// The block face an arrow is fired at in the landing tests.
    const WALL_X: i32 = 12;

    fn arrow_world(key: &'static str) -> Arc<World> {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world(key);
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        world
    }

    fn spawn_arrow(world: &Arc<World>, position: DVec3, velocity: DVec3) -> Arc<ArrowEntity> {
        let arrow = Arc::new(ArrowEntity::new(
            &vanilla_entities::ARROW,
            next_entity_id(),
            position,
            Arc::downgrade(world),
        ));
        arrow.set_velocity(velocity);
        world
            .try_add_entity(Arc::clone(&arrow) as SharedEntity)
            .expect("the test chunk is loaded, so the arrow should attach");
        arrow
    }

    fn spawn_pig(world: &Arc<World>, position: DVec3) -> Arc<PigEntity> {
        let pig = Arc::new(PigEntity::new(
            &vanilla_entities::PIG,
            next_entity_id(),
            position,
            Arc::downgrade(world),
        ));
        world
            .try_add_entity(Arc::clone(&pig) as SharedEntity)
            .expect("the test chunk is loaded, so the pig should attach");
        pig
    }

    /// Flies an arrow into whatever is in front of it, one tick at a time,
    /// through `Entity::tick` -- the door the world uses.
    fn fly_until_it_lands(arrow: &ArrowEntity) {
        for _ in 0..40 {
            Entity::tick(arrow);
            if arrow.is_in_ground() || arrow.is_removed() {
                return;
            }
        }
        panic!("the arrow never hit anything");
    }

    #[test]
    fn an_arrow_stays_stuck_in_the_mob_it_hit() {
        let world = arrow_world("arrow_sticks_in_mob");
        let pig = spawn_pig(&world, DVec3::new(10.5, 64.0, 8.5));
        let arrow = spawn_arrow(
            &world,
            DVec3::new(8.5, 64.9, 8.5),
            DVec3::new(1.0, 0.0, 0.0),
        );

        assert_eq!(pig.arrow_count(), 0, "the pig starts clean");
        fly_until_it_lands(&arrow);

        assert_eq!(
            pig.arrow_count(),
            1,
            "the arrow should be left sticking out of the pig"
        );
        assert_eq!(arrow.removal_reason(), Some(RemovalReason::Discarded));
    }

    #[test]
    fn arrows_work_their_way_back_out_of_a_mob() {
        let world = arrow_world("arrow_falls_out_of_mob");
        let pig = spawn_pig(&world, DVec3::new(8.5, 64.0, 8.5));
        // Vanilla's timer is `20 * (30 - arrowCount)`, so a mob carrying the
        // full thirty sheds one on the very next tick.
        pig.set_arrow_count(30);

        Entity::tick(pig.as_ref());

        assert_eq!(
            pig.arrow_count(),
            29,
            "the living tick should let arrows fall back out"
        );
    }

    #[test]
    fn an_arrow_settles_against_the_face_it_struck() {
        let world = arrow_world("arrow_settles_against_block");
        assert!(world.set_block(
            BlockPos::new(WALL_X, 64, 8),
            vanilla_blocks::STONE.default_state(),
            UpdateFlags::UPDATE_NONE,
        ));
        let arrow = spawn_arrow(
            &world,
            DVec3::new(8.5, 64.5, 8.5),
            DVec3::new(1.0, 0.0, 0.0),
        );

        fly_until_it_lands(&arrow);

        assert!(arrow.is_in_ground(), "the arrow should have stuck");
        let overshoot = f64::from(WALL_X) - arrow.position().x;
        assert!(
            (overshoot - 0.05).abs() < 1.0e-6,
            "the arrow should settle a twentieth of a block off the face it hit, \
             but it stopped {overshoot} short of it"
        );
    }

    #[test]
    fn a_landed_arrow_still_shakes_before_it_can_be_collected() {
        let world = arrow_world("arrow_shakes_before_pickup");
        assert!(world.set_block(
            BlockPos::new(WALL_X, 64, 8),
            vanilla_blocks::STONE.default_state(),
            UpdateFlags::UPDATE_NONE,
        ));
        let arrow = spawn_arrow(
            &world,
            DVec3::new(8.5, 64.5, 8.5),
            DVec3::new(1.0, 0.0, 0.0),
        );

        fly_until_it_lands(&arrow);

        assert_eq!(
            arrow.shake_time(),
            SHAKE_TIME,
            "landing should start vanilla's wobble"
        );
        for _ in 0..SHAKE_TIME {
            Entity::tick(arrow.as_ref());
        }
        assert_eq!(arrow.shake_time(), 0, "the wobble should run out");
    }

    #[test]
    fn a_critical_arrow_sometimes_hits_harder_than_a_plain_one() {
        let world = arrow_world("arrow_crit_bonus");
        let plain_pig = spawn_pig(&world, DVec3::new(10.5, 64.0, 8.5));
        let plain_before = plain_pig.get_health();
        let plain_arrow = spawn_arrow(
            &world,
            DVec3::new(8.5, 64.9, 8.5),
            DVec3::new(1.0, 0.0, 0.0),
        );
        fly_until_it_lands(&plain_arrow);
        let plain_damage = plain_before - plain_pig.get_health();
        assert!(plain_damage > 0.0, "the plain arrow did nothing");

        // The bonus is `random(0, damage / 2 + 2)`, so it can roll zero. Twenty
        // shots that all roll zero is a one-in-three-billion event.
        for lane_chunk in 1..=3 {
            insert_ready_full_chunk(&world, ChunkPos::new(0, lane_chunk));
        }
        let mut best = 0.0f32;
        for lane in 0..20 {
            let z = 17.5 + f64::from(lane) * 2.0;
            let pig = spawn_pig(&world, DVec3::new(10.5, 64.0, z));
            let before = pig.get_health();
            let arrow = spawn_arrow(&world, DVec3::new(8.5, 64.9, z), DVec3::new(1.0, 0.0, 0.0));
            arrow.set_crit_arrow(true);
            fly_until_it_lands(&arrow);
            best = best.max(before - pig.get_health());
        }

        assert!(
            best > plain_damage,
            "a critical arrow should be able to beat a plain one's {plain_damage}, \
             but the best of twenty was {best}"
        );
    }

    #[test]
    fn a_burning_arrow_sets_its_target_alight() {
        let world = arrow_world("arrow_sets_target_alight");
        let pig = spawn_pig(&world, DVec3::new(10.5, 64.0, 8.5));
        let arrow = spawn_arrow(
            &world,
            DVec3::new(8.5, 64.9, 8.5),
            DVec3::new(1.0, 0.0, 0.0),
        );
        arrow.set_remaining_fire_ticks(20);

        fly_until_it_lands(&arrow);

        assert!(
            pig.remaining_fire_ticks() > 0,
            "a burning arrow should set what it hits on fire"
        );
    }
}
