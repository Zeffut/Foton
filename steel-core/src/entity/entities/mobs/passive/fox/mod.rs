//! Fox entity.
//!
//! Vanilla parity: `Fox`. The largest goal list of the tameable batch, and the
//! only one of the five that is not tameable at all: a fox is won over by being
//! *bred*, and the two kits inherit their parents' trust. Everything else about
//! it -- sleeping through the day, carrying one item in its mouth, stalking a
//! chicken and faceplanting into the snow -- hangs off the seven flag bits and
//! the two trusted-entity slots in its synchronized data.

mod goals;

use std::ptr;
use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_macros::entity_behavior;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::BlockStateProperties;
use steel_registry::data_components::vanilla_components::{CONSUMABLE, FOOD};
use steel_registry::entity_type::MobCategory;
use steel_registry::entity_type::{
    EntityAttachmentPoint, EntityAttachments, EntityDimensions, EntityTypeRef,
};
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_biome_tags::BiomeTag;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_entity_data::FoxEntityData;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{
    REGISTRY, TaggedRegistryExt, sound_events, vanilla_blocks, vanilla_entities,
    vanilla_game_rules::MOB_GRIEFING, vanilla_items,
};
use steel_utils::entity_events::EntityStatus;
use steel_utils::locks::SyncMutex;
use steel_utils::types::{InteractionHand, UpdateFlags};
use steel_utils::{BlockPos, Downcast as _, DowncastType, DowncastTypeKey, UuidExt};
use uuid::Uuid;

use crate::behavior::{ITEM_BEHAVIORS, InteractionResult};
use crate::entity::ai::goal::{
    ClimbOnTopOfPowderSnowGoal, LeapAtTargetGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::ai::path::PathType;
use crate::entity::damage::DamageSource;
use crate::entity::entities::ItemEntity;
use crate::entity::{
    AgeableMob, AgeableMobBase, Animal, AnimalBase, Entity, EntityBase, EntityBaseLoad, EntityPose,
    EntitySpawnReason, EntitySyncedData, LivingEntity, LivingEntityBase, LivingEntitySyncedData,
    LivingTravelInput, Mob, MobBase, PathfinderMob, RemovalReason, SharedEntity, SpawnGroupData,
};

use crate::inventory::equipment::EquipmentSlot;
use crate::physics::MoveResult;
use crate::player::Player;
use crate::world::World;

use goals::{
    DefendTrustedTargetGoal, FaceplantGoal, FoxBreedGoal, FoxEatBerriesGoal, FoxFloatGoal,
    FoxFollowParentGoal, FoxLookAtPlayerGoal, FoxMeleeAttackGoal, FoxPounceGoal,
    FoxSearchForItemsGoal, PerchAndSearchGoal, SeekShelterGoal, SleepGoal, StalkPreyGoal,
    fox_avoid_players_goal, fox_avoid_wolves_goal, fox_panic_goal, fox_prey_target_goal,
    prey_goal_priorities,
};

/// Bit of the synced flag byte for each fox state.
///
/// Vanilla parity: the seven `FLAG_*` constants of `Fox`.
const FLAG_SITTING: i8 = 1;
const FLAG_CROUCHING: i8 = 4;
const FLAG_INTERESTED: i8 = 8;
const FLAG_POUNCING: i8 = 16;
const FLAG_SLEEPING: i8 = 32;
const FLAG_FACEPLANTED: i8 = 64;
/// Vanilla's `FLAG_DEFENDING` is 128, which is this byte's sign bit.
const FLAG_DEFENDING: i8 = i8::MIN;

/// How long a fox holds food before it eats it.
///
/// Vanilla parity: `Fox.MIN_TICKS_BEFORE_EAT`.
const MIN_TICKS_BEFORE_EAT: i32 = 600;

/// When a fox starts chewing visibly.
///
/// Vanilla parity: the `ticksSinceEaten > 560` of `Fox.aiStep`.
const START_CHEWING_TICKS: i32 = 560;

/// How far a fox may crouch.
///
/// Vanilla parity: `Fox.MAX_CROUCH_AMOUNT`.
const MAX_CROUCH_AMOUNT: f32 = 5.0;

/// How much closer to fully crouched one tick gets a fox.
///
/// Vanilla parity: the `crouchAmount += 0.2F` of `Fox.tick`.
const CROUCH_STEP: f32 = 0.2;

/// How far a fox can see prey and follow it.
///
/// Vanilla parity: the `getNavigation().setRequiredPathLength(32.0F)` of the
/// constructor, which pairs with the fox's 32-block follow range.
const REQUIRED_PATH_LENGTH: f32 = 32.0;

/// The fox's baby scale and hitbox.
///
/// Vanilla parity: `Fox.BABY_SCALE` and `Fox.BABY_DIMENSIONS`.
const BABY_SCALE: f32 = 0.6;
const FOX_BABY_PASSENGER_ATTACHMENTS: [EntityAttachmentPoint; 1] =
    [EntityAttachmentPoint::new(0.0, 0.375, 0.0)];

/// The two coats a fox comes in.
///
/// Vanilla parity: `Fox.Variant`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FoxVariant {
    /// The ordinary forest fox.
    #[default]
    Red,
    /// The white fox of the snowy taigas.
    Snow,
}

