//! Crossbow item behavior.
//!
//! Vanilla parity: `CrossbowItem`. Holding right-click charges the weapon over
//! `getChargeDuration` ticks -- 25 by default, fewer with Quick Charge. When the
//! charge completes the ammunition leaves the inventory and moves into the
//! stack's `charged_projectiles` component, and the crossbow stays loaded until
//! the next right-click fires it.
//!
//! Every step but `use` takes a `&dyn LivingEntity`, exactly as vanilla does:
//! `onUseTick`, `tryLoadProjectiles`, `performShooting`, `shoot`,
//! `createProjectile` and `shootProjectile` are all `LivingEntity`-typed
//! upstream, and only `Item.use(Level, Player, InteractionHand)` is not. A mob
//! draws through [`perform_crossbow_attack`], which is vanilla
//! `CrossbowAttackMob.performCrossbowAttack`; its ammunition comes from
//! `Monster.getProjectile`, which is the held stack or a plain arrow, so a mob
//! needs no quiver of its own.

use foton_registry::stat::Stat;
use foton_registry::vanilla_stat_types;
use std::sync::Arc;

use std::f64::consts::FRAC_PI_2;

use foton_macros::item_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::data_components::vanilla_components::{
    CHARGED_PROJECTILES, ChargedProjectiles, INTANGIBLE_PROJECTILE,
};
use foton_registry::enchantment_effect::CrossbowChargingSounds;
use foton_registry::item_stack::ItemStack;
use foton_registry::vanilla_item_tags::ItemTag;
use foton_registry::{
    ItemStackTemplate, REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_entities,
    vanilla_items,
};
use foton_utils::types::InteractionHand;
use glam::{DQuat, DVec3};

use crate::advancement::triggers;
use crate::behavior::context::{InteractionResult, UseItemContext};
use crate::behavior::item::{ItemBehavior, ItemUseAnimation};
use crate::behavior::items::arrow_entity_type_for;
use crate::enchantment_helper;
use crate::entity::entities::{ArrowEntity, FireworkRocketEntity};
use crate::entity::{Entity, LivingEntity, Projectile as _, SharedEntity, next_entity_id};
use crate::inventory::container::Container as _;
use crate::inventory::equipment::EquipmentSlot;
use crate::world::World;

/// Seconds a crossbow takes to charge before Quick Charge scales it.
///
/// Vanilla parity: `CrossbowItem.MAX_CHARGE_DURATION`.
const MAX_CHARGE_DURATION: f32 = 1.25;

/// Ticks the crossbow stays usable while held, effectively forever.
///
/// Vanilla parity: `CrossbowItem.getUseDuration`.
const CROSSBOW_USE_DURATION: i32 = 72_000;

/// Fraction of the charge at which the loading-start sound plays.
///
/// Vanilla parity: `CrossbowItem.START_SOUND_PERCENT`.
const START_SOUND_PERCENT: f32 = 0.2;

/// Fraction of the charge at which the loading-middle sound plays.
///
/// Vanilla parity: `CrossbowItem.MID_SOUND_PERCENT`.
const MID_SOUND_PERCENT: f32 = 0.5;

/// Speed given to a loaded arrow.
///
/// Vanilla parity: `CrossbowItem.ARROW_POWER`. Well above the bow's 3.0, which
/// is why a crossbow bolt outranges a fully drawn bow.
const ARROW_POWER: f32 = 3.15;

/// Speed given to a loaded firework rocket.
///
/// Vanilla parity: `CrossbowItem.FIREWORK_POWER`.
const FIREWORK_POWER: f32 = 1.6;

/// How far a mob will stand off and still fire a crossbow.
///
/// Vanilla parity: `CrossbowItem.getDefaultProjectileRange`, which is half a
/// bow's -- a crossbow piglin closes much further than a skeleton does.
const CROSSBOW_PROJECTILE_RANGE: i32 = 8;

/// Speed a mob's bolt leaves the crossbow at.
///
/// Vanilla parity: `CrossbowItem.MOB_ARROW_POWER`. Half a player's, which is
/// why a piglin's bolt arcs visibly and a player's barely does.
pub(crate) const MOB_ARROW_POWER: f32 = 1.6;

/// A mob's aim spread before difficulty is taken off it.
///
/// Vanilla parity: the `14 - difficulty.getId() * 4` of
/// `CrossbowAttackMob.performCrossbowAttack`.
const MOB_UNCERTAINTY_BASE: f32 = 14.0;

/// How much each difficulty step tightens a mob's aim.
const MOB_UNCERTAINTY_PER_DIFFICULTY: f32 = 4.0;

/// Fraction of the way up a target a mob aims for.
///
/// Vanilla parity: the `targetOverride.getY(0.3333333333333333)` of
/// `CrossbowItem.shootProjectile`.
const MOB_AIM_HEIGHT_FRACTION: f64 = 1.0 / 3.0;

/// How much a mob lifts its aim per block of horizontal distance.
///
/// Vanilla parity: the `distanceToTarget * 0.2F` of the same branch.
const MOB_AIM_LIFT_PER_BLOCK: f64 = 0.2;

/// Spread applied to every crossbow shot.
///
/// Vanilla parity: the `1.0F` uncertainty `CrossbowItem.use` passes to
/// `performShooting`.
const SHOT_UNCERTAINTY: f32 = 1.0;

/// Durability a firework rocket costs to fire.
///
/// Vanilla parity: `CrossbowItem.getDurabilityUse`.
const FIREWORK_DURABILITY_USE: i32 = 3;

/// Durability an arrow costs to fire.
const ARROW_DURABILITY_USE: i32 = 1;

/// Height below the eye a fired arrow starts at.
///
/// Vanilla parity: the `getEyeY() - 0.1` of `AbstractArrow`'s shooter
/// constructor.
const ARROW_SPAWN_EYE_OFFSET: f64 = 0.1;

/// Height below the eye a fired rocket starts at.
///
/// Vanilla parity: the `getEyeY() - 0.15F` of `CrossbowItem.createProjectile`.
const FIREWORK_SPAWN_EYE_OFFSET: f64 = 0.15;

