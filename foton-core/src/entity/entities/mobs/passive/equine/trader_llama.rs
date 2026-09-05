//! The vanilla trader llama.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.equine.TraderLlama`. A
//! llama with a despawn clock: it counts down unless it is tamed, ridden, or
//! tied to the trader that brought it.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_registry::entity_type::{
    EntityAttachmentPoint, EntityAttachments, EntityDimensions, EntityTypeRef,
};
use foton_registry::item_stack::ItemStack;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_entity_data::TraderLlamaEntityData;
use foton_registry::{sound_events, vanilla_attributes, vanilla_entities};
use foton_utils::locks::SyncMutex;
use foton_utils::types::InteractionHand;
use foton_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::behavior::InteractionResult;
use crate::entity::ai::goal::{NearestAttackableTargetGoal, PanicGoal};
use crate::entity::damage::DamageSource;
use crate::entity::entities::mobs::passive::equine::llama::{
    LLAMA_MAX_TEMPER, add_llama_goals, configure_llama_navigation,
};
use crate::entity::entities::mobs::passive::equine::sync_mob_effect_entity_data;
use crate::entity::{
    AbstractChestedHorse, AbstractHorse, AbstractHorseBase, AgeableMob, AgeableMobBase,
    AgeableMobGroupData, Animal, AnimalBase, ENTITIES, Entity, EntityBase, EntityBaseLoad,
    EntityPose, EntitySpawnReason, EntitySyncedData, LivingEntity, LivingEntityBase,
    LivingEntitySyncedData, Llama, LlamaBase, LlamaGroupData, LlamaVariant, Mob, MobBase,
    MoveResult, PathfinderMob, RemovalReason, SharedEntity, SpawnGroupData, generate_max_health,
    next_entity_id, should_follow_mommy,
};
use crate::inventory::equipment::EquipmentSlot;
use crate::player::Player;
use crate::world::World;

/// How much smaller a trader cria is.
const LLAMA_BABY_SCALE: f32 = 0.5;

/// Where a trader cria carries its rider.
const TRADER_LLAMA_BABY_PASSENGER_ATTACHMENTS: [EntityAttachmentPoint; 1] =
    [EntityAttachmentPoint::new(
        0.0,
        (1.87 - 0.25) * LLAMA_BABY_SCALE as f64,
        -0.3 * LLAMA_BABY_SCALE as f64,
    )];

/// A trader cria's hitbox.
const TRADER_LLAMA_BABY_DIMENSIONS: EntityDimensions = EntityDimensions::new_with_attachments(
    0.9 * LLAMA_BABY_SCALE,
    1.87 * LLAMA_BABY_SCALE,
    1.7765 * LLAMA_BABY_SCALE,
    EntityAttachments::new(&TRADER_LLAMA_BABY_PASSENGER_ATTACHMENTS, &[], &[], &[]),
);

/// Ticks a loose trader llama lasts.
///
/// Vanilla parity: `TraderLlama.DEFAULT_DESPAWN_DELAY`.
const DEFAULT_DESPAWN_DELAY: i32 = 47999;

/// A vanilla trader llama.
#[entity_behavior(class = "TraderLlama")]
pub struct TraderLlamaEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    horse_base: AbstractHorseBase,
    llama_base: LlamaBase,
    despawn_delay: SyncMutex<i32>,
    entity_data: SyncMutex<TraderLlamaEntityData>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `TraderLlamaEntity`.
unsafe impl DowncastType for TraderLlamaEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/trader_llama");
}

impl TraderLlamaEntity {
    /// Creates a new trader llama.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a trader llama from saved base data.
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
        let mut entity_data = TraderLlamaEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        configure_llama_navigation(&mob_base);
        add_llama_goals(&mob_base);
        {
            // Vanilla parity: `TraderLlama.registerGoals` adds a faster panic on
            // top of the llama list.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(1, PanicGoal::new(2.0));
        }
        {
            // MISSING FOUNDATION: vanilla also adds
            // `TraderLlamaDefendWanderingTraderGoal` and a target goal for
            // `AbstractIllager`. Foton has neither the wandering trader nor the
            // illagers, so only the zombie target survives the port.
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(
                2,
                NearestAttackableTargetGoal::new(true, |_, target, _| {
                    target.entity_type() == &vanilla_entities::ZOMBIE
                }),
            );
        }

