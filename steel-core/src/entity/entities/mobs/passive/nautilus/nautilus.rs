//! Nautilus entity.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.nautilus.Nautilus`. A
//! nautilus is the shared [`AbstractNautilus`] layer plus its sound table, its
//! five minutes of air, and a brain that courts and breeds -- the zombie one
//! does neither.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_registry::entity_data::EntityPose;
use steel_registry::entity_type::{
    EntityAttachmentPoint, EntityAttachments, EntityDimensions, EntityTypeRef,
};
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::NautilusEntityData;
use steel_registry::{sound_events, vanilla_damage_types};
use steel_utils::locks::SyncMutex;
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};
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

use super::nautilus_ai;

/// Vanilla parity: `Nautilus.NAUTILUS_TOTAL_AIR_SUPPLY`, fifteen seconds of air.
const NAUTILUS_TOTAL_AIR_SUPPLY: i32 = 300;
/// Vanilla parity: the `hurtServer(..., dryOut(), 2.0F)` of `handleAirSupply`.
const DRY_OUT_DAMAGE: f32 = 2.0;
/// Vanilla parity: the `airSupply <= -20` of `Nautilus.handleAirSupply`.
const DROWNING_AIR_SUPPLY: i32 = -20;
/// Vanilla parity: the `scale(0.5F)` of `Nautilus.BABY_DIMENSIONS`.
const BABY_SCALE: f32 = 0.5;
/// Vanilla parity: the `attach(PASSENGER, 0.0F, 0.5F, 0.0F)` of the same.
///
/// Without it a rider would sit at the calf's full height; vanilla's builder
/// replaces the default passenger point rather than scaling it.
static BABY_PASSENGER_ATTACHMENT: [EntityAttachmentPoint; 1] =
    [EntityAttachmentPoint::new(0.0, 0.5, 0.0)];

/// Vanilla parity: the `SmoothSwimmingMoveControl(this, 85, 10, 0.011F, 0.0F, true)`
/// of the `AbstractNautilus` constructor.
const SWIM_MOVE_CONTROL: SmoothSwimmingMoveControl =
    SmoothSwimmingMoveControl::new(85, 10, 0.011, 0.0, true);
/// Vanilla parity: the `SmoothSwimmingLookControl(this, 10)`.
const SWIM_LOOK_CONTROL: SmoothSwimmingLookControl = SmoothSwimmingLookControl::new(10);

/// A nautilus.
#[entity_behavior(class = "Nautilus")]
pub struct NautilusEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    tamable_base: TamableAnimalBase,
    nautilus_base: AbstractNautilusBase,
    brain: Brain,
    entity_data: SyncMutex<NautilusEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `NautilusEntity`.
unsafe impl DowncastType for NautilusEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/nautilus");
}

impl NautilusEntity {
    /// Creates a nautilus at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a nautilus from saved base data.
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
        let mut entity_data = NautilusEntityData::new();
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
            brain: nautilus_ai::make_brain(),
            entity_data: SyncMutex::new(entity_data),
        };
        // Vanilla parity: the `createInventory()` of the `AbstractNautilus`
        // constructor, which is what sizes the container to the column count.
        nautilus.create_nautilus_inventory();
        nautilus
    }

    /// Vanilla parity: `Nautilus.handleAirSupply`.
    ///
    /// A beached nautilus spends its fifteen seconds of air and then takes a
    /// heart a tick, which is what makes carrying one overland a race.
    fn handle_air_supply(&self, world: &Arc<World>, pre_tick_air_supply: i32) {
        if !Entity::is_alive(self) || self.is_in_water() {
            self.set_air_supply(NAUTILUS_TOTAL_AIR_SUPPLY);
            return;
        }

        self.set_air_supply(pre_tick_air_supply - 1);
        if self.air_supply() <= DROWNING_AIR_SUPPLY {
            self.set_air_supply(0);
            self.hurt_server(
                world,
                &DamageSource::environment(&vanilla_damage_types::DRY_OUT),
                DRY_OUT_DAMAGE,
            );
        }
    }
}

