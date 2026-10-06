//! Book item components crossing the Bukkit inventory slot bridge.
//! Bukkit exposes only raw pages. Filtered projections travel as typed native
//! NBT and the resolved flag travels separately, so cover edits retain it.
//! Native content is accepted back only if every visible field still matches.

mod extensions;

pub(crate) use extensions::attribute_modifier;

use std::fmt::Write as _;
use std::io::Cursor;

use foton_registry::data_components::components::{
    Filterable, WritableBookContent, WrittenBookContent,
};
use foton_registry::data_components::vanilla_components::{
    CUSTOM_NAME, WRITABLE_BOOK_CONTENT, WRITTEN_BOOK_CONTENT,
};
use foton_registry::item_stack::ItemStack;
use foton_registry::vanilla_items;
use simdnbt::borrow::read_tag;
use simdnbt::owned::NbtTag;
use simdnbt::{FromNbtTag, ToNbtTag as _};

use crate::natives::{adventure_component_from_json, adventure_component_to_json};

fn field(out: &mut String, name: &str, value: &str) {
    field_bytes(out, name, value.as_bytes());
}

fn field_bytes(out: &mut String, name: &str, value: &[u8]) {
    out.push('\u{1d}');
    out.push_str(name);
    out.push('=');
    for byte in value {
        let _ = write!(out, "{byte:02x}");
    }
}

fn unhex_bytes(value: &str) -> Option<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(value.get(index..index + 2)?, 16).ok())
        .collect::<Option<Vec<_>>>()
}

fn unhex(value: &str) -> Option<String> {
    String::from_utf8(unhex_bytes(value)?).ok()
}

fn native_book<T: FromNbtTag>(value: &str) -> Option<T> {
    let bytes = unhex_bytes(value)?;
    let mut cursor = Cursor::new(bytes.as_slice());
    let book = T::from_nbt_tag(read_tag(&mut cursor).ok()?.as_tag())?;
    (cursor.position() as usize == bytes.len()).then_some(book)
}

fn push_book_raw(out: &mut String, value: NbtTag) {
    let mut bytes = Vec::new();
    value.write(&mut bytes);
    field_bytes(out, "bookrawhex", &bytes);
}

fn book_resolved(metadata: &[&str]) -> Option<bool> {
    match metadata
        .iter()
        .find_map(|part| part.strip_prefix("bookresolved="))
    {
        Some("true") => Some(true),
        Some("false") | None => Some(false),
        Some(_) => None,
    }
}

pub(crate) fn describe_name(stack: &ItemStack, out: &mut String) -> Option<()> {
    if let Some(name) = stack.get(CUSTOM_NAME) {
        field(out, "namehex", &name.to_string());
        field(out, "namejsonhex", &adventure_component_to_json(name)?);
    }
    Some(())
}

pub(crate) fn describe(stack: &ItemStack, out: &mut String) -> Option<()> {
    extensions::describe(stack, out);
    if let Some(book) = stack.get(WRITTEN_BOOK_CONTENT) {
        field(out, "booktitlehex", book.title().raw());
        field(out, "bookauthorhex", book.author());
        out.push('\u{1d}');
        let _ = write!(out, "bookgen={}", book.generation());
        out.push('\u{1d}');
        let _ = write!(out, "bookresolved={}", book.resolved());
        for page in book.pages() {
            field(
                out,
                "bookpagehex",
                &adventure_component_to_json(page.raw())?,
            );
        }
        if book.title().filtered().is_some()
            || book.pages().iter().any(|page| page.filtered().is_some())
        {
            push_book_raw(out, book.clone().to_nbt_tag());
        }
    } else if let Some(book) = stack.get(WRITABLE_BOOK_CONTENT) {
        for page in book.pages() {
            field(out, "bookpagehex", page.raw());
        }
        if book.pages().iter().any(|page| page.filtered().is_some()) {
            push_book_raw(out, book.clone().to_nbt_tag());
        }
    }
    Some(())
}

