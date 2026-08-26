//! Shared vanilla `AbstractNautilus` state and hooks.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.nautilus.AbstractNautilus`.
//! A nautilus is the game's only tameable mount that lives underwater: it is
//! tamed with food, saddled, ridden with the mouse rather than with a walk
//! input, dashes when the rider charges a jump, and keeps its rider breathing
//! while they sit on it. Both the nautilus and the zombie nautilus are this
//! class plus a sound table, so everything here is shared.

use std::fmt;
use std::sync::Arc;

use glam::DVec3;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_attributes, vanilla_blocks,
    vanilla_fluid_tags, vanilla_game_events, vanilla_items, vanilla_mob_effects,
};
use steel_utils::BlockPos;
use steel_utils::entity_events::EntityStatus;
use steel_utils::locks::{IntoShared as _, Shared, SyncMutex};
use steel_utils::types::InteractionHand;
use steel_utils::value_providers::UniformIntProvider;

use crate::behavior::InteractionResult;
use crate::entity::ai::brain::behavior::utils::block_closer_than;
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::ai::brain::sensor::is_entity_attackable_ignoring_line_of_sight;
use crate::entity::damage::DamageSource;
use crate::entity::{
    AgeableMob, Animal, Entity, LivingEntity, Mob, MobEffectInstance, MoveResult, SharedEntity,
    TamableAnimal,
};
use crate::inventory::container::{Container as _, SimpleContainer};
use crate::inventory::equipment::EquipmentSlot;
use crate::inventory::menu::kinds::{nautilus_inventory, open_mount_screen};
use crate::physics::MoverType;
use crate::player::Player;
use crate::player::movement::wrap_degrees;
use crate::world::game_event::GameEventContext;
use crate::world::{LevelReader as _, World};

/// Rows the mount screen lays the nautilus inventory out in.
///
/// Vanilla parity: `AbstractNautilus.INVENTORY_ROWS`.
pub const INVENTORY_ROWS: usize = 3;

/// The leash a tame nautilus keeps to its home while it is saddled or a baby.
///
/// Vanilla parity: `AbstractNautilus.SMALL_RESTRICTION_RADIUS`.
const SMALL_RESTRICTION_RADIUS: i32 = 16;
/// The leash a tame adult nautilus with no saddle keeps to its home.
///
/// Vanilla parity: `AbstractNautilus.LARGE_RESTRICTION_RADIUS`.
const LARGE_RESTRICTION_RADIUS: i32 = 32;
/// How far past its radius a nautilus may drift before its home is moved.
///
/// Vanilla parity: `AbstractNautilus.RESTRICTION_RADIUS_BUFFER`.
const RESTRICTION_RADIUS_BUFFER: i32 = 8;

/// How long a rider's breath of the nautilus lasts.
///
/// Vanilla parity: `AbstractNautilus.EFFECT_DURATION`.
const EFFECT_DURATION: i32 = 60;
/// How often the rider's effect is topped back up.
///
/// Vanilla parity: `AbstractNautilus.EFFECT_REFRESH_RATE`.
const EFFECT_REFRESH_RATE: i64 = 40;

/// What a nautilus keeps of its speed each tick under water.
///
/// Vanilla parity: `AbstractNautilus.NAUTILUS_WATER_RESISTANCE`.
const NAUTILUS_WATER_RESISTANCE: f64 = 0.9;
/// The ridden speed multiplier while the nautilus is in water.
///
/// Vanilla parity: `AbstractNautilus.RIDDEN_SPEED_MODIFIER_IN_WATER`.
const RIDDEN_SPEED_MODIFIER_IN_WATER: f32 = 0.0325;
/// The ridden speed multiplier while the nautilus is beached.
///
/// Vanilla parity: `AbstractNautilus.RIDDEN_SPEED_MODIFIER_ON_LAND`.
const RIDDEN_SPEED_MODIFIER_ON_LAND: f32 = 0.02;

