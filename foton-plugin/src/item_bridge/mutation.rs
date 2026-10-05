//! Owning Java mutations become detached candidates before any native destination changes.

use crate::natives as bridge_native_bridge;
use foton_registry::data_components::vanilla_components as bridge_vanilla_components;
use foton_registry::data_components::{
    ComponentData,
    components::{CustomModelData, ItemEnchantments, TooltipDisplay},
};
use foton_registry::items as bridge_items;
use foton_registry::vanilla_items as bridge_vanilla_items;
use foton_registry::{REGISTRY, RegistryExt, item_stack::ItemStack};
use foton_utils::Identifier;
use jni::{
    JNIEnv,
    objects::{JObject, JObjectArray, JString},
};
use std::iter as bridge_iter;
#[cfg(test)]
use std::num as bridge_num;
#[cfg(test)]
use std::slice as bridge_slice;

use super::{
    ItemBridgeError,
    edits::{Edit, Edits, Operation},
    snapshot::Candidate,
    store, transfer,
};

fn text(env: &mut JNIEnv<'_>, object: JObject<'_>) -> Result<String, ItemBridgeError> {
    let value = JString::from(object);
    let value = env.get_string(&value)?;
    let value = value
        .to_str()
        .map_err(|_| ItemBridgeError::InvalidEdit("invalid string encoding"))?;
    if value.len() > 8_388_608 {
        return Err(ItemBridgeError::TransportLimit);
    }
    Ok(value.to_owned())
}

fn string_field(
    env: &mut JNIEnv<'_>,
    object: &JObject<'_>,
    name: &str,
) -> Result<String, ItemBridgeError> {
    let field = env.get_field(object, name, "Ljava/lang/String;")?.l()?;
    text(env, field)
}

pub(crate) fn materialize(
    env: &mut JNIEnv<'_>,
    mutation: &JObject<'_>,
) -> Result<Candidate, ItemBridgeError> {
    super::require_registry()?;
    if mutation.is_null() {
        return Err(ItemBridgeError::InvalidEdit("missing owning mutation"));
    }
    let projection = string_field(env, mutation, "projection")?;
    let lease = env
        .get_field(mutation, "lease", "Lfoton/item/NativeItemLease;")?
        .l()?;
    let keys = JObjectArray::from(
        env.get_field(mutation, "keys", "[Ljava/lang/String;")?
            .l()?,
    );
    let operations = JObjectArray::from(
        env.get_field(mutation, "operations", "[Ljava/lang/String;")?
            .l()?,
    );
    let size = env.get_array_length(&keys)?;
    if size > 4096 || size != env.get_array_length(&operations)? {
        return Err(ItemBridgeError::InvalidEdit("invalid journal length"));
    }
    let (item, count) = header(&projection)?;
    let mut candidate = if lease.is_null() {
        store()?.stage(|| {
            bridge_native_bridge::parse_slot(&projection)
                .ok_or(ItemBridgeError::InvalidEdit("invalid legacy item"))
        })?
    } else {
        store()?.materialize(transfer::lease_key(env, &lease)?)?
    };
    let mut groups = rustc_hash::FxHashSet::default();
    for index in 0..size {
        let key_object = env.get_object_array_element(&keys, index)?;
        let key = text(env, key_object)?;
        let operation_object = env.get_object_array_element(&operations, index)?;
        let operation = text(env, operation_object)?;
        if !groups.insert(key.clone()) {
            return Err(ItemBridgeError::InvalidEdit("duplicate metadata group"));
        }
        let edit = decode_edit(&candidate.stack, &projection, &key, &operation)?;
        // Subfield groups can share a component (potion base/effects, tooltip
        // visibility/flags). Compose on the detached candidate only. A later
        // failure discards all edits; no destination or source lease changes.
        candidate = Edits::new(vec![edit])?.apply(candidate)?;
    }
    if let Some(item) = item {
        if !lease.is_null() && candidate.stack.item() != item {
            return Err(ItemBridgeError::InvalidEdit(
                "item type requires an explicit rebase",
            ));
        }
        candidate.stack.count = count;
    } else {
        candidate.stack = ItemStack::empty();
    }
    Ok(candidate)
}

