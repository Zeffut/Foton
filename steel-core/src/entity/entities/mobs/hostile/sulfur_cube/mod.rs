//! Sulfur cube entity.
//!
//! Vanilla parity: `net.minecraft.world.entity.monster.cubemob.SulfurCube`. It
//! shares [`super::cube_common`] with the slime and the magma cube, and almost
//! nothing else: it hurts nobody by touch, it has no targets, and it is a
//! `Bucketable` `Shearable` animal in every way that matters.
//!
//! What it is really about is the block in its body. Swallow one and the cube
//! stops moving under its own power and becomes a thing you kick around: every
//! archetype whose item tag holds that block is applied at once, and between
//! them they decide how it bounces, how far it slides, whether it floats,
//! whether touching it burns, and whether it is a walking TNT block. Shears get
//! the block back out.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::{CTakeItemEntity, SoundSource};
use steel_registry::data_components::components::SulfurCubeContent;
use steel_registry::data_components::vanilla_components;
use steel_registry::entity_data::EntityPose;
use steel_registry::entity_type::{EntityAttachment, EntityDimensions, EntityTypeRef};
use steel_registry::item_stack::ItemStack;
use steel_registry::item_stack_template::ItemStackTemplate;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::sulfur_cube_archetype::{
    DEFAULT_KNOCKBACK_MODIFIERS, DEFAULT_SOUND_SETTINGS, SulfurCubeArchetypeRef,
    SulfurCubeContactDamage, SulfurCubeExplosion, SulfurCubeKnockbackModifiers,
    SulfurCubeSoundSettings,
};
use steel_registry::vanilla_attributes;
use steel_registry::vanilla_damage_type_tags::DamageTypeTag;
use steel_registry::vanilla_entity_data::SulfurCubeEntityData;
use steel_registry::vanilla_game_rules::{MOB_GRIEFING, TNT_EXPLODES, TNT_EXPLOSION_DROP_DECAY};
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_game_events, vanilla_items,
};
use steel_utils::locks::SyncMutex;
use steel_utils::random::xoroshiro::Xoroshiro;
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, ChunkPos, Downcast as _, DowncastType, DowncastTypeKey};

use crate::behavior::InteractionResult;
use crate::entity::ai::goal::{Goal, GoalControls, TemptGoal, TemptNavigation};
use crate::entity::attribute::AttributeModifier;
use crate::entity::bucketable::{
    Bucketable, bucket_mob_pickup, load_default_data_from_bucket_tag, read_bucket_entity_data,
    save_default_data_to_bucket_tag, set_bucket_entity_data,
};
use crate::entity::damage::DamageSource;
use crate::entity::entities::{ItemEntity, PrimedTntEntity};
use crate::entity::mob::rotlerp;
use crate::entity::{
    AgeableMob, AgeableMobBase, Entity, EntityBase, EntityBaseLoad, EntitySpawnReason,
    EntitySyncedData, LivingEntity, LivingEntityBase, Mob, MobBase, MoveResult, PathfinderMob,
    RemovalReason, SharedEntity, SpawnGroupData, next_entity_id,
};
use crate::inventory::equipment::EquipmentSlot;
use crate::player::Player;
use crate::world::explosion::{ExplosionBlockInteraction, ExplosionSpec};
use crate::world::game_event::GameEventContext;
use crate::world::{SignalGetter as _, World};

use super::cube_common::{
    self, CubeFloatGoal, CubeHooks, CubeKeepOnJumpingGoal, CubeLike, CubeRandomDirectionGoal,
    CubeState,
};

/// Ticks a sheared cube refuses to pick anything up for.
///
/// Vanilla parity: `SulfurCube.PICKUP_TIMER_DURATION`, five seconds, which is
/// what stops a sheared cube from swallowing its own block straight back.
const PICKUP_TIMER_DURATION: i32 = 100;

/// How many children a killed cube leaves.
///
/// Vanilla parity: `SulfurCube.SPLIT_COUNT`, always two rather than the
/// two-to-four the other cubes roll.
const SPLIT_COUNT: i32 = 2;

/// Size a grown sulfur cube is.
///
/// Vanilla parity: `SulfurCube.MAX_SIZE`. Unlike a slime, a sulfur cube has
/// exactly two sizes and the small one is the baby.
const ADULT_SIZE: i32 = 2;

/// Size a baby sulfur cube is.
///
/// Vanilla parity: `SulfurCube.MIN_SIZE`.
const BABY_SIZE: i32 = 1;

/// Health per size step.
///
/// Vanilla parity: the `4 * actualSize` of `SulfurCube.setcubeMobHealth`, which
/// replaces the squared health every other cube has.
const HEALTH_PER_SIZE: f64 = 4.0;

/// How close a player has to be for a shove to land.
///
/// Vanilla parity: `SulfurCube.PUSH_DISTANCE_THRESHOLD`.
const PUSH_DISTANCE_THRESHOLD: f64 = 1.3;

/// Fastest a player's own speed may shove a cube.
///
/// Vanilla parity: `SulfurCube.MAX_PLAYER_PUSH_SPEED`.
const MAX_PLAYER_PUSH_SPEED: f64 = 0.5;

/// How much of a walking player's speed becomes shove.
///
/// Vanilla parity: `SulfurCube.PLAYER_PUSH_SPEED_SCALE_MULTIPLIER`.
const PLAYER_PUSH_SPEED_SCALE_MULTIPLIER: f64 = 0.3;

/// How much of a riding player's speed becomes shove.
///
/// Vanilla parity: `SulfurCube.VEHICLE_PUSH_SPEED_SCALE_MULTIPLIER`. A player
/// on a horse shoves a cube about half as hard as one on foot.
const VEHICLE_PUSH_SPEED_SCALE_MULTIPLIER: f64 = 0.16;

/// How much of a shove goes upward.
///
/// Vanilla parity: `SulfurCube.VERTICAL_PUSH_MULTIPLIER`.
const VERTICAL_PUSH_MULTIPLIER: f64 = 0.3;

/// How far the tempt goal follows a player before it stops.
///
/// Vanilla parity: the `1.0` stop distance of `SulfurCube.addBehaviourGoals`.
const TEMPT_STOP_DISTANCE: f64 = 1.0;

/// How fast the tempt goal asks the cube to hop.
const TEMPT_SPEED_MODIFIER: f64 = 1.0;

/// How far a cube looks for an item to swallow.
///
/// Vanilla parity: the `inflate(8.0, 8.0, 8.0)` of `SulfurCubeSearchForItemsGoal`.
const ITEM_SEARCH_RANGE: f64 = 8.0;

/// How far the cube turns toward what it is chasing each tick.
///
/// Vanilla parity: the `lookAt(target, 10.0F, 10.0F)` both custom goals use.
const LOOK_TURN_RATE: f32 = 10.0;

/// How deep a sulfur cube counts as swimming.
///
/// Vanilla parity: `SulfurCube.getFluidJumpThreshold`, a fifth of the hitbox
/// rather than the flat value every other mob uses.
const FLUID_JUMP_THRESHOLD_FRACTION: f64 = 0.2;