/// How long a nautilus waits between dashes.
///
/// Vanilla parity: `AbstractNautilus.DASH_COOLDOWN_TICKS`.
const DASH_COOLDOWN_TICKS: i32 = 40;
/// How long the dash flag stays up once a dash starts.
///
/// Vanilla parity: the `dashCooldown < 35` of `AbstractNautilus.tick`, which is
/// `DASH_COOLDOWN_TICKS - DASH_MINIMUM_DURATION_TICKS`.
const DASH_FLAG_CLEAR_AT: i32 = DASH_COOLDOWN_TICKS - DASH_MINIMUM_DURATION_TICKS;
/// The shortest a dash can look, whatever else happens.
///
/// Vanilla parity: `AbstractNautilus.DASH_MINIMUM_DURATION_TICKS`.
const DASH_MINIMUM_DURATION_TICKS: i32 = 5;
/// How hard a dash pushes while the nautilus is swimming.
///
/// Vanilla parity: `AbstractNautilus.DASH_MOMENTUM_IN_WATER`.
const DASH_MOMENTUM_IN_WATER: f32 = 1.2;
/// How hard a dash pushes while the nautilus is beached.
///
/// Vanilla parity: `AbstractNautilus.DASH_MOMENTUM_ON_LAND`.
const DASH_MOMENTUM_ON_LAND: f32 = 0.5;

/// How much a rider's pitch turns the nautilus.
///
/// Vanilla parity: the `controller.getXRot() * 0.5F` of
/// `AbstractNautilus.getRiddenRotation`.
const RIDDEN_PITCH_FACTOR: f32 = 0.5;
/// How fast the nautilus swings round to the rider's yaw.
///
/// Vanilla parity: the `turnSpeed` of `AbstractNautilus.tickRidden`.
const RIDDEN_TURN_SPEED: f32 = 0.5;
/// What backing up costs a rider's look direction.
///
/// Vanilla parity: the `*= -0.5F` of `AbstractNautilus.getRiddenInput`.
const RIDDEN_BACKWARDS_FACTOR: f32 = -0.5;

/// The chance in three that a feeding tames a nautilus.
///
/// Vanilla parity: the `random.nextInt(3) == 0` of `AbstractNautilus.tryToTame`.
const TAME_CHANCE_IN: i32 = 3;
/// How much of a food's nutrition a nautilus heals for.
///
/// Vanilla parity: the `2.0F` of the `feed` call in `AbstractNautilus.mobInteract`.
const FEED_HEALING_FACTOR: f32 = 2.0;
/// What a nautilus heals for eating something with no food component.
///
/// Vanilla parity: the `1.0F` of the same call.
const FEED_DEFAULT_HEAL: f32 = 1.0;

/// Runtime fields vanilla keeps on `AbstractNautilus` itself.
#[derive(Debug, Clone, Copy, PartialEq)]
struct AbstractNautilusState {
    /// Vanilla parity: `AbstractNautilus.dashCooldown`.
    dash_cooldown: i32,
    /// Vanilla parity: `AbstractNautilus.playerJumpPendingScale`.
    player_jump_pending_scale: f32,
}

impl AbstractNautilusState {
    const fn new() -> Self {
        Self {
            dash_cooldown: 0,
            player_jump_pending_scale: 0.0,
        }
    }
}

/// Shared vanilla `AbstractNautilus` runtime state.
pub struct AbstractNautilusBase {
    state: SyncMutex<AbstractNautilusState>,
    /// Vanilla parity: `AbstractNautilus.inventory`. Held behind its own handle
    /// so an open mount screen keeps working across a resize, and so
    /// `hasInventoryChanged` can compare identity afterwards.
    inventory: SyncMutex<Shared<SimpleContainer>>,
}

impl fmt::Debug for AbstractNautilusBase {
    /// `SimpleContainer` is not `Debug`, so the inventory is summarized by size.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AbstractNautilusBase")
            .field("state", &*self.state.lock())
            .field(
                "inventory_size",
                &self.inventory.lock().lock().items().len(),
            )
            .finish()
    }
}