fn header(projection: &str) -> Result<(Option<bridge_items::ItemRef>, i32), ItemBridgeError> {
    if projection.is_empty() {
        return Ok((None, 0));
    }
    let header = projection
        .split('\u{1d}')
        .next()
        .ok_or(ItemBridgeError::InvalidEdit("missing item header"))?;
    let (key, count) = header
        .rsplit_once(' ')
        .ok_or(ItemBridgeError::InvalidEdit("invalid item header"))?;
    let key: Identifier = key
        .parse()
        .map_err(|_| ItemBridgeError::InvalidEdit("invalid item key"))?;
    let item = REGISTRY
        .items
        .by_key(&key)
        .ok_or(ItemBridgeError::InvalidEdit("unknown item"))?;
    let count: i32 = count
        .parse()
        .map_err(|_| ItemBridgeError::InvalidEdit("invalid count"))?;
    if count <= 0 {
        return Err(ItemBridgeError::InvalidEdit("nonpositive count"));
    }
    Ok((Some(item), count))
}

fn decode_potion_edit(
    base: &ItemStack,
    projection: &str,
    group: &str,
    operation: &str,
) -> Result<Edit, ItemBridgeError> {
    use foton_registry::data_components::{
        components::PotionContents, vanilla_components::POTION_CONTENTS,
    };
    if operation != "SET" {
        return Err(ItemBridgeError::InvalidEdit(
            "invalid potion subfield operation",
        ));
    }
    let original = base.get(POTION_CONTENTS);
    if group == "potion_base" {
        let fields: Vec<_> = projection
            .split('\u{1d}')
            .skip(1)
            .filter(|field| field.starts_with("basepotionhex="))
            .collect();
        if fields.len() > 1 {
            return Err(ItemBridgeError::InvalidEdit("duplicate base potion"));
        }
        let selected = bridge_iter::once(projection.split('\u{1d}').next().unwrap_or_default())
            .chain(fields)
            .collect::<Vec<_>>()
            .join("\u{1d}");
        let parsed = bridge_native_bridge::parse_slot(&selected)
            .ok_or(ItemBridgeError::InvalidEdit("invalid base potion"))?;
        return Ok(Edit {
            key: POTION_CONTENTS.key().clone(),
            operation: Operation::Set(ComponentData::new(PotionContents::new(
                parsed.get(POTION_CONTENTS).and_then(PotionContents::potion),
                original.and_then(PotionContents::custom_color),
                original.map_or_else(Vec::new, |value| value.custom_effects().to_vec()),
                original
                    .and_then(PotionContents::custom_name)
                    .map(str::to_owned),
            ))),
        });
    }
    if original.is_some_and(|contents| {
        contents
            .custom_effects()
            .iter()
            .any(|effect| effect.hidden_effect().is_some())
    }) {
        return Err(ItemBridgeError::Unsupported(
            "unprojected native potion effects",
        ));
    }
    let fields: Vec<_> = projection
        .split('\u{1d}')
        .skip(1)
        .filter_map(|field| {
            field
                .strip_prefix("potioneffects=")
                .or_else(|| (field.contains(',') && !field.contains('=')).then_some(field))
        })
        .collect();
    if fields.len() > 1 {
        return Err(ItemBridgeError::InvalidEdit("duplicate potion effects"));
    }
    let effects = match fields.first() {
        Some(field) => super::legacy::parse_effects(field)
            .ok_or(ItemBridgeError::InvalidEdit("invalid potion effects"))?,
        None => Vec::new(),
    };
    Ok(Edit {
        key: POTION_CONTENTS.key().clone(),
        operation: Operation::Set(ComponentData::new(PotionContents::new(
            original.and_then(PotionContents::potion),
            original.and_then(PotionContents::custom_color),
            effects,
            original
                .and_then(PotionContents::custom_name)
                .map(str::to_owned),
        ))),
    })
}