/// How far a buoyant cube bobs above and below the surface.
///
/// Vanilla parity: the `0.2F * Mth.sin(...)` of `SulfurCube.travelInFluid`.
const BUOYANCY_BOB_AMPLITUDE: f32 = 0.2;

/// How fast it bobs.
///
/// Vanilla parity: the `tickCount * 0.4F` of the same line.
const BUOYANCY_BOB_RATE: f32 = 0.4;

/// How hard the fluid pushes a buoyant cube up per tick.
///
/// Vanilla parity: the `Math.min(1.0, immersion) * 0.04F` of the same method.
const BUOYANCY_LIFT: f64 = 0.04;

/// The blast starts at this height above the cube's feet.
///
/// Vanilla parity: the `getY(0.0625)` of `SulfurCube.tickFuse`.
const EXPLOSION_HEIGHT_FRACTION: f64 = 0.0625;

/// Everything the swallowed block decides, plus the two timers.
///
/// Vanilla keeps these as loose fields; they are one struct here because they
/// are rewritten together every time the body slot changes.
#[derive(Debug)]
struct SulfurCubeState {
    /// Vanilla parity: `SulfurCube.pickupTimer`.
    pickup_timer: i32,
    /// Vanilla parity: `SulfurCube.pushSoundCooldown`.
    push_sound_cooldown: i32,
    /// Vanilla parity: `SulfurCube.floatsInLiquids`.
    floats_in_liquids: bool,
    /// Vanilla parity: `SulfurCube.fuse`, `-1` while unprimed.
    fuse: i32,
    /// Vanilla parity: `SulfurCube.explosionData`.
    explosion: Option<SulfurCubeExplosion>,
    /// Vanilla parity: `SulfurCube.contactDamages`.
    contact_damages: Vec<SulfurCubeContactDamage>,
    /// Vanilla parity: `SulfurCube.knockbackModifier`.
    knockback_modifier: SulfurCubeKnockbackModifiers,
    /// Vanilla parity: `SulfurCube.soundSettings`.
    sound_settings: SulfurCubeSoundSettings,
}

impl Default for SulfurCubeState {
    fn default() -> Self {
        Self {
            pickup_timer: 0,
            push_sound_cooldown: 0,
            floats_in_liquids: false,
            fuse: -1,
            explosion: None,
            contact_damages: Vec::new(),
            knockback_modifier: DEFAULT_KNOCKBACK_MODIFIERS,
            sound_settings: DEFAULT_SOUND_SETTINGS,
        }
    }
}

/// A sulfur cube.
#[entity_behavior(class = "SulfurCube")]
pub struct SulfurCubeEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    entity_data: SyncMutex<SulfurCubeEntityData>,
    cube: SyncMutex<CubeState>,
    state: SyncMutex<SulfurCubeState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `SulfurCubeEntity`.
unsafe impl DowncastType for SulfurCubeEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/sulfur_cube");
}

/// Returns whether a sulfur cube would swallow this item.
///
/// Vanilla parity: `SulfurCube.isSwallowableItem`.
#[must_use]
fn is_swallowable_item(item_stack: &ItemStack) -> bool {
    REGISTRY
        .items
        .is_in_tag(item_stack.item(), &ItemTag::SULFUR_CUBE_SWALLOWABLE)
}

/// Returns whether a baby sulfur cube eats this item.
///
/// Vanilla parity: `SulfurCube.isFood`.
#[must_use]
fn is_sulfur_cube_food(item_stack: &ItemStack) -> bool {
    REGISTRY
        .items
        .is_in_tag(item_stack.item(), &ItemTag::SULFUR_CUBE_FOOD)
}

