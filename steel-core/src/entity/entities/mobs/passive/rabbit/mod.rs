//! Rabbit entity.
//!
//! Vanilla parity: `Rabbit`. A rabbit never walks. Vanilla swaps out both its
//! jump control and its move control so that every step is a hop, and the two
//! collaborate: the move control decides how fast the next hop will be, the
//! jump control fires it, and `customServerAiStep` decides when the rabbit is
//! allowed to hop again. That trio is what this module is mostly about; the
//! variants, the carrot raiding and the breeding sit on top of it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{BlockStateProperties, IntProperty};
use steel_registry::entity_type::{EntityDimensions, EntityTypeRef};
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_biome_tags::BiomeTag;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_entity_data::RabbitEntityData;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{
    REGISTRY, TaggedRegistryExt as _, level_events, sound_events, vanilla_attributes,
    vanilla_blocks, vanilla_entities, vanilla_game_events, vanilla_game_rules,
};
use steel_utils::entity_events::EntityStatus;
use steel_utils::locks::SyncMutex;
use steel_utils::types::{InteractionHand, UpdateFlags};
use steel_utils::{BlockPos, Downcast as _, DowncastType, DowncastTypeKey, Identifier};
use text_components::{TextComponent, translation::TranslatedMessage};

use crate::behavior::InteractionResult;
use crate::entity::ai::control::{MoveControlOperation, RabbitJumpControl};
use crate::entity::ai::goal::{
    AvoidEntityGoal, BreedGoal, ClimbOnTopOfPowderSnowGoal, FloatGoal, Goal, GoalControls,
    HurtByTargetGoal, LookAtPlayerGoal, MeleeAttackGoal, MoveToBlockGoal,
    NearestAttackableTargetGoal, PanicGoal, TemptGoal, WaterAvoidingRandomStrollGoal,
    no_creative_or_spectator,
};
use crate::entity::attribute::{AttributeModifier, AttributeModifierOperation};
use crate::entity::damage::DamageSource;
use crate::entity::spawn::RabbitGroupData;
use crate::entity::{
    AgeableMob, AgeableMobBase, Animal, AnimalBase, Entity, EntityBase, EntityBaseLoad, EntityPose,
    EntitySpawnReason, EntitySyncedData, LivingEntity, LivingEntityBase, LivingEntitySyncedData,
    Mob, MobBase, MoveResult, PathfinderMob, SpawnGroupData,
};
use crate::player::Player;
use crate::world::game_event::GameEventContext;
use crate::world::{LevelReader, World};

/// Vanilla `Rabbit.BABY_DIMENSIONS`.
const BABY_DIMENSIONS: EntityDimensions = EntityDimensions::new(0.24, 0.4, 0.39);

/// Vanilla `Rabbit.STROLL_SPEED_MOD`.
const STROLL_SPEED_MOD: f64 = 0.6;
/// Vanilla `Rabbit.BREED_SPEED_MOD`.
const BREED_SPEED_MOD: f64 = 0.8;
/// Vanilla `Rabbit.FOLLOW_SPEED_MOD`.
const FOLLOW_SPEED_MOD: f64 = 1.0;
/// Vanilla `Rabbit.FLEE_SPEED_MOD`.
const FLEE_SPEED_MOD: f64 = 2.2;
/// Vanilla `Rabbit.ATTACK_SPEED_MOD`.
const ATTACK_SPEED_MOD: f64 = 1.4;
/// Vanilla `Rabbit.BABY_JUMP_HEIGHT`.
const BABY_JUMP_HEIGHT: f64 = 0.5;
/// Vanilla `Rabbit.ADULT_JUMP_HEIGHT`.
const ADULT_JUMP_HEIGHT: f64 = 1.5;
/// Vanilla `Rabbit.JUMP_DELAY_TICKS`.
const JUMP_DELAY_TICKS: i32 = 10;
/// Vanilla `Rabbit.PANIC_JUMP_DELAY_TICKS`.
const PANIC_JUMP_DELAY_TICKS: i32 = 3;
/// Vanilla `Rabbit.JUMP_DURATION_IN_TICKS`.
const JUMP_DURATION_IN_TICKS: i32 = 15;
/// Vanilla `Rabbit.EVIL_ATTACK_POWER_INCREMENT`.
const EVIL_ATTACK_POWER_INCREMENT: f64 = 5.0;
/// Vanilla `Rabbit.EVIL_ARMOR_VALUE`.
const EVIL_ARMOR_VALUE: f64 = 8.0;
/// Vanilla `Rabbit.MORE_CARROTS_DELAY`.
const MORE_CARROTS_DELAY: i32 = 40;
/// Vanilla `Rabbit.EVIL_ATTACK_POWER_MODIFIER`.
const EVIL_ATTACK_POWER_MODIFIER: Identifier = Identifier::vanilla_static("evil");
/// Vanilla `Rabbit.KILLER_BUNNY`.
const KILLER_BUNNY_NAME_KEY: &str = "entity.minecraft.killer_bunny";

