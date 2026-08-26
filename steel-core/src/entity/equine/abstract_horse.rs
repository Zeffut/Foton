//! Shared vanilla `AbstractHorse` state and hooks.
//!
//! Vanilla parity: `AbstractHorse`. Everything a horse-shaped mob does that is
//! not its coat lives here: the saddle and armor slots, the temper that turns
//! a bucking wild horse into a tame one, the rearing, the steering seam a rider
//! drives, and the NBT round trip that keeps all of it across a save.
//!
//! Vanilla makes this class an `OwnableEntity` rather than a `TamableAnimal`:
//! a horse has an owner and a tame flag, but no sit order and no owner-following,
//! so it deliberately does not sit on Steel's [`TamableAnimal`] layer.
//!
//! [`TamableAnimal`]: crate::entity::TamableAnimal

use std::fmt;
use std::mem;
use std::ptr;
use std::sync::Arc;

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtTag};
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::enchantment_effect::EnchantmentEffectComponent;
use steel_registry::item_stack::ItemStack;
use steel_registry::items::ItemRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::sound_types::SoundType;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, sound_types, vanilla_attributes,
    vanilla_blocks, vanilla_game_events, vanilla_items,
};
use steel_utils::entity_events::EntityStatus;
use steel_utils::locks::{IntoShared as _, Shared, SyncMutex};
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, BlockStateId, UuidExt as _};
use uuid::Uuid;

use crate::behavior::InteractionResult;
use crate::entity::ai::targeting::TargetingConditions;
use crate::entity::damage::DamageSource;
use crate::entity::{
    AgeableMob, AgeableMobGroupData, Animal, Entity, EntitySpawnReason, LivingEntity, Mob,
    SpawnGroupData,
};
use crate::inventory::container::{Container as _, SimpleContainer};
use crate::inventory::equipment::EquipmentSlot;
use crate::player::Player;
use crate::world::World;

/// Bit of the synced flag byte that marks a tamed horse.
///
/// Vanilla parity: `AbstractHorse.FLAG_TAME`.
const FLAG_TAME: i8 = 2;

/// Bit of the synced flag byte that marks a horse born from breeding.
///
/// Vanilla parity: `AbstractHorse.FLAG_BRED`.
const FLAG_BRED: i8 = 8;

/// Bit of the synced flag byte that marks a horse with its head in the grass.
///
/// Vanilla parity: `AbstractHorse.FLAG_EATING`.
const FLAG_EATING: i8 = 16;

/// Bit of the synced flag byte that marks a rearing horse.
///
/// Vanilla parity: `AbstractHorse.FLAG_STANDING`.
const FLAG_STANDING: i8 = 32;

/// Bit of the synced flag byte that opens the horse's mouth.
///
/// Vanilla parity: `AbstractHorse.FLAG_OPEN_MOUTH`.
const FLAG_OPEN_MOUTH: i8 = 64;

/// How much smaller a foal is than the adult.
///
/// Vanilla parity: `AbstractHorse.BABY_SCALE`.
pub(crate) const BABY_SCALE: f32 = 0.7;

/// Rows in a horse's inventory grid.
///
/// Vanilla parity: `AbstractHorse.INVENTORY_ROWS`.
const INVENTORY_ROWS: usize = 3;

/// Share of an attribute range that widens a foal's inherited roll.
///
/// Vanilla parity: `AbstractHorse.BREEDING_CROSS_FACTOR`.
const BREEDING_CROSS_FACTOR: f64 = 0.15;

/// How much a rider's sideways input is worth to a horse.
///
/// Vanilla parity: `AbstractHorse.SIDEWAYS_MOVE_SPEED_FACTOR`.
const SIDEWAYS_MOVE_SPEED_FACTOR: f32 = 0.5;

/// How much a rider's backwards input is worth to a horse.
///
/// Vanilla parity: `AbstractHorse.BACKWARDS_MOVE_SPEED_FACTOR`.
const BACKWARDS_MOVE_SPEED_FACTOR: f32 = 0.25;

/// Ticks a horse rears for when something makes it rear.
///
/// Vanilla parity: the `setStanding(20)` of `AbstractHorse.standIfPossible`.
const STANDING_TICKS: i32 = 20;

/// How far a foal looks for the mare that bore it.
///
/// Vanilla parity: the `range(16.0)` of `AbstractHorse.MOMMY_TARGETING`.
const MOMMY_SEARCH_RANGE: f64 = 16.0;

/// How far a foal is willing to drift from its mother before pathing back.
///
/// Vanilla parity: the `distanceToSqr(mommy) > 4.0` of `AbstractHorse.followMommy`.
const MOMMY_FOLLOW_DISTANCE_SQR: f64 = 4.0;

/// Ticks a horse keeps its head down before it stops grazing.
///
/// Vanilla parity: the `++this.eatingCounter > 50` of `AbstractHorse.aiStep`.
const EATING_TICKS: i32 = 50;

/// Ticks a horse keeps its mouth open after eating.
///
/// Vanilla parity: the `++this.mouthCounter > 30` of `AbstractHorse.tick`.
const MOUTH_OPEN_TICKS: i32 = 30;

/// Ticks a tail flick lasts.
///
/// Vanilla parity: the `++this.tailCounter > 8` of `AbstractHorse.tick`.
const TAIL_TICKS: i32 = 8;

/// Ticks a sprint burst is tracked for.
///
/// Vanilla parity: the `this.sprintCounter > 300` of `AbstractHorse.tick`.
const SPRINT_TICKS: i32 = 300;

/// Odds per tick of a horse flicking its tail.
///
/// Vanilla parity: the `random.nextInt(200)` of `AbstractHorse.aiStep`.
const TAIL_FLICK_CHANCE: i32 = 200;

/// Odds per tick of a horse healing a half heart.
///
/// Vanilla parity: the `random.nextInt(900)` of `AbstractHorse.aiStep`.
const IDLE_HEAL_CHANCE: i32 = 900;

/// Odds per tick of a grazing horse putting its head down.
///
/// Vanilla parity: the `random.nextInt(300)` of `AbstractHorse.aiStep`.
const START_EATING_CHANCE: i32 = 300;

/// Odds of a hurt horse rearing.
///
/// Vanilla parity: the `random.nextInt(3) == 0` of `AbstractHorse.hurtServer`.
const REAR_WHEN_HURT_CHANCE: i32 = 3;

/// Gallop steps before the gallop sound starts repeating.
///
/// Vanilla parity: the `gallopSoundCounter > 5` of `AbstractHorse.playStepSound`.
const GALLOP_SOUND_DELAY: i32 = 5;

/// How often the gallop sound repeats once it has started.
///
/// Vanilla parity: the `gallopSoundCounter % 3` of `AbstractHorse.playStepSound`.
const GALLOP_SOUND_INTERVAL: i32 = 3;