impl Default for AbstractNautilusBase {
    fn default() -> Self {
        Self::new()
    }
}

impl AbstractNautilusBase {
    /// Creates nautilus runtime state with an empty inventory.
    ///
    /// The real size arrives from [`AbstractNautilus::create_nautilus_inventory`],
    /// which the constructor calls the way vanilla's does -- the column count is
    /// the mob's to answer, and the mob does not exist yet here.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: SyncMutex::new(AbstractNautilusState::new()),
            inventory: SyncMutex::new(SimpleContainer::new(0).into_shared()),
        }
    }

    /// Returns the current inventory handle.
    #[must_use]
    pub fn inventory(&self) -> Shared<SimpleContainer> {
        Arc::clone(&self.inventory.lock())
    }

    /// Replaces the inventory with one of `size` slots, carrying items across.
    ///
    /// Vanilla parity: `AbstractNautilus.createInventory`.
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

    /// Returns vanilla `AbstractNautilus.dashCooldown`.
    #[must_use]
    pub fn dash_cooldown(&self) -> i32 {
        self.state.lock().dash_cooldown
    }

    fn set_dash_cooldown(&self, dash_cooldown: i32) {
        self.state.lock().dash_cooldown = dash_cooldown;
    }

    fn take_player_jump_pending_scale(&self) -> f32 {
        let mut state = self.state.lock();
        let pending_scale = state.player_jump_pending_scale;
        state.player_jump_pending_scale = 0.0;
        pending_scale
    }

    fn set_player_jump_pending_scale(&self, scale: f32) {
        self.state.lock().player_jump_pending_scale = scale;
    }
}

/// Vanilla-shaped behavior shared by entities that extend `AbstractNautilus`.
pub trait AbstractNautilus: TamableAnimal {
    /// Returns shared nautilus runtime state.
    fn abstract_nautilus_base(&self) -> &AbstractNautilusBase;

    /// Returns the synchronized `AbstractNautilus.DASH` flag.
    fn is_dashing(&self) -> bool;

    /// Writes the synchronized `AbstractNautilus.DASH` flag.
    ///
    /// Implementations only touch the synced value; the cooldown side effect is
    /// [`Self::set_dashing`]'s job.
    fn set_dash_flag(&self, is_dashing: bool);

    /// Vanilla parity: `Nautilus.getDashSound` / `ZombieNautilus.getDashSound`.
    fn dash_sound(&self) -> Option<SoundEventRef> {
        None
    }

    /// Vanilla parity: `Nautilus.getDashReadySound` / `ZombieNautilus.getDashReadySound`.
    fn dash_ready_sound(&self) -> Option<SoundEventRef> {
        None
    }

    /// Applies vanilla `AbstractNautilus.setDashing`.
    ///
    /// Vanilla's `onSyncedDataUpdated` arms the cooldown the first time `DASH`
    /// changes with the clock at zero, which is the only thing that starts the
    /// server's cooldown when a rider's `handleStartJump` raises the flag.
    /// Steel has no synced-data change hook, so the same rule lives here: every
    /// vanilla path that writes `DASH` goes through `setDashing`, so the two are
    /// equivalent.
    fn set_dashing(&self, is_dashing: bool) {
        self.set_dash_flag(is_dashing);
        let base = self.abstract_nautilus_base();
        if base.dash_cooldown() == 0 {
            base.set_dash_cooldown(DASH_COOLDOWN_TICKS);
        }
    }

    /// Returns vanilla `AbstractNautilus.isFood`.
    ///
    /// An untamed adult only takes its taming items; everything else it eats is
    /// the food tag.
    fn is_nautilus_food(&self, item_stack: &ItemStack) -> bool {
        let tag = if !self.is_tame() && !AgeableMob::is_baby(self) {
            &ItemTag::NAUTILUS_TAMING_ITEMS
        } else {
            &ItemTag::NAUTILUS_FOOD
        };
        REGISTRY.items.is_in_tag(item_stack.item(), tag)
    }

