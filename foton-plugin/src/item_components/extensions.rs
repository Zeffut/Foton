//! Bounded adapter for component groups already supported by the integration parent.
//! This is a projection for explicit edits, not a native snapshot reconstruction.

use std::fmt::{Display, Write as _};

use crate::natives::{adventure_component_from_json, adventure_component_to_json};
use foton_registry::attribute::AttributeModifierOperation;
use foton_registry::data_components::components::{
    ArmorTrim, BannerPatternLayer, BannerPatternLayers, CustomData, InstrumentComponent,
    ItemAttributeModifierDisplay, ItemAttributeModifierEntry, ItemAttributeModifiers, ItemLore,
    TooltipDisplay, UseCooldown,
};
use foton_registry::data_components::vanilla_components::{
    ATTRIBUTE_MODIFIERS, BANNER_PATTERNS, BASE_COLOR, CUSTOM_DATA, ENCHANTMENT_GLINT_OVERRIDE,
    INSTRUMENT, LORE, MAX_STACK_SIZE, PROFILE, TOOLTIP_DISPLAY, TRIM, USE_COOLDOWN,
};
use foton_registry::equipment::EquipmentSlotGroup;
use foton_registry::item_stack::ItemStack;
use foton_registry::{
    DyeColor, PartialProfile, PlayerSkinPatch, ProfileProperty, REGISTRY, RegistryEntry,
    RegistryExt as _, RegistryHolder, ResolvableProfile, ResolvableProfileContents,
    StoredGameProfile,
};
use foton_utils::Identifier;
use foton_utils::nbt::{parse_snbt_compound, to_canonical_snbt};
use simdnbt::owned::NbtTag;

const SEPARATOR: char = '\u{1d}';

fn field(out: &mut String, name: &str, value: impl Display) {
    out.push(SEPARATOR);
    let _ = write!(out, "{name}={value}");
}