/// Where the ammunition for the next charge comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AmmoSource {
    /// A hand holding supported ammunition.
    ///
    /// Vanilla parity: `ProjectileWeaponItem.getHeldProjectile`, which prefers
    /// the off hand so a shield-hand quiver beats one buried in the pack.
    Hand(InteractionHand),
    /// A main-inventory slot holding an arrow.
    Inventory(usize),
    /// Creative with no ammunition anywhere.
    ///
    /// Vanilla parity: the `hasInfiniteMaterials()` tail of
    /// `Player.getProjectile`, which conjures an arrow rather than refusing.
    Creative,
    /// A mob with nothing in either hand.
    ///
    /// Vanilla parity: the `new ItemStack(Items.ARROW)` fallback of
    /// `Monster.getProjectile`. A mob has no pack to search, so it always has
    /// exactly one bolt and never pays for it.
    MobFallback,
}

/// Behavior for the crossbow.
#[item_behavior(class = "CrossbowItem")]
pub struct CrossbowItem;

impl ItemBehavior for CrossbowItem {
    /// Fires a loaded crossbow, or starts charging an empty one.
    ///
    /// Vanilla parity: `CrossbowItem.use`.
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        let mut weapon = context.inv.with_item(|item| item.clone());

        if let Some(power) = loaded_shooting_power(&weapon) {
            perform_shooting(
                context.world,
                context.player,
                context.hand,
                &mut weapon,
                power,
                SHOT_UNCERTAINTY,
                None,
            );
            context.inv.with_item(|item| *item = weapon);
            return InteractionResult::Consume;
        }

        if find_ammo(context.player).is_none() {
            return InteractionResult::Fail;
        }

        context.player.start_using_item(context.hand);
        InteractionResult::Consume
    }

    /// Vanilla parity: `CrossbowItem.getDefaultProjectileRange`.
    fn default_projectile_range(&self) -> Option<i32> {
        Some(CROSSBOW_PROJECTILE_RANGE)
    }

    fn get_use_duration(&self, _stack: &ItemStack, _user: &dyn LivingEntity) -> i32 {
        CROSSBOW_USE_DURATION
    }

    fn get_use_animation(&self, _stack: &ItemStack) -> ItemUseAnimation {
        ItemUseAnimation::Crossbow
    }

    /// Plays the loading sounds and loads the ammunition at full charge.
    ///
    /// Vanilla parity: `CrossbowItem.onUseTick`.
    fn on_use_tick(
        &self,
        world: &Arc<World>,
        user: &dyn LivingEntity,
        stack: &mut ItemStack,
        ticks_remaining: i32,
    ) {
        let charge_ticks = charge_duration(stack);
        let time_held = CROSSBOW_USE_DURATION - ticks_remaining;
        let percent = charge_percent(time_held, charge_ticks);
        let previous = charge_percent(time_held - 1, charge_ticks);

        let default_sounds = default_charging_sounds();
        let sounds =
            enchantment_helper::pick_crossbow_charging_sounds(stack).unwrap_or(&default_sounds);

        if crosses(previous, percent, START_SOUND_PERCENT)
            && let Some(sound) = sounds.start
        {
            world.play_sound_at(sound, SoundSource::Players, user.position(), 0.5, 1.0, None);
        }

        if crosses(previous, percent, MID_SOUND_PERCENT)
            && let Some(sound) = sounds.mid
        {
            world.play_sound_at(sound, SoundSource::Players, user.position(), 0.5, 1.0, None);
        }

        if percent >= 1.0
            && !is_charged(stack)
            && try_load_projectiles(user, stack)
            && let Some(sound) = sounds.end
        {
            let pitch = 0.5f32.mul_add(rand::random::<f32>(), 1.0).recip() + 0.2;
            world.play_sound_at(
                sound,
                user.sound_source(),
                user.position(),
                1.0,
                pitch,
                None,
            );
        }
    }

    /// Reports whether the release should run one more use tick.
    ///
    /// Vanilla parity: `CrossbowItem.releaseUsing`. Vanilla splits this in two
    /// and Foton has one hook for both: `LivingEntity.releaseUsingItem`
    /// discards what `releaseUsing` returns and asks `useOnRelease` -- always
    /// true for a crossbow -- whether to run the extra tick, while Foton's hook
    /// returns the one boolean the caller acts on. Returning `releaseUsing`'s
    /// own answer, as `BowItem` already does here, keeps the extra tick for the
    /// case it exists for: a charge that finishes on the very tick the button
    /// comes up. The cost is that a release below full charge skips one
    /// `on_use_tick`, which can only move a loading sound by a single tick.
    fn release_using(
        &self,
        stack: &mut ItemStack,
        _world: &Arc<World>,
        _user: &dyn LivingEntity,
        time_left: i32,
    ) -> bool {
        let time_held = CROSSBOW_USE_DURATION - time_left;
        power_for_time(time_held, stack) >= 1.0 && is_charged(stack)
    }
}

/// Returns vanilla `CrossbowItem.DEFAULT_SOUNDS`.
fn default_charging_sounds() -> CrossbowChargingSounds {
    CrossbowChargingSounds {
        start: Some(&sound_events::ITEM_CROSSBOW_LOADING_START),
        mid: Some(&sound_events::ITEM_CROSSBOW_LOADING_MIDDLE),
        end: Some(&sound_events::ITEM_CROSSBOW_LOADING_END),
    }
}

/// Returns vanilla `CrossbowItem.getChargeDuration`, in ticks.
pub(crate) fn charge_duration(crossbow: &ItemStack) -> i32 {
    let seconds = enchantment_helper::modify_crossbow_charging_time(crossbow, MAX_CHARGE_DURATION);
    (seconds * 20.0).floor() as i32
}

/// Returns how far through the charge `time_held` ticks are.
const fn charge_percent(time_held: i32, charge_ticks: i32) -> f32 {
    time_held as f32 / charge_ticks as f32
}

/// Returns vanilla `CrossbowItem.getPowerForTime`.
fn power_for_time(time_held: i32, crossbow: &ItemStack) -> f32 {
    let power = charge_percent(time_held, charge_duration(crossbow));
    // Written as a branch rather than `min` because `f32::min` treats NaN as
    // the missing operand and would report a zero-tick charge as full power.
    if power > 1.0 { 1.0 } else { power }
}

/// Returns whether this tick is the one that crosses `threshold`.
///
/// Vanilla keeps `startSoundPlayed` and `midLoadSoundPlayed` on the single
/// `CrossbowItem` instance, so every crossbow in the world shares them and two
/// players charging at once silence each other. Foton compares this tick's
/// percentage against the previous tick's instead: the same play-once behavior,
/// but per user. The negated `>=` keeps NaN -- what a zero-tick charge
/// produces -- on the "not yet past" side, matching the `false` vanilla starts
/// its flags at.
#[expect(
    clippy::neg_cmp_op_on_partial_ord,
    reason = "NaN must count as below the threshold, which `<` would not"
)]
const fn crosses(previous: f32, current: f32, threshold: f32) -> bool {
    current >= threshold && !(previous >= threshold)
}

