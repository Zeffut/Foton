//! Zombie nautilus entity.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.nautilus.ZombieNautilus`.
//! The undead half of the family: it wears a coral variant picked from the
//! biome it spawns in, never grows from a baby and never breeds, moves a tenth
//! faster than the living one, and is still tameable and rideable.
//!
//! MISSING FOUNDATION: vanilla's `ZombieNautilus.sunProtectionSlot` moves the
//! slot `Mob.burnUndead` reads from the head to the body, so a zombie nautilus
//! in body armor does not burn in daylight. Foton implements no undead sun
//! burning at all -- there is no `burnUndead`, no `isSunBurnTick` and no
//! `sunProtectionSlot` hook anywhere in the crate -- so nothing burns and the
//! override has nothing to move. The same gap sits on the zombie horse.

use std::str::FromStr as _;
use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::item_stack::ItemStack;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_entity_data::ZombieNautilusEntityData;
use foton_registry::zombie_nautilus_variant::ZombieNautilusVariantRef;
use foton_registry::{REGISTRY, RegistryExt as _, RegistryReference, sound_events};
use foton_utils::Identifier;
use foton_utils::locks::SyncMutex;
use foton_utils::random::legacy_random::LegacyRandom;
use foton_utils::types::InteractionHand;
use foton_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use uuid::Uuid;

use crate::behavior::InteractionResult;
use crate::entity::ai::brain::Brain;
use crate::entity::ai::control::{SmoothSwimmingLookControl, SmoothSwimmingMoveControl};
use crate::entity::ai::path::PathType;
use crate::entity::damage::DamageSource;
use crate::entity::mob::NavigationKind;
use crate::entity::nautilus::{
    check_nautilus_spawn_rules, init_nautilus_memories, nautilus_can_be_affected,
};
use crate::entity::{
    AbstractNautilus, AbstractNautilusBase, AgeableMob, AgeableMobBase, Animal, AnimalBase, Entity,
    EntityBase, EntityBaseLoad, EntitySpawnReason, EntitySyncedData, LivingEntity,
    LivingEntityBase, LivingEntitySyncedData, Mob, MobBase, MobEffectInstance, MoveResult,
    PathfinderMob, SharedEntity, SpawnGroupData, TamableAnimal, TamableAnimalBase,
};
use crate::inventory::equipment::EquipmentSlot;
use crate::player::Player;
use crate::world::World;

use super::zombie_nautilus_ai;

/// Vanilla parity: the `SmoothSwimmingMoveControl(this, 85, 10, 0.011F, 0.0F, true)`
/// of the `AbstractNautilus` constructor.
const SWIM_MOVE_CONTROL: SmoothSwimmingMoveControl =
    SmoothSwimmingMoveControl::new(85, 10, 0.011, 0.0, true);
/// Vanilla parity: the `SmoothSwimmingLookControl(this, 10)`.
const SWIM_LOOK_CONTROL: SmoothSwimmingLookControl = SmoothSwimmingLookControl::new(10);

/// A zombie nautilus.
#[entity_behavior(class = "ZombieNautilus")]
pub struct ZombieNautilusEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    tamable_base: TamableAnimalBase,
    nautilus_base: AbstractNautilusBase,
    brain: Brain,
    entity_data: SyncMutex<ZombieNautilusEntityData>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `ZombieNautilusEntity`.
unsafe impl DowncastType for ZombieNautilusEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/zombie_nautilus");
}

impl ZombieNautilusEntity {
    /// Creates a zombie nautilus at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a zombie nautilus from saved base data.
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
        // Vanilla parity: the `setPathfindingMalus(PathType.WATER, 0.0F)` of the
        // `AbstractNautilus` constructor.
        mob_base
            .pathfinding_malus()
            .lock()
            .set(PathType::Water, 0.0);
        let mut entity_data = ZombieNautilusEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        let nautilus = Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            tamable_base: TamableAnimalBase::new(),
            nautilus_base: AbstractNautilusBase::new(),
            brain: zombie_nautilus_ai::make_brain(),
            entity_data: SyncMutex::new(entity_data),
        };
        // Vanilla parity: the `createInventory()` of the `AbstractNautilus`
        // constructor, which is what sizes the container to the column count.
        nautilus.create_nautilus_inventory();
        nautilus
    }

    /// Applies vanilla `ZombieNautilus.setVariant`.
    pub fn set_variant(&self, variant: ZombieNautilusVariantRef) {
        self.entity_data
            .lock()
            .variant
            .set(RegistryReference::new(variant));
    }

    /// Returns vanilla `ZombieNautilus.getVariant`, falling back to temperate
    /// when the stored reference no longer resolves.
    #[must_use]
    pub fn variant(&self) -> ZombieNautilusVariantRef {
        self.entity_data.lock().variant.get().value()
    }

    fn set_variant_by_key(&self, key: &Identifier) {
        if let Some(variant) = REGISTRY.zombie_nautilus_variants.by_key(key) {
            self.set_variant(variant);
        }
    }
}

