//! Panda entity.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.panda.Panda`. A panda is
//! an `Animal` with a pair of genes, and what those two genes come out as is
//! the whole mob: a lazy one lies on its back, a worried one bolts from players
//! and sits out thunderstorms, a playful one rolls downhill, a weak one sneezes
//! as a cub and drops a slime ball doing it, an aggressive one bites back, and a
//! brown one is only brown. Everything below is one of those seven.

mod goals;

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_attributes, vanilla_entities,
    vanilla_game_events, vanilla_game_rules, vanilla_items,
};
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};

use crate::behavior::InteractionResult;
use crate::entity::damage::DamageSource;
use crate::entity::entities::ItemEntity;
use crate::entity::{
    AgeableMob, AgeableMobBase, AgeableMobGroupData, Animal, AnimalBase, Entity, EntityBase,
    EntityBaseLoad, EntityEventSource as _, EntitySpawnReason, EntitySyncedData, LivingEntity,
    LivingEntityBase, LivingEntitySyncedData, LivingTravelInput, Mob, MobBase, MoveResult,
    PathfinderMob, RemovalReason, SharedEntity, SpawnGroupData,
};
use crate::inventory::equipment::EquipmentSlot;
use crate::player::Player;
use crate::world::World;
use crate::world::game_event::GameEventContext;

use steel_registry::vanilla_entity_data::PandaEntityData;
use steel_utils::types::InteractionHand;

/// Vanilla parity: `Panda.FLAG_SNEEZE`.
const FLAG_SNEEZE: i8 = 2;
/// Vanilla parity: `Panda.FLAG_ROLL`.
const FLAG_ROLL: i8 = 4;
/// Vanilla parity: `Panda.FLAG_SIT`.
const FLAG_SIT: i8 = 8;
/// Vanilla parity: `Panda.FLAG_ON_BACK`.
const FLAG_ON_BACK: i8 = 16;
/// Vanilla parity: `Panda.EAT_TICK_INTERVAL`.
const EAT_TICK_INTERVAL: i32 = 5;
/// Vanilla parity: `Panda.TOTAL_ROLL_STEPS`.
pub const TOTAL_ROLL_STEPS: i32 = 32;
/// Vanilla parity: `Panda.TOTAL_UNHAPPY_TIME`.
pub const TOTAL_UNHAPPY_TIME: i32 = 32;

/// Vanilla parity: the two `getUnhappyCounter()` values that whine.
const UNHAPPY_SOUND_TICKS: [i32; 2] = [29, 14];
/// Vanilla parity: the `getSneezeCounter() > 20` of `Panda.tick`.
const SNEEZE_DURATION: i32 = 20;
/// Vanilla parity: the `nextInt(80) == 1` that starts a sitting panda eating.
const START_EATING_CHANCE: i32 = 80;
/// Vanilla parity: the `getEatCounter() > 80` before a panda may stop.
const MIN_EAT_TICKS: i32 = 80;
/// Vanilla parity: the `getEatCounter() > 100` before the item is swallowed.
const SWALLOW_EAT_TICKS: i32 = 100;
/// Vanilla parity: the `nextInt(20) == 1` roll that ends a meal.
const STOP_EATING_CHANCE: i32 = 20;
/// Vanilla parity: the `nextInt(32) == 0` mutation chance per gene.
const GENE_MUTATION_CHANCE: i32 = 32;
/// Vanilla parity: the `random.nextInt(16)` of `Gene.getRandom`.
const GENE_ROLL_BOUND: i32 = 16;
/// Vanilla parity: the `0.2F` baby chance of `Panda.finalizeSpawn`.
const BABY_SPAWN_CHANCE: f32 = 0.2;
/// Vanilla parity: the `10.0` max health of a weak panda.
const WEAK_MAX_HEALTH: f64 = 10.0;
/// Vanilla parity: the `0.07F` movement speed of a lazy panda.
const LAZY_MOVEMENT_SPEED: f64 = 0.07;
/// Vanilla parity: the roll shove, doubled for a grown panda.
const ROLL_PUSH_BABY: f64 = 0.1;
const ROLL_PUSH_ADULT: f64 = 0.2;
/// Vanilla parity: the `0.27` hop each roll bounce gets.
const ROLL_HOP: f64 = 0.27;
/// Vanilla parity: the three roll counters that bounce rather than run on.
const ROLL_BOUNCE_STEPS: [i32; 3] = [7, 15, 23];
/// Vanilla parity: the `0.15F` volume of `Panda.playStepSound`.
const STEP_SOUND_VOLUME: f32 = 0.15;
/// Vanilla parity: the `inflate(10.0)` a sneeze startles pandas within.
const SNEEZE_STARTLE_RANGE: f64 = 10.0;

