use super::*;
use crate::data_components::components::{BundleContents, UseRemainder};
use crate::data_components::vanilla_components::{DAMAGE, LOCK, USE_REMAINDER};
use crate::item_predicate::{
    AdventureModePredicate, BlockPredicate, IntBounds, ItemPredicate, LockCode,
};
use crate::{ItemStackTemplate, init_vanilla_registry, vanilla_items};
use foton_utils::serial::budget::DecodeBudget;
use std::io::Cursor;

fn entry(name: &'static str) -> ComponentEntryRef {
    REGISTRY
        .data_components
        .by_key(&Identifier::vanilla_static(name))
        .expect("registered")
}
fn exact(entry: ComponentEntryRef, value: ComponentData) -> DataComponentMatchers {
    DataComponentMatchers {
        exact: DataComponentExactPredicate {
            values: vec![(entry, value)],
        },
        partial: vec![],
    }
}
fn template(wrappers: usize, leaf: DataComponentPatch) -> ItemStackTemplate {
    DecodeBudget::new(64 * 1024 * 1024)
        .decode(|| {
            let _scope = PersistentValidationScope::enter();
            let mut value =
                ItemStackTemplate::try_with_count_and_patch(&vanilla_items::STONE, 1, leaf)?;
            for _ in 0..wrappers {
                let mut patch = DataComponentPatch::new();
                patch.set(USE_REMAINDER, UseRemainder::new(value));
                value =
                    ItemStackTemplate::try_with_count_and_patch(&vanilla_items::STONE, 1, patch)?;
            }
            Ok(value)
        })
        .expect("fixture construction outside measurement")
}
fn registered_oracle(name: &'static str, value: ComponentData, accepted: bool) {
    let component = entry(name);
    let tag = component
        .write_nbt(&value)
        .expect("ordinary persistent representation");
    let mut ordinary = Vec::new();
    tag.write(&mut ordinary);
    let read = |bytes: &[u8]| {
        let mut cursor = Cursor::new(bytes);
        let Ok(tag) = simdnbt::borrow::read_tag(&mut cursor) else {
            return None;
        };
        DecodeBudget::new(64 * 1024 * 1024)
            .decode(|| {
                component
                    .read_nbt(tag.as_tag())
                    .ok_or_else(|| Error::other("registered rejection"))
            })
            .ok()
    };
    assert_eq!(
        read(&ordinary).is_some(),
        accepted,
        "independent registered reader: {name}"
    );
    let mut bytes = Vec::new();
    let result = component.write_nbt_bounded(
        &value,
        &mut nbt_stream::LimitedWriter::persistent(&mut bytes, 65536),
        0,
    );
    assert_eq!(
        result.is_ok(),
        accepted,
        "persistent writer mismatch for {name}: {result:?}"
    );
    if accepted {
        let decoded = read(&bytes).expect("accepted bytes must read back");
        let mut reencoded = Vec::new();
        component
            .write_nbt_bounded(
                &decoded,
                &mut nbt_stream::LimitedWriter::persistent(&mut reencoded, 65536),
                0,
            )
            .expect("registered reencode");
        assert_eq!(
            bytes, reencoded,
            "registered persistence must reencode canonically"
        );
    }
}

#[test]
fn brewing_task12_removal_compound_registered_depth_boundary() {
    init_vanilla_registry();
    // Root block 0, exact 1, bundle list 2, template 3 + 2*w,
    // patch 4 + 2*w, removal 5 + 2*w. A second block adds one list level.
    for (wrappers, listed, accepted) in [(252, false, true), (252, true, true), (253, false, false)]
    {
        let mut patch = DataComponentPatch::new();
        patch.remove(DAMAGE);
        let bundle = BundleContents::new(vec![template(wrappers, patch)]);
        let block = BlockPredicate::new(
            None,
            None,
            None,
            exact(entry("bundle_contents"), ComponentData::new(bundle)),
        );
        let mut blocks = vec![block];
        if listed {
            blocks.push(BlockPredicate::new(
                None,
                None,
                None,
                DataComponentMatchers::ANY,
            ));
        }
        registered_oracle(
            "can_break",
            ComponentData::new(AdventureModePredicate::new(blocks).expect("blocks")),
            accepted,
        );
    }
}