/// Distance at which an evil rabbit pounces on its target.
///
/// Vanilla parity: the `16.0` of `Rabbit.customServerAiStep`.
const EVIL_POUNCE_RANGE_SQR: f64 = 16.0;

/// Distance a rabbit keeps from a player.
///
/// Vanilla parity: the `8.0F` of `registerGoals`.
const AVOID_PLAYER_RANGE: f32 = 8.0;
/// Distance a rabbit keeps from a wolf.
const AVOID_WOLF_RANGE: f32 = 10.0;
/// Distance a rabbit keeps from a monster.
const AVOID_MONSTER_RANGE: f32 = 4.0;

/// Speed the raiding rabbit walks to a carrot at.
///
/// Vanilla parity: the `0.7F` of `RaidGardenGoal`.
const RAID_SPEED_MOD: f64 = 0.7;
/// How far the raiding rabbit looks for a carrot.
const RAID_SEARCH_RANGE: i32 = 16;
/// Ticks the raiding rabbit waits after eating before looking again.
const RAID_RESTART_DELAY_TICKS: i32 = 10;

/// The age property of a carrot crop.
const CARROT_AGE: &IntProperty = &BlockStateProperties::AGE_7;
/// Vanilla `CropBlock.getMaxAge` for carrots.
const CARROT_MAX_AGE: u8 = 7;

/// Vanilla `Rabbit.Variant`.
///
/// The ids are sparse: `EVIL` is `99`, which is why the saved value is read
/// through [`RabbitVariant::by_id`] rather than indexed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RabbitVariant {
    /// Vanilla `BROWN`.
    Brown,
    /// Vanilla `WHITE`.
    White,
    /// Vanilla `BLACK`.
    Black,
    /// Vanilla `WHITE_SPLOTCHED`.
    WhiteSplotched,
    /// Vanilla `GOLD`.
    Gold,
    /// Vanilla `SALT`.
    Salt,
    /// Vanilla `EVIL`, the killer bunny.
    Evil,
}

impl RabbitVariant {
    /// Vanilla `Rabbit.Variant.DEFAULT`.
    pub const DEFAULT: Self = Self::Brown;

    /// Vanilla `Rabbit.Variant.id`.
    #[must_use]
    pub const fn id(self) -> i32 {
        match self {
            Self::Brown => 0,
            Self::White => 1,
            Self::Black => 2,
            Self::WhiteSplotched => 3,
            Self::Gold => 4,
            Self::Salt => 5,
            Self::Evil => 99,
        }
    }

    /// Vanilla `Rabbit.Variant.byId`, which falls back to the default id.
    #[must_use]
    pub const fn by_id(id: i32) -> Self {
        match id {
            1 => Self::White,
            2 => Self::Black,
            3 => Self::WhiteSplotched,
            4 => Self::Gold,
            5 => Self::Salt,
            99 => Self::Evil,
            _ => Self::DEFAULT,
        }
    }
}

/// Runtime rabbit fields that vanilla keeps on the entity or its controls.
#[derive(Debug, Clone, Copy, PartialEq)]
struct RabbitState {
    /// Vanilla `Rabbit.jumpTicks`.
    jump_ticks: i32,
    /// Vanilla `Rabbit.jumpDuration`.
    jump_duration: i32,
    /// Vanilla `Rabbit.wasOnGround`.
    was_on_ground: bool,
    /// Vanilla `Rabbit.jumpDelayTicks`.
    jump_delay_ticks: i32,
    /// Vanilla `Rabbit.moreCarrotTicks`.
    more_carrot_ticks: i32,
    /// Vanilla `Rabbit.RabbitMoveControl.nextJumpSpeed`.
    next_jump_speed: f64,
}

impl RabbitState {
    const fn new() -> Self {
        Self {
            jump_ticks: 0,
            jump_duration: 0,
            was_on_ground: false,
            jump_delay_ticks: 0,
            more_carrot_ticks: 0,
            next_jump_speed: 0.0,
        }
    }
}