fn group_prefixes(group: &str) -> Result<&'static [&'static str], ItemBridgeError> {
    Ok(match group {
        "custom_name" => &["namehex=", "namejsonhex="],
        "lore" => &["lorehex=", "lorejsonhex="],
        "damage" => &["damage="],
        "unbreakable" => &["unbreakable"],
        "item_model" => &["itemmodelhex="],
        "tooltip_style" => &["tooltipstylehex="],
        "tooltip_visibility" => &["hidetooltip"],
        "tooltip_display" => &["hidetooltip", "hide="],
        "custom_model_data" => &[
            "model=",
            "modelfloat=",
            "modelflag=",
            "modelstrhex=",
            "modelcolor=",
        ],
        "enchantments" => &["enchhex="],
        "stored_enchantments" => &["storedenchhex="],
        "custom_data" => &["customhex=", "pdcrawhex=", "pdcidentity=", "pdc"],
        "trim" => &["trim="],
        "banner_patterns" => &["pattern="],
        "base_color" => &["basecolor="],
        "enchantment_glint_override" => &["glint="],
        "max_stack_size" => &["maxstack="],
        "use_cooldown" => &["cooldown=", "cooldowngroup="],
        "instrument" => &["instrument="],
        "book" => &[
            "booktitlehex=",
            "bookauthorhex=",
            "bookgen=",
            "bookpagehex=",
            "bookrawhex=",
            "bookresolved=",
        ],
        _ => return Err(ItemBridgeError::Unsupported("metadata mutation group")),
    })
}

fn decode_edit(
    base: &ItemStack,
    projection: &str,
    group: &str,
    operation: &str,
) -> Result<Edit, ItemBridgeError> {
    if group == "potion_effects" || group == "potion_base" {
        return decode_potion_edit(base, projection, group, operation);
    }
    if group.starts_with("unsupported:") {
        let capability = match group {
            "unsupported:attribute_modifiers" => "attribute modifiers",
            "unsupported:item_flags" => "item flags",
            "unsupported:bundle_contents" => "bundle contents",
            "unsupported:charged_projectiles" => "charged projectiles",
            "unsupported:dyed_color" => "dyed color",
            "unsupported:trim" => "armor trim",
            "unsupported:banner" => "banner patterns/color",
            "unsupported:fireworks" => "fireworks",
            "unsupported:firework_explosion" => "firework explosion",
            "unsupported:profile" => "skull profile",
            "unsupported:map" => "map metadata",
            "unsupported:stew_effects" => "suspicious stew effects",
            "unsupported:base_potion" => "base potion",
            "unsupported:potion_color" => "potion color",
            _ => "metadata mutation group",
        };
        return Err(ItemBridgeError::Unsupported(capability));
    }
    let prefixes = group_prefixes(group)?;
    let component = if group == "book" {
        match header(projection)?.0 {
            Some(item) if item == &*bridge_vanilla_items::WRITTEN_BOOK => "written_book_content",
            Some(item) if item == &*bridge_vanilla_items::WRITABLE_BOOK => "writable_book_content",
            _ => return Err(ItemBridgeError::InvalidEdit("book edit on non-book item")),
        }
    } else if group == "tooltip_visibility" {
        "tooltip_display"
    } else {
        group
    };
    let key: Identifier = format!("minecraft:{component}")
        .parse()
        .map_err(|_| ItemBridgeError::InvalidEdit("invalid component key"))?;
    let operation = match operation {
        "REMOVE" => Operation::Remove,
        "RESET" => Operation::Reset,
        "SET" => {
            let mut fields = projection.split('\u{1d}');
            let mut selected = fields
                .next()
                .ok_or(ItemBridgeError::InvalidEdit("missing edit header"))?
                .to_owned();
            for field in fields {
                if prefixes.iter().any(|prefix| field.starts_with(prefix)) {
                    selected.push('\u{1d}');
                    selected.push_str(field);
                }
            }
            let parsed = bridge_native_bridge::parse_slot(&selected)
                .ok_or(ItemBridgeError::InvalidEdit("invalid component payload"))?;
            if group == "tooltip_visibility" {
                // The existing Java setter edits the global flag, not hidden component types.
                let mut display = base
                    .get(bridge_vanilla_components::TOOLTIP_DISPLAY)
                    .cloned()
                    .unwrap_or_default();
                display.hide_tooltip = parsed
                    .get(bridge_vanilla_components::TOOLTIP_DISPLAY)
                    .is_some_and(|display| display.hide_tooltip);
                Operation::Set(ComponentData::new(display))
            } else if let Some(value) = parsed.get_effective_value_raw(&key) {
                super::preflight::check_component(value)?;
                Operation::Set(value.clone())
            } else {
                match group {
                    "damage" => Operation::Set(ComponentData::new(0_i32)),
                    "tooltip_display" => {
                        Operation::Set(ComponentData::new(TooltipDisplay::new(false)))
                    }
                    "custom_model_data" => Operation::Set(ComponentData::new(
                        CustomModelData::new(vec![], vec![], vec![], vec![]),
                    )),
                    "enchantments" | "stored_enchantments" => {
                        Operation::Set(ComponentData::new(ItemEnchantments::empty()))
                    }
                    "custom_data" => Operation::Remove,
                    _ => return Err(ItemBridgeError::InvalidEdit("missing SET value")),
                }
            }
        }
        _ => return Err(ItemBridgeError::InvalidEdit("unknown edit operation")),
    };
    Ok(Edit { key, operation })
}

