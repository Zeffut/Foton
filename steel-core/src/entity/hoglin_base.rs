//! The charge a hoglin and a zoglin share.
//!
//! Vanilla parity: `net.minecraft.world.entity.monster.hoglin.HoglinBase`. It
//! is the interface behind the one thing both mobs are known for: the hit that
//! launches you rather than merely hurting you.

use glam::DVec3;
use steel_registry::{vanilla_attributes, vanilla_damage_types};

use crate::entity::damage::DamageSource;
use crate::entity::{LivingEntity, SharedEntity};
use crate::world::World;

/// How long the attack animation runs.
///
/// Vanilla parity: `HoglinBase.ATTACK_ANIMATION_DURATION`.
pub const ATTACK_ANIMATION_DURATION: i32 = 10;

/// Chance a naturally spawned hoglin or zoglin is a baby.
///
/// Vanilla parity: `HoglinBase.PROBABILITY_OF_SPAWNING_AS_BABY`.
pub const PROBABILITY_OF_SPAWNING_AS_BABY: f32 = 0.2;

/// The entity event that plays the attack animation on the client.
///
/// Vanilla parity: the `broadcastEntityEvent(this, (byte)4)` of both
/// `doHurtTarget` overrides.
pub const ATTACK_ANIMATION_EVENT: u8 = 4;

/// A mob that gores and throws.
///
/// Vanilla parity: the `HoglinBase` interface. Only the animation counter is a
/// method on it; the two static helpers below are the behavior.
pub trait HoglinBase: LivingEntity {
    /// Vanilla parity: `HoglinBase.getAttackAnimationRemainingTicks`.
    fn attack_animation_remaining_ticks(&self) -> i32;

    /// Starts the attack animation.
    ///
    /// Vanilla assigns the field directly from `doHurtTarget` and
    /// `handleEntityEvent`; a trait cannot, so the write is a method.
    fn set_attack_animation_remaining_ticks(&self, ticks: i32);
}

/// Hits `target` for a randomized share of the attacker's damage, then launches it.
///
/// Vanilla parity: `HoglinBase.hurtAndThrowTarget`. The damage roll is half the
/// attribute plus up to the whole of it, so an adult hoglin's six hits for
/// between three and nine; a baby's is not rolled at all, and a baby never
/// throws.
pub fn hurt_and_throw_target(
    world: &World,
    body: &dyn LivingEntity,
    target: &SharedEntity,
) -> bool {
    let attack_damage = body
        .attributes()
        .lock()
        .required_value(vanilla_attributes::ATTACK_DAMAGE) as f32;

    #[expect(
        clippy::cast_possible_truncation,
        reason = "vanilla rolls `nextInt((int)attackDamage)`, truncating the same way"
    )]
    let whole_damage = attack_damage as i32;
    let actual_damage = if !body.is_baby() && whole_damage > 0 {
        #[expect(
            clippy::cast_precision_loss,
            reason = "an attack damage attribute is far below the f32 integer limit"
        )]
        let roll = rand::random_range(0..whole_damage) as f32;
        attack_damage / 2.0 + roll
    } else {
        attack_damage
    };

    let damage_source = DamageSource::environment(&vanilla_damage_types::MOB_ATTACK)
        .with_causing_entity(body.id())
        .with_direct_entity(body.id());
    let was_hurt = target.hurt(world, &damage_source, actual_damage);
    if was_hurt && !body.is_baby() {
        // Vanilla also runs `EnchantmentHelper.doPostAttackEffects` here. A
        // hoglin has no weapon and no enchantments, so there is nothing for it
        // to run; a zoglin is the same.
        throw_target(body, target);
    }
    was_hurt
}

/// Launches `target` away from `body`.
///
/// Vanilla parity: `HoglinBase.throwTarget`. Vanilla feeds `nextInt(21) - 10`
/// straight into `Vec3.yRot`, which takes **radians**, so the push is swung by
/// up to ten radians rather than ten degrees -- effectively a random direction.
/// It is ported as written, because that scatter is what the hit looks like in
/// game.
pub fn throw_target(body: &dyn LivingEntity, target: &SharedEntity) {
    let knockback_power = body
        .attributes()
        .lock()
        .get_value(vanilla_attributes::ATTACK_KNOCKBACK)
        .unwrap_or(0.0);
    let knockback_resistance = target
        .as_living_entity()
        .and_then(|living| {
            living
                .attributes()
                .lock()
                .get_value(vanilla_attributes::KNOCKBACK_RESISTANCE)
        })
        .unwrap_or(0.0);
    let effective_power = knockback_power - knockback_resistance;
    if effective_power <= 0.0 {
        return;
    }

    let body_position = body.position();
    let target_position = target.position();
    let horizontal = DVec3::new(
        target_position.x - body_position.x,
        0.0,
        target_position.z - body_position.z,
    );
    // Vanilla parity: `Vec3.normalize` returns `ZERO` rather than refusing when
    // the two are stacked, and `throwTarget` carries on -- so a hoglin standing
    // exactly on its target still launches it straight up.
    let direction = horizontal.normalize_or_zero();

    let push_angle = f64::from(rand::random_range(0..21) - 10);
    let horizontal_scale = effective_power * f64::from(0.5f32.mul_add(rand::random::<f32>(), 0.2));
    let scaled = direction * horizontal_scale;
    let (sin, cos) = push_angle.sin_cos();
    let push = DVec3::new(
        scaled.x * cos + scaled.z * sin,
        0.0,
        scaled.z * cos - scaled.x * sin,
    );
    let vertical_scale = effective_power * f64::from(rand::random::<f32>()) * 0.5;

    target.push_impulse(DVec3::new(push.x, vertical_scale, push.z));
    target.mark_hurt();
}