fn hex(value: &str) -> String {
    value
        .bytes()
        .fold(String::with_capacity(value.len() * 2), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

fn unhex(value: &str) -> Option<String> {
    let bytes = (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(value.get(index..index + 2)?, 16).ok())
        .collect::<Option<Vec<_>>>()?;
    String::from_utf8(bytes).ok()
}

/// The registry key a holder names; an inline value has none, so it is not carried.
fn holder_key<T: RegistryEntry + foton_registry::RegistryHolderEntry>(
    holder: &RegistryHolder<T>,
) -> Option<&Identifier> {
    match holder {
        RegistryHolder::Reference(entry) => Some(entry.key()),
        RegistryHolder::Direct(_) => None,
    }
}

/// Appends every component this module carries, in the order Java reads them.
pub(crate) fn describe(stack: &ItemStack, out: &mut String) {
    if let Some(lore) = stack.get(LORE) {
        for line in lore.lines() {
            if let Some(json) = adventure_component_to_json(line) {
                field(out, "lorejsonhex", hex(&json));
            }
        }
    }
    if let Some(display) = stack.get(TOOLTIP_DISPLAY) {
        for hidden in display.hidden_components() {
            field(out, "hide", hidden);
        }
    }
    // Prototype glint is inherent to the type, not a plugin metadata override.
    if stack
        .patch()
        .get_entry(ENCHANTMENT_GLINT_OVERRIDE.key())
        .is_some()
        && let Some(glint) = stack.get(ENCHANTMENT_GLINT_OVERRIDE)
    {
        field(out, "glint", glint);
    }
    // Only a value set on this stack: the prototype's size is the type's,
    // which Java already knows.
    if stack.patch().get_entry(MAX_STACK_SIZE.key()).is_some()
        && let Some(size) = stack.get(MAX_STACK_SIZE)
    {
        field(out, "maxstack", size);
    }
    if let Some(cooldown) = stack.get(USE_COOLDOWN) {
        field(out, "cooldown", cooldown.seconds);
        if let Some(group) = &cooldown.cooldown_group {
            field(out, "cooldowngroup", group);
        }
    }
    if let Some(layers) = stack.get(BANNER_PATTERNS) {
        for layer in layers.layers() {
            if let Some(key) = holder_key(layer.pattern()) {
                field(
                    out,
                    "pattern",
                    format!("{key},{}", layer.color().serialized_name()),
                );
            }
        }
    }
    if let Some(color) = stack.get(BASE_COLOR) {
        field(out, "basecolor", color.serialized_name());
    }
    if let Some(trim) = stack.get(TRIM)
        && let (Some(material), Some(pattern)) =
            (holder_key(trim.material()), holder_key(trim.pattern()))
    {
        field(out, "trim", format!("{material},{pattern}"));
    }
    if let Some(instrument) = stack.get(INSTRUMENT)
        && let Some(key) = holder_key(instrument.instrument())
    {
        field(out, "instrument", key);
    }
    // Only modifiers set on this stack; the prototype's belong to the type.
    if stack.patch().get_entry(ATTRIBUTE_MODIFIERS.key()).is_some()
        && let Some(modifiers) = stack.get(ATTRIBUTE_MODIFIERS)
    {
        for entry in &modifiers.modifiers {
            field(
                out,
                "attrmod",
                format!(
                    "{},{},{},{},{}",
                    entry.attribute.key,
                    entry.id,
                    entry.amount,
                    entry.operation.name(),
                    entry.slot.name()
                ),
            );
        }
    }
    if let Some(profile) = stack.get(PROFILE) {
        describe_profile(profile, out);
    }
    if let Some(data) = stack.get(CUSTOM_DATA)
        && !data.is_empty()
        && let Some(snbt) = to_canonical_snbt(&NbtTag::Compound(data.copy_tag()))
    {
        field(out, "customhex", hex(&snbt));
    }
}

/// Strictly decode only the current parent's established component fields.
pub(super) fn parse(stack: &mut ItemStack, fields: &[&str]) -> Option<()> {
    let mut hidden = Vec::new();
    let mut layers = Vec::new();
    let mut cooldown = None;
    let mut cooldown_group = None;
    let mut lore = Vec::new();
    let mut modifiers = Vec::new();
    let mut profile = ProfileFields::default();
    for (name, value) in fields.iter().filter_map(|field| field.split_once('=')) {
        match name {
            "attrmod" => modifiers.push(attribute_modifier(value)?),
            "profileid" | "profilename" | "profileprop" => profile.read(name, value)?,
            "lorejsonhex" => lore.push(adventure_component_from_json(&unhex(value)?)?),
            "hide" => hidden.push(value.parse::<Identifier>().ok()?),
            "glint" => {
                stack.set(ENCHANTMENT_GLINT_OVERRIDE, value.parse::<bool>().ok()?);
            }
            "maxstack" => {
                let size = value.parse::<i32>().ok()?;
                if !(1..=99).contains(&size) {
                    return None;
                }
                stack.set(MAX_STACK_SIZE, size);
            }
            "cooldown" => {
                let seconds = value.parse::<f32>().ok()?;
                if !seconds.is_finite() || seconds <= 0.0 {
                    return None;
                }
                cooldown = Some(seconds);
            }
            "cooldowngroup" => cooldown_group = Some(value.parse::<Identifier>().ok()?),
            "pattern" => layers.push(banner_layer(value)?),
            "basecolor" => {
                stack.set(BASE_COLOR, DyeColor::from_serialized_name(value)?);
            }
            "trim" => {
                stack.set(TRIM, armor_trim(value)?);
            }
            "instrument" => {
                let key = value.parse::<Identifier>().ok()?;
                stack.set(
                    INSTRUMENT,
                    InstrumentComponent::new(RegistryHolder::Reference(
                        REGISTRY.instruments.by_key(&key)?,
                    )),
                );
            }
            "customhex" => {
                let data = parse_snbt_compound(&unhex(value)?).ok()?;
                stack.set(CUSTOM_DATA, CustomData::try_from_compound(data)?);
            }
            _ => {}
        }
    }
    if !lore.is_empty() {
        stack.set(LORE, ItemLore::new(lore).ok()?);
    }
    if !hidden.is_empty() {
        let hide_tooltip = stack
            .get(TOOLTIP_DISPLAY)
            .is_some_and(|display| display.hide_tooltip);
        stack.set(
            TOOLTIP_DISPLAY,
            TooltipDisplay::from_hidden_components(hide_tooltip, hidden),
        );
    }
    if !layers.is_empty() {
        stack.set(BANNER_PATTERNS, BannerPatternLayers::new(layers));
    }
    if !modifiers.is_empty() {
        stack.set(ATTRIBUTE_MODIFIERS, ItemAttributeModifiers { modifiers });
    }
    if let Some(profile) = profile.build()? {
        stack.set(PROFILE, profile);
    }
    if let Some(seconds) = cooldown {
        stack.set(USE_COOLDOWN, UseCooldown::new(seconds, cooldown_group));
    } else if cooldown_group.is_some() {
        return None;
    }
    Some(())
}

/// `attribute,id,amount,operation,slot`. The tooltip display is not carried;
/// an edit keeps the one the base entry had.
pub(crate) fn attribute_modifier(value: &str) -> Option<ItemAttributeModifierEntry> {
    let mut parts = value.split(',');
    let attribute = REGISTRY
        .attributes
        .by_key(&parts.next()?.parse::<Identifier>().ok()?)?;
    let id = parts.next()?.parse::<Identifier>().ok()?;
    let amount = parts.next()?.parse::<f64>().ok()?;
    let operation = AttributeModifierOperation::by_name(parts.next()?)?;
    let slot = EquipmentSlotGroup::by_name(parts.next()?)?;
    if !amount.is_finite() || parts.next().is_some() {
        return None;
    }
    Some(ItemAttributeModifierEntry {
        attribute,
        id,
        amount,
        operation,
        slot,
        display: ItemAttributeModifierDisplay::Default,
    })
}

/// `profileid`, `profilename` and one `profileprop` per authlib property.
fn describe_profile(profile: &ResolvableProfile, out: &mut String) {
    let (id, name, properties) = match profile.contents() {
        ResolvableProfileContents::StaticFull(full) => {
            (Some(full.id()), Some(full.name()), full.properties())
        }
        ResolvableProfileContents::DynamicName(name) => (None, Some(name.as_str()), &[][..]),
        ResolvableProfileContents::DynamicId(id) => (Some(*id), None, &[][..]),
        ResolvableProfileContents::StaticPartial(partial) => {
            (partial.id(), partial.name(), partial.properties())
        }
    };
    if let Some(id) = id {
        field(out, "profileid", id);
    }
    if let Some(name) = name {
        field(out, "profilename", hex(name));
    }
    for property in properties {
        field(
            out,
            "profileprop",
            format!(
                "{},{},{}",
                hex(property.name()),
                hex(property.value()),
                property.signature().map(hex).unwrap_or_default()
            ),
        );
    }
}

#[derive(Default)]
struct ProfileFields {
    id: Option<uuid::Uuid>,
    name: Option<String>,
    properties: Vec<ProfileProperty>,
}

impl ProfileFields {
    fn read(&mut self, field: &str, value: &str) -> Option<()> {
        match field {
            "profileid" => self.id = Some(value.parse().ok()?),
            "profilename" => self.name = Some(unhex(value)?),
            _ => {
                let mut parts = value.split(',');
                let name = unhex(parts.next()?)?;
                let value = unhex(parts.next()?)?;
                let signature = unhex(parts.next()?)?;
                if parts.next().is_some() {
                    return None;
                }
                self.properties.push(
                    ProfileProperty::new(name, value, (!signature.is_empty()).then_some(signature))
                        .ok()?,
                );
            }
        }
        Some(())
    }

    /// The component in vanilla's canonical shape; `Some(None)` is no profile at all.
    #[expect(
        clippy::option_option,
        reason = "the outer option reports a malformed field, the inner one an absent profile"
    )]
    fn build(self) -> Option<Option<ResolvableProfile>> {
        let patch = PlayerSkinPatch::default();
        Some(Some(match (self.id, self.name) {
            (None, None) if self.properties.is_empty() => return Some(None),
            (Some(id), Some(name)) => ResolvableProfile::static_full(
                StoredGameProfile::new(id, name, self.properties).ok()?,
                patch,
            ),
            (None, Some(name)) if self.properties.is_empty() => {
                ResolvableProfile::dynamic_name(name, patch).ok()?
            }
            (Some(id), None) if self.properties.is_empty() => {
                ResolvableProfile::dynamic_id(id, patch)
            }
            (id, name) => ResolvableProfile::static_partial(
                PartialProfile::new(name, id, self.properties).ok()?,
                patch,
            ),
        }))
    }
}