impl FoxVariant {
    /// Returns the synchronized id.
    #[must_use]
    pub const fn id(self) -> i32 {
        match self {
            Self::Red => 0,
            Self::Snow => 1,
        }
    }

    /// Returns the variant for a synchronized id.
    ///
    /// Vanilla parity: `Fox.Variant.byId`, whose out-of-bounds strategy is
    /// `ZERO` rather than the parrot's clamp.
    #[must_use]
    pub const fn by_id(id: i32) -> Self {
        if id == 1 { Self::Snow } else { Self::Red }
    }

    /// Returns the serialized name.
    ///
    /// Vanilla parity: `Fox.Variant.getSerializedName`.
    #[must_use]
    pub const fn serialized_name(self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Snow => "snow",
        }
    }

    /// Returns the variant for a serialized name.
    #[must_use]
    pub fn from_serialized_name(name: &str) -> Self {
        if name == "snow" {
            Self::Snow
        } else {
            Self::Red
        }
    }
}

/// Vanilla fox entity.
#[entity_behavior(class = "Fox")]
pub struct FoxEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    /// How long the fox has held the food in its mouth.
    ///
    /// Vanilla parity: `Fox.ticksSinceEaten`.
    ticks_since_eaten: SyncMutex<i32>,
    /// How far into the crouch the fox is.
    ///
    /// Vanilla parity: `Fox.crouchAmount`, which the pounce goal reads back
    /// through `isFullyCrouched`.
    crouch_amount: SyncMutex<f32>,
    entity_data: SyncMutex<FoxEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `FoxEntity`.
unsafe impl DowncastType for FoxEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/fox");
}