/// Lowest max health a bred horse can inherit.
///
/// Vanilla parity: `AbstractHorse.MIN_HEALTH`, which is `generateMaxHealth(i -> 0)`.
pub(crate) const MIN_HEALTH: f32 = 15.0;

/// Highest max health a bred horse can inherit.
///
/// Vanilla parity: `AbstractHorse.MAX_HEALTH`, which is `generateMaxHealth(i -> i - 1)`.
pub(crate) const MAX_HEALTH: f32 = 30.0;

/// Lowest movement speed a bred horse can inherit.
///
/// Vanilla parity: `AbstractHorse.MIN_MOVEMENT_SPEED`, which is `generateSpeed(() -> 0.0)`.
pub(crate) const MIN_MOVEMENT_SPEED: f32 = 0.1125;

/// Highest movement speed a bred horse can inherit.
///
/// Vanilla parity: `AbstractHorse.MAX_MOVEMENT_SPEED`, which is `generateSpeed(() -> 1.0)`.
pub(crate) const MAX_MOVEMENT_SPEED: f32 = 0.3375;

/// Lowest jump strength a bred horse can inherit.
///
/// Vanilla parity: `AbstractHorse.MIN_JUMP_STRENGTH`, which is `generateJumpStrength(() -> 0.0)`.
pub(crate) const MIN_JUMP_STRENGTH: f32 = 0.4;

/// Highest jump strength a bred horse can inherit.
///
/// Vanilla parity: `AbstractHorse.MAX_JUMP_STRENGTH`, which is `generateJumpStrength(() -> 1.0)`.
pub(crate) const MAX_JUMP_STRENGTH: f32 = 1.0;

/// Rolls a horse's max health.
///
/// Vanilla parity: `AbstractHorse.generateMaxHealth`. `bound_to_int` is vanilla's
/// `IntUnaryOperator`, which is `RandomSource::nextInt` at spawn and a constant
/// when the bounds of the breeding range are being derived.
pub(crate) fn generate_max_health(bound_to_int: &mut dyn FnMut(i32) -> i32) -> f32 {
    15.0 + bound_to_int(8) as f32 + bound_to_int(9) as f32
}

/// Rolls a horse's jump strength.
///
/// Vanilla parity: `AbstractHorse.generateJumpStrength`.
pub(crate) fn generate_jump_strength(probability: &mut dyn FnMut() -> f64) -> f64 {
    f64::from(0.4_f32) + probability() * 0.2 + probability() * 0.2 + probability() * 0.2
}

/// Rolls a horse's movement speed.
///
/// Vanilla parity: `AbstractHorse.generateSpeed`.
pub(crate) fn generate_speed(probability: &mut dyn FnMut() -> f64) -> f64 {
    (f64::from(0.45_f32) + probability() * 0.3 + probability() * 0.3 + probability() * 0.3) * 0.25
}

/// Crosses two parent attribute values into a foal's.
///
/// Vanilla parity: `AbstractHorse.createOffspringAttribute`. The average of the
/// parents, spread over their gap plus a margin, is why two good horses breed a
/// better one more often than not without ever guaranteeing it.
pub(crate) fn create_offspring_attribute(
    own_value: f64,
    partner_value: f64,
    range_min: f64,
    range_max: f64,
    quality_roll: &mut dyn FnMut() -> f64,
) -> f64 {
    debug_assert!(range_max > range_min, "incorrect range for an attribute");

    let own_value = own_value.clamp(range_min, range_max);
    let partner_value = partner_value.clamp(range_min, range_max);
    let margin = BREEDING_CROSS_FACTOR * (range_max - range_min);
    let range = (own_value - partner_value).abs() + margin * 2.0;
    let average = f64::midpoint(own_value, partner_value);
    let baby_quality = (quality_roll() + quality_roll() + quality_roll()) / 3.0 - 0.5;
    let new_value = range.mul_add(baby_quality, average);

    if new_value > range_max {
        return range_max - (new_value - range_max);
    }
    if new_value < range_min {
        return range_min + (range_min - new_value);
    }
    new_value
}

/// Vanilla `SoundType` values that make a horse clop on wood.
///
/// Vanilla parity: `AbstractHorse.isWoodSoundType`. Vanilla compares the sound
/// type by reference; `SoundType` is a plain value in Steel's generated registry,
/// so the five of them are compared field by field instead.
const WOOD_SOUND_TYPES: [SoundType; 5] = [
    sound_types::WOOD,
    sound_types::NETHER_WOOD,
    sound_types::STEM,
    sound_types::CHERRY_WOOD,
    sound_types::BAMBOO_WOOD,
];

fn is_same_sound_type(left: SoundType, right: SoundType) -> bool {
    ptr::eq(left.break_sound, right.break_sound)
        && ptr::eq(left.step_sound, right.step_sound)
        && ptr::eq(left.place_sound, right.place_sound)
        && ptr::eq(left.hit_sound, right.hit_sound)
        && ptr::eq(left.fall_sound, right.fall_sound)
}

fn is_wood_sound_type(sound_type: SoundType) -> bool {
    WOOD_SOUND_TYPES
        .into_iter()
        .any(|wood| is_same_sound_type(sound_type, wood))
}

/// Counters and rearing state vanilla keeps on `AbstractHorse` itself.
#[derive(Debug, Clone, Copy, PartialEq)]
struct AbstractHorseState {
    eating_counter: i32,
    mouth_counter: i32,
    stand_counter: i32,
    tail_counter: i32,
    sprint_counter: i32,
    gallop_sound_counter: i32,
    temper: i32,
    player_jump_pending_scale: f32,
    allow_stand_sliding: bool,
    can_gallop: bool,
    stand_anim: f32,
    stand_anim_o: f32,
}

impl AbstractHorseState {
    const fn new() -> Self {
        Self {
            eating_counter: 0,
            mouth_counter: 0,
            stand_counter: 0,
            tail_counter: 0,
            sprint_counter: 0,
            gallop_sound_counter: 0,
            temper: 0,
            player_jump_pending_scale: 0.0,
            allow_stand_sliding: false,
            can_gallop: true,
            stand_anim: 0.0,
            stand_anim_o: 0.0,
        }
    }
}

/// Runtime fields shared by every vanilla horse-shaped mob.
pub struct AbstractHorseBase {
    state: SyncMutex<AbstractHorseState>,
    /// Vanilla parity: `AbstractHorse.owner`, an `EntityReference<LivingEntity>`.
    /// Steel stores the UUID it resolves from, the way [`crate::entity::TamableAnimal`] does.
    owner: SyncMutex<Option<Uuid>>,
    /// Vanilla parity: `AbstractHorse.inventory`. Held behind its own handle so
    /// an open horse screen keeps working across an unrelated state change, and
    /// so `hasInventoryChanged` can compare identity after a resize.
    inventory: SyncMutex<Shared<SimpleContainer>>,
}