impl SulfurCubeEntity {
    /// Creates a sulfur cube at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a sulfur cube from saved base data.
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
        let mut entity_data = SulfurCubeEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);
        entity_data.abstract_cube_mob_mut().id_size.set(1);

        let cube = Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base: AgeableMobBase::new(),
            entity_data: SyncMutex::new(entity_data),
            cube: SyncMutex::new(CubeState::default()),
            state: SyncMutex::new(SulfurCubeState::default()),
        };
        cube.register_goals();
        cube
    }

    /// Registers the goal set.
    ///
    /// Vanilla parity: `AbstractCubeMob.registerGoals` with
    /// `SulfurCube.addBehaviourGoals` slotted in, and no targeting goals at all.
    /// It is a method rather than inline construction because vanilla calls it
    /// again every time the body slot empties.
    fn register_goals(&self) {
        let hooks = cube_common::hooks_for::<Self>();
        let mut goals = self.mob_base.goal_selector().lock();
        goals.add_goal(1, CubeFloatGoal::new(hooks));
        goals.add_goal(
            2,
            TemptGoal::with_stop_distance(
                TEMPT_SPEED_MODIFIER,
                |item_stack| is_swallowable_item(item_stack) || is_sulfur_cube_food(item_stack),
                false,
                TEMPT_STOP_DISTANCE,
            )
            .with_navigation(CubeTemptNavigation { hooks }, GoalControls::LOOK),
        );
        goals.add_goal(3, SearchForItemsGoal::new(hooks));
        goals.add_goal(4, CubeRandomDirectionGoal::new(hooks));
        goals.add_goal(5, CubeKeepOnJumpingGoal::new(hooks));
    }

    /// Returns vanilla `AbstractCubeMob.getSize`.
    ///
    /// Public because `CubeLike` is `pub(super)` to the hostile module, so this
    /// is how anything outside it reads a cube's size.
    #[must_use]
    pub fn cube_size(&self) -> i32 {
        <Self as CubeLike>::size(self)
    }

    /// Sets vanilla `AbstractCubeMob.setSize`, and with it the health, the
    /// speed, the hitbox and -- for this cube alone -- the baby flag.
    pub fn set_cube_size(&self, size: i32, update_health: bool) {
        <Self as CubeLike>::set_size(self, size, update_health);
    }

    /// Returns whether a block is swallowed.
    ///
    /// Vanilla parity: `SulfurCube.hasBodyItem`.
    #[must_use]
    pub fn has_body_item(&self) -> bool {
        !self.get_item_by_slot(EquipmentSlot::Body).is_empty()
    }

    /// Returns the ticks left on the fuse, or `-1` when the cube is unprimed.
    ///
    /// Vanilla parity: `SulfurCube.getFuse`.
    #[must_use]
    pub fn fuse(&self) -> i32 {
        self.state.lock().fuse
    }

    /// Returns whether the fuse is burning.
    ///
    /// Vanilla parity: `SulfurCube.isPrimed`.
    #[must_use]
    pub fn is_primed(&self) -> bool {
        self.fuse() >= 0
    }

    /// Returns whether lighting this cube would do anything.
    ///
    /// Vanilla parity: `SulfurCube.canExplode`.
    #[must_use]
    pub fn can_explode(&self) -> bool {
        let state = self.state.lock();
        state.explosion.is_some() && state.fuse < 0 && Entity::is_alive(self)
    }

    /// Returns every archetype whose item tag holds this stack.
    ///
    /// Vanilla parity: `SulfurCube.matchingArchetypes`. A block can be in more
    /// than one, and vanilla applies all of them, so this returns a list rather
    /// than a first match.
    #[must_use]
    pub fn matching_archetypes(item_stack: &ItemStack) -> Vec<SulfurCubeArchetypeRef> {
        if item_stack.is_empty() {
            return Vec::new();
        }
        let item = item_stack.item();
        REGISTRY
            .sulfur_cube_archetypes
            .iter()
            .map(|(_, archetype)| archetype)
            .filter(|archetype| archetype.items.contains(item))
            .collect()
    }

    /// Swaps every archetype the old block gave for the ones the new block does.
    ///
    /// Vanilla parity: `SulfurCube.collectEquipmentChanges`, which is the whole
    /// system: the goals go away while something is swallowed, the previous
    /// block's attribute modifiers come off, and the new block's archetypes are
    /// applied one after another so the last one wins on the single-valued
    /// fields and the contact damages accumulate.
    ///
    /// Steel gap: `bounciness`, `friction_modifier` and `air_drag_modifier` are
    /// applied here as vanilla writes them, but Steel's movement code does not
    /// read those three attributes yet, so a bouncy cube is not yet bouncier
    /// than a sticky one. The knockback resistances it also sets are read.
    fn apply_archetypes(&self, previous: &ItemStack, current: &ItemStack) {
        if current.is_empty() {
            self.register_goals();
        } else {
            self.mob_base.goal_selector().lock().remove_all_goals(self);
            self.set_mob_speed(0.0);
        }

        {
            let mut attributes = self.attributes().lock();
            for archetype in Self::matching_archetypes(previous) {
                for entry in archetype.attribute_modifiers {
                    attributes.remove_modifier(entry.attribute, &entry.id);
                }
            }
        }

        let archetypes = Self::matching_archetypes(current);
        {
            let mut state = self.state.lock();
            state.floats_in_liquids = false;
            state.explosion = None;
            state.contact_damages.clear();
            state.knockback_modifier = DEFAULT_KNOCKBACK_MODIFIERS;
            state.sound_settings = DEFAULT_SOUND_SETTINGS;

            for archetype in &archetypes {
                if archetype.buoyant {
                    state.floats_in_liquids = true;
                }
                if let Some(explosion) = archetype.explosion {
                    state.explosion = Some(explosion);
                }
                if let Some(contact_damage) = archetype.contact_damage {
                    state.contact_damages.push(contact_damage);
                }
                state.knockback_modifier = archetype.knockback_modifiers;
                state.sound_settings = archetype.sound_settings;
            }
        }

        let mut attributes = self.attributes().lock();
        for archetype in &archetypes {
            for entry in archetype.attribute_modifiers {
                attributes.set_modifier(
                    entry.attribute,
                    AttributeModifier {
                        id: entry.id.clone(),
                        amount: entry.amount,
                        operation: entry.operation,
                    },
                    false,
                );
            }
        }
    }

    /// Burns the fuse down, and blows up when it reaches zero.
    ///
    /// Vanilla parity: `SulfurCube.tickFuse`.
    fn tick_fuse(&self) {
        let (fuse, explosion) = {
            let mut state = self.state.lock();
            if state.fuse > 0 {
                state.fuse -= 1;
            }
            (state.fuse, state.explosion)
        };

        let Some(explosion) = explosion else {
            return;
        };
        if fuse != 0 {
            return;
        }

        self.drop_leash();
        let Some(world) = self.level() else {
            return;
        };
        if world.get_game_rule(&TNT_EXPLODES) {
            self.explode(&world, explosion);
        }
        self.set_removed(RemovalReason::Discarded);
    }

    /// Detonates.
    ///
    /// Vanilla parity: the `level.explode(...)` of `SulfurCube.tickFuse`. The
    /// blast starts a fraction above the cube's feet, and it only breaks blocks
    /// when mob griefing is on.
    fn explode(&self, world: &Arc<World>, explosion: SulfurCubeExplosion) {
        let interaction = if world.get_game_rule(&MOB_GRIEFING) {
            world.explosion_destroy_type(&TNT_EXPLOSION_DROP_DECAY)
        } else {
            ExplosionBlockInteraction::Keep
        };
        let position = self.position();
        let center = DVec3::new(
            position.x,
            f64::from(self.dimensions_for_pose(self.pose()).height)
                .mul_add(EXPLOSION_HEIGHT_FRACTION, position.y),
            position.z,
        );
        #[expect(
            clippy::cast_precision_loss,
            reason = "an explosion power from extracted archetype data, at most a few blocks"
        )]
        let radius = explosion.power as f32;
        world.explode(
            ExplosionSpec::new(
                Some(self.id()),
                Some(self.id()),
                None,
                radius,
                explosion.causes_fire,
                interaction,
            ),
            center,
        );
    }

    /// Lights the fuse.
    ///
    /// Vanilla parity: `SulfurCube.primeTime`. `imminent` is the explosion
    /// route, which shortens the fuse instead of using the archetype's own.
    pub fn prime_time(&self, imminent: bool) -> bool {
        let Some(world) = self.level() else {
            return false;
        };
        if !world.get_game_rule(&TNT_EXPLODES) {
            return false;
        }

        let fuse_time = {
            let state = self.state.lock();
            let Some(explosion) = state.explosion else {
                return false;
            };
            if state.fuse >= 0 || !Entity::is_alive(self) {
                return false;
            }
            if imminent {
                PrimedTntEntity::random_short_fuse(explosion.fuse)
            } else {
                explosion.fuse
            }
        };

        self.set_invulnerable(true);
        self.state.lock().fuse = fuse_time;
        self.entity_data
            .lock()
            .sulfur_cube_mut()
            .max_fuse
            .set(fuse_time);
        self.make_sound(Some(&sound_events::ENTITY_TNT_PRIMED));
        self.game_event(&vanilla_game_events::PRIME_FUSE);
        true
    }

    /// Lights the fuse when the cube is standing on a powered block.
    ///
    /// Vanilla parity: `SulfurCube.primeWhenOnPoweredPosition`. This is what
    /// makes an explosive cube a redstone component you can herd onto a
    /// pressure plate.
    fn prime_when_on_powered_position(&self) {
        if !self.can_explode() {
            return;
        }
        let Some(world) = self.level() else {
            return;
        };
        let position = self.position();
        let here = BlockPos::containing(position.x, position.y, position.z);
        if world.get_best_own_or_neighbour_signal(here) != 0 {
            self.prime_time(false);
        }
    }

    /// Hurts whatever the cube is touching with each archetype's contact damage.
    ///
    /// Vanilla parity: `SulfurCube.applyContactDamage`.
    fn apply_contact_damage(&self, target: &SharedEntity) {
        let Some(world) = self.level() else {
            return;
        };
        let damages = self.state.lock().contact_damages.clone();
        // Vanilla samples each provider from the cube's own `RandomSource`.
        // Contact damage is incidental live randomness rather than anything
        // save-repeatable, so the vanilla-shaped generator is seeded from
        // Steel's runtime RNG instead of being carried on the entity.
        let mut random = Xoroshiro::from_seed(rand::random::<u64>());
        for damage in damages {
            let source = if damage.attribute_to_source {
                DamageSource::environment(damage.damage_type).with_causing_entity(self.id())
            } else {
                DamageSource::environment(damage.damage_type)
            };
            let amount = damage.amount.sample(&mut random);
            target.hurt(&world, &source, amount);
        }
    }

    /// Shoves the cube away from a player who walked into it.
    ///
    /// Vanilla parity: `SulfurCube.playerPush`, which is the whole reason a
    /// sulfur cube with a block in it is a football: the shove is scaled by how
    /// fast the player was already moving, so you dribble it rather than
    /// nudging it.
    fn player_push(&self, player: &Arc<Player>) {
        if !self.has_body_item() {
            return;
        }

        let pusher: SharedEntity = player
            .is_passenger()
            .then(|| player.root_vehicle())
            .flatten()
            .unwrap_or_else(|| player.clone());

        let position = self.position();
        let pusher_position = pusher.position();
        let cube_to_pusher = position - pusher_position;
        let cube_bottom = position.y;
        let cube_top = cube_bottom + f64::from(self.dimensions_for_pose(self.pose()).height);
        let pusher_feet = pusher_position.y;
        let pusher_top = pusher_feet + f64::from(pusher.dimensions_for_pose(pusher.pose()).height);

        let horizontal_distance = cube_to_pusher.with_y(0.0).length();
        if horizontal_distance >= PUSH_DISTANCE_THRESHOLD
            || pusher_feet > cube_top
            || pusher_top <= cube_bottom
        {
            return;
        }

        let knockback = (1.0 - self.knockback_resistance()).max(0.0);
        let horizontal = cube_to_pusher.with_y(0.0);
        if horizontal.length_squared() <= 0.0 {
            return;
        }
        let push_direction = horizontal.normalize() * knockback;
        let push_speed_scale = if player.is_passenger() {
            VEHICLE_PUSH_SPEED_SCALE_MULTIPLIER
        } else {
            PLAYER_PUSH_SPEED_SCALE_MULTIPLIER
        };
        let player_speed = (player.known_speed().length() * 2.0 * push_speed_scale)
            .clamp(0.0, MAX_PLAYER_PUSH_SPEED);
        let push_velocity = DVec3::new(
            push_direction.x,
            if self.on_ground() {
                knockback * VERTICAL_PUSH_MULTIPLIER
            } else {
                0.0
            },
            push_direction.z,
        ) * player_speed;

        let play_push_sound = {
            let mut state = self.state.lock();
            let threshold = f64::from(state.sound_settings.push_sound_impulse_threshold);
            let loud = push_velocity.length_squared() > threshold * threshold;
            if loud && state.push_sound_cooldown <= 0 {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "a cooldown in seconds from extracted archetype data"
                )]
                let cooldown = (state.sound_settings.push_sound_cooldown * 20.0) as i32;
                state.push_sound_cooldown = cooldown;
                Some(state.sound_settings.push_sound)
            } else {
                None
            }
        };
        if let Some(sound) = play_push_sound {
            self.play_sound(sound, self.sound_volume(), 1.0);
        }

        self.set_velocity(self.velocity() + push_velocity);
        self.mark_velocity_sync();
        self.apply_contact_damage(&(player.clone() as SharedEntity));
    }

    /// Swallows a held block, spitting out whatever was in there before.
    ///
    /// Vanilla parity: `SulfurCube.equipItem`. Feeding it the block it already
    /// holds is refused, which is what stops a player toggling a cube's
    /// archetype off and on for free.
    pub fn equip_item(&self, held_item: &ItemStack) -> bool {
        if AgeableMob::is_baby(self) {
            return false;
        }

        let swallowed = self.get_item_by_slot(EquipmentSlot::Body);
        if !swallowed.is_empty() {
            if held_item.is(swallowed.item()) {
                return false;
            }
            self.eject_body_item(swallowed);
        }

        self.set_item_slot_and_drop_when_killed(EquipmentSlot::Body, held_item.copy_with_count(1));
        self.play_sound(&sound_events::ENTITY_SULFUR_CUBE_ABSORB, 1.0, 1.0);
        true
    }

    /// Drops what was swallowed at the cube's passenger attachment point.
    ///
    /// Vanilla parity: the `spawnAtLocation(level, item, getAttachments()
    /// .getAverage(PASSENGER))` both `equipItem` and `shear` use, which is what
    /// makes the block pop out of the top rather than the feet.
    fn eject_body_item(&self, item_stack: ItemStack) {
        if item_stack.is_empty() {
            return;
        }
        let dimensions = self.dimensions_for_pose(self.pose());
        let offset = dimensions
            .attachments
            .get_average(EntityAttachment::Passenger, dimensions);
        self.spawn_at_location_with_offset(item_stack, offset);
    }

    /// Cuts the swallowed block back out.
    ///
    /// Vanilla parity: `SulfurCube.shear`, plus the pickup timer that stops the
    /// cube swallowing it straight back off the ground.
    pub fn shear(&self) {
        let item_stack = self.get_item_by_slot(EquipmentSlot::Body);
        self.set_item_slot(EquipmentSlot::Body, ItemStack::empty());
        self.eject_body_item(item_stack);
        self.play_sound(&sound_events::ENTITY_SULFUR_CUBE_EJECT, 1.0, 1.0);
        self.state.lock().pickup_timer = PICKUP_TIMER_DURATION;
    }

    /// Returns whether shears would get anything out.
    ///
    /// Vanilla parity: `SulfurCube.readyForShearing`.
    #[must_use]
    pub fn ready_for_shearing(&self) -> bool {
        self.has_body_item()
    }

    /// Vanilla parity: the `isBaby()` branch of `SulfurCube.mobInteract`.
    fn feed_baby(
        &self,
        player: &Player,
        hand: InteractionHand,
        item_stack: &ItemStack,
    ) -> InteractionResult {
        if !is_sulfur_cube_food(item_stack) || !self.can_age_up() {
            return InteractionResult::Pass;
        }
        let age = self.get_age();
        Mob::use_player_item(self, player, hand);
        self.age_up(
            AgeableMobBase::get_speed_up_seconds_when_feeding(-age),
            true,
        );
        self.play_sound(
            &sound_events::ENTITY_SMALL_SULFUR_CUBE_EAT,
            self.sound_volume(),
            1.0,
        );
        InteractionResult::Success
    }

    /// Vanilla parity: the flint-and-steel branch of `SulfurCube.mobInteract`.
    fn try_light(
        &self,
        player: &Player,
        hand: InteractionHand,
        item_stack: &ItemStack,
    ) -> Option<InteractionResult> {
        let is_lighter = item_stack.is(&vanilla_items::FLINT_AND_STEEL)
            || item_stack.is(&vanilla_items::FIRE_CHARGE);
        if !self.can_explode() || !is_lighter {
            return None;
        }

        let tnt_explodes = self
            .level()
            .is_some_and(|world| world.get_game_rule(&TNT_EXPLODES));
        if !tnt_explodes {
            // Vanilla sends `block.minecraft.tnt.disabled` to the action bar
            // here; Steel's overlay message path is the same either way, and
            // the interaction still passes.
            return Some(InteractionResult::Pass);
        }

        self.prime_time(false);
        if item_stack.is(&vanilla_items::FLINT_AND_STEEL) {
            player
                .inventory
                .lock()
                .hurt_item_in_hand(hand, 1, player.has_infinite_materials());
        } else {
            Mob::use_player_item(self, player, hand);
        }
        Some(InteractionResult::Success)
    }

    /// Returns the volume of this cube's own noises.
    fn sound_volume(&self) -> f32 {
        cube_common::sound_volume(self)
    }

    /// Returns the swallowed block as an item component.
    ///
    /// Vanilla parity: `SulfurCube.getSulfurCubeContent`, which is how the
    /// block rides in a bucket: two buckets of cubes that swallowed different
    /// blocks carry different components and so refuse to stack.
    #[must_use]
    pub fn sulfur_cube_content(&self) -> Option<SulfurCubeContent> {
        let item_stack = self.get_item_by_slot(EquipmentSlot::Body);
        if item_stack.is_empty() {
            return None;
        }
        ItemStackTemplate::from_stack(&item_stack)
            .ok()
            .map(SulfurCubeContent::new)
    }
}