impl FoxEntity {
    /// Creates a new fox entity.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a fox entity from saved base data.
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
        let animal_base = AnimalBase::new();
        AnimalBase::initialize_pathfinding_malus(&mob_base);
        let mut entity_data = FoxEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        let fox = Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base: AgeableMobBase::new(),
            animal_base,
            ticks_since_eaten: SyncMutex::new(0),
            crouch_amount: SyncMutex::new(0.0),
            entity_data: SyncMutex::new(entity_data),
        };

        // Vanilla parity: the constructor walks a fox straight through damaging
        // blocks, which is how it crosses a berry bush without hesitating.
        fox.set_pathfinding_malus(PathType::DamagingInNeighbor, 0.0);
        fox.set_pathfinding_malus(PathType::Damaging, 0.0);
        fox.set_can_pick_up_loot(true);
        fox.mob_base
            .navigation()
            .lock()
            .set_required_path_length(REQUIRED_PATH_LENGTH, f64::from(REQUIRED_PATH_LENGTH));
        fox.register_goals();
        fox
    }

    /// Vanilla parity: `Fox.registerGoals`.
    fn register_goals(&self) {
        let mut goals = self.mob_base.goal_selector().lock();
        goals.add_goal(0, FoxFloatGoal::new(&self.mob_base));
        goals.add_goal(0, ClimbOnTopOfPowderSnowGoal::new());
        goals.add_goal(1, FaceplantGoal::new());
        goals.add_goal(2, fox_panic_goal(2.2));
        goals.add_goal(3, FoxBreedGoal::new(1.0));
        goals.add_goal(4, fox_avoid_players_goal());
        goals.add_goal(4, fox_avoid_wolves_goal());
        // Vanilla parity gap: the third avoid goal flees polar bears, which
        // Steel does not have yet.
        goals.add_goal(5, StalkPreyGoal);
        goals.add_goal(6, FoxPounceGoal);
        goals.add_goal(6, SeekShelterGoal::new(1.25));
        goals.add_goal(7, FoxMeleeAttackGoal::new(1.2, true));
        goals.add_goal(7, SleepGoal::new());
        goals.add_goal(8, FoxFollowParentGoal::new(1.25));
        // Vanilla parity gap: `Fox.FoxStrollThroughVillageGoal` sends a fox
        // wandering through a village. Steel has no village points of interest,
        // so the goal has nothing to path to.
        goals.add_goal(10, FoxEatBerriesGoal::new(1.2, 12, 1));
        goals.add_goal(10, LeapAtTargetGoal::new(0.4));
        goals.add_goal(11, WaterAvoidingRandomStrollGoal::new(1.0));
        goals.add_goal(11, FoxSearchForItemsGoal);
        goals.add_goal(12, FoxLookAtPlayerGoal::new(24.0));
        goals.add_goal(13, PerchAndSearchGoal::new());
        drop(goals);

        self.mob_base
            .target_selector()
            .lock()
            .add_goal(3, DefendTrustedTargetGoal::new());
    }

    /// Registers the two prey goals this fox's coat prefers.
    ///
    /// Vanilla parity: `Fox.setTargetGoals`, which a fox runs once it knows its
    /// variant -- on spawn and on load.
    ///
    /// Vanilla parity gap: the turtle-egg goal of the same method is missing
    /// with the turtle itself.
    fn set_target_goals(&self) {
        let (land_priority, fish_priority) = prey_goal_priorities(self.variant());
        let mut targets = self.mob_base.target_selector().lock();
        targets.add_goal(
            land_priority,
            fox_prey_target_goal(10, Self::is_stalkable_prey),
        );
        targets.add_goal(
            fish_priority,
            fox_prey_target_goal(20, Self::is_schooling_fish),
        );
    }

    /// Returns the current fox variant.
    #[must_use]
    pub fn variant(&self) -> FoxVariant {
        FoxVariant::by_id(*self.entity_data.lock().variant_type.get())
    }

    /// Sets the current fox variant.
    pub fn set_variant(&self, variant: FoxVariant) {
        self.entity_data.lock().variant_type.set(variant.id());
    }

    fn flag(&self, flag: i8) -> bool {
        *self.entity_data.lock().flags.get() & flag != 0
    }

    fn set_flag(&self, flag: i8, value: bool) {
        let mut entity_data = self.entity_data.lock();
        let current = *entity_data.flags.get();
        entity_data.flags.set(if value {
            current | flag
        } else {
            current & !flag
        });
    }

    /// Returns vanilla `Fox.isSitting`.
    #[must_use]
    pub fn is_sitting(&self) -> bool {
        self.flag(FLAG_SITTING)
    }

    /// Sets vanilla `Fox.setSitting`.
    pub fn set_sitting(&self, value: bool) {
        self.set_flag(FLAG_SITTING, value);
    }

    /// Returns vanilla `Fox.isCrouching`.
    ///
    /// Named apart from [`Entity::is_crouching`], which the fox overrides to
    /// return this flag rather than the shared pose.
    #[must_use]
    pub fn is_crouching_flag(&self) -> bool {
        self.flag(FLAG_CROUCHING)
    }

    /// Sets vanilla `Fox.setIsCrouching`.
    pub fn set_is_crouching(&self, value: bool) {
        self.set_flag(FLAG_CROUCHING, value);
    }

    /// Returns vanilla `Fox.isInterested`.
    #[must_use]
    pub fn is_interested(&self) -> bool {
        self.flag(FLAG_INTERESTED)
    }

    /// Sets vanilla `Fox.setIsInterested`.
    pub fn set_is_interested(&self, value: bool) {
        self.set_flag(FLAG_INTERESTED, value);
    }

    /// Returns vanilla `Fox.isPouncing`.
    #[must_use]
    pub fn is_pouncing(&self) -> bool {
        self.flag(FLAG_POUNCING)
    }

    /// Sets vanilla `Fox.setIsPouncing`.
    pub fn set_is_pouncing(&self, value: bool) {
        self.set_flag(FLAG_POUNCING, value);
    }

    /// Returns vanilla `Fox.isSleeping`.
    #[must_use]
    pub fn is_sleeping(&self) -> bool {
        self.flag(FLAG_SLEEPING)
    }

    /// Sets vanilla `Fox.setSleeping`.
    pub fn set_sleeping(&self, value: bool) {
        self.set_flag(FLAG_SLEEPING, value);
    }

    /// Returns vanilla `Fox.isFaceplanted`.
    #[must_use]
    pub fn is_faceplanted(&self) -> bool {
        self.flag(FLAG_FACEPLANTED)
    }

    /// Sets vanilla `Fox.setFaceplanted`.
    pub fn set_faceplanted(&self, value: bool) {
        self.set_flag(FLAG_FACEPLANTED, value);
    }

    /// Returns vanilla `Fox.isDefending`.
    #[must_use]
    pub fn is_defending(&self) -> bool {
        self.flag(FLAG_DEFENDING)
    }

    /// Sets vanilla `Fox.setDefending`.
    pub fn set_defending(&self, value: bool) {
        self.set_flag(FLAG_DEFENDING, value);
    }

    /// Returns vanilla `Fox.isFullyCrouched`.
    #[must_use]
    pub fn is_fully_crouched(&self) -> bool {
        (*self.crouch_amount.lock() - MAX_CROUCH_AMOUNT).abs() < f32::EPSILON
    }

    /// Resets the crouch, as the pounce goal does when it stops.
    pub fn reset_crouch_amount(&self) {
        *self.crouch_amount.lock() = 0.0;
    }

    /// Returns vanilla `Fox.canMove`.
    #[must_use]
    pub fn can_move(&self) -> bool {
        !self.is_sleeping() && !self.is_sitting() && !self.is_faceplanted()
    }

    /// Clears every pose flag at once.
    ///
    /// Vanilla parity: `Fox.clearStates`.
    pub fn clear_states(&self) {
        self.set_is_interested(false);
        self.set_is_crouching(false);
        self.set_sitting(false);
        self.set_sleeping(false);
        self.set_defending(false);
        self.set_faceplanted(false);
    }

    /// Returns the two entities this fox trusts.
    ///
    /// Vanilla parity: `Fox.getTrustedEntities`.
    #[must_use]
    pub fn trusted_uuids(&self) -> [Option<Uuid>; 2] {
        let entity_data = self.entity_data.lock();
        [
            *entity_data.trusted_id_0.get(),
            *entity_data.trusted_id_1.get(),
        ]
    }

    /// Vanilla parity: `Fox.addTrustedEntity`.
    pub fn add_trusted_entity(&self, uuid: Uuid) {
        let mut entity_data = self.entity_data.lock();
        if entity_data.trusted_id_0.get().is_some() {
            entity_data.trusted_id_1.set(Some(uuid));
        } else {
            entity_data.trusted_id_0.set(Some(uuid));
        }
    }

    /// Vanilla parity: `Fox.clearTrusted`.
    fn clear_trusted(&self) {
        let mut entity_data = self.entity_data.lock();
        entity_data.trusted_id_0.set(None);
        entity_data.trusted_id_1.set(None);
    }

    /// Returns vanilla `Fox.trusts`.
    #[must_use]
    pub fn trusts(&self, entity: &dyn Entity) -> bool {
        self.trusted_uuids()
            .into_iter()
            .flatten()
            .any(|trusted| trusted == entity.uuid())
    }

    /// Returns the first trusted entity that is loaded.
    #[must_use]
    pub fn first_trusted_entity(&self) -> Option<SharedEntity> {
        let world = self.level()?;
        self.trusted_uuids()
            .into_iter()
            .flatten()
            .find_map(|uuid| world.get_entity_by_uuid(&uuid))
    }

    /// Returns whether this entity is prey a fox stalks.
    ///
    /// Vanilla parity: `Fox.STALKABLE_PREY`.
    #[must_use]
    pub fn is_stalkable_prey(target: &dyn LivingEntity) -> bool {
        let entity_type = target.entity_type();
        entity_type == &vanilla_entities::CHICKEN || entity_type == &vanilla_entities::RABBIT
    }

    /// Returns whether this entity is a schooling fish a snow fox hunts.
    ///
    /// Vanilla parity: the `target instanceof AbstractSchoolingFish` selector of
    /// the `fishTargetGoal`.
    #[must_use]
    pub fn is_schooling_fish(target: &dyn LivingEntity) -> bool {
        let entity_type = target.entity_type();
        entity_type == &vanilla_entities::COD
            || entity_type == &vanilla_entities::SALMON
            || entity_type == &vanilla_entities::TROPICAL_FISH
    }

    /// Returns whether this entity is a monster.
    ///
    /// Vanilla parity: the `target instanceof Monster` branch of
    /// `FoxAlertableEntitiesSelector`.
    ///
    /// Steel matches on the spawn category rather than a Java class, which is
    /// the same set for everything a fox meets on the surface.
    #[must_use]
    pub fn is_monster(target: &dyn LivingEntity) -> bool {
        target.entity_type().mob_category == MobCategory::Monster
    }

    /// Returns whether this entity has attacked something recently.
    ///
    /// Vanilla parity: `Fox.TRUSTED_TARGET_SELECTOR`.
    #[must_use]
    pub fn is_recent_aggressor(target: &dyn LivingEntity) -> bool {
        target.last_hurt_mob().is_some()
            && target.last_hurt_mob_timestamp() < target.tick_count() + 600
    }

    /// Returns whether a fox has a clear line to pounce along.
    ///
    /// Vanilla parity: `Fox.isPathClear`.
    #[must_use]
    pub fn is_path_clear(fox: &Self, target: &dyn Entity) -> bool {
        let Some(world) = fox.level() else {
            return false;
        };

        let position = fox.position();
        let zdiff = target.position().z - position.z;
        let xdiff = target.position().x - position.x;
        let slope = zdiff / xdiff;

        for step in 0..6 {
            let fraction = f64::from(step) / 6.0;
            let z = if slope == 0.0 { 0.0 } else { zdiff * fraction };
            let x = if slope == 0.0 {
                xdiff * fraction
            } else {
                z / slope
            };

            for height in 1..4 {
                let pos = BlockPos::containing(
                    position.x + x,
                    position.y + f64::from(height),
                    position.z + z,
                );
                if !world.get_block_state(pos).is_replaceable() {
                    return false;
                }
            }
        }

        true
    }

    /// Returns whether the fox's mouth is empty.
    #[must_use]
    pub fn mouth_item_is_empty(&self) -> bool {
        !self.has_item_in_slot(EquipmentSlot::MainHand)
    }

    /// Returns whether a stack is something the fox would eat.
    ///
    /// Vanilla parity: `Fox.isConsumableFood`.
    #[must_use]
    fn is_consumable_food(item_stack: &ItemStack) -> bool {
        item_stack.has(FOOD) && item_stack.has(CONSUMABLE)
    }

    /// Vanilla parity: `Fox.spitOutItem`.
    fn spit_out_item(&self, item_stack: ItemStack) {
        if item_stack.is_empty() {
            return;
        }
        let Some(world) = self.level() else {
            return;
        };

        let look = self.look_angle();
        let position = self.position();
        let spit_position = DVec3::new(position.x + look.x, position.y + 1.0, position.z + look.z);
        if let Some(thrown) = world.spawn_item(spit_position, item_stack) {
            thrown.set_pickup_delay(40);
            thrown.set_thrower(self.uuid());
        }
        self.play_sound(&sound_events::ENTITY_FOX_SPIT, 1.0, 1.0);
    }

    /// Vanilla parity: the eating half of `Fox.aiStep`.
    fn tick_eating(&self) {
        let Some(world) = self.level() else {
            return;
        };

        *self.ticks_since_eaten.lock() += 1;
        let mut mouth_item = ItemStack::empty();
        self.with_equipment_slot(EquipmentSlot::MainHand, &mut |stack| {
            mouth_item = stack.copy_with_count(stack.count());
        });
        if !self.can_eat(&mouth_item) {
            return;
        }

        let ticks_since_eaten = *self.ticks_since_eaten.lock();
        if ticks_since_eaten > MIN_TICKS_BEFORE_EAT {
            let remaining = ITEM_BEHAVIORS.get_behavior(mouth_item.item()).finish_using(
                &mut mouth_item,
                &world,
                self,
            );
            self.living_base
                .equipment()
                .lock()
                .set(EquipmentSlot::MainHand, remaining);
            *self.ticks_since_eaten.lock() = 0;
        } else if ticks_since_eaten > START_CHEWING_TICKS && rand::random::<f32>() < 0.1 {
            self.play_eating_sound();
            self.broadcast_entity_event(EntityStatus::FoxEat);
        }
    }

    /// Vanilla parity: `Fox.canEat`.
    fn can_eat(&self, mouth_item: &ItemStack) -> bool {
        Self::is_consumable_food(mouth_item)
            && self.target().is_none()
            && self.on_ground()
            && !self.is_sleeping()
    }

    /// Vanilla parity: `Fox.wakeUp`, plus the conditions of `Fox.tick`.
    fn tick_wake_conditions(&self) {
        let Some(world) = self.level() else {
            return;
        };

        let in_water = self.is_in_water();
        if in_water || self.target().is_some() || world.is_thundering() {
            self.set_sleeping(false);
        }
        if in_water || self.is_sleeping() {
            self.set_sitting(false);
        }

        if self.is_faceplanted() && rand::random::<f32>() < 0.2 {
            let pos = self.block_position();
            let state = world.get_block_state(pos);
            world.destroy_block_effect(pos, u32::from(state.0), None);
        }
    }

    /// Vanilla parity: the crouch interpolation of `Fox.tick`, which the pounce
    /// goal reads back through `isFullyCrouched`.
    fn tick_crouch_amount(&self) {
        let mut crouch_amount = self.crouch_amount.lock();
        if self.is_crouching_flag() {
            *crouch_amount = (*crouch_amount + CROUCH_STEP).min(MAX_CROUCH_AMOUNT);
        } else {
            *crouch_amount = 0.0;
        }
    }

    /// Takes the berries off a bush the fox has reached.
    ///
    /// Vanilla parity: `Fox.FoxEatBerriesGoal.onReachedTarget`.
    pub fn pick_berries(&self, pos: BlockPos) {
        let Some(world) = self.level() else {
            return;
        };
        if !world.get_game_rule(&MOB_GRIEFING) {
            return;
        }

        let state = world.get_block_state(pos);
        if state.get_block() != &vanilla_blocks::SWEET_BERRY_BUSH {
            // Vanilla parity gap: `CaveVines.use` also hands a fox glow
            // berries. Steel's cave-vine behavior exposes no server-side pick
            // that is not a player interaction, so only the bush is picked.
            return;
        }

        let age = state.get_value(&BlockStateProperties::AGE_3);
        let mut count = 1 + rand::random_range(0..2) + i32::from(age == 3);
        if self.mouth_item_is_empty() {
            self.living_base.equipment().lock().set(
                EquipmentSlot::MainHand,
                ItemStack::new(&vanilla_items::SWEET_BERRIES),
            );
            count -= 1;
        }

        if count > 0 {
            let (x, y, z) = pos.get_center();
            world.spawn_item(
                DVec3::new(x, y, z),
                ItemStack::with_count(&vanilla_items::SWEET_BERRIES, count),
            );
        }

        self.play_sound(&sound_events::BLOCK_SWEET_BERRY_BUSH_PICK_BERRIES, 1.0, 1.0);
        world.set_block(
            pos,
            state.set_value(&BlockStateProperties::AGE_3, 1),
            UpdateFlags::UPDATE_CLIENTS,
        );
    }

    fn update_dirty_mob_effect_entity_data(&self) {
        if !self.living_base.take_effects_dirty() {
            return;
        }

        let display = self.living_base.mob_effect_display_state();

        {
            let mut entity_data = self.entity_data.lock();
            let living = entity_data.living_entity_mut();
            living.effect_particles.set(display.particles);
            living.effect_ambience.set(display.ambient);
        }

        self.entity_data.set_base_invisible_flag(display.invisible);
        self.entity_data
            .set_base_glowing_flag(self.has_glowing_tag() || display.glowing);
    }
}

