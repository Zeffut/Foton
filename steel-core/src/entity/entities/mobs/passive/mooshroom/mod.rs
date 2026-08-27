//! Vanilla Mooshroom entity (`MushroomCow`) with variant, stew feeding, and
//! shearing parity.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use simdnbt::{FromNbtTag as _, ToNbtTag as _};
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::data_components::components::SuspiciousStewEffects;
use steel_registry::data_components::vanilla_components::SUSPICIOUS_STEW_EFFECTS;
use steel_registry::entity_type::{
    EntityAttachmentPoint, EntityAttachments, EntityDimensions, EntityTypeRef,
};
use steel_registry::item_stack::ItemStack;
use steel_registry::particle_type::ParticleData;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_entity_data::MushroomCowEntityData;
use steel_registry::vanilla_game_events;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::vanilla_loot_tables;
use steel_registry::{
    REGISTRY, TaggedRegistryExt, sound_events, vanilla_attributes, vanilla_blocks,
    vanilla_entities, vanilla_items, vanilla_particle_types,
};
use steel_utils::locks::SyncMutex;
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, BlockStateId, Downcast as _, DowncastType, DowncastTypeKey};
use uuid::Uuid;

use crate::behavior::InteractionResult;
use crate::entity::ai::goal::{
    BreedGoal, FloatGoal, FollowParentGoal, LookAtPlayerGoal, PanicGoal, RandomLookAroundGoal,
    TemptGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::conversion::{ConversionParams, convert_to};
use crate::entity::damage::DamageSource;
use crate::entity::entities::CowEntity;
use crate::entity::living_entity::shearing_loot_items_with_rng;
use crate::entity::{
    AgeableMob, AgeableMobBase, Animal, AnimalBase, Entity, EntityBase, EntityBaseLoad, EntityPose,
    EntitySpawnReason, EntitySyncedData, LivingEntity, LivingEntityBase, Mob, MobBase,
    PathfinderMob, SpawnGroupData,
};
use crate::physics::MoveResult;
use crate::player::Player;
use crate::world::game_event::GameEventContext;
use crate::world::{LevelReader, World};

/// Vanilla `MushroomCow.BABY_DIMENSIONS`'s passenger attachment
/// (`EntityAttachment.PASSENGER`, 0.0, 0.75, 0.0).
const MOOSHROOM_BABY_PASSENGER_ATTACHMENTS: [EntityAttachmentPoint; 1] =
    [EntityAttachmentPoint::new(0.0, 0.75, 0.0)];
/// Vanilla `MushroomCow.BABY_DIMENSIONS` width.
const MOOSHROOM_BABY_WIDTH: f32 = 0.45;
/// Vanilla `MushroomCow.BABY_DIMENSIONS` height.
const MOOSHROOM_BABY_HEIGHT: f32 = 0.7;
/// Vanilla `MushroomCow.BABY_DIMENSIONS` eye height.
const MOOSHROOM_BABY_EYE_HEIGHT: f32 = 0.69;

/// Vanilla `MushroomCow.BABY_DIMENSIONS`.
const MOOSHROOM_BABY_DIMENSIONS: EntityDimensions = EntityDimensions::new_with_attachments(
    MOOSHROOM_BABY_WIDTH,
    MOOSHROOM_BABY_HEIGHT,
    MOOSHROOM_BABY_EYE_HEIGHT,
    EntityAttachments::new(&MOOSHROOM_BABY_PASSENGER_ATTACHMENTS, &[], &[], &[]),
);
/// Vanilla `LivingEntity`'s default step height, used as the `Entity.maxUpStep`
/// fallback when no `STEP_HEIGHT` attribute modifier is present.
const DEFAULT_STEP_HEIGHT: f32 = 0.6;
/// Vanilla `MushroomCow.MUTATE_CHANCE`: 1-in-1024 odds that two same-variant
/// parents produce a baby of the opposite variant.
const MUTATE_CHANCE: i32 = 1024;

/// Vanilla parity: `MushroomCow.Variant`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MushroomCowVariant {
    /// `MushroomCow.Variant.RED`, id 0. Also `Variant.DEFAULT`.
    Red,
    /// `MushroomCow.Variant.BROWN`, id 1.
    Brown,
}

impl MushroomCowVariant {
    /// Returns the synced entity-data id for this variant (vanilla `Variant.id`).
    #[must_use]
    pub const fn id(self) -> i32 {
        match self {
            Self::Red => 0,
            Self::Brown => 1,
        }
    }

