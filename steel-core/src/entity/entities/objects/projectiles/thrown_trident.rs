//! Thrown trident entity.
//!
//! Vanilla parity: `ThrownTrident`, which extends `AbstractArrow`. A thrown
//! trident flies like an arrow, deals a flat 8 damage to the first thing it
//! touches, sticks into the block it lands in, and, when the trident carries
//! loyalty, flies back to whoever threw it until they walk into it.
//!
//! The arrow half is modeled on the sibling [`super::ArrowEntity`], which ports
//! the parts of `AbstractArrow` Steel supports. Piercing, crit arrows and the
//! `inBlockState`/`startFalling` shake-loose behavior are not ported there and
//! are not ported here either; none of them apply to a trident.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_entity_data::TridentEntityData;
use steel_registry::{sound_events, vanilla_damage_types, vanilla_entities, vanilla_items};
use steel_utils::locks::SyncMutex;
use steel_utils::{DowncastType, DowncastTypeKey};
use uuid::Uuid;

use crate::enchantment_helper::{self, EnchantmentDamageContext, EnchantmentPostAttackContext};
use crate::entity::damage::DamageSource;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, Projectile, ProjectileBase,
    ProjectileDeflection, ProjectileHit, RemovalReason, SharedEntity, next_entity_id,
};
use crate::inventory::container::Container as _;
use crate::physics::MoverType;
use crate::player::Player;
use crate::world::{ClipBlockShape, ClipFluid, ClipHitResult, World};

/// Flat damage a trident deals, before enchantments.
///
/// Vanilla parity: the `8.0F` literal in `ThrownTrident.onHitEntity`. Unlike an
/// arrow the trident does not scale its damage with flight speed.
const BASE_DAMAGE: f32 = 8.0;

/// Vanilla parity: `AbstractArrow.getDefaultGravity`.
const DEFAULT_GRAVITY: f64 = 0.05;

/// Velocity kept each tick.
///
/// Vanilla parity: `AbstractArrow.getAirDrag` is 0.99 and
/// `ThrownTrident.getWaterInertia` overrides the arrow's 0.6 back up to the same
/// 0.99, so a trident loses speed at one rate whether it is in air or in water.
const DRAG: f64 = 0.99;

/// Ticks a stuck trident survives before it disappears.
///
/// Vanilla parity: the 1200-tick limit in `AbstractArrow.tickDespawn`.
const DESPAWN_TICKS: i32 = 1200;

/// Ticks a freshly landed trident cannot be picked up for.
///
/// Vanilla parity: `AbstractArrow.SHAKE_TIME`.
const SHAKE_TIME: i32 = 7;

/// Ticks in the ground after which the trident counts as spent.
///
/// Vanilla parity: the `inGroundTime > 4` guard at the top of `ThrownTrident.tick`.
const SPENT_IN_GROUND_TIME: i32 = 4;

/// Who, if anyone, may pick this trident back up.
///
/// Vanilla parity: `AbstractArrow.Pickup`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TridentPickup {
    /// Nobody can collect it.
    Disallowed,
    /// Anyone who touches it collects it.
    Allowed,
    /// Only a creative-mode player may collect it, and collecting adds nothing.
    CreativeOnly,
}

/// State that is not mirrored to clients.
struct TridentState {
    /// Whether the trident has already spent its hit.
    ///
    /// Vanilla parity: `ThrownTrident.dealtDamage`.
    dealt_damage: bool,
    /// Ticks spent stuck in a block.
    ///
    /// Vanilla parity: `AbstractArrow.life`.
    life: i32,
    /// Consecutive ticks spent stuck in a block.
    ///
    /// Vanilla parity: `AbstractArrow.inGroundTime`.
    in_ground_time: i32,
    /// Ticks left of the post-landing wobble that blocks pickup.
    ///
    /// Vanilla parity: `AbstractArrow.shakeTime`.
    shake_time: i32,
    /// Who may collect the trident.
    pickup: TridentPickup,
    /// The stack handed back on pickup, and the weapon whose enchantments apply.
    ///
    /// Vanilla parity: `AbstractArrow.pickupItemStack`, which `ThrownTrident`
    /// also returns from `getWeaponItem`.
    pickup_item_stack: ItemStack,
    /// Whether the return sound has already played for this flight home.
    ///
    /// Vanilla parity: the `clientSideReturnTridentTickCount == 0` gate.
    played_return_sound: bool,
}