/// Returns whether a fox may appear at `pos`.
///
/// Vanilla parity: `Fox.checkFoxSpawnRules`.
#[must_use]
fn check_fox_spawn_rules(world: &Arc<World>, pos: BlockPos) -> bool {
    world
        .get_block_state(pos.below())
        .get_block()
        .has_tag(&BlockTag::FOXES_SPAWNABLE_ON)
        && <FoxEntity as Animal>::is_bright_enough_to_spawn(world.as_ref(), pos)
}

impl Entity for FoxEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);
    }

    fn tick(&self) {
        LivingEntity::tick_living_entity(self);
        if self.is_effective_ai() {
            self.tick_wake_conditions();
        }
        self.tick_crouch_amount();
        // VANILLA CLIENT-LOCAL: the interested-angle interpolation of
        // `Fox.tick` only tilts the head on the client.
    }

    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let scale = LivingEntity::get_scale(self);
        if AgeableMob::is_baby(self) {
            baby_dimensions(self.entity_type).scale(scale)
        } else if self.entity_type.fixed {
            self.entity_type.dimensions
        } else {
            self.entity_type.dimensions.scale(scale)
        }
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn update_data_before_sync(&self) {
        self.update_dirty_mob_effect_entity_data();
    }

    /// Vanilla parity: `Fox.isCrouching`, which reads the fox's own flag rather
    /// than the shared pose.
    fn is_crouching(&self) -> bool {
        self.is_crouching_flag()
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);

        let trusted: Vec<NbtTag> = self
            .trusted_uuids()
            .into_iter()
            .flatten()
            .map(|uuid| NbtTag::IntArray(uuid.to_int_array().to_vec()))
            .collect();
        if !trusted.is_empty() {
            nbt.insert("Trusted", NbtTag::List(NbtList::from(trusted)));
        }
        nbt.insert("Sleeping", i8::from(self.is_sleeping()));
        nbt.insert("Type", self.variant().serialized_name());
        nbt.insert("Sitting", i8::from(self.is_sitting()));
        nbt.insert("Crouching", i8::from(self.is_crouching_flag()));
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);

        self.clear_trusted();
        if let Some(trusted) = nbt.list("Trusted")
            && let Some(entries) = trusted.int_arrays()
        {
            for entry in entries {
                if let Some(uuid) = Uuid::from_int_array(&entry.to_vec()) {
                    self.add_trusted_entity(uuid);
                }
            }
        }

        self.set_sleeping(nbt.byte("Sleeping").is_some_and(|value| value != 0));
        self.set_variant(nbt.string("Type").map_or(FoxVariant::Red, |name| {
            FoxVariant::from_serialized_name(name.to_str().as_ref())
        }));
        self.set_sitting(nbt.byte("Sitting").is_some_and(|value| value != 0));
        self.set_is_crouching(nbt.byte("Crouching").is_some_and(|value| value != 0));
        self.set_target_goals();
    }
}

