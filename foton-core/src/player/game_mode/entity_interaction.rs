use super::{
    ATTACK_RANGE_BUFFER, CSetEntityMotion, ClipBlockShape, ClipFluid, DVec3, DamageSource,
    DamageType, ENTITY_INTERACTION_RANGE_BUFFER, EnchantmentDamageContext,
    EnchantmentPostAttackContext, Entity, EntityTypeRef, GameType, ITEM_BEHAVIORS, InteractionHand,
    InteractionResult, InventoryAccess, ItemStack, LivingEntity, PiercingWeapon, Player, SAttack,
    SInteract, SharedEntity, SoundEventHolder, SoundEventRef, TextComponent, TranslatedMessage,
    World, WorldAabb, enchantment_helper, piercing_ray_hit_t, vanilla_attributes,
    vanilla_damage_types, vanilla_entities,
};
use std::sync::Arc;

use foton_protocol::packets::game::{AnimateAction, CAnimate};
use foton_registry::data_components::components::{KineticWeapon, KineticWeaponCondition};
use foton_registry::entity_data::ParticleData;
use foton_registry::equipment::EquipmentSlot;
use foton_registry::vanilla_item_tags::ItemTag;
use foton_registry::{sound_events, vanilla_mob_effects, vanilla_particle_types};
use foton_utils::entity_events::EntityStatus;

use crate::advancement::triggers;

/// Returns a height `progress` of the way up an entity's hitbox.
///
/// Vanilla parity: `Entity.getY(double)`.
fn entity_y_at(entity: &dyn Entity, progress: f64) -> f64 {
    entity.position().y + entity.bounding_box().height() * progress
}

const fn sound_holder_ref(holder: &SoundEventHolder) -> Option<SoundEventRef> {
    match holder {
        SoundEventHolder::Registry(sound) => Some(*sound),
        SoundEventHolder::Direct { .. } => {
            // TODO: Support direct sound holders when entity sound playback can send them.
            None
        }
    }
}
impl Player {
    fn invalid_entity_attacked_message() -> TextComponent {
        TranslatedMessage {
            key: "multiplayer.disconnect.invalid_entity_attacked".into(),
            fallback: None,
            args: None,
        }
        .component()
    }

    fn eye_position(&self) -> DVec3 {
        let position = self.position();
        DVec3::new(position.x, self.get_eye_y(), position.z)
    }

    fn damage_source_for_attack_type(&self, damage_type: &'static DamageType) -> DamageSource {
        DamageSource::environment(damage_type)
            .with_causing_entity(self.id())
            .with_direct_entity(self.id())
            .with_source_position(self.position())
    }

    fn attack_damage_source(&self, attacking_item: &ItemStack) -> DamageSource {
        if let Some(damage_type) = attacking_item.get_damage_type() {
            return self.damage_source_for_attack_type(damage_type);
        }
        if let Some(source) = ITEM_BEHAVIORS
            .get_behavior(attacking_item.item())
            .get_item_damage_source(self)
        {
            return source;
        }
        self.damage_source_for_attack_type(&vanilla_damage_types::PLAYER_ATTACK)
    }

    /// Ticks vanilla attack-strength recovery and resets it on main-hand item changes.
    pub(in crate::player) fn tick_attack_strength(&self) {
        self.tick_state.lock().advance_attack_strength_ticker();

        let main_hand_item = {
            let inventory = self.inventory.lock();
            let stack = inventory.get_item_in_hand(InteractionHand::MainHand);
            stack.copy_with_count(stack.count())
        };

        let mut last_item = self.last_item_in_main_hand.lock();
        if ItemStack::matches(&last_item, &main_hand_item) {
            return;
        }

        if !ItemStack::is_same_item(&last_item, &main_hand_item) {
            self.reset_attack_strength_ticker();
        }

        *last_item = main_hand_item;
    }

    fn reset_attack_strength_ticker(&self) {
        self.tick_state.lock().reset_attack_strength_ticker();
    }

    fn current_item_attack_strength_delay(&self) -> f32 {
        let attack_speed = self
            .attributes()
            .lock()
            .required_value(vanilla_attributes::ATTACK_SPEED);
        Self::attack_strength_delay_from_speed(attack_speed)
    }

    fn attack_strength_delay_from_speed(attack_speed: f64) -> f32 {
        (1.0 / attack_speed * 20.0) as f32
    }

    /// Returns vanilla `Player.getAttackStrengthScale`.
    #[must_use]
    pub fn attack_strength_scale(&self, partial_tick: f32) -> f32 {
        let attack_strength_delay = self.current_item_attack_strength_delay();
        self.attack_strength_scale_for_delay(partial_tick, attack_strength_delay)
    }

    fn attack_strength_scale_for_delay(
        &self,
        partial_tick: f32,
        attack_strength_delay: f32,
    ) -> f32 {
        let ticker = self.tick_state.lock().attack_strength_ticker() as f32;
        ((ticker + partial_tick) / attack_strength_delay).clamp(0.0, 1.0)
    }

    fn base_damage_scale_factor(attack_strength_scale: f32) -> f32 {
        0.2 + attack_strength_scale * attack_strength_scale * 0.8
    }

    fn get_knockback(
        attack_knockback: f64,
        weapon: &ItemStack,
        enchantment_context: &EnchantmentDamageContext<'_>,
    ) -> f64 {
        let modified = enchantment_helper::modify_knockback(
            weapon,
            enchantment_context,
            attack_knockback as f32,
        );
        f64::from(modified) / 2.0
    }

    fn cause_extra_knockback(
        &self,
        entity: &dyn Entity,
        knockback_amount: f64,
        old_movement: DVec3,
    ) {
        if knockback_amount > 0.0 {
            let yaw_radians = self.rotation().0.to_radians();
            let yaw_sin = f64::from(yaw_radians.sin());
            let yaw_cos = f64::from(yaw_radians.cos());
            if let Some(living_target) = entity.as_living_entity() {
                living_target.knockback(knockback_amount, yaw_sin, -yaw_cos);
            } else {
                entity.push_impulse(DVec3::new(
                    -yaw_sin * knockback_amount,
                    0.1,
                    yaw_cos * knockback_amount,
                ));
            }

            let velocity = self.velocity();
            self.set_velocity(DVec3::new(velocity.x * 0.6, velocity.y, velocity.z * 0.6));
            self.set_sprinting(false);
        }

        if entity.entity_type() == &vanilla_entities::PLAYER
            && entity.hurt_marked()
            && let Some(player) = self.get_world().players.get_by_entity_id(entity.id())
        {
            let velocity = entity.velocity();
            player.send_packet(CSetEntityMotion::new(entity.id(), velocity));
            entity.clear_hurt_mark();
            entity.set_velocity(old_movement);
        }
    }