impl TridentState {
    fn new() -> Self {
        Self {
            dealt_damage: false,
            life: 0,
            in_ground_time: 0,
            shake_time: 0,
            pickup: TridentPickup::Disallowed,
            pickup_item_stack: ItemStack::new(&vanilla_items::TRIDENT),
            played_return_sound: false,
        }
    }
}

/// A trident in flight.
#[entity_behavior(class = "ThrownTrident")]
pub struct ThrownTridentEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<TridentEntityData>,
    projectile_base: ProjectileBase,
    state: SyncMutex<TridentState>,
}

// SAFETY: This Steel-owned key uniquely identifies `ThrownTridentEntity`.
unsafe impl DowncastType for ThrownTridentEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/thrown_trident");
}

impl ThrownTridentEntity {
    /// Creates a trident at `position`.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(TridentEntityData::new()),
            projectile_base: ProjectileBase::new(),
            state: SyncMutex::new(TridentState::new()),
        }
    }

    /// Creates a trident from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(TridentEntityData::new()),
            projectile_base: ProjectileBase::new(),
            state: SyncMutex::new(TridentState::new()),
        }
    }

    /// Throws `trident_item` from `thrower` and adds the trident to the world.
    ///
    /// Vanilla parity: `TridentItem.releaseUsing` calling
    /// `Projectile.spawnProjectileFromRotation(ThrownTrident::new, ...)`, which
    /// spawns at the thrower's eyes less 0.1, aims along their look, hands the
    /// entity to the level and then runs `EnchantmentHelper.onProjectileSpawned`.
    pub fn throw_from(
        world: &Arc<World>,
        thrower: &dyn Entity,
        trident_item: &ItemStack,
        power: f32,
        uncertainty: f32,
    ) -> Arc<Self> {
        let position = thrower.position().with_y(thrower.get_eye_y() - 0.1);
        let trident = Arc::new(Self::new(
            &vanilla_entities::TRIDENT,
            next_entity_id(),
            position,
            Arc::downgrade(world),
        ));
        trident.set_pickup_item_stack(trident_item.copy_with_count(1));
        trident.set_owner(thrower);

        let (yaw, pitch) = thrower.rotation();
        trident.shoot_from_rotation(thrower, pitch, yaw, 0.0, power, uncertainty);

        if let Err(error) = world.try_add_entity(Arc::clone(&trident) as Arc<dyn Entity>) {
            log::error!("failed to add thrown trident entity: {error}");
        }

        // Vanilla `Projectile.spawnProjectile` runs the weapon's
        // `projectile_spawned` effects once the entity is in the world. The
        // trident is its own weapon, so the copy handed in here is discarded --
        // no vanilla enchantment on a trident damages the weapon from this hook.
        let mut weapon = trident_item.copy_with_count(trident_item.count());
        enchantment_helper::on_projectile_spawned(
            world,
            &mut weapon,
            trident.as_ref(),
            Some(thrower),
        );

        trident
    }

    /// Sets the owner and applies the vanilla pickup promotion.
    ///
    /// Vanilla parity: `AbstractArrow.setOwner`, which turns a `DISALLOWED`
    /// pickup into `ALLOWED` when a player throws the projectile.
    fn set_owner(&self, owner: &dyn Entity) {
        self.set_owner_uuid(Some(owner.uuid()));
        let mut state = self.state.lock();
        if owner.entity_type() == &vanilla_entities::PLAYER
            && state.pickup == TridentPickup::Disallowed
        {
            state.pickup = TridentPickup::Allowed;
        }
    }

    /// Returns who may collect this trident.
    #[must_use]
    pub fn pickup(&self) -> TridentPickup {
        self.state.lock().pickup
    }

    /// Sets who may collect this trident.
    ///
    /// Vanilla parity: `TridentItem.releaseUsing` marks a creative throw
    /// `CREATIVE_ONLY` so it is never handed back to the inventory.
    pub fn set_pickup(&self, pickup: TridentPickup) {
        self.state.lock().pickup = pickup;
    }

    /// Returns the stack this trident hands back, and the weapon its
    /// enchantments are read from.
    ///
    /// Vanilla parity: `AbstractArrow.getPickupItem` /
    /// `ThrownTrident.getWeaponItem`.
    #[must_use]
    pub fn pickup_item_stack(&self) -> ItemStack {
        let state = self.state.lock();
        state
            .pickup_item_stack
            .copy_with_count(state.pickup_item_stack.count())
    }

    /// Stores the thrown stack and refreshes the loyalty/foil synced values.
    ///
    /// Vanilla parity: the `ThrownTrident` constructor, which sets `ID_LOYALTY`
    /// from `getLoyaltyFromItem` and `ID_FOIL` from `ItemStack.hasFoil`.
    fn set_pickup_item_stack(&self, stack: ItemStack) {
        let loyalty = enchantment_helper::get_trident_return_to_owner_acceleration(&stack);
        // Vanilla clamps to a signed byte before it reaches the synced value.
        let loyalty = loyalty.clamp(0, i32::from(i8::MAX)) as i8;
        // Vanilla `ItemStack.hasFoil` also honors the `enchantment_glint_override`
        // component; Steel has no such component yet, so any enchantment glints.
        let foil = stack
            .get_enchantments()
            .is_some_and(|enchantments| enchantments.iter().any(|(_, level)| *level > 0));

        self.state.lock().pickup_item_stack = stack;
        let mut entity_data = self.entity_data.lock();
        entity_data.thrown_trident.id_loyalty.set(loyalty);
        entity_data.thrown_trident.id_foil.set(foil);
    }

    /// Returns the loyalty acceleration this trident returns home with.
    ///
    /// Vanilla parity: the `ID_LOYALTY` synced value.
    #[must_use]
    pub fn loyalty(&self) -> i32 {
        i32::from(*self.entity_data.lock().thrown_trident.id_loyalty.get())
    }

    /// Returns whether the trident has already spent its hit.
    #[must_use]
    pub fn dealt_damage(&self) -> bool {
        self.state.lock().dealt_damage
    }

    /// Returns whether the trident is stuck in a block.
    #[must_use]
    pub fn is_in_ground(&self) -> bool {
        *self
            .entity_data
            .lock()
            .thrown_trident
            .abstract_arrow
            .in_ground
            .get()
    }

    fn set_in_ground(&self, in_ground: bool) {
        self.entity_data
            .lock()
            .thrown_trident
            .abstract_arrow
            .in_ground
            .set(in_ground);
    }

    /// Vanilla parity: `ThrownTrident.isAcceptibleReturnOwner`.
    ///
    /// A dead owner, or a spectating player, is not somewhere the trident will
    /// fly back to; it drops instead.
    fn is_acceptible_return_owner(owner: &SharedEntity) -> bool {
        if !owner.is_alive() {
            return false;
        }
        owner
            .as_player()
            .is_none_or(|player| !player.is_spectator())
    }

    /// Counts down the lifetime of a stuck trident.
    ///
    /// Vanilla parity: `ThrownTrident.tickDespawn`, which skips
    /// `AbstractArrow.tickDespawn` entirely for a collectable loyalty trident so
    /// it waits for its owner however long that takes.
    fn tick_despawn(&self) {
        if self.pickup() == TridentPickup::Allowed && self.loyalty() > 0 {
            return;
        }

        let expired = {
            let mut state = self.state.lock();
            state.life += 1;
            state.life >= DESPAWN_TICKS
        };
        if expired {
            self.set_removed(RemovalReason::Discarded);
        }
    }

    /// Runs the loyalty return leg of `ThrownTrident.tick`.
    ///
    /// Returns `false` when the trident removed itself and the arrow tick must
    /// not run.
    fn tick_loyalty_return(&self) -> bool {
        if self.in_ground_time() > SPENT_IN_GROUND_TIME {
            self.state.lock().dealt_damage = true;
        }

        let loyalty = self.loyalty();
        if loyalty <= 0 || !(self.dealt_damage() || self.no_physics()) {
            return true;
        }
        let Some(owner) = self.get_owner() else {
            return true;
        };

        if !Self::is_acceptible_return_owner(&owner) {
            if self.pickup() == TridentPickup::Allowed {
                self.spawn_at_location(self.pickup_item_stack(), 0.1);
            }
            self.set_removed(RemovalReason::Discarded);
            // Deviation: vanilla falls through to `super.tick()` after
            // discarding. Steel's removal is immediate, so ticking the arrow
            // half of a discarded entity would only move a corpse.
            return false;
        }

        // A non-player owner swallows the trident rather than picking it up:
        // there is no inventory to put it in.
        let owner_position = owner.position();
        let owner_eye = DVec3::new(owner_position.x, owner.get_eye_y(), owner_position.z);
        if owner.as_player().is_none()
            && self.position().distance(owner_eye)
                < f64::from(owner.entity_type().dimensions.width) + 1.0
        {
            self.set_removed(RemovalReason::Discarded);
            return false;
        }

        self.set_no_physics(true);
        let to_owner = owner_eye - self.position();
        // Vanilla `setPosRaw` lifts the trident toward the owner's eyes without
        // moving it horizontally, so the arc home stays flat.
        let position = self.position();
        if let Err(error) = self.try_set_position(DVec3::new(
            position.x,
            to_owner.y.mul_add(0.015 * f64::from(loyalty), position.y),
            position.z,
        )) {
            log::debug!("thrown trident {} could not climb home: {error}", self.id());
        }

        let acceleration = 0.05 * f64::from(loyalty);
        self.set_velocity(self.velocity() * 0.95 + to_owner.normalize_or_zero() * acceleration);
        self.mark_velocity_sync();

        let mut state = self.state.lock();
        let should_play = !state.played_return_sound;
        state.played_return_sound = true;
        drop(state);
        if should_play {
            self.play_sound(&sound_events::ITEM_TRIDENT_RETURN, 10.0, 1.0);
        }

        true
    }

    /// Returns the number of consecutive ticks spent stuck in a block.
    fn in_ground_time(&self) -> i32 {
        self.state.lock().in_ground_time
    }

    /// Casts the move vector for this tick.
    ///
    /// Vanilla parity: `ProjectileUtil.getHitResultOnMoveVector`, narrowed by
    /// `ThrownTrident.findHitEntity` returning null once the trident has spent
    /// its hit -- a returning trident passes through mobs but still lands in a
    /// block it runs into.
    fn hit_result(&self) -> Option<ProjectileHit> {
        if !self.dealt_damage() {
            return self.get_hit_result_on_move_vector();
        }

        let world = self.level()?;
        let from = self.position();
        let to = from + self.velocity();
        let hit = world.clip_including_border(from, to, ClipBlockShape::Collider, ClipFluid::None);
        if hit.is_miss() {
            return None;
        }
        Some(ProjectileHit::Block {
            location: hit.location,
            hit,
        })
    }

    /// Points the trident along its flight, smoothed the way the client expects.
    ///
    /// Vanilla parity: the rotation block of `AbstractArrow.tick`, including its
    /// sign flip while `noPhysics` is set so a returning trident flies point-first
    /// at its owner.
    fn update_arrow_rotation(&self, physics_enabled: bool) {
        let movement = self.velocity();
        let horizontal = movement.x.hypot(movement.z);
        let (yaw_x, yaw_z) = if physics_enabled {
            (movement.x, movement.z)
        } else {
            (-movement.x, -movement.z)
        };
        let (old_yaw, old_pitch) = self.rotation();
        let yaw = lerp_rotation(old_yaw, yaw_x.atan2(yaw_z).to_degrees() as f32);
        let pitch = lerp_rotation(old_pitch, movement.y.atan2(horizontal).to_degrees() as f32);
        self.set_rotation((yaw, pitch));
    }

    /// Runs the `AbstractArrow.tick` half of the trident's tick.
    fn abstract_arrow_tick(&self) {
        let physics_enabled = !self.no_physics();

        // Vanilla clears fire on a rained-on or submerged arrow before anything else.
        if self.is_in_water_or_rain() {
            self.clear_fire();
        }

        {
            let mut state = self.state.lock();
            if state.shake_time > 0 {
                state.shake_time -= 1;
            }
        }

        if self.is_in_ground() && physics_enabled {
            // Deviation: vanilla also checks `shouldFall`, which shakes an arrow
            // loose when the block it is stuck in disappears. Steel's arrow does
            // not track `inBlockState` either, so a stuck trident stays put until
            // it despawns or is collected.
            self.tick_despawn();
            self.state.lock().in_ground_time += 1;
            if self.is_alive() {
                self.apply_effects_from_blocks();
            }
            return;
        }

        self.state.lock().in_ground_time = 0;
        self.update_arrow_rotation(physics_enabled);
        self.check_left_owner();

        if physics_enabled && let Some(hit) = self.hit_result() {
            self.hit_target_or_deflect_self(&hit);
            if !self.is_alive() {
                return;
            }
        }

        let _ = self.move_entity(MoverType::SelfMovement, self.velocity());
        self.apply_effects_from_blocks();
        self.set_velocity(self.velocity() * DRAG);
        if physics_enabled && !self.is_in_ground() {
            self.apply_gravity();
        }
        // Vanilla parity: the `super.tick()` that closes `AbstractArrow.tick`'s
        // in-flight branch, which is `Projectile.tick`.
        self.projectile_base_tick();
    }

    /// Tries to hand the trident to `player`.
    ///
    /// Vanilla parity: `ThrownTrident.tryPickup`, which extends
    /// `AbstractArrow.tryPickup` with the mid-air catch a loyalty trident makes
    /// as it reaches its owner.
    fn try_pickup(&self, player: &Arc<Player>) -> bool {
        let pickup = self.pickup();
        let collected = match pickup {
            TridentPickup::Disallowed => false,
            TridentPickup::Allowed => self.add_to_inventory(player),
            TridentPickup::CreativeOnly => player.has_infinite_materials(),
        };

        collected
            || (self.no_physics()
                && self.owned_by(player.as_ref())
                && self.add_to_inventory(player))
    }

    fn add_to_inventory(&self, player: &Arc<Player>) -> bool {
        let mut stack = self.pickup_item_stack();
        player.inventory.lock().add(&mut stack)
    }
}

