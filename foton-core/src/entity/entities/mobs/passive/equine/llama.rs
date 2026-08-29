//! The vanilla llama.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.equine.Llama`. A chested
//! horse that never rears, never grazes, spits at wolves, and strings itself
//! into a caravan behind a leashed neighbour.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_registry::entity_type::{
    EntityAttachmentPoint, EntityAttachments, EntityDimensions, EntityTypeRef,
};
use foton_registry::item_stack::ItemStack;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_entity_data::LlamaEntityData;
use foton_registry::vanilla_item_tags::ItemTag;
use foton_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_attributes, vanilla_entities,
};
use foton_utils::locks::SyncMutex;
use foton_utils::types::InteractionHand;
use foton_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::behavior::InteractionResult;
use crate::entity::ai::goal::{
    BreedGoal, FloatGoal, FollowParentGoal, Goal, GoalControls, HurtByTargetGoal,
    LlamaFollowCaravanGoal, LookAtPlayerGoal, NearestAttackableTargetGoal, PanicGoal,
    RandomLookAroundGoal, RangedAttackGoal, RunAroundLikeCrazyGoal, TemptGoal,
    WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::entities::mobs::passive::equine::sync_mob_effect_entity_data;
use crate::entity::{
    AbstractChestedHorse, AbstractHorse, AbstractHorseBase, AgeableMob, AgeableMobBase, Animal,
    AnimalBase, ENTITIES, Entity, EntityBase, EntityBaseLoad, EntityPose, EntitySpawnReason,
    EntitySyncedData, LivingEntity, LivingEntityBase, LivingEntitySyncedData, Llama, LlamaBase,
    LlamaGroupData, LlamaVariant, Mob, MobBase, MoveResult, PathfinderMob, SharedEntity,
    SpawnGroupData, generate_max_health, is_tamed, next_entity_id, should_follow_mommy,
};
use crate::inventory::equipment::EquipmentSlot;
use crate::player::Player;
use crate::world::World;

/// How much smaller a llama cria is.
///
/// Vanilla parity: the `scale(0.5F)` of `Llama.BABY_DIMENSIONS`.
const LLAMA_BABY_SCALE: f32 = 0.5;

/// Where a cria carries its rider.
///
/// Vanilla parity: `Llama.BABY_DIMENSIONS`, attached at
/// `LLAMA.getHeight() - 0.25F` and `-0.3F` back before the halving.
const LLAMA_BABY_PASSENGER_ATTACHMENTS: [EntityAttachmentPoint; 1] = [EntityAttachmentPoint::new(
    0.0,
    (1.87 - 0.25) * LLAMA_BABY_SCALE as f64,
    -0.3 * LLAMA_BABY_SCALE as f64,
)];

/// A cria's hitbox.
const LLAMA_BABY_DIMENSIONS: EntityDimensions = EntityDimensions::new_with_attachments(
    0.9 * LLAMA_BABY_SCALE,
    1.87 * LLAMA_BABY_SCALE,
    1.7765 * LLAMA_BABY_SCALE,
    EntityAttachments::new(&LLAMA_BABY_PASSENGER_ATTACHMENTS, &[], &[], &[]),
);

/// How far a llama is willing to path.
///
/// Vanilla parity: the `setRequiredPathLength(40.0F)` of the `Llama` constructor,
/// which is what lets a caravan straggler find its way back.
pub(super) const LLAMA_REQUIRED_PATH_LENGTH: f32 = 40.0;

/// A llama's temper ceiling.
///
/// Vanilla parity: `Llama.getMaxTemper`.
pub(super) const LLAMA_MAX_TEMPER: i32 = 30;

/// How far a llama searches for a wolf to spit at.
///
/// Vanilla parity: the `16` random interval of `Llama.LlamaAttackWolfGoal`.
const WOLF_SEARCH_INTERVAL: i32 = 16;

/// How much of its follow range a llama uses to look for wolves.
///
/// Vanilla parity: the `getFollowDistance() * 0.25` of the same goal.
const WOLF_FOLLOW_DISTANCE_SCALE: f64 = 0.25;

/// Wraps the shared hurt-by-target goal with the llama's spit rule.
///
/// Vanilla parity: `Llama.LlamaHurtByTargetGoal`, which drops the grudge as soon
/// as the llama has spat once -- a llama retaliates, it does not hunt.
pub(super) struct LlamaHurtByTargetGoal {
    inner: HurtByTargetGoal,
}

impl LlamaHurtByTargetGoal {
    #[must_use]
    pub(super) const fn new() -> Self {
        Self {
            inner: HurtByTargetGoal::new(),
        }
    }
}

impl Goal for LlamaHurtByTargetGoal {
    fn controls(&self) -> GoalControls {
        self.inner.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.inner.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        if let Some(llama) = mob.as_llama()
            && llama.did_spit()
        {
            llama.set_did_spit(false);
            return false;
        }
        self.inner.can_continue_to_use(mob)
    }

    fn is_interruptable(&self) -> bool {
        self.inner.is_interruptable()
    }

    fn requires_update_every_tick(&self) -> bool {
        self.inner.requires_update_every_tick()
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

/// Spits at a target.
///
/// Vanilla parity: `Llama.performRangedAttack`.
pub(super) fn spit_at_target(mob: &dyn PathfinderMob, target: &SharedEntity, _power: f32) {
    if let Some(llama) = mob.as_llama() {
        llama.spit(target);
    }
}

/// Registers the goals every llama shares.
///
/// Vanilla parity: `Llama.registerGoals`, which replaces the horse list wholesale.
pub(super) fn add_llama_goals(mob_base: &MobBase) {
    {
        let mut goals = mob_base.goal_selector().lock();
        goals.add_goal(0, FloatGoal::new(mob_base));
        goals.add_goal(1, RunAroundLikeCrazyGoal::new(1.2));
        goals.add_goal(2, LlamaFollowCaravanGoal::new(2.1));
        goals.add_goal(3, RangedAttackGoal::new(1.25, 40, 20.0, spit_at_target));
        goals.add_goal(3, PanicGoal::new(1.2));
        goals.add_goal(4, BreedGoal::new(1.0));
        goals.add_goal(
            5,
            TemptGoal::new(
                1.25,
                |item_stack| {
                    REGISTRY
                        .items
                        .is_in_tag(item_stack.item(), &ItemTag::LLAMA_TEMPT_ITEMS)
                },
                false,
            ),
        );
        goals.add_goal(6, FollowParentGoal::new(1.0));
        goals.add_goal(7, WaterAvoidingRandomStrollGoal::new(0.7));
        goals.add_goal(8, LookAtPlayerGoal::new(6.0));
        goals.add_goal(9, RandomLookAroundGoal::new());
    }

    let mut targets = mob_base.target_selector().lock();
    targets.add_goal(1, LlamaHurtByTargetGoal::new());
    targets.add_goal(
        2,
        NearestAttackableTargetGoal::new_with_interval(
            WOLF_SEARCH_INTERVAL,
            false,
            true,
            |_, target, _| {
                target.entity_type() == &vanilla_entities::WOLF
                    && !is_tamed(target.as_entity_event_source())
            },
        )
        .with_follow_distance_scale(WOLF_FOLLOW_DISTANCE_SCALE),
    );
}

/// Applies the navigation reach a caravan needs.
pub(super) fn configure_llama_navigation(mob_base: &MobBase) {
    let mut navigation = mob_base.navigation().lock();
    navigation.set_required_path_length(
        LLAMA_REQUIRED_PATH_LENGTH,
        f64::from(LLAMA_REQUIRED_PATH_LENGTH),
    );
}

/// A vanilla llama.
#[entity_behavior(class = "Llama")]
pub struct LlamaEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    horse_base: AbstractHorseBase,
    llama_base: LlamaBase,
    entity_data: SyncMutex<LlamaEntityData>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `LlamaEntity`.
unsafe impl DowncastType for LlamaEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/llama");
}

impl LlamaEntity {
    /// Creates a new llama.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a llama from saved base data.
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
        let mut entity_data = LlamaEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        configure_llama_navigation(&mob_base);
        add_llama_goals(&mob_base);

        let horse_base = AbstractHorseBase::new(0);
        // Vanilla parity: the `this.canGallop = false` of `AbstractChestedHorse`.
        horse_base.set_can_gallop(false);

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            horse_base,
            llama_base: LlamaBase::new(),
            entity_data: SyncMutex::new(entity_data),
        }
    }
}