/// A rabbit.
#[entity_behavior(class = "Rabbit")]
pub struct RabbitEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    state: SyncMutex<RabbitState>,
    jump_control: SyncMutex<RabbitJumpControl>,
    entity_data: SyncMutex<RabbitEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `RabbitEntity`.
unsafe impl DowncastType for RabbitEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/rabbit");
}

impl RabbitEntity {
    /// Creates a rabbit at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a rabbit from saved base data.
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
        let mut entity_data = RabbitEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Vanilla parity: `Rabbit.registerGoals`.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(1, FloatGoal::new(&mob_base));
            goals.add_goal(1, ClimbOnTopOfPowderSnowGoal::new());
            goals.add_goal(1, RabbitPanicGoal::new(FLEE_SPEED_MOD));
            goals.add_goal(2, BreedGoal::new(BREED_SPEED_MOD));
            goals.add_goal(
                3,
                TemptGoal::new(
                    FOLLOW_SPEED_MOD,
                    |item_stack| {
                        REGISTRY
                            .items
                            .is_in_tag(item_stack.item(), &ItemTag::RABBIT_FOOD)
                    },
                    false,
                ),
            );
            goals.add_goal(
                4,
                RabbitAvoidEntityGoal::new(AVOID_PLAYER_RANGE, |target| {
                    target.as_player().is_some()
                }),
            );
            goals.add_goal(
                4,
                RabbitAvoidEntityGoal::new(AVOID_WOLF_RANGE, |target| {
                    target.entity_type() == &vanilla_entities::WOLF
                }),
            );
            goals.add_goal(
                4,
                RabbitAvoidEntityGoal::new(AVOID_MONSTER_RANGE, |target| {
                    target.as_mob().is_some_and(Mob::is_monster)
                }),
            );
            goals.add_goal(5, RaidGardenGoal::new());
            goals.add_goal(6, WaterAvoidingRandomStrollGoal::new(STROLL_SPEED_MOD));
            goals.add_goal(11, LookAtPlayerGoal::new(10.0));
        }

        let rabbit = Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            state: SyncMutex::new(RabbitState::new()),
            jump_control: SyncMutex::new(RabbitJumpControl::new()),
            entity_data: SyncMutex::new(entity_data),
        };
        // Vanilla parity: the `setSpeedModifier(0.0)` of the `Rabbit` constructor.
        rabbit.set_speed_modifier(0.0);
        rabbit
    }

    /// Returns vanilla `Rabbit.getVariant`.
    #[must_use]
    pub fn variant(&self) -> RabbitVariant {
        RabbitVariant::by_id(*self.entity_data.lock().variant_type.get())
    }

    /// Applies vanilla `Rabbit.setVariant`.
    ///
    /// The killer bunny is not a skin: it gains armor, attack power, a name and
    /// a hostile goal set here. Vanilla adds those goals on every call rather
    /// than once, and this keeps that so a save reload behaves the same way.
    pub fn set_variant(&self, variant: RabbitVariant) {
        if variant == RabbitVariant::Evil {
            self.attributes()
                .lock()
                .set_base_value(vanilla_attributes::ARMOR, EVIL_ARMOR_VALUE);
            self.mob_base()
                .goal_selector()
                .lock()
                .add_goal(4, MeleeAttackGoal::new(ATTACK_SPEED_MOD, true));
            {
                let mut targets = self.mob_base().target_selector().lock();
                targets.add_goal(1, HurtByTargetGoal::new().set_alert_others([]));
                targets.add_goal(
                    2,
                    NearestAttackableTargetGoal::new_for_players(true, |_, _, _| true),
                );
                targets.add_goal(
                    2,
                    NearestAttackableTargetGoal::new(true, |_, target, _| {
                        target.entity_type() == &vanilla_entities::WOLF
                    }),
                );
            }
            self.attributes().lock().set_modifier(
                vanilla_attributes::ATTACK_DAMAGE,
                AttributeModifier {
                    id: EVIL_ATTACK_POWER_MODIFIER,
                    amount: EVIL_ATTACK_POWER_INCREMENT,
                    operation: AttributeModifierOperation::AddValue,
                },
                false,
            );
            if self.custom_name().is_none() {
                self.set_custom_name(Some(TextComponent::translated(TranslatedMessage {
                    key: KILLER_BUNNY_NAME_KEY.into(),
                    fallback: None,
                    args: None,
                })));
            }
        } else {
            self.attributes().lock().remove_modifier(
                vanilla_attributes::ATTACK_DAMAGE,
                &EVIL_ATTACK_POWER_MODIFIER,
            );
        }

        self.entity_data.lock().variant_type.set(variant.id());
    }

    /// Returns vanilla `Rabbit.wantsMoreFood`.
    #[must_use]
    fn wants_more_food(&self) -> bool {
        self.state.lock().more_carrot_ticks <= 0
    }

    /// Vanilla parity: `Rabbit.setSpeedModifier`.
    fn set_speed_modifier(&self, speed: f64) {
        self.mob_base()
            .navigation()
            .lock()
            .set_speed_modifier(speed);
        let wanted_position = self
            .mob_base()
            .controls()
            .lock()
            .move_control
            .wanted_position();
        self.set_wanted_position(wanted_position, speed);
    }

    /// Vanilla parity: `Rabbit.startJumping`.
    fn start_jumping(&self) {
        self.set_jumping(true);
        let mut state = self.state.lock();
        state.jump_duration = JUMP_DURATION_IN_TICKS;
        state.jump_ticks = 0;
    }

    /// Vanilla parity: `Rabbit.facePoint`.
    fn face_point(&self, face_x: f64, face_z: f64) {
        let position = self.position();
        let yaw = ((face_z - position.z)
            .atan2(face_x - position.x)
            .to_degrees() as f32)
            - 90.0;
        let (_, pitch) = self.rotation();
        self.set_rotation((yaw, pitch));
    }

    /// Vanilla parity: `Rabbit.setLandingDelay`.
    fn set_landing_delay(&self) {
        let speed_modifier = self
            .mob_base()
            .controls()
            .lock()
            .move_control
            .speed_modifier();
        self.state.lock().jump_delay_ticks = if speed_modifier < FLEE_SPEED_MOD {
            JUMP_DELAY_TICKS
        } else {
            PANIC_JUMP_DELAY_TICKS
        };
    }

    /// Vanilla parity: `Rabbit.checkLandingDelay`.
    fn check_landing_delay(&self) {
        self.set_landing_delay();
        self.jump_control.lock().set_can_jump(false);
    }

    /// Vanilla parity: `Rabbit.getJumpSound`.
    const fn jump_sound() -> SoundEventRef {
        &sound_events::ENTITY_RABBIT_JUMP
    }

    /// Vanilla parity: `Rabbit.getRandomRabbitVariant`.
    #[must_use]
    fn random_rabbit_variant(world: &Arc<World>, pos: BlockPos) -> RabbitVariant {
        let roll = rand::random_range(0..100);
        let Some(biome) = world.biome_at(pos) else {
            return if roll < 50 {
                RabbitVariant::Brown
            } else if roll < 90 {
                RabbitVariant::Salt
            } else {
                RabbitVariant::Black
            };
        };

        if biome.has_tag(&BiomeTag::SPAWNS_WHITE_RABBITS) {
            if roll < 80 {
                RabbitVariant::White
            } else {
                RabbitVariant::WhiteSplotched
            }
        } else if biome.has_tag(&BiomeTag::SPAWNS_GOLD_RABBITS) {
            RabbitVariant::Gold
        } else if roll < 50 {
            RabbitVariant::Brown
        } else if roll < 90 {
            RabbitVariant::Salt
        } else {
            RabbitVariant::Black
        }
    }

    /// Returns whether the stack is vanilla rabbit food.
    #[must_use]
    pub fn is_food(item_stack: &ItemStack) -> bool {
        REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::RABBIT_FOOD)
    }
}