/// Java's `Math.signum`, which returns zero for zero.
///
/// Rust's `f64::signum` returns 1.0 for `+0.0`, which would nudge a trident that
/// landed with no motion left on an axis.
fn java_signum(value: f64) -> f64 {
    if value == 0.0 { 0.0 } else { value.signum() }
}

/// Vanilla `Mth.lerp(0.2, rotO, rot)` after wrapping the old angle into range.
///
/// Vanilla parity: `Projectile.lerpRotation`.
fn lerp_rotation(mut rot_old: f32, rot: f32) -> f32 {
    while rot - rot_old < -180.0 {
        rot_old -= 360.0;
    }
    while rot - rot_old >= 180.0 {
        rot_old += 360.0;
    }
    0.2f32.mul_add(rot - rot_old, rot_old)
}

impl Entity for ThrownTridentEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    /// Vanilla parity: `ThrownTrident.tick`, which runs the return leg and then
    /// delegates to `AbstractArrow.tick`.
    fn tick(&self) {
        if self.tick_loyalty_return() {
            self.abstract_arrow_tick();
        }
    }

    fn get_default_gravity(&self) -> f64 {
        DEFAULT_GRAVITY
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `Projectile.getAddEntityPacket` sends the owner's entity
    /// id so the client can skip rendering the trident inside the thrower.
    fn spawn_data(&self) -> i32 {
        self.get_owner().map_or(0, |owner| owner.id())
    }

    fn restore_owner_reference(&self, owner: &SharedEntity) {
        self.cache_owner_entity(owner);
    }

    fn projectile_owner_uuid(&self) -> Option<Uuid> {
        self.owner_uuid()
    }

    fn projectile_owner(&self) -> Option<SharedEntity> {
        self.get_owner()
    }

    /// Matches the sibling arrow: a projectile the player can walk into has to
    /// take part in collision for `player_touch` to fire.
    fn is_pickable(&self) -> bool {
        true
    }

    /// Lets the thrower collect the trident.
    ///
    /// Vanilla parity: `ThrownTrident.playerTouch` gating
    /// `AbstractArrow.playerTouch` on ownership, so a loyalty trident on its way
    /// home cannot be intercepted by a bystander.
    fn player_touch(self: Arc<Self>, player: &Arc<Player>) {
        if !self.owned_by(player.as_ref()) && self.get_owner().is_some() {
            return;
        }
        if !self.is_in_ground() && !self.no_physics() {
            return;
        }
        if self.state.lock().shake_time > 0 {
            return;
        }

        if self.try_pickup(player) {
            // VANILLA CLIENT-LOCAL: `Player.take` also plays the pickup animation
            // through `ClientboundTakeItemEntityPacket`.
            self.set_removed(RemovalReason::Discarded);
        }
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_projectile(nbt);

        let state = self.state.lock();
        // Vanilla parity: `AbstractArrow.addAdditionalSaveData` plus
        // `ThrownTrident.addAdditionalSaveData`. `inBlockState`, `crit`,
        // `PierceLevel`, `SoundEvent` and `weapon` are not modeled by Steel's
        // arrows, so they are not written.
        nbt.insert("life", state.life as i16);
        nbt.insert("shake", state.shake_time as i8);
        nbt.insert("inGround", self.is_in_ground());
        nbt.insert(
            "pickup",
            match state.pickup {
                TridentPickup::Disallowed => 0i8,
                TridentPickup::Allowed => 1i8,
                TridentPickup::CreativeOnly => 2i8,
            },
        );
        nbt.insert("DealtDamage", state.dealt_damage);
        if !state.pickup_item_stack.is_empty() {
            nbt.insert("item", state.pickup_item_stack.to_nbt_tag_ref());
        }
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_projectile(nbt);

        let item = nbt
            .compound("item")
            .and_then(|tag| ItemStack::from_borrowed_compound(&tag))
            .unwrap_or_else(|| ItemStack::new(&vanilla_items::TRIDENT));
        // Vanilla `ThrownTrident.readAdditionalSaveData` recomputes loyalty from
        // the restored stack rather than saving the synced value.
        self.set_pickup_item_stack(item);
        self.set_in_ground(nbt.byte("inGround").is_some_and(|value| value != 0));

        let mut state = self.state.lock();
        state.life = i32::from(nbt.short("life").unwrap_or(0));
        state.shake_time = i32::from(nbt.byte("shake").unwrap_or(0)) & 255;
        state.dealt_damage = nbt.byte("DealtDamage").is_some_and(|value| value != 0);
        state.pickup = match nbt.byte("pickup").unwrap_or(0) {
            1 => TridentPickup::Allowed,
            2 => TridentPickup::CreativeOnly,
            _ => TridentPickup::Disallowed,
        };
    }
}