/// Returns whether the hit came from a projectile that is on fire.
///
/// Vanilla parity: the `sourceEntity instanceof AbstractArrow projectile &&
/// projectile.isOnFire()` of `SulfurCube.hurtServer`.
///
/// Steel gap: there is no `AbstractArrow` capability trait to narrow this to
/// arrows, so any burning projectile lights the fuse. Every other burning
/// projectile already carries an `is_fire` damage type, so the branch above
/// catches it first either way.
#[must_use]
fn is_burning_projectile(world: &World, source: &DamageSource) -> bool {
    let Some(direct_entity_id) = source.direct_entity_id else {
        return false;
    };
    let Some(entity) = world.get_entity_by_id(direct_entity_id) else {
        return false;
    };
    entity.as_projectile().is_some() && entity.is_on_fire()
}

impl Entity for SulfurCubeEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        cube_common::dimensions_for_size(self)
    }

    /// Vanilla parity: `SulfurCube.getSoundSource`, which is `NEUTRAL` rather
    /// than the `HOSTILE` every other cube uses.
    fn sound_source(&self) -> SoundSource {
        SoundSource::Neutral
    }

    /// Vanilla parity: `SulfurCube.tick`, which burns the fuse and checks for
    /// redstone underneath before the shared cube tick runs.
    fn base_tick(&self) {
        self.tick_fuse();
        self.prime_when_on_powered_position();
        Mob::base_tick_mob(self);
        cube_common::tick_landing(self);
    }

    /// Vanilla parity: `SulfurCube.getFluidJumpThreshold`. A loaded cube is
    /// short and wide, so it counts as swimming far sooner than a walking mob.
    fn get_fluid_jump_threshold(&self) -> f64 {
        f64::from(self.dimensions_for_pose(self.pose()).height) * FLUID_JUMP_THRESHOLD_FRACTION
    }

    /// Vanilla parity: `SulfurCube.canFreeze`, which is `false` while a block
    /// is swallowed.
    fn can_freeze(&self) -> bool {
        !self.has_body_item() && self.default_can_freeze()
    }

    /// Vanilla parity: `SulfurCube.maxUpStep`, which is zero while a block is
    /// swallowed -- a loaded cube cannot climb a step, so it stays where you
    /// kicked it.
    fn max_up_step(&self) -> f32 {
        if self.has_body_item() {
            return 0.0;
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a step height attribute, always below one block"
        )]
        let step_height = self
            .attributes()
            .lock()
            .required_value(vanilla_attributes::STEP_HEIGHT) as f32;
        step_height
    }

    /// Vanilla parity: `SulfurCube.playerTouch`, which is the base cube's
    /// contact damage -- which a sulfur cube never deals -- plus the shove.
    fn player_touch(self: Arc<Self>, player: &Arc<Player>) {
        let target: SharedEntity = player.clone();
        cube_common::player_touch(self.as_ref(), &target);
        self.player_push(player);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let (pickup_timer, fuse) = {
            let state = self.state.lock();
            (state.pickup_timer, state.fuse)
        };
        nbt.insert("pickup_timer", pickup_timer);
        nbt.insert("from_bucket", self.from_bucket());
        nbt.insert("fuse", fuse);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        let fuse = nbt.int("fuse").unwrap_or(-1);
        {
            let mut state = self.state.lock();
            state.pickup_timer = nbt.int("pickup_timer").unwrap_or(0);
            state.fuse = fuse;
        }
        self.set_from_bucket(nbt.byte("from_bucket").is_some_and(|flag| flag != 0));
        self.entity_data.lock().sulfur_cube_mut().max_fuse.set(fuse);
    }
}