#[test]
fn brewing_task12_any_compound_registered_depth_boundary() {
    init_vanilla_registry();
    // Bundle list 0, template 1 + 2*w, patch 2 + 2*w, lock 3 + 2*w,
    // partial map 4 + 2*w, Any 5 + 2*w (511 at w=253).
    for (wrappers, bundled, accepted) in [(252, true, true), (253, false, true), (253, true, false)]
    {
        let any = DataComponentMatchers::new(
            DataComponentExactPredicate::EMPTY,
            vec![DataComponentPredicateData::any(entry("damage"))],
        )
        .expect("Any");
        let mut patch = DataComponentPatch::new();
        patch.set(
            LOCK,
            LockCode::new(ItemPredicate::new(None, IntBounds::ANY, any)),
        );
        let item = template(wrappers, patch);
        if bundled {
            registered_oracle(
                "bundle_contents",
                ComponentData::new(BundleContents::new(vec![item])),
                accepted,
            );
        } else {
            registered_oracle(
                "use_remainder",
                ComponentData::new(UseRemainder::new(item)),
                accepted,
            );
        }
    }
}

#[test]
fn brewing_task12_empty_matcher_at_depth_510_emits_no_child() {
    init_vanilla_registry();
    for (wrappers, accepted) in [(254, true), (255, false)] {
        let mut patch = DataComponentPatch::new();
        patch.set(LOCK, LockCode::NO_LOCK);
        registered_oracle(
            "use_remainder",
            ComponentData::new(UseRemainder::new(template(wrappers, patch))),
            accepted,
        );
    }
}

#[test]
fn brewing_task12_empty_container_list_needs_no_compound_frame() {
    use crate::data_components::components::ItemContainerContents;
    use crate::data_components::vanilla_components::CONTAINER;
    init_vanilla_registry();
    let mut patch = DataComponentPatch::new();
    patch.set(CONTAINER, ItemContainerContents::empty());
    registered_oracle(
        "bundle_contents",
        ComponentData::new(BundleContents::new(vec![template(254, patch)])),
        true,
    );
}

#[test]
fn brewing_task12_lock_exact_adventure_composes_container_depths() {
    init_vanilla_registry();
    for (wrappers, listed, accepted) in [(251, true, true), (252, false, false)] {
        let mut patch = DataComponentPatch::new();
        patch.remove(DAMAGE);
        let bundle = BundleContents::new(vec![template(wrappers, patch)]);
        let mut blocks = vec![BlockPredicate::new(
            None,
            None,
            None,
            exact(entry("bundle_contents"), ComponentData::new(bundle)),
        )];
        if listed {
            blocks.push(BlockPredicate::new(
                None,
                None,
                None,
                DataComponentMatchers::ANY,
            ));
        }
        let adventure = AdventureModePredicate::new(blocks).expect("blocks");
        let lock = LockCode::new(ItemPredicate::new(
            None,
            IntBounds::ANY,
            exact(entry("can_break"), ComponentData::new(adventure)),
        ));
        registered_oracle("lock", ComponentData::new(lock), accepted);
    }
}

#[test]
fn brewing_task13_registered_leaf_depth_parity() {
    use crate::data_components::components::{
        BannerPatternLayers, Bees, JukeboxPlayable, PotDecorations, Recipes, SuspiciousStewEffects,
    };
    use crate::data_components::vanilla_components::{
        BANNER_PATTERNS, BEES, JUKEBOX_PLAYABLE, POT_DECORATIONS, RECIPES, SUSPICIOUS_STEW_EFFECTS,
    };
    init_vanilla_registry();
    let mut fixtures = Vec::new();
    for value in [
        Recipes::empty(),
        Recipes::new(vec![Identifier::vanilla_static("bread")]),
    ] {
        let mut patch = DataComponentPatch::new();
        patch.set(RECIPES, value);
        fixtures.push(("recipes", patch));
    }
    let mut patch = DataComponentPatch::new();
    patch.set(BEES, Bees::empty());
    fixtures.push(("bees", patch));
    let mut patch = DataComponentPatch::new();
    patch.set(BANNER_PATTERNS, BannerPatternLayers::empty());
    fixtures.push(("banner_patterns", patch));
    let mut patch = DataComponentPatch::new();
    patch.set(SUSPICIOUS_STEW_EFFECTS, SuspiciousStewEffects::empty());
    fixtures.push(("stew", patch));
    let song = REGISTRY
        .jukebox_songs
        .by_key(&Identifier::vanilla_static("cat"))
        .expect("song");
    let mut patch = DataComponentPatch::new();
    patch.set(JUKEBOX_PLAYABLE, JukeboxPlayable::new(song));
    fixtures.push(("jukebox", patch));
    let mut patch = DataComponentPatch::new();
    patch.set(POT_DECORATIONS, PotDecorations::EMPTY);
    fixtures.push(("pot", patch));
    let mut failures = 0;
    for (name, patch) in fixtures {
        eprintln!("task13 leaf boundary: {name}");
        registered_oracle(
            "use_remainder",
            ComponentData::new(UseRemainder::new(template(254, patch.clone()))),
            true,
        );
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            registered_oracle(
                "bundle_contents",
                ComponentData::new(BundleContents::new(vec![template(254, patch)])),
                true,
            );
        }));
        failures += usize::from(outcome.is_err());
    }
    assert_eq!(failures, 0, "leaf families with depth disagreement");
}

