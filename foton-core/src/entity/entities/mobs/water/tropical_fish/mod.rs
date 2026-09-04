//! Tropical fish entity.
//!
//! Vanilla parity: `TropicalFish`. What is worth care here is the variant: one
//! `int` holds the pattern, the base color and the pattern color at once, and
//! the pattern's own id is itself two fields packed together. Nine spawns in
//! ten take one of twenty-two named combinations; the tenth is rolled freely
//! out of the full two thousand seven hundred and change.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::dye_color::DyeColor;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_biome_tags::BiomeTag;
use foton_registry::vanilla_entity_data::TropicalFishEntityData;
use foton_registry::{sound_events, vanilla_blocks};
use foton_utils::locks::SyncMutex;
use foton_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::entity::LivingEntitySyncedData;
use crate::entity::ai::goal::{AvoidEntityGoal, PanicGoal, RandomSwimmingGoal};
use crate::entity::ai::path::PathType;
use crate::entity::damage::DamageSource;
use crate::entity::mob::NavigationKind;
use crate::entity::spawn::TropicalFishGroupData;
use crate::entity::spawn_rules::check_surface_water_animal_spawn_rules;
use crate::entity::{
    AgeableMobGroupData, Entity, EntityBase, EntityBaseLoad, EntitySpawnReason, EntitySyncedData,
    LivingEntity, LivingEntityBase, Mob, MobBase, PathfinderMob, SpawnGroupData,
};
use crate::fluid::FluidStateExt as _;
use crate::physics::MoveResult;
use crate::world::World;

use super::fish;

/// How big a tropical fish's body is.
///
/// Vanilla parity: `TropicalFish.Base`, the low byte of a pattern's packed id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TropicalFishBase {
    /// Vanilla `SMALL`.
    Small,
    /// Vanilla `LARGE`.
    Large,
}

impl TropicalFishBase {
    const fn id(self) -> i32 {
        match self {
            Self::Small => 0,
            Self::Large => 1,
        }
    }
}

/// One of the twelve tropical fish shapes.
///
/// Vanilla parity: `TropicalFish.Pattern`. The packed id is
/// `base.id | index << 8`, so the six small shapes are `0x0000..=0x0500` and
/// the six large ones are `0x0001..=0x0501`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TropicalFishPattern {
    /// Vanilla `KOB`.
    Kob,
    /// Vanilla `SUNSTREAK`.
    Sunstreak,
    /// Vanilla `SNOOPER`.
    Snooper,
    /// Vanilla `DASHER`.
    Dasher,
    /// Vanilla `BRINELY`.
    Brinely,
    /// Vanilla `SPOTTY`.
    Spotty,
    /// Vanilla `FLOPPER`.
    Flopper,
    /// Vanilla `STRIPEY`.
    Stripey,
    /// Vanilla `GLITTER`.
    Glitter,
    /// Vanilla `BLOCKFISH`.
    Blockfish,
    /// Vanilla `BETTY`.
    Betty,
    /// Vanilla `CLAYFISH`.
    Clayfish,
}

impl TropicalFishPattern {
    /// Vanilla parity: `TropicalFish.Pattern.values()`, in declaration order.
    pub const VALUES: [Self; 12] = [
        Self::Kob,
        Self::Sunstreak,
        Self::Snooper,
        Self::Dasher,
        Self::Brinely,
        Self::Spotty,
        Self::Flopper,
        Self::Stripey,
        Self::Glitter,
        Self::Blockfish,
        Self::Betty,
        Self::Clayfish,
    ];

    /// Returns the body size this pattern is drawn on.
    #[must_use]
    pub const fn base(self) -> TropicalFishBase {
        match self {
            Self::Kob
            | Self::Sunstreak
            | Self::Snooper
            | Self::Dasher
            | Self::Brinely
            | Self::Spotty => TropicalFishBase::Small,
            Self::Flopper
            | Self::Stripey
            | Self::Glitter
            | Self::Blockfish
            | Self::Betty
            | Self::Clayfish => TropicalFishBase::Large,
        }
    }

