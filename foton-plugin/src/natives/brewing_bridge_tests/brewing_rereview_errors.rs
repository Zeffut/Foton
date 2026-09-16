use super::*;
use foton_registry::mob_effect::{MobEffect, instance::MobEffectInstance};
use foton_registry::potion::Potion;
use foton_registry::sound_event::{SoundEvent, SoundEventHolder};
use foton_registry::{
    RegistryReference,
    data_components::components::{
        Consumable, ItemUseAnimation, PotionContents, SuspiciousStewEffect, SuspiciousStewEffects,
    },
};
use std::io::sink;

#[test]
fn brewing_rereview_unregistered_reference_errors_are_bounded() {
    init_vanilla_registry();
    let key = || Identifier::new("test", "x".repeat(2 << 20));
    let original = REGISTRY.mob_effects.iter().next().expect("effect").1;
    let effect = Box::leak(Box::new(MobEffect {
        key: key(),
        category: original.category,
        color: original.color,
        particle: original.particle,
        attribute_modifiers: original.attribute_modifiers,
    }));
    let potion = Box::leak(Box::new(Potion::new(key(), "test", &[])));
    let sound = Box::leak(Box::new(SoundEvent {
        key: key(),
        sound_id: Identifier::vanilla_static("test"),
        fixed_range: None,
    }));
    let values = [
        (
            "potion_contents",
            ComponentData::new(
                PotionContents::empty().with_effect_added(MobEffectInstance::simple(effect, 1, 0)),
            ),
        ),
        (
            "potion_contents",
            ComponentData::new(PotionContents::empty().with_potion(RegistryReference::new(potion))),
        ),
        (
            "suspicious_stew_effects",
            ComponentData::new(SuspiciousStewEffects::new(vec![SuspiciousStewEffect::new(
                effect, 1,
            )])),
        ),
        (
            "consumable",
            ComponentData::new(
                Consumable::new(
                    1.0,
                    ItemUseAnimation::Eat,
                    SoundEventHolder::Registry(sound),
                    false,
                    vec![],
                )
                .expect("consumable"),
            ),
        ),
    ];
    let mut failures = Vec::new();
    for (name, value) in values {
        let stats = allocation_counter::measure(|| {
            assert!(
                entry(name)
                    .write_network_bounded(&value, MAX_BREWING_ITEM_BYTES, &mut sink())
                    .is_err()
            );
        });
        eprintln!("{name} unknown reference: {stats:?}");
        if stats.bytes_max >= 1 << 20 {
            failures.push((name, stats.bytes_max));
        }
    }
    assert!(
        failures.is_empty(),
        "oversized registered errors: {failures:?}"
    );
}

#[test]
fn brewing_rereview_unregistered_discriminator_errors_are_bounded() {
    use foton_registry::consume_effect::{
        ClearAllStatusEffectsConsumeEffect, ConsumeEffectData, ConsumeEffectType,
    };
    use foton_registry::data_component_predicate::{
        DamagePredicate, DataComponentPredicateData, DataComponentPredicateType,
    };
    use foton_registry::data_components::components::DeathProtection;
    init_vanilla_registry();
    let key = || Identifier::new("test", "x".repeat(2 << 20));
    let effect_type = Box::leak(Box::new(ConsumeEffectType::of::<
        ClearAllStatusEffectsConsumeEffect,
    >(key())));
    let predicate_type = Box::leak(Box::new(DataComponentPredicateType::of::<DamagePredicate>(
        key(),
    )));
    let partial = DataComponentMatchers::new(
        DataComponentExactPredicate::EMPTY,
        vec![DataComponentPredicateData::new(
            predicate_type,
            DamagePredicate::new(IntBounds::ANY, IntBounds::ANY),
        )],
    )
    .expect("matchers");
    let values = [
        (
            "death_protection",
            ComponentData::new(DeathProtection::new(vec![ConsumeEffectData::new(
                effect_type,
                ClearAllStatusEffectsConsumeEffect,
            )])),
        ),
        (
            "can_break",
            ComponentData::new(
                AdventureModePredicate::new(vec![BlockPredicate::new(None, None, None, partial)])
                    .expect("adventure"),
            ),
        ),
    ];
    let mut failures = Vec::new();
    for (name, value) in values {
        let stats = allocation_counter::measure(|| {
            assert!(
                entry(name)
                    .write_network_bounded(&value, MAX_BREWING_ITEM_BYTES, &mut sink())
                    .is_err()
            );
        });
        eprintln!("{name} unknown discriminator: {stats:?}");
        if stats.bytes_max >= 1 << 20 {
            failures.push((name, stats.bytes_max));
        }
    }
    assert!(
        failures.is_empty(),
        "oversized discriminator errors: {failures:?}"
    );
}