impl Projectile for ThrownTridentEntity {
    fn projectile_base(&self) -> &ProjectileBase {
        &self.projectile_base
    }

    /// Vanilla parity: `ThrownTrident.onHitEntity`.
    ///
    /// The trident always bounces off whatever it struck, whether or not the
    /// blow landed, and from then on flies as spent.
    fn on_hit_entity(&self, entity: &SharedEntity, _location: DVec3) {
        let owner = self.get_owner();
        let mut source = DamageSource::environment(&vanilla_damage_types::TRIDENT)
            .with_direct_entity(self.id())
            .with_source_position(self.position());
        if let Some(owner) = &owner {
            source = source.with_causing_entity(owner.id());
        }

        let weapon = self.pickup_item_stack();
        let Some(world) = self.level() else {
            return;
        };

        // Impaling rides on the `damage` effect component, so it lands here.
        let context =
            EnchantmentDamageContext::from_damage_source(&world, entity.entity_type(), &source);
        let damage = enchantment_helper::modify_damage(&weapon, &context, BASE_DAMAGE);

        self.state.lock().dealt_damage = true;
        if entity.hurt(&world, &source, damage) {
            if entity.entity_type() == &vanilla_entities::ENDERMAN {
                // Vanilla leaves `onHitEntity` here. An enderman teleports out of
                // the way, so the trident neither bounces off it nor makes a sound.
                return;
            }

            // Channeling is not reachable: its `post_attack` effect summons a
            // `LightningBolt`, an entity Steel does not implement, so the helper
            // below skips it as an unsupported effect. Every other post-attack
            // effect on the weapon and on the victim's armor still runs.
            let post_attack = EnchantmentPostAttackContext::new(
                entity.as_ref(),
                owner.as_deref(),
                Some(self),
                &source,
            );
            enchantment_helper::do_post_attack_effects_with_item_source(
                &world,
                entity.as_ref(),
                &weapon,
                &post_attack,
            );

            // Vanilla also calls `doKnockback` and `doPostHurtEffects` here. Both
            // are no-ops for a trident: knockback reads `AbstractArrow.firedFromWeapon`,
            // which `ThrownTrident` never sets, and `doPostHurtEffects` is only
            // overridden by tipped arrows.
        }

        self.deflect(
            ProjectileDeflection::Reverse,
            Some(entity.as_ref()),
            self.owner_uuid(),
            owner.as_ref(),
            false,
        );
        self.set_velocity(self.velocity() * DVec3::new(0.02, 0.2, 0.02));
        self.play_sound(&sound_events::ITEM_TRIDENT_HIT, 1.0, 1.0);
    }