impl Entity for RabbitEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);
    }

    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let scale = LivingEntity::get_scale(self);
        if AgeableMob::is_baby(self) {
            BABY_DIMENSIONS.scale(scale)
        } else if self.entity_type.fixed {
            self.entity_type.dimensions
        } else {
            self.entity_type.dimensions.scale(scale)
        }
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `Rabbit.getSoundSource`.
    fn sound_source(&self) -> SoundSource {
        if self.variant() == RabbitVariant::Evil {
            SoundSource::Hostile
        } else {
            SoundSource::Neutral
        }
    }

    // VANILLA CLIENT-LOCAL: `Rabbit.canSpawnSprintParticle` returns false so a
    // hopping rabbit does not trail dust. Sprint particles are spawned by the
    // client from `Entity.spawnSprintParticle`, so Steel has nothing to skip.

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
        nbt.insert("RabbitType", self.variant().id());
        nbt.insert("MoreCarrotTicks", self.state.lock().more_carrot_ticks);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
        self.set_variant(RabbitVariant::by_id(
            nbt.int("RabbitType").unwrap_or(RabbitVariant::DEFAULT.id()),
        ));
        self.state.lock().more_carrot_ticks = nbt.int("MoreCarrotTicks").unwrap_or(0);
    }
}

impl LivingEntity for RabbitEntity {
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
        Some(&sound_events::ENTITY_RABBIT_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_RABBIT_DEATH)
    }

    /// Vanilla parity: `Rabbit.getJumpPower`.
    ///
    /// A rabbit picks its hop height from what it is doing: a stroll is a
    /// shuffle, a climb or a wall in the way is a full leap.
    fn get_jump_power(&self) -> f32 {
        let mut base_jump_power = 0.3_f32;
        let (speed_modifier, wanted_y) = {
            let controls = self.mob_base().controls().lock();
            (
                controls.move_control.speed_modifier(),
                controls.move_control.wanted_position().y,
            )
        };
        if speed_modifier <= STROLL_SPEED_MOD {
            base_jump_power = 0.2;
        }

        let position_y = self.position().y;
        let next_pos = {
            let navigation = self.mob_base().navigation().lock();
            navigation.path().and_then(|path| {
                (!path.is_done()).then(|| path.next_entity_pos(self.bounding_box().width()))
            })
        };
        if let Some(Some(next_pos)) = next_pos
            && next_pos.y > position_y + 0.5
        {
            base_jump_power = 0.5;
        }

        if self.horizontal_collision() || self.is_jumping() && wanted_y > position_y + 0.5 {
            base_jump_power = 0.5;
        }

        self.get_jump_power_with_multiplier(base_jump_power / 0.42)
    }

    /// Vanilla parity: `Rabbit.jumpFromGround`.
    fn jump_from_ground(&self) {
        self.default_jump_from_ground();
        let speed_modifier = self
            .mob_base()
            .controls()
            .lock()
            .move_control
            .speed_modifier();
        if speed_modifier > 0.0 {
            let velocity = self.velocity();
            let horizontal_sqr = velocity.x.mul_add(velocity.x, velocity.z * velocity.z);
            if horizontal_sqr < 0.01 {
                let jump_height = if AgeableMob::is_baby(self) {
                    BABY_JUMP_HEIGHT
                } else {
                    ADULT_JUMP_HEIGHT
                };
                self.move_relative(0.1, DVec3::new(0.0, jump_height, 1.0));
            }
        }

        self.broadcast_entity_event(EntityStatus::Jump);
    }

    /// Vanilla parity: `Rabbit.setJumping`, which is where the hop sound comes
    /// from rather than from the jump control.
    fn set_jumping(&self, jumping: bool) {
        self.living_base().set_jumping(jumping);
        if jumping {
            let pitch = (rand::random::<f32>() - rand::random::<f32>()).mul_add(0.2, 1.0) * 0.8;
            self.play_sound(Self::jump_sound(), self.sound_volume(), pitch);
        }
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    /// Vanilla parity: `Rabbit.aiStep`, which runs down the hop animation timer.
    fn ai_step(&self) -> Option<MoveResult> {
        let result = self.default_ai_step();
        AgeableMob::tick_ageable_mob(self);
        Animal::tick_animal_love(self);

        let finished_jump = {
            let mut state = self.state.lock();
            if state.jump_ticks != state.jump_duration {
                state.jump_ticks += 1;
                false
            } else if state.jump_duration != 0 {
                state.jump_ticks = 0;
                state.jump_duration = 0;
                true
            } else {
                false
            }
        };
        if finished_jump {
            self.set_jumping(false);
        }

        result
    }
}

