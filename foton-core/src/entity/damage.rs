//! Damage source system.

use foton_registry::{
    REGISTRY, TaggedRegistryExt, damage_type::DamageScaling, damage_type::DamageType,
    vanilla_damage_type_tags,
};
use glam::DVec3;

use text_components::TextComponent;
use text_components::translation::TranslatedMessage;

use crate::entity::Entity;
use crate::world::World;

/// Describes how an entity was damaged.
#[derive(Debug, Clone)]
pub struct DamageSource {
    /// The damage type registry entry.
    pub damage_type: &'static DamageType,
    /// The entity ultimately responsible (e.g. the shooter for projectiles).
    pub causing_entity_id: Option<i32>,
    /// The entity that directly dealt the damage (e.g. the projectile itself).
    pub direct_entity_id: Option<i32>,
    /// Source position (for explosions, etc.).
    pub source_position: Option<DVec3>,
}

impl DamageSource {
    /// Environmental damage with no entity or position context (void, starvation, etc.).
    #[must_use]
    pub const fn environment(damage_type: &'static DamageType) -> Self {
        Self {
            damage_type,
            causing_entity_id: None,
            direct_entity_id: None,
            source_position: None,
        }
    }

    /// Builds the death message this source gives the victim.
    ///
    /// Vanilla parity: `DamageSource.getLocalizedDeathMessage`. The killer's
    /// name is the second argument, which is what `death.attack.mob` and most of
    /// its siblings are written to take -- one argument leaves the sentence
    /// half-finished.
    ///
    /// `kill_credit` is the victim's `LivingEntity.getKillCredit()`. Vanilla only
    /// looks at it when nothing entity-shaped dealt the blow, so a player who
    /// drowns after being knocked in still names whoever pushed them.
    //
    // TODO: the `.item` variant, for a killer swinging a renamed weapon. It
    // needs the attacker's main-hand stack, which no entity-wide accessor
    // reaches yet.
    #[must_use]
    pub fn localized_death_message(
        &self,
        world: &World,
        victim: &dyn Entity,
        kill_credit: Option<&dyn Entity>,
    ) -> TextComponent {
        let key = format!("death.attack.{}", self.damage_type.message_id);
        let victim_name = victim.display_name();

        // Vanilla reads the causing entity first and falls back to the direct
        // one, so an arrow names the archer. An id that no longer resolves is
        // Foton's own case: vanilla holds the reference and cannot lose it.
        let attacker = self
            .causing_entity_id
            .or(self.direct_entity_id)
            .and_then(|id| world.get_entity_by_id(id));

        if let Some(attacker) = attacker {
            return translated_death(key, [victim_name, attacker.display_name()]);
        }

        match kill_credit {
            Some(credit) => translated_death(
                format!("{key}.player"),
                [victim_name, credit.display_name()],
            ),
            None => translated_death_alone(key, victim_name),
        }
    }

    /// Adds the entity ultimately responsible for the damage.
    #[must_use]
    pub const fn with_causing_entity(mut self, entity_id: i32) -> Self {
        self.causing_entity_id = Some(entity_id);
        self
    }

    /// Adds the direct entity that delivered the damage.
    #[must_use]
    pub const fn with_direct_entity(mut self, entity_id: i32) -> Self {
        self.direct_entity_id = Some(entity_id);
        self
    }

    /// Adds the vanilla source position used by damage events and knockback.
    #[must_use]
    pub const fn with_source_position(mut self, source_position: DVec3) -> Self {
        self.source_position = Some(source_position);
        self
    }

    /// Whether this damage bypasses creative/spectator invulnerability.
    #[must_use]
    pub fn bypasses_invulnerability(&self) -> bool {
        self.is(&vanilla_damage_type_tags::DamageTypeTag::BYPASSES_INVULNERABILITY)
    }

    /// Returns whether this damage type is in the given vanilla damage-type tag.
    #[must_use]
    pub fn is(&self, tag: &foton_utils::Identifier) -> bool {
        REGISTRY.damage_types.is_in_tag(self.damage_type, tag)
    }

    /// Returns vanilla `DamageSource.isDirect`.
    #[must_use]
    pub fn is_direct(&self) -> bool {
        self.causing_entity_id == self.direct_entity_id
    }

    /// Whether this damage bypasses the invulnerability cooldown timer.
    /// No vanilla damage types currently use this, but the logic exists in
    /// `LivingEntity.hurtServer()`.
    /// TODO: use damage type tag query once supported
    #[expect(clippy::unused_self, reason = "this is an api function")]
    #[must_use]
    pub const fn bypasses_cooldown(&self) -> bool {
        false
    }