fn banner_layer(value: &str) -> Option<BannerPatternLayer> {
    let (pattern, color) = value.split_once(',')?;
    let pattern = REGISTRY
        .banner_patterns
        .by_key(&pattern.parse::<Identifier>().ok()?)?;
    let color = DyeColor::from_serialized_name(color)?;
    Some(BannerPatternLayer::new(
        RegistryHolder::Reference(pattern),
        color,
    ))
}

fn armor_trim(value: &str) -> Option<ArmorTrim> {
    let (material, pattern) = value.split_once(',')?;
    let material = REGISTRY
        .trim_materials
        .by_key(&material.parse::<Identifier>().ok()?)?;
    let pattern = REGISTRY
        .trim_patterns
        .by_key(&pattern.parse::<Identifier>().ok()?)?;
    Some(ArmorTrim::new(
        RegistryHolder::Reference(material),
        RegistryHolder::Reference(pattern),
    ))
}

#[cfg(test)]
mod tests {
    use foton_registry::data_components::vanilla_components::{
        ATTRIBUTE_MODIFIERS, BANNER_PATTERNS, CUSTOM_DATA, CUSTOM_NAME, LORE, MAX_STACK_SIZE,
        POTION_CONTENTS, PROFILE, TOOLTIP_DISPLAY, TRIM, WRITTEN_BOOK_CONTENT,
    };
    use text_components::TextComponent;