impl LivingEntity for SulfurCubeEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    fn cube_loot_size(&self) -> Option<i32> {
        Some(CubeLike::size(self))
    }

    /// Vanilla parity: `Mob.serverAiStep`. Without this the goals never tick.
    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    /// Vanilla parity: `SulfurCube.doPush`, which adds every archetype's
    /// contact damage on top of the ordinary shove -- a magma-block cube burns
    /// whatever crowds it.
    fn do_push(&self, entity: &SharedEntity) {
        self.living_do_push(entity);
        self.apply_contact_damage(entity);
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

    /// Vanilla parity: `SulfurCube.getBaseExperienceReward`. A sulfur cube is
    /// the one cube whose reward is not its size: it is worth one or two, and a
    /// baby is worth nothing. That is why its `setSize` sets no `xpReward` at
    /// all -- the slime's and the magma cube's do.
    fn base_experience_reward(&self) -> i32 {
        if AgeableMob::is_baby(self) {
            0
        } else {
            1 + rand::random_range(0..2)
        }
    }

    /// Vanilla parity: `SulfurCube.travelInFluid`, whose second half is what a
    /// buoyant archetype buys. A cube that swallowed wool or TNT rides up to
    /// the surface and bobs there; one that swallowed ice or slime sinks.
    fn travel_in_fluid(&self, input: DVec3) -> Option<MoveResult> {
        let result = self.default_travel_in_fluid(input);
        let floats = self.state.lock().floats_in_liquids;
        if !floats || !self.has_body_item() {
            return result;
        }

        #[expect(
            clippy::cast_precision_loss,
            reason = "a tick count driving a sine wave, where the drift is the point"
        )]
        let phase = self.tick_count() as f32 * BUOYANCY_BOB_RATE;
        let bob = f64::from(BUOYANCY_BOB_AMPLITUDE * phase.sin());
        let contact = self.fluid_contact();
        let fluid_height = if self.is_in_water() {
            contact.water_height()
        } else {
            contact.lava_height()
        };

        let immersion = fluid_height - self.get_fluid_jump_threshold() + bob;
        if immersion > 0.0 {
            self.set_velocity(
                self.velocity() + DVec3::new(0.0, immersion.min(1.0) * BUOYANCY_LIFT, 0.0),
            );
        }

        result
    }

    /// Vanilla parity: `SulfurCube.hurtServer`. A hit with a block swallowed
    /// can light the fuse, and the damage types the block is immune to are
    /// answered with a shove instead of damage.
    ///
    /// Steel gap: vanilla answers the immune case with
    /// `dealDefaultKnockback(source, damage, true)`, which needs the damage
    /// amount that Steel's `LivingEntity::knockback` does not carry. The
    /// ordinary damage knockback stands in, so the shove is the right direction
    /// with the wrong strength.
    fn hurt_server(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        if !self.has_body_item() {
            return self.living_hurt_server(world, source, amount);
        }

        if self.can_explode() {
            if source.is(&DamageTypeTag::IS_FIRE) || is_burning_projectile(world, source) {
                self.prime_time(false);
            } else if source.is(&DamageTypeTag::IS_EXPLOSION) {
                self.prime_time(true);
            }
        }

        if source.is(&DamageTypeTag::SULFUR_CUBE_WITH_BLOCK_IMMUNE_TO) {
            if !source.is(&DamageTypeTag::NO_KNOCKBACK) {
                self.apply_damage_knockback(source);
            }
            return true;
        }

        self.living_hurt_server(world, source, amount)
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(if self.is_tiny() {
            &sound_events::ENTITY_SMALL_SULFUR_CUBE_HURT
        } else {
            &sound_events::ENTITY_SULFUR_CUBE_HURT
        })
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(if self.is_tiny() {
            &sound_events::ENTITY_SMALL_SULFUR_CUBE_DEATH
        } else {
            &sound_events::ENTITY_SULFUR_CUBE_DEATH
        })
    }

    /// Vanilla parity: the `remove` override of `AbstractCubeMob` with
    /// `SulfurCube.getSplitCount`, which is two -- or none at all when the fuse
    /// is burning, because a cube that explodes leaves no children.
    fn die(&self, source: &DamageSource) {
        if self.is_removed() {
            return;
        }
        if !self.is_primed()
            && CubeLike::size(self) > BABY_SIZE
            && let Some(world) = self.level()
        {
            for index in 0..SPLIT_COUNT {
                self.spawn_split_child(&world, index);
            }
        }
        self.living_die(source);
    }

    /// Vanilla parity: `SulfurCube.canUseSlot`, which opens the body slot only
    /// on a living adult.
    fn can_use_slot(&self, slot: EquipmentSlot) -> bool {
        if slot != EquipmentSlot::Body {
            return true;
        }
        Entity::is_alive(self) && !AgeableMob::is_baby(self)
    }

    /// Vanilla parity: `SulfurCube.canDispenserEquipIntoSlot`, which is how a
    /// dispenser feeds a cube its block.
    fn can_dispenser_equip_into_slot(&self, slot: EquipmentSlot) -> bool {
        slot == EquipmentSlot::Body
    }

    /// Vanilla parity: `SulfurCube.getEquipmentSlotForItem`.
    fn equipment_slot_for_item(&self, item_stack: &ItemStack) -> EquipmentSlot {
        if is_swallowable_item(item_stack) {
            return EquipmentSlot::Body;
        }
        item_stack
            .get_equippable_slot()
            .filter(|slot| self.can_use_slot(*slot))
            .unwrap_or(EquipmentSlot::MainHand)
    }

    /// Vanilla parity: `SulfurCube.isEquippableInSlot`.
    fn is_equippable_in_slot(&self, item_stack: &ItemStack, slot: EquipmentSlot) -> bool {
        if slot == EquipmentSlot::Body {
            return is_swallowable_item(item_stack);
        }
        let Some(equippable) = item_stack.get_equippable() else {
            return slot == EquipmentSlot::MainHand && self.can_use_slot(EquipmentSlot::MainHand);
        };
        slot == equippable.slot
            && self.can_use_slot(equippable.slot)
            && equippable.can_be_equipped_by(self.entity_type())
    }

    /// Vanilla parity: `SulfurCube.collectEquipmentChanges`, reached here
    /// through the shared equipment-change hook so a block reaching the body
    /// slot changes the cube however it got there.
    fn on_equipment_changed(&self, slot: EquipmentSlot, previous: &ItemStack, current: &ItemStack) {
        if slot != EquipmentSlot::Body {
            return;
        }
        self.apply_archetypes(previous, current);
    }
}

