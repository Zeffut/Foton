//! Borrowed composite component codecs for exact lock predicates.
use foton_utils::serial::nbt_stream::NbtWrite;
use std::io;
use std::io::Result;

use foton_utils::serial::nbt_encode::{self, NbtEncode, check_depth, end, field};

use crate::data_components::components::{
    CustomModelData, Filterable, FireworkExplosion, Fireworks, WritableBookContent,
    WrittenBookContent,
};

struct List<'a, T>(&'a [T]);

impl<T: NbtEncode> NbtEncode for List<'_, T> {
    fn nbt_id(&self) -> u8 {
        9
    }
    fn write_nbt_payload(&self, writer: &mut dyn NbtWrite, depth: usize) -> Result<()> {
        nbt_encode::list(self.0, writer, depth)
    }
}

macro_rules! compound {
    ($ty:ty, $value:ident, $writer:ident, $depth:ident, $body:block) => {
        impl NbtEncode for $ty {
            fn nbt_id(&self) -> u8 { 10 }
            fn write_nbt_payload(&self, $writer: &mut dyn NbtWrite, $depth: usize) -> Result<()> {
                check_depth($depth)?;
                let $value = self;
                $body
                end($writer)
            }
        }
    };
}

impl<T: NbtEncode> NbtEncode for Filterable<T> {
    fn nbt_id(&self) -> u8 {
        10
    }
    fn write_nbt_payload(&self, writer: &mut dyn NbtWrite, depth: usize) -> Result<()> {
        field("raw", self.raw(), writer, depth)?;
        if let Some(filtered) = self.filtered() {
            field("filtered", filtered, writer, depth)?;
        }
        end(writer)
    }
}

compound!(WrittenBookContent, value, writer, depth, {
    field("title", value.title(), writer, depth)?;
    field("author", &value.author(), writer, depth)?;
    if value.generation() != 0 {
        field("generation", &value.generation(), writer, depth)?;
    }
    if !value.pages().is_empty() {
        field("pages", &WrittenPages(value.pages()), writer, depth)?;
    }
    if value.resolved() {
        field("resolved", &true, writer, depth)?;
    }
});

compound!(WritableBookContent, value, writer, depth, {
    if !value.pages().is_empty() {
        field("pages", &List(value.pages()), writer, depth)?;
    }
});

compound!(CustomModelData, value, writer, depth, {
    if !value.floats().is_empty() {
        field("floats", &List(value.floats()), writer, depth)?;
    }
    if !value.flags().is_empty() {
        field("flags", &List(value.flags()), writer, depth)?;
    }
    if !value.strings().is_empty() {
        field("strings", &List(value.strings()), writer, depth)?;
    }
    if !value.colors().is_empty() {
        field("colors", &List(value.colors()), writer, depth)?;
    }
});

compound!(FireworkExplosion, value, writer, depth, {
    field("shape", &value.shape().serialized_name(), writer, depth)?;
    if !value.colors().is_empty() {
        field("colors", &List(value.colors()), writer, depth)?;
    }
    if !value.fade_colors().is_empty() {
        field("fade_colors", &List(value.fade_colors()), writer, depth)?;
    }
    if value.has_trail() {
        field("has_trail", &true, writer, depth)?;
    }
    if value.has_twinkle() {
        field("has_twinkle", &true, writer, depth)?;
    }
});

compound!(Fireworks, value, writer, depth, {
    if value.flight_duration() > 255 {
        return Err(io::Error::other("Firework flight duration exceeds 255"));
    }
    if value.flight_duration() != 0 {
        field(
            "flight_duration",
            &(value.flight_duration() as u8 as i8),
            writer,
            depth,
        )?;
    }
    if !value.explosions().is_empty() {
        field("explosions", &List(value.explosions()), writer, depth)?;
    }
});

struct WrittenPages<'a>(&'a [Filterable<text_components::TextComponent>]);
struct RestrictedText<'a>(&'a text_components::TextComponent);
impl NbtEncode for RestrictedText<'_> {
    fn nbt_id(&self) -> u8 {
        self.0.nbt_id()
    }
    fn write_nbt_payload(&self, writer: &mut dyn NbtWrite, depth: usize) -> Result<()> {
        WrittenBookContent::validate_page(self.0, writer, depth)?;
        self.0.write_nbt_payload(writer, depth)
    }
}
impl NbtEncode for WrittenPages<'_> {
    fn nbt_id(&self) -> u8 {
        9
    }
    fn write_nbt_payload(&self, writer: &mut dyn NbtWrite, depth: usize) -> Result<()> {
        check_depth(depth)?;
        writer.ensure_remaining(self.0.len().saturating_add(5))?;
        writer.write_all(&[10])?;
        writer.write_all(
            &i32::try_from(self.0.len())
                .map_err(io::Error::other)?
                .to_be_bytes(),
        )?;
        for page in self.0 {
            field("raw", &RestrictedText(page.raw()), writer, depth + 1)?;
            if let Some(filtered) = page.filtered() {
                field("filtered", &RestrictedText(filtered), writer, depth + 1)?;
            }
            end(writer)?;
        }
        Ok(())
    }
}