    fn entity_interaction_range(&self) -> f64 {
        self.attributes()
            .lock()
            .required_value(vanilla_attributes::ENTITY_INTERACTION_RANGE)
    }

    /// Returns true if the target box is within the player's attack range for `item_stack`.
    #[must_use]
    pub fn is_within_attack_range_with_buffer(
        &self,
        item_stack: &ItemStack,
        aabb: WorldAabb,
        buffer: f64,
    ) -> bool {
        let distance = aabb.distance_to_sqr(self.eye_position()).sqrt();
        let (min_reach, max_reach, hitbox_margin) =
            if let Some(attack_range) = item_stack.get_attack_range() {
                if self.game_mode() == GameType::Creative {
                    (
                        attack_range.min_creative_reach,
                        attack_range.max_creative_reach,
                        attack_range.hitbox_margin,
                    )
                } else {
                    (
                        attack_range.min_reach,
                        attack_range.max_reach,
                        attack_range.hitbox_margin,
                    )
                }
            } else {
                (0.0, self.entity_interaction_range() as f32, 0.0)
            };
        let min_reach = f64::from(min_reach) - f64::from(hitbox_margin) - buffer;
        let max_reach = f64::from(max_reach) + f64::from(hitbox_margin) + buffer;
        distance >= min_reach && distance <= max_reach
    }

    /// Returns true if the target box is within the player's entity interaction range.
    #[must_use]
    pub fn is_within_entity_interaction_range_with_buffer(
        &self,
        aabb: WorldAabb,
        buffer: f64,
    ) -> bool {
        let max_range = self.entity_interaction_range() + buffer;
        aabb.distance_to_sqr(self.eye_position()) <= max_range * max_range
    }

    fn attack_range_for_item(&self, item_stack: &ItemStack) -> (f64, f64, f64) {
        let Some(attack_range) = item_stack.get_attack_range() else {
            return (0.0, self.entity_interaction_range(), 0.0);
        };

        let (min_reach, max_reach) = if self.game_mode() == GameType::Creative {
            (
                attack_range.min_creative_reach,
                attack_range.max_creative_reach,
            )
        } else {
            (attack_range.min_reach, attack_range.max_reach)
        };
        (
            f64::from(min_reach),
            f64::from(max_reach),
            f64::from(attack_range.hitbox_margin),
        )
    }

    fn piercing_hit_entities(&self, item_stack: &ItemStack, world: &World) -> Vec<SharedEntity> {
        let look = self.look_angle();
        if look.length_squared() <= f64::EPSILON {
            return Vec::new();
        }

        let (min_reach, max_reach, hitbox_margin) = self.attack_range_for_item(item_stack);
        let eye_position = self.eye_position();
        let from = eye_position + look * min_reach;
        let movement_extension = self.known_movement().dot(look).max(0.0);
        let mut to = eye_position + look * (max_reach + movement_extension);

        let block_hit = world.clip(eye_position, to, ClipBlockShape::Collider, ClipFluid::None);
        if !block_hit.is_miss() {
            to = block_hit.location;
            if eye_position.distance_squared(to) < eye_position.distance_squared(from) {
                return Vec::new();
            }
        }

        let search_area = WorldAabb::new(from.x, from.y, from.z, from.x, from.y, from.z)
            .inflate_xyz(hitbox_margin, hitbox_margin, hitbox_margin)
            .expand_towards(to - from)
            .inflate(1.0);
        let mut hits = world
            .get_entities_in_aabb_matching(&search_area, |entity| {
                self.can_piercing_hit_entity(entity)
            })
            .into_iter()
            .filter_map(|entity| {
                piercing_ray_hit_t(world, entity.bounding_box(), from, to, hitbox_margin)
                    .map(|hit_t| (hit_t, entity))
            })
            .collect::<Vec<_>>();
        hits.sort_by(|(left, _), (right, _)| left.total_cmp(right));
        hits.into_iter().map(|(_, entity)| entity).collect()
    }

    fn can_piercing_hit_entity(&self, target: &dyn Entity) -> bool {
        target.id() != self.id()
            && !target.is_invulnerable()
            && target.is_alive()
            && target.can_be_hit_by_projectile()
            && !self.is_passenger_of_same_vehicle(target)
    }

    pub(super) fn piercing_attack(&self, item_stack: &ItemStack, piercing_weapon: &PiercingWeapon) {
        let world = self.get_world();
        let base_damage = self
            .attributes()
            .lock()
            .required_value(vanilla_attributes::ATTACK_DAMAGE) as f32;
        let mut hit_something = false;
        for target in self.piercing_hit_entities(item_stack, &world) {
            hit_something |= self.stab_attack(
                &target,
                base_damage,
                true,
                piercing_weapon.deals_knockback,
                piercing_weapon.dismounts,
                InteractionHand::MainHand,
            );
        }

        self.reset_attack_strength_ticker();
        enchantment_helper::do_post_piercing_attack_effects(&world, self);
        if hit_something {
            self.play_hit_sound_holder(piercing_weapon.hit_sound.as_ref());
        }
        self.play_sound_holder(piercing_weapon.sound.as_ref());
        self.swing(InteractionHand::MainHand, false);
    }