/// Vanilla parity: `Fox.BABY_DIMENSIONS`.
fn baby_dimensions(entity_type: EntityTypeRef) -> EntityDimensions {
    let scaled = entity_type.dimensions.scale(BABY_SCALE);
    EntityDimensions::new_with_attachments(
        scaled.width,
        scaled.height,
        0.34375,
        EntityAttachments::new(&FOX_BABY_PASSENGER_ATTACHMENTS, &[], &[], &[]),
    )
}

impl LivingEntity for FoxEntity {
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
        Some(&sound_events::ENTITY_FOX_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_FOX_DEATH)
    }

    /// Vanilla parity: `Fox.isImmobile`, which is narrower than the living
    /// default: a sleeping fox is held still by its goals, not by immobility.
    fn is_immobile(&self) -> bool {
        self.is_dead_or_dying()
    }

    /// Vanilla parity: `Fox.dropAllDeathLoot`, which spits the mouth item out
    /// before the loot table runs so it is never duplicated.
    fn drop_all_death_loot(&self, source: &DamageSource) {
        let mut mouth_item = ItemStack::empty();
        self.with_equipment_slot(EquipmentSlot::MainHand, &mut |stack| {
            mouth_item = stack.copy_with_count(stack.count());
        });
        if !mouth_item.is_empty() {
            self.spawn_at_location(mouth_item, 0.0);
            self.living_base
                .equipment()
                .lock()
                .set(EquipmentSlot::MainHand, ItemStack::empty());
        }

        self.living_drop_all_death_loot(source);
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    /// Vanilla parity: `Fox.aiStep`.
    fn ai_step(&self) -> Option<MoveResult> {
        if Entity::is_alive(self) && self.is_effective_ai() {
            self.tick_eating();
            if self.target().is_none_or(|target| !target.is_alive()) {
                self.set_is_crouching(false);
                self.set_is_interested(false);
            }
        }

        if self.is_sleeping() || self.is_immobile() {
            self.set_jumping(false);
            self.set_travel_input(LivingTravelInput::new(0.0, 0.0, 0.0));
        }

        let result = self.default_ai_step();
        AgeableMob::tick_ageable_mob(self);
        Animal::tick_animal_love(self);

        if self.is_defending() && rand::random::<f32>() < 0.05 {
            self.play_sound(&sound_events::ENTITY_FOX_AGGRO, 1.0, 1.0);
        }

        result
    }
}