/// Returns vanilla `CrossbowItem.isCharged`.
pub(crate) fn is_charged(crossbow: &ItemStack) -> bool {
    crossbow
        .get(CHARGED_PROJECTILES)
        .is_some_and(|projectiles| !projectiles.items().is_empty())
}

/// Returns the speed a loaded crossbow fires at, or `None` when it is empty.
///
/// Vanilla parity: the `getShootingPower` guard at the head of
/// `CrossbowItem.use`.
fn loaded_shooting_power(crossbow: &ItemStack) -> Option<f32> {
    let projectiles = crossbow.get(CHARGED_PROJECTILES)?;
    if projectiles.items().is_empty() {
        return None;
    }

    let has_firework = projectiles
        .items()
        .iter()
        .any(|template| template.item() == &*vanilla_items::FIREWORK_ROCKET);
    Some(if has_firework {
        FIREWORK_POWER
    } else {
        ARROW_POWER
    })
}

/// Returns whether a stack can be held as crossbow ammunition.
///
/// Vanilla parity: `ProjectileWeaponItem.ARROW_OR_FIREWORK`.
fn is_held_projectile(stack: &ItemStack) -> bool {
    is_arrow(stack) || stack.is(&vanilla_items::FIREWORK_ROCKET)
}

/// Returns whether a stack is an arrow.
///
/// Vanilla parity: `ProjectileWeaponItem.ARROW_ONLY`.
fn is_arrow(stack: &ItemStack) -> bool {
    REGISTRY.items.is_in_tag(stack.item(), &ItemTag::ARROWS)
}

/// Locates the ammunition the next charge will draw from.
///
/// Vanilla parity: `Player.getProjectile` for a player and
/// `Monster.getProjectile` for anything else. A held arrow or rocket wins for
/// both; only a player then searches its pack, and only a mob falls back to a
/// conjured arrow it never has to own.
fn find_ammo(shooter: &dyn LivingEntity) -> Option<AmmoSource> {
    for hand in [InteractionHand::OffHand, InteractionHand::MainHand] {
        if is_held_projectile(&shooter.get_item_in_hand(hand)) {
            return Some(AmmoSource::Hand(hand));
        }
    }

    let Some(player) = shooter.as_player() else {
        return Some(AmmoSource::MobFallback);
    };

    if let Some(slot) = player
        .inventory
        .lock()
        .get_items()
        .iter()
        .position(is_arrow)
    {
        return Some(AmmoSource::Inventory(slot));
    }

    player
        .has_infinite_materials()
        .then_some(AmmoSource::Creative)
}

/// Reads the ammunition stack a source points at.
fn ammo_stack(shooter: &dyn LivingEntity, source: AmmoSource) -> ItemStack {
    match source {
        AmmoSource::Hand(hand) => shooter.get_item_in_hand(hand),
        AmmoSource::Inventory(slot) => {
            shooter.as_player().map_or_else(ItemStack::empty, |player| {
                player.inventory.lock().get_item(slot).clone()
            })
        }
        AmmoSource::Creative | AmmoSource::MobFallback => ItemStack::new(&vanilla_items::ARROW),
    }
}

/// Removes `amount` ammunition from its source and returns what came out.
fn take_ammo(shooter: &dyn LivingEntity, source: AmmoSource, amount: i32) -> Option<ItemStack> {
    let taken = match source {
        // Vanilla's `ItemStack.split` writes through the live stack; Foton's
        // hand hands back a clone, so the remainder is put away explicitly.
        AmmoSource::Hand(hand) => {
            let mut stack = shooter.get_item_in_hand(hand);
            let taken = stack.split(amount);
            shooter.set_item_in_hand(hand, stack);
            taken
        }
        AmmoSource::Inventory(slot) => {
            let player = shooter.as_player()?;
            let mut inventory = player.inventory.lock();
            let mut stack = inventory.get_item(slot).clone();
            let taken = stack.split(amount);
            inventory.set_item(slot, stack);
            taken
        }
        // Only reachable if a creative player somehow pays for ammunition, or a
        // mob does; `use_ammo` short-circuits both to a free copy first.
        AmmoSource::Creative | AmmoSource::MobFallback => {
            ItemStack::with_count(&vanilla_items::ARROW, amount)
        }
    };
    (!taken.is_empty()).then_some(taken)
}

/// Takes one projectile out of the ammunition source for a single shot.
///
/// Vanilla parity: `ProjectileWeaponItem.useAmmo`. A free draw -- creative, or
/// the second and third bolts of a Multishot volley -- copies the ammunition
/// and marks it intangible so it cannot be picked back up.
fn use_ammo(
    weapon: &ItemStack,
    ammo: &ItemStack,
    shooter: &dyn LivingEntity,
    source: AmmoSource,
    force_infinite: bool,
) -> Option<ItemStackTemplate> {
    // A mob's fallback arrow is conjured rather than carried, so it costs
    // nothing the same way creative's does; vanilla splits it out of a
    // throwaway stack and discards the remainder.
    let free =
        force_infinite || shooter.has_infinite_materials() || source == AmmoSource::MobFallback;
    let ammo_to_use = if free {
        0
    } else {
        enchantment_helper::process_ammo_use(weapon, ammo, 1)
    };

    if ammo_to_use > ammo.count() {
        return None;
    }

    if ammo_to_use == 0 {
        let mut free_copy = ammo.copy_with_count(1);
        free_copy.set(INTANGIBLE_PROJECTILE, ());
        // Foton's arrow entity does not read the pickup stack yet, so the
        // component only travels with the crossbow's saved and synced data.
        return ItemStackTemplate::from_stack(&free_copy).ok();
    }

    let used = take_ammo(shooter, source, ammo_to_use)?;
    ItemStackTemplate::from_stack(&used).ok()
}

/// Draws every projectile one charge loads.
///
/// Vanilla parity: `ProjectileWeaponItem.draw`. Multishot raises the count; only
/// the first draw is paid for.
fn draw(weapon: &ItemStack, shooter: &dyn LivingEntity) -> Vec<ItemStackTemplate> {
    let Some(source) = find_ammo(shooter) else {
        return Vec::new();
    };
    let ammo = ammo_stack(shooter, source);
    if ammo.is_empty() {
        return Vec::new();
    }

    let count = enchantment_helper::process_projectile_count(weapon, 1);
    let mut drawn = Vec::new();
    for index in 0..count {
        if let Some(template) = use_ammo(weapon, &ammo, shooter, source, index > 0) {
            drawn.push(template);
        }
    }
    drawn
}