    /// Applies vanilla `AbstractNautilus.usePlayerItem`, which hands back the
    /// water bucket a bucket of fish leaves behind.
    ///
    /// Rust has no `super`, and this is what `Mob::use_player_item` is
    /// overridden with, so the `else` branch spells `Mob.usePlayerItem` out
    /// rather than calling it -- calling it would come straight back here.
    fn use_nautilus_player_item(&self, player: &Player, hand: InteractionHand) {
        let is_bucket_food = {
            let inventory = player.inventory.lock();
            REGISTRY.items.is_in_tag(
                inventory.get_item_in_hand(hand).item(),
                &ItemTag::NAUTILUS_BUCKET_FOOD,
            )
        };
        if !is_bucket_food {
            player.inventory.lock().shrink_item_in_hand(hand, 1);
            return;
        }

        let overflow = {
            let mut inventory = player.inventory.lock();
            inventory.apply_filled_result(
                hand,
                ItemStack::new(&vanilla_items::WATER_BUCKET),
                player.has_infinite_materials(),
                true,
            )
        };
        if !overflow.is_empty() {
            let _ = player.drop_item(overflow, false, false);
        }
    }

    /// Returns vanilla `AbstractNautilus.canUseSlot`.
    ///
    /// Only a live, grown, tame nautilus wears a saddle or body armor.
    fn nautilus_can_use_slot(&self, slot: EquipmentSlot) -> bool {
        if slot != EquipmentSlot::Saddle && slot != EquipmentSlot::Body {
            return true;
        }
        Entity::is_alive(self) && !AgeableMob::is_baby(self) && self.is_tame()
    }

    /// Returns vanilla `AbstractNautilus.getRiddenInput`.
    ///
    /// Unlike every land mount, forward is the rider's *look* direction, so a
    /// nautilus climbs and dives with the mouse.
    fn nautilus_ridden_input(&self, controller: &Player) -> DVec3 {
        let input = controller.travel_input();
        let strafe = input.sideways();
        if input.forward() == 0.0 {
            return DVec3::new(f64::from(strafe), 0.0, 0.0);
        }

        let pitch_radians = controller.rotation().1.to_radians();
        let mut forward_look = pitch_radians.cos();
        let mut up_look = -pitch_radians.sin();
        if input.forward() < 0.0 {
            forward_look *= RIDDEN_BACKWARDS_FACTOR;
            up_look *= RIDDEN_BACKWARDS_FACTOR;
        }

        DVec3::new(
            f64::from(strafe),
            f64::from(up_look),
            f64::from(forward_look),
        )
    }

    /// Applies vanilla `AbstractNautilus.tickRidden`.
    fn nautilus_tick_ridden(&self, controller: &Player) {
        let (controller_yaw, controller_pitch) = controller.rotation();
        let mut yaw = self.rotation().0;
        let difference = wrap_degrees(controller_yaw - yaw);
        yaw += difference * RIDDEN_TURN_SPEED;
        self.set_rotation((yaw, controller_pitch * RIDDEN_PITCH_FACTOR));
        self.base().set_old_yaw_to_current();
        self.set_y_body_rot(yaw);
        self.set_y_head_rot(yaw);

        if !self.is_server_driven_movement() {
            return;
        }

        let pending_scale = self
            .abstract_nautilus_base()
            .take_player_jump_pending_scale();
        if pending_scale > 0.0 && !self.is_jumping() {
            self.execute_riders_jump(pending_scale, controller);
        }
    }