impl Entity for NautilusEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `Nautilus.getDefaultDimensions`, a half-size baby whose
    /// rider sits half a block up rather than at the calf's own height, and
    /// then the shared `LivingEntity.getDimensions` scale on top of it.
    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let default = if AgeableMob::is_baby(self) {
            let scaled = self.entity_type.dimensions.scale(BABY_SCALE);
            EntityDimensions::new_with_attachments(
                scaled.width,
                scaled.height,
                scaled.eye_height,
                EntityAttachments::new(&BABY_PASSENGER_ATTACHMENT, &[], &[], &[]),
            )
        } else {
            self.entity_type.dimensions
        };
        default.scale(self.get_scale())
    }

    /// Vanilla parity: `Nautilus.baseTick`, which reads the air left before the
    /// shared tick spends it and then runs its own drying-out clock.
    fn base_tick(&self) {
        let air_before_tick = self.air_supply();
        Mob::base_tick_mob(self);
        if self.is_no_ai() {
            return;
        }
        if let Some(world) = self.level() {
            self.handle_air_supply(&world, air_before_tick);
        }
    }

    /// Vanilla parity: `AbstractNautilus.tick`, whose `super.tick()` is the
    /// whole living tick -- not just `Entity.baseTick`.
    fn tick(&self) {
        LivingEntity::tick_living_entity(self);
        self.tick_nautilus();
    }

    /// Vanilla parity: `Nautilus.getMaxAirSupply`.
    fn max_air_supply(&self) -> i32 {
        NAUTILUS_TOTAL_AIR_SUPPLY
    }

    /// Vanilla parity: `Nautilus.getSwimSound`.
    fn swim_sound(&self) -> SoundEventRef {
        if AgeableMob::is_baby(self) {
            &sound_events::ENTITY_BABY_NAUTILUS_SWIM
        } else {
            &sound_events::ENTITY_NAUTILUS_SWIM
        }
    }

    /// Vanilla parity: `AbstractNautilus.isPushedByFluid`; a nautilus holds its
    /// line in a current.
    fn is_pushed_by_fluid(&self) -> bool {
        false
    }

    /// Vanilla parity: `AbstractNautilus.playStepSound`, which is silent.
    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {}

    /// Vanilla parity: `AbstractNautilus.canAddPassenger`, a single seat.
    fn can_add_passenger(&self, _passenger: &dyn Entity) -> bool {
        !self.is_vehicle()
    }

    /// Vanilla parity: `AbstractNautilus.getControllingPassenger`, which only
    /// hands the reins over once the nautilus is saddled.
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

    /// Vanilla parity: `AbstractNautilus.interact`, whose one addition is that
    /// a nautilus a player has touched never despawns.
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
        self.brain.save(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
        self.load_tamable_animal(nbt);
        self.brain.load(nbt);
    }
}

impl LivingEntity for NautilusEntity {
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

    /// Vanilla parity: `Nautilus.getHurtSound`.
    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(match (AgeableMob::is_baby(self), self.is_under_water()) {
            (true, true) => &sound_events::ENTITY_BABY_NAUTILUS_HURT,
            (true, false) => &sound_events::ENTITY_BABY_NAUTILUS_HURT_LAND,
            (false, true) => &sound_events::ENTITY_NAUTILUS_HURT,
            (false, false) => &sound_events::ENTITY_NAUTILUS_HURT_LAND,
        })
    }

    /// Vanilla parity: `Nautilus.getDeathSound`.
    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(match (AgeableMob::is_baby(self), self.is_under_water()) {
            (true, true) => &sound_events::ENTITY_BABY_NAUTILUS_DEATH,
            (true, false) => &sound_events::ENTITY_BABY_NAUTILUS_DEATH_LAND,
            (false, true) => &sound_events::ENTITY_NAUTILUS_DEATH,
            (false, false) => &sound_events::ENTITY_NAUTILUS_DEATH_LAND,
        })
    }

    /// Vanilla parity: `AbstractNautilus.canBeAffected`, which shrugs poison off.
    fn can_be_affected(&self, effect: &MobEffectInstance) -> bool {
        nautilus_can_be_affected(effect) && self.default_can_be_affected(effect)
    }

    /// Vanilla parity: `AbstractNautilus.canUseSlot`.
    fn can_use_slot(&self, slot: EquipmentSlot) -> bool {
        self.nautilus_can_use_slot(slot)
    }

    /// Vanilla parity: `AbstractNautilus.canDispenserEquipIntoSlot`, which lets
    /// a dispenser saddle and armor a nautilus that would not pick either up.
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

impl AgeableMob for NautilusEntity {
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

impl Animal for NautilusEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    /// Vanilla parity: `AbstractNautilus.isFood`.
    fn is_food(&self, item_stack: &ItemStack) -> bool {
        self.is_nautilus_food(item_stack)
    }

    /// Vanilla parity: `Nautilus.playEatingSound`.
    fn play_eating_sound(&self) {
        self.make_sound(Some(if AgeableMob::is_baby(self) {
            &sound_events::ENTITY_BABY_NAUTILUS_EAT
        } else {
            &sound_events::ENTITY_NAUTILUS_EAT
        }));
    }