    /// Runs one tick of a raised kinetic weapon -- a spear.
    ///
    /// Vanilla parity: `KineticWeapon.damageEntities`, reached from
    /// `ItemStack.onUseTick`. A spear does not swing: it is held out, and what
    /// decides whether it hurts anything is how fast the two of you are
    /// closing. The component was fully parsed, serialized, hashed and tested
    /// before this existed and nothing ever called it, so a player charging on
    /// an elytra passed straight through what they aimed at.
    ///
    /// Every threshold below is read from the item rather than assumed. A
    /// copper spear, for instance, waits 13 ticks before it can do anything at
    /// all and wants 4.6 blocks per second of *closing* speed to draw blood --
    /// which is why walking into a mob with one does nothing and flying into
    /// one does.
    pub(crate) fn kinetic_weapon_damage_entities(
        &self,
        stack: &ItemStack,
        kinetic: &KineticWeapon,
        ticks_remaining: i32,
        hand: InteractionHand,
    ) {
        let use_duration = ITEM_BEHAVIORS
            .get_behavior(stack.item())
            .get_use_duration(stack, self);
        let ticks_used = use_duration - ticks_remaining - kinetic.delay_ticks();
        if ticks_used < 0 {
            return;
        }

        let world = self.get_world();
        let look = self.look_angle();
        let attacker_speed = look.dot(kinetic_motion(self, &world));
        // Vanilla parity: `livingEntity instanceof Player ? 1.0F : 0.2F`. A mob
        // is held to a fifth of the speed a player is, because it cannot fly.
        // Foton only reaches this from a player, so the factor is the player's.
        let action_factor = 1.0;
        let base_damage = self
            .attributes()
            .lock()
            .required_value(vanilla_attributes::ATTACK_DAMAGE);

        let game_time = world.game_time();
        let cooldown = kinetic.contact_cooldown_ticks();
        let mut affected = false;

        for target in self.piercing_hit_entities(stack, &world) {
            // One thrust, one hit per target. Without this the spear would
            // damage whatever it is inside on every tick of the charge.
            if self
                .living_base()
                .was_recently_stabbed(target.id(), game_time, cooldown)
            {
                continue;
            }
            self.living_base()
                .remember_stabbed_entity(target.id(), game_time);

            let target_speed = look.dot(kinetic_motion(target.as_ref(), &world));
            let relative_speed = (attacker_speed - target_speed).max(0.0);
            let holds = |condition: Option<&KineticWeaponCondition>| {
                condition.is_some_and(|condition| {
                    ticks_used <= condition.max_duration_ticks()
                        && attacker_speed >= f64::from(condition.min_speed()) * action_factor
                        && relative_speed
                            >= f64::from(condition.min_relative_speed()) * action_factor
                })
            };
            let dismounts = holds(kinetic.dismount_conditions());
            let deals_knockback = holds(kinetic.knockback_conditions());
            let deals_damage = holds(kinetic.damage_conditions());
            if !dismounts && !deals_knockback && !deals_damage {
                continue;
            }

            // Vanilla floors the speed term on its own, before the attribute is
            // added: `baseMobDamage + Mth.floor(relativeSpeed * multiplier)`.
            let damage = base_damage as f32
                + (relative_speed * f64::from(kinetic.damage_multiplier())).floor() as f32;
            affected |= self.stab_attack(
                &target,
                damage,
                deals_damage,
                deals_knockback,
                dismounts,
                hand,
            );
        }

        if affected {
            // The client draws the recoil off this event; without it a landed
            // hit looks identical to a miss.
            self.broadcast_entity_event(EntityStatus::KineticHit);
            let speared = self
                .living_base()
                .stabbed_entity_ids()
                .into_iter()
                .filter(|id| {
                    world
                        .get_entity_by_id(*id)
                        .is_some_and(|entity| entity.as_living_entity().is_some())
                })
                .count();
            triggers::item::speared_mobs(self, speared as i32);
        }
    }

    fn stab_attack(
        &self,
        target: &SharedEntity,
        base_damage: f32,
        deals_damage: bool,
        deals_knockback: bool,
        dismounts: bool,
        hand: InteractionHand,
    ) -> bool {
        let entity = target.as_ref();
        if self.cannot_attack(entity) {
            return false;
        }

        let attacking_item = {
            let inventory = self.inventory.lock();
            let stack = inventory.get_item_in_hand(hand);
            stack.copy_with_count(stack.count())
        };
        let damage_source = self.attack_damage_source(&attacking_item);
        let enchantment_context = EnchantmentDamageContext::new(
            entity.entity_type(),
            Some(self.entity_type()),
            Some(self.entity_type()),
            &damage_source,
        );
        let enchanted_damage =
            enchantment_helper::modify_damage(&attacking_item, &enchantment_context, base_damage);
        // Vanilla parity: `Player.stabAttack` applies the attack-strength
        // cooldown only when the weapon is *not* the one being held up. A
        // spear thrust is not a swing, so the cooldown has nothing to say
        // about it -- scaling it there would make a charge at full speed land
        // for a fraction of its damage.
        let (base_damage, magic_boost) = if self.active_item_use_hand() == Some(hand) {
            (base_damage, enchanted_damage - base_damage)
        } else {
            let attack_strength_scale = self.attack_strength_scale(0.5);
            (
                base_damage * Self::base_damage_scale_factor(attack_strength_scale),
                attack_strength_scale * (enchanted_damage - base_damage),
            )
        };
        let damage = base_damage + magic_boost;
        let old_movement = entity.velocity();
        let mut affected = deals_knockback;
        let mut damage_allowed = true;
        if deals_damage {
            let mut event = crate::event::EntityDamageByEntityEvent::new(self.uuid(), entity.uuid(), "ENTITY_ATTACK".to_owned());
            self.fire_event(&mut event);
            damage_allowed = !event.is_cancelled();
        }
        let damage_dealt = deals_damage && damage_allowed
            && entity.level().is_some_and(|world| entity.hurt(&world, &damage_source, damage));
        affected |= damage_dealt;
        if deals_knockback {
            self.cause_extra_knockback(
                entity,
                0.4 + Self::get_knockback(0.0, &attacking_item, &enchantment_context),
                old_movement,
            );
        }
        if dismounts && entity.is_passenger() {
            affected = true;
            entity.stop_riding();
        }

        if !affected {
            return false;
        }

        self.item_attack_interaction(entity, &damage_source, damage_dealt);
        self.set_last_hurt_mob(Some(target));
        self.cause_food_exhaustion(0.1);
        true
    }

    /// Vanilla parity: `PiercingWeapon.makeSound`, which excludes the attacker.
    ///
    /// The swing is something the attacker's own client makes, so sending it
    /// back would be the second copy.
    pub(crate) fn play_sound_holder(&self, holder: Option<&SoundEventHolder>) {
        let Some(sound) = holder.and_then(sound_holder_ref) else {
            return;
        };
        self.play_sound(sound, 1.0, 1.0);
    }

    /// Vanilla parity: `PiercingWeapon.makeHitSound`, which excludes nobody.
    ///
    /// The two differ by one argument in vanilla and it is deliberate: a hit
    /// is confirmation the server sends back, and an attacker who never hears
    /// it cannot tell a landed hit from a missed one.
    fn play_hit_sound_holder(&self, holder: Option<&SoundEventHolder>) {
        let Some(sound) = holder.and_then(sound_holder_ref) else {
            return;
        };
        self.play_server_side_sound(sound, 1.0, 1.0);
    }

