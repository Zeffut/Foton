//! The item components a plugin sets beyond name, lore and enchantments.
//!
//! One `name=value` field each in the slot string `natives::describe_slot`
//! writes and `natives::parse_slot` reads; `foton.ItemComponents` is the Java
//! half and must agree field for field. Anything unreadable in a field loses
//! that component, never the item.

use std::fmt::{Display, Write as _};

use foton_registry::data_components::components::{
    ArmorTrim, BannerPatternLayer, BannerPatternLayers, CustomData, Filterable,
    InstrumentComponent, TooltipDisplay, UseCooldown, WritableBookContent, WrittenBookContent,
};
use foton_registry::data_components::vanilla_components::{
    BANNER_PATTERNS, BASE_COLOR, CUSTOM_DATA, ENCHANTMENT_GLINT_OVERRIDE, INSTRUMENT,
    MAX_STACK_SIZE, TOOLTIP_DISPLAY, TRIM, USE_COOLDOWN, WRITABLE_BOOK_CONTENT,
    WRITTEN_BOOK_CONTENT,
};
use foton_registry::item_stack::ItemStack;
use foton_registry::{
    DyeColor, REGISTRY, RegistryEntry, RegistryExt as _, RegistryHolder, vanilla_items,
};
use foton_utils::Identifier;
use foton_utils::nbt::{parse_snbt_compound, to_canonical_snbt};
use simdnbt::owned::NbtTag;
use text_components::TextComponent;

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
    if let Some(display) = stack.get(TOOLTIP_DISPLAY) {
        for hidden in display.hidden_components() {
            field(out, "hide", hidden);
        }
    }
    if let Some(glint) = stack.get(ENCHANTMENT_GLINT_OVERRIDE) {
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
    if let Some(data) = stack.get(CUSTOM_DATA)
        && !data.is_empty()
        && let Some(snbt) = to_canonical_snbt(&NbtTag::Compound(data.copy_tag()))
    {
        field(out, "customhex", hex(&snbt));
    }
    if let Some(book) = stack.get(WRITTEN_BOOK_CONTENT) {
        field(out, "booktitlehex", hex(book.title().raw()));
        field(out, "bookauthorhex", hex(book.author()));
        field(out, "bookgen", book.generation());
        for page in book.pages() {
            if let Ok(json) = serde_json::to_string(page.raw()) {
                field(out, "bookpagehex", hex(&json));
            }
        }
    } else if let Some(book) = stack.get(WRITABLE_BOOK_CONTENT) {
        for page in book.pages() {
            field(out, "bookpagehex", hex(page.raw()));
        }
    }
}

/// Everything the book fields add up to, applied once they have all been read.
#[derive(Default)]
struct BookFields {
    title: Option<String>,
    author: Option<String>,
    generation: i32,
    pages: Vec<String>,
}

/// Applies the fields [`describe`] writes.
pub(crate) fn parse(stack: &mut ItemStack, fields: &[&str]) {
    let mut hidden = Vec::new();
    let mut layers = Vec::new();
    let mut cooldown: Option<f32> = None;
    let mut cooldown_group = None;
    let mut book = BookFields::default();
    for (name, value) in fields.iter().filter_map(|field| field.split_once('=')) {
        match name {
            "hide" => hidden.extend(value.parse::<Identifier>().ok()),
            "glint" => {
                if let Ok(glint) = value.parse::<bool>() {
                    stack.set(ENCHANTMENT_GLINT_OVERRIDE, glint);
                }
            }
            "maxstack" => {
                if let Ok(size @ 1..=99) = value.parse::<i32>() {
                    stack.set(MAX_STACK_SIZE, size);
                }
            }
            "cooldown" => cooldown = value.parse().ok().filter(|seconds: &f32| *seconds > 0.0),
            "cooldowngroup" => cooldown_group = value.parse::<Identifier>().ok(),
            "pattern" => layers.extend(banner_layer(value)),
            "basecolor" => {
                if let Some(color) = DyeColor::from_serialized_name(value) {
                    stack.set(BASE_COLOR, color);
                }
            }
            "trim" => {
                if let Some(trim) = armor_trim(value) {
                    stack.set(TRIM, trim);
                }
            }
            "instrument" => {
                if let Some(instrument) = value
                    .parse::<Identifier>()
                    .ok()
                    .and_then(|key| REGISTRY.instruments.by_key(&key))
                {
                    stack.set(
                        INSTRUMENT,
                        InstrumentComponent::new(RegistryHolder::Reference(instrument)),
                    );
                }
            }
            "customhex" => {
                if let Some(data) = unhex(value)
                    .and_then(|snbt| parse_snbt_compound(&snbt).ok())
                    .and_then(CustomData::try_from_compound)
                {
                    stack.set(CUSTOM_DATA, data);
                }
            }
            "booktitlehex" => book.title = unhex(value),
            "bookauthorhex" => book.author = unhex(value),
            "bookgen" => book.generation = value.parse().unwrap_or_default(),
            "bookpagehex" => book.pages.extend(unhex(value)),
            _ => {}
        }
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
    if let Some(seconds) = cooldown {
        stack.set(USE_COOLDOWN, UseCooldown::new(seconds, cooldown_group));
    }
    apply_book(stack, book);
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

/// A written book carries its cover and component pages; a writable one only
/// the text of its pages, which is all vanilla stores for it.
fn apply_book(stack: &mut ItemStack, book: BookFields) {
    if stack.is(&vanilla_items::WRITTEN_BOOK) {
        if book.title.is_none() && book.author.is_none() && book.pages.is_empty() {
            return;
        }
        let pages = book
            .pages
            .iter()
            .map(|json| {
                serde_json::from_str::<TextComponent>(json)
                    .unwrap_or_else(|_| TextComponent::plain(json.clone()))
            })
            .map(Filterable::pass_through)
            .collect();
        if let Ok(content) = WrittenBookContent::new(
            Filterable::pass_through(book.title.unwrap_or_default()),
            book.author.unwrap_or_default(),
            book.generation,
            pages,
            true,
        ) {
            stack.set(WRITTEN_BOOK_CONTENT, content);
        }
    } else if stack.is(&vanilla_items::WRITABLE_BOOK) && !book.pages.is_empty() {
        let pages = book
            .pages
            .into_iter()
            .map(Filterable::pass_through)
            .collect();
        if let Ok(content) = WritableBookContent::new(pages) {
            stack.set(WRITABLE_BOOK_CONTENT, content);
        }
    }
}

#[cfg(test)]
mod tests {
    use foton_registry::data_components::vanilla_components::{
        BANNER_PATTERNS, CUSTOM_DATA, MAX_STACK_SIZE, POTION_CONTENTS, TOOLTIP_DISPLAY, TRIM,
        WRITTEN_BOOK_CONTENT,
    };
    use text_components::TextComponent;

    use super::{Filterable, hex};
    use crate::natives::{describe_slot, parse_slot};

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
        let stack = parse_slot(&slot(&["minecraft:potion 1", "trim=a,b,c", "speed,20,1;"]))
            .expect("a readable slot");
        assert!(stack.get(TRIM).is_none());
        assert!(stack.get(POTION_CONTENTS).is_some());
    }
}