impl AgeableMob for RabbitEntity {
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
}

impl Animal for RabbitEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        RabbitEntity::is_food(item_stack)
    }

    /// Vanilla parity: `Rabbit.getBreedOffspring`. One kit in twenty is born a
    /// stranger to both parents, taking the local wild variant instead.
    fn initialize_breed_offspring(&self, partner: &dyn Animal, offspring: &dyn Animal) {
        let Some(offspring) = offspring.downcast_ref::<Self>() else {
            log::error!("rabbit breeding produced a non-rabbit offspring");
            return;
        };
        let Some(world) = self.level() else {
            return;
        };

        let mut variant = Self::random_rabbit_variant(&world, self.block_position());
        if rand::random_range(0..20) != 0 {
            let partner_rabbit = partner.downcast_ref::<Self>();
            variant = match partner_rabbit {
                Some(partner_rabbit) if rand::random::<bool>() => partner_rabbit.variant(),
                _ => self.variant(),
            };
        }

        offspring.set_variant(variant);
    }
}

impl Mob for RabbitEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_RABBIT_AMBIENT)
    }

    /// Vanilla parity: `Rabbit.playAttackSound`; only the killer bunny has one.
    fn play_attack_sound(&self) {
        if self.variant() != RabbitVariant::Evil {
            return;
        }
        let pitch = (rand::random::<f32>() - rand::random::<f32>()).mul_add(0.2, 1.0);
        self.play_sound(&sound_events::ENTITY_RABBIT_ATTACK, 1.0, pitch);
    }

    /// Vanilla parity: `Rabbit.checkRabbitSpawnRules`.
    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        let _ = spawn_reason;
        world
            .get_block_state(pos.below())
            .get_block()
            .has_tag(&BlockTag::RABBITS_SPAWNABLE_ON)
            && <Self as Animal>::is_bright_enough_to_spawn(world.as_ref(), pos)
    }

    /// Vanilla parity: `Rabbit.finalizeSpawn`. The whole group shares one
    /// variant, which is why a warren is all one color.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        let group_data = match group_data {
            Some(SpawnGroupData::Rabbit(rabbit_group_data)) => {
                SpawnGroupData::Rabbit(rabbit_group_data)
            }
            _ => SpawnGroupData::Rabbit(RabbitGroupData::new(Self::random_rabbit_variant(
                world,
                self.block_position(),
            ))),
        };

        let SpawnGroupData::Rabbit(rabbit_group_data) = group_data else {
            unreachable!("rabbit group data was just constructed")
        };
        self.set_variant(rabbit_group_data.variant());
        self.finalize_spawn_ageable_mob(world, spawn_reason, Some(group_data))
    }

    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        Animal::mob_interact_animal(self, player, hand)
    }

    /// Vanilla parity: `Rabbit.customServerAiStep`. This is the whole hop cycle:
    /// count down the landing delay, let an evil rabbit pounce, and otherwise
    /// start a hop whenever the move control still wants to be somewhere else.
    fn custom_server_ai_step(&self) {
        {
            let mut state = self.state.lock();
            if state.jump_delay_ticks > 0 {
                state.jump_delay_ticks -= 1;
            }

            if state.more_carrot_ticks > 0 {
                state.more_carrot_ticks -= rand::random_range(0..3);
                if state.more_carrot_ticks < 0 {
                    state.more_carrot_ticks = 0;
                }
            }
        }

        if self.on_ground() {
            if !self.state.lock().was_on_ground {
                self.set_jumping(false);
                self.check_landing_delay();
            }

            if self.variant() == RabbitVariant::Evil && self.state.lock().jump_delay_ticks == 0 {
                let target = self.target();
                if let Some(target) = target
                    && self.position().distance_squared(target.position()) < EVIL_POUNCE_RANGE_SQR
                {
                    let target_position = target.position();
                    self.face_point(target_position.x, target_position.z);
                    let speed_modifier = self
                        .mob_base()
                        .controls()
                        .lock()
                        .move_control
                        .speed_modifier();
                    self.set_wanted_position(target_position, speed_modifier);
                    self.start_jumping();
                    self.state.lock().was_on_ground = true;
                }
            }

            let (want_jump, can_jump) = (
                self.mob_base().controls().lock().jump_control.want_jump(),
                self.jump_control.lock().can_jump(),
            );
            if want_jump {
                if !can_jump {
                    self.jump_control.lock().set_can_jump(true);
                }
            } else {
                let (has_wanted, wanted_position) = {
                    let controls = self.mob_base().controls().lock();
                    (
                        matches!(
                            controls.move_control.operation(),
                            MoveControlOperation::MoveTo
                        ),
                        controls.move_control.wanted_position(),
                    )
                };
                if has_wanted && self.state.lock().jump_delay_ticks == 0 {
                    let path_pos = {
                        let navigation = self.mob_base().navigation().lock();
                        navigation.path().and_then(|path| {
                            (!path.is_done())
                                .then(|| path.next_entity_pos(self.bounding_box().width()))
                                .flatten()
                        })
                    };
                    let pos = path_pos.unwrap_or(wanted_position);
                    self.face_point(pos.x, pos.z);
                    self.start_jumping();
                }
            }
        }

        self.state.lock().was_on_ground = self.on_ground();
    }

    /// Vanilla parity: `Rabbit.RabbitMoveControl.tick`.
    fn tick_move_control(&self) {
        let (operation, want_jump) = {
            let controls = self.mob_base().controls().lock();
            (
                controls.move_control.operation(),
                controls.jump_control.want_jump(),
            )
        };

        if self.on_ground() && !self.is_jumping() && !want_jump {
            self.set_speed_modifier(0.0);
        } else if matches!(
            operation,
            MoveControlOperation::MoveTo | MoveControlOperation::Jumping
        ) {
            let next_jump_speed = self.state.lock().next_jump_speed;
            self.set_speed_modifier(next_jump_speed);
        }

        self.default_tick_move_control();
    }

    /// Vanilla parity: `Rabbit.RabbitMoveControl.setWantedPosition`, which pins
    /// the swim speed and remembers what the next hop should cost.
    fn set_wanted_position(&self, position: DVec3, speed_modifier: f64) {
        let speed_modifier = if self.is_in_water() {
            1.5
        } else {
            speed_modifier
        };
        self.default_set_wanted_position(position, speed_modifier);
        if speed_modifier > 0.0 {
            self.state.lock().next_jump_speed = speed_modifier;
        }
    }

    /// Vanilla parity: `Rabbit.RabbitJumpControl.tick`, which starts a hop
    /// instead of forwarding the flag to `LivingEntity.jumping`.
    fn tick_jump_control(&self) {
        let want_jump = {
            let mut controls = self.mob_base().controls().lock();
            let want_jump = controls.jump_control.want_jump();
            if want_jump {
                controls.jump_control.clear_jump();
            }
            want_jump
        };
        if want_jump {
            self.start_jumping();
        }
    }
}