impl fmt::Debug for AbstractHorseBase {
    /// `SimpleContainer` is not `Debug`, so the inventory is summarized by size.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AbstractHorseBase")
            .field("state", &*self.state.lock())
            .field("owner", &*self.owner.lock())
            .field(
                "inventory_size",
                &self.inventory.lock().lock().items().len(),
            )
            .finish()
    }
}

impl AbstractHorseBase {
    /// Creates horse runtime state with an inventory sized for `inventory_columns`.
    #[must_use]
    pub fn new(inventory_columns: usize) -> Self {
        Self {
            state: SyncMutex::new(AbstractHorseState::new()),
            owner: SyncMutex::new(None),
            inventory: SyncMutex::new(
                SimpleContainer::new(inventory_columns * INVENTORY_ROWS).into_shared(),
            ),
        }
    }

    /// Returns the current inventory handle.
    #[must_use]
    pub fn inventory(&self) -> Shared<SimpleContainer> {
        Arc::clone(&self.inventory.lock())
    }

    /// Replaces the inventory with one of `size` slots, carrying items across.
    ///
    /// Vanilla parity: `AbstractHorse.createInventory`.
    pub fn create_inventory(&self, size: usize) {
        let mut slot = self.inventory.lock();
        let carried: Vec<ItemStack> = {
            let old = slot.lock();
            old.items()
                .iter()
                .take(size)
                .map(|item| item.copy_with_count(item.count()))
                .collect()
        };

        let mut items = vec![ItemStack::empty(); size];
        for (index, item) in carried.into_iter().enumerate() {
            items[index] = item;
        }
        *slot = SimpleContainer::from_items(items).into_shared();
    }

    /// Returns vanilla `AbstractHorse.temper`.
    #[must_use]
    pub fn temper(&self) -> i32 {
        self.state.lock().temper
    }

    /// Sets vanilla `AbstractHorse.temper`.
    pub fn set_temper(&self, temper: i32) {
        self.state.lock().temper = temper;
    }

    /// Returns vanilla `AbstractHorse.canGallop`.
    #[must_use]
    pub fn can_gallop(&self) -> bool {
        self.state.lock().can_gallop
    }

    /// Sets vanilla `AbstractHorse.canGallop`.
    pub fn set_can_gallop(&self, can_gallop: bool) {
        self.state.lock().can_gallop = can_gallop;
    }

    /// Returns the persisted owner UUID.
    #[must_use]
    pub fn owner_uuid(&self) -> Option<Uuid> {
        *self.owner.lock()
    }

    /// Sets the persisted owner UUID.
    pub fn set_owner_uuid(&self, owner: Option<Uuid>) {
        *self.owner.lock() = owner;
    }

    /// Returns vanilla `AbstractHorse.standAnimO`, the previous rearing frame.
    #[must_use]
    pub fn stand_anim_o(&self) -> f32 {
        self.state.lock().stand_anim_o
    }
}

/// Vanilla-shaped behavior shared by entities that extend `AbstractHorse`.
pub trait AbstractHorse: Animal {
    /// Returns shared horse runtime state.
    fn abstract_horse_base(&self) -> &AbstractHorseBase;

    /// Returns the synchronized `AbstractHorse.DATA_ID_FLAGS` byte.
    fn horse_flags(&self) -> i8;

    /// Sets the synchronized `AbstractHorse.DATA_ID_FLAGS` byte.
    fn set_horse_flags(&self, flags: i8);

    /// Returns vanilla `AbstractHorse.getFlag`.
    fn horse_flag(&self, flag: i8) -> bool {
        self.horse_flags() & flag != 0
    }

    /// Applies vanilla `AbstractHorse.setFlag`.
    fn set_horse_flag(&self, flag: i8, value: bool) {
        let current = self.horse_flags();
        self.set_horse_flags(if value {
            current | flag
        } else {
            current & !flag
        });
    }

    /// Returns vanilla `AbstractHorse.isTamed`.
    fn is_tamed(&self) -> bool {
        self.horse_flag(FLAG_TAME)
    }

    /// Applies vanilla `AbstractHorse.setTamed`.
    fn set_tamed(&self, tamed: bool) {
        self.set_horse_flag(FLAG_TAME, tamed);
    }

    /// Returns vanilla `AbstractHorse.isEating`.
    fn is_eating(&self) -> bool {
        self.horse_flag(FLAG_EATING)
    }

    /// Applies vanilla `AbstractHorse.setEating`.
    fn set_eating(&self, eating: bool) {
        self.set_horse_flag(FLAG_EATING, eating);
    }

    /// Returns vanilla `AbstractHorse.isStanding`.
    fn is_standing(&self) -> bool {
        self.horse_flag(FLAG_STANDING)
    }

    /// Returns vanilla `AbstractHorse.isBred`.
    fn is_bred(&self) -> bool {
        self.horse_flag(FLAG_BRED)
    }

    /// Applies vanilla `AbstractHorse.setBred`.
    fn set_bred(&self, bred: bool) {
        self.set_horse_flag(FLAG_BRED, bred);
    }

    /// Returns the owner UUID.
    ///
    /// Vanilla parity: `AbstractHorse.getOwnerReference`.
    fn horse_owner_uuid(&self) -> Option<Uuid> {
        self.abstract_horse_base().owner_uuid()
    }

    /// Applies vanilla `AbstractHorse.setOwner`.
    fn set_horse_owner(&self, owner: Option<Uuid>) {
        self.abstract_horse_base().set_owner_uuid(owner);
    }

    /// Returns vanilla `AbstractHorse.getTemper`.
    fn temper(&self) -> i32 {
        self.abstract_horse_base().temper()
    }

    /// Applies vanilla `AbstractHorse.setTemper`.
    fn set_temper(&self, temper: i32) {
        self.abstract_horse_base().set_temper(temper);
    }

    /// Returns vanilla `AbstractHorse.getMaxTemper`.
    fn max_temper(&self) -> i32 {
        100
    }

    /// Applies vanilla `AbstractHorse.modifyTemper`.
    fn modify_temper(&self, amount: i32) -> i32 {
        let temper = (self.temper() + amount).clamp(0, self.max_temper());
        self.set_temper(temper);
        temper
    }

    /// Returns vanilla `AbstractHorse.getInventoryColumns`.
    fn inventory_columns(&self) -> usize {
        0
    }

    /// Returns vanilla `AbstractHorse.getInventorySize`.
    fn inventory_size(&self) -> usize {
        self.inventory_columns() * INVENTORY_ROWS
    }