pub(crate) fn parse(stack: &mut ItemStack, metadata: &[&str]) -> Option<()> {
    extensions::parse(stack, metadata)?;
    if stack.is(&vanilla_items::WRITTEN_BOOK) {
        let title = metadata
            .iter()
            .find_map(|part| part.strip_prefix("booktitlehex="));
        let author = metadata
            .iter()
            .find_map(|part| part.strip_prefix("bookauthorhex="));
        let generation = metadata
            .iter()
            .find_map(|part| part.strip_prefix("bookgen="));
        let resolved = book_resolved(metadata)?;
        let page_fields = metadata
            .iter()
            .filter_map(|part| part.strip_prefix("bookpagehex="));
        let pages = page_fields
            .map(|value| unhex(value).and_then(|json| adventure_component_from_json(&json)))
            .map(|page| page.map(Filterable::pass_through))
            .collect::<Option<Vec<_>>>()?;
        if title.is_none() && author.is_none() && generation.is_none() && pages.is_empty() {
            return Some(());
        }
        let title = match title {
            Some(value) => unhex(value)?,
            None => String::new(),
        };
        let author = match author {
            Some(value) => unhex(value)?,
            None => String::new(),
        };
        let generation = match generation {
            Some(value) => value.parse::<i32>().ok()?,
            None => 0,
        };
        let visible = WrittenBookContent::new(
            Filterable::pass_through(title),
            author,
            generation,
            pages,
            resolved,
        )
        .ok()?;
        let book = if let Some(raw) = metadata
            .iter()
            .find_map(|part| part.strip_prefix("bookrawhex="))
        {
            let original: WrittenBookContent = native_book(raw)?;
            if original.title().raw() != visible.title().raw()
                || original.author() != visible.author()
                || original.generation() != visible.generation()
                || original.resolved() != visible.resolved()
                || original.pages().len() != visible.pages().len()
                || original
                    .pages()
                    .iter()
                    .zip(visible.pages())
                    .any(|(left, right)| left.raw() != right.raw())
            {
                return None;
            }
            original
        } else {
            visible
        };
        stack.set(WRITTEN_BOOK_CONTENT, book);
    } else if stack.is(&vanilla_items::WRITABLE_BOOK) {
        let pages = metadata
            .iter()
            .filter_map(|part| part.strip_prefix("bookpagehex="))
            .map(|page| unhex(page).map(Filterable::pass_through))
            .collect::<Option<Vec<_>>>()?;
        if !pages.is_empty() || metadata.iter().any(|part| part.starts_with("bookrawhex=")) {
            let visible = WritableBookContent::new(pages).ok()?;
            let book = if let Some(raw) = metadata
                .iter()
                .find_map(|part| part.strip_prefix("bookrawhex="))
            {
                let original: WritableBookContent = native_book(raw)?;
                if original.pages().len() != visible.pages().len()
                    || original
                        .pages()
                        .iter()
                        .zip(visible.pages())
                        .any(|(left, right)| left.raw() != right.raw())
                {
                    return None;
                }
                original
            } else {
                visible
            };
            stack.set(WRITABLE_BOOK_CONTENT, book);
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{field, unhex};
    use crate::natives::{adventure_component_from_json, describe_slot, parse_slot};
    use foton_registry::data_components::components::{Filterable, WritableBookContent};
    use foton_registry::data_components::vanilla_components::{
        CUSTOM_DATA, CUSTOM_NAME, WRITABLE_BOOK_CONTENT, WRITTEN_BOOK_CONTENT,
    };
    use foton_registry::init_vanilla_registry;
    use foton_registry::item_stack::ItemStack;
    use foton_registry::vanilla_items;
    use simdnbt::FromNbtTag as _;
    use simdnbt::borrow::read_tag;

    #[test]
    fn zelda_book_keeps_styled_pages_cover_and_pdc_through_save() {
        init_vanilla_registry();
        let page = r##"{"text":"","extra":[{"text":"Herbes","color":"dark_green"},{"text":"★ Radis","color":"#e6af31","bold":true},{"text":"","font":"minecraft:cuisine","color":"white","italic":false}]}"##;
        let cover = r#"{"text":"Livre de Cuisine","color":"gold","italic":false}"#;
        let mut encoded = "minecraft:written_book 1".to_owned();
        field(&mut encoded, "booktitlehex", "Livre de Cuisine");
        field(&mut encoded, "bookauthorhex", "Les cuisiniers d'Hyrule");
        encoded.push_str("\u{1d}bookgen=0");
        field(&mut encoded, "bookpagehex", page);
        field(&mut encoded, "namejsonhex", cover);
        encoded.push_str("\u{1d}pdcint=7a656c64616369763a6c697672655f63756973696e65:3");

        let stack = parse_slot(&encoded).expect("Zelda book should parse");
        let book = stack.get(WRITTEN_BOOK_CONTENT).expect("book component");
        assert_eq!(book.title().raw(), "Livre de Cuisine");
        assert_eq!(book.author(), "Les cuisiniers d'Hyrule");
        assert_eq!(book.generation(), 0);
        assert_eq!(book.pages().len(), 1);
        assert_eq!(
            book.pages()[0].raw(),
            &adventure_component_from_json(page).expect("styled page")
        );
        assert_eq!(
            stack.get(CUSTOM_NAME),
            Some(&adventure_component_from_json(cover).expect("cover"))
        );
        assert_eq!(
            stack
                .get(CUSTOM_DATA)
                .and_then(|data| data.as_compound().compound("PublicBukkitValues"))
                .and_then(|values| values.int("zeldaciv:livre_cuisine")),
            Some(3)
        );

        let described = describe_slot(&stack);
        let described_page = described
            .split('\u{1d}')
            .find_map(|part| part.strip_prefix("bookpagehex="))
            .and_then(unhex)
            .expect("styled page in native description");
        assert!(
            described_page.contains("minecraft:cuisine"),
            "glyph font lost: {described_page}"
        );
        assert!(
            described_page.contains("dark_green") || described_page.contains("#00aa00"),
            "page color lost: {described_page}"
        );
        let reparsed = parse_slot(&described).expect("native slot round trip");
        assert_eq!(reparsed, stack);
        let saved = stack.to_nbt_tag_ref();
        let mut bytes = Vec::new();
        saved.write(&mut bytes);
        let borrowed = read_tag(&mut Cursor::new(bytes.as_slice())).expect("saved item NBT");
        let reloaded = ItemStack::from_nbt_tag(borrowed.as_tag()).expect("reload saved book");
        assert_eq!(reloaded, stack);
    }

    #[test]
    fn malformed_written_book_cannot_silently_lose_its_pages() {
        init_vanilla_registry();
        let mut encoded = "minecraft:written_book 1".to_owned();
        field(&mut encoded, "bookpagehex", "{bad JSON");
        assert!(parse_slot(&encoded).is_none());
        let invalid_resolved = "minecraft:written_book 1\u{1d}bookresolved=maybe\u{1d}bookgen=0";
        assert!(parse_slot(invalid_resolved).is_none());
        assert!(unhex("f").is_none());
        assert!(unhex("xx").is_none());
    }

    #[test]
    fn filtered_writable_pages_survive_and_stale_passthrough_is_rejected() {
        init_vanilla_registry();
        let mut stack = ItemStack::new(&vanilla_items::WRITABLE_BOOK);
        stack.set(
            WRITABLE_BOOK_CONTENT,
            WritableBookContent::new(vec![Filterable::new(
                "visible".to_owned(),
                Some("filtered".to_owned()),
            )])
            .expect("writable book"),
        );
        let encoded = describe_slot(&stack);
        assert!(encoded.contains("bookrawhex="));
        assert_eq!(parse_slot(&encoded).as_ref(), Some(&stack));
        let mut bytes = Vec::new();
        stack.to_nbt_tag_ref().write(&mut bytes);
        let borrowed = read_tag(&mut Cursor::new(bytes.as_slice())).expect("filtered writable NBT");
        assert_eq!(ItemStack::from_nbt_tag(borrowed.as_tag()), Some(stack));

        let stale = encoded.replace("bookpagehex=76697369626c65", "bookpagehex=6368616e676564");
        assert!(
            parse_slot(&stale).is_none(),
            "stale native book must not override edited page"
        );
    }

    #[test]
    fn empty_styled_name_is_still_a_present_native_component() {
        init_vanilla_registry();
        let json = r#"{"text":"","color":"gold"}"#;
        let mut encoded = "minecraft:paper 1".to_owned();
        field(&mut encoded, "namejsonhex", json);
        let stack = parse_slot(&encoded).expect("styled empty name");
        assert!(stack.get(CUSTOM_NAME).is_some());
        let described = describe_slot(&stack);
        let rendered = described
            .split('\u{1d}')
            .find_map(|part| part.strip_prefix("namejsonhex="))
            .and_then(unhex)
            .expect("styled name JSON");
        assert!(
            rendered.contains("gold"),
            "empty name color lost: {rendered}"
        );
        assert_eq!(parse_slot(&described), Some(stack));
    }
}