impl Mob for SulfurCubeEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    /// Vanilla parity: `SulfurCube.customServerAiStep`, which is only these
    /// two timers; everything else the cube does per tick is in `tick`.
    fn custom_server_ai_step(&self) {
        let mut state = self.state.lock();
        state.pickup_timer = (state.pickup_timer - 1).max(0);
        state.push_sound_cooldown = (state.push_sound_cooldown - 1).max(0);
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    /// Vanilla parity: `SulfurCube.SulfurCubeMobMoveControl`, which does nothing
    /// at all while a block is swallowed. That is what makes a loaded cube inert
    /// -- it moves only when something else pushes it.
    fn tick_move_control(&self) {
        if self.has_body_item() {
            return;
        }
        cube_common::tick_move_control(self);
    }

    /// Vanilla parity: `SulfurCube.SulfurCubeLookControl`, which snaps a loaded
    /// cube's yaw to the nearest quarter turn so the block in it stays square.
    fn tick_look_control(&self) {
        if !self.has_body_item() {
            self.default_tick_look_control();
            return;
        }
        let (yaw, pitch) = self.rotation();
        let snapped = yaw - wrap_degrees_90(yaw);
        self.set_rotation((snapped, pitch));
        self.set_y_head_rot(snapped);
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        None
    }

    /// Vanilla parity: `SulfurCube.checkSulfurCubeSpawnRules`, which is `true`:
    /// wherever the spawn list puts one, it appears.
    fn check_spawn_rules(
        &self,
        _world: &Arc<World>,
        _spawn_reason: EntitySpawnReason,
        _pos: BlockPos,
    ) -> bool {
        true
    }

    /// Vanilla parity: `SulfurCube.setSpawnSize`, which has no roll in it at
    /// all -- a sulfur cube is small if it is a baby and big otherwise.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        let result = self.finalize_spawn_ageable_mob(world, spawn_reason, group_data);
        let size = if AgeableMob::is_baby(self) {
            BABY_SIZE
        } else {
            ADULT_SIZE
        };
        self.set_size(size, true);
        result
    }

    /// Vanilla parity: `SulfurCube.requiresCustomPersistence`, so a cube that
    /// swallowed something or came out of a bucket is never despawned.
    fn requires_custom_persistence(&self) -> bool {
        self.is_passenger() || self.is_leashed() || self.has_body_item() || self.from_bucket()
    }

    /// Vanilla parity: `SulfurCube.canBeLeashed`, which only allows a lead on a
    /// cube with a block in it.
    fn can_be_leashed(&self) -> bool {
        self.has_body_item()
    }

    /// Vanilla parity: `SulfurCube.canPickUpLoot`, which is `false` once
    /// something is already swallowed.
    fn can_pick_up_loot(&self) -> bool {
        !self.has_body_item()
    }

    /// Vanilla parity: `SulfurCube.canHoldItem`.
    fn can_hold_item(&self, item_stack: &ItemStack) -> bool {
        self.get_item_by_slot(EquipmentSlot::Body).is_empty()
            && is_swallowable_item(item_stack)
            && !AgeableMob::is_baby(self)
    }

    /// Vanilla parity: `SulfurCube.pickUpItem`, which takes one of the stack
    /// rather than all of it and is silenced by the shear cooldown.
    fn pick_up_item(&self, world: &Arc<World>, item_entity: &SharedEntity) {
        let Some(item) = item_entity.downcast_ref::<ItemEntity>() else {
            return;
        };
        let mut stack = item.get_item();
        let waiting_after_shear = self.state.lock().pickup_timer > 0;
        if waiting_after_shear || !self.can_hold_item(&stack) {
            return;
        }

        let swallowed = stack.split(1);
        self.set_item_slot(EquipmentSlot::Body, swallowed);
        self.play_sound(&sound_events::ENTITY_SULFUR_CUBE_ABSORB, 1.0, 1.0);
        Mob::set_guaranteed_drop(self, EquipmentSlot::Body);

        // Vanilla parity: `mob.take(entity, 1)`, the pickup animation, followed
        // by the shrunk stack -- an item entity with nothing left removes itself.
        world.broadcast_to_nearby(
            ChunkPos::from_entity_pos(item_entity.position()),
            CTakeItemEntity::new(item_entity.id(), self.id(), 1),
            None,
        );
        if stack.is_empty() {
            item_entity.set_removed(RemovalReason::Discarded);
        } else {
            item.set_item(stack);
        }
    }

    /// Vanilla parity: `SulfurCube.mobInteract`.
    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        let held_item = {
            let inventory = player.inventory.lock();
            let held = inventory.get_item_in_hand(hand);
            held.copy_with_count(held.count())
        };

        if AgeableMob::is_baby(self) {
            return self.feed_baby(player, hand, &held_item);
        }

        if self.is_primed() {
            return InteractionResult::Pass;
        }

        if let Some(result) = self.try_light(player, hand, &held_item) {
            return result;
        }

        if held_item.is(&vanilla_items::SHEARS) && self.ready_for_shearing() {
            if let Some(world) = self.level() {
                self.shear();
                world.game_event_at(
                    &vanilla_game_events::SHEAR,
                    self.position(),
                    &GameEventContext::new(Some(player as &dyn Entity), None),
                );
                player
                    .inventory
                    .lock()
                    .hurt_item_in_hand(hand, 1, player.has_infinite_materials());
            }
            return InteractionResult::Success;
        }

        if is_swallowable_item(&held_item) {
            if !self.equip_item(&held_item) {
                return InteractionResult::Pass;
            }
            player.inventory.lock().shrink_item_in_hand(hand, 1);
            if let Some(world) = self.level() {
                world.game_event_at(
                    &vanilla_game_events::ENTITY_INTERACT,
                    self.position(),
                    &GameEventContext::new(Some(self as &dyn Entity), None),
                );
            }
            return InteractionResult::Success;
        }

        bucket_mob_pickup(player, hand, self).unwrap_or(InteractionResult::Pass)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl AgeableMob for SulfurCubeEntity {
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

    /// Vanilla parity: `SulfurCube.ageBoundaryReached`, which is how a fed baby
    /// grows into a full-sized cube.
    fn age_boundary_changed(&self, baby: bool) {
        self.refresh_dimensions();
        if !baby {
            self.set_size(ADULT_SIZE, true);
        }
    }
}