#[test]
fn brewing_task13_shared_depth_guard_restores_after_unwind() {
    use crate::item_stack_template::TemplateDepthGuard;
    let panic = std::panic::catch_unwind(|| {
        let _guard = TemplateDepthGuard::enter().expect("guard");
        std::panic::resume_unwind(Box::new(()));
    });
    assert!(panic.is_err());
    let guards = (0..512)
        .map(|_| TemplateDepthGuard::enter().expect("restored depth"))
        .collect::<Vec<_>>();
    let valid = [1, 0, 0, 0, 0, 0];
    assert!(
        AdventureModePredicate::read(&mut Cursor::new(&valid)).is_err(),
        "adventure must share template depth state"
    );
    drop(guards);
    assert!(AdventureModePredicate::read(&mut Cursor::new(&valid)).is_ok());
}

#[test]
fn brewing_task13_nonempty_leaf_lists_retain_compound_depth_checks() {
    use crate::data_components::components::{
        BannerPatternLayer, BannerPatternLayers, SuspiciousStewEffect, SuspiciousStewEffects,
    };
    use crate::data_components::vanilla_components::{BANNER_PATTERNS, SUSPICIOUS_STEW_EFFECTS};
    use crate::{DyeColor, RegistryHolder, vanilla_banner_patterns};
    init_vanilla_registry();
    let mut patterns = DataComponentPatch::new();
    patterns.set(
        BANNER_PATTERNS,
        BannerPatternLayers::new(vec![BannerPatternLayer::new(
            RegistryHolder::reference(&vanilla_banner_patterns::CREEPER),
            DyeColor::Lime,
        )]),
    );
    let mut stew = DataComponentPatch::new();
    let effect = REGISTRY
        .mob_effects
        .by_key(&Identifier::vanilla_static("night_vision"))
        .expect("effect");
    stew.set(
        SUSPICIOUS_STEW_EFFECTS,
        SuspiciousStewEffects::new(vec![SuspiciousStewEffect::new(effect, 100)]),
    );
    for patch in [patterns, stew] {
        // Leaf list 509 and child compound 510 are accepted; list 510 and
        // its compound 511 are rejected by the actual registered reader.
        registered_oracle(
            "bundle_contents",
            ComponentData::new(BundleContents::new(vec![template(253, patch.clone())])),
            true,
        );
        registered_oracle(
            "use_remainder",
            ComponentData::new(UseRemainder::new(template(254, patch))),
            false,
        );
    }
}

#[test]
fn brewing_task13_unknown_only_recursive_patch_is_omitted() {
    use crate::data_components::DataComponentType;
    init_vanilla_registry();
    let mut patch = DataComponentPatch::new();
    patch.remove(DataComponentType::<()>::new(Identifier::new(
        "test",
        "unknown_remove",
    )));
    patch.set(
        DataComponentType::<i32>::new(Identifier::new("test", "unknown_set")),
        7,
    );
    let value = template(3, patch);
    let lock = LockCode::new(ItemPredicate::new(
        None,
        IntBounds::ANY,
        exact(
            entry("use_remainder"),
            ComponentData::new(UseRemainder::new(value)),
        ),
    ));
    registered_oracle("lock", ComponentData::new(lock), true);
}

#[test]
fn brewing_task13_bee_compounds_retain_depth_checks() {
    use crate::data_components::components::{BeehiveOccupant, Bees, CustomData, EntityData};
    use crate::data_components::vanilla_components::BEES;
    init_vanilla_registry();
    let bee = REGISTRY
        .entity_types
        .by_key(&Identifier::vanilla_static("bee"))
        .expect("bee");
    let mut patch = DataComponentPatch::new();
    patch.set(
        BEES,
        Bees::new(vec![BeehiveOccupant::new(
            EntityData::new(
                bee,
                CustomData::try_from_compound(NbtCompound::new()).expect("payload"),
            ),
            0,
            600,
        )]),
    );
    registered_oracle(
        "use_remainder",
        ComponentData::new(UseRemainder::new(template(253, patch.clone()))),
        true,
    );
    registered_oracle(
        "bundle_contents",
        ComponentData::new(BundleContents::new(vec![template(253, patch)])),
        false,
    );
}