    /// Returns vanilla `AbstractNautilus.getRiddenSpeed`.
    fn nautilus_ridden_speed(&self) -> f32 {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a movement-speed attribute, immediately used as a speed"
        )]
        let movement_speed = self
            .attributes()
            .lock()
            .required_value(vanilla_attributes::MOVEMENT_SPEED) as f32;
        if self.is_in_water() {
            RIDDEN_SPEED_MODIFIER_IN_WATER * movement_speed
        } else {
            RIDDEN_SPEED_MODIFIER_ON_LAND * movement_speed
        }
    }

    /// Applies vanilla `AbstractNautilus.travelInWater`, which swims rather
    /// than sinking and bleeds a tenth of its speed a tick.
    fn nautilus_travel_in_water(&self, input: DVec3) -> Option<MoveResult> {
        self.move_relative(self.get_speed(), input);
        let result = self.move_entity(MoverType::SelfMovement, self.velocity());
        self.set_velocity(self.velocity() * NAUTILUS_WATER_RESISTANCE);
        result
    }

    /// Applies vanilla `AbstractNautilus.doPlayerRide`.
    fn nautilus_do_player_ride(&self, player: &Player) {
        let Some(world) = self.level() else {
            return;
        };
        let Some(vehicle) = world.get_entity_by_id(self.id()) else {
            return;
        };
        player.start_riding(&vehicle);
        if !self.is_vehicle() {
            self.clear_home();
        }
    }

    /// Returns vanilla `AbstractNautilus.getNautilusRestrictionRadius`.
    fn nautilus_restriction_radius(&self) -> i32 {
        if !AgeableMob::is_baby(self) && !self.has_item_in_slot(EquipmentSlot::Saddle) {
            LARGE_RESTRICTION_RADIUS
        } else {
            SMALL_RESTRICTION_RADIUS
        }
    }

    /// Applies vanilla `AbstractNautilus.checkRestriction`, which is what keeps
    /// a tame nautilus near where its owner left it.
    fn check_nautilus_restriction(&self) {
        if self.is_leashed() || self.is_vehicle() || !self.is_tame() {
            return;
        }

        let radius = self.nautilus_restriction_radius();
        let position = self.block_position();
        let keeps_home = self.has_home()
            && block_closer_than(
                self.home_position(),
                position,
                f64::from(radius + RESTRICTION_RADIUS_BUFFER),
            )
            && radius == self.home_radius();
        if !keeps_home {
            self.set_home_to(position, radius);
        }
    }

    /// Applies vanilla `AbstractNautilus.applyEffects`, the rider's air supply.
    fn apply_nautilus_effects(&self, world: &Arc<World>) {
        let Some(passenger) = self.first_passenger() else {
            return;
        };
        let Some(player) = passenger.as_player() else {
            return;
        };

        let has_effect = player.has_mob_effect(vanilla_mob_effects::BREATH_OF_THE_NAUTILUS);
        let should_refresh = world.game_time() % EFFECT_REFRESH_RATE == 0;
        if has_effect && !should_refresh {
            return;
        }

        player.add_mob_effect(
            MobEffectInstance::with_duration(
                vanilla_mob_effects::BREATH_OF_THE_NAUTILUS,
                EFFECT_DURATION,
                0,
            )
            .with_ambient(true),
        );
    }

    /// Applies vanilla `AbstractNautilus.tick`.
    ///
    /// Vanilla also spawns the bubble trail here; `Level.addParticle` is
    /// client-local, so the server has nothing to send.
    fn tick_nautilus(&self) {
        if let Some(world) = self.level() {
            self.apply_nautilus_effects(&world);
        }

        let base = self.abstract_nautilus_base();
        if self.is_dashing() && base.dash_cooldown() < DASH_FLAG_CLEAR_AT {
            self.set_dashing(false);
        }

        let cooldown = base.dash_cooldown();
        if cooldown > 0 {
            base.set_dash_cooldown(cooldown - 1);
            if cooldown - 1 == 0 {
                self.make_sound(self.dash_ready_sound());
            }
        }
    }

    /// Returns vanilla `AbstractNautilus.canJump`.
    fn nautilus_can_jump_while_ridden(&self) -> bool {
        Mob::is_saddled(self)
    }

    /// Applies vanilla `AbstractNautilus.onPlayerJump`.
    ///
    /// Vanilla only reaches this from `LocalPlayer`, so it never fires on a
    /// dedicated server; it is here because a locally simulated nautilus still
    /// needs the pending scale [`Self::execute_riders_jump`] consumes.
    fn nautilus_on_player_jump(&self, jump_amount: i32) {
        if !Mob::is_saddled(self) || self.abstract_nautilus_base().dash_cooldown() > 0 {
            return;
        }
        self.abstract_nautilus_base()
            .set_player_jump_pending_scale(Entity::player_jump_pending_scale(self, jump_amount));
    }

    /// Applies vanilla `AbstractNautilus.executeRidersJump`.
    fn execute_riders_jump(&self, amount: f32, controller: &Player) {
        let momentum = if self.is_in_water() {
            DASH_MOMENTUM_IN_WATER
        } else {
            DASH_MOMENTUM_ON_LAND
        };
        let scale = f64::from(momentum * amount)
            * self
                .attributes()
                .lock()
                .required_value(vanilla_attributes::MOVEMENT_SPEED)
            * f64::from(self.block_speed_factor());
        self.set_velocity(self.velocity() + controller.look_angle() * scale);
        self.mark_velocity_sync();
        self.abstract_nautilus_base()
            .set_dash_cooldown(DASH_COOLDOWN_TICKS);
        self.set_dashing(true);
    }

    /// Applies vanilla `AbstractNautilus.handleStartJump`.
    fn nautilus_handle_start_jump(&self) {
        self.make_sound(self.dash_sound());
        if let Some(world) = self.level() {
            world.game_event(
                &vanilla_game_events::ENTITY_ACTION,
                self.block_position(),
                &GameEventContext::new(Some(self.as_entity_event_source()), None),
            );
        }
        self.set_dashing(true);
    }

    /// Returns vanilla `AbstractNautilus.getEquipSound`, whose saddle is
    /// quieter under water.
    fn nautilus_equip_sound(
        &self,
        slot: EquipmentSlot,
        stack: &ItemStack,
    ) -> Option<SoundEventRef> {
        if slot != EquipmentSlot::Saddle {
            return self.default_equip_sound(slot, stack);
        }
        Some(if self.is_under_water() {
            &sound_events::ITEM_NAUTILUS_SADDLE_UNDERWATER_EQUIP
        } else {
            &sound_events::ITEM_NAUTILUS_SADDLE_EQUIP
        })
    }

    /// Applies vanilla `AbstractNautilus.mobInteract`.
    fn nautilus_mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        if AgeableMob::is_baby(self) {
            return Animal::mob_interact_animal(self, player, hand);
        }

        if self.is_tame() && player.is_secondary_use_active() {
            self.open_custom_inventory_screen(player);
            return InteractionResult::Success;
        }

        let item_stack = {
            let inventory = player.inventory.lock();
            let item_stack = inventory.get_item_in_hand(hand);
            item_stack.copy_with_count(item_stack.count())
        };
        let is_food = self.is_nautilus_food(&item_stack);
        if !item_stack.is_empty() {
            if !self.is_tame() && is_food {
                self.use_nautilus_player_item(player, hand);
                self.try_to_tame_nautilus(player);
                return InteractionResult::SuccessServer;
            }

            if is_food && self.get_health() < self.get_max_health() {
                self.feed(
                    player,
                    hand,
                    &item_stack,
                    FEED_HEALING_FACTOR,
                    FEED_DEFAULT_HEAL,
                );
                return InteractionResult::Success;
            }

            let interaction_result =
                LivingEntity::interact_living_entity_with_equippable(self, player, hand);
            if interaction_result.consumes_action() {
                return interaction_result;
            }
        }

        if self.is_tame() && !player.is_secondary_use_active() && !is_food {
            self.nautilus_do_player_ride(player);
            return InteractionResult::Success;
        }

        Animal::mob_interact_animal(self, player, hand)
    }

    /// Applies vanilla `AbstractNautilus.tryToTame`, a one-in-three roll.
    fn try_to_tame_nautilus(&self, player: &Player) {
        if rand::random_range(0..TAME_CHANCE_IN) == 0 {
            self.tame(player);
            self.mob_base().navigation().lock().stop();
            self.broadcast_entity_event(EntityStatus::TamingSucceeded);
        } else {
            self.broadcast_entity_event(EntityStatus::TamingFailed);
        }

        self.play_eating_sound();
    }

    /// Returns vanilla `AbstractNautilus.isMobControlled`.
    fn is_nautilus_mob_controlled(&self) -> bool {
        self.first_passenger()
            .is_some_and(|passenger| passenger.as_mob().is_some())
    }

    /// Returns vanilla `AbstractNautilus.isAggravated`.
    fn is_aggravated(&self) -> bool {
        let Some(brain) = Mob::brain(self) else {
            return false;
        };
        brain.has_memory_value(memory_module_types::ANGRY_AT.id())
            || brain.has_memory_value(memory_module_types::ATTACK_TARGET.id())
    }

    /// Returns vanilla `AbstractNautilus.getInventoryColumns`.
    ///
    /// Zero in 26.2: neither nautilus carries cargo, so the mount screen's grid
    /// is empty and only the saddle and armor slots are live.
    fn nautilus_inventory_columns(&self) -> usize {
        0
    }

    /// Returns vanilla `AbstractNautilus.getInventorySize`.
    fn nautilus_inventory_size(&self) -> usize {
        self.nautilus_inventory_columns() * INVENTORY_ROWS
    }

    /// Applies vanilla `AbstractNautilus.createInventory`.
    fn create_nautilus_inventory(&self) {
        self.abstract_nautilus_base()
            .create_inventory(self.nautilus_inventory_size());
    }

    /// Applies vanilla `AbstractNautilus.openCustomInventoryScreen`.
    ///
    /// In 26.2 `getInventoryColumns` is zero for both nautiluses, so the cargo
    /// grid of the screen is empty and only the saddle and armor slots are
    /// live. The zero still travels: the client sizes the screen from it.
    fn open_nautilus_inventory_screen(&self, player: &Player) {
        if (self.is_vehicle() && !self.has_passenger(player)) || !self.is_tame() {
            return;
        }
        let Some(world) = self.level() else {
            return;
        };
        let Some(mount) = world.get_entity_by_id(Entity::id(self)) else {
            return;
        };
        open_mount_screen(
            &mount,
            self.abstract_nautilus_base().inventory(),
            self.nautilus_inventory_columns(),
            nautilus_inventory,
            player,
        );
    }

    /// Applies vanilla `AbstractNautilus.hurtServer`, whose only addition is
    /// remembering who did it.
    fn nautilus_hurt_server(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        let was_hurt = self.living_hurt_server(world, source, amount);
        if !was_hurt {
            return false;
        }

        let Some(attacker) = source
            .causing_entity_id
            .and_then(|id| world.get_entity_by_id(id))
        else {
            return true;
        };
        if attacker.as_living_entity().is_some() {
            set_anger_target(world, self, &attacker);
        }
        true
    }

    /// Returns vanilla `AbstractNautilus.requiresCustomPersistence`.
    fn nautilus_requires_custom_persistence(&self) -> bool {
        self.is_passenger() || self.is_leashed() || self.is_tame()
    }
}