/// Moves the drawn ammunition into the crossbow.
///
/// Vanilla parity: `CrossbowItem.tryLoadProjectiles`.
fn try_load_projectiles(shooter: &dyn LivingEntity, weapon: &mut ItemStack) -> bool {
    let drawn = draw(weapon, shooter);
    if drawn.is_empty() {
        return false;
    }

    match ChargedProjectiles::new(drawn) {
        Ok(projectiles) => {
            weapon.set(CHARGED_PROJECTILES, projectiles);
            true
        }
        Err(error) => {
            log::debug!("failed to load crossbow projectiles: {error}");
            false
        }
    }
}

/// Winds nothing and simply fires a mob's loaded crossbow at `target`.
///
/// Vanilla parity: `CrossbowAttackMob.performCrossbowAttack`, which picks the
/// hand holding the crossbow, reads the difficulty-scaled spread, and hands the
/// target down so `shootProjectile` lobs the bolt instead of firing it along
/// the shooter's look vector.
pub(crate) fn perform_crossbow_attack(
    world: &Arc<World>,
    shooter: &dyn LivingEntity,
    target: &SharedEntity,
    power: f32,
) {
    let hand = if shooter
        .get_item_in_hand(InteractionHand::MainHand)
        .is(&vanilla_items::CROSSBOW)
    {
        InteractionHand::MainHand
    } else {
        InteractionHand::OffHand
    };
    let mut weapon = shooter.get_item_in_hand(hand);
    if !weapon.is(&vanilla_items::CROSSBOW) {
        return;
    }

    let difficulty = f32::from(u8::from(world.difficulty()));
    let uncertainty = MOB_UNCERTAINTY_PER_DIFFICULTY.mul_add(-difficulty, MOB_UNCERTAINTY_BASE);
    perform_shooting(
        world,
        shooter,
        hand,
        &mut weapon,
        power,
        uncertainty,
        Some(target),
    );
    shooter.set_item_in_hand(hand, weapon);
}

/// Empties the crossbow and launches what was in it.
///
/// Vanilla parity: `CrossbowItem.performShooting`.
fn perform_shooting(
    world: &Arc<World>,
    shooter: &dyn LivingEntity,
    hand: InteractionHand,
    weapon: &mut ItemStack,
    power: f32,
    uncertainty: f32,
    target_override: Option<&SharedEntity>,
) {
    let charged = weapon
        .get(CHARGED_PROJECTILES)
        .cloned()
        .unwrap_or_else(ChargedProjectiles::empty);
    weapon.set(CHARGED_PROJECTILES, ChargedProjectiles::empty());
    if charged.items().is_empty() {
        return;
    }

    shoot(
        world,
        shooter,
        hand,
        weapon,
        charged.items(),
        power,
        uncertainty,
        target_override,
    );

    // Vanilla parity: the tail of `CrossbowItem.performShooting`. The weapon
    // has already had its charged projectiles cleared, which is the stack
    // vanilla hands the trigger too.
    if let Some(player) = shooter.as_player() {
        triggers::item::shot_crossbow(player, weapon);
        player.award_stat(Stat::new(&vanilla_stat_types::USED, weapon.item));
    }
}

/// Spawns each loaded projectile, fanned out by the Multishot spread.
///
/// Vanilla parity: `ProjectileWeaponItem.shoot`. The fan alternates sides of
/// the aim line, which is why three bolts land left, center and right.
#[expect(
    clippy::too_many_arguments,
    reason = "vanilla ProjectileWeaponItem.shoot takes the same nine"
)]
fn shoot(
    world: &Arc<World>,
    shooter: &dyn LivingEntity,
    hand: InteractionHand,
    weapon: &mut ItemStack,
    projectiles: &[ItemStackTemplate],
    power: f32,
    uncertainty: f32,
    target_override: Option<&SharedEntity>,
) {
    let max_angle = enchantment_helper::process_projectile_spread(weapon, 0.0);
    let count = projectiles.len();
    let angle_step = if count == 1 {
        0.0
    } else {
        2.0 * max_angle / (count - 1) as f32
    };
    let angle_offset = ((count - 1) % 2) as f32 * angle_step / 2.0;
    let mut side = 1.0f32;

    for (index, template) in projectiles.iter().enumerate() {
        let mut ammo = template.create();
        if ammo.is_empty() {
            continue;
        }

        // Vanilla writes the step count as `(i + 1) / 2`, which for a
        // non-negative index is the same ladder as `div_ceil`: 0, 1, 1, 2, 2.
        let angle = angle_step.mul_add(side * index.div_ceil(2) as f32, angle_offset);
        side = -side;

        let projectile = create_projectile(world, shooter, weapon, &ammo);
        shoot_projectile(
            world,
            shooter,
            projectile.as_ref(),
            index,
            power,
            uncertainty,
            angle,
            target_override,
        );
        if let Err(error) = world.try_add_entity(Arc::clone(&projectile)) {
            log::debug!("failed to spawn crossbow projectile: {error}");
        }
        // Vanilla parity: `Projectile.applyOnProjectileSpawned` runs the
        // enchantments of the ammunition, then those of the weapon the arrow
        // remembers -- which is the same crossbow, handed over directly here.
        enchantment_helper::on_projectile_spawned(
            world,
            &mut ammo,
            projectile.as_ref(),
            Some(shooter.as_entity_event_source()),
        );
        if !ammo.is(&vanilla_items::FIREWORK_ROCKET) {
            enchantment_helper::on_projectile_spawned(
                world,
                weapon,
                projectile.as_ref(),
                Some(shooter.as_entity_event_source()),
            );
        }

        if weapon.hurt_and_break(durability_use(&ammo), shooter.has_infinite_materials()) {
            shooter.on_equipped_item_broken(equipment_slot(hand));
        }
        if weapon.is_empty() {
            break;
        }
    }
}