impl AgeableMob for FoxEntity {
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

impl Animal for FoxEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::FOX_FOOD)
    }

    fn play_eating_sound(&self) {
        self.play_sound(&sound_events::ENTITY_FOX_EAT, 1.0, 1.0);
    }

    /// Vanilla parity: `Fox.getBreedOffspring` plus the trust the kit inherits
    /// in `Fox.FoxBreedGoal.breed`.
    fn initialize_breed_offspring(&self, partner: &dyn Animal, offspring: &dyn Animal) {
        let Some(kit) = offspring.as_entity_event_source().downcast_ref::<Self>() else {
            return;
        };

        let inherit_from_self = rand::random::<bool>();
        let variant = if inherit_from_self {
            self.variant()
        } else {
            partner
                .as_entity_event_source()
                .downcast_ref::<Self>()
                .map_or_else(|| self.variant(), Self::variant)
        };
        kit.set_variant(variant);
        kit.set_target_goals();

        // Vanilla parity: the kit trusts both players that bred its parents.
        let own_love_cause = self.love_cause_uuid();
        let partner_love_cause = partner.love_cause_uuid();
        if let Some(uuid) = own_love_cause {
            kit.add_trusted_entity(uuid);
        }
        if let Some(uuid) = partner_love_cause
            && Some(uuid) != own_love_cause
        {
            kit.add_trusted_entity(uuid);
        }
    }
}