    /// Applies vanilla `AbstractHorse.createInventory`.
    fn create_horse_inventory(&self) {
        self.abstract_horse_base()
            .create_inventory(self.inventory_size());
    }

    /// Returns vanilla `AbstractHorse.canPerformRearing`.
    fn can_perform_rearing(&self) -> bool {
        true
    }

    /// Returns vanilla `AbstractHorse.getEatingSound`.
    fn eating_sound(&self) -> Option<SoundEventRef> {
        None
    }

    /// Returns vanilla `AbstractHorse.getAngrySound`.
    fn angry_sound(&self) -> Option<SoundEventRef> {
        None
    }

    /// Returns vanilla `AbstractHorse.getAmbientStandSound`.
    fn ambient_stand_sound(&self) -> Option<SoundEventRef> {
        Mob::ambient_sound(self)
    }

    /// Returns vanilla `AbstractHorse.getAmbientStandInterval`.
    fn ambient_stand_interval(&self) -> i32 {
        Mob::ambient_sound_interval(self)
    }

    /// Returns vanilla `AbstractHorse.canEatGrass`.
    fn can_eat_grass(&self) -> bool {
        true
    }

    /// Returns vanilla `AbstractHorse.isMobControlled`.
    fn is_mob_controlled(&self) -> bool {
        false
    }

    /// Applies vanilla `AbstractHorse.eating`, the mouth flap plus its sound.
    fn horse_eating_effect(&self) {
        self.open_mouth();
        if let Some(eating_sound) = self.eating_sound() {
            let pitch = (rand::random::<f32>() - rand::random::<f32>()).mul_add(0.2, 1.0);
            self.play_sound(eating_sound, 1.0, pitch);
        }
    }

    /// Applies vanilla `AbstractHorse.openMouth`.
    fn open_mouth(&self) {
        self.abstract_horse_base().state.lock().mouth_counter = 1;
        self.set_horse_flag(FLAG_OPEN_MOUTH, true);
    }

    /// Applies vanilla `AbstractHorse.setStanding`.
    fn set_standing(&self, ticks: i32) {
        self.set_eating(false);
        self.set_horse_flag(FLAG_STANDING, true);
        self.abstract_horse_base().state.lock().stand_counter = ticks;
    }

    /// Applies vanilla `AbstractHorse.clearStanding`.
    fn clear_standing(&self) {
        self.set_horse_flag(FLAG_STANDING, false);
        self.abstract_horse_base().state.lock().stand_counter = 0;
    }

    /// Applies vanilla `AbstractHorse.standIfPossible`.
    fn stand_if_possible(&self) {
        if self.can_perform_rearing() {
            self.set_standing(STANDING_TICKS);
        }
    }

    /// Applies vanilla `AbstractHorse.makeMad`.
    fn make_mad(&self) {
        if self.is_standing() {
            return;
        }
        self.stand_if_possible();
        self.make_sound(self.angry_sound());
    }

    /// Applies vanilla `AbstractHorse.tameWithName`.
    fn tame_with_name(&self, player: &Player) -> bool {
        self.set_horse_owner(Some(player.gameprofile.id));
        self.set_tamed(true);
        // TODO: Fire the `TAME_ANIMAL` criterion trigger once advancements exist.
        self.broadcast_entity_event(EntityStatus::TamingSucceeded);
        true
    }

    /// Applies vanilla `AbstractHorse.doPlayerRide`.
    fn do_player_ride(&self, player: &Player) {
        self.set_eating(false);
        self.clear_standing();
        player.set_rotation(self.rotation());
        let Some(world) = self.level() else {
            return;
        };
        let Some(vehicle) = world.get_entity_by_id(self.id()) else {
            return;
        };
        player.start_riding(&vehicle);
    }