/// What the `panda_sneeze` gift loot table drops.
///
/// Vanilla parity: the one weighted entry of
/// `minecraft:gameplay/panda_sneeze` -- one slime ball against 699 empties, so
/// a sneeze pays out about one time in seven hundred. Steel has no loot tables,
/// so the weights are written out.
const SNEEZE_SLIME_BALL_WEIGHT: i32 = 1;
const SNEEZE_EMPTY_WEIGHT: i32 = 699;

/// One of the seven things a panda can be.
///
/// Vanilla parity: `Panda.Gene`. A panda carries two, and the pair decides what
/// it acts like: a recessive gene only shows when both match.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum PandaGene {
    /// A panda with nothing special about it.
    #[default]
    Normal,
    /// Lies on its back, moves at half speed, and stays put far longer.
    Lazy,
    /// Bolts from players and monsters, and sits out thunderstorms.
    Worried,
    /// Rolls, at any age.
    Playful,
    /// Brown and white rather than black and white; recessive.
    Brown,
    /// Half the health, and sneezes as a cub twelve times as often; recessive.
    Weak,
    /// Bites back, and is the only panda an alert reaches.
    Aggressive,
}

impl PandaGene {
    /// Vanilla parity: `Panda.Gene.MAX_GENE`.
    const MAX_GENE: i8 = 6;

    /// Vanilla parity: `Panda.Gene.getId`.
    #[must_use]
    pub const fn id(self) -> i8 {
        match self {
            Self::Normal => 0,
            Self::Lazy => 1,
            Self::Worried => 2,
            Self::Playful => 3,
            Self::Brown => 4,
            Self::Weak => 5,
            Self::Aggressive => 6,
        }
    }

    /// Vanilla parity: `Panda.Gene.byId`, which clamps out-of-range to normal.
    #[must_use]
    pub const fn by_id(id: i8) -> Self {
        match id {
            1 => Self::Lazy,
            2 => Self::Worried,
            3 => Self::Playful,
            4 => Self::Brown,
            5 => Self::Weak,
            6 => Self::Aggressive,
            _ => Self::Normal,
        }
    }

    /// Vanilla parity: `Panda.Gene.getSerializedName`.
    #[must_use]
    pub const fn serialized_name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Lazy => "lazy",
            Self::Worried => "worried",
            Self::Playful => "playful",
            Self::Brown => "brown",
            Self::Weak => "weak",
            Self::Aggressive => "aggressive",
        }
    }

    /// Vanilla parity: the `StringRepresentable.fromEnum` decode.
    #[must_use]
    pub const fn from_serialized_name(name: &str) -> Option<Self> {
        match name.as_bytes() {
            b"normal" => Some(Self::Normal),
            b"lazy" => Some(Self::Lazy),
            b"worried" => Some(Self::Worried),
            b"playful" => Some(Self::Playful),
            b"brown" => Some(Self::Brown),
            b"weak" => Some(Self::Weak),
            b"aggressive" => Some(Self::Aggressive),
            _ => None,
        }
    }

    /// Vanilla parity: `Panda.Gene.isRecessive`, true for brown and weak.
    #[must_use]
    pub const fn is_recessive(self) -> bool {
        matches!(self, Self::Brown | Self::Weak)
    }

    /// Vanilla parity: `Panda.Gene.getVariantFromGenes`. A recessive gene shows
    /// only when both genes are it; otherwise the panda looks normal and the
    /// gene waits a generation.
    #[must_use]
    pub const fn variant_from_genes(main: Self, hidden: Self) -> Self {
        if !main.is_recessive() {
            return main;
        }
        if matches!(
            (main, hidden),
            (Self::Brown, Self::Brown) | (Self::Weak, Self::Weak)
        ) {
            main
        } else {
            Self::Normal
        }
    }

    /// Vanilla parity: `Panda.Gene.getRandom`, whose weights are the reason
    /// weak is the commonest gene and lazy, worried, playful and aggressive are
    /// each one in sixteen.
    #[must_use]
    pub fn random() -> Self {
        Self::from_roll(rand::random_range(0..GENE_ROLL_BOUND))
    }

    /// The body of [`Self::random`], split out so a test can drive every roll.
    #[must_use]
    pub const fn from_roll(roll: i32) -> Self {
        match roll {
            0 => Self::Lazy,
            1 => Self::Worried,
            2 => Self::Playful,
            4 => Self::Aggressive,
            3 | 5..=8 => Self::Weak,
            9 | 10 => Self::Brown,
            _ => Self::Normal,
        }
    }
}