    /// Returns this pattern's index within its body size.
    const fn index(self) -> i32 {
        match self {
            Self::Kob | Self::Flopper => 0,
            Self::Sunstreak | Self::Stripey => 1,
            Self::Snooper | Self::Glitter => 2,
            Self::Dasher | Self::Blockfish => 3,
            Self::Brinely | Self::Betty => 4,
            Self::Spotty | Self::Clayfish => 5,
        }
    }

    /// Vanilla parity: `TropicalFish.Pattern.getPackedId`.
    #[must_use]
    pub const fn packed_id(self) -> i32 {
        self.base().id() | (self.index() << 8)
    }

    /// Vanilla parity: `TropicalFish.Pattern.byId`, which falls back to `KOB`.
    #[must_use]
    pub fn by_id(packed_id: i32) -> Self {
        Self::VALUES
            .into_iter()
            .find(|pattern| pattern.packed_id() == packed_id)
            .unwrap_or(Self::Kob)
    }

    /// Returns the name this pattern is saved and translated under.
    #[must_use]
    pub const fn serialized_name(self) -> &'static str {
        match self {
            Self::Kob => "kob",
            Self::Sunstreak => "sunstreak",
            Self::Snooper => "snooper",
            Self::Dasher => "dasher",
            Self::Brinely => "brinely",
            Self::Spotty => "spotty",
            Self::Flopper => "flopper",
            Self::Stripey => "stripey",
            Self::Glitter => "glitter",
            Self::Blockfish => "blockfish",
            Self::Betty => "betty",
            Self::Clayfish => "clayfish",
        }
    }
}

/// A whole tropical fish appearance.
///
/// Vanilla parity: the `TropicalFish.Variant` record, which is stored and sent
/// as the single packed `int` [`TropicalFishVariant::packed_id`] returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TropicalFishVariant {
    pattern: TropicalFishPattern,
    base_color: DyeColor,
    pattern_color: DyeColor,
}

impl TropicalFishVariant {
    /// Vanilla `TropicalFish.DEFAULT_VARIANT`.
    pub const DEFAULT: Self = Self::new(TropicalFishPattern::Kob, DyeColor::White, DyeColor::White);

    /// Creates a variant from its three parts.
    #[must_use]
    pub const fn new(
        pattern: TropicalFishPattern,
        base_color: DyeColor,
        pattern_color: DyeColor,
    ) -> Self {
        Self {
            pattern,
            base_color,
            pattern_color,
        }
    }

    /// Vanilla parity: `TropicalFish.packVariant`.
    ///
    /// The pattern keeps the low sixteen bits, the base color the third byte
    /// and the pattern color the fourth.
    #[must_use]
    pub const fn packed_id(self) -> i32 {
        (self.pattern.packed_id() & 0xFFFF)
            | ((self.base_color.id() & 0xFF) << 16)
            | ((self.pattern_color.id() & 0xFF) << 24)
    }

    /// Vanilla parity: the `Variant(int)` constructor.
    #[must_use]
    pub fn from_packed_id(packed_id: i32) -> Self {
        Self {
            pattern: TropicalFishPattern::by_id(packed_id & 0xFFFF),
            base_color: DyeColor::by_id((packed_id >> 16) & 0xFF),
            pattern_color: DyeColor::by_id((packed_id >> 24) & 0xFF),
        }
    }

    /// Returns the shape.
    #[must_use]
    pub const fn pattern(self) -> TropicalFishPattern {
        self.pattern
    }

    /// Returns the body color.
    #[must_use]
    pub const fn base_color(self) -> DyeColor {
        self.base_color
    }

    /// Returns the color the pattern is drawn in.
    #[must_use]
    pub const fn pattern_color(self) -> DyeColor {
        self.pattern_color
    }
}