    /// Decodes a synced id, clamping out-of-range values to `BROWN` (or up to `RED`
    /// for negative ids), matching vanilla's
    /// `ByIdMap.continuous(Variant::id, values(), OutOfBoundsStrategy.CLAMP)`.
    #[must_use]
    pub const fn from_id(id: i32) -> Self {
        if id <= 0 { Self::Red } else { Self::Brown }
    }

    /// Returns the persisted name (vanilla `Variant.getSerializedName`, backed by
    /// `Variant.CODEC`).
    #[must_use]
    pub const fn serialized_name(self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Brown => "brown",
        }
    }

    /// Parses a persisted name (vanilla `Variant.CODEC`).
    #[must_use]
    pub fn from_serialized_name(name: &str) -> Option<Self> {
        match name {
            "red" => Some(Self::Red),
            "brown" => Some(Self::Brown),
            _ => None,
        }
    }
}

#[entity_behavior(class = "MushroomCow")]
/// Vanilla mooshroom entity: a `Shearable` `AbstractCow` subclass with a synced
/// RED/BROWN variant and an unsynced suspicious-stew-effect payload.
pub struct MushroomCowEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    entity_data: SyncMutex<MushroomCowEntityData>,
    /// Vanilla `MushroomCow.stewEffects`: server-only state, not part of the synced
    /// entity data. Set by feeding a suspicious-effect item to a brown mooshroom and
    /// consumed the next time it's milked with a bowl.
    stew_effects: SyncMutex<Option<SuspiciousStewEffects>>,
    /// Vanilla `MushroomCow.lastLightningBoltUUID`: the bolt that last flipped
    /// this mooshroom. A bolt sweeps for entities on every tick it is alive, so
    /// without this the variant would flicker back and forth under one strike.
    last_lightning_bolt_uuid: SyncMutex<Option<Uuid>>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `MushroomCowEntity`.
unsafe impl DowncastType for MushroomCowEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/mushroom_cow");
}