    /// Sticks the trident into the block it hit.
    ///
    /// Vanilla parity: `AbstractArrow.onHitBlock` with
    /// `ThrownTrident.getDefaultHitGroundSoundEvent`.
    fn on_hit_block(&self, hit: &ClipHitResult) {
        self.projectile_on_hit_block(hit);

        // Not ported: `ThrownTrident.hitBlockEnchantmentEffects` runs the weapon's
        // `minecraft:hit_block` enchantment effects, and Steel has no application
        // path for that effect component yet.

        // Vanilla backs the arrow out along its own motion so the model sits in
        // the block face rather than inside it.
        let movement = self.velocity();
        let backoff = DVec3::new(
            java_signum(movement.x),
            java_signum(movement.y),
            java_signum(movement.z),
        ) * 0.05;
        if let Err(error) = self.try_set_position(self.position() - backoff) {
            log::debug!("thrown trident {} could not settle: {error}", self.id());
        }

        self.set_velocity(DVec3::ZERO);
        let pitch = 1.2 / 0.2f32.mul_add(rand::random::<f32>(), 0.9);
        self.play_sound(&sound_events::ITEM_TRIDENT_HIT_GROUND, 1.0, pitch);
        self.set_in_ground(true);

        let mut state = self.state.lock();
        state.shake_time = SHAKE_TIME;
        state.life = 0;
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound as read_borrowed_compound;
    use steel_registry::{init_vanilla_registry, vanilla_enchantments};
    use steel_utils::{BlockPos, ChunkPos, Direction};

    use crate::behavior::init_behaviors;
    use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};