/// A panda.
#[entity_behavior(class = "Panda")]
pub struct PandaEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    /// Vanilla parity: `Panda.gotBamboo`, set when a player feeds a panda that
    /// is angry at them -- the bribe that ends the fight.
    got_bamboo: SyncMutex<bool>,
    /// Vanilla parity: `Panda.didBite`, which is what makes a non-aggressive
    /// panda stop after one bite.
    did_bite: SyncMutex<bool>,
    /// Vanilla parity: `Panda.rollCounter`.
    roll_counter: SyncMutex<i32>,
    /// Vanilla parity: `Panda.rollDelta`.
    roll_delta: SyncMutex<DVec3>,
    /// The player an unhappy panda is complaining to.
    ///
    /// Vanilla keeps this on `PandaLookAtPlayerGoal` and has the breed goal
    /// reach into it. Steel's goals live behind the selector's mutex, so the
    /// panda owns it and the look goal reads it.
    unhappy_look_target: SyncMutex<Option<i32>>,
    entity_data: SyncMutex<PandaEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `PandaEntity`.
unsafe impl DowncastType for PandaEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/panda");
}

impl PandaEntity {
    /// Creates a panda at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a panda from saved base data.
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
        let mut entity_data = PandaEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        // Vanilla parity: the `setCanPickUpLoot(true)` of the `Panda`
        // constructor, which a cub does not get -- only a grown panda picks
        // bamboo off the ground.
        *mob_base.can_pick_up_loot().lock() = true;