/// Vanilla `TropicalFish.COMMON_VARIANTS`, in order; the index into this list
/// is what names a fish in the client.
pub const COMMON_VARIANTS: [TropicalFishVariant; 22] = [
    TropicalFishVariant::new(
        TropicalFishPattern::Stripey,
        DyeColor::Orange,
        DyeColor::Gray,
    ),
    TropicalFishVariant::new(TropicalFishPattern::Flopper, DyeColor::Gray, DyeColor::Gray),
    TropicalFishVariant::new(TropicalFishPattern::Flopper, DyeColor::Gray, DyeColor::Blue),
    TropicalFishVariant::new(
        TropicalFishPattern::Clayfish,
        DyeColor::White,
        DyeColor::Gray,
    ),
    TropicalFishVariant::new(
        TropicalFishPattern::Sunstreak,
        DyeColor::Blue,
        DyeColor::Gray,
    ),
    TropicalFishVariant::new(TropicalFishPattern::Kob, DyeColor::Orange, DyeColor::White),
    TropicalFishVariant::new(
        TropicalFishPattern::Spotty,
        DyeColor::Pink,
        DyeColor::LightBlue,
    ),
    TropicalFishVariant::new(
        TropicalFishPattern::Blockfish,
        DyeColor::Purple,
        DyeColor::Yellow,
    ),
    TropicalFishVariant::new(
        TropicalFishPattern::Clayfish,
        DyeColor::White,
        DyeColor::Red,
    ),
    TropicalFishVariant::new(
        TropicalFishPattern::Spotty,
        DyeColor::White,
        DyeColor::Yellow,
    ),
    TropicalFishVariant::new(
        TropicalFishPattern::Glitter,
        DyeColor::White,
        DyeColor::Gray,
    ),
    TropicalFishVariant::new(
        TropicalFishPattern::Clayfish,
        DyeColor::White,
        DyeColor::Orange,
    ),
    TropicalFishVariant::new(TropicalFishPattern::Dasher, DyeColor::Cyan, DyeColor::Pink),
    TropicalFishVariant::new(
        TropicalFishPattern::Brinely,
        DyeColor::Lime,
        DyeColor::LightBlue,
    ),
    TropicalFishVariant::new(TropicalFishPattern::Betty, DyeColor::Red, DyeColor::White),
    TropicalFishVariant::new(TropicalFishPattern::Snooper, DyeColor::Gray, DyeColor::Red),
    TropicalFishVariant::new(
        TropicalFishPattern::Blockfish,
        DyeColor::Red,
        DyeColor::White,
    ),
    TropicalFishVariant::new(
        TropicalFishPattern::Flopper,
        DyeColor::White,
        DyeColor::Yellow,
    ),
    TropicalFishVariant::new(TropicalFishPattern::Kob, DyeColor::Red, DyeColor::White),
    TropicalFishVariant::new(
        TropicalFishPattern::Sunstreak,
        DyeColor::Gray,
        DyeColor::White,
    ),
    TropicalFishVariant::new(
        TropicalFishPattern::Dasher,
        DyeColor::Cyan,
        DyeColor::Yellow,
    ),
    TropicalFishVariant::new(
        TropicalFishPattern::Flopper,
        DyeColor::Yellow,
        DyeColor::Yellow,
    ),
];

/// Odds a spawning fish takes one of the named combinations.
///
/// Vanilla parity: the `random.nextFloat() < 0.9` of
/// `TropicalFish.finalizeSpawn`.
const COMMON_VARIANT_CHANCE: f32 = 0.9;

/// A tropical fish.
#[entity_behavior(class = "TropicalFish")]
pub struct TropicalFishEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<TropicalFishEntityData>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `TropicalFishEntity`.
unsafe impl DowncastType for TropicalFishEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/tropical_fish");
}