impl Entity for LlamaEntity {
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
        self.tick_abstract_horse();
    }

    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let scale = LivingEntity::get_scale(self);
        if AgeableMob::is_baby(self) {
            LLAMA_BABY_DIMENSIONS.scale(scale)
        } else if self.entity_type.fixed {
            self.entity_type.dimensions
        } else {
            self.entity_type.dimensions.scale(scale)
        }
    }

    fn controlling_passenger(&self) -> Option<SharedEntity> {
        if Mob::is_saddled(self)
            && let Some(passenger) = self.first_passenger()
            && passenger.as_player().is_some()
        {
            return Some(passenger);
        }
        self.controlling_passenger_mob()
    }

    fn on_climbable(&self) -> bool {
        false
    }

    fn is_pushable(&self) -> bool {
        !self.is_vehicle()
    }

    fn can_jump_while_ridden(&self) -> bool {
        Mob::is_saddled(self)
    }

    fn handle_start_jump(&self, _jump_scale: i32) {
        self.handle_start_jump_abstract_horse();
    }

    fn open_custom_inventory_screen(&self, player: &Player) {
        if (self.is_vehicle() && !self.has_passenger(player)) || !self.is_tamed() {
            return;
        }
        self.open_horse_inventory_screen(player);
    }

    fn cause_fall_damage(
        &self,
        fall_distance: f64,
        damage_modifier: f32,
        source: &DamageSource,
    ) -> bool {
        self.llama_cause_fall_damage(fall_distance, damage_modifier, source)
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn update_data_before_sync(&self) {
        sync_mob_effect_entity_data(self, &self.entity_data);
    }

    /// Vanilla parity: `Llama.playStepSound`, a flat pad regardless of the block.
    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {
        self.play_sound(&sound_events::ENTITY_LLAMA_STEP, 0.15, 1.0);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
        self.save_abstract_horse(nbt);
        self.save_chested_horse(nbt);
        nbt.insert("Variant", self.llama_variant().id());
        nbt.insert("Strength", self.strength());
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        // Vanilla reads Strength before the super call so the inventory that
        // `readAdditionalSaveData` rebuilds is already the right width.
        self.set_strength(nbt.int("Strength").unwrap_or(0));
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
        self.load_abstract_horse(nbt);
        self.load_chested_horse(nbt);
        self.set_llama_variant(LlamaVariant::by_id(nbt.int("Variant").unwrap_or(0)));
    }
}