impl MushroomCowEntity {
    /// Creates a new mooshroom at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Reconstructs a mooshroom from persisted base entity state.
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
        let mut entity_data = MushroomCowEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Vanilla `AbstractCow.registerGoals`, inherited unchanged by `MushroomCow`.
            let mut goal_selector = mob_base.goal_selector().lock();
            goal_selector.add_goal(0, FloatGoal::new(&mob_base));
            goal_selector.add_goal(1, PanicGoal::new(2.0));
            goal_selector.add_goal(2, BreedGoal::new(1.0));
            goal_selector.add_goal(
                3,
                TemptGoal::new(
                    1.25,
                    |item_stack| {
                        REGISTRY
                            .items
                            .is_in_tag(item_stack.item(), &ItemTag::COW_FOOD)
                    },
                    false,
                ),
            );
            goal_selector.add_goal(4, FollowParentGoal::new(1.25));
            goal_selector.add_goal(5, WaterAvoidingRandomStrollGoal::new(1.0));
            goal_selector.add_goal(6, LookAtPlayerGoal::new(6.0));
            goal_selector.add_goal(7, RandomLookAroundGoal::new());
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            entity_data: SyncMutex::new(entity_data),
            stew_effects: SyncMutex::new(None),
            last_lightning_bolt_uuid: SyncMutex::new(None),
        }
    }

    /// Returns the active mooshroom variant (vanilla `MushroomCow.getVariant`).
    #[must_use]
    pub fn variant(&self) -> MushroomCowVariant {
        MushroomCowVariant::from_id(*self.entity_data.lock().variant_type.get())
    }

    /// Sets the active mooshroom variant (vanilla `MushroomCow.setVariant`).
    pub fn set_variant(&self, variant: MushroomCowVariant) {
        self.entity_data.lock().variant_type.set(variant.id());
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

        // Sync base entity flags from resolved effect display state in one place.
        self.entity_data.set_base_invisible_flag(display.invisible);
        self.entity_data
            .set_base_glowing_flag(self.has_glowing_tag() || display.glowing);
    }

    /// Returns whether an item stack matches the vanilla cow/mooshroom food tag
    /// (vanilla `AbstractCow.isFood`, shared unchanged by `Cow` and `MushroomCow`).
    #[must_use]
    pub fn is_food(item_stack: &ItemStack) -> bool {
        REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::COW_FOOD)
    }

    /// Returns the stored suspicious stew effects (vanilla `MushroomCow.stewEffects`).
    #[must_use]
    pub fn stew_effects(&self) -> Option<SuspiciousStewEffects> {
        self.stew_effects.lock().clone()
    }

    /// Sets the stored suspicious stew effects.
    pub fn set_stew_effects(&self, effects: Option<SuspiciousStewEffects>) {
        *self.stew_effects.lock() = effects;
    }

    /// Returns vanilla `MushroomCow.readyForShearing`.
    #[must_use]
    pub fn ready_for_shearing(&self) -> bool {
        !AgeableMob::is_baby(self)
    }

    /// Handles milking with a bucket. Vanilla parity: `AbstractCow.mobInteract`'s
    /// bucket branch, which `MushroomCow` inherits unchanged by falling through to
    /// `super.mobInteract`.
    fn try_milk(&self, player: &Player, hand: InteractionHand) -> bool {
        if AgeableMob::is_baby(self) {
            return false;
        }

        let is_bucket = {
            let inventory = player.inventory.lock();
            inventory.get_item_in_hand(hand).is(&vanilla_items::BUCKET)
        };
        if !is_bucket {
            return false;
        }

        player.play_sound(&sound_events::ENTITY_COW_MILK, 1.0, 1.0);

        let overflow = {
            let mut inventory = player.inventory.lock();
            inventory.apply_filled_result(
                hand,
                ItemStack::new(&vanilla_items::MILK_BUCKET),
                player.has_infinite_materials(),
                true,
            )
        };

        if !overflow.is_empty() {
            let _ = player.drop_item(overflow, false, false);
        }

        true
    }

    /// Handles milking with a bowl into mushroom stew, or suspicious stew (carrying
    /// this mooshroom's stored effects) when one is set. Vanilla parity:
    /// `MushroomCow.mobInteract`'s bowl branch.
    fn try_fill_bowl(&self, player: &Player, hand: InteractionHand) -> bool {
        if AgeableMob::is_baby(self) {
            return false;
        }

        let is_bowl = {
            let inventory = player.inventory.lock();
            inventory.get_item_in_hand(hand).is(&vanilla_items::BOWL)
        };
        if !is_bowl {
            return false;
        }

        let stored_effects = self.stew_effects.lock().take();
        let (stew, sound) = if let Some(effects) = stored_effects {
            let mut stew = ItemStack::new(&vanilla_items::SUSPICIOUS_STEW);
            stew.set(SUSPICIOUS_STEW_EFFECTS, effects);
            (stew, &sound_events::ENTITY_MOOSHROOM_SUSPICIOUS_MILK)
        } else {
            (
                ItemStack::new(&vanilla_items::MUSHROOM_STEW),
                &sound_events::ENTITY_MOOSHROOM_MILK,
            )
        };

        let overflow = {
            let mut inventory = player.inventory.lock();
            // Vanilla passes `limitCreativeStackSize = false` here, unlike the bucket
            // branch above.
            inventory.apply_filled_result(hand, stew, player.has_infinite_materials(), false)
        };
        if !overflow.is_empty() {
            let _ = player.drop_item(overflow, false, false);
        }

        self.play_sound(sound, 1.0, 1.0);

        true
    }

    /// Vanilla `MushroomCow.shear`: the shear sound, then this mooshroom is
    /// replaced by a plain cow that leaves its mushrooms behind.
    ///
    /// The conversion is the whole point. A mooshroom that stayed a mooshroom
    /// would still be `ready_for_shearing` on the next tick, so one pair of
    /// shears would be an unlimited mushroom supply.
    ///
    /// The particle burst and the drops run inside the conversion callback, as
    /// vanilla does, so they are placed while this mooshroom is still in the
    /// world and before the cow joins it.
    pub fn shear(&self, world: &World, tool: &ItemStack) {
        world.play_sound_at(
            &sound_events::ENTITY_MOOSHROOM_SHEAR,
            SoundSource::Players,
            self.position(),
            1.0,
            1.0,
            None,
        );

        // Vanilla's `ConversionParams.single(this, false, false)`.
        convert_to(
            self,
            ConversionParams::single(false, false),
            |id, position, level| CowEntity::new(&vanilla_entities::COW, id, position, level),
            |_cow| {
                let position = self.position();
                world.send_particles(
                    ParticleData::simple(&vanilla_particle_types::EXPLOSION),
                    DVec3::new(
                        position.x,
                        position.y + self.bounding_box().height() * 0.5,
                        position.z,
                    ),
                    1,
                    DVec3::ZERO,
                    0.0,
                );
                self.drop_shearing_loot(tool);
            },
        );
    }

    /// Rolls and spawns the shearing drops for the current variant.
    ///
    /// The generic `SHEARING_MOOSHROOM` loot table dispatches on a
    /// `mooshroom/variant` entity-property predicate that Steel's loot `EntityRef`
    /// doesn't carry yet (only sheep color/sheared state are wired up there).
    /// Picking the per-variant sub-table directly reproduces the same drops
    /// without needing that predicate.
    fn drop_shearing_loot(&self, tool: &ItemStack) {
        let loot_table = match self.variant() {
            MushroomCowVariant::Red => &vanilla_loot_tables::SHEARING_MOOSHROOM_RED,
            MushroomCowVariant::Brown => &vanilla_loot_tables::SHEARING_MOOSHROOM_BROWN,
        };

        let mut rng = rand::rng();
        for drop in shearing_loot_items_with_rng(self, loot_table, tool, &mut rng) {
            self.spawn_shearing_drop(&drop);
        }
    }

    /// Drops one item entity per unit of `drop`'s count (vanilla `MushroomCow.shear`'s
    /// spawn lambda: a plain `ItemEntity` at `y + 1.0`, with no extra jitter, unlike
    /// `Sheep.shear`).
    fn spawn_shearing_drop(&self, drop: &ItemStack) {
        for _ in 0..drop.count() {
            let _ = self.spawn_at_location(drop.copy_with_count(1), 1.0);
        }
    }
}