        goals::register(&mob_base);

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            got_bamboo: SyncMutex::new(false),
            did_bite: SyncMutex::new(false),
            roll_counter: SyncMutex::new(0),
            roll_delta: SyncMutex::new(DVec3::ZERO),
            unhappy_look_target: SyncMutex::new(None),
            entity_data: SyncMutex::new(entity_data),
        }
    }

    fn flag(&self, flag: i8) -> bool {
        *self.entity_data.lock().id_flags.get() & flag != 0
    }

    fn set_flag(&self, flag: i8, value: bool) {
        let mut data = self.entity_data.lock();
        let current = *data.id_flags.get();
        data.id_flags.set(if value {
            current | flag
        } else {
            current & !flag
        });
    }

    /// Returns vanilla `Panda.getUnhappyCounter`.
    #[must_use]
    pub fn unhappy_counter(&self) -> i32 {
        *self.entity_data.lock().unhappy_counter.get()
    }

    /// Sets vanilla `Panda.setUnhappyCounter`.
    pub fn set_unhappy_counter(&self, value: i32) {
        self.entity_data.lock().unhappy_counter.set(value);
    }

    /// Returns vanilla `Panda.isSneezing`.
    #[must_use]
    pub fn is_sneezing(&self) -> bool {
        self.flag(FLAG_SNEEZE)
    }

    /// Sets vanilla `Panda.sneeze`.
    pub fn sneeze(&self, value: bool) {
        self.set_flag(FLAG_SNEEZE, value);
        if !value {
            self.set_sneeze_counter(0);
        }
    }

    /// Returns vanilla `Panda.getSneezeCounter`.
    #[must_use]
    pub fn sneeze_counter(&self) -> i32 {
        *self.entity_data.lock().sneeze_counter.get()
    }

    fn set_sneeze_counter(&self, value: i32) {
        self.entity_data.lock().sneeze_counter.set(value);
    }

    /// Returns vanilla `Panda.isSitting`.
    #[must_use]
    pub fn is_sitting(&self) -> bool {
        self.flag(FLAG_SIT)
    }

    /// Sets vanilla `Panda.sit`.
    pub fn sit(&self, value: bool) {
        self.set_flag(FLAG_SIT, value);
    }

    /// Returns vanilla `Panda.isOnBack`.
    #[must_use]
    pub fn is_on_back(&self) -> bool {
        self.flag(FLAG_ON_BACK)
    }

    /// Sets vanilla `Panda.setOnBack`.
    pub fn set_on_back(&self, value: bool) {
        self.set_flag(FLAG_ON_BACK, value);
    }

    /// Returns vanilla `Panda.isRolling`.
    #[must_use]
    pub fn is_rolling(&self) -> bool {
        self.flag(FLAG_ROLL)
    }

    /// Sets vanilla `Panda.roll`.
    pub fn roll(&self, value: bool) {
        self.set_flag(FLAG_ROLL, value);
    }

    /// Returns vanilla `Panda.isEating`.
    #[must_use]
    pub fn is_eating(&self) -> bool {
        self.eat_counter() > 0
    }

    /// Sets vanilla `Panda.eat`.
    pub fn eat(&self, value: bool) {
        self.entity_data.lock().eat_counter.set(i32::from(value));
    }

    fn eat_counter(&self) -> i32 {
        *self.entity_data.lock().eat_counter.get()
    }

    fn set_eat_counter(&self, value: i32) {
        self.entity_data.lock().eat_counter.set(value);
    }

    /// Returns vanilla `Panda.getMainGene`.
    #[must_use]
    pub fn main_gene(&self) -> PandaGene {
        PandaGene::by_id(*self.entity_data.lock().main_gene.get())
    }

    /// Sets vanilla `Panda.setMainGene`, which rerolls anything out of range.
    pub fn set_main_gene(&self, gene: PandaGene) {
        let gene = if gene.id() > PandaGene::MAX_GENE {
            PandaGene::random()
        } else {
            gene
        };
        self.entity_data.lock().main_gene.set(gene.id());
    }

    /// Returns vanilla `Panda.getHiddenGene`.
    #[must_use]
    pub fn hidden_gene(&self) -> PandaGene {
        PandaGene::by_id(*self.entity_data.lock().hidden_gene.get())
    }

    /// Sets vanilla `Panda.setHiddenGene`.
    pub fn set_hidden_gene(&self, gene: PandaGene) {
        let gene = if gene.id() > PandaGene::MAX_GENE {
            PandaGene::random()
        } else {
            gene
        };
        self.entity_data.lock().hidden_gene.set(gene.id());
    }

    /// Returns vanilla `Panda.getVariant`.
    #[must_use]
    pub fn variant(&self) -> PandaGene {
        PandaGene::variant_from_genes(self.main_gene(), self.hidden_gene())
    }

    /// Returns vanilla `Panda.isLazy`.
    #[must_use]
    pub fn is_lazy(&self) -> bool {
        self.variant() == PandaGene::Lazy
    }

    /// Returns vanilla `Panda.isWorried`.
    #[must_use]
    pub fn is_worried(&self) -> bool {
        self.variant() == PandaGene::Worried
    }

    /// Returns vanilla `Panda.isPlayful`.
    #[must_use]
    pub fn is_playful(&self) -> bool {
        self.variant() == PandaGene::Playful
    }

    /// Returns vanilla `Panda.isBrown`.
    #[must_use]
    pub fn is_brown(&self) -> bool {
        self.variant() == PandaGene::Brown
    }

    /// Returns vanilla `Panda.isWeak`.
    #[must_use]
    pub fn is_weak(&self) -> bool {
        self.variant() == PandaGene::Weak
    }

    /// Returns vanilla `Panda.isScared`: a worried panda in a thunderstorm.
    #[must_use]
    pub fn is_scared(&self) -> bool {
        self.is_worried() && self.level().is_some_and(|world| world.is_thundering())
    }

    /// Returns vanilla `Panda.canPerformAction`, the gate every panda goal but
    /// panic and breeding passes through.
    #[must_use]
    pub fn can_perform_action(&self) -> bool {
        !self.is_on_back()
            && !self.is_scared()
            && !self.is_eating()
            && !self.is_rolling()
            && !self.is_sitting()
    }

    /// Returns whether this panda took bamboo from whoever it was angry at.
    #[must_use]
    pub fn got_bamboo(&self) -> bool {
        *self.got_bamboo.lock()
    }

    /// Returns whether this panda has already bitten.
    #[must_use]
    pub fn did_bite(&self) -> bool {
        *self.did_bite.lock()
    }

    /// The player an unhappy panda is complaining to, when there is one.
    #[must_use]
    pub fn unhappy_look_target(&self) -> Option<SharedEntity> {
        let id = (*self.unhappy_look_target.lock())?;
        self.level()?.get_entity_by_id(id)
    }

    /// Sets that player.
    ///
    /// Vanilla parity: the `lookAtPlayerGoal.setTarget(player)` of
    /// `PandaBreedGoal.canUse`.
    pub fn set_unhappy_look_target(&self, target: Option<&SharedEntity>) {
        *self.unhappy_look_target.lock() = target.map(|entity| entity.id());
    }

    /// Vanilla parity: `Panda.tryToSit`.
    pub fn try_to_sit(&self) {
        if self.is_in_water() {
            return;
        }
        self.set_travel_input(LivingTravelInput::new(0.0, 0.0, 0.0));
        self.mob_base.navigation().lock().stop();
        self.sit(true);
    }

    /// Returns whether an item entity is bamboo a panda would pick up and eat.
    ///
    /// Vanilla parity: `Panda.canPickUpAndEat`.
    #[must_use]
    pub fn can_pick_up_and_eat(entity: &dyn Entity) -> bool {
        use steel_utils::Downcast as _;

        let Some(item) = entity.downcast_ref::<ItemEntity>() else {
            return false;
        };
        REGISTRY
            .items
            .is_in_tag(item.get_item().item(), &ItemTag::PANDA_EATS_FROM_GROUND)
            && entity.is_alive()
            && !item.has_pickup_delay()
    }

    /// Returns whether the stack is vanilla panda food.
    #[must_use]
    pub fn is_panda_food(item_stack: &ItemStack) -> bool {
        REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::PANDA_FOOD)
    }

    /// Vanilla parity: `Panda.setAttributes`, the only place a gene changes a
    /// number rather than a behaviour.
    pub fn set_attributes(&self) {
        if self.is_weak() {
            self.attributes()
                .lock()
                .set_base_value(vanilla_attributes::MAX_HEALTH, WEAK_MAX_HEALTH);
            self.set_health(self.get_max_health());
        }
        if self.is_lazy() {
            self.attributes()
                .lock()
                .set_base_value(vanilla_attributes::MOVEMENT_SPEED, LAZY_MOVEMENT_SPEED);
        }
    }

    /// Vanilla parity: `Panda.setGeneFromParents`, which is the whole reason a
    /// brown panda can come from two black ones.
    pub fn set_gene_from_parents(&self, first: &Self, second: Option<&Self>) {
        match second {
            None => {
                if rand::random::<bool>() {
                    self.set_main_gene(first.one_of_genes_randomly());
                    self.set_hidden_gene(PandaGene::random());
                } else {
                    self.set_main_gene(PandaGene::random());
                    self.set_hidden_gene(first.one_of_genes_randomly());
                }
            }
            Some(second) => {
                if rand::random::<bool>() {
                    self.set_main_gene(first.one_of_genes_randomly());
                    self.set_hidden_gene(second.one_of_genes_randomly());
                } else {
                    self.set_main_gene(second.one_of_genes_randomly());
                    self.set_hidden_gene(first.one_of_genes_randomly());
                }
            }
        }

        if rand::random_range(0..GENE_MUTATION_CHANCE) == 0 {
            self.set_main_gene(PandaGene::random());
        }
        if rand::random_range(0..GENE_MUTATION_CHANCE) == 0 {
            self.set_hidden_gene(PandaGene::random());
        }
    }

    fn one_of_genes_randomly(&self) -> PandaGene {
        if rand::random::<bool>() {
            self.main_gene()
        } else {
            self.hidden_gene()
        }
    }

    /// Vanilla parity: the unhappy half of `Panda.tick`.
    fn tick_unhappy(&self) {
        let counter = self.unhappy_counter();
        if counter <= 0 {
            return;
        }
        if let Some(target) = Mob::target(self) {
            Mob::look_at(self, target.as_ref(), 90.0, 90.0);
        }
        if UNHAPPY_SOUND_TICKS.contains(&counter) {
            self.play_sound(&sound_events::ENTITY_PANDA_CANT_BREED, 1.0, 1.0);
        }
        self.set_unhappy_counter(counter - 1);
    }

    /// Vanilla parity: the sneeze half of `Panda.tick`.
    fn tick_sneeze(&self) {
        if !self.is_sneezing() {
            return;
        }
        self.set_sneeze_counter(self.sneeze_counter() + 1);
        if self.sneeze_counter() > SNEEZE_DURATION {
            self.sneeze(false);
            self.after_sneeze();
        } else if self.sneeze_counter() == 1 {
            self.play_sound(&sound_events::ENTITY_PANDA_PRE_SNEEZE, 1.0, 1.0);
        }
    }

    /// Vanilla parity: `Panda.afterSneeze`.
    ///
    /// The sneeze startles every grown panda within ten blocks into a hop, and
    /// pays out a slime ball about one time in seven hundred.
    fn after_sneeze(&self) {
        self.play_sound(&sound_events::ENTITY_PANDA_SNEEZE, 1.0, 1.0);
        let Some(world) = self.level() else {
            return;
        };

        let startled = world.get_entities_in_aabb_matching(
            &self.bounding_box().inflate(SNEEZE_STARTLE_RANGE),
            |entity| entity.entity_type() == &vanilla_entities::PANDA,
        );
        for entity in startled {
            use steel_utils::Downcast as _;

            let Some(panda) = entity.downcast_ref::<Self>() else {
                continue;
            };
            if !AgeableMob::is_baby(panda)
                && panda.on_ground()
                && !panda.is_in_water()
                && panda.can_perform_action()
            {
                panda.jump_from_ground();
            }
        }

        if world.get_game_rule(&vanilla_game_rules::MOB_DROPS)
            && rand::random_range(0..(SNEEZE_SLIME_BALL_WEIGHT + SNEEZE_EMPTY_WEIGHT))
                < SNEEZE_SLIME_BALL_WEIGHT
        {
            self.spawn_at_location(ItemStack::new(&vanilla_items::SLIME_BALL), 0.0);
        }
    }

    /// Vanilla parity: `Panda.handleRoll`.
    ///
    /// The first tick launches the panda along its facing; three later ticks
    /// stop it dead and hop it again, which is the bounce down a hill.
    fn handle_roll(&self) {
        let counter = {
            let mut roll_counter = self.roll_counter.lock();
            *roll_counter += 1;
            *roll_counter
        };
        if counter > TOTAL_ROLL_STEPS {
            self.roll(false);
            return;
        }

        let movement = self.velocity();
        if counter == 1 {
            let angle = self.rotation().0.to_radians();
            let multiplier = if AgeableMob::is_baby(self) {
                ROLL_PUSH_BABY
            } else {
                ROLL_PUSH_ADULT
            };
            let delta = DVec3::new(
                movement.x - f64::from(angle.sin()) * multiplier,
                0.0,
                movement.z + f64::from(angle.cos()) * multiplier,
            );
            *self.roll_delta.lock() = delta;
            self.set_velocity(delta + DVec3::new(0.0, ROLL_HOP, 0.0));
        } else if ROLL_BOUNCE_STEPS.contains(&counter) {
            self.set_velocity(DVec3::new(
                0.0,
                if self.on_ground() {
                    ROLL_HOP
                } else {
                    movement.y
                },
                0.0,
            ));
        } else {
            let delta = *self.roll_delta.lock();
            self.set_velocity(DVec3::new(delta.x, movement.y, delta.z));
        }
        self.mark_velocity_sync();
    }

    /// Vanilla parity: `Panda.handleEating`, the server half.
    fn handle_eating(&self) {
        let held_empty = self.get_item_by_slot(EquipmentSlot::MainHand).is_empty();

        if !self.is_eating()
            && self.is_sitting()
            && !self.is_scared()
            && !held_empty
            && rand::random_range(0..START_EATING_CHANCE) == 1
        {
            self.eat(true);
        } else if held_empty || !self.is_sitting() {
            self.eat(false);
        }

        if !self.is_eating() {
            return;
        }

        self.add_eating_sound();
        if self.eat_counter() > MIN_EAT_TICKS && rand::random_range(0..STOP_EATING_CHANCE) == 1 {
            if self.eat_counter() > SWALLOW_EAT_TICKS
                && REGISTRY.items.is_in_tag(
                    self.get_item_by_slot(EquipmentSlot::MainHand).item(),
                    &ItemTag::PANDA_EATS_FROM_GROUND,
                )
            {
                self.living_base
                    .equipment()
                    .lock()
                    .set(EquipmentSlot::MainHand, ItemStack::empty());
                if let Some(world) = self.level() {
                    world.game_event(
                        &vanilla_game_events::EAT,
                        self.block_position(),
                        &GameEventContext::new(Some(self.as_entity_event_source()), None),
                    );
                }
                self.sit(false);
            }

            self.eat(false);
            return;
        }

        self.set_eat_counter(self.eat_counter() + 1);
    }

    /// Vanilla parity: the sound half of `Panda.addEatingParticles`; the
    /// particles themselves are client-local.
    fn add_eating_sound(&self) {
        if self.eat_counter() % EAT_TICK_INTERVAL != 0 {
            return;
        }
        self.play_sound(
            &sound_events::ENTITY_PANDA_EAT,
            0.5 + 0.5 * rand::random_range(0..2) as f32,
            (rand::random::<f32>() - rand::random::<f32>()) * 0.2 + 1.0,
        );
    }

    /// Vanilla parity: the bamboo branch of `Panda.mobInteract`.
    fn interact_with_food(
        &self,
        world: &Arc<World>,
        player: &Player,
        hand: InteractionHand,
        food: &ItemStack,
    ) -> InteractionResult {
        if Mob::target(self).is_some() {
            *self.got_bamboo.lock() = true;
        }

        if self.can_age_up() {
            Mob::use_player_item(self, player, hand);
            // Vanilla parity: the `(int)(-this.getAge() / 20 * 0.1F)` of
            // `mobInteract` -- integer division first, then a tenth, so a
            // newborn cub grows by a hundred and twenty seconds of feeding.
            self.age_up((-self.get_age() / 20) / 10, true);
            return InteractionResult::Success;
        }

        if AgeableMob::is_baby(self) {
            return InteractionResult::Pass;
        }

        if self.get_age() == 0 && self.can_fall_in_love() {
            Mob::use_player_item(self, player, hand);
            self.set_in_love(Some(player));
            return InteractionResult::Success;
        }

        if self.is_sitting() || self.is_in_water() {
            return InteractionResult::Pass;
        }

        self.try_to_sit();
        self.eat(true);
        let held_by_panda = self.get_item_by_slot(EquipmentSlot::MainHand);
        if !held_by_panda.is_empty() && !player.has_infinite_materials() {
            self.spawn_at_location(held_by_panda, 0.0);
        }
        let _ = world;
        self.living_base
            .equipment()
            .lock()
            .set(EquipmentSlot::MainHand, food.copy_with_count(1));
        Mob::use_player_item(self, player, hand);
        InteractionResult::Success
    }
}