impl LivingEntity for LlamaEntity {
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

    fn sound_volume(&self) -> f32 {
        0.8
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_LLAMA_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_LLAMA_DEATH)
    }

    fn hurt_server(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        let was_hurt = self.living_hurt_server(world, source, amount);
        self.abstract_horse_react_to_hurt(was_hurt)
    }

    /// Vanilla parity: `Llama.canUseSlot`.
    fn can_use_slot(&self, _slot: EquipmentSlot) -> bool {
        true
    }

    fn can_dispenser_equip_into_slot(&self, slot: EquipmentSlot) -> bool {
        self.abstract_horse_can_dispenser_equip_into_slot(slot)
    }

    fn equip_sound(&self, slot: EquipmentSlot, stack: &ItemStack) -> Option<SoundEventRef> {
        if slot == EquipmentSlot::Saddle {
            return Some(&sound_events::ENTITY_HORSE_SADDLE);
        }
        LivingEntity::default_equip_sound(self, slot, stack)
    }

    fn is_immobile(&self) -> bool {
        self.llama_is_immobile()
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn tick_ridden(&self, controller: &Player, ridden_input: DVec3) {
        self.tick_ridden_abstract_horse(controller, ridden_input);
    }

    fn ridden_input(&self, controller: &Player, _self_input: DVec3) -> DVec3 {
        self.abstract_horse_ridden_input(controller)
    }

    fn ridden_speed(&self, _controller: &Player) -> f32 {
        self.abstract_horse_ridden_speed()
    }

    fn drop_custom_death_loot(&self, _source: &DamageSource, _killed_by_player: bool) {
        self.drop_abstract_horse_inventory();
        self.drop_chested_horse_chest();
    }

    fn ai_step(&self) -> Option<MoveResult> {
        self.ai_step_abstract_horse();
        let result = self.default_ai_step();
        AgeableMob::tick_ageable_mob(self);
        Animal::tick_animal_love(self);
        self.server_ai_step_abstract_horse();
        result
    }
}

impl AgeableMob for LlamaEntity {
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

impl Animal for LlamaEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        self.is_llama_food(item_stack)
    }

    /// Vanilla parity: `Llama.canMate`.
    fn can_mate(&self, partner: &dyn Animal) -> bool {
        if partner.uuid() == self.uuid() {
            return false;
        }
        let Some(partner_llama) = partner.as_llama() else {
            return false;
        };
        self.can_parent() && partner_llama.can_parent()
    }

    /// Vanilla parity: `Llama.getBreedOffspring`.
    fn get_breed_offspring(
        &self,
        world: &Arc<World>,
        partner: &dyn Animal,
    ) -> Option<SharedEntity> {
        let offspring = self.make_new_llama(world)?;
        let baby = offspring.as_llama()?;
        let partner_llama = partner.as_llama()?;
        self.initialize_bred_llama(partner_llama, baby);
        Some(offspring)
    }
}