impl Entity for MushroomCowEntity {
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
            MOOSHROOM_BABY_DIMENSIONS.scale(scale)
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

    /// Vanilla parity: `MushroomCow.thunderHit`, which flips the variant and
    /// takes neither the damage nor the fire -- there is no `super` call here,
    /// so a mooshroom rides a strike out unharmed.
    fn thunder_hit(&self, _world: &World, bolt: &dyn Entity) {
        let bolt_uuid = bolt.uuid();
        {
            let mut last = self.last_lightning_bolt_uuid.lock();
            if *last == Some(bolt_uuid) {
                return;
            }
            *last = Some(bolt_uuid);
        }

        self.set_variant(match self.variant() {
            MushroomCowVariant::Red => MushroomCowVariant::Brown,
            MushroomCowVariant::Brown => MushroomCowVariant::Red,
        });
        self.play_sound(&sound_events::ENTITY_MOOSHROOM_CONVERT, 2.0, 1.0);
    }

    fn max_up_step(&self) -> f32 {
        self.attributes()
            .lock()
            .get_value(vanilla_attributes::STEP_HEIGHT)
            .unwrap_or(f64::from(DEFAULT_STEP_HEIGHT)) as f32
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Neutral
    }

    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {
        // Vanilla `AbstractCow.playStepSound`, inherited unchanged: `MushroomCow` does
        // not override `getSoundSet`, so it stays on the fixed CLASSIC cow sound set.
        self.play_sound(&sound_events::ENTITY_COW_STEP, 0.15, 1.0);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
        nbt.insert("Type", self.variant().serialized_name());
        if let Some(effects) = self.stew_effects() {
            nbt.insert("stew_effects", effects.to_nbt_tag());
        }
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);

        if let Some(raw_variant) = nbt.string("Type")
            && let Some(variant) =
                MushroomCowVariant::from_serialized_name(raw_variant.to_str().as_ref())
        {
            self.set_variant(variant);
        }

        self.set_stew_effects(
            nbt.get("stew_effects")
                .and_then(SuspiciousStewEffects::from_nbt_tag),
        );
    }
}

impl LivingEntity for MushroomCowEntity {
    fn mooshroom_loot_variant(&self) -> Option<&'static str> {
        Some(self.variant().serialized_name())
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