/// Builds the entity one loaded projectile becomes.
///
/// Vanilla parity: `CrossbowItem.createProjectile`.
fn create_projectile(
    world: &Arc<World>,
    shooter: &dyn LivingEntity,
    weapon: &ItemStack,
    ammo: &ItemStack,
) -> SharedEntity {
    let position = shooter.position();

    if ammo.is(&vanilla_items::FIREWORK_ROCKET) {
        let rocket = FireworkRocketEntity::launched(
            &vanilla_entities::FIREWORK_ROCKET,
            next_entity_id(),
            position.with_y(shooter.get_eye_y() - FIREWORK_SPAWN_EYE_OFFSET),
            Arc::downgrade(world),
            ammo.clone(),
        );
        rocket.set_owner_uuid(Some(shooter.uuid()));
        // Vanilla passes `shotAtAngle = true`, which stops the rocket from
        // curving upward the way a launched one does.
        rocket.set_shot_at_angle(true);
        return Arc::new(rocket);
    }

    let arrow = ArrowEntity::new(
        arrow_entity_type_for(ammo),
        next_entity_id(),
        position.with_y(shooter.get_eye_y() - ARROW_SPAWN_EYE_OFFSET),
        Arc::downgrade(world),
    );
    arrow.set_owner_uuid(Some(shooter.uuid()));
    // Vanilla parity: `ProjectileWeaponItem.createProjectile` hands the arrow
    // the weapon it came off, which is where the bolt's Piercing is read from.
    // Without it a Piercing crossbow stops at the first mob like a plain one.
    arrow.set_fired_from_weapon(Some(weapon.copy_with_count(weapon.count())));
    // TODO: vanilla also swaps the arrow's hit sound to `CROSSBOW_HIT` and
    // marks a player's shot critical. Foton's arrow models neither.
    Arc::new(arrow)
}

/// Aims and launches one projectile, then plays the shot.
///
/// Vanilla parity: `CrossbowItem.shootProjectile`, both branches. Without a
/// target the bolt leaves along the shooter's look vector; with one it is lobbed
/// at a third of the target's height plus a fifth of the horizontal distance,
/// which is the arc a piglin's bolt visibly takes.
#[expect(
    clippy::too_many_arguments,
    reason = "vanilla ProjectileWeaponItem.shootProjectile takes the same seven, plus the world"
)]
fn shoot_projectile(
    world: &Arc<World>,
    shooter: &dyn LivingEntity,
    projectile: &dyn Entity,
    index: usize,
    power: f32,
    uncertainty: f32,
    angle: f32,
    target_override: Option<&SharedEntity>,
) {
    let projectile_position = projectile.position();
    let Some(projectile) = projectile.as_projectile() else {
        return;
    };

    let shot_vector = if let Some(target) = target_override {
        let shooter_position = shooter.position();
        let target_position = target.position();
        let dx = target_position.x - shooter_position.x;
        let dz = target_position.z - shooter_position.z;
        let distance_to_target = dx.hypot(dz);
        let aim_y = f64::from(target.base().dimensions().height)
            .mul_add(MOB_AIM_HEIGHT_FRACTION, target_position.y);
        let dy = distance_to_target.mul_add(MOB_AIM_LIFT_PER_BLOCK, aim_y - projectile_position.y);
        projectile_shot_vector(shooter, DVec3::new(dx, dy, dz), angle)
    } else {
        let (yaw, pitch) = shooter.rotation();
        // Vanilla `Entity.getUpVector` is the view vector pitched a quarter
        // turn up.
        let up = shooter.calculate_view_vector(pitch - 90.0, yaw);
        let rotation = DQuat::from_axis_angle(up.normalize(), f64::from(angle.to_radians()));
        rotation * shooter.look_angle()
    };
    projectile.shoot(shot_vector, power, uncertainty);

    world.play_sound_at(
        &sound_events::ITEM_CROSSBOW_SHOOT,
        shooter.sound_source(),
        shooter.position(),
        1.0,
        shot_pitch(index),
        None,
    );
}

/// Rotates an aim vector by the Multishot fan angle.
///
/// Vanilla parity: `CrossbowItem.getProjectileShotVector`. The fan turns about
/// an axis perpendicular to the aim rather than about the world's up, so a
/// volley fired steeply upward still spreads sideways.
fn projectile_shot_vector(shooter: &dyn LivingEntity, aim: DVec3, angle: f32) -> DVec3 {
    let view = aim.normalize_or_zero();
    let mut right = view.cross(DVec3::Y);
    if right.length_squared() <= 1.0e-7 {
        let (yaw, pitch) = shooter.rotation();
        right = view.cross(shooter.calculate_view_vector(pitch - 90.0, yaw));
    }
    let Some(right) = right.try_normalize() else {
        return view;
    };

    // Vanilla rotates the view a quarter turn about `right` to get the axis the
    // fan opens around.
    let fan_axis = DQuat::from_axis_angle(right, FRAC_PI_2) * view;
    let Some(fan_axis) = fan_axis.try_normalize() else {
        return view;
    };
    DQuat::from_axis_angle(fan_axis, f64::from(angle.to_radians())) * view
}

/// Returns the pitch the shot sound plays at.
///
/// Vanilla parity: `CrossbowItem.getShotPitch`. The first bolt keeps the plain
/// sound and the rest are detuned in two alternating bands, so a Multishot
/// volley reads as one ragged crack rather than three identical ones.
fn shot_pitch(index: usize) -> f32 {
    if index == 0 {
        return 1.0;
    }
    let range_decider = if index & 1 == 1 { 0.63 } else { 0.43 };
    0.5f32.mul_add(rand::random::<f32>(), 1.8).recip() + range_decider
}

/// Returns vanilla `CrossbowItem.getDurabilityUse`.
fn durability_use(ammo: &ItemStack) -> i32 {
    if ammo.is(&vanilla_items::FIREWORK_ROCKET) {
        FIREWORK_DURABILITY_USE
    } else {
        ARROW_DURABILITY_USE
    }
}

/// Returns vanilla `InteractionHand.asEquipmentSlot`.
const fn equipment_slot(hand: InteractionHand) -> EquipmentSlot {
    match hand {
        InteractionHand::MainHand => EquipmentSlot::MainHand,
        InteractionHand::OffHand => EquipmentSlot::OffHand,
    }
}

#[cfg(test)]
mod tests {
    use foton_registry::data_components::vanilla_components::{ENCHANTMENTS, ItemEnchantments};
    use foton_registry::{init_vanilla_registry, vanilla_enchantments};
    use foton_utils::types::GameType;
    use foton_utils::{ChunkPos, WorldAabb};
    use glam::DVec3;

    use foton_utils::Downcast as _;