impl Mob for FoxEntity {
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

    fn custom_server_ai_step(&self) {
        Animal::custom_server_ai_step_animal(self);
    }

    /// Vanilla parity: `Fox.FoxMoveControl`, which only ticks while the fox is
    /// free to move.
    fn tick_move_control(&self) {
        if !self.can_move() {
            return;
        }
        self.default_tick_move_control();
    }

    /// Vanilla parity: `Fox.FoxLookControl`, which holds still while asleep.
    fn tick_look_control(&self) {
        if self.is_sleeping() {
            return;
        }
        self.default_tick_look_control();
    }

    /// Vanilla parity: the `resetXRotOnTick` override of `Fox.FoxLookControl`,
    /// which is what keeps a pouncing or crouching fox's nose down.
    fn look_control_resets_pitch(&self) -> bool {
        !self.is_pouncing()
            && !self.is_crouching_flag()
            && !self.is_interested()
            && !self.is_faceplanted()
    }

    /// Vanilla parity: `Fox.setTarget`, which stops defending once the target
    /// is gone.
    fn set_target(&self, target: Option<&SharedEntity>) -> bool {
        if self.is_defending() && target.is_none() {
            self.set_defending(false);
        }
        self.mob_base().set_target(target, |_| true)
    }

    /// Vanilla parity: `Fox.canHoldItem`, which lets a fox swap a trinket for
    /// something it can actually eat.
    fn can_hold_item(&self, item_stack: &ItemStack) -> bool {
        let mut held = ItemStack::empty();
        self.with_equipment_slot(EquipmentSlot::MainHand, &mut |stack| {
            held = stack.copy_with_count(stack.count());
        });

        held.is_empty()
            || *self.ticks_since_eaten.lock() > 0
                && Self::is_consumable_food(item_stack)
                && !Self::is_consumable_food(&held)
    }