        let horse_base = AbstractHorseBase::new(0);
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
            despawn_delay: SyncMutex::new(DEFAULT_DESPAWN_DELAY),
            entity_data: SyncMutex::new(entity_data),
        }
    }

    /// Applies vanilla `TraderLlama.setDespawnDelay`.
    pub fn set_despawn_delay(&self, despawn_delay: i32) {
        *self.despawn_delay.lock() = despawn_delay;
    }

    /// Returns vanilla `TraderLlama.despawnDelay`.
    #[must_use]
    pub fn despawn_delay(&self) -> i32 {
        *self.despawn_delay.lock()
    }

    /// Returns vanilla `TraderLlama.canDespawn`.
    ///
    /// MISSING FOUNDATION: vanilla also excludes a llama leashed to a wandering
    /// trader; Foton has no wandering trader, so any leash counts.
    fn can_despawn(&self) -> bool {
        !self.is_tamed()
            && !Mob::is_leashed(self)
            && !self.has_exactly_one_player_passenger()
            && !self.is_age_locked()
            && !self.is_persistence_required()
    }

    /// Applies vanilla `TraderLlama.maybeDespawn`.
    fn maybe_despawn(&self) {
        if !self.can_despawn() {
            return;
        }

        let expired = {
            let mut delay = self.despawn_delay.lock();
            *delay -= 1;
            *delay <= 0
        };
        if expired {
            self.remove_leash();
            self.set_removed(RemovalReason::Discarded);
        }
    }
}

impl Entity for TraderLlamaEntity {
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
            TRADER_LLAMA_BABY_DIMENSIONS.scale(scale)
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
        nbt.insert("DespawnDelay", self.despawn_delay());
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.set_strength(nbt.int("Strength").unwrap_or(0));
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
        self.load_abstract_horse(nbt);
        self.load_chested_horse(nbt);
        self.set_llama_variant(LlamaVariant::by_id(nbt.int("Variant").unwrap_or(0)));
        self.set_despawn_delay(nbt.int("DespawnDelay").unwrap_or(DEFAULT_DESPAWN_DELAY));
    }
}

impl LivingEntity for TraderLlamaEntity {
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
        self.maybe_despawn();
        result
    }
}

impl AgeableMob for TraderLlamaEntity {
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

impl Animal for TraderLlamaEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        self.is_llama_food(item_stack)
    }

    fn can_mate(&self, partner: &dyn Animal) -> bool {
        if partner.uuid() == self.uuid() {
            return false;
        }
        let Some(partner_llama) = partner.as_llama() else {
            return false;
        };
        self.can_parent() && partner_llama.can_parent()
    }

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

impl AbstractHorse for TraderLlamaEntity {
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

    fn can_perform_rearing(&self) -> bool {
        false
    }

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

    fn follow_mommy(&self, world: &Arc<World>) {
        if should_follow_mommy(self) {
            AbstractHorse::follow_mommy_default(self, world);
        }
    }

    fn randomize_attributes(&self) {
        self.attributes().lock().set_base_value(
            vanilla_attributes::MAX_HEALTH,
            f64::from(generate_max_health(&mut |bound| {
                rand::random_range(0..bound)
            })),
        );
    }
}

impl AbstractChestedHorse for TraderLlamaEntity {
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

impl Llama for TraderLlamaEntity {
    fn llama_base(&self) -> &LlamaBase {
        &self.llama_base
    }

    fn is_trader_llama(&self) -> bool {
        true
    }

    fn synced_strength(&self) -> i32 {
        *self.entity_data.lock().llama().strength.get()
    }

    fn set_synced_strength(&self, strength: i32) {
        self.entity_data.lock().llama_mut().strength.set(strength);
    }

    fn synced_variant_id(&self) -> i32 {
        *self.entity_data.lock().llama().variant.get()
    }

    fn set_synced_variant_id(&self, variant_id: i32) {
        self.entity_data.lock().llama_mut().variant.set(variant_id);
    }

    /// Vanilla parity: `TraderLlama.makeNewLlama`, whose foal stays a trader
    /// llama and is marked persistent so it never counts down.
    fn make_new_llama(&self, world: &Arc<World>) -> Option<SharedEntity> {
        let offspring = ENTITIES.create(
            &vanilla_entities::TRADER_LLAMA,
            next_entity_id(),
            self.position(),
            Arc::downgrade(world),
        )?;
        offspring.as_mob()?.set_persistence_required();
        Some(offspring)
    }
}

impl Mob for TraderLlamaEntity {
    /// Vanilla parity: `AbstractHorse.getMaxSpawnClusterSize`.
    fn max_spawn_cluster_size(&self) -> i32 {
        6
    }

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

    /// Vanilla parity: `TraderLlama.finalizeSpawn`.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        if spawn_reason == EntitySpawnReason::Event {
            self.set_age(0);
        }

        // Vanilla substitutes an `AgeableMobGroupData(false)` here, but
        // `Llama.finalizeSpawn` immediately replaces anything that is not a
        // `LlamaGroupData`, so the substitution never survives.
        let group_data = group_data.unwrap_or(SpawnGroupData::AgeableMob(
            AgeableMobGroupData::with_should_spawn_baby(false),
        ));

        self.finalize_spawn_llama(world);
        let (variant, group_data) = if let SpawnGroupData::Llama(llama_group) = group_data {
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

impl PathfinderMob for TraderLlamaEntity {}