    fn cannot_attack(&self, entity: &dyn Entity) -> bool {
        !entity.attackable() || entity.skip_attack_interaction(self)
    }

    /// Attacks an entity with the player's main-hand base damage.
    ///
    /// Returns `true` if the target accepted damage.
    #[must_use]
    pub fn attack(&self, target: &SharedEntity) -> bool {
        let entity = target.as_ref();
        if self.cannot_attack(entity) {
            return false;
        }

        let attacking_item = {
            let inventory = self.inventory.lock();
            let stack = inventory.get_item_in_hand(InteractionHand::MainHand);
            stack.copy_with_count(stack.count())
        };
        let (attack_damage, attack_speed, attack_knockback) = {
            let attributes = self.attributes().lock();
            (
                attributes.required_value(vanilla_attributes::ATTACK_DAMAGE) as f32,
                attributes.required_value(vanilla_attributes::ATTACK_SPEED),
                attributes.required_value(vanilla_attributes::ATTACK_KNOCKBACK),
            )
        };
        let attack_strength_delay = Self::attack_strength_delay_from_speed(attack_speed);
        let attack_strength_scale =
            self.attack_strength_scale_for_delay(0.5, attack_strength_delay);
        let damage_source = self.attack_damage_source(&attacking_item);
        let enchantment_context = EnchantmentDamageContext::new(
            entity.entity_type(),
            Some(self.entity_type()),
            Some(self.entity_type()),
            &damage_source,
        );
        let enchanted_damage =
            enchantment_helper::modify_damage(&attacking_item, &enchantment_context, attack_damage);
        let magic_boost = attack_strength_scale * (enchanted_damage - attack_damage);
        let mut base_damage = attack_damage * Self::base_damage_scale_factor(attack_strength_scale);
        base_damage += ITEM_BEHAVIORS
            .get_behavior(attacking_item.item())
            .get_attack_damage_bonus(self, entity, base_damage, &damage_source);
        self.reset_attack_strength_ticker();

        let world = self.get_world();
        if base_damage <= 0.0 && magic_boost <= 0.0 {
            enchantment_helper::do_post_piercing_attack_effects(&world, self);
            return false;
        }

        let full_strength_attack = attack_strength_scale > 0.9;
        let knockback_attack = self.is_sprinting() && full_strength_attack;
        if knockback_attack {
            self.play_server_side_sound(&sound_events::ENTITY_PLAYER_ATTACK_KNOCKBACK, 1.0, 1.0);
        }

        let critical_attack = full_strength_attack && self.can_critical_attack(entity);
        if critical_attack {
            base_damage *= 1.5;
        }
        let total_damage = base_damage + magic_boost;
        let sweep_attack =
            self.is_sweep_attack(full_strength_attack, critical_attack, knockback_attack);

        let old_health = entity
            .as_living_entity()
            .map_or(0.0, LivingEntity::get_health);
        let old_movement = entity.velocity();
        let Some(target_world) = entity.level() else {
            enchantment_helper::do_post_piercing_attack_effects(&world, self);
            return false;
        };
        let mut event = crate::event::EntityDamageByEntityEvent::new(self.uuid(), entity.uuid(), "ENTITY_ATTACK".to_owned());
        self.fire_event(&mut event);
        let damage_allowed = !event.is_cancelled();
        let was_hurt = damage_allowed && entity.hurt(&target_world, &damage_source, total_damage);
        if was_hurt {
            let sprint_knockback = if knockback_attack { 0.5 } else { 0.0 };
            self.cause_extra_knockback(
                entity,
                Self::get_knockback(attack_knockback, &attacking_item, &enchantment_context)
                    + sprint_knockback,
                old_movement,
            );
            if sweep_attack {
                self.do_sweep_attack(
                    entity,
                    &attacking_item,
                    base_damage,
                    &damage_source,
                    attack_strength_scale,
                );
            }
            self.attack_visual_effects(
                entity,
                critical_attack,
                sweep_attack,
                full_strength_attack,
                magic_boost,
            );
            self.set_last_hurt_mob(Some(target));
            self.item_attack_interaction(entity, &damage_source, true);
            self.show_damage_indicators(entity, old_health);
            self.cause_food_exhaustion(0.1);
        } else {
            self.play_server_side_sound(&sound_events::ENTITY_PLAYER_ATTACK_NODAMAGE, 1.0, 1.0);
        }

        enchantment_helper::do_post_piercing_attack_effects(&world, self);
        was_hurt
    }

    /// Vanilla parity: `Player.isMobilityRestricted`, which in 26.2 is
    /// blindness alone -- slowness has not gated crits since the combat rework.
    fn is_mobility_restricted(&self) -> bool {
        self.mob_effect(vanilla_mob_effects::BLINDNESS).is_some()
    }

    /// Vanilla parity: `Player.canCriticalAttack`.
    fn can_critical_attack(&self, entity: &dyn Entity) -> bool {
        self.fall_distance() > 0.0
            && !self.on_ground()
            && !self.on_climbable()
            && !self.is_in_water()
            && !self.is_mobility_restricted()
            && !self.is_passenger()
            && entity.as_living_entity().is_some()
            && !self.is_sprinting()
    }

    /// Vanilla parity: `Player.isSweepAttack`.
    ///
    /// 26.2 no longer asks for Sweeping Edge: anything in `#minecraft:swords`
    /// sweeps, and the enchantment only feeds `SWEEPING_DAMAGE_RATIO`.
    fn is_sweep_attack(
        &self,
        full_strength_attack: bool,
        critical_attack: bool,
        knockback_attack: bool,
    ) -> bool {
        if !full_strength_attack || critical_attack || knockback_attack || !self.on_ground() {
            return false;
        }

        let movement = self.known_movement();
        let approximate_speed_sq = movement.x.mul_add(movement.x, movement.z * movement.z);
        let max_speed_for_sweep_attack = f64::from(self.get_speed()) * 2.5;
        if approximate_speed_sq >= max_speed_for_sweep_attack * max_speed_for_sweep_attack {
            return false;
        }

        let inventory = self.inventory.lock();
        inventory
            .get_item_in_hand(InteractionHand::MainHand)
            .item()
            .has_tag(&ItemTag::SWORDS)
    }