impl Entity for PandaEntity {
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

    /// Vanilla parity: `Panda.tick`.
    fn tick(&self) {
        self.default_tick();

        if self.is_worried() {
            let thundering = self.level().is_some_and(|world| world.is_thundering());
            if thundering && !self.is_in_water() {
                self.sit(true);
                self.eat(false);
            } else if !self.is_eating() {
                self.sit(false);
            }
        }

        if Mob::target(self).is_none() {
            *self.got_bamboo.lock() = false;
            *self.did_bite.lock() = false;
        }

        self.tick_unhappy();
        self.tick_sneeze();

        if self.is_rolling() {
            self.handle_roll();
        } else {
            *self.roll_counter.lock() = 0;
        }

        if self.is_sitting() {
            self.set_rotation((self.rotation().0, 0.0));
        }

        self.handle_eating();
    }

    /// Vanilla parity: `Panda.playStepSound`.
    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {
        self.play_sound(&sound_events::ENTITY_PANDA_STEP, STEP_SOUND_VOLUME, 1.0);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
        nbt.insert("MainGene", self.main_gene().serialized_name());
        nbt.insert("HiddenGene", self.hidden_gene().serialized_name());
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
        let read_gene = |key: &str| {
            nbt.string(key)
                .and_then(|name| PandaGene::from_serialized_name(name.to_str().as_ref()))
                .unwrap_or(PandaGene::Normal)
        };
        self.set_main_gene(read_gene("MainGene"));
        self.set_hidden_gene(read_gene("HiddenGene"));
    }
}