    /// Returns vanilla `AbstractHorse.isFood`.
    fn is_horse_food(&self, item_stack: &ItemStack) -> bool {
        REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::HORSE_FOOD)
    }

    /// Applies vanilla `AbstractHorse.fedFood`.
    fn fed_food(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        let item_stack = {
            let inventory = player.inventory.lock();
            let item_stack = inventory.get_item_in_hand(hand);
            item_stack.copy_with_count(item_stack.count())
        };

        if !self.handle_eating(player, &item_stack) {
            return InteractionResult::Pass;
        }

        Mob::use_player_item(self, player, hand);
        InteractionResult::SuccessServer
    }

    /// Applies vanilla `AbstractHorse.handleEating`.
    fn handle_eating(&self, player: &Player, item_stack: &ItemStack) -> bool {
        let mut item_used = false;
        let (heal, age_up, temper) = if item_stack.is(&vanilla_items::WHEAT) {
            (2.0, 20, 3)
        } else if item_stack.is(&vanilla_items::SUGAR) {
            (1.0, 30, 3)
        } else if item_stack.is(&vanilla_items::HAY_BLOCK) {
            (20.0, 180, 0)
        } else if item_stack.is(&vanilla_items::APPLE) {
            (3.0, 60, 3)
        } else if item_stack.is(&vanilla_items::RED_MUSHROOM) {
            (3.0, 0, 3)
        } else if item_stack.is(&vanilla_items::CARROT) {
            (3.0, 60, 3)
        } else if item_stack.is(&vanilla_items::GOLDEN_CARROT) {
            if self.is_tamed() && self.get_age() == 0 && !self.is_in_love() {
                item_used = true;
                self.set_in_love(Some(player));
            }
            (4.0, 60, 5)
        } else if item_stack.is(&vanilla_items::GOLDEN_APPLE)
            || item_stack.is(&vanilla_items::ENCHANTED_GOLDEN_APPLE)
        {
            if self.is_tamed() && self.get_age() == 0 && !self.is_in_love() {
                item_used = true;
                self.set_in_love(Some(player));
            }
            (10.0, 240, 10)
        } else {
            (0.0, 0, 0)
        };

        let item_used = self.apply_eating_effects(item_used, heal, age_up, temper);
        if item_used {
            self.horse_eating_effect();
            self.game_event(&vanilla_game_events::EAT);
        }
        item_used
    }

    /// Applies the heal, age-up and temper part shared by every `handleEating`.
    ///
    /// Vanilla repeats these three blocks verbatim in `AbstractHorse.handleEating`
    /// and `Llama.handleEating`; only the table above them differs. `item_used`
    /// carries in whatever the table already decided, because the temper block
    /// reads it.
    fn apply_eating_effects(
        &self,
        mut item_used: bool,
        heal: f32,
        age_up: i32,
        temper: i32,
    ) -> bool {
        if self.get_health() < self.get_max_health() && heal > 0.0 {
            self.heal(heal);
            item_used = true;
        }

        if AgeableMob::is_baby(self) && age_up > 0 && !self.is_age_locked() {
            // VANILLA CLIENT-LOCAL: `handleEating` adds the happy-villager particles.
            self.age_up(age_up, false);
            item_used = true;
        }

        if temper > 0 && (item_used || !self.is_tamed()) && self.temper() < self.max_temper() {
            self.modify_temper(temper);
            item_used = true;
        }

        item_used
    }

    /// Returns vanilla `AbstractHorse.canUseSlot`.
    fn abstract_horse_can_use_slot(&self, slot: EquipmentSlot) -> bool {
        if slot != EquipmentSlot::Saddle {
            return true;
        }
        Entity::is_alive(self) && !AgeableMob::is_baby(self) && self.is_tamed()
    }

    /// Applies vanilla `AbstractHorse.equipBodyArmor`.
    fn equip_body_armor(&self, player: &Player, hand: InteractionHand) {
        let item_stack = {
            let inventory = player.inventory.lock();
            let item_stack = inventory.get_item_in_hand(hand);
            item_stack.copy_with_count(item_stack.count())
        };
        if !LivingEntity::is_equippable_in_slot(self, &item_stack, EquipmentSlot::Body) {
            return;
        }

        let equipped = {
            let mut inventory = player.inventory.lock();
            inventory.split_item_in_hand(hand, 1)
        };
        if equipped.is_empty() {
            return;
        }

        self.living_base()
            .equipment()
            .lock()
            .set(EquipmentSlot::Body, equipped);
        Mob::set_guaranteed_drop(self, EquipmentSlot::Body);
    }

    /// Returns vanilla `AbstractHorse.canDispenserEquipIntoSlot`.
    fn abstract_horse_can_dispenser_equip_into_slot(&self, slot: EquipmentSlot) -> bool {
        (matches!(slot, EquipmentSlot::Body | EquipmentSlot::Saddle) && self.is_tamed())
            || Mob::can_pick_up_loot(self)
    }

    /// Returns vanilla `AbstractHorse.isImmobile`.
    fn abstract_horse_is_immobile(&self) -> bool {
        LivingEntity::default_is_immobile(self) && self.is_vehicle() && Mob::is_saddled(self)
            || self.is_eating()
            || self.is_standing()
    }

    /// Applies vanilla `AbstractHorse.playStepSound`.
    fn abstract_horse_play_step_sound(&self, pos: BlockPos, block_state: BlockStateId) {
        if block_state.get_block().config.liquid {
            return;
        }
        let Some(world) = self.level() else {
            return;
        };

        let above_state = world.get_block_state(pos.above());
        let mut sound_type = block_state.get_block().config.sound_type;
        if above_state.get_block() == &vanilla_blocks::SNOW {
            sound_type = above_state.get_block().config.sound_type;
        }

        if self.is_vehicle() && self.abstract_horse_base().can_gallop() {
            let gallop_sound_counter = self.bump_gallop_sound_counter();
            if gallop_sound_counter > GALLOP_SOUND_DELAY {
                if gallop_sound_counter % GALLOP_SOUND_INTERVAL == 0 {
                    self.play_gallop_sound(sound_type);
                }
            } else {
                self.play_sound(
                    &sound_events::ENTITY_HORSE_STEP_WOOD,
                    sound_type.volume * 0.15,
                    sound_type.pitch,
                );
            }
            return;
        }

        if is_wood_sound_type(sound_type) {
            self.play_sound(
                &sound_events::ENTITY_HORSE_STEP_WOOD,
                sound_type.volume * 0.15,
                sound_type.pitch,
            );
            return;
        }

        let step_sound = if AgeableMob::is_baby(self) {
            &sound_events::ENTITY_BABY_HORSE_STEP
        } else {
            &sound_events::ENTITY_HORSE_STEP
        };
        self.play_sound(step_sound, sound_type.volume * 0.15, sound_type.pitch);
    }

    /// Advances and returns vanilla `AbstractHorse.gallopSoundCounter`.
    ///
    /// The skeleton horse counts its gallop through water in `getSwimSound`
    /// rather than in `playStepSound`, which is the only reason this is exposed.
    fn bump_gallop_sound_counter(&self) -> i32 {
        let mut state = self.abstract_horse_base().state.lock();
        state.gallop_sound_counter += 1;
        state.gallop_sound_counter
    }

    /// Applies vanilla `AbstractHorse.playGallopSound`.
    fn play_gallop_sound(&self, sound_type: SoundType) {
        self.play_sound(
            &sound_events::ENTITY_HORSE_GALLOP,
            sound_type.volume * 0.15,
            sound_type.pitch,
        );
    }

    /// Applies vanilla `AbstractHorse.playJumpSound`.
    fn play_jump_sound(&self) {
        self.play_sound(&sound_events::ENTITY_HORSE_JUMP, 0.4, 1.0);
    }

    /// Applies vanilla `AbstractHorse.causeFallDamage`.
    fn abstract_horse_cause_fall_damage(
        &self,
        fall_distance: f64,
        damage_modifier: f32,
        source: &DamageSource,
    ) -> bool {
        if fall_distance > 1.0 {
            let land_sound = if AgeableMob::is_baby(self) {
                &sound_events::ENTITY_BABY_HORSE_LAND
            } else {
                &sound_events::ENTITY_HORSE_LAND
            };
            self.play_sound(land_sound, 0.4, 1.0);
        }

        let damage = self.calculate_fall_damage(fall_distance, damage_modifier);
        if damage <= 0 {
            return false;
        }

        if let Some(world) = self.level() {
            self.hurt(&world, source, damage as f32);
        }
        self.propagate_fall_to_passengers(fall_distance, damage_modifier, source);
        self.play_block_fall_sound();
        true
    }

    /// Applies vanilla `AbstractHorse.hurtServer`'s rearing reaction.
    fn abstract_horse_react_to_hurt(&self, was_hurt: bool) -> bool {
        if was_hurt && rand::random_range(0..REAR_WHEN_HURT_CHANCE) == 0 {
            self.stand_if_possible();
        }
        was_hurt
    }

    /// Applies vanilla `AbstractHorse.tick`'s counters.
    ///
    /// The eat and mouth animations are client-side interpolation; the rearing
    /// one is not, because `getPassengerAttachmentPoint` shifts a rider by it.
    fn tick_abstract_horse(&self) {
        {
            let mut state = self.abstract_horse_base().state.lock();
            if state.mouth_counter > 0 {
                state.mouth_counter += 1;
            }
            if state.tail_counter > 0 {
                state.tail_counter += 1;
                if state.tail_counter > TAIL_TICKS {
                    state.tail_counter = 0;
                }
            }
            if state.sprint_counter > 0 {
                state.sprint_counter += 1;
                if state.sprint_counter > SPRINT_TICKS {
                    state.sprint_counter = 0;
                }
            }
        }

        let close_mouth = {
            let mut state = self.abstract_horse_base().state.lock();
            let close = state.mouth_counter > MOUTH_OPEN_TICKS;
            if close {
                state.mouth_counter = 0;
            }
            close
        };
        if close_mouth {
            self.set_horse_flag(FLAG_OPEN_MOUTH, false);
        }

        let stop_standing = {
            let mut state = self.abstract_horse_base().state.lock();
            state.stand_counter > 0 && {
                state.stand_counter -= 1;
                state.stand_counter <= 0
            }
        };
        if stop_standing {
            self.clear_standing();
        }

        let standing = self.is_standing();
        let mut state = self.abstract_horse_base().state.lock();
        state.stand_anim_o = state.stand_anim;
        if standing {
            state.stand_anim += (1.0 - state.stand_anim).mul_add(0.4, 0.05);
            state.stand_anim = state.stand_anim.min(1.0);
        } else {
            state.allow_stand_sliding = false;
            let anim = state.stand_anim;
            state.stand_anim += (0.8 * anim * anim).mul_add(anim, -anim).mul_add(0.6, -0.05);
            state.stand_anim = state.stand_anim.max(0.0);
        }
    }

    /// Applies vanilla `AbstractHorse.aiStep`.
    fn ai_step_abstract_horse(&self) {
        if rand::random_range(0..TAIL_FLICK_CHANCE) == 0 {
            self.abstract_horse_base().state.lock().tail_counter = 1;
        }
    }

    /// Applies the server-side half of vanilla `AbstractHorse.aiStep`.
    fn server_ai_step_abstract_horse(&self) {
        let Some(world) = self.level() else {
            return;
        };
        if !Entity::is_alive(self) {
            return;
        }

        if rand::random_range(0..IDLE_HEAL_CHANCE) == 0 && self.living_base().death_time() == 0 {
            self.heal(1.0);
        }

        if self.can_eat_grass() {
            if !self.is_eating()
                && !self.is_vehicle()
                && rand::random_range(0..START_EATING_CHANCE) == 0
                && world
                    .get_block_state(self.block_position().below())
                    .get_block()
                    == &vanilla_blocks::GRASS_BLOCK
            {
                self.set_eating(true);
            }

            if self.is_eating() {
                let stop = {
                    let mut state = self.abstract_horse_base().state.lock();
                    state.eating_counter += 1;
                    let stop = state.eating_counter > EATING_TICKS;
                    if stop {
                        state.eating_counter = 0;
                    }
                    stop
                };
                if stop {
                    self.set_eating(false);
                }
            }
        }

        self.follow_mommy(&world);
    }

    /// Applies vanilla `AbstractHorse.followMommy`.
    fn follow_mommy(&self, world: &Arc<World>) {
        self.follow_mommy_default(world);
    }

    /// The body of [`Self::follow_mommy`], callable from an override.
    ///
    /// Rust has no `super`, and the llama only wants to add a guard in front.
    fn follow_mommy_default(&self, world: &Arc<World>) {
        if !self.is_bred() || !AgeableMob::is_baby(self) || self.is_eating() {
            return;
        }

        let mommy_targeting = TargetingConditions::for_non_combat()
            .range(MOMMY_SEARCH_RANGE)
            .ignore_line_of_sight();
        let search_box = self.bounding_box().inflate(MOMMY_SEARCH_RANGE);
        let Some(pathfinder) = self.as_pathfinder_mob() else {
            return;
        };
        let mommy = world.nearest_entity_in_aabb_matching(&search_box, self.position(), |entity| {
            let Some(candidate) = entity.as_living_entity() else {
                return false;
            };
            entity
                .as_abstract_horse()
                .is_some_and(AbstractHorse::is_bred)
                && mommy_targeting.test(world.as_ref(), Some(pathfinder), candidate)
        });

        let Some(mommy) = mommy else {
            return;
        };
        if self.position().distance_squared(mommy.position()) > MOMMY_FOLLOW_DISTANCE_SQR {
            pathfinder.move_to_pos_with_reach(mommy.position(), 0, 1.0);
        }
    }

    /// Applies vanilla `AbstractHorse.mobInteract`.
    fn abstract_horse_mob_interact(
        &self,
        player: &Player,
        hand: InteractionHand,
    ) -> InteractionResult {
        if self.is_vehicle() || AgeableMob::is_baby(self) {
            return Animal::mob_interact_animal(self, player, hand);
        }

        if self.is_tamed() && player.is_secondary_use_active() {
            self.open_custom_inventory_screen(player);
            return InteractionResult::Success;
        }

        let item_stack = {
            let inventory = player.inventory.lock();
            let item_stack = inventory.get_item_in_hand(hand);
            item_stack.copy_with_count(item_stack.count())
        };
        if !item_stack.is_empty() {
            let interaction_result =
                LivingEntity::interact_living_entity_with_equippable(self, player, hand);
            if interaction_result.consumes_action() {
                return interaction_result;
            }

            if LivingEntity::is_equippable_in_slot(self, &item_stack, EquipmentSlot::Body)
                && !self.has_item_in_slot(EquipmentSlot::Body)
            {
                self.equip_body_armor(player, hand);
                return InteractionResult::Success;
            }
        }

        self.do_player_ride(player);
        InteractionResult::Success
    }

    /// Opens the horse's own inventory screen.
    ///
    /// Vanilla parity: `AbstractHorse.openCustomInventoryScreen`, which sends
    /// `ClientboundMountScreenOpenPacket` and installs a `HorseInventoryMenu`.
    ///
    /// MISSING FOUNDATION: Steel's menu slots are backed by a slice-shaped
    /// [`Container`](crate::inventory::container::Container), and the saddle and
    /// body-armor slots of that screen are entity equipment, which no container
    /// can expose. The chest inventory itself, its contents, the slot rules, the
    /// NBT round trip and the drops are all implemented; only the screen is not,
    /// so the interaction is accepted and nothing opens.
    fn open_horse_inventory_screen(&self, player: &Player) {
        let _ = player;
    }

    /// Returns whether the mob's own `mobInteract` should defer straight to
    /// [`Self::abstract_horse_mob_interact`].
    ///
    /// Vanilla parity: the guard `Horse`, `AbstractChestedHorse` and
    /// `ZombieHorse` each put in front of their feeding branch. `baby_bypass`
    /// is the item that lets a foal be interacted with anyway; the zombie horse
    /// has none.
    fn skips_feeding_interact(&self, player: &Player, baby_bypass: Option<ItemRef>) -> bool {
        let should_open_inventory =
            !AgeableMob::is_baby(self) && self.is_tamed() && player.is_secondary_use_active();
        let baby_bypass_held = baby_bypass.is_some_and(|item| {
            AgeableMob::is_baby(self)
                && player.is_holding(&mut |item_stack: &ItemStack| item_stack.is(item))
        });
        self.is_vehicle() || should_open_inventory || baby_bypass_held
    }

    /// Feeds the horse, or makes an untamed one buck.
    ///
    /// Vanilla parity: the shared body of the same three `mobInteract`
    /// overrides. `None` means the caller should carry on to its own next step.
    fn try_feed_or_anger(
        &self,
        player: &Player,
        hand: InteractionHand,
    ) -> Option<InteractionResult> {
        let item_stack = {
            let inventory = player.inventory.lock();
            let item_stack = inventory.get_item_in_hand(hand);
            item_stack.copy_with_count(item_stack.count())
        };
        if item_stack.is_empty() {
            return None;
        }

        if self.is_food(&item_stack) {
            return Some(self.fed_food(player, hand));
        }
        if !self.is_tamed() {
            self.make_mad();
            return Some(InteractionResult::Success);
        }
        None
    }

    /// Returns vanilla `AbstractHorse.getRiddenInput`.
    fn abstract_horse_ridden_input(&self, controller: &Player) -> DVec3 {
        let (pending_scale, allow_stand_sliding) = {
            let state = self.abstract_horse_base().state.lock();
            (state.player_jump_pending_scale, state.allow_stand_sliding)
        };
        if self.on_ground() && pending_scale == 0.0 && self.is_standing() && !allow_stand_sliding {
            return DVec3::ZERO;
        }

        let input = controller.travel_input();
        let sideways = input.sideways() * SIDEWAYS_MOVE_SPEED_FACTOR;
        let mut forward = input.forward();
        if forward <= 0.0 {
            forward *= BACKWARDS_MOVE_SPEED_FACTOR;
        }
        DVec3::new(f64::from(sideways), 0.0, f64::from(forward))
    }

    /// Returns vanilla `AbstractHorse.getRiddenSpeed`.
    fn abstract_horse_ridden_speed(&self) -> f32 {
        self.attributes()
            .lock()
            .required_value(vanilla_attributes::MOVEMENT_SPEED) as f32
    }

    /// Applies vanilla `AbstractHorse.tickRidden`.
    fn tick_ridden_abstract_horse(&self, controller: &Player, ridden_input: DVec3) {
        let (controller_yaw, controller_pitch) = controller.rotation();
        self.set_rotation((controller_yaw, controller_pitch * 0.5));
        self.base().set_old_yaw_to_current();
        let yaw = self.rotation().0;
        self.set_y_body_rot(yaw);
        self.set_y_head_rot(yaw);

        if !self.is_server_driven_movement() {
            return;
        }

        if ridden_input.z <= 0.0 {
            self.abstract_horse_base().state.lock().gallop_sound_counter = 0;
        }

        if !self.on_ground() {
            return;
        }

        let pending_scale = {
            let mut state = self.abstract_horse_base().state.lock();
            let pending_scale = state.player_jump_pending_scale;
            state.player_jump_pending_scale = 0.0;
            pending_scale
        };
        if pending_scale > 0.0 && !self.is_jumping() {
            self.execute_riders_jump(pending_scale, ridden_input);
        }
    }

    /// Applies vanilla `AbstractHorse.executeRidersJump`.
    fn execute_riders_jump(&self, amount: f32, input: DVec3) {
        let impulse = self.get_jump_power_with_multiplier(amount);
        let movement = self.velocity();
        self.set_velocity(DVec3::new(movement.x, f64::from(impulse), movement.z));
        self.mark_velocity_sync();

        if input.z > 0.0 {
            let yaw = self.rotation().0.to_radians();
            self.set_velocity(
                self.velocity()
                    + DVec3::new(
                        f64::from(-0.4 * yaw.sin() * amount),
                        0.0,
                        f64::from(0.4 * yaw.cos() * amount),
                    ),
            );
        }
    }

    /// Applies vanilla `AbstractHorse.onPlayerJump`.
    ///
    /// Vanilla only reaches this from `LocalPlayer`, so it never fires on a
    /// dedicated server; it is here because a locally simulated horse still
    /// needs the pending scale that [`Self::execute_riders_jump`] consumes.
    fn on_player_jump(&self, jump_amount: i32) {
        self.abstract_horse_on_player_jump(jump_amount);
    }

    /// The body of [`Self::on_player_jump`], callable from an override.
    ///
    /// Rust has no `super`, so a mob that only adds a condition -- the camel,
    /// which also needs to be off its dash cooldown and on the ground -- calls
    /// this for the rest.
    fn abstract_horse_on_player_jump(&self, jump_amount: i32) {
        if !Mob::is_saddled(self) {
            return;
        }

        let jump_amount = if jump_amount < 0 {
            0
        } else {
            self.abstract_horse_base().state.lock().allow_stand_sliding = true;
            self.stand_if_possible();
            jump_amount
        };
        self.abstract_horse_base()
            .state
            .lock()
            .player_jump_pending_scale = Entity::player_jump_pending_scale(self, jump_amount);
    }

    /// Applies vanilla `AbstractHorse.handleStartJump`.
    fn handle_start_jump_abstract_horse(&self) {
        self.abstract_horse_base().state.lock().allow_stand_sliding = true;
        self.stand_if_possible();
        self.play_jump_sound();
    }

    /// Returns vanilla `AbstractHorse.canParent`.
    fn can_parent(&self) -> bool {
        !self.is_vehicle()
            && !self.is_passenger()
            && self.is_tamed()
            && !AgeableMob::is_baby(self)
            && self.get_health() >= self.get_max_health()
            && self.is_in_love()
    }

    /// Applies vanilla `AbstractHorse.setOffspringAttributes`.
    fn set_offspring_attributes(&self, partner: &dyn Animal, baby: &dyn AbstractHorse) {
        for (attribute, range_min, range_max) in [
            (
                vanilla_attributes::MAX_HEALTH,
                f64::from(MIN_HEALTH),
                f64::from(MAX_HEALTH),
            ),
            (
                vanilla_attributes::JUMP_STRENGTH,
                f64::from(MIN_JUMP_STRENGTH),
                f64::from(MAX_JUMP_STRENGTH),
            ),
            (
                vanilla_attributes::MOVEMENT_SPEED,
                f64::from(MIN_MOVEMENT_SPEED),
                f64::from(MAX_MOVEMENT_SPEED),
            ),
        ] {
            let Some(own_value) = self.attributes().lock().get_base_value(attribute) else {
                continue;
            };
            let Some(partner_value) = partner.attributes().lock().get_base_value(attribute) else {
                continue;
            };
            let new_value = create_offspring_attribute(
                own_value,
                partner_value,
                range_min,
                range_max,
                &mut || rand::random::<f64>(),
            );
            baby.attributes()
                .lock()
                .set_base_value(attribute, new_value);
        }
    }

    /// Applies vanilla `AbstractHorse.randomizeAttributes`.
    fn randomize_attributes(&self) {}

    /// Applies vanilla `AbstractHorse.finalizeSpawn`.
    fn finalize_spawn_abstract_horse(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        let group_data = group_data.unwrap_or(SpawnGroupData::AgeableMob(
            AgeableMobGroupData::with_baby_spawn_chance(0.2),
        ));
        self.randomize_attributes();
        self.finalize_spawn_ageable_mob(world, spawn_reason, Some(group_data))
    }

    /// Drops the horse's inventory contents.
    ///
    /// Vanilla parity: the inventory half of `AbstractHorse.dropEquipment`. The
    /// saddle and body armor are equipment, which Steel already drops through
    /// `Mob.dropCustomDeathLoot`.
    fn drop_abstract_horse_inventory(&self) {
        let inventory = self.abstract_horse_base().inventory();
        let dropped: Vec<ItemStack> = {
            let mut container = inventory.lock();
            container
                .items_mut()
                .iter_mut()
                .filter(|item| {
                    !item.is_empty()
                        && !item.has_enchantment_effect(
                            EnchantmentEffectComponent::PreventEquipmentDrop,
                        )
                })
                .map(mem::take)
                .collect()
        };
        for item_stack in dropped {
            self.spawn_at_location(item_stack, 0.0);
        }
    }

    /// Returns the rider offset vanilla adds while the horse rears.
    ///
    /// Vanilla parity: `AbstractHorse.getPassengerAttachmentPoint`.
    fn abstract_horse_rearing_rider_offset(&self, scale: f32) -> DVec3 {
        let stand_anim_o = f64::from(self.abstract_horse_base().stand_anim_o());
        let scale = f64::from(scale);
        let offset = DVec3::new(
            0.0,
            0.15 * stand_anim_o * scale,
            -0.7 * stand_anim_o * scale,
        );
        let yaw = -f64::from(self.rotation().0).to_radians();
        let (sin, cos) = yaw.sin_cos();
        DVec3::new(
            offset.x.mul_add(cos, offset.z * sin),
            offset.y,
            offset.z.mul_add(cos, -(offset.x * sin)),
        )
    }

    /// Saves vanilla `AbstractHorse` fields.
    fn save_abstract_horse(&self, nbt: &mut NbtCompound) {
        nbt.insert("EatingHaystack", i8::from(self.is_eating()));
        nbt.insert("Bred", i8::from(self.is_bred()));
        nbt.insert("Temper", self.temper());
        nbt.insert("Tame", i8::from(self.is_tamed()));
        if let Some(owner) = self.horse_owner_uuid() {
            nbt.insert("Owner", NbtTag::IntArray(owner.to_int_array().to_vec()));
        }
    }

    /// Loads vanilla `AbstractHorse` fields.
    fn load_abstract_horse(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.set_eating(nbt.byte("EatingHaystack").is_some_and(|value| value != 0));
        self.set_bred(nbt.byte("Bred").is_some_and(|value| value != 0));
        self.set_temper(nbt.int("Temper").unwrap_or(0));
        self.set_tamed(nbt.byte("Tame").is_some_and(|value| value != 0));
        self.set_horse_owner(
            nbt.int_array("Owner")
                .and_then(|values| Uuid::from_int_array(&values)),
        );
    }
}