    use super::*;
    use crate::bootstrap::init_globals_once;
    use crate::entity::entities::ArrowEntity;
    use crate::player::Player;
    use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};

    /// Slot holding the crossbow, which is also the selected hotbar slot.
    const WEAPON_SLOT: usize = 0;
    /// Slot the tests keep their quiver in.
    const QUIVER_SLOT: usize = 1;
    const TEST_CHUNK: ChunkPos = ChunkPos::new(0, 0);
    const TEST_POSITION: DVec3 = DVec3::new(8.5, 64.0, 8.5);

    fn crossbow_with(enchantment: Option<(&foton_utils::Identifier, u32)>) -> ItemStack {
        let mut stack = ItemStack::new(&vanilla_items::CROSSBOW);
        if let Some((key, level)) = enchantment {
            let mut enchantments = ItemEnchantments::empty();
            enchantments.set(key.clone(), level);
            stack.set(ENCHANTMENTS, enchantments);
        }
        stack
    }

    fn test_player(world: &Arc<World>) -> Arc<Player> {
        let player = TestPlayerBuilder::new(Arc::clone(world), "CrossbowTester", 1).build();
        player.base().set_position_local(TEST_POSITION);
        player
    }

    /// Charges the crossbow in the weapon slot to completion, one tick at a time.
    fn charge_to_completion(world: &Arc<World>, player: &Arc<Player>) -> ItemStack {
        let behavior = CrossbowItem;
        let mut stack = player.inventory.lock().get_item(WEAPON_SLOT).clone();
        let charge_ticks = charge_duration(&stack);
        for tick in 0..=charge_ticks {
            behavior.on_use_tick(
                world,
                player.as_ref(),
                &mut stack,
                CROSSBOW_USE_DURATION - tick,
            );
        }
        stack
    }

    fn arrows_left(player: &Arc<Player>) -> i32 {
        player.inventory.lock().get_item(QUIVER_SLOT).count()
    }

    #[test]
    fn an_unenchanted_crossbow_takes_the_vanilla_twenty_five_ticks_to_charge() {
        init_vanilla_registry();
        // Vanilla: floor(1.25s * 20 ticks) with no charge-time enchantment.
        assert_eq!(charge_duration(&crossbow_with(None)), 25);
    }

    #[test]
    fn quick_charge_shortens_the_charge_a_quarter_second_per_level() {
        init_vanilla_registry();
        let key = &vanilla_enchantments::QUICK_CHARGE.key;

        // 1.25 - 0.25 * level seconds, floored into ticks.
        assert_eq!(charge_duration(&crossbow_with(Some((key, 1)))), 20);
        assert_eq!(charge_duration(&crossbow_with(Some((key, 2)))), 15);
        assert_eq!(charge_duration(&crossbow_with(Some((key, 3)))), 10);
    }

    #[test]
    fn a_charge_time_below_zero_is_clamped_rather_than_run_backwards() {
        init_vanilla_registry();
        // Quick Charge only reaches level three in vanilla, but an over-leveled
        // stack must not produce a negative duration.
        let overcharged = crossbow_with(Some((&vanilla_enchantments::QUICK_CHARGE.key, 10)));
        assert_eq!(charge_duration(&overcharged), 0);
    }

    #[test]
    fn a_partly_held_crossbow_is_below_full_power_and_a_held_one_never_exceeds_it() {
        init_vanilla_registry();
        let crossbow = crossbow_with(None);

        assert!(power_for_time(0, &crossbow) < 1.0);
        assert!(power_for_time(24, &crossbow) < 1.0);
        assert!((power_for_time(25, &crossbow) - 1.0).abs() < f32::EPSILON);
        assert!((power_for_time(72_000, &crossbow) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn each_loading_sound_threshold_is_crossed_exactly_once() {
        // The tick that reaches the threshold reports the crossing; the ones
        // after it stay quiet, which is what vanilla's one-shot flags buy.
        assert!(crosses(0.19, 0.21, START_SOUND_PERCENT));
        assert!(!crosses(0.21, 0.25, START_SOUND_PERCENT));
        assert!(!crosses(0.1, 0.15, START_SOUND_PERCENT));
        // A zero-tick charge yields NaN for the previous tick and must still
        // count as a crossing rather than being swallowed.
        assert!(crosses(f32::NAN, f32::INFINITY, START_SOUND_PERCENT));
    }

    #[test]
    fn holding_a_crossbow_to_full_charge_loads_one_arrow_and_pays_for_it() {
        init_globals_once();
        let world = fresh_test_world("crossbow_charges_one_arrow");
        insert_ready_full_chunk(&world, TEST_CHUNK);
        let player = test_player(&world);
        {
            let mut inventory = player.inventory.lock();
            inventory.set_item(WEAPON_SLOT, crossbow_with(None));
            inventory.set_item(QUIVER_SLOT, ItemStack::with_count(&vanilla_items::ARROW, 5));
        }

        let charged = charge_to_completion(&world, &player);

        assert!(is_charged(&charged));
        let loaded = charged
            .get(CHARGED_PROJECTILES)
            .expect("a charged crossbow carries the component");
        assert_eq!(loaded.items().len(), 1);
        assert_eq!(loaded.items()[0].item(), &*vanilla_items::ARROW);
        assert_eq!(arrows_left(&player), 4);
    }

    /// Widening the shooting path from `&Player` to `&dyn LivingEntity` moved the
    /// held-ammunition lookup off `PlayerInventory` and onto the equipment
    /// slots. For a player the two are the same storage, and this is the case
    /// that proves it: arrows held in the off hand, none in the pack, still
    /// charge the crossbow and still get paid for.
    ///
    /// Vanilla parity: `ProjectileWeaponItem.getHeldProjectile`, which prefers
    /// a hand over the pack.
    #[test]
    fn a_player_charges_from_arrows_held_in_the_off_hand() {
        init_globals_once();
        let world = fresh_test_world("crossbow_offhand_quiver");
        insert_ready_full_chunk(&world, TEST_CHUNK);
        let player = test_player(&world);
        {
            let mut inventory = player.inventory.lock();
            inventory.set_item(WEAPON_SLOT, crossbow_with(None));
            // Nothing in the pack: the off hand is the only ammunition there is.
            inventory.set_item_in_hand(
                InteractionHand::OffHand,
                ItemStack::with_count(&vanilla_items::ARROW, 3),
            );
        }

        let charged = charge_to_completion(&world, &player);

        assert!(
            is_charged(&charged),
            "a player holding arrows in the off hand should be able to charge"
        );
        let loaded = charged
            .get(CHARGED_PROJECTILES)
            .expect("a charged crossbow carries the component");
        assert_eq!(loaded.items().len(), 1);
        assert_eq!(
            player
                .inventory
                .lock()
                .get_item_in_hand(InteractionHand::OffHand)
                .count(),
            2,
            "the bolt is paid for out of the off hand it was drawn from"
        );
    }

    /// A player is still charged for the arrow, unlike a mob, whose fallback
    /// bolt is conjured. This is the survival-mode half of the widened path.
    #[test]
    fn a_survival_player_still_pays_for_every_bolt() {
        init_globals_once();
        let world = fresh_test_world("crossbow_survival_pays");
        insert_ready_full_chunk(&world, TEST_CHUNK);
        let player = test_player(&world);
        {
            let mut inventory = player.inventory.lock();
            inventory.set_item(WEAPON_SLOT, crossbow_with(None));
            inventory.set_item(QUIVER_SLOT, ItemStack::with_count(&vanilla_items::ARROW, 1));
        }

        let charged = charge_to_completion(&world, &player);

        assert!(is_charged(&charged));
        assert_eq!(
            arrows_left(&player),
            0,
            "the last arrow in the pack is spent, not conjured"
        );
    }

    #[test]
    fn a_crossbow_stays_empty_when_the_charge_is_released_early() {
        init_globals_once();
        let world = fresh_test_world("crossbow_released_early");
        insert_ready_full_chunk(&world, TEST_CHUNK);
        let player = test_player(&world);
        {
            let mut inventory = player.inventory.lock();
            inventory.set_item(WEAPON_SLOT, crossbow_with(None));
            inventory.set_item(QUIVER_SLOT, ItemStack::with_count(&vanilla_items::ARROW, 5));
        }

        let behavior = CrossbowItem;
        let mut stack = player.inventory.lock().get_item(WEAPON_SLOT).clone();
        for tick in 0..charge_duration(&stack) {
            behavior.on_use_tick(
                &world,
                player.as_ref(),
                &mut stack,
                CROSSBOW_USE_DURATION - tick,
            );
        }

        assert!(!is_charged(&stack));
        assert_eq!(arrows_left(&player), 5);
        assert!(!behavior.release_using(
            &mut stack,
            &world,
            player.as_ref(),
            CROSSBOW_USE_DURATION - 24
        ));
    }

    #[test]
    fn a_completed_charge_asks_for_the_extra_release_tick() {
        init_globals_once();
        let world = fresh_test_world("crossbow_release_asks_for_a_tick");
        insert_ready_full_chunk(&world, TEST_CHUNK);
        let player = test_player(&world);
        {
            let mut inventory = player.inventory.lock();
            inventory.set_item(WEAPON_SLOT, crossbow_with(None));
            inventory.set_item(QUIVER_SLOT, ItemStack::with_count(&vanilla_items::ARROW, 5));
        }

        let mut charged = charge_to_completion(&world, &player);

        assert!(CrossbowItem.release_using(
            &mut charged,
            &world,
            player.as_ref(),
            CROSSBOW_USE_DURATION - 25
        ));
    }

    #[test]
    fn multishot_loads_three_bolts_but_only_one_leaves_the_quiver() {
        init_globals_once();
        let world = fresh_test_world("crossbow_multishot_loads_three");
        insert_ready_full_chunk(&world, TEST_CHUNK);
        let player = test_player(&world);
        {
            let mut inventory = player.inventory.lock();
            inventory.set_item(
                WEAPON_SLOT,
                crossbow_with(Some((&vanilla_enchantments::MULTISHOT.key, 1))),
            );
            inventory.set_item(QUIVER_SLOT, ItemStack::with_count(&vanilla_items::ARROW, 5));
        }

        let charged = charge_to_completion(&world, &player);

        let loaded = charged
            .get(CHARGED_PROJECTILES)
            .expect("a charged crossbow carries the component");
        assert_eq!(loaded.items().len(), 3);
        assert_eq!(arrows_left(&player), 4, "only the first bolt is paid for");
    }

    #[test]
    fn creative_charges_with_an_empty_pack_and_takes_nothing() {
        init_globals_once();
        let world = fresh_test_world("crossbow_creative_charges_empty");
        insert_ready_full_chunk(&world, TEST_CHUNK);
        let player = test_player(&world);
        player.restore_game_modes(GameType::Creative, None);
        player
            .inventory
            .lock()
            .set_item(WEAPON_SLOT, crossbow_with(None));

        let charged = charge_to_completion(&world, &player);

        assert!(is_charged(&charged));
        assert!(player.inventory.lock().get_item(QUIVER_SLOT).is_empty());
    }

    #[test]
    fn firing_a_loaded_crossbow_spawns_the_arrow_and_empties_the_component() {
        init_globals_once();
        let world = fresh_test_world("crossbow_fires_the_loaded_arrow");
        insert_ready_full_chunk(&world, TEST_CHUNK);
        let player = test_player(&world);
        {
            let mut inventory = player.inventory.lock();
            inventory.set_item(WEAPON_SLOT, crossbow_with(None));
            inventory.set_item(QUIVER_SLOT, ItemStack::with_count(&vanilla_items::ARROW, 5));
        }
        let charged = charge_to_completion(&world, &player);
        player.inventory.lock().set_item(WEAPON_SLOT, charged);

        let mut context = UseItemContext::new(
            player.as_ref(),
            InteractionHand::MainHand,
            &world,
            Arc::clone(&player.inventory),
        );
        assert_eq!(
            CrossbowItem.use_item(&mut context),
            InteractionResult::Consume
        );

        let arrows = world.get_entities_in_aabb_matching(
            &WorldAabb::from_min_max(
                TEST_POSITION - DVec3::splat(4.0),
                TEST_POSITION + DVec3::splat(4.0),
            ),
            |entity| entity.entity_type() == &vanilla_entities::ARROW,
        );
        assert_eq!(arrows.len(), 1);
        assert!(
            arrows[0].velocity().length() > 0.0,
            "the bolt must be moving"
        );

        let weapon = player.inventory.lock().get_item(WEAPON_SLOT).clone();
        assert!(!is_charged(&weapon), "firing empties the crossbow");
        assert!(weapon.get_damage_value() > 0, "firing costs durability");
    }

    /// Fires a charged crossbow and reports the pierce level of the bolt that
    /// left it.
    fn fired_bolt_pierce_level(world: &Arc<World>, player: &Arc<Player>) -> i8 {
        let charged = charge_to_completion(world, player);
        player.inventory.lock().set_item(WEAPON_SLOT, charged);

        let mut context = UseItemContext::new(
            player.as_ref(),
            InteractionHand::MainHand,
            world,
            Arc::clone(&player.inventory),
        );
        assert_eq!(
            CrossbowItem.use_item(&mut context),
            InteractionResult::Consume
        );

        let arrows = world.get_entities_in_aabb_matching(
            &WorldAabb::from_min_max(
                TEST_POSITION - DVec3::splat(4.0),
                TEST_POSITION + DVec3::splat(4.0),
            ),
            |entity| entity.downcast_ref::<ArrowEntity>().is_some(),
        );
        assert_eq!(arrows.len(), 1, "exactly one bolt should have left");
        arrows[0]
            .as_ref()
            .downcast_ref::<ArrowEntity>()
            .expect("the matcher already proved this is an arrow")
            .pierce_level()
    }

    /// Piercing was inert: nothing ever set a bolt's pierce level above zero.
    ///
    /// The crossbow never handed the arrow the weapon it came off, so
    /// `getPiercingCount` had nothing to read, and every bolt stopped in the
    /// first mob it touched. The test fires a real crossbow through
    /// `use_item` and reads the pierce level off the entity that lands in the
    /// world, because that is where the number has to be by the time it hits
    /// anything.
    #[test]
    fn piercing_reaches_the_bolt_that_leaves_the_crossbow() {
        init_globals_once();
        let world = fresh_test_world("crossbow_piercing_reaches_the_bolt");
        insert_ready_full_chunk(&world, TEST_CHUNK);
        let player = test_player(&world);
        {
            let mut inventory = player.inventory.lock();
            inventory.set_item(WEAPON_SLOT, crossbow_with(None));
            inventory.set_item(QUIVER_SLOT, ItemStack::with_count(&vanilla_items::ARROW, 5));
        }
        assert_eq!(
            fired_bolt_pierce_level(&world, &player),
            0,
            "a plain crossbow's bolt pierces nothing"
        );

        let world = fresh_test_world("crossbow_piercing_reaches_the_bolt_enchanted");
        insert_ready_full_chunk(&world, TEST_CHUNK);
        let player = test_player(&world);
        {
            let mut inventory = player.inventory.lock();
            inventory.set_item(
                WEAPON_SLOT,
                crossbow_with(Some((&vanilla_enchantments::PIERCING.key, 3))),
            );
            inventory.set_item(QUIVER_SLOT, ItemStack::with_count(&vanilla_items::ARROW, 5));
        }
        // Vanilla `piercing.json` is `add_value` of one per level.
        assert_eq!(
            fired_bolt_pierce_level(&world, &player),
            3,
            "Piercing III should let a bolt through three mobs"
        );
    }

    #[test]
    fn a_loaded_rocket_leaves_the_crossbow_as_a_firework_and_costs_triple_durability() {
        init_globals_once();
        let world = fresh_test_world("crossbow_fires_a_rocket");
        insert_ready_full_chunk(&world, TEST_CHUNK);
        let player = test_player(&world);
        {
            let mut inventory = player.inventory.lock();
            inventory.set_item(WEAPON_SLOT, crossbow_with(None));
            inventory.set_item_in_hand(
                InteractionHand::OffHand,
                ItemStack::with_count(&vanilla_items::FIREWORK_ROCKET, 2),
            );
        }
        let charged = charge_to_completion(&world, &player);
        let power = loaded_shooting_power(&charged).expect("the crossbow is loaded");
        assert!(
            (power - FIREWORK_POWER).abs() < f32::EPSILON,
            "a rocket flies slower than a bolt"
        );
        player.inventory.lock().set_item(WEAPON_SLOT, charged);

        let mut context = UseItemContext::new(
            player.as_ref(),
            InteractionHand::MainHand,
            &world,
            Arc::clone(&player.inventory),
        );
        assert_eq!(
            CrossbowItem.use_item(&mut context),
            InteractionResult::Consume
        );

        let rockets = world.get_entities_in_aabb_matching(
            &WorldAabb::from_min_max(
                TEST_POSITION - DVec3::splat(4.0),
                TEST_POSITION + DVec3::splat(4.0),
            ),
            |entity| entity.entity_type() == &vanilla_entities::FIREWORK_ROCKET,
        );
        assert_eq!(rockets.len(), 1);

        let weapon = player.inventory.lock().get_item(WEAPON_SLOT).clone();
        assert_eq!(weapon.get_damage_value(), FIREWORK_DURABILITY_USE);
    }

    #[test]
    fn using_a_crossbow_with_nothing_to_load_fails() {
        init_globals_once();
        let world = fresh_test_world("crossbow_without_ammunition");
        insert_ready_full_chunk(&world, TEST_CHUNK);
        let player = test_player(&world);
        player
            .inventory
            .lock()
            .set_item(WEAPON_SLOT, crossbow_with(None));

        let mut context = UseItemContext::new(
            player.as_ref(),
            InteractionHand::MainHand,
            &world,
            Arc::clone(&player.inventory),
        );
        assert_eq!(CrossbowItem.use_item(&mut context), InteractionResult::Fail);
        assert!(player.active_item_use_hand().is_none());
    }

    #[test]
    fn the_first_bolt_of_a_volley_keeps_the_undisturbed_shot_pitch() {
        // Vanilla detunes every bolt after the first into two alternating bands.
        assert!((shot_pitch(0) - 1.0).abs() < f32::EPSILON);
        for _ in 0..32 {
            let odd = shot_pitch(1);
            let even = shot_pitch(2);
            assert!((1.06..=1.19).contains(&odd), "odd bolt pitch {odd}");
            assert!((0.86..=0.99).contains(&even), "even bolt pitch {even}");
        }
    }

    #[test]
    fn quick_charge_swaps_in_its_own_loading_start_sound() {
        init_vanilla_registry();
        let plain = crossbow_with(None);
        assert!(enchantment_helper::pick_crossbow_charging_sounds(&plain).is_none());

        let quick = crossbow_with(Some((&vanilla_enchantments::QUICK_CHARGE.key, 2)));
        let sounds = enchantment_helper::pick_crossbow_charging_sounds(&quick)
            .expect("Quick Charge declares charging sounds");
        assert_eq!(
            sounds.start.map(|sound| sound.key.to_string()),
            Some("minecraft:item.crossbow.quick_charge_2".to_owned())
        );
        assert_eq!(
            sounds.end.map(|sound| sound.key.to_string()),
            Some("minecraft:item.crossbow.loading_end".to_owned())
        );
    }
}