impl TropicalFishEntity {
    /// Creates a tropical fish at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a tropical fish from saved base data.
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
        // Vanilla parity: `WaterAnimal` clears the water malus.
        mob_base
            .pathfinding_malus()
            .lock()
            .set(PathType::Water, 0.0);
        let mut entity_data = TropicalFishEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Vanilla parity: `AbstractFish.registerGoals`.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(0, PanicGoal::new(fish::PANIC_SPEED_MODIFIER));
            goals.add_goal(
                2,
                AvoidEntityGoal::with_selector(
                    fish::AVOID_PLAYER_RANGE,
                    fish::AVOID_WALK_SPEED,
                    fish::AVOID_SPRINT_SPEED,
                    |_, target, _| fish::is_player_to_flee(target),
                ),
            );
            goals.add_goal(
                4,
                RandomSwimmingGoal::new(fish::SWIM_SPEED_MODIFIER, fish::SWIM_INTERVAL_TICKS),
            );
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
        }
    }

    /// Returns the packed variant this fish is synchronized with.
    ///
    /// Vanilla parity: the private `getPackedVariant`.
    #[must_use]
    pub fn packed_variant(&self) -> i32 {
        *self.entity_data.lock().id_type_variant.get()
    }

    /// Sets the packed variant.
    ///
    /// Vanilla parity: the private `setPackedVariant`.
    pub fn set_packed_variant(&self, packed_variant: i32) {
        self.entity_data.lock().id_type_variant.set(packed_variant);
    }

    /// Returns the unpacked variant.
    #[must_use]
    pub fn variant(&self) -> TropicalFishVariant {
        TropicalFishVariant::from_packed_id(self.packed_variant())
    }

    /// Sets the variant.
    pub fn set_variant(&self, variant: TropicalFishVariant) {
        self.set_packed_variant(variant.packed_id());
    }

    /// Returns the vanilla pattern component of this fish's variant.
    #[must_use]
    pub fn pattern(&self) -> TropicalFishPattern {
        self.variant().pattern()
    }

    /// Replaces the pattern while preserving both vanilla dye colors.
    pub fn set_pattern(&self, pattern: TropicalFishPattern) {
        let variant = self.variant();
        self.set_variant(TropicalFishVariant::new(
            pattern,
            variant.base_color(),
            variant.pattern_color(),
        ));
    }

    /// Returns whether this fish came out of a bucket.
    ///
    /// Vanilla parity: `AbstractFish.fromBucket`.
    #[must_use]
    #[expect(
        clippy::wrong_self_convention,
        reason = "the name mirrors the vanilla accessor"
    )]
    pub fn from_bucket(&self) -> bool {
        *self.entity_data.lock().abstract_fish.from_bucket.get()
    }

    /// Marks this fish as having come out of a bucket.
    pub fn set_from_bucket(&self, from_bucket: bool) {
        self.entity_data
            .lock()
            .abstract_fish_mut()
            .from_bucket
            .set(from_bucket);
    }

    /// Vanilla parity: `TropicalFish.checkTropicalFishSpawnRules`. Warm oceans
    /// let a fish spawn at any depth; anywhere else it has to be near the top.
    #[must_use]
    pub fn check_tropical_fish_spawn_rules(world: &Arc<World>, pos: BlockPos) -> bool {
        if !world
            .get_block_state(pos.below())
            .get_fluid_state()
            .is_water()
            || world.get_block_state(pos.above()).get_block() != &vanilla_blocks::WATER
        {
            return false;
        }

        world.biome_at(pos).is_some_and(|biome| {
            biome.has_tag(&BiomeTag::ALLOWS_TROPICAL_FISH_SPAWNS_AT_ANY_HEIGHT)
        }) || check_surface_water_animal_spawn_rules(world, pos)
    }
}

impl Entity for TropicalFishEntity {
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
        let air_before_tick = self.air_supply();
        Mob::base_tick_mob(self);
        if let Some(world) = self.level() {
            fish::handle_air_supply(self, &world, air_before_tick);
        }
    }

    /// Vanilla parity: `WaterAnimal.isPushedByFluid`; a fish holds its line in
    /// a current rather than being swept along by it.
    fn is_pushed_by_fluid(&self) -> bool {
        false
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Neutral
    }

    /// Vanilla parity: `AbstractFish.playStepSound` is empty.
    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {}

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        nbt.insert("FromBucket", self.from_bucket());
        nbt.insert("Variant", self.packed_variant());
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.set_from_bucket(nbt.byte("FromBucket").is_some_and(|flag| flag != 0));
        // Vanilla runs the saved int back through `Variant(int)` and repacks it,
        // so an unknown pattern falls back to `KOB` rather than persisting.
        let variant = TropicalFishVariant::from_packed_id(
            nbt.int("Variant")
                .unwrap_or_else(|| TropicalFishVariant::DEFAULT.packed_id()),
        );
        self.set_variant(variant);
    }
}