impl CubeLike for SulfurCubeEntity {
    fn cube_state(&self) -> &SyncMutex<CubeState> {
        &self.cube
    }

    fn size(&self) -> i32 {
        *self.entity_data.lock().abstract_cube_mob().id_size.get()
    }

    fn store_size(&self, size: i32) {
        self.entity_data
            .lock()
            .abstract_cube_mob_mut()
            .id_size
            .set(size);
    }

    /// Vanilla parity: `SulfurCube.setcubeMobHealth`, four health per size step
    /// rather than the size squared.
    fn max_health_for_size(&self, size: i32) -> f64 {
        HEALTH_PER_SIZE * f64::from(size)
    }

    /// Vanilla parity: `SulfurCube.setSize`, which sets no attack damage and no
    /// experience reward -- unlike the slime's and the magma cube's -- and
    /// instead turns a cube sized down to one into a baby.
    fn set_size(&self, size: i32, update_health: bool) {
        cube_common::apply_size(self, size, update_health);
        if update_health && size == BABY_SIZE && !AgeableMob::is_baby(self) {
            self.set_baby(true);
        }
    }

    /// Vanilla parity: `SulfurCube.isDealsDamage`, which is flatly `false`. A
    /// sulfur cube never hurts you by being touched -- only its archetype's
    /// contact damage does.
    fn deals_damage(&self) -> bool {
        false
    }

    fn jump_sound(&self) -> SoundEventRef {
        if self.is_tiny() {
            &sound_events::ENTITY_SMALL_SULFUR_CUBE_JUMP
        } else {
            &sound_events::ENTITY_SULFUR_CUBE_JUMP
        }
    }

    /// Vanilla parity: `SulfurCube.getSquishSound`, whose grown form has two
    /// answers: a loaded cube bounces, an empty one squishes.
    fn squish_sound(&self) -> SoundEventRef {
        if self.is_tiny() {
            return &sound_events::ENTITY_SMALL_SULFUR_CUBE_SQUISH;
        }
        if self.has_body_item() {
            &sound_events::ENTITY_SULFUR_CUBE_BOUNCE
        } else {
            &sound_events::ENTITY_SULFUR_CUBE_SQUISH
        }
    }

    /// Vanilla parity: `SulfurCube.setUpSplitCube`, which makes both children
    /// babies rather than half-sized adults.
    fn split_child(&self, position: DVec3, world: &Arc<World>) -> SharedEntity {
        let child = Arc::new(Self::new(
            self.entity_type,
            next_entity_id(),
            position,
            Arc::downgrade(world),
        ));
        child.set_size(CubeLike::size(self) / 2, true);
        child.set_baby(true);
        child.set_rotation((rand::random::<f32>() * 360.0, 0.0));
        child
    }
}