/// Applies vanilla `NautilusAi.setAngerTarget`.
///
/// Lives beside the mob rather than in its brain module because both nautilus
/// brains share it and `AbstractNautilus.hurtServer` is its only caller.
pub fn set_anger_target<B: AbstractNautilus + ?Sized>(
    world: &World,
    body: &B,
    target: &SharedEntity,
) {
    let Some(target_living) = target.as_living_entity() else {
        return;
    };
    let Some(brain) = Mob::brain(body) else {
        return;
    };
    let Some(body_living) = body.as_entity_event_source().as_living_entity() else {
        return;
    };
    if !is_entity_attackable_ignoring_line_of_sight(world, body_living, target_living) {
        return;
    }

    brain.erase_memory(memory_module_types::CANT_REACH_WALK_TARGET_SINCE.id());
    brain.set_memory_with_expiry(
        memory_module_types::ANGRY_AT,
        target.uuid(),
        ANGER_DURATION_TICKS,
    );
}

/// How long a nautilus stays angry at whatever hurt it.
///
/// Vanilla parity: the `400L` expiry of `NautilusAi.setAngerTarget`.
const ANGER_DURATION_TICKS: i64 = 400;

/// Returns vanilla `AbstractNautilus.checkNautilusSpawnRules`.
///
/// A nautilus spawns in the twenty blocks of water just under the surface, with
/// water below it and water above it.
#[must_use]
pub fn check_nautilus_spawn_rules(world: &Arc<World>, pos: BlockPos) -> bool {
    let sea_level = world.sea_level;
    let min_spawn_level = sea_level - SPAWN_DEPTH_BELOW_SEA_LEVEL;
    pos.y() >= min_spawn_level
        && pos.y() <= sea_level - SPAWN_MIN_DEPTH
        && world
            .get_block_state(pos.below())
            .get_fluid_state()
            .fluid_id
            .has_tag(&vanilla_fluid_tags::FluidTag::WATER)
        && world.get_block_state(pos.above()).get_block() == &vanilla_blocks::WATER
}

