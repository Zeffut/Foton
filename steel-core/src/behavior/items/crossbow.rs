//! Crossbow item behavior.
//!
//! Vanilla parity: `CrossbowItem`. Holding right-click charges the weapon over
//! `getChargeDuration` ticks -- 25 by default, fewer with Quick Charge. When the
//! charge completes the ammunition leaves the inventory and moves into the
//! stack's `charged_projectiles` component, and the crossbow stays loaded until
//! the next right-click fires it.

use std::sync::Arc;

use glam::DQuat;
use steel_macros::item_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::data_components::vanilla_components::{
    CHARGED_PROJECTILES, ChargedProjectiles, INTANGIBLE_PROJECTILE,
};
use steel_registry::enchantment_effect::CrossbowChargingSounds;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{
    ItemStackTemplate, REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_entities,
    vanilla_items,
};
use steel_utils::types::InteractionHand;

use crate::behavior::context::{InteractionResult, UseItemContext};
use crate::behavior::item::{ItemBehavior, ItemUseAnimation};
use crate::behavior::items::arrow_entity_type_for;
use crate::enchantment_helper;
use crate::entity::entities::{ArrowEntity, FireworkRocketEntity};
use crate::entity::{Entity, LivingEntity, Projectile as _, SharedEntity, next_entity_id};
use crate::inventory::container::Container as _;
use crate::inventory::equipment::EquipmentSlot;
use crate::player::Player;
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
        // Vanilla lets any `LivingEntity` charge a crossbow, which is how a
        // pillager reloads. Steel has no mob ammunition inventory, so only a
        // player can charge one here.
        let Some(player) = user.as_player() else {
            return;
        };

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
            && try_load_projectiles(player, stack)
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
    /// and Steel has one hook for both: `LivingEntity.releaseUsingItem`
    /// discards what `releaseUsing` returns and asks `useOnRelease` -- always
    /// true for a crossbow -- whether to run the extra tick, while Steel's hook
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
/// players charging at once silence each other. Steel compares this tick's
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
fn is_charged(crossbow: &ItemStack) -> bool {
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
/// Vanilla parity: `Player.getProjectile`. A held arrow or rocket wins, then
/// the first arrow in the pack, then creative's free arrow.
fn find_ammo(player: &Player) -> Option<AmmoSource> {
    {
        let inventory = player.inventory.lock();
        for hand in [InteractionHand::OffHand, InteractionHand::MainHand] {
            if is_held_projectile(inventory.get_item_in_hand(hand)) {
                return Some(AmmoSource::Hand(hand));
            }
        }

        if let Some(slot) = inventory.get_items().iter().position(is_arrow) {
            return Some(AmmoSource::Inventory(slot));
        }
    }

    player
        .has_infinite_materials()
        .then_some(AmmoSource::Creative)
}

/// Reads the ammunition stack a source points at.
fn ammo_stack(player: &Player, source: AmmoSource) -> ItemStack {
    match source {
        AmmoSource::Hand(hand) => player.inventory.lock().get_item_in_hand(hand).clone(),
        AmmoSource::Inventory(slot) => player.inventory.lock().get_item(slot).clone(),
        AmmoSource::Creative => ItemStack::new(&vanilla_items::ARROW),
    }
}

/// Removes `amount` ammunition from its source and returns what came out.
fn take_ammo(player: &Player, source: AmmoSource, amount: i32) -> Option<ItemStack> {
    let mut inventory = player.inventory.lock();
    let taken = match source {
        AmmoSource::Hand(hand) => inventory.split_item_in_hand(hand, amount),
        AmmoSource::Inventory(slot) => {
            let mut stack = inventory.get_item(slot).clone();
            let taken = stack.split(amount);
            inventory.set_item(slot, stack);
            taken
        }
        // Only reachable if a creative player somehow pays for ammunition;
        // `use_ammo` short-circuits infinite materials to a free copy first.
        AmmoSource::Creative => ItemStack::with_count(&vanilla_items::ARROW, amount),
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
    player: &Player,
    source: AmmoSource,
    force_infinite: bool,
) -> Option<ItemStackTemplate> {
    let ammo_to_use = if force_infinite || player.has_infinite_materials() {
        0
    } else {
        enchantment_helper::process_ammo_use(weapon, 1)
    };

    if ammo_to_use > ammo.count() {
        return None;
    }

    if ammo_to_use == 0 {
        let mut free_copy = ammo.copy_with_count(1);
        free_copy.set(INTANGIBLE_PROJECTILE, ());
        // Steel's arrow entity does not read the pickup stack yet, so the
        // component only travels with the crossbow's saved and synced data.
        return ItemStackTemplate::from_stack(&free_copy).ok();
    }

    let used = take_ammo(player, source, ammo_to_use)?;
    ItemStackTemplate::from_stack(&used).ok()
}

/// Draws every projectile one charge loads.
///
/// Vanilla parity: `ProjectileWeaponItem.draw`. Multishot raises the count; only
/// the first draw is paid for.
fn draw(weapon: &ItemStack, player: &Player) -> Vec<ItemStackTemplate> {
    let Some(source) = find_ammo(player) else {
        return Vec::new();
    };
    let ammo = ammo_stack(player, source);
    if ammo.is_empty() {
        return Vec::new();
    }

    let count = enchantment_helper::process_projectile_count(weapon, 1);
    let mut drawn = Vec::new();
    for index in 0..count {
        if let Some(template) = use_ammo(weapon, &ammo, player, source, index > 0) {
            drawn.push(template);
        }
    }
    drawn
}

/// Moves the drawn ammunition into the crossbow.
///
/// Vanilla parity: `CrossbowItem.tryLoadProjectiles`.
fn try_load_projectiles(player: &Player, weapon: &mut ItemStack) -> bool {
    let drawn = draw(weapon, player);
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

/// Empties the crossbow and launches what was in it.
///
/// Vanilla parity: `CrossbowItem.performShooting`.
fn perform_shooting(
    world: &Arc<World>,
    shooter: &Player,
    hand: InteractionHand,
    weapon: &mut ItemStack,
    power: f32,
    uncertainty: f32,
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
    );

    // TODO: fire the SHOT_CROSSBOW advancement trigger and award the ITEM_USED
    // stat once Steel has advancements and statistics.
}

/// Spawns each loaded projectile, fanned out by the Multishot spread.
///
/// Vanilla parity: `ProjectileWeaponItem.shoot`. The fan alternates sides of
/// the aim line, which is why three bolts land left, center and right.
fn shoot(
    world: &Arc<World>,
    shooter: &Player,
    hand: InteractionHand,
    weapon: &mut ItemStack,
    projectiles: &[ItemStackTemplate],
    power: f32,
    uncertainty: f32,
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

        let projectile = create_projectile(world, shooter, &ammo);
        shoot_projectile(
            world,
            shooter,
            projectile.as_ref(),
            index,
            power,
            uncertainty,
            angle,
        );
        if let Err(error) = world.try_add_entity(Arc::clone(&projectile)) {
            log::debug!("failed to spawn crossbow projectile: {error}");
        }
        // Vanilla parity: `Projectile.applyOnProjectileSpawned` runs the
        // enchantments of the ammunition, then those of the weapon the arrow
        // remembers. Steel's arrow does not carry a weapon stack, so the
        // crossbow is handed over directly.
        enchantment_helper::on_projectile_spawned(
            world,
            &mut ammo,
            projectile.as_ref(),
            Some(shooter),
        );
        if !ammo.is(&vanilla_items::FIREWORK_ROCKET) {
            enchantment_helper::on_projectile_spawned(
                world,
                weapon,
                projectile.as_ref(),
                Some(shooter),
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
fn create_projectile(world: &Arc<World>, shooter: &Player, ammo: &ItemStack) -> SharedEntity {
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
    // TODO: vanilla also swaps the arrow's hit sound to `CROSSBOW_HIT`, marks a
    // player's shot critical and applies Piercing. Steel's arrow entity models
    // none of those three, so a crossbow bolt currently hits like a bow's.
    Arc::new(arrow)
}

/// Aims and launches one projectile, then plays the shot.
///
/// Vanilla parity: `CrossbowItem.shootProjectile` without the mob-only target
/// override, which needs the ranged-attack goals Steel has no crossbow user for.
fn shoot_projectile(
    world: &Arc<World>,
    shooter: &Player,
    projectile: &dyn Entity,
    index: usize,
    power: f32,
    uncertainty: f32,
    angle: f32,
) {
    let Some(projectile) = projectile.as_projectile() else {
        return;
    };

    let (yaw, pitch) = shooter.rotation();
    // Vanilla `Entity.getUpVector` is the view vector pitched a quarter turn up.
    let up = shooter.calculate_view_vector(pitch - 90.0, yaw);
    let rotation = DQuat::from_axis_angle(up.normalize(), f64::from(angle.to_radians()));
    projectile.shoot(rotation * shooter.look_angle(), power, uncertainty);

    world.play_sound_at(
        &sound_events::ITEM_CROSSBOW_SHOOT,
        shooter.sound_source(),
        shooter.position(),
        1.0,
        shot_pitch(index),
        None,
    );
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
    use glam::DVec3;
    use steel_registry::data_components::vanilla_components::{ENCHANTMENTS, ItemEnchantments};
    use steel_registry::{init_vanilla_registry, vanilla_enchantments};
    use steel_utils::types::GameType;
    use steel_utils::{ChunkPos, WorldAabb};

    use super::*;
    use crate::bootstrap::init_globals_once;
    use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};

    /// Slot holding the crossbow, which is also the selected hotbar slot.
    const WEAPON_SLOT: usize = 0;
    /// Slot the tests keep their quiver in.
    const QUIVER_SLOT: usize = 1;
    const TEST_CHUNK: ChunkPos = ChunkPos::new(0, 0);
    const TEST_POSITION: DVec3 = DVec3::new(8.5, 64.0, 8.5);

    fn crossbow_with(enchantment: Option<(&steel_utils::Identifier, u32)>) -> ItemStack {
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
