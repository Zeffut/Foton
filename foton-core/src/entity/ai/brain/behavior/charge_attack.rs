//! Vanilla `ChargeAttack`.

use foton_registry::item_stack::ItemStack;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::{vanilla_attributes, vanilla_damage_types, vanilla_mob_effects};
use glam::DVec3;

use super::{BrainContext, MemoryModuleId, MemoryStatus, TimedBehavior};
use crate::enchantment_helper::{self, EnchantmentPostAttackContext};
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::ai::targeting::TargetingConditions;
use crate::entity::damage::DamageSource;
use crate::entity::{Mob, SharedEntity, is_tamed};
use crate::inventory::equipment::EquipmentSlot;

/// The smallest speed factor a charge's knockback is scaled by.
///
/// Vanilla parity: the low bound of the `Mth.clamp(..., 0.2F, 2.0F)` in
/// `ChargeAttack.dealKnockBack`.
const MIN_KNOCKBACK_SPEED_FACTOR: f32 = 0.2;
/// The largest speed factor a charge's knockback is scaled by.
const MAX_KNOCKBACK_SPEED_FACTOR: f32 = 2.0;
/// What one level of speed or slowness is worth to a charge's knockback.
///
/// Vanilla parity: the `0.25F` of `ChargeAttack.dealKnockBack`.
const KNOCKBACK_PER_EFFECT_LEVEL: f32 = 0.25;
/// The rotation limit that lets a charging body snap straight at its target.
///
/// Vanilla parity: the `360.0F` pair of `ChargeAttack.tick`'s `lookAt`.
const FULL_TURN: f32 = 360.0;

/// Rushes `ATTACK_TARGET` in a straight line and rams whatever it reaches.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.ChargeAttack`. The
/// charge is not pathfinding: [`Self::start`] freezes one velocity vector at
/// the target and the body carries it until it hits something, runs out of
/// charge distance, loses sight of the target, or is tamed mid-run.
pub struct ChargeAttack {
    time_between_attacks: i32,
    charge_targeting: TargetingConditions,
    speed: f32,
    knockback_force: f32,
    max_charge_distance: f64,
    max_target_detection_distance: f64,
    charge_sound: SoundEventRef,
    charge_velocity_vector: DVec3,
    start_position: DVec3,
    entry_condition: [(MemoryModuleId, MemoryStatus); 2],
}

impl ChargeAttack {
    /// Vanilla parity: the `ChargeAttack` constructor.
    #[must_use]
    pub const fn new(
        time_between_attacks: i32,
        charge_targeting: TargetingConditions,
        speed: f32,
        knockback_force: f32,
        max_charge_distance: f64,
        max_target_detection_distance: f64,
        charge_sound: SoundEventRef,
    ) -> Self {
        Self {
            time_between_attacks,
            charge_targeting,
            speed,
            knockback_force,
            max_charge_distance,
            max_target_detection_distance,
            charge_sound,
            charge_velocity_vector: DVec3::ZERO,
            start_position: DVec3::ZERO,
            entry_condition: [
                (
                    memory_module_types::CHARGE_COOLDOWN_TICKS.id(),
                    MemoryStatus::ValueAbsent,
                ),
                (
                    memory_module_types::ATTACK_TARGET.id(),
                    MemoryStatus::ValuePresent,
                ),
            ],
        }
    }

    /// Vanilla parity: `ChargeAttack.dealDamageToTarget`.
    fn deal_damage_to_target(ctx: &BrainContext<'_>, target: &SharedEntity) {
        let body = ctx.mob();
        let damage_source = Self::damage_source(ctx);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "an attack-damage attribute, immediately used as damage"
        )]
        let damage = body
            .attributes()
            .lock()
            .required_value(vanilla_attributes::ATTACK_DAMAGE) as f32;
        if !target.hurt(ctx.world(), &damage_source, damage) {
            return;
        }

        // Vanilla parity: `EnchantmentHelper.doPostAttackEffects`, whose item
        // source is the attacker's weapon -- a nautilus never holds one, but
        // the victim half of the call still runs off the target's own armor.
        let weapon_item = {
            let mut main_hand = ItemStack::empty();
            body.with_equipment_slot(EquipmentSlot::MainHand, &mut |item_stack| {
                main_hand = item_stack.copy_with_count(item_stack.count());
            });
            main_hand
        };
        let context = EnchantmentPostAttackContext::new(
            target.as_ref(),
            Some(body.as_entity_event_source()),
            Some(body.as_entity_event_source()),
            &damage_source,
        );
        enchantment_helper::do_post_attack_effects_with_item_source(
            ctx.world(),
            target.as_ref(),
            &weapon_item,
            &context,
        );
    }

    /// Vanilla parity: `ChargeAttack.dealKnockBack`.
    fn deal_knockback(ctx: &BrainContext<'_>, target: &SharedEntity, speed: f32, force: f32) {
        let body = ctx.mob();
        let speed_level = body
            .mob_effect(vanilla_mob_effects::SPEED)
            .map_or(0, |effect| effect.amplifier() + 1);
        let slowness_level = body
            .mob_effect(vanilla_mob_effects::SLOWNESS)
            .map_or(0, |effect| effect.amplifier() + 1);
        #[expect(
            clippy::cast_precision_loss,
            reason = "an effect amplifier, which vanilla widens the same way"
        )]
        let speed_boost_power = KNOCKBACK_PER_EFFECT_LEVEL * (speed_level - slowness_level) as f32;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a movement-speed attribute, immediately used as a speed"
        )]
        let movement_speed = body
            .attributes()
            .lock()
            .required_value(vanilla_attributes::MOVEMENT_SPEED) as f32;
        let speed_factor = (speed * movement_speed)
            .clamp(MIN_KNOCKBACK_SPEED_FACTOR, MAX_KNOCKBACK_SPEED_FACTOR)
            + speed_boost_power;

        body.cause_extra_knockback(
            target.as_ref(),
            f64::from(speed_factor * force),
            body.velocity(),
        );
    }

    /// The body of `ChargeAttack.stop`, callable while the charge is mid-tick.
    fn end_charge(ctx: &BrainContext<'_>, time_between_attacks: i32) {
        ctx.brain().set_memory(
            memory_module_types::CHARGE_COOLDOWN_TICKS,
            time_between_attacks,
        );
        ctx.brain()
            .erase_memory(memory_module_types::ATTACK_TARGET.id());
    }

    /// Vanilla parity: `level.damageSources().mobAttack(body)`.
    fn damage_source(ctx: &BrainContext<'_>) -> DamageSource {
        let body = ctx.mob();
        DamageSource::environment(&vanilla_damage_types::MOB_ATTACK)
            .with_causing_entity(body.id())
            .with_direct_entity(body.id())
            .with_source_position(body.position())
    }
}