    use super::*;

    const THROWER_POSITION: DVec3 = DVec3::new(0.5, 64.0, 0.5);

    fn trident_stack(loyalty: u32) -> ItemStack {
        let mut stack = ItemStack::new(&vanilla_items::TRIDENT);
        if loyalty > 0 {
            stack.set_enchantments(
                &[(vanilla_enchantments::LOYALTY.key.clone(), loyalty)],
                true,
            );
        }
        stack
    }

    fn world_with_a_loaded_chunk(key: &'static str) -> Arc<World> {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world(key);
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        world
    }

    fn thrower(world: &Arc<World>, name: &'static str, id: i32) -> Arc<Player> {
        let player = TestPlayerBuilder::new(Arc::clone(world), name, id).build();
        player.base().set_position_local(THROWER_POSITION);
        player
    }

    /// Builds a trident already owned by `owner`, without going through the item
    /// so the test can place it exactly where it wants it.
    fn planted_trident(
        world: &Arc<World>,
        owner: &Arc<Player>,
        position: DVec3,
        loyalty: u32,
    ) -> Arc<ThrownTridentEntity> {
        let trident = Arc::new(ThrownTridentEntity::new(
            &vanilla_entities::TRIDENT,
            next_entity_id(),
            position,
            Arc::downgrade(world),
        ));
        trident.set_pickup_item_stack(trident_stack(loyalty));
        trident.set_pickup(TridentPickup::Allowed);
        let owner: SharedEntity = owner.clone();
        trident.set_owner_entity(Some(&owner));
        trident
    }