impl PathfinderMob for RabbitEntity {}

/// Keeps the rabbit hopping at panic speed for as long as it is fleeing.
///
/// Vanilla parity: `Rabbit.RabbitPanicGoal`.
struct RabbitPanicGoal {
    inner: PanicGoal,
    speed_modifier: f64,
}

impl RabbitPanicGoal {
    const fn new(speed_modifier: f64) -> Self {
        Self {
            inner: PanicGoal::new(speed_modifier),
            speed_modifier,
        }
    }
}

impl Goal for RabbitPanicGoal {
    fn controls(&self) -> GoalControls {
        self.inner.controls()
    }

    fn is_panic_goal(&self) -> bool {
        true
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.inner.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.inner.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.inner.stop(mob);
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.inner.tick(mob);
        if let Some(rabbit) = mob.downcast_ref::<RabbitEntity>() {
            rabbit.set_speed_modifier(self.speed_modifier);
        }
    }
}

/// Flees a player, a wolf or a monster, unless this rabbit is the killer bunny.
///
/// Vanilla parity: `Rabbit.RabbitAvoidEntityGoal`.
struct RabbitAvoidEntityGoal {
    inner: AvoidEntityGoal,
}

impl RabbitAvoidEntityGoal {
    fn new(
        max_dist: f32,
        matches: impl Fn(&dyn LivingEntity) -> bool + Send + Sync + 'static,
    ) -> Self {
        Self {
            inner: AvoidEntityGoal::with_selector(
                max_dist,
                FLEE_SPEED_MOD,
                FLEE_SPEED_MOD,
                // The five-argument vanilla constructor supplies
                // `NO_CREATIVE_OR_SPECTATOR` on top of the class test, so a
                // rabbit ignores a creative-mode player.
                move |_, target, _| no_creative_or_spectator(target) && matches(target),
            ),
        }
    }
}

