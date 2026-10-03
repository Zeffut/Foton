//! Book item components crossing the Bukkit inventory slot bridge.

use std::fmt::Write as _;

use foton_registry::data_components::components::{
    Filterable, WritableBookContent, WrittenBookContent,
};
use foton_registry::data_components::vanilla_components::{
    CUSTOM_NAME, WRITABLE_BOOK_CONTENT, WRITTEN_BOOK_CONTENT,
};
use foton_registry::item_stack::ItemStack;
use foton_registry::vanilla_items;

use crate::natives::{adventure_component_from_json, adventure_component_to_json};

fn field(out: &mut String, name: &str, value: &str) {
    out.push('\u{1d}');
    out.push_str(name);
    out.push('=');
    for byte in value.as_bytes() {
        let _ = write!(out, "{byte:02x}");
    }
}

fn unhex(value: &str) -> Option<String> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    let bytes = (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(value.get(index..index + 2)?, 16).ok())
        .collect::<Option<Vec<_>>>()?;
    String::from_utf8(bytes).ok()
}

pub(crate) fn describe_name(stack: &ItemStack, out: &mut String) -> Option<()> {
    if let Some(name) = stack.get(CUSTOM_NAME) {
        field(out, "namehex", &name.to_string());
        field(out, "namejsonhex", &adventure_component_to_json(name)?);
    }
    Some(())
}

pub(crate) fn describe(stack: &ItemStack, out: &mut String) -> Option<()> {
    if let Some(book) = stack.get(WRITTEN_BOOK_CONTENT) {
        field(out, "booktitlehex", book.title().raw());
        field(out, "bookauthorhex", book.author());
        out.push('\u{1d}');
        let _ = write!(out, "bookgen={}", book.generation());
        for page in book.pages() {
            field(
                out,
                "bookpagehex",
                &adventure_component_to_json(page.raw())?,
            );
        }
    } else if let Some(book) = stack.get(WRITABLE_BOOK_CONTENT) {
        for page in book.pages() {
            field(out, "bookpagehex", page.raw());
        }
    }
    Some(())
}

pub(crate) fn parse(stack: &mut ItemStack, metadata: &[&str]) -> Option<()> {
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
        let book = WrittenBookContent::new(
            Filterable::pass_through(title),
            author,
            generation,
            pages,
            true,
        )
        .ok()?;
        stack.set(WRITTEN_BOOK_CONTENT, book);
    } else if stack.is(&vanilla_items::WRITABLE_BOOK) {
        let pages = metadata
            .iter()
            .filter_map(|part| part.strip_prefix("bookpagehex="))
            .map(|page| unhex(page).map(Filterable::pass_through))
            .collect::<Option<Vec<_>>>()?;
        if !pages.is_empty() {
            stack.set(WRITABLE_BOOK_CONTENT, WritableBookContent::new(pages).ok()?);
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{field, unhex};
    use crate::natives::{adventure_component_from_json, describe_slot, parse_slot};
    use foton_registry::data_components::vanilla_components::{
        CUSTOM_DATA, CUSTOM_NAME, WRITTEN_BOOK_CONTENT,
    };
    use foton_registry::init_vanilla_registry;
    use foton_registry::item_stack::ItemStack;
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
            "page colour lost: {described_page}"
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
        assert!(unhex("f").is_none());
        assert!(unhex("xx").is_none());
    }
}