impl Entity for ZombieNautilusEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `AbstractNautilus.tick`, whose `super.tick()` is the
    /// whole living tick -- not just `Entity.baseTick`.
    fn tick(&self) {
        LivingEntity::tick_living_entity(self);
        self.tick_nautilus();
    }

    /// Vanilla parity: `ZombieNautilus.getSwimSound`.
    fn swim_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_ZOMBIE_NAUTILUS_SWIM
    }

    /// Vanilla parity: `AbstractNautilus.isPushedByFluid`.
    fn is_pushed_by_fluid(&self) -> bool {
        false
    }

    /// Vanilla parity: `AbstractNautilus.playStepSound`, which is silent.
    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {}

    /// Vanilla parity: `AbstractNautilus.canAddPassenger`, a single seat.
    fn can_add_passenger(&self, _passenger: &dyn Entity) -> bool {
        !self.is_vehicle()
    }

    /// Vanilla parity: `AbstractNautilus.getControllingPassenger`.
    fn controlling_passenger(&self) -> Option<SharedEntity> {
        if Mob::is_saddled(self)
            && let Some(passenger) = self.first_passenger()
            && passenger.as_player().is_some()
        {
            return Some(passenger);
        }
        self.controlling_passenger_mob()
    }

    /// Vanilla parity: `AbstractNautilus.canJump`.
    fn can_jump_while_ridden(&self) -> bool {
        self.nautilus_can_jump_while_ridden()
    }

    /// Vanilla parity: `AbstractNautilus.handleStartJump`.
    fn handle_start_jump(&self, _jump_scale: i32) {
        self.nautilus_handle_start_jump();
    }

    /// Vanilla parity: `AbstractNautilus.openCustomInventoryScreen`.
    fn open_custom_inventory_screen(&self, player: &Player) {
        self.open_nautilus_inventory_screen(player);
    }

    /// Vanilla parity: `AbstractNautilus.interact`.
    fn interact(
        &self,
        player: &Player,
        hand: InteractionHand,
        location: DVec3,
    ) -> InteractionResult {
        self.set_persistence_required();
        self.interact_mob(player, hand, location)
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
        self.save_tamable_animal(nbt);
        nbt.insert("variant", self.variant().key.to_string());
        self.brain.save(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
        self.load_tamable_animal(nbt);
        if let Some(variant) = nbt.string("variant")
            && let Ok(key) = Identifier::from_str(variant.to_str().as_ref())
        {
            self.set_variant_by_key(&key);
        }
        self.brain.load(nbt);
    }
}

impl LivingEntity for ZombieNautilusEntity {
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