    /// Vanilla parity: `Player.attackVisualEffects`, minus the stab flag that
    /// only a piercing weapon sets.
    fn attack_visual_effects(
        &self,
        entity: &dyn Entity,
        critical_attack: bool,
        sweep_attack: bool,
        full_strength_attack: bool,
        magic_boost: f32,
    ) {
        if critical_attack {
            self.play_server_side_sound(&sound_events::ENTITY_PLAYER_ATTACK_CRIT, 1.0, 1.0);
            self.send_attack_animation(entity, AnimateAction::CriticalHit);
        }

        if !critical_attack && !sweep_attack {
            self.play_server_side_sound(
                if full_strength_attack {
                    &sound_events::ENTITY_PLAYER_ATTACK_STRONG
                } else {
                    &sound_events::ENTITY_PLAYER_ATTACK_WEAK
                },
                1.0,
                1.0,
            );
        }

        if magic_boost > 0.0 {
            self.send_attack_animation(entity, AnimateAction::MagicCriticalHit);
        }
    }

    /// Vanilla parity: `ServerPlayer.crit` and `ServerPlayer.magicCrit`, which
    /// both go to the attacker's trackers and to the attacker itself.
    fn send_attack_animation(&self, entity: &dyn Entity, action: AnimateAction) {
        let packet = CAnimate::new(entity.id(), action);
        self.get_world()
            .broadcast_to_entity_trackers(self.id(), packet.clone(), None);
        self.send_packet(packet);
    }

    /// Vanilla parity: `Player.doSweepAttack`.
    fn do_sweep_attack(
        &self,
        entity: &dyn Entity,
        attacking_item: &ItemStack,
        base_damage: f32,
        damage_source: &DamageSource,
        attack_strength_scale: f32,
    ) {
        self.play_server_side_sound(&sound_events::ENTITY_PLAYER_ATTACK_SWEEP, 1.0, 1.0);

        let world = self.get_world();
        let sweeping_damage_ratio = self
            .attributes()
            .lock()
            .get_value(vanilla_attributes::SWEEPING_DAMAGE_RATIO)
            .unwrap_or(0.0) as f32;
        let sweep_damage = sweeping_damage_ratio.mul_add(base_damage, 1.0);

        let yaw_radians = self.rotation().0.to_radians();
        let yaw_sin = f64::from(yaw_radians.sin());
        let yaw_cos = f64::from(yaw_radians.cos());
        let position = self.position();

        let area = entity.bounding_box().inflate_xyz(1.0, 0.25, 1.0);
        for shared in world.get_entities_in_aabb(&area) {
            let nearby = shared.as_ref();
            if nearby.id() == self.id()
                || nearby.id() == entity.id()
                || self.is_allied_to(nearby)
                || nearby.is_marker_armor_stand()
                || position.distance_squared(nearby.position()) >= 9.0
            {
                continue;
            }
            let Some(living) = nearby.as_living_entity() else {
                continue;
            };

            let context = EnchantmentDamageContext::new(
                nearby.entity_type(),
                Some(self.entity_type()),
                Some(self.entity_type()),
                damage_source,
            );
            let enchanted_damage =
                enchantment_helper::modify_damage(attacking_item, &context, sweep_damage)
                    * attack_strength_scale;
            if !living.hurt(&world, damage_source, enchanted_damage) {
                continue;
            }
            living.knockback(0.4, yaw_sin, -yaw_cos);
            let post_attack_context =
                EnchantmentPostAttackContext::new(nearby, Some(self), Some(self), damage_source);
            enchantment_helper::do_post_attack_effects_from_item(
                &world,
                attacking_item,
                &post_attack_context,
            );
        }

        world.send_particles(
            ParticleData::simple(&vanilla_particle_types::SWEEP_ATTACK),
            DVec3::new(
                position.x - yaw_sin,
                entity_y_at(self, 0.5),
                position.z + yaw_cos,
            ),
            0,
            DVec3::new(-yaw_sin, 0.0, yaw_cos),
            0.0,
        );
    }

    /// Vanilla parity: the particle half of `Player.damageStatsAndHearts`.
    ///
    /// Foton has no statistics registry yet, so the `DAMAGE_DEALT` award is
    /// left out; the damage indicators are what a player actually sees.
    fn show_damage_indicators(&self, entity: &dyn Entity, old_health: f32) {
        let Some(living) = entity.as_living_entity() else {
            return;
        };
        let actual_damage = old_health - living.get_health();
        if actual_damage <= 2.0 {
            return;
        }

        let position = entity.position();
        self.get_world().send_particles(
            ParticleData::simple(&vanilla_particle_types::DAMAGE_INDICATOR),
            DVec3::new(position.x, entity_y_at(entity, 0.5), position.z),
            (actual_damage * 0.5) as i32,
            DVec3::new(0.1, 0.0, 0.1),
            0.2,
        );
    }

    fn item_attack_interaction(
        &self,
        entity: &dyn Entity,
        damage_source: &DamageSource,
        apply_to_target: bool,
    ) {
        let post_attack_context =
            EnchantmentPostAttackContext::new(entity, Some(self), Some(self), damage_source);
        let (source_item, item_hurt_enemy) = {
            let mut inventory = self.inventory.lock();
            inventory.mutate_item_in_hand(InteractionHand::MainHand, |stack| {
                if stack.is_empty() {
                    return (ItemStack::empty(), false);
                }
                let behavior = ITEM_BEHAVIORS.get_behavior(stack.item());
                if let Some(living_target) = entity.as_living_entity() {
                    behavior.hurt_enemy(stack, living_target, self);
                }
                let source_item = stack.copy_with_count(stack.count());
                (source_item, stack.get_weapon().is_some())
            })
        };

        if apply_to_target {
            let world = self.get_world();
            enchantment_helper::do_post_attack_effects_with_item_source(
                &world,
                entity,
                &source_item,
                &post_attack_context,
            );
        }

        if !item_hurt_enemy {
            return;
        }

        let Some(living_target) = entity.as_living_entity() else {
            return;
        };
        let has_infinite_materials = self.has_infinite_materials();
        let weapon_broke = {
            let mut inventory = self.inventory.lock();
            inventory.mutate_item_in_hand(InteractionHand::MainHand, |stack| {
                if stack.is_empty() {
                    return false;
                }
                let behavior = ITEM_BEHAVIORS.get_behavior(stack.item());
                behavior.post_hurt_enemy(stack, living_target, self);
                behavior
                    .item_damage_per_attack(stack)
                    .is_some_and(|damage| stack.hurt_and_break(damage, has_infinite_materials))
            })
        };
        // Vanilla's `hurtAndBreak` takes the attacker and announces the break
        // itself; Foton's item stacks cannot reach one, so the sword's last
        // swing is announced here instead.
        if weapon_broke {
            LivingEntity::on_equipped_item_broken(self, EquipmentSlot::MainHand);
        }
    }