    /// Whether this damage scales with world difficulty for the resolved causing entity.
    ///
    /// `causing_entity` is `None` when the source has no cause or its stored entity ID no
    /// longer resolves. Both cases fail Vanilla's living non-player type check.
    #[must_use]
    pub fn scales_with_difficulty(&self, causing_entity: Option<&dyn Entity>) -> bool {
        match self.damage_type.scaling {
            DamageScaling::Never => false,
            DamageScaling::WhenCausedByLivingNonPlayer => causing_entity.is_some_and(|entity| {
                entity.as_living_entity().is_some() && entity.as_player().is_none()
            }),
            DamageScaling::Always => true,
        }
    }
}

fn translated_death(key: String, args: [TextComponent; 2]) -> TextComponent {
    TranslatedMessage {
        key: key.into(),
        fallback: None,
        args: Some(Box::new(args)),
    }
    .component()
}

fn translated_death_alone(key: String, victim: TextComponent) -> TextComponent {
    TranslatedMessage {
        key: key.into(),
        fallback: None,
        args: Some(Box::new([victim])),
    }
    .component()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Weak};

    use foton_registry::{init_vanilla_registry, vanilla_damage_types, vanilla_entities};
    use foton_utils::text::DisplayResolutor;
    use glam::DVec3;
    use text_components::TextComponent;
    use text_components::content::Content;

    use crate::entity::SharedEntity;
    use crate::entity::entities::{FireworkRocketEntity, PigEntity};
    use crate::test_support::{fresh_test_world, insert_entity_ticking_chunk};

    use super::*;

    fn pig(id: i32, world: &Arc<World>) -> SharedEntity {
        Arc::new(PigEntity::new(
            &vanilla_entities::PIG,
            id,
            DVec3::ZERO,
            Arc::downgrade(world),
        ))
    }

    /// The arguments a translated component was built with.
    fn death_args(message: &TextComponent) -> &[TextComponent] {
        let Content::Translate(translated) = &message.content else {
            panic!("a death message is a translated component");
        };
        translated
            .args
            .as_deref()
            .expect("a death message carries its arguments")
    }

    /// `death.attack.mob` reads "%1$s was slain by %2$s". Built with a single
    /// argument it comes out with the killer missing, which is what every death
    /// by a mob used to look like.
    #[test]
    fn a_death_by_a_mob_names_the_mob() {
        init_vanilla_registry();

        let world = fresh_test_world("death_message_names_killer");
        insert_entity_ticking_chunk(&world, foton_utils::ChunkPos::new(0, 0));
        let victim = pig(1, &world);
        let killer = pig(2, &world);
        world
            .try_add_entity(Arc::clone(&killer))
            .expect("the killer should join the world");

        let source = DamageSource::environment(&vanilla_damage_types::MOB_ATTACK)
            .with_causing_entity(killer.id());
        let message = source.localized_death_message(&world, victim.as_ref(), None);

        let args = death_args(&message);
        assert_eq!(args.len(), 2, "the killer is the second argument");
        assert_eq!(
            args[1].to_plain(&DisplayResolutor),
            killer.display_name().to_plain(&DisplayResolutor)
        );
    }

    /// With nobody holding the weapon, vanilla falls back to whoever last hurt
    /// the victim and switches to the `.player` wording.
    #[test]
    fn a_death_with_no_attacker_falls_back_to_kill_credit() {
        init_vanilla_registry();

        let world = fresh_test_world("death_message_kill_credit");
        let victim = pig(1, &world);
        let credit = pig(2, &world);

        let source = DamageSource::environment(&vanilla_damage_types::IN_FIRE);
        let alone = source.localized_death_message(&world, victim.as_ref(), None);
        assert_eq!(death_args(&alone).len(), 1);

        let blamed = source.localized_death_message(&world, victim.as_ref(), Some(credit.as_ref()));
        assert_eq!(death_args(&blamed).len(), 2);
    }

    #[test]
    fn conditional_difficulty_scaling_requires_a_resolved_living_non_player() {
        init_vanilla_registry();
        let source = DamageSource::environment(&vanilla_damage_types::FIREWORKS);
        let pig = PigEntity::new(&vanilla_entities::PIG, 1, DVec3::ZERO, Weak::new());
        let rocket = FireworkRocketEntity::new(
            &vanilla_entities::FIREWORK_ROCKET,
            2,
            DVec3::ZERO,
            Weak::new(),
        );

        assert!(source.scales_with_difficulty(Some(&pig)));
        assert!(!source.scales_with_difficulty(Some(&rocket)));
        assert!(!source.scales_with_difficulty(None));
    }
}