impl Goal for RabbitAvoidEntityGoal {
    fn controls(&self) -> GoalControls {
        self.inner.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let is_evil = mob
            .downcast_ref::<RabbitEntity>()
            .is_some_and(|rabbit| rabbit.variant() == RabbitVariant::Evil);
        !is_evil && self.inner.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.inner.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.inner.stop(mob);
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.inner.tick(mob);
    }
}

/// Walks to a fully grown carrot and eats one growth stage out of it.
///
/// Vanilla parity: `Rabbit.RaidGardenGoal`. Vanilla keeps `wantsToRaid` and
/// `canRaid` as goal fields that its `isValidTarget` override writes; Steel
/// passes `isValidTarget` as a closure, so the two flags are shared with it.
struct RaidGardenGoal {
    inner: MoveToBlockGoal,
    wants_to_raid: Arc<AtomicBool>,
    can_raid: Arc<AtomicBool>,
}

impl RaidGardenGoal {
    fn new() -> Self {
        let wants_to_raid = Arc::new(AtomicBool::new(false));
        let can_raid = Arc::new(AtomicBool::new(false));
        let target_wants_to_raid = Arc::clone(&wants_to_raid);
        let target_can_raid = Arc::clone(&can_raid);
        Self {
            inner: MoveToBlockGoal::new(RAID_SPEED_MOD, RAID_SEARCH_RANGE, move |level, pos| {
                is_valid_carrot_target(level, pos, &target_wants_to_raid, &target_can_raid)
            }),
            wants_to_raid,
            can_raid,
        }
    }