    /// Interacts with an entity using the held item.
    pub fn interact_on(
        &self,
        entity: &dyn Entity,
        hand: InteractionHand,
        location: DVec3,
    ) -> InteractionResult {
        if self.is_spectator() {
            // TODO: Open entity menu providers in spectator once that foundation exists.
            return InteractionResult::Pass;
        }

        let inventory_access = InventoryAccess::new(self.inventory.clone(), hand);
        let original_count = inventory_access.with_item(|item| item.count);
        let result = entity.interact(self, hand, location);

        if self.has_infinite_materials() {
            inventory_access.with_item(|item| {
                if item.count < original_count {
                    item.count = original_count;
                }
            });
        }

        if result.consumes_action() {
            return result;
        }

        if inventory_access.with_item(|item| item.is_empty()) {
            return InteractionResult::Pass;
        }
        let Some(living_entity) = entity.as_living_entity() else {
            return InteractionResult::Pass;
        };
        let result = living_entity.interact_living_entity_with_equippable(self, hand);
        if self.has_infinite_materials() {
            inventory_access.with_item(|item| {
                if item.count < original_count {
                    item.count = original_count;
                }
            });
        }
        if result.consumes_action() {
            return result;
        }

        let item_ref = inventory_access.with_item(|item| item.item());
        let item_behavior = ITEM_BEHAVIORS.get_behavior(item_ref);
        let result = inventory_access.with_item(|item| {
            item_behavior.interact_living_entity(item, self, living_entity, hand)
        });
        if self.has_infinite_materials() {
            inventory_access.with_item(|item| {
                if item.count < original_count {
                    item.count = original_count;
                }
            });
        }
        result
    }

    /// Handles a client request to attack an entity.
    pub fn handle_attack(&self, packet: SAttack) {
        if !self.has_client_loaded() || self.is_spectator() {
            return;
        }

        let world = self.get_world();
        // Vanilla parity: `level.getEntityOrPart(packet.entityId())`. A hit on
        // the ender dragon names one of its hitboxes, never the dragon itself.
        let Some(target) = world.get_accessible_entity_or_part_by_id(packet.entity_id) else {
            return;
        };

        let target_pos = target.block_position();
        if !world.world_border_snapshot().is_within_bounds_with_margin(
            f64::from(target_pos.x()),
            f64::from(target_pos.z()),
            0.0,
        ) {
            return;
        }

        let main_hand_item = {
            let inventory = self.inventory.lock();
            let stack = inventory.get_item_in_hand(InteractionHand::MainHand);
            stack.copy_with_count(stack.count())
        };

        if !self.is_within_attack_range_with_buffer(
            &main_hand_item,
            target.bounding_box(),
            ATTACK_RANGE_BUFFER,
        ) {
            return;
        }

        if main_hand_item.get_piercing_weapon().is_some() {
            return;
        }

        if Self::is_invalid_attack_target(self.id(), target.id(), target.entity_type()) {
            self.disconnect(Self::invalid_entity_attacked_message());
            log::warn!(
                "Player {} tried to attack an invalid entity",
                self.gameprofile.name
            );
            return;
        }

        if self.cannot_attack_with_item(&main_hand_item, 5) {
            return;
        }

        let _ = self.attack(&target);
    }

    pub(super) fn cannot_attack_with_item(&self, item_stack: &ItemStack, tolerance: i32) -> bool {
        let required_strength = item_stack.minimum_attack_charge();
        if required_strength <= 0.0 {
            return false;
        }

        let optimistic_strength = {
            let ticker = self.tick_state.lock().attack_strength_ticker() + tolerance;
            ticker as f32 / self.current_item_attack_strength_delay()
        };
        optimistic_strength < required_strength
    }

    pub(super) fn is_invalid_attack_target(
        player_id: i32,
        target_id: i32,
        target_type: EntityTypeRef,
    ) -> bool {
        target_id == player_id
            || target_type == &vanilla_entities::ITEM
            || target_type == &vanilla_entities::EXPERIENCE_ORB
    }

    /// Handles a client request to interact with an entity.
    pub fn handle_interact(&self, packet: SInteract) {
        if !self.has_client_loaded() {
            return;
        }

        let world = self.get_world();
        // Vanilla parity: `level.getEntityOrPart(packet.entityId())`.
        let target = world.get_accessible_entity_or_part_by_id(packet.entity_id);
        self.set_crouching(packet.using_secondary_action);
        let Some(target) = target else {
            return;
        };

        let target_pos = target.block_position();
        if !world.world_border_snapshot().is_within_bounds_with_margin(
            f64::from(target_pos.x()),
            f64::from(target_pos.z()),
            0.0,
        ) {
            return;
        }

        if !self.is_within_entity_interaction_range_with_buffer(
            target.bounding_box(),
            ENTITY_INTERACTION_RANGE_BUFFER,
        ) {
            return;
        }

        let result = self.interact_on(target.as_ref(), packet.hand, packet.location);
        if result.should_swing_server() {
            self.swing(packet.hand, true);
        }
        self.broadcast_inventory_changes();
    }
}

/// The melee attack a player performs every few seconds, entered where the
/// client enters it: `Player::handle_attack` with a `SAttack` packet.
///
/// Going in through the packet is the point. Foton already had crit-shaped
/// helpers that nothing called; a test that pokes `Player::attack` directly
/// would pass for either version of the code.
/// Returns how fast an entity is moving, in blocks per second.
///
/// Vanilla parity: `KineticWeapon.getMotion`. The scale by twenty is what
/// turns a per-tick velocity into the units every threshold on the component
/// is written in, and the root-vehicle step means a spear measures the horse's
/// charge rather than the rider's shuffle on its back. A player is exempt from
/// that step because their own reported speed already covers the whole ride.
fn kinetic_motion(entity: &dyn Entity, world: &Arc<World>) -> DVec3 {
    if entity.as_player().is_none()
        && entity.is_passenger()
        && let Some(root) = world.get_entity_by_id(entity.root_vehicle_id())
    {
        return root.known_speed() * 20.0;
    }
    entity.known_speed() * 20.0
}