impl TimedBehavior for ChargeAttack {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &self.entry_condition
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        ctx.brain()
            .has_memory_value(memory_module_types::ATTACK_TARGET.id())
    }

    /// Vanilla parity: `ChargeAttack.canStillUse`.
    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        let Some(target) = brain
            .get_memory(memory_module_types::ATTACK_TARGET)
            .and_then(|memory| memory.get())
        else {
            return false;
        };
        let body = ctx.mob();
        // Vanilla parity: the `body instanceof TamableAnimal tamed && tamed.isTame()`
        // bail-out -- a nautilus tamed mid-charge breaks it off.
        if is_tamed(body.as_entity_event_source()) {
            return false;
        }
        if body.position().distance_squared(self.start_position)
            >= self.max_charge_distance * self.max_charge_distance
        {
            return false;
        }
        if target.position().distance_squared(body.position())
            >= self.max_target_detection_distance * self.max_target_detection_distance
        {
            return false;
        }
        if !body.has_line_of_sight(target.as_ref()) {
            return false;
        }
        !brain.has_memory_value(memory_module_types::CHARGE_COOLDOWN_TICKS.id())
    }

    /// Vanilla parity: `ChargeAttack.start`.
    fn start(&mut self, ctx: &BrainContext<'_>) {
        let body = ctx.mob();
        self.start_position = body.position();
        let Some(target) = ctx
            .brain()
            .get_memory(memory_module_types::ATTACK_TARGET)
            .and_then(|memory| memory.get())
        else {
            return;
        };
        let direction = (target.position() - body.position()).normalize_or_zero();
        self.charge_velocity_vector = direction * f64::from(self.speed);
        if self.can_still_use(ctx) {
            body.play_sound(self.charge_sound, 1.0, 1.0);
        }
    }

    /// Vanilla parity: `ChargeAttack.tick`.
    fn tick(&mut self, ctx: &BrainContext<'_>) {
        let Some(target) = ctx
            .brain()
            .get_memory(memory_module_types::ATTACK_TARGET)
            .and_then(|memory| memory.get())
        else {
            return;
        };
        let body = ctx.mob();
        Mob::look_at(body, target.as_ref(), FULL_TURN, FULL_TURN);
        body.set_velocity(self.charge_velocity_vector);

        let Some(body_living) = body.as_entity_event_source().as_living_entity() else {
            return;
        };
        let charge_targeting = &self.charge_targeting;
        let rammed = ctx
            .world()
            .get_entities_in_aabb_matching(&body.bounding_box(), |candidate| {
                candidate.as_living_entity().is_some_and(|living| {
                    charge_targeting.test(ctx.world(), Some(body_living), living)
                })
            })
            .into_iter()
            .next();
        let Some(rammed) = rammed else {
            return;
        };
        // Vanilla parity: a rider is not rammed by the mob carrying them.
        if body.has_passenger(rammed.as_ref()) {
            return;
        }

        Self::deal_damage_to_target(ctx, &rammed);
        Self::deal_knockback(ctx, &rammed, self.speed, self.knockback_force);
        Self::end_charge(ctx, self.time_between_attacks);
    }

    /// Vanilla parity: `ChargeAttack.stop`, which is also what [`Self::tick`]
    /// calls the moment the charge connects.
    fn stop(&mut self, ctx: &BrainContext<'_>) {
        Self::end_charge(ctx, self.time_between_attacks);
    }

    fn debug_name(&self) -> &'static str {
        "ChargeAttack"
    }
}