    /// Vanilla parity: the `isReachedTarget` branch of `RaidGardenGoal.tick`.
    fn eat_carrot(&self, mob: &dyn PathfinderMob, world: &Arc<World>) {
        let crops_pos = self.inner.block_pos().above();
        let state = world.get_block_state(crops_pos);
        if state.get_block() != &vanilla_blocks::CARROTS {
            return;
        }

        let carrot_age = state.get_value(CARROT_AGE);
        if carrot_age == 0 {
            world.set_block(
                crops_pos,
                vanilla_blocks::AIR.default_state(),
                UpdateFlags::UPDATE_CLIENTS,
            );
            world.destroy_block(crops_pos, true);
        } else {
            world.set_block(
                crops_pos,
                state.set_value(CARROT_AGE, carrot_age - 1),
                UpdateFlags::UPDATE_CLIENTS,
            );
            world.game_event(
                &vanilla_game_events::BLOCK_CHANGE,
                crops_pos,
                &GameEventContext::new(Some(mob.as_entity_event_source()), None),
            );
            world.level_event(
                level_events::PARTICLES_DESTROY_BLOCK,
                crops_pos,
                i32::from(state.0),
                None,
            );
        }

        if let Some(rabbit) = mob.downcast_ref::<RabbitEntity>() {
            rabbit.state.lock().more_carrot_ticks = MORE_CARROTS_DELAY;
        }
    }
}

fn is_valid_carrot_target(
    level: &dyn LevelReader,
    pos: BlockPos,
    wants_to_raid: &AtomicBool,
    can_raid: &AtomicBool,
) -> bool {
    if !level
        .get_block_state(pos)
        .get_block()
        .has_tag(&BlockTag::SUPPORTS_CROPS)
        || !wants_to_raid.load(Ordering::Relaxed)
        || can_raid.load(Ordering::Relaxed)
    {
        return false;
    }

    let above = level.get_block_state(pos.above());
    if above.get_block() != &vanilla_blocks::CARROTS
        || above.get_value(CARROT_AGE) != CARROT_MAX_AGE
    {
        return false;
    }

    can_raid.store(true, Ordering::Relaxed);
    true
}

impl Goal for RaidGardenGoal {
    fn controls(&self) -> GoalControls {
        self.inner.controls()
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        if self.inner.next_start_tick() <= 0 {
            let Some(world) = mob.level() else {
                return false;
            };
            if !world.get_game_rule(&vanilla_game_rules::MOB_GRIEFING) {
                return false;
            }

            self.can_raid.store(false, Ordering::Relaxed);
            let wants_more_food = mob
                .downcast_ref::<RabbitEntity>()
                .is_some_and(RabbitEntity::wants_more_food);
            self.wants_to_raid.store(wants_more_food, Ordering::Relaxed);
        }

        self.inner.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.can_raid.load(Ordering::Relaxed) && self.inner.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.inner.stop(mob);
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.inner.tick(mob);

        let block_pos = self.inner.block_pos();
        mob.mob_base().controls().lock().look_control.set_look_at(
            DVec3::new(
                f64::from(block_pos.x()) + 0.5,
                f64::from(block_pos.y() + 1),
                f64::from(block_pos.z()) + 0.5,
            ),
            10.0,
            mob.max_head_x_rot(),
        );

        if !self.inner.is_reached_target() {
            return;
        }

        if self.can_raid.load(Ordering::Relaxed)
            && let Some(world) = mob.level()
        {
            self.eat_carrot(mob, &world);
        }

        self.can_raid.store(false, Ordering::Relaxed);
        self.inner.set_next_start_tick(RAID_RESTART_DELAY_TICKS);
    }
}

#[cfg(test)]
mod tests;