#[cfg(test)]
mod melee_tests {
    use std::io::Cursor;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use foton_protocol::packet_traits::{CompressionInfo, EncodedPacket};
    use foton_protocol::packets::game::SAttack;
    use foton_registry::packets::play::{C_ANIMATE, C_ENTITY_EVENT};
    use foton_registry::{init_vanilla_registry, vanilla_entities, vanilla_items};
    use foton_utils::codec::VarInt;
    use foton_utils::entity_events::EntityStatus;
    use foton_utils::locks::SyncMutex;
    use foton_utils::serial::ReadFrom as _;
    use foton_utils::types::InteractionHand;
    use glam::DVec3;
    use text_components::TextComponent;

    use crate::behavior::init_behaviors;
    use crate::chunk::player_chunk_view::PlayerChunkView;
    use crate::entity::entities::PigEntity;
    use crate::entity::{Entity, LivingEntity, SharedEntity, next_entity_id};
    use crate::player::connection::NetworkConnection;
    use crate::player::{Player, PlayerConnection, ResetReason};
    use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};
    use crate::world::World;
    use foton_registry::item_stack::ItemStack;
    use foton_utils::ChunkPos;

    /// Ticks of charge that fill the attack strength meter for any vanilla
    /// weapon: the slowest is the mace at 0.5 attacks per second, i.e. 40.
    const FULL_CHARGE_TICKS: i32 = 40;

    /// Vanilla `Player.aiStep` puts the movement speed attribute here.
    const WALKING_SPEED: f32 = 0.1;

    #[derive(Default)]
    struct RecordingConnection {
        sent: Arc<SyncMutex<Vec<EncodedPacket>>>,
        closed: AtomicBool,
    }

    impl NetworkConnection for RecordingConnection {
        fn compression(&self) -> Option<CompressionInfo> {
            None
        }

        fn send_encoded(&self, packet: EncodedPacket) {
            self.sent.lock().push(packet);
        }

        fn send_encoded_bundle(&self, packets: Vec<EncodedPacket>) {
            self.sent.lock().extend(packets);
        }

        fn disconnect_with_reason(&self, _reason: TextComponent) {}

        fn tick(&self) {}

        fn latency(&self) -> i32 {
            0
        }

        fn close(&self) {
            self.closed.store(true, Ordering::Release);
        }

        fn closed(&self) -> bool {
            self.closed.load(Ordering::Acquire)
        }
    }

    /// Reads back the `(entity_id, action)` of a clientbound animate packet.
    fn animate_payload(packet: &EncodedPacket) -> Option<(i32, u8)> {
        let mut cursor = Cursor::new(packet.encoded_data.as_slice());
        VarInt::read(&mut cursor).ok()?;
        let packet_id = VarInt::read(&mut cursor).ok()?;
        if packet_id.0 != C_ANIMATE {
            return None;
        }
        let entity_id = VarInt::read(&mut cursor).ok()?;
        let action = u8::read(&mut cursor).ok()?;
        Some((entity_id.0, action))
    }

    /// Reads back the `(entity_id, status)` of a clientbound entity-event packet.
    fn entity_event_payload(packet: &EncodedPacket) -> Option<(i32, i32)> {
        let mut cursor = Cursor::new(packet.encoded_data.as_slice());
        VarInt::read(&mut cursor).ok()?;
        let packet_id = VarInt::read(&mut cursor).ok()?;
        if packet_id.0 != C_ENTITY_EVENT {
            return None;
        }
        let entity_id = i32::read(&mut cursor).ok()?;
        let status = VarInt::read(&mut cursor).ok()?;
        Some((entity_id, status.0))
    }

    fn combat_world(key: &'static str) -> Arc<World> {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world(key);
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        world
    }

    fn spawn_pig(world: &Arc<World>, position: DVec3) -> Arc<PigEntity> {
        let pig = Arc::new(PigEntity::new(
            &vanilla_entities::PIG,
            next_entity_id(),
            position,
            Arc::downgrade(world),
        ));
        world
            .try_add_entity(Arc::clone(&pig) as SharedEntity)
            .expect("the test chunk is loaded, so the pig should attach");
        pig
    }

    struct Attacker {
        player: Arc<Player>,
        sent: Arc<SyncMutex<Vec<EncodedPacket>>>,
    }

    fn attacker(world: &Arc<World>) -> Attacker {
        let sent = Arc::new(SyncMutex::new(Vec::new()));
        let connection = Arc::new(PlayerConnection::Other(Box::new(RecordingConnection {
            sent: Arc::clone(&sent),
            closed: AtomicBool::new(false),
        })));
        let player = TestPlayerBuilder::new(Arc::clone(world), "Attacker", next_entity_id())
            .connection(connection)
            .build();
        player.set_on_ground(true);
        player.set_speed(WALKING_SPEED);
        Attacker { player, sent }
    }

    impl Attacker {
        /// Fills the attack strength meter and clears the recorded packets, so
        /// each swing is judged on its own.
        fn ready(&self) {
            for _ in 0..FULL_CHARGE_TICKS {
                self.player.tick_attack_strength();
            }
            self.sent.lock().clear();
        }

        fn swing_at(&self, target: &dyn Entity) {
            self.ready();
            self.player.handle_attack(SAttack {
                entity_id: target.id(),
            });
        }

        fn animations(&self) -> Vec<(i32, u8)> {
            self.sent
                .lock()
                .iter()
                .filter_map(animate_payload)
                .collect()
        }

        fn entity_events(&self) -> Vec<(i32, i32)> {
            self.sent
                .lock()
                .iter()
                .filter_map(entity_event_payload)
                .collect()
        }
    }

    /// Puts the attacker in the air, mid-fall, which is the whole of vanilla's
    /// `canCriticalAttack` a test can arrange.
    fn start_falling(player: &Player) {
        player.set_on_ground(false);
        player.set_fall_distance(1.0);
    }

    fn damage_taken(pig: &PigEntity, before: f32) -> f32 {
        before - pig.get_health()
    }

    #[test]
    fn a_falling_hit_deals_half_as_much_damage_again() {
        let world = combat_world("melee_crit_damage");
        let attacker = attacker(&world);
        let position = attacker.player.position();

        let grounded_target = spawn_pig(&world, position + DVec3::new(1.0, 0.0, 0.0));
        let falling_target = spawn_pig(&world, position + DVec3::new(2.2, 0.0, 0.0));

        let grounded_before = grounded_target.get_health();
        attacker.swing_at(grounded_target.as_ref());
        let plain = damage_taken(&grounded_target, grounded_before);
        assert!(plain > 0.0, "the plain hit landed no damage at all");

        start_falling(&attacker.player);
        let falling_before = falling_target.get_health();
        attacker.swing_at(falling_target.as_ref());
        let critical = damage_taken(&falling_target, falling_before);

        assert!(
            (critical - plain * 1.5).abs() < 1.0e-4,
            "a critical hit should be 1.5x a plain one, got {critical} against {plain}"
        );
    }

    #[test]
    fn a_critical_hit_animates_the_target_for_the_attacker() {
        let world = combat_world("melee_crit_animation");
        let attacker = attacker(&world);
        let position = attacker.player.position();
        let target = spawn_pig(&world, position + DVec3::new(1.0, 0.0, 0.0));

        attacker.swing_at(target.as_ref());
        assert!(
            !attacker.animations().contains(&(target.id(), 4)),
            "a hit taken with both feet on the ground is not a critical hit"
        );

        start_falling(&attacker.player);
        attacker.swing_at(target.as_ref());
        assert!(
            attacker.animations().contains(&(target.id(), 4)),
            "a falling hit should send the critical-hit animation, got {:?}",
            attacker.animations()
        );
    }

    #[test]
    fn a_sword_sweep_reaches_the_neighbour() {
        let world = combat_world("melee_sweep_reaches");
        let attacker = attacker(&world);
        let position = attacker.player.position();
        attacker.player.inventory.lock().set_item_in_hand(
            InteractionHand::MainHand,
            ItemStack::new(&vanilla_items::IRON_SWORD),
        );

        let target = spawn_pig(&world, position + DVec3::new(1.0, 0.0, 0.0));
        let bystander = spawn_pig(&world, position + DVec3::new(1.6, 0.0, 0.0));
        let bystander_before = bystander.get_health();

        attacker.swing_at(target.as_ref());

        assert!(
            damage_taken(&bystander, bystander_before) > 0.0,
            "a full-strength sword swing on the ground should sweep the neighbour"
        );
    }

    #[test]
    fn a_sprinting_sword_hit_does_not_sweep() {
        let world = combat_world("melee_sweep_sprint");
        let attacker = attacker(&world);
        let position = attacker.player.position();
        attacker.player.inventory.lock().set_item_in_hand(
            InteractionHand::MainHand,
            ItemStack::new(&vanilla_items::IRON_SWORD),
        );
        attacker.player.set_sprinting(true);

        let target = spawn_pig(&world, position + DVec3::new(1.0, 0.0, 0.0));
        let bystander = spawn_pig(&world, position + DVec3::new(1.6, 0.0, 0.0));
        let bystander_before = bystander.get_health();

        attacker.swing_at(target.as_ref());

        assert!(
            (damage_taken(&bystander, bystander_before)).abs() < f32::EPSILON,
            "a sprint hit is a knockback hit, and vanilla never sweeps on one"
        );
    }

    #[test]
    fn a_bare_handed_hit_does_not_sweep() {
        let world = combat_world("melee_sweep_needs_a_sword");
        let attacker = attacker(&world);
        let position = attacker.player.position();

        let target = spawn_pig(&world, position + DVec3::new(1.0, 0.0, 0.0));
        let bystander = spawn_pig(&world, position + DVec3::new(1.6, 0.0, 0.0));
        let bystander_before = bystander.get_health();

        attacker.swing_at(target.as_ref());

        assert!(
            (damage_taken(&bystander, bystander_before)).abs() < f32::EPSILON,
            "only an item in `#minecraft:swords` sweeps in 26.2"
        );
    }

    /// `Player.isSweepAttack` compares the known movement against
    /// `getSpeed() * 2.5`, and a player whose speed was never set fails that
    /// test at zero -- every sweep dies silently. Vanilla writes the movement
    /// speed attribute there once per `aiStep`.
    #[test]
    fn the_player_tick_publishes_the_movement_speed() {
        let world = combat_world("melee_ai_step_speed");
        let player = TestPlayerBuilder::new(world, "Walker", next_entity_id()).build();
        player.set_speed(0.0);

        let _ = LivingEntity::ai_step(player.as_ref());

        assert!(
            player.get_speed() > 0.0,
            "without this the sweep check compares against a zero speed and never fires"
        );
    }

    /// A weapon that dies on the swing has to say so.
    ///
    /// Vanilla's `hurtAndBreak` takes the holder and calls
    /// `onEquippedItemBroken` itself -- the snap and the splinters. Foton's
    /// item stacks cannot reach a holder, and the attack path threw the "it
    /// broke" answer away, so a sword simply vanished from the hand in silence.
    #[test]
    fn a_weapon_that_breaks_on_the_swing_announces_it() {
        let world = combat_world("melee_weapon_breaks");
        let attacker = attacker(&world);
        // The break is a broadcast, not a self-send, so the attacker only hears
        // it as one of the players tracking its own chunk.
        assert!(world.add_player(Arc::clone(&attacker.player), ResetReason::InitialJoin));
        let _ = attacker.player.mark_joined_world();
        attacker.player.set_client_loaded(true);
        attacker
            .player
            .chunk_sender
            .lock()
            .mark_chunk_sent_for_test(ChunkPos::new(0, 0));
        world.player_area_map.on_player_join(
            &attacker.player,
            &PlayerChunkView::new(ChunkPos::new(0, 0), 2),
        );
        let position = attacker.player.position();

        let mut sword = ItemStack::new(&vanilla_items::IRON_SWORD);
        sword.set_damage_value(sword.get_max_damage() - 1);
        attacker
            .player
            .inventory
            .lock()
            .set_item_in_hand(InteractionHand::MainHand, sword);

        let target = spawn_pig(&world, position + DVec3::new(1.0, 0.0, 0.0));
        attacker.swing_at(target.as_ref());

        assert!(
            attacker
                .player
                .inventory
                .lock()
                .get_item_in_hand(InteractionHand::MainHand)
                .is_empty(),
            "the sword was one point from the end, so the swing should have finished it"
        );
        assert!(
            attacker
                .entity_events()
                .contains(&(attacker.player.id(), EntityStatus::MainhandBreak as i32)),
            "the break should be broadcast, got {:?}",
            attacker.entity_events()
        );
    }
}