impl LivingEntity for TropicalFishEntity {
    /// Returns synchronized data declared by vanilla `LivingEntity`.
    fn living_synced_data(&self) -> Option<&dyn LivingEntitySyncedData> {
        Some(&self.entity_data)
    }

    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
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
        Some(&sound_events::ENTITY_TROPICAL_FISH_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_TROPICAL_FISH_DEATH)
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn ai_step(&self) -> Option<MoveResult> {
        fish::flop(self, &sound_events::ENTITY_TROPICAL_FISH_FLOP);
        self.default_ai_step()
    }

    /// Vanilla parity: `AbstractFish.travelInWater`.
    fn travel_in_water(
        &self,
        input: DVec3,
        _base_gravity: f64,
        _is_falling: bool,
        _old_y: f64,
    ) -> Option<MoveResult> {
        fish::travel_in_water(self, input)
    }
}

impl Mob for TropicalFishEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }

    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        let _ = spawn_reason;
        Self::check_tropical_fish_spawn_rules(world, pos)
    }

    /// Vanilla parity: `TropicalFish.finalizeSpawn`. Nine spawns in ten join a
    /// shoal that shares one of the named combinations; the tenth is a
    /// one-off, rolled freely and excluded from schooling.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        let (variant, group_data) = match group_data {
            Some(SpawnGroupData::TropicalFish(shoal)) => {
                (shoal.variant(), SpawnGroupData::TropicalFish(shoal))
            }
            _ if rand::random::<f32>() < COMMON_VARIANT_CHANCE => {
                let variant = COMMON_VARIANTS[rand::random_range(0..COMMON_VARIANTS.len())];
                (
                    variant,
                    SpawnGroupData::TropicalFish(TropicalFishGroupData::new(variant)),
                )
            }
            _ => {
                // Vanilla also clears `isSchool` here, which only feeds
                // `isMaxGroupSizeReached`; Foton has no `AbstractSchoolingFish`,
                // so a lone fish is simply not carried in the group data.
                let variant = TropicalFishVariant::new(
                    TropicalFishPattern::VALUES
                        [rand::random_range(0..TropicalFishPattern::VALUES.len())],
                    DyeColor::VALUES[rand::random_range(0..DyeColor::VALUES.len())],
                    DyeColor::VALUES[rand::random_range(0..DyeColor::VALUES.len())],
                );
                (
                    variant,
                    SpawnGroupData::AgeableMob(AgeableMobGroupData::with_should_spawn_baby(false)),
                )
            }
        };

        self.set_variant(variant);
        self.finalize_spawn_mob_base(world, spawn_reason, Some(group_data))
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_TROPICAL_FISH_AMBIENT)
    }

    /// Vanilla parity: `WaterAnimal.getBaseExperienceReward`.
    fn base_experience_reward_mob(&self) -> i32 {
        1 + rand::random_range(0..3)
    }

    fn ambient_sound_interval(&self) -> i32 {
        fish::AMBIENT_SOUND_INTERVAL
    }

    /// Vanilla parity: `AbstractFish.removeWhenFarAway`.
    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        !self.from_bucket()
    }

    /// Vanilla parity: `AbstractFish.FishMoveControl.tick`.
    fn tick_move_control(&self) {
        fish::tick_move_control(self);
    }
}

impl PathfinderMob for TropicalFishEntity {
    /// Vanilla parity: `AbstractFish.createNavigation`.
    fn navigation_kind(&self) -> NavigationKind {
        NavigationKind::WaterBound {
            allow_breaching: false,
        }
    }
}

#[cfg(test)]
mod tests;
