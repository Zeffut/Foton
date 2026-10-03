//! Owning Java mutations become detached candidates before any native destination changes.

use foton_registry::data_components::{
    ComponentData,
    components::{CustomModelData, ItemEnchantments, TooltipDisplay},
};
use foton_registry::{REGISTRY, RegistryExt, item_stack::ItemStack};
use foton_utils::Identifier;
use jni::{
    JNIEnv,
    objects::{JObject, JObjectArray, JString},
};

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
            crate::natives::parse_slot(&projection)
                .ok_or(ItemBridgeError::InvalidEdit("invalid legacy item"))
        })?
    } else {
        store()?.materialize(transfer::lease_key(env, &lease)?)?
    };
    let mut edits = Vec::with_capacity(
        usize::try_from(size)
            .map_err(|_| ItemBridgeError::InvalidEdit("invalid journal length"))?,
    );
    for index in 0..size {
        let key_object = env.get_object_array_element(&keys, index)?;
        let key = text(env, key_object)?;
        let operation_object = env.get_object_array_element(&operations, index)?;
        let operation = text(env, operation_object)?;
        if !lease.is_null()
            && key == "potion_effects"
            && candidate
                .stack
                .get(foton_registry::data_components::vanilla_components::POTION_CONTENTS)
                .is_some_and(|contents| !contents.custom_effects().is_empty())
        {
            return Err(ItemBridgeError::Unsupported(
                "unprojected native potion effects",
            ));
        }
        edits.push(decode_edit(
            &candidate.stack,
            &projection,
            &key,
            &operation,
        )?);
    }
    let edits = Edits::new(edits)?;
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
    edits.apply(candidate)
}

fn header(
    projection: &str,
) -> Result<(Option<foton_registry::items::ItemRef>, i32), ItemBridgeError> {
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

fn decode_edit(
    base: &ItemStack,
    projection: &str,
    group: &str,
    operation: &str,
) -> Result<Edit, ItemBridgeError> {
    if group == "potion_effects" {
        use foton_registry::data_components::{
            components::PotionContents, vanilla_components::POTION_CONTENTS,
        };
        if operation != "SET" {
            return Err(ItemBridgeError::InvalidEdit(
                "invalid potion effects operation",
            ));
        }
        let fields: Vec<_> = projection
            .split('\u{1d}')
            .skip(1)
            .filter(|field| field.contains(',') && !field.contains('='))
            .collect();
        if fields.len() > 1 {
            return Err(ItemBridgeError::InvalidEdit("duplicate potion effects"));
        }
        let effects = match fields.first() {
            Some(field) => super::legacy::parse_effects(field)
                .ok_or(ItemBridgeError::InvalidEdit("invalid potion effects"))?,
            None => Vec::new(),
        };
        let original = base.get(POTION_CONTENTS);
        return Ok(Edit {
            key: POTION_CONTENTS.key().clone(),
            operation: Operation::Set(ComponentData::new(PotionContents::new(
                original.and_then(PotionContents::potion),
                original.and_then(PotionContents::custom_color),
                effects,
                original
                    .and_then(PotionContents::custom_name)
                    .map(str::to_owned),
            ))),
        });
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
    let prefixes: &[&str] = match group {
        "custom_name" => &["namehex=", "namejsonhex="],
        "lore" => &["lorehex="],
        "damage" => &["damage="],
        "unbreakable" => &["unbreakable"],
        "item_model" => &["itemmodelhex="],
        "tooltip_style" => &["tooltipstylehex="],
        "tooltip_display" => &["hidetooltip"],
        "custom_model_data" => &[
            "model=",
            "modelfloat=",
            "modelflag=",
            "modelstrhex=",
            "modelcolor=",
        ],
        "enchantments" => &["enchhex="],
        "stored_enchantments" => &["storedenchhex="],
        "custom_data" => &["pdcrawhex=", "pdcidentity=", "pdc"],
        "book" => &[
            "booktitlehex=",
            "bookauthorhex=",
            "bookgen=",
            "bookpagehex=",
            "bookrawhex=",
            "bookresolved=",
        ],
        _ => return Err(ItemBridgeError::Unsupported("metadata mutation group")),
    };
    let component = if group == "book" {
        match header(projection)?.0 {
            Some(item) if item == &*foton_registry::vanilla_items::WRITTEN_BOOK => {
                "written_book_content"
            }
            Some(item) if item == &*foton_registry::vanilla_items::WRITABLE_BOOK => {
                "writable_book_content"
            }
            _ => return Err(ItemBridgeError::InvalidEdit("book edit on non-book item")),
        }
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
            let parsed = crate::natives::parse_slot(&selected)
                .ok_or(ItemBridgeError::InvalidEdit("invalid component payload"))?;
            if group == "tooltip_display" {
                // The existing Java setter edits the global flag, not hidden component types.
                let mut display = base
                    .get(foton_registry::data_components::vanilla_components::TOOLTIP_DISPLAY)
                    .cloned()
                    .unwrap_or_default();
                display.hide_tooltip = parsed
                    .get(foton_registry::data_components::vanilla_components::TOOLTIP_DISPLAY)
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
            "tooltip_display",
            "SET",
        )
        .expect("visibility edit");
        let store = super::super::SnapshotStore::new(std::num::NonZeroUsize::MIN);
        let candidate = Edits::new(vec![damage, tooltip])
            .expect("journal")
            .apply(store.candidate(base).expect("candidate"))
            .expect("edits");
        assert_eq!(candidate.stack.get(DAMAGE), Some(&70000));
        let display = candidate.stack.get(TOOLTIP_DISPLAY).expect("tooltip");
        assert!(display.hide_tooltip);
        assert_eq!(
            display.hidden_components(),
            std::slice::from_ref(ATTRIBUTE_MODIFIERS.key())
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