/// The deepest a nautilus spawns below sea level.
///
/// Vanilla parity: the `seaLevel - 25` of `checkNautilusSpawnRules`.
const SPAWN_DEPTH_BELOW_SEA_LEVEL: i32 = 25;
/// The shallowest a nautilus spawns below sea level.
///
/// Vanilla parity: the `seaLevel - 5` of the same check.
const SPAWN_MIN_DEPTH: i32 = 5;

/// Whether a nautilus may take `effect`.
///
/// Vanilla parity: `AbstractNautilus.canBeAffected`, which is immune to poison
/// and nothing else.
#[must_use]
pub fn nautilus_can_be_affected(effect: &MobEffectInstance) -> bool {
    effect.effect() != vanilla_mob_effects::POISON
}

/// Applies vanilla `NautilusAi.initMemories`.
pub fn init_nautilus_memories<B: AbstractNautilus + ?Sized>(body: &B) {
    let Some(brain) = Mob::brain(body) else {
        return;
    };
    brain.set_memory(
        memory_module_types::ATTACK_TARGET_COOLDOWN,
        sample_time_between_non_player_attacks(),
    );
}

/// How long a nautilus waits between fights nobody started.
///
/// Vanilla parity: `NautilusAi.TIME_BETWEEN_NON_PLAYER_ATTACKS`.
const TIME_BETWEEN_NON_PLAYER_ATTACKS: UniformIntProvider = UniformIntProvider {
    min_inclusive: 2400,
    max_inclusive: 3600,
};

/// Rolls the pause a nautilus takes before it looks for a fight of its own.
#[must_use]
pub fn sample_time_between_non_player_attacks() -> i32 {
    rand::random_range(
        TIME_BETWEEN_NON_PLAYER_ATTACKS.min_inclusive
            ..=TIME_BETWEEN_NON_PLAYER_ATTACKS.max_inclusive,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dash_flag_clears_five_ticks_into_the_cooldown() {
        assert_eq!(
            DASH_COOLDOWN_TICKS - DASH_FLAG_CLEAR_AT,
            DASH_MINIMUM_DURATION_TICKS
        );
    }

    #[test]
    fn wrap_degrees_folds_a_full_turn_back_to_zero() {
        assert!((wrap_degrees(360.0)).abs() < f32::EPSILON);
        assert!((wrap_degrees(190.0) - -170.0).abs() < f32::EPSILON);
        assert!((wrap_degrees(-190.0) - 170.0).abs() < f32::EPSILON);
    }
}