    use super::hex;
    use crate::natives::{describe_slot, parse_slot};
    use foton_registry::data_components::components::Filterable;

    fn slot(fields: &[&str]) -> String {
        fields.join("\u{1d}")
    }

    /// What Java writes must be what the server stores, and what the server
    /// describes must read back to the same item.
    #[test]
    fn components_survive_the_slot_string() {
        foton_registry::init_vanilla_registry();
        let custom = hex(r#"{PublicBukkitValues:{"zelda:owner":"link"},other:1b}"#);
        let text = slot(&[
            "minecraft:diamond_chestplate 1",
            "trim=minecraft:gold,minecraft:wild",
            "hide=minecraft:trim",
            "hide=minecraft:attribute_modifiers",
            "glint=true",
            "maxstack=16",
            "cooldown=2.5",
            "cooldowngroup=zelda:whistle",
            &format!("customhex={custom}"),
        ]);
        let stack = parse_slot(&text).expect("a readable slot");
        assert!(stack.get(TRIM).is_some());
        assert_eq!(stack.get(MAX_STACK_SIZE), Some(&16));
        let hidden = stack
            .get(TOOLTIP_DISPLAY)
            .map(|display| display.hidden_components().len());
        assert_eq!(hidden, Some(2));
        assert!(stack.get(CUSTOM_DATA).is_some_and(|data| !data.is_empty()));

        let described = describe_slot(&stack);
        let again = parse_slot(&described).expect("the description reads back");
        assert_eq!(
            describe_slot(&again),
            described,
            "a second trip changes nothing"
        );
        for field in [
            "trim=minecraft:gold,minecraft:wild",
            "glint=true",
            "maxstack=16",
            "cooldown=2.5",
            "cooldowngroup=zelda:whistle",
        ] {
            assert!(
                described.contains(field),
                "{field} missing from {described}"
            );
        }
    }

    /// The prototype's modifiers are not described; an explicit list is, in order,
    /// and a malformed entry refuses the whole slot rather than being dropped.
    #[test]
    fn attribute_modifiers_and_profile_survive_the_slot_string() {
        foton_registry::init_vanilla_registry();
        let plain = describe_slot(&parse_slot("minecraft:iron_chestplate 1").expect("a slot"));
        assert!(!plain.contains("attrmod"), "{plain}");

        let text = slot(&[
            "minecraft:elytra 1",
            "attrmod=minecraft:armor,zelda:a,6,add_value,chest",
            "attrmod=minecraft:luck,zelda:b,-0.5,add_multiplied_total,any",
        ]);
        let stack = parse_slot(&text).expect("a readable slot");
        let modifiers = stack.get(ATTRIBUTE_MODIFIERS).expect("modifiers");
        assert_eq!(modifiers.modifiers.len(), 2);
        assert_eq!(
            modifiers.modifiers[1].amount.to_bits(),
            (-0.5_f64).to_bits()
        );
        let described = describe_slot(&stack);
        assert_eq!(parse_slot(&described), Some(stack), "{described}");
        for bad in [
            "attrmod=minecraft:armor,zelda:a,NaN,add_value,chest",
            "attrmod=minecraft:armor,zelda:a,1,multiply,chest",
            "attrmod=minecraft:nonsense,zelda:a,1,add_value,chest",
        ] {
            assert!(parse_slot(&slot(&["minecraft:elytra 1", bad])).is_none());
        }

        let textures = format!("{},{},{}", hex("textures"), hex("dGV4"), hex("c2ln"));
        let id = "00000000-0000-0000-0000-000000000012";
        for fields in [
            vec![
                format!("profileid={id}"),
                format!("profilename={}", hex("Link")),
                format!("profileprop={textures}"),
            ],
            vec![format!("profilename={}", hex("Link"))],
            vec![format!("profileid={id}")],
        ] {
            let mut parts = vec!["minecraft:player_head 1".to_owned()];
            parts.extend(fields);
            let head = parse_slot(&parts.join("\u{1d}")).expect("a readable head");
            let profile = head.get(PROFILE).expect("profile");
            let described = describe_slot(&head);
            assert_eq!(
                parse_slot(&described).and_then(|again| again.get(PROFILE).cloned()),
                Some(profile.clone()),
                "{described}"
            );
        }
    }

    /// A shield's layers keep their order, which is the drawing.
    #[test]
    fn banner_layers_keep_their_order() {
        foton_registry::init_vanilla_registry();
        let text = slot(&[
            "minecraft:shield 1",
            "basecolor=white",
            "pattern=minecraft:cross,red",
            "pattern=minecraft:border,blue",
        ]);
        let stack = parse_slot(&text).expect("a readable slot");
        assert_eq!(
            stack
                .get(BANNER_PATTERNS)
                .map(|layers| layers.layers().len()),
            Some(2)
        );
        let described = describe_slot(&stack);
        let cross = described.find("pattern=minecraft:cross,red");
        let border = described.find("pattern=minecraft:border,blue");
        assert!(cross.is_some() && cross < border, "{described}");
        assert!(described.contains("basecolor=white"), "{described}");
    }

    /// Book pages cross as JSON text, and styled text survives it.
    #[test]
    fn written_book_pages_are_json_components() {
        foton_registry::init_vanilla_registry();
        let page = r#"{"text":"one","color":"red","bold":true}"#;
        let text = slot(&[
            "minecraft:written_book 1",
            &format!("booktitlehex={}", hex("Livre")),
            &format!("bookauthorhex={}", hex("Hyrule")),
            "bookgen=0",
            &format!("bookpagehex={}", hex(page)),
        ]);
        let stack = parse_slot(&text).expect("a readable slot");
        let book = stack
            .get(WRITTEN_BOOK_CONTENT)
            .expect("the book content was set");
        assert_eq!(book.title().raw(), "Livre");
        assert_eq!(book.author(), "Hyrule");
        let expected: TextComponent = serde_json::from_str(page).expect("valid JSON text");
        assert_eq!(book.pages().first().map(Filterable::raw), Some(&expected));
        let again = parse_slot(&describe_slot(&stack)).expect("the description reads back");
        assert_eq!(again.get(WRITTEN_BOOK_CONTENT), Some(book));
    }

    /// A named field is never read as the potion effects, the one field without a name.
    #[test]
    fn named_fields_are_not_potion_effects() {
        foton_registry::init_vanilla_registry();
        let stack = parse_slot(&slot(&["minecraft:potion 1", "glint=true", "speed,20,1;"]))
            .expect("a readable slot");
        assert!(stack.get(TRIM).is_none());
        assert!(stack.get(POTION_CONTENTS).is_some());
    }

    /// A colored name set by a plugin is the name the item carries, and
    /// what the server hands back reads as the same component.
    #[test]
    fn a_styled_name_keeps_its_style() {
        foton_registry::init_vanilla_registry();
        let name = r#"{"text":"Bouclier","color":"gold","italic":false}"#;
        let stack = parse_slot(&slot(&[
            "minecraft:shield 1",
            &format!("namejsonhex={}", hex(name)),
            &format!(
                "lorejsonhex={}",
                hex(r#"{"text":"une ligne","color":"gray"}"#)
            ),
        ]))
        .expect("a readable slot");
        let expected: TextComponent = serde_json::from_str(name).expect("valid JSON text");
        assert_eq!(stack.get(CUSTOM_NAME), Some(&expected));
        assert_eq!(stack.get(LORE).map(|lore| lore.lines().len()), Some(1));
        let described = describe_slot(&stack);
        let round_trip = parse_slot(&described).expect("styled description reads back");
        assert_eq!(round_trip.get(CUSTOM_NAME), Some(&expected));
        assert_eq!(round_trip.get(LORE), stack.get(LORE));
    }
}