    /// Vanilla parity: `ZombieNautilus.getHurtSound`.
    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(if self.is_under_water() {
            &sound_events::ENTITY_ZOMBIE_NAUTILUS_HURT
        } else {
            &sound_events::ENTITY_ZOMBIE_NAUTILUS_HURT_LAND
        })
    }

    /// Vanilla parity: `ZombieNautilus.getDeathSound`.
    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(if self.is_under_water() {
            &sound_events::ENTITY_ZOMBIE_NAUTILUS_DEATH
        } else {
            &sound_events::ENTITY_ZOMBIE_NAUTILUS_DEATH_LAND
        })
    }

    /// Vanilla parity: `AbstractNautilus.canBeAffected`.
    fn can_be_affected(&self, effect: &MobEffectInstance) -> bool {
        nautilus_can_be_affected(effect) && self.default_can_be_affected(effect)
    }

    /// Vanilla parity: `AbstractNautilus.canUseSlot`.
    fn can_use_slot(&self, slot: EquipmentSlot) -> bool {
        self.nautilus_can_use_slot(slot)
    }

    /// Vanilla parity: `AbstractNautilus.canDispenserEquipIntoSlot`.
    fn can_dispenser_equip_into_slot(&self, slot: EquipmentSlot) -> bool {
        slot == EquipmentSlot::Body
            || slot == EquipmentSlot::Saddle
            || self.as_mob().is_none_or(Mob::can_pick_up_loot)
    }

    /// Vanilla parity: `AbstractNautilus.getEquipSound`.
    fn equip_sound(&self, slot: EquipmentSlot, stack: &ItemStack) -> Option<SoundEventRef> {
        self.nautilus_equip_sound(slot, stack)
    }

    /// Vanilla parity: `AbstractNautilus.hurtServer`.
    fn hurt_server(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        self.nautilus_hurt_server(world, source, amount)
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

    /// Vanilla parity: `AbstractNautilus.travelInWater`.
    fn travel_in_water(
        &self,
        input: DVec3,
        _base_gravity: f64,
        _is_falling: bool,
        _old_y: f64,
    ) -> Option<MoveResult> {
        self.nautilus_travel_in_water(input)
    }

    /// Vanilla parity: `AbstractNautilus.getRiddenInput`.
    fn ridden_input(&self, controller: &Player, _self_input: DVec3) -> DVec3 {
        self.nautilus_ridden_input(controller)
    }

    /// Vanilla parity: `AbstractNautilus.tickRidden`.
    fn tick_ridden(&self, controller: &Player, _ridden_input: DVec3) {
        self.nautilus_tick_ridden(controller);
    }

    /// Vanilla parity: `AbstractNautilus.getRiddenSpeed`.
    fn ridden_speed(&self, _controller: &Player) -> f32 {
        self.nautilus_ridden_speed()
    }
}

impl AgeableMob for ZombieNautilusEntity {
    fn ageable_base(&self) -> &AgeableMobBase {
        &self.ageable_base
    }

    /// Vanilla parity: `ZombieNautilus.canBeABaby`; there is no calf.
    fn can_be_a_baby(&self) -> bool {
        false
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

impl Animal for ZombieNautilusEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    /// Vanilla parity: `AbstractNautilus.isFood`.
    fn is_food(&self, item_stack: &ItemStack) -> bool {
        self.is_nautilus_food(item_stack)
    }

    /// Vanilla parity: `ZombieNautilus.playEatingSound`.
    fn play_eating_sound(&self) {
        self.make_sound(Some(&sound_events::ENTITY_ZOMBIE_NAUTILUS_EAT));
    }

    /// Vanilla parity: `ZombieNautilus.getBreedOffspring`, which returns null --
    /// a zombie nautilus never has young.
    fn get_breed_offspring(
        &self,
        _world: &Arc<World>,
        _partner: &dyn Animal,
    ) -> Option<SharedEntity> {
        None
    }
}

impl TamableAnimal for ZombieNautilusEntity {
    fn tamable_base(&self) -> &TamableAnimalBase {
        &self.tamable_base
    }

    fn tamable_flags(&self) -> i8 {
        *self.entity_data.lock().tamable_animal().flags.get()
    }

    fn set_tamable_flags(&self, flags: i8) {
        self.entity_data
            .lock()
            .tamable_animal_mut()
            .flags
            .set(flags);
    }

    fn owner_uuid(&self) -> Option<Uuid> {
        *self.entity_data.lock().tamable_animal().owneruuid.get()
    }

    fn set_owner_uuid(&self, owner: Option<Uuid>) {
        self.entity_data
            .lock()
            .tamable_animal_mut()
            .owneruuid
            .set(owner);
    }
}

impl AbstractNautilus for ZombieNautilusEntity {
    fn abstract_nautilus_base(&self) -> &AbstractNautilusBase {
        &self.nautilus_base
    }

    fn is_dashing(&self) -> bool {
        *self.entity_data.lock().abstract_nautilus().dash.get()
    }

    fn set_dash_flag(&self, is_dashing: bool) {
        self.entity_data
            .lock()
            .abstract_nautilus_mut()
            .dash
            .set(is_dashing);
    }

    /// Vanilla parity: `ZombieNautilus.getDashSound`.
    fn dash_sound(&self) -> Option<SoundEventRef> {
        Some(if self.is_under_water() {
            &sound_events::ENTITY_ZOMBIE_NAUTILUS_DASH
        } else {
            &sound_events::ENTITY_ZOMBIE_NAUTILUS_DASH_LAND
        })
    }