    /// Vanilla parity: `Nautilus.getBreedOffspring`, where a calf born to a
    /// tame parent is already that player's.
    fn initialize_breed_offspring(&self, _partner: &dyn Animal, offspring: &dyn Animal) {
        use steel_utils::Downcast as _;

        if !self.is_tame() {
            return;
        }
        let Some(calf) = offspring.downcast_ref::<Self>() else {
            return;
        };
        calf.set_owner_uuid(self.owner_uuid());
        calf.set_tame(true, true);
    }
}

impl TamableAnimal for NautilusEntity {
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

impl AbstractNautilus for NautilusEntity {
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

    /// Vanilla parity: `Nautilus.getDashSound`.
    fn dash_sound(&self) -> Option<SoundEventRef> {
        Some(if self.is_under_water() {
            &sound_events::ENTITY_NAUTILUS_DASH
        } else {
            &sound_events::ENTITY_NAUTILUS_DASH_LAND
        })
    }

    /// Vanilla parity: `Nautilus.getDashReadySound`.
    fn dash_ready_sound(&self) -> Option<SoundEventRef> {
        Some(if self.is_under_water() {
            &sound_events::ENTITY_NAUTILUS_DASH_READY
        } else {
            &sound_events::ENTITY_NAUTILUS_DASH_READY_LAND
        })
    }
}

impl Mob for NautilusEntity {
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

    /// Vanilla parity: `Nautilus.getTarget` is `getTargetFromBrain`; a brain mob
    /// keeps no target field of its own.
    fn target(&self) -> Option<SharedEntity> {
        self.target_from_brain()
    }

    /// Vanilla parity: `Nautilus.customServerAiStep`, which ticks the brain,
    /// updates the activity, and then runs `AbstractNautilus.customServerAiStep`
    /// -- the home check that keeps a tame nautilus from wandering off.
    fn custom_server_ai_step(&self) {
        let Some(world) = self.level() else {
            return;
        };
        self.brain.tick(&world, self);
        nautilus_ai::update_activity(&self.brain);
        self.check_nautilus_restriction();
        Animal::custom_server_ai_step_animal(self);
    }

    fn tick_move_control(&self) {
        SWIM_MOVE_CONTROL.tick(self);
    }

    fn tick_look_control(&self) {
        SWIM_LOOK_CONTROL.tick(self);
    }

    /// Vanilla parity: `Nautilus.getAmbientSound`.
    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(match (AgeableMob::is_baby(self), self.is_under_water()) {
            (true, true) => &sound_events::ENTITY_BABY_NAUTILUS_AMBIENT,
            (true, false) => &sound_events::ENTITY_BABY_NAUTILUS_AMBIENT_LAND,
            (false, true) => &sound_events::ENTITY_NAUTILUS_AMBIENT,
            (false, false) => &sound_events::ENTITY_NAUTILUS_AMBIENT_LAND,
        })
    }

    /// Vanilla parity: `Nautilus.canBeLeashed`, which refuses while it is angry.
    fn can_be_leashed(&self) -> bool {
        !self.is_aggravated()
    }

    /// Vanilla parity: `AbstractNautilus.removeWhenFarAway`, an unconditional yes.
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
    /// MISSING FOUNDATION: vanilla's `AbstractNautilus.checkSpawnObstruction`
    /// narrows the shared one to `isUnobstructed` alone, dropping the "no liquid
    /// in the bounding box" half -- otherwise a nautilus could never spawn,
    /// because it only ever spawns in water. Steel has no
    /// `checkSpawnObstruction` hook on `Mob` at all, so nothing applies that
    /// half either and the narrowing has nothing to narrow. The same gap is
    /// already recorded on the axolotl, the guardian and the ocelot.
    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        let _ = spawn_reason;
        check_nautilus_spawn_rules(world, pos)
    }

    /// Vanilla parity: `AbstractNautilus.finalizeSpawn`, which seeds the long
    /// cooldown that keeps a fresh nautilus from picking a fight immediately
    /// and then hands over to the shared ageable spawn, whose one-in-five roll
    /// is where calves come from.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        init_nautilus_memories(self);
        self.finalize_spawn_ageable_mob(world, spawn_reason, group_data)
    }
}

impl PathfinderMob for NautilusEntity {
    /// Vanilla parity: `AbstractNautilus.createNavigation` returns a
    /// `WaterBoundPathNavigation`, which does not breach.
    fn navigation_kind(&self) -> NavigationKind {
        NavigationKind::WaterBound {
            allow_breaching: false,
        }
    }

    /// Vanilla parity: `AbstractNautilus.getWalkTargetValue`, a flat zero -- a
    /// nautilus has no preferred block to stand on.
    fn get_walk_target_value(&self, _pos: BlockPos) -> f32 {
        0.0
    }
}