    fn sound_volume(&self) -> f32 {
        0.4
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        // `AbstractCow.getHurtSound` reads the fixed CLASSIC sound set.
        Some(&sound_events::ENTITY_COW_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        // `AbstractCow.getDeathSound` reads the fixed CLASSIC sound set.
        Some(&sound_events::ENTITY_COW_DEATH)
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

impl AgeableMob for MushroomCowEntity {
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

    fn age_boundary_changed(&self, _baby: bool) {
        self.refresh_dimensions();
    }
}

impl Animal for MushroomCowEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        MushroomCowEntity::is_food(item_stack)
    }

    /// Vanilla `MushroomCow.getWalkTargetValue`: mycelium is preferred over grass.
    fn animal_walk_target_value(&self, pos: BlockPos) -> f32 {
        let Some(world) = self.level() else {
            return 0.0;
        };

        if world.get_block_state(pos.below()).get_block() == &vanilla_blocks::MYCELIUM {
            10.0
        } else {
            world.pathfinding_cost_from_light_levels(pos)
        }
    }

    /// Vanilla `MushroomCow.checkMushroomSpawnRules`: a dedicated spawn predicate
    /// (not an override of `Animal.checkAnimalSpawnRules`) that requires mooshroom
    /// spawnable ground and always checks brightness, regardless of spawn reason.
    fn check_animal_spawn_rules(
        level: &dyn LevelReader,
        _spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool
    where
        Self: Sized,
    {
        level
            .get_block_state(pos.below())
            .get_block()
            .has_tag(&BlockTag::MOOSHROOMS_SPAWNABLE_ON)
            && Self::is_bright_enough_to_spawn(level, pos)
    }

    /// Vanilla `MushroomCow.getOffspringVariant`, invoked from `getBreedOffspring`.
    fn initialize_breed_offspring(&self, partner: &dyn Animal, offspring: &dyn Animal) {
        let Some(partner_variant) = partner
            .downcast_ref::<MushroomCowEntity>()
            .map(MushroomCowEntity::variant)
        else {
            return;
        };
        let self_variant = self.variant();

        let offspring_variant =
            if self_variant == partner_variant && rand::random_range(0..MUTATE_CHANCE) == 0 {
                match self_variant {
                    MushroomCowVariant::Brown => MushroomCowVariant::Red,
                    MushroomCowVariant::Red => MushroomCowVariant::Brown,
                }
            } else if rand::random::<bool>() {
                self_variant
            } else {
                partner_variant
            };

        if let Some(offspring) = offspring.downcast_ref::<MushroomCowEntity>() {
            offspring.set_variant(offspring_variant);
        }
    }
}

impl Mob for MushroomCowEntity {
    /// Returns whether this mob accepts where the spawner put it.
    ///
    /// Vanilla parity: `MushroomCow::checkMushroomSpawnRules`, reached through
    /// this mob's own `check_animal_spawn_rules`, which already narrows the
    /// ground to mycelium and the like. That is why mooshrooms stay on their
    /// island.
    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        <Self as Animal>::check_animal_spawn_rules(world.as_ref(), spawn_reason, pos)
    }

    fn mob_base(&self) -> &MobBase {
        &self.mob_base
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

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        // `AbstractCow.getAmbientSound` reads the fixed CLASSIC sound set.
        Some(&sound_events::ENTITY_COW_AMBIENT)
    }

    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        // Vanilla `MushroomCow` does not override `finalizeSpawn`; it inherits
        // `AgeableMob.finalizeSpawn` through `AbstractCow`/`Animal` unchanged (the
        // variant stays at its synced default, `Variant.DEFAULT` = RED).
        self.finalize_spawn_ageable_mob(world, spawn_reason, group_data)
    }

    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        if self.try_fill_bowl(player, hand) {
            return InteractionResult::Success;
        }

        let item_stack = {
            let inventory = player.inventory.lock();
            let item_stack = inventory.get_item_in_hand(hand);
            item_stack.copy_with_count(item_stack.count())
        };