    /// Vanilla parity: `ZombieNautilus.getDashReadySound`.
    fn dash_ready_sound(&self) -> Option<SoundEventRef> {
        Some(if self.is_under_water() {
            &sound_events::ENTITY_ZOMBIE_NAUTILUS_DASH_READY
        } else {
            &sound_events::ENTITY_ZOMBIE_NAUTILUS_DASH_READY_LAND
        })
    }
}

impl Mob for ZombieNautilusEntity {
    /// Vanilla parity: `Mob.serverAiStep` ticks the goal selector for every
    /// mob it runs, brain-driven or not. `Mob::tick_goal_selectors` has an
    /// empty default, so leaving it out is how a registered goal set never
    /// runs.
    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn brain(&self) -> Option<&Brain> {
        Some(&self.brain)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    fn target(&self) -> Option<SharedEntity> {
        self.target_from_brain()
    }

    /// Vanilla parity: `ZombieNautilus.customServerAiStep`.
    fn custom_server_ai_step(&self) {
        let Some(world) = self.level() else {
            return;
        };
        self.brain.tick(&world, self);
        zombie_nautilus_ai::update_activity(&self.brain);
        self.check_nautilus_restriction();
        Animal::custom_server_ai_step_animal(self);
    }

    fn tick_move_control(&self) {
        SWIM_MOVE_CONTROL.tick(self);
    }

    fn tick_look_control(&self) {
        SWIM_LOOK_CONTROL.tick(self);
    }

    /// Vanilla parity: `ZombieNautilus.getAmbientSound`.
    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(if self.is_under_water() {
            &sound_events::ENTITY_ZOMBIE_NAUTILUS_AMBIENT
        } else {
            &sound_events::ENTITY_ZOMBIE_NAUTILUS_AMBIENT_LAND
        })
    }

    /// Vanilla parity: `ZombieNautilus.canBeLeashed`, which also refuses while
    /// something other than a player is riding it.
    fn can_be_leashed(&self) -> bool {
        !self.is_aggravated() && !self.is_nautilus_mob_controlled()
    }

    /// Vanilla parity: `AbstractNautilus.removeWhenFarAway`.
    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        true
    }

    /// Vanilla parity: `AbstractNautilus.requiresCustomPersistence`.
    fn requires_custom_persistence(&self) -> bool {
        self.nautilus_requires_custom_persistence()
    }

    /// Vanilla parity: `AbstractNautilus.mobInteract`.
    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        self.nautilus_mob_interact(player, hand)
    }

    /// Vanilla parity: `AbstractNautilus.usePlayerItem`.
    fn use_player_item(&self, player: &Player, hand: InteractionHand) {
        self.use_nautilus_player_item(player, hand);
    }

    /// Vanilla parity: `AbstractNautilus.checkNautilusSpawnRules`.
    ///
    /// Vanilla registers no `SpawnPlacements` entry for the zombie nautilus, so
    /// nothing natural reaches this; it answers the same rule the living one
    /// does for the paths that do check, like a spawn egg on water.
    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        let _ = spawn_reason;
        check_nautilus_spawn_rules(world, pos)
    }

    /// Vanilla parity: `ZombieNautilus.finalizeSpawn`, which picks a coral
    /// variant from the biome before the shared nautilus memories are seeded.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        let biome = world.biome_at(self.block_position());
        let variant = {
            let mut random = LegacyRandom::from_seed(rand::random());
            biome.and_then(|biome| {
                REGISTRY
                    .zombie_nautilus_variants
                    .select_spawn_variant(biome, &mut random)
            })
        };
        if let Some(variant) = variant {
            self.set_variant(variant);
        }

        init_nautilus_memories(self);
        self.finalize_spawn_ageable_mob(world, spawn_reason, group_data)
    }
}

impl PathfinderMob for ZombieNautilusEntity {
    /// Vanilla parity: `AbstractNautilus.createNavigation`.
    fn navigation_kind(&self) -> NavigationKind {
        NavigationKind::WaterBound {
            allow_breaching: false,
        }
    }

    /// Vanilla parity: `AbstractNautilus.getWalkTargetValue`.
    fn get_walk_target_value(&self, _pos: BlockPos) -> f32 {
        0.0
    }
}