    fn land(trident: &ThrownTridentEntity) {
        let position = trident.position();
        trident.on_hit_block(&ClipHitResult {
            location: position,
            direction: Direction::Up,
            block_pos: BlockPos::containing(position.x, position.y - 1.0, position.z),
            miss: false,
            inside: false,
            world_border_hit: false,
        });
    }

    fn holds_a_trident(player: &Arc<Player>) -> Option<ItemStack> {
        let inventory = player.inventory.lock();
        (0..inventory.get_container_size())
            .map(|slot| inventory.get_item(slot).clone())
            .find(|item| item.is(&vanilla_items::TRIDENT))
    }

    #[test]
    fn a_thrown_trident_belongs_to_the_thrower_and_keeps_its_enchantments() {
        let world = world_with_a_loaded_chunk("trident_throw_owner");
        let player = thrower(&world, "TridentThrower", 1);

        let trident =
            ThrownTridentEntity::throw_from(&world, player.as_ref(), &trident_stack(3), 2.5, 1.0);

        assert_eq!(trident.owner_uuid(), Some(player.uuid()));
        assert_eq!(trident.pickup(), TridentPickup::Allowed);
        assert_eq!(
            trident
                .pickup_item_stack()
                .get_enchantment_level(&vanilla_enchantments::LOYALTY.key),
            3
        );
        assert!(
            (trident.position().y - (player.get_eye_y() - 0.1)).abs() < 1.0e-9,
            "vanilla spawns the trident a tenth of a block below the thrower's eyes"
        );
        let speed = trident.velocity().length();
        assert!(
            (speed - 2.5).abs() < 0.2,
            "expected a 2.5 throw, got {speed}"
        );
    }

    #[test]
    fn loyalty_on_the_thrown_stack_becomes_the_return_acceleration() {
        let world = world_with_a_loaded_chunk("trident_throw_loyalty");
        let player = thrower(&world, "TridentThrower", 1);

        let plain =
            ThrownTridentEntity::throw_from(&world, player.as_ref(), &trident_stack(0), 2.5, 1.0);
        let loyal =
            ThrownTridentEntity::throw_from(&world, player.as_ref(), &trident_stack(2), 2.5, 1.0);

        assert_eq!(plain.loyalty(), 0);
        assert_eq!(loyal.loyalty(), 2);
    }