#[cfg(test)]
#[expect(
    clippy::float_cmp,
    reason = "these assert the exact vanilla constants the generators must produce"
)]
mod tests {
    use super::*;

    #[test]
    fn the_breeding_range_bounds_are_what_the_generators_produce() {
        // Vanilla derives MIN/MAX_* by feeding the generators their extreme
        // inputs; keeping the literals honest here is what stops a typo in one
        // of the three formulas from silently reshaping every bred foal.
        assert_eq!(generate_max_health(&mut |_| 0), MIN_HEALTH);
        assert_eq!(generate_max_health(&mut |bound| bound - 1), MAX_HEALTH);
        assert_eq!(generate_speed(&mut || 0.0) as f32, MIN_MOVEMENT_SPEED);
        assert_eq!(generate_speed(&mut || 1.0) as f32, MAX_MOVEMENT_SPEED);
        assert_eq!(
            generate_jump_strength(&mut || 0.0) as f32,
            MIN_JUMP_STRENGTH
        );
        assert_eq!(
            generate_jump_strength(&mut || 1.0) as f32,
            MAX_JUMP_STRENGTH
        );
    }

    #[test]
    fn a_bred_attribute_stays_inside_the_range_when_the_roll_overshoots() {
        // The reflection at both ends is the whole reason two maxed parents do
        // not breed an out-of-range foal.
        let high = create_offspring_attribute(
            f64::from(MAX_HEALTH),
            f64::from(MAX_HEALTH),
            f64::from(MIN_HEALTH),
            f64::from(MAX_HEALTH),
            &mut || 1.0,
        );
        assert!(high <= f64::from(MAX_HEALTH), "{high} exceeded the range");

        let low = create_offspring_attribute(
            f64::from(MIN_HEALTH),
            f64::from(MIN_HEALTH),
            f64::from(MIN_HEALTH),
            f64::from(MAX_HEALTH),
            &mut || 0.0,
        );
        assert!(low >= f64::from(MIN_HEALTH), "{low} undershot the range");
    }