impl LivingEntity for PandaEntity {
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
        Some(&sound_events::ENTITY_PANDA_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_PANDA_DEATH)
    }

    /// Vanilla parity: `Panda.hurtServer`, which stands a sitting panda up
    /// before anything else -- a panda cannot be attacked while it eats.
    fn hurt_server(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        self.sit(false);
        self.living_hurt_server(world, source, amount)
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn ai_step(&self) -> Option<MoveResult> {
        let result = self.default_ai_step();
        AgeableMob::tick_ageable_mob(self);
        Animal::tick_animal_love(self);
        result
    }
}

impl AgeableMob for PandaEntity {
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

impl Animal for PandaEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        Self::is_panda_food(item_stack)
    }

    /// Vanilla parity: `Panda.getBreedOffspring`.
    fn initialize_breed_offspring(&self, partner: &dyn Animal, offspring: &dyn Animal) {
        use steel_utils::Downcast as _;

        let Some(cub) = offspring.downcast_ref::<Self>() else {
            return;
        };
        cub.set_gene_from_parents(self, partner.downcast_ref::<Self>());
        cub.set_attributes();
    }
}

impl Mob for PandaEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }

    /// Vanilla parity: `Panda.isAggressive`, which reads the gene rather than
    /// the shared attack flag -- an aggressive panda is angry by birth.
    fn is_aggressive(&self) -> bool {
        self.variant() == PandaGene::Aggressive
    }

    /// Vanilla parity: `Panda.getAmbientSound`.
    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(if Mob::is_aggressive(self) {
            &sound_events::ENTITY_PANDA_AGGRESSIVE_AMBIENT
        } else if self.is_worried() {
            &sound_events::ENTITY_PANDA_WORRIED_AMBIENT
        } else {
            &sound_events::ENTITY_PANDA_AMBIENT
        })
    }

    /// Vanilla parity: `Panda.playAttackSound`.
    fn play_attack_sound(&self) {
        self.play_sound(&sound_events::ENTITY_PANDA_BITE, 1.0, 1.0);
    }

    /// Vanilla parity: `Panda.canBeLeashed`, a flat no.
    fn can_be_leashed(&self) -> bool {
        false
    }

    /// Vanilla parity: `Panda.doHurtTarget`, which records the bite so a
    /// non-aggressive panda stops after it.
    fn do_hurt_target(&self, world: &World, target: &SharedEntity) -> bool {
        if !Mob::is_aggressive(self) {
            *self.did_bite.lock() = true;
        }
        self.mob_do_hurt_target(world, target)
    }

    /// Vanilla parity: `Panda.PandaMoveControl.tick`, which holds a panda still
    /// while it is doing anything else.
    fn tick_move_control(&self) {
        if !self.can_perform_action() {
            return;
        }
        self.default_tick_move_control();
    }

    /// Vanilla parity: `Panda.pickUpItem`, which takes bamboo into the hand
    /// rather than into an inventory.
    fn pick_up_item(&self, _world: &Arc<World>, item_entity: &SharedEntity) {
        use steel_utils::Downcast as _;

        if !self.get_item_by_slot(EquipmentSlot::MainHand).is_empty() {
            return;
        }
        if !Self::can_pick_up_and_eat(item_entity.as_ref()) {
            return;
        }
        let Some(item) = item_entity.downcast_ref::<ItemEntity>() else {
            return;
        };

        self.living_base
            .equipment()
            .lock()
            .set(EquipmentSlot::MainHand, item.get_item());
        Mob::set_guaranteed_drop(self, EquipmentSlot::MainHand);
        item_entity.set_removed(RemovalReason::Discarded);
    }

    /// Vanilla parity: `Panda.mobInteract`.
    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        if self.is_scared() {
            return InteractionResult::Pass;
        }
        if self.is_on_back() {
            self.set_on_back(false);
            return InteractionResult::Success;
        }

        let held = {
            let inventory = player.inventory.lock();
            let held = inventory.get_item_in_hand(hand);
            held.copy_with_count(held.count())
        };

        if Self::is_panda_food(&held) {
            let Some(world) = self.level() else {
                return InteractionResult::Pass;
            };
            return self.interact_with_food(&world, player, hand, &held);
        }

        // Vanilla parity: the golden-dandelion fall-through, which is the only
        // other thing a panda answers to.
        if AgeableMob::is_baby(self) && held.is(&vanilla_items::GOLDEN_DANDELION) {
            return Animal::mob_interact_animal(self, player, hand);
        }
        InteractionResult::Pass
    }

    /// Vanilla parity: `Panda.finalizeSpawn`, which rolls both genes and then
    /// asks for a one-in-five cub.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        self.set_main_gene(PandaGene::random());
        self.set_hidden_gene(PandaGene::random());
        self.set_attributes();

        let group_data = group_data.unwrap_or(SpawnGroupData::AgeableMob(
            AgeableMobGroupData::with_baby_spawn_chance(BABY_SPAWN_CHANCE),
        ));
        self.finalize_spawn_ageable_mob(world, spawn_reason, Some(group_data))
    }
}

impl PathfinderMob for PandaEntity {}

#[cfg(test)]
mod tests;