#[cfg(test)]
mod tests {
    use super::*;
    use foton_registry::{
        data_components::vanilla_components::{ATTRIBUTE_MODIFIERS, DAMAGE, TOOLTIP_DISPLAY},
        init_vanilla_registry, vanilla_items,
    };

    #[test]
    fn migrated_component_journals_preserve_opaque_state_and_distinguish_remove_reset() {
        use foton_registry::data_components::vanilla_components::GLIDER;
        init_vanilla_registry();
        for (group, fields) in [
            ("trim", "trim=minecraft:gold,minecraft:wild"),
            ("banner_patterns", "pattern=minecraft:cross,red"),
            ("base_color", "basecolor=blue"),
            ("enchantment_glint_override", "glint=true"),
            ("max_stack_size", "maxstack=16"),
            (
                "use_cooldown",
                "cooldown=2.5\u{1d}cooldowngroup=fixture:cooldown",
            ),
            ("instrument", "instrument=minecraft:ponder_goat_horn"),
            (
                "lore",
                "lorejsonhex=7b2274657874223a2272696368222c22626f6c64223a747275657d",
            ),
            ("custom_data", "customhex=7b6f70617175653a317d"),
        ] {
            let mut base = ItemStack::new(&vanilla_items::IRON_CHESTPLATE);
            base.set(GLIDER, ());
            base.remove(DAMAGE);
            base.set_opaque_nbt(Some("fixture opaque stack state".to_owned()));
            let store = super::super::SnapshotStore::new(bridge_num::NonZeroUsize::MIN);
            let original = store.capture(&base).expect("snapshot").publish();
            let projection = format!("minecraft:iron_chestplate 1\u{1d}{fields}");
            let edit = decode_edit(&base, &projection, group, "SET").expect(group);
            let key = edit.key.clone();
            let candidate = Edits::new(vec![edit])
                .expect("set journal")
                .apply(store.materialize(original).expect("candidate"))
                .expect("set");
            assert!(
                candidate.stack.get_effective_value_raw(&key).is_some(),
                "{group}"
            );
            assert!(candidate.stack.has(GLIDER), "{group}");
            assert!(!candidate.stack.has(DAMAGE), "{group}");
            assert_eq!(
                candidate.stack.opaque_nbt(),
                Some("fixture opaque stack state")
            );
            let remove =
                decode_edit(&candidate.stack, &projection, group, "REMOVE").expect("remove");
            let candidate = Edits::new(vec![remove])
                .expect("remove journal")
                .apply(candidate)
                .expect("removed");
            assert!(
                candidate.stack.get_effective_value_raw(&key).is_none(),
                "{group}"
            );
            let reset = decode_edit(&candidate.stack, &projection, group, "RESET").expect("reset");
            let candidate = Edits::new(vec![reset])
                .expect("reset journal")
                .apply(candidate)
                .expect("reset");
            assert_eq!(
                candidate.stack.get_effective_value_raw(&key),
                base.item().components.get_raw(&key),
                "{group}"
            );
            assert_eq!(
                store.lookup(original).expect("source snapshot").stack(),
                &base
            );
        }
    }