impl AbstractHorse for LlamaEntity {
    fn abstract_horse_base(&self) -> &AbstractHorseBase {
        &self.horse_base
    }

    fn horse_flags(&self) -> i8 {
        *self.entity_data.lock().abstract_horse().id_flags.get()
    }

    fn set_horse_flags(&self, flags: i8) {
        self.entity_data
            .lock()
            .abstract_horse_mut()
            .id_flags
            .set(flags);
    }

    fn inventory_columns(&self) -> usize {
        self.llama_inventory_columns()
    }

    /// Vanilla parity: `Llama.canPerformRearing`, which is why a llama never rears.
    fn can_perform_rearing(&self) -> bool {
        false
    }

    /// Vanilla parity: `Llama.canEatGrass`.
    fn can_eat_grass(&self) -> bool {
        false
    }

    fn max_temper(&self) -> i32 {
        LLAMA_MAX_TEMPER
    }

    fn eating_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_LLAMA_EAT)
    }

    fn angry_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_LLAMA_ANGRY)
    }

    fn handle_eating(&self, player: &Player, item_stack: &ItemStack) -> bool {
        self.llama_handle_eating(player, item_stack)
    }

    /// Vanilla parity: `Llama.followMommy`, which a cria in a caravan skips.
    fn follow_mommy(&self, world: &Arc<World>) {
        if should_follow_mommy(self) {
            AbstractHorse::follow_mommy_default(self, world);
        }
    }

    /// Vanilla parity: `AbstractChestedHorse.randomizeAttributes`, health only.
    fn randomize_attributes(&self) {
        self.attributes().lock().set_base_value(
            vanilla_attributes::MAX_HEALTH,
            f64::from(generate_max_health(&mut |bound| {
                rand::random_range(0..bound)
            })),
        );
    }
}

impl AbstractChestedHorse for LlamaEntity {
    /// Vanilla parity: `Llama.playChestEquipsSound`.
    fn play_chest_equips_sound(&self) {
        let pitch = (rand::random::<f32>() - rand::random::<f32>()).mul_add(0.2, 1.0);
        self.play_sound(&sound_events::ENTITY_LLAMA_CHEST, 1.0, pitch);
    }

    fn has_chest(&self) -> bool {
        *self
            .entity_data
            .lock()
            .abstract_chested_horse()
            .id_chest
            .get()
    }

    fn set_chest(&self, has_chest: bool) {
        self.entity_data
            .lock()
            .abstract_chested_horse_mut()
            .id_chest
            .set(has_chest);
    }
}

impl Llama for LlamaEntity {
    fn llama_base(&self) -> &LlamaBase {
        &self.llama_base
    }

    fn synced_strength(&self) -> i32 {
        *self.entity_data.lock().strength.get()
    }

    fn set_synced_strength(&self, strength: i32) {
        self.entity_data.lock().strength.set(strength);
    }

    fn synced_variant_id(&self) -> i32 {
        *self.entity_data.lock().variant.get()
    }

    fn set_synced_variant_id(&self, variant_id: i32) {
        self.entity_data.lock().variant.set(variant_id);
    }

    /// Vanilla parity: `Llama.makeNewLlama`.
    fn make_new_llama(&self, world: &Arc<World>) -> Option<SharedEntity> {
        ENTITIES.create(
            &vanilla_entities::LLAMA,
            next_entity_id(),
            self.position(),
            Arc::downgrade(world),
        )
    }
}

impl Mob for LlamaEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        <Self as Animal>::check_animal_spawn_rules(world.as_ref(), spawn_reason, pos)
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

    fn ambient_sound_interval(&self) -> i32 {
        400
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_LLAMA_AMBIENT)
    }

    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        self.finalize_spawn_llama(world);
        let (variant, group_data) = if let Some(SpawnGroupData::Llama(llama_group)) = group_data {
            (llama_group.variant(), SpawnGroupData::Llama(llama_group))
        } else {
            let variant = LlamaVariant::random();
            (variant, SpawnGroupData::Llama(LlamaGroupData::new(variant)))
        };

        self.set_llama_variant(variant);
        self.finalize_spawn_abstract_horse(world, spawn_reason, Some(group_data))
    }

    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        self.chested_horse_mob_interact(player, hand)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for LlamaEntity {}