    /// Vanilla parity: `Fox.pickUpItem`, which is what puts the item in the
    /// fox's mouth rather than in an armor slot.
    fn pick_up_item(&self, world: &Arc<World>, item_entity: &SharedEntity) {
        let Some(item) = item_entity.downcast_ref::<ItemEntity>() else {
            return;
        };
        let mut item_stack = item.get_item();
        if !self.can_hold_item(&item_stack) {
            return;
        }

        let count = item_stack.count();
        if count > 1 {
            let extra = item_stack.split(count - 1);
            world.spawn_item(item_entity.position(), extra);
        }

        let mut previous = ItemStack::empty();
        self.with_equipment_slot(EquipmentSlot::MainHand, &mut |stack| {
            previous = stack.copy_with_count(stack.count());
        });
        self.spit_out_item(previous);

        self.living_base
            .equipment()
            .lock()
            .set(EquipmentSlot::MainHand, item_stack.split(1));
        Mob::set_guaranteed_drop(self, EquipmentSlot::MainHand);
        item_entity.set_removed(RemovalReason::Discarded);
        *self.ticks_since_eaten.lock() = 0;
    }

    /// Vanilla parity: `Fox.getAmbientSound`, whose night screech is the reason
    /// a dark forest sounds like that.
    fn ambient_sound(&self) -> Option<SoundEventRef> {
        if self.is_sleeping() {
            return Some(&sound_events::ENTITY_FOX_SLEEP);
        }

        let alone_in_the_dark = self.level().is_some_and(|world| {
            !world.is_bright_outside()
                && rand::random::<f32>() < 0.1
                && world
                    .nearest_player(self.position(), 16.0, |player| !player.is_spectator())
                    .is_none()
        });
        if alone_in_the_dark {
            return Some(&sound_events::ENTITY_FOX_SCREECH);
        }

        Some(&sound_events::ENTITY_FOX_AMBIENT)
    }

    /// Vanilla parity: `Fox.playAmbientSound`, which doubles the volume of the
    /// screech and leaves everything else alone.
    fn play_ambient_sound(&self) {
        let Some(sound) = self.ambient_sound() else {
            return;
        };
        if ptr::eq(sound, &raw const sound_events::ENTITY_FOX_SCREECH) {
            self.play_sound(sound, 2.0, self.voice_pitch());
        } else {
            self.play_sound(sound, self.sound_volume(), self.voice_pitch());
        }
    }

    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        _spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        check_fox_spawn_rules(world, pos)
    }

    /// Vanilla parity: `Fox.finalizeSpawn`, which keeps a whole litter one coat
    /// and makes the later members of it kits.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        let variant = world
            .biome_at(self.block_position())
            .map_or(FoxVariant::Red, |biome| {
                if REGISTRY
                    .biomes
                    .is_in_tag(biome, &BiomeTag::SPAWNS_SNOW_FOXES)
                {
                    FoxVariant::Snow
                } else {
                    FoxVariant::Red
                }
            });
        self.set_variant(variant);
        // Vanilla parity gap: `Fox.FoxGroupData` also carries the litter's coat
        // and turns members past the first into kits. Steel's `SpawnGroupData`
        // has no fox case, so each fox reads the biome itself -- identical
        // within one biome -- and none of them spawns as a kit.

        self.set_target_goals();
        self.populate_default_mouth_item();
        self.finalize_spawn_ageable_mob(world, spawn_reason, group_data)
    }

    /// Vanilla parity: `Animal.mobInteract`, which the fox does not override:
    /// berries feed it and breed it, and nothing else about it is interactive.
    ///
    /// Vanilla parity gap: `Fox.onOffspringSpawnedFromEgg` makes a kit hatched
    /// from a spawn egg trust the player who used it. Steel's `Mob.interact`
    /// has no spawn-egg branch yet, so there is nothing to hook.
    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        Animal::mob_interact_animal(self, player, hand)
    }
}

impl FoxEntity {
    /// Vanilla parity: `Fox.populateDefaultEquipmentSlots`, the one-in-five
    /// chance that a wild fox is already carrying something.
    fn populate_default_mouth_item(&self) {
        if rand::random::<f32>() >= 0.2 {
            return;
        }

        let odds = rand::random::<f32>();
        let item = if odds < 0.05 {
            &vanilla_items::EMERALD
        } else if odds < 0.2 {
            &vanilla_items::EGG
        } else if odds < 0.4 {
            if rand::random::<bool>() {
                &vanilla_items::RABBIT_FOOT
            } else {
                &vanilla_items::RABBIT_HIDE
            }
        } else if odds < 0.6 {
            &vanilla_items::WHEAT
        } else if odds < 0.8 {
            &vanilla_items::LEATHER
        } else {
            &vanilla_items::FEATHER
        };

        self.living_base
            .equipment()
            .lock()
            .set(EquipmentSlot::MainHand, ItemStack::new(item));
    }
}

impl PathfinderMob for FoxEntity {}

#[cfg(test)]
mod tests;