    #[test]
    fn a_spent_loyalty_trident_turns_around_and_flies_home() {
        let world = world_with_a_loaded_chunk("trident_returns_home");
        let player = thrower(&world, "TridentThrower", 1);
        let trident = planted_trident(&world, &player, DVec3::new(8.5, 64.0, 0.5), 2);

        land(&trident);
        assert!(trident.is_in_ground());

        // Vanilla counts the trident as spent once it has sat in the ground for
        // more than four ticks, and only then does loyalty pull it back.
        for _ in 0..=SPENT_IN_GROUND_TIME + 1 {
            trident.tick();
        }

        assert!(trident.dealt_damage());
        assert!(
            trident.no_physics(),
            "a returning trident passes through the world"
        );
        assert!(
            trident.velocity().x < 0.0,
            "the thrower is west of the trident, so it should head west"
        );
    }

    #[test]
    fn a_spent_trident_without_loyalty_stays_where_it_landed() {
        let world = world_with_a_loaded_chunk("trident_stays_put");
        let player = thrower(&world, "TridentThrower", 1);
        let trident = planted_trident(&world, &player, DVec3::new(8.5, 64.0, 0.5), 0);

        land(&trident);
        let landed_at = trident.position();
        for _ in 0..=SPENT_IN_GROUND_TIME + 1 {
            trident.tick();
        }

        assert!(!trident.no_physics());
        assert!(trident.is_in_ground());
        assert_eq!(trident.position(), landed_at);
    }

    #[test]
    fn a_collectable_loyalty_trident_waits_for_its_owner_forever() {
        let world = world_with_a_loaded_chunk("trident_despawn");
        let player = thrower(&world, "TridentThrower", 1);
        let loyal = planted_trident(&world, &player, DVec3::new(8.5, 64.0, 0.5), 1);
        let plain = planted_trident(&world, &player, DVec3::new(9.5, 64.0, 0.5), 0);

        for _ in 0..DESPAWN_TICKS {
            loyal.tick_despawn();
            plain.tick_despawn();
        }

        assert!(!loyal.is_removed());
        assert!(plain.is_removed());
    }

    #[test]
    fn only_the_thrower_picks_a_landed_trident_back_up() {
        let world = world_with_a_loaded_chunk("trident_pickup");
        let player = thrower(&world, "TridentThrower", 1);
        let stranger = thrower(&world, "Bystander", 2);
        let trident = planted_trident(&world, &player, DVec3::new(0.5, 64.5, 0.5), 3);

        land(&trident);
        // Vanilla blocks pickup for the seven-tick landing wobble.
        trident.state.lock().shake_time = 0;

        Arc::clone(&trident).player_touch(&stranger);
        assert!(!trident.is_removed());
        assert!(holds_a_trident(&stranger).is_none());

        Arc::clone(&trident).player_touch(&player);
        assert!(trident.is_removed());
        let collected = holds_a_trident(&player).expect("the thrower gets the trident back");
        assert_eq!(
            collected.get_enchantment_level(&vanilla_enchantments::LOYALTY.key),
            3,
            "the trident comes back with the enchantments it left with"
        );
    }

    #[test]
    fn saving_and_loading_keeps_the_stack_the_spent_flag_and_the_landing() {
        let world = world_with_a_loaded_chunk("trident_save");
        let player = thrower(&world, "TridentThrower", 1);
        let trident = planted_trident(&world, &player, DVec3::new(8.5, 64.0, 0.5), 3);
        land(&trident);
        trident.state.lock().dealt_damage = true;

        let mut nbt = NbtCompound::new();
        trident.save_additional(&mut nbt);
        assert_eq!(nbt.byte("DealtDamage"), Some(1));
        assert_eq!(nbt.byte("pickup"), Some(1));

        let mut bytes = Vec::new();
        nbt.write(&mut bytes);
        let borrowed = read_borrowed_compound(&mut Cursor::new(bytes.as_slice()))
            .unwrap_or_else(|error| panic!("test nbt should reborrow: {error}"));
        let loaded = Arc::new(ThrownTridentEntity::new(
            &vanilla_entities::TRIDENT,
            next_entity_id(),
            DVec3::ZERO,
            Arc::downgrade(&world),
        ));
        loaded.load_additional((&borrowed).into());

        assert!(loaded.dealt_damage());
        assert!(loaded.is_in_ground());
        assert_eq!(loaded.pickup(), TridentPickup::Allowed);
        assert_eq!(loaded.owner_uuid(), Some(player.uuid()));
        // Vanilla recomputes loyalty from the reloaded stack rather than saving it.
        assert_eq!(loaded.loyalty(), 3);
    }
}