impl SulfurCubeEntity {
    /// Places one of the two children a killed cube leaves.
    ///
    /// Vanilla parity: the loop of the `AbstractCubeMob.remove` override, whose
    /// offsets put the children on the corners of the parent's footprint.
    fn spawn_split_child(&self, world: &Arc<World>, index: i32) {
        let width = f64::from(self.dimensions_for_pose(self.pose()).width);
        let offset = width / 2.0;
        let origin = self.position();
        let position = DVec3::new(
            (f64::from(index % 2) - 0.5).mul_add(offset, origin.x),
            origin.y + 0.5,
            (f64::from(index / 2) - 0.5).mul_add(offset, origin.z),
        );
        let child = self.split_child(position, world);
        if let Err(error) = world.try_add_entity(child) {
            log::debug!("sulfur cube split rejected: {error}");
        }
    }
}

impl PathfinderMob for SulfurCubeEntity {}

impl Bucketable for SulfurCubeEntity {
    fn from_bucket(&self) -> bool {
        *self.entity_data.lock().sulfur_cube().from_bucket.get()
    }

    fn set_from_bucket(&self, from_bucket: bool) {
        self.entity_data
            .lock()
            .sulfur_cube_mut()
            .from_bucket
            .set(from_bucket);
    }

    /// Vanilla parity: `SulfurCube.saveToBucketTag`. The swallowed block rides
    /// in the bucket's own `minecraft:sulfur_cube_content` component, which is
    /// what stops two buckets holding differently-fed cubes from stacking.
    fn save_to_bucket_tag(&self, bucket: &mut ItemStack) {
        save_default_data_to_bucket_tag(self, bucket);
        if let Some(content) = self.sulfur_cube_content() {
            bucket.set(vanilla_components::SULFUR_CUBE_CONTENT, content);
        }

        let mut tag = NbtCompound::new();
        read_bucket_entity_data(bucket, |saved| {
            for (key, value) in saved.iter() {
                tag.insert(key.to_string(), value.to_owned());
            }
        });
        tag.insert("age", self.get_age());
        tag.insert("age_locked", self.is_age_locked());
        set_bucket_entity_data(bucket, tag);
    }

    /// Vanilla parity: `SulfurCube.loadFromBucketTag`.
    fn load_from_bucket_tag(&self, tag: BorrowedNbtCompoundView<'_, '_>) {
        load_default_data_from_bucket_tag(self, tag);
        self.set_age(tag.int("age").unwrap_or(0));
        self.set_age_locked(tag.byte("age_locked").is_some_and(|flag| flag != 0));
    }

    fn bucket_item_stack(&self) -> ItemStack {
        ItemStack::new(&vanilla_items::SULFUR_CUBE_BUCKET)
    }

    fn pickup_sound(&self) -> SoundEventRef {
        &sound_events::ITEM_BUCKET_FILL_SULFUR_CUBE
    }

    /// Vanilla parity: `SulfurCube.canBePickedUpWithBucket`, an empty bucket
    /// rather than the water bucket every fish needs.
    fn can_be_picked_up_with_bucket(&self, item_stack: &ItemStack) -> bool {
        item_stack.is(&vanilla_items::BUCKET)
    }
}

/// How the tempt goal steers a cube.
///
/// Vanilla parity: `SulfurCube.SulfurCubeTemptGoal`, which replaces the
/// navigation of `TemptGoal.ForNonPathfinders` with the same turn-and-hop the
/// attack goal uses -- a cube has no path to follow.
#[derive(Clone, Copy)]
struct CubeTemptNavigation {
    hooks: CubeHooks,
}

impl TemptNavigation for CubeTemptNavigation {
    fn stop_navigation(&self, mob: &dyn PathfinderMob) {
        (self.hooks.set_wanted_movement)(mob, 0.0);
    }

    fn navigate_towards(
        &self,
        mob: &dyn PathfinderMob,
        player: &Arc<Player>,
        _speed_modifier: f64,
    ) {
        let turned = look_at_yaw(mob, player.position());
        (self.hooks.set_heading)(mob, turned, true);
    }
}

/// Hops toward the nearest item worth swallowing.
///
/// Vanilla parity: `SulfurCube.SulfurCubeSearchForItemsGoal`. This is what lets
/// a cube feed itself off the ground, and the pickup timer is what stops a
/// freshly sheared one from picking its own block straight back up.
struct SearchForItemsGoal {
    hooks: CubeHooks,
    target_item: Option<SharedEntity>,
}

impl SearchForItemsGoal {
    const fn new(hooks: CubeHooks) -> Self {
        Self {
            hooks,
            target_item: None,
        }
    }
}

impl Goal for SearchForItemsGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::LOOK
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(cube) = mob.downcast_ref::<SulfurCubeEntity>() else {
            return false;
        };
        let waiting_after_shear = cube.state.lock().pickup_timer > 0;
        if waiting_after_shear || AgeableMob::is_baby(cube) {
            return false;
        }
        let Some(world) = mob.level() else {
            return false;
        };

        let origin = mob.position();
        let bounds = mob.bounding_box().inflate(ITEM_SEARCH_RANGE);
        let mut nearest: Option<(f64, SharedEntity)> = None;
        for entity in world.get_entities_in_aabb(&bounds) {
            let Some(item) = entity.downcast_ref::<ItemEntity>() else {
                continue;
            };
            if item.has_pickup_delay() || entity.is_removed() {
                continue;
            }
            if !is_swallowable_item(&item.get_item()) {
                continue;
            }
            let distance = entity.position().distance_squared(origin);
            if nearest.as_ref().is_none_or(|(best, _)| distance < *best) {
                nearest = Some((distance, entity));
            }
        }

        self.target_item = nearest.map(|(_, entity)| entity);
        self.target_item.is_some()
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(target) = &self.target_item else {
            return;
        };
        let turned = look_at_yaw(mob, target.position());
        (self.hooks.set_heading)(mob, turned, true);
    }

    fn stop(&mut self, _mob: &dyn PathfinderMob) {
        self.target_item = None;
    }
}

/// Turns the mob toward `target` at vanilla's rate and returns the new yaw.
///
/// Vanilla parity: the `lookAt(target, 10.0F, 10.0F)` both sulfur cube goals
/// run before they hand the yaw to the move control.
fn look_at_yaw(mob: &dyn PathfinderMob, target: DVec3) -> f32 {
    let to_target = target - mob.position();
    #[expect(
        clippy::cast_possible_truncation,
        reason = "an angle in degrees, immediately used as a rotation"
    )]
    let wanted = -(to_target.x.atan2(to_target.z).to_degrees() as f32);
    let (yaw, pitch) = mob.rotation();
    let turned = rotlerp(yaw, wanted, LOOK_TURN_RATE);
    mob.set_rotation((turned, pitch));
    turned
}

/// Returns how far `degrees` is from the nearest quarter turn.
///
/// Vanilla parity: `Mth.wrapDegrees90`.
fn wrap_degrees_90(degrees: f32) -> f32 {
    let wrapped = degrees % 90.0;
    if wrapped >= 45.0 {
        wrapped - 90.0
    } else if wrapped < -45.0 {
        wrapped + 90.0
    } else {
        wrapped
    }
}

#[cfg(test)]
mod tests;