        if item_stack.is(&vanilla_items::SHEARS) && self.ready_for_shearing() {
            if let Some(world) = self.level() {
                self.shear(world.as_ref(), &item_stack);
                // Vanilla `MushroomCow.mobInteract` sources the shear game event to
                // the player.
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

        // TODO: vanilla's `else if (getVariant() == BROWN && !isBaby())` branch lets a
        // brown mooshroom absorb a suspicious-effect item (e.g. a flower recognized by
        // `SuspiciousEffectHolder`) into `stewEffects`, playing `MOOSHROOM_EAT` and
        // spawning particles. Steel's item/block registry has no per-item
        // suspicious-stew-effect table (a `SuspiciousEffectHolder` equivalent) yet, so
        // there's nothing to look up here; every held item falls through exactly like
        // vanilla's `effectsFromItemStack.isEmpty()` case.

        if self.try_milk(player, hand) {
            return InteractionResult::Success;
        }

        Animal::mob_interact_animal(self, player, hand)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for MushroomCowEntity {}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound as read_borrowed_compound;
    use steel_registry::data_components::components::SuspiciousStewEffect;
    use steel_registry::{RegistryExt, init_vanilla_registry, vanilla_entities};
    use steel_utils::{ChunkPos, Identifier};

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::entity::entities::PigEntity;
    use crate::entity::{SharedEntity, next_entity_id};
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    fn new_mooshroom(id: i32) -> MushroomCowEntity {
        MushroomCowEntity::new(&vanilla_entities::MOOSHROOM, id, DVec3::ZERO, Weak::new())
    }

    /// Puts a mooshroom in a live test world and hands back the shared handle
    /// plus the world it joined.
    fn mooshroom_in_world(name: &'static str) -> (Arc<World>, SharedEntity) {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world(name);
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

        let mooshroom: SharedEntity = Arc::new(MushroomCowEntity::new(
            &vanilla_entities::MOOSHROOM,
            next_entity_id(),
            DVec3::new(8.5, 64.0, 8.5),
            Arc::downgrade(&world),
        ));
        world
            .try_add_entity(Arc::clone(&mooshroom))
            .unwrap_or_else(|error| panic!("mooshroom should join the test world: {error:?}"));
        (world, mooshroom)
    }

    #[test]
    fn mooshroom_defaults_to_red_variant() {
        init_vanilla_registry();
        let mooshroom = new_mooshroom(1);
        assert_eq!(mooshroom.variant(), MushroomCowVariant::Red);
    }

    #[test]
    fn variant_serialized_name_round_trips() {
        for variant in [MushroomCowVariant::Red, MushroomCowVariant::Brown] {
            assert_eq!(
                MushroomCowVariant::from_serialized_name(variant.serialized_name()),
                Some(variant)
            );
        }
        assert_eq!(MushroomCowVariant::from_serialized_name("purple"), None);
    }

    #[test]
    fn variant_from_id_clamps_out_of_range_like_vanilla() {
        assert_eq!(MushroomCowVariant::from_id(0), MushroomCowVariant::Red);
        assert_eq!(MushroomCowVariant::from_id(1), MushroomCowVariant::Brown);
        assert_eq!(MushroomCowVariant::from_id(-5), MushroomCowVariant::Red);
        assert_eq!(MushroomCowVariant::from_id(99), MushroomCowVariant::Brown);
    }

    /// Shearing has to leave a cow behind.
    ///
    /// A mooshroom that stayed a mooshroom is `ready_for_shearing` again on the
    /// next tick, so a single pair of shears would be an unlimited supply of
    /// mushrooms. The cow and the drops are asserted together because the drops
    /// are rolled inside the conversion callback: moving them out, or dropping
    /// the conversion, breaks exactly one of the two.
    #[test]
    fn shearing_a_mooshroom_leaves_a_cow_and_its_mushrooms() {
        let (world, mooshroom) = mooshroom_in_world("mooshroom_shearing");
        let mooshroom = mooshroom
            .downcast_ref::<MushroomCowEntity>()
            .expect("the shared entity is the mooshroom");

        mooshroom.shear(world.as_ref(), &ItemStack::new(&vanilla_items::SHEARS));

        assert!(
            mooshroom.is_removed(),
            "the sheared mooshroom should have been replaced"
        );

        let nearby = mooshroom.bounding_box().inflate(4.0);
        let cows = world.get_entities_in_aabb_matching(&nearby, |entity| {
            entity.entity_type() == &vanilla_entities::COW
        });
        assert_eq!(cows.len(), 1, "shearing leaves exactly one cow behind");

        let dropped = world.get_entities_in_aabb_matching(&nearby, |entity| {
            entity.entity_type() == &vanilla_entities::ITEM
        });
        assert_eq!(
            dropped.len(),
            5,
            "`shearing/mooshroom/red` rolls five separate red mushrooms"
        );
    }

    /// The same mooshroom cannot be sheared twice: it is gone after the first
    /// pass, so the second finds nothing to convert and drops nothing.
    #[test]
    fn a_sheared_mooshroom_cannot_be_sheared_again() {
        let (world, mooshroom) = mooshroom_in_world("mooshroom_reshearing");
        let mooshroom = mooshroom
            .downcast_ref::<MushroomCowEntity>()
            .expect("the shared entity is the mooshroom");
        let shears = ItemStack::new(&vanilla_items::SHEARS);

        mooshroom.shear(world.as_ref(), &shears);
        mooshroom.shear(world.as_ref(), &shears);

        let nearby = mooshroom.bounding_box().inflate(4.0);
        let dropped = world.get_entities_in_aabb_matching(&nearby, |entity| {
            entity.entity_type() == &vanilla_entities::ITEM
        });
        assert_eq!(
            dropped.len(),
            5,
            "a second shear must not pay out a second time"
        );
    }

    #[test]
    fn ready_for_shearing_requires_non_baby() {
        init_vanilla_registry();
        let mooshroom = new_mooshroom(1);
        assert!(mooshroom.ready_for_shearing());

        Mob::set_baby(&mooshroom, true);
        assert!(!mooshroom.ready_for_shearing());
    }

    #[test]
    fn is_food_matches_cow_food_tag() {
        init_vanilla_registry();
        assert!(MushroomCowEntity::is_food(&ItemStack::new(
            &vanilla_items::WHEAT
        )));
        assert!(!MushroomCowEntity::is_food(&ItemStack::new(
            &vanilla_items::APPLE
        )));
    }

    #[test]
    fn stew_effects_get_and_set_round_trip() {
        init_vanilla_registry();
        let mooshroom = new_mooshroom(1);
        assert_eq!(mooshroom.stew_effects(), None);

        let effects = SuspiciousStewEffects::empty();
        mooshroom.set_stew_effects(Some(effects.clone()));
        assert_eq!(mooshroom.stew_effects(), Some(effects));

        mooshroom.set_stew_effects(None);
        assert_eq!(mooshroom.stew_effects(), None);
    }

    #[test]
    fn save_and_load_round_trip_persists_variant_and_stew_effects() {
        init_vanilla_registry();
        let mooshroom = new_mooshroom(1);
        mooshroom.set_variant(MushroomCowVariant::Brown);
        let night_vision = REGISTRY
            .mob_effects
            .by_key(&Identifier::vanilla_static("night_vision"))
            .expect("night vision should be registered");
        let effects = SuspiciousStewEffects::new(vec![SuspiciousStewEffect::new(
            night_vision,
            SuspiciousStewEffect::DEFAULT_DURATION,
        )]);
        mooshroom.set_stew_effects(Some(effects.clone()));

        let mut nbt = NbtCompound::new();
        mooshroom.save_additional(&mut nbt);

        let mut bytes = Vec::new();
        nbt.write(&mut bytes);
        let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
            .unwrap_or_else(|error| panic!("test nbt should reborrow: {error}"));

        let loaded = new_mooshroom(2);
        loaded.load_additional((&borrowed).into());

        assert_eq!(loaded.variant(), MushroomCowVariant::Brown);
        assert_eq!(loaded.stew_effects(), Some(effects));
    }

    #[test]
    fn initialize_breed_offspring_leaves_variant_unchanged_for_non_mooshroom_partner() {
        init_vanilla_registry();
        let parent = new_mooshroom(1);
        parent.set_variant(MushroomCowVariant::Brown);
        let offspring = new_mooshroom(2);
        offspring.set_variant(MushroomCowVariant::Red);
        let unrelated_partner = PigEntity::new(&vanilla_entities::PIG, 3, DVec3::ZERO, Weak::new());

        parent.initialize_breed_offspring(&unrelated_partner, &offspring);

        assert_eq!(offspring.variant(), MushroomCowVariant::Red);
    }

    #[test]
    fn initialize_breed_offspring_can_produce_either_parent_variant_when_they_differ() {
        init_vanilla_registry();
        let mut saw_red = false;
        let mut saw_brown = false;

        for i in 0..200 {
            let parent = new_mooshroom(i);
            parent.set_variant(MushroomCowVariant::Red);
            let partner = new_mooshroom(i + 1000);
            partner.set_variant(MushroomCowVariant::Brown);
            let offspring = new_mooshroom(i + 2000);

            parent.initialize_breed_offspring(&partner, &offspring);

            match offspring.variant() {
                MushroomCowVariant::Red => saw_red = true,
                MushroomCowVariant::Brown => saw_brown = true,
            }
            if saw_red && saw_brown {
                break;
            }
        }

        assert!(saw_red && saw_brown);
    }
}