    #[test]
    fn an_average_roll_lands_on_the_midpoint_of_the_parents() {
        let value = create_offspring_attribute(20.0, 24.0, 15.0, 30.0, &mut || 0.5);
        assert!((value - 22.0).abs() < 1.0e-9, "unexpected midpoint {value}");
    }

    #[test]
    fn only_the_five_wood_sound_types_make_a_horse_clop_on_wood() {
        assert!(is_wood_sound_type(sound_types::WOOD));
        assert!(is_wood_sound_type(sound_types::CHERRY_WOOD));
        assert!(!is_wood_sound_type(sound_types::STONE));
        assert!(!is_wood_sound_type(sound_types::GRASS));
    }

    #[test]
    fn resizing_the_inventory_keeps_the_slots_that_still_fit() {
        steel_registry::init_vanilla_registry();
        let base = AbstractHorseBase::new(5);
        base.inventory()
            .lock()
            .set_item(0, ItemStack::new(&vanilla_items::WHEAT));
        base.inventory()
            .lock()
            .set_item(14, ItemStack::new(&vanilla_items::APPLE));

        base.create_inventory(0);
        assert_eq!(base.inventory().lock().get_container_size(), 0);

        base.create_inventory(15);
        assert!(base.inventory().lock().get_item(0).is_empty());
    }
}