    #[test]
    fn migrated_potion_and_hidden_groups_preserve_unedited_subfields() {
        use foton_registry::data_components::vanilla_components::POTION_CONTENTS;
        init_vanilla_registry();
        let base = bridge_native_bridge::parse_slot("minecraft:potion 1\u{1d}basepotionhex=6d696e6563726166743a7761746572\u{1d}potioneffects=speed,20,1,true,false,false;").expect("source potion");
        let original = base.get(POTION_CONTENTS).expect("contents");
        let store = super::super::SnapshotStore::new(bridge_num::NonZeroUsize::MIN);
        let mut candidate = store.candidate(base.clone()).expect("candidate");
        for group in ["potion_effects", "potion_base"] {
            let edit = decode_edit(
                &candidate.stack,
                "minecraft:potion 1\u{1d}potioneffects=slowness,80,2,false,true,false;",
                group,
                "SET",
            )
            .expect(group);
            candidate = Edits::new(vec![edit])
                .expect("journal")
                .apply(candidate)
                .expect("apply");
        }
        let contents = candidate.stack.get(POTION_CONTENTS).expect("edited potion");
        assert!(contents.potion().is_none());
        assert_eq!(contents.custom_effects().len(), 1);
        let effect = &contents.custom_effects()[0];
        assert_eq!(effect.duration(), 80);
        assert_eq!(effect.amplifier(), 2);
        assert!(!effect.ambient());
        assert!(effect.show_particles());
        assert!(!effect.show_icon());
        assert!(original.potion().is_some());
        assert_eq!(original.custom_effects()[0].duration(), 20);
        drop(candidate);

        let mut base = ItemStack::new(&vanilla_items::IRON_CHESTPLATE);
        base.set(
            TOOLTIP_DISPLAY,
            TooltipDisplay::from_hidden_components(
                true,
                vec![
                    "fixture:opaque".parse().expect("key"),
                    ATTRIBUTE_MODIFIERS.key().clone(),
                ],
            ),
        );
        let edit = decode_edit(
            &base,
            "minecraft:iron_chestplate 1\u{1d}hidetooltip\u{1d}hide=fixture:opaque",
            "tooltip_display",
            "SET",
        )
        .expect("remove flag only");
        let candidate = Edits::new(vec![edit])
            .expect("journal")
            .apply(store.candidate(base).expect("candidate"))
            .expect("apply");
        let display = candidate.stack.get(TOOLTIP_DISPLAY).expect("tooltip");
        assert!(display.hide_tooltip);
        assert_eq!(
            display.hidden_components(),
            &["fixture:opaque".parse::<Identifier>().expect("key")]
        );
    }

    #[test]
    fn explicit_damage_is_full_width_and_tooltip_flag_preserves_hidden_components() {
        init_vanilla_registry();
        let mut base = ItemStack::new(&vanilla_items::IRON_CHESTPLATE);
        base.set(
            TOOLTIP_DISPLAY,
            TooltipDisplay::new(false).with_hidden(ATTRIBUTE_MODIFIERS, true),
        );
        let damage = decode_edit(
            &base,
            "minecraft:iron_chestplate 1\u{1d}damage=70000",
            "damage",
            "SET",
        )
        .expect("int damage");
        let tooltip = decode_edit(
            &base,
            "minecraft:iron_chestplate 1\u{1d}hidetooltip",
            "tooltip_visibility",
            "SET",
        )
        .expect("visibility edit");
        let store = super::super::SnapshotStore::new(bridge_num::NonZeroUsize::MIN);
        let candidate = Edits::new(vec![damage, tooltip])
            .expect("journal")
            .apply(store.candidate(base).expect("candidate"))
            .expect("edits");
        assert_eq!(candidate.stack.get(DAMAGE), Some(&70000));
        let display = candidate.stack.get(TOOLTIP_DISPLAY).expect("tooltip");
        assert!(display.hide_tooltip);
        assert_eq!(
            display.hidden_components(),
            bridge_slice::from_ref(ATTRIBUTE_MODIFIERS.key())
        );
        assert!(matches!(
            decode_edit(
                &candidate.stack,
                "minecraft:iron_chestplate 1",
                "unsupported:attribute_modifiers",
                "SET"
            ),
            Err(ItemBridgeError::Unsupported("attribute modifiers"))
        ));
    }
}
