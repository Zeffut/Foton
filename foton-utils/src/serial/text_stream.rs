//! Text codec wire output borrowing the original graph (no owned-NBT conversion).
use super::nbt_stream::{self, LimitedWriter, NbtWrite};
use simdnbt::owned::NbtTag;
use std::io::{Error, ErrorKind, Result, Write, sink};
use text_components::{
    TextComponent,
    content::{Content, NbtSource, Object, PlayerModel, Resolvable},
    custom::{CustomData, Payload},
    format::Format,
    interactivity::{ClickEvent, Dialog, HoverEvent, Interactivity},
};

/// Test-only observations of child-list classification work.
#[cfg(feature = "codec-test-support")]
pub mod child_classification_work {
    use std::cell::Cell;

    thread_local! { static CLASSIFICATIONS: Cell<usize> = const { Cell::new(0) }; }

    pub(super) fn visit() {
        CLASSIFICATIONS.set(CLASSIFICATIONS.get() + 1);
    }

    /// Counts child values classified by an actual codec route.
    pub fn measure(operation: impl FnOnce()) -> usize {
        CLASSIFICATIONS.set(0);
        operation();
        CLASSIFICATIONS.get()
    }
}

/// Encodes text after a budget-limited wire-size pass.
pub fn write_bounded(value: &TextComponent, maximum: usize, writer: &mut dyn Write) -> Result<()> {
    let mut sink = sink();
    write(value, &mut LimitedWriter::new(&mut sink, maximum))?;
    write(value, &mut LimitedWriter::new(writer, maximum))
}
/// Encodes the component codec representation without materializing owned NBT.
pub fn write(value: &TextComponent, writer: &mut dyn NbtWrite) -> Result<()> {
    writer.write_all(&[if plain(value) { 8 } else { 10 }])?;
    write_payload(value, writer, 0)
}
/// Returns the text codec's concrete NBT tag ID.
pub(crate) fn tag_id(value: &TextComponent) -> u8 {
    if plain(value) { 8 } else { 10 }
}
fn plain(value: &TextComponent) -> bool {
    matches!(value.content, Content::Text { .. })
        && value.children.is_empty()
        && value.format.is_none()
        && value.interactions.is_none()
}
fn invalid(message: &'static str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}
/// Streams a Rust string as length-prefixed MUTF-8.
pub(crate) fn write_utf(value: &str, writer: &mut dyn NbtWrite) -> Result<()> {
    writer.ensure_remaining(value.len().saturating_add(2))?;
    let mut length = 0usize;
    for unit in value.encode_utf16() {
        length += if (1..=0x7f).contains(&unit) {
            1
        } else if unit <= 0x7ff {
            2
        } else {
            3
        };
        writer.ensure_remaining(length.saturating_add(2))?;
        if length > u16::MAX as usize {
            return Err(invalid("text NBT string exceeds u16 length"));
        }
    }
    writer.write_all(&(length as u16).to_be_bytes())?;
    for unit in value.encode_utf16() {
        if (1..=0x7f).contains(&unit) {
            writer.write_all(&[unit as u8])?;
        } else if unit <= 0x7ff {
            writer.write_all(&[0xc0 | (unit >> 6) as u8, 0x80 | (unit & 63) as u8])?;
        } else {
            writer.write_all(&[
                0xe0 | (unit >> 12) as u8,
                0x80 | ((unit >> 6) & 63) as u8,
                0x80 | (unit & 63) as u8,
            ])?;
        }
    }
    Ok(())
}
fn field(kind: u8, name: &str, writer: &mut dyn NbtWrite) -> Result<()> {
    writer.write_all(&[kind])?;
    write_utf(name, writer)
}
fn compound_field(name: &str, writer: &mut dyn NbtWrite, depth: usize) -> Result<()> {
    if depth + 1 >= writer.container_limit() {
        return Err(invalid("text nesting exceeds limit"));
    }
    field(10, name, writer)
}
fn string(name: &str, value: &str, writer: &mut dyn NbtWrite) -> Result<()> {
    field(8, name, writer)?;
    write_utf(value, writer)
}
fn boolean(name: &str, value: bool, writer: &mut dyn NbtWrite) -> Result<()> {
    field(1, name, writer)?;
    writer.write_all(&[u8::from(value)])
}
fn integer(name: &str, value: i32, writer: &mut dyn NbtWrite) -> Result<()> {
    field(3, name, writer)?;
    writer.write_all(&value.to_be_bytes())
}
fn nested(
    name: &str,
    value: &TextComponent,
    writer: &mut dyn NbtWrite,
    depth: usize,
) -> Result<()> {
    field(if plain(value) { 8 } else { 10 }, name, writer)?;
    write_payload(value, writer, depth + 1)
}
fn nbt(name: &str, value: &NbtTag, writer: &mut dyn NbtWrite, depth: usize) -> Result<()> {
    field(value.id(), name, writer)?;
    nbt_stream::write_payload(value, writer, depth + 1)
}
fn end(writer: &mut dyn NbtWrite) -> Result<()> {
    writer.write_all(&[0])
}
fn list(
    name: &str,
    values: &[TextComponent],
    writer: &mut dyn NbtWrite,
    depth: usize,
) -> Result<()> {
    field(9, name, writer)?;
    writer.ensure_remaining(values.len().saturating_mul(2).saturating_add(5))?;
    let all_plain = !values.is_empty()
        && values.iter().all(|value| {
            #[cfg(feature = "codec-test-support")]
            child_classification_work::visit();
            plain(value)
        });
    if !all_plain && depth + 1 >= writer.container_limit() {
        return Err(invalid("text nesting exceeds limit"));
    }
    writer.write_all(&[if all_plain { 8 } else { 10 }])?;
    writer.write_all(
        &i32::try_from(values.len())
            .map_err(|_| invalid("text list exceeds i32 length"))?
            .to_be_bytes(),
    )?;
    for value in values {
        if !all_plain && plain(value) {
            nested("", value, writer, depth + 2)?;
            end(writer)?;
        } else {
            write_payload(value, writer, depth + 2)?;
        }
    }
    Ok(())
}
fn custom(value: &CustomData, writer: &mut dyn NbtWrite, depth: usize) -> Result<()> {
    string("id", &value.id, writer)?;
    if let Payload::Nbt(value) = &value.payload {
        nbt("payload", value.as_nbt(), writer, depth)?;
    }
    Ok(())
}
/// Streams the text payload without its NBT tag ID.
pub fn write_payload(value: &TextComponent, writer: &mut dyn NbtWrite, depth: usize) -> Result<()> {
    if !plain(value) && depth >= writer.container_limit() {
        return Err(invalid("text nesting exceeds limit"));
    }
    if plain(value)
        && let Content::Text { text } = &value.content
    {
        return write_utf(text, writer);
    }
    content(&value.content, writer, depth)?;
    style_fields(&value.format, &value.interactions, writer, depth)?;
    if !value.children.is_empty() {
        list("extra", &value.children, writer, depth)?;
    }
    end(writer)
}
/// Streams only the style of a component: `Style.Serializer.CODEC`'s compound.
pub fn write_style(
    format: &Format,
    interactions: &Interactivity,
    writer: &mut dyn NbtWrite,
) -> Result<()> {
    writer.write_all(&[10])?;
    style_fields(format, interactions, writer, 0)?;
    end(writer)
}
/// Streams the style fields of a component, without its content or its end tag.
fn style_fields(
    format: &Format,
    interactions: &Interactivity,
    writer: &mut dyn NbtWrite,
    depth: usize,
) -> Result<()> {
    if let Some(color) = &format.color {
        // Color's display uses the vanilla named/hex codec spelling.
        string("color", &color.to_string(), writer)?;
    }
    if let Some(font) = &format.font {
        string("font", font, writer)?;
    }
    for (name, value) in [
        ("bold", format.bold),
        ("italic", format.italic),
        ("underlined", format.underlined),
        ("strikethrough", format.strikethrough),
        ("obfuscated", format.obfuscated),
    ] {
        if let Some(value) = value {
            boolean(name, value, writer)?;
        }
    }
    if let Some(color) = format.shadow_color {
        integer("shadow_color", color, writer)?;
    }
    if let Some(insertion) = &interactions.insertion {
        string("insertion", insertion, writer)?;
    }
    if let Some(hover) = &interactions.hover {
        compound_field("hover_event", writer, depth)?;
        hover_event(hover, writer, depth + 1)?;
        end(writer)?;
    }
    if let Some(click) = &interactions.click {
        compound_field("click_event", writer, depth)?;
        click_event(click, writer, depth + 1)?;
        end(writer)?;
    }
    Ok(())
}
#[expect(
    clippy::too_many_lines,
    reason = "keeping the exhaustive content-to-wire mapping together makes it auditable"
)]
fn content(value: &Content, writer: &mut dyn NbtWrite, depth: usize) -> Result<()> {
    match value {
        Content::Text { text } => string("text", text, writer),
        Content::Keybind { keybind } => string("keybind", keybind, writer),
        Content::Translate(msg) => {
            string("translate", &msg.key, writer)?;
            if let Some(fallback) = &msg.fallback {
                string("fallback", fallback, writer)?;
            }
            if let Some(args) = &msg.args {
                list("with", args, writer, depth)?;
            }
            Ok(())
        }
        Content::Custom(value) => {
            compound_field("custom", writer, depth)?;
            custom(value, writer, depth + 1)?;
            end(writer)
        }
        Content::Object(Object::Atlas {
            atlas,
            sprite,
            fallback,
        }) => {
            if atlas != "minecraft:blocks" {
                string("atlas", atlas, writer)?;
            }
            string("sprite", sprite, writer)?;
            if let Some(fallback) = fallback {
                nested("fallback", fallback, writer, depth)?;
            }
            Ok(())
        }
        Content::Object(Object::Player {
            player,
            hat,
            fallback,
        }) => {
            compound_field("player", writer, depth)?;
            if let Some(id) = player.id {
                field(11, "id", writer)?;
                writer.write_all(&4_i32.to_be_bytes())?;
                for value in id {
                    writer.write_all(&value.to_be_bytes())?;
                }
            }
            for (name, value) in [
                ("name", &player.name),
                ("texture", &player.texture),
                ("cape", &player.cape),
                ("elytra", &player.elytra),
            ] {
                if let Some(value) = value {
                    string(name, value, writer)?;
                }
            }
            if let Some(model) = player.model {
                string(
                    "model",
                    match model {
                        PlayerModel::Slim => "slim",
                        PlayerModel::Wide => "wide",
                    },
                    writer,
                )?;
            }
            if !player.properties.is_empty() {
                if depth + 3 >= writer.container_limit() {
                    return Err(invalid("text nesting exceeds limit"));
                }
                field(9, "properties", writer)?;
                writer.write_all(&[10])?;
                writer.write_all(
                    &i32::try_from(player.properties.len())
                        .map_err(|_| invalid("too many player properties"))?
                        .to_be_bytes(),
                )?;
                for property in &player.properties {
                    string("name", &property.name, writer)?;
                    string("value", &property.value, writer)?;
                    if let Some(signature) = &property.signature {
                        string("signature", signature, writer)?;
                    }
                    end(writer)?;
                }
            }
            end(writer)?;
            if !hat {
                boolean("hat", false, writer)?;
            }
            if let Some(fallback) = fallback {
                nested("fallback", fallback, writer, depth)?;
            }
            Ok(())
        }
        Content::Resolvable(Resolvable::Scoreboard {
            selector,
            objective,
        }) => {
            compound_field("score", writer, depth)?;
            string("name", selector, writer)?;
            string("objective", objective, writer)?;
            end(writer)
        }
        Content::Resolvable(Resolvable::Entity {
            selector,
            separator,
        }) => {
            string("selector", selector, writer)?;
            if let Some(separator) = separator {
                nested("separator", separator, writer, depth)?;
            }
            Ok(())
        }
        Content::Resolvable(Resolvable::NBT {
            path,
            interpret,
            plain,
            separator,
            source,
        }) => {
            string("nbt", path, writer)?;
            if *interpret {
                boolean("interpret", true, writer)?;
            }
            if *plain {
                boolean("plain", true, writer)?;
            }
            if let Some(separator) = separator {
                nested("separator", separator, writer, depth)?;
            }
            let (name, value) = match source {
                NbtSource::Entity(value) => ("entity", value),
                NbtSource::Block(value) => ("block", value),
                NbtSource::Storage(value) => ("storage", value),
            };
            string(name, value, writer)
        }
    }
}
fn hover_event(value: &HoverEvent, writer: &mut dyn NbtWrite, depth: usize) -> Result<()> {
    match value {
        HoverEvent::ShowText { value } => {
            string("action", "show_text", writer)?;
            nested("value", value, writer, depth)
        }
        HoverEvent::ShowItem {
            id,
            count,
            components,
        } => {
            string("action", "show_item", writer)?;
            string("id", id, writer)?;
            if *count != 1 {
                integer("count", *count, writer)?;
            }
            if let Some(value) = components
                && !value.is_empty_compound()
            {
                nbt("components", value.as_nbt(), writer, depth)?;
            }
            Ok(())
        }
        HoverEvent::ShowEntity { name, id, uuid } => {
            string("action", "show_entity", writer)?;
            string("id", id, writer)?;
            field(11, "uuid", writer)?;
            writer.write_all(&4_i32.to_be_bytes())?;
            writer.write_all(uuid.as_bytes())?;
            if let Some(name) = name {
                nested("name", name, writer, depth)?;
            }
            Ok(())
        }
    }
}
fn click_event(value: &ClickEvent, writer: &mut dyn NbtWrite, depth: usize) -> Result<()> {
    match value {
        ClickEvent::OpenUrl { url } => {
            string("action", "open_url", writer)?;
            string("url", url, writer)
        }
        ClickEvent::RunCommand { command } => {
            string("action", "run_command", writer)?;
            string("command", command, writer)
        }
        ClickEvent::SuggestCommand { command } => {
            string("action", "suggest_command", writer)?;
            string("command", command, writer)
        }
        ClickEvent::ChangePage { page } => {
            string("action", "change_page", writer)?;
            integer("page", *page, writer)
        }
        ClickEvent::CopyToClipboard { value } => {
            string("action", "copy_to_clipboard", writer)?;
            string("value", value, writer)
        }
        ClickEvent::ShowDialog { dialog } => {
            string("action", "show_dialog", writer)?;
            match dialog {
                Dialog::Reference(value) => string("dialog", value, writer),
                Dialog::Inline(value) => nbt("dialog", value.as_nbt(), writer, depth),
            }
        }
        ClickEvent::Custom(value) => {
            string("action", "custom", writer)?;
            custom(value, writer, depth)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use text_components::{content::ObjectPlayer, format::Color, translation::TranslatedMessage};
    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "one fixture matrix keeps all text codec variants on the same parity path"
    )]
    fn borrowed_text_matches_pinned_codec_variants_and_mixed_lists() {
        let mut styled = TextComponent::plain("\0é😀");
        styled.format.bold = Some(false);
        styled.format.italic = Some(true);
        styled.format.underlined = Some(true);
        styled.format.strikethrough = Some(false);
        styled.format.obfuscated = Some(true);
        styled.format.shadow_color = Some(12345);
        styled.format.color = Some(Color::Rgb(1, 2, 255));
        styled.format.font = Some("minecraft:default".into());
        let mut player = ObjectPlayer::property("texture", Some("signature"));
        player.id = Some([1, 2, 3, 4]);
        player.name = Some("player".into());
        player.texture = Some("skin".into());
        player.cape = Some("cape".into());
        player.elytra = Some("elytra".into());
        player.model = Some(PlayerModel::Slim);
        let payload = NbtTag::IntArray(vec![1, 2, 3]);
        let custom = CustomData {
            id: "foton:test".into(),
            payload: Payload::Nbt(payload.clone().into()),
        };
        let contents = vec![
            Content::Text {
                text: "plain".into(),
            },
            Content::Keybind {
                keybind: "key.jump".into(),
            },
            Content::Translate(TranslatedMessage {
                key: "key".into(),
                fallback: Some("fallback".into()),
                args: Some(vec![TextComponent::plain("plain"), styled.clone()].into_boxed_slice()),
            }),
            Content::Object(Object::Atlas {
                atlas: "foton:atlas".into(),
                sprite: "foton:sprite".into(),
                fallback: Some(Box::new(styled.clone())),
            }),
            Content::Object(Object::Player {
                player,
                hat: false,
                fallback: Some(Box::new(styled.clone())),
            }),
            Content::Resolvable(Resolvable::Scoreboard {
                selector: "@s".into(),
                objective: "score".into(),
            }),
            Content::Resolvable(Resolvable::Entity {
                selector: "@a".into(),
                separator: Some(Box::new(styled.clone())),
            }),
            Content::Resolvable(Resolvable::NBT {
                path: "path".into(),
                interpret: true,
                plain: true,
                separator: Some(Box::new(styled.clone())),
                source: NbtSource::Storage("foton:store".into()),
            }),
            Content::Custom(custom.clone()),
        ];
        let clicks = vec![
            ClickEvent::OpenUrl {
                url: "https://example.org".into(),
            },
            ClickEvent::RunCommand {
                command: "/test".into(),
            },
            ClickEvent::SuggestCommand {
                command: "/test".into(),
            },
            ClickEvent::ChangePage { page: 2 },
            ClickEvent::CopyToClipboard {
                value: "copy".into(),
            },
            ClickEvent::ShowDialog {
                dialog: Dialog::Reference("foton:dialog".into()),
            },
            ClickEvent::Custom(custom),
        ];
        let mut fixtures: Vec<TextComponent> = contents.into_iter().map(Into::into).collect();
        for click in clicks {
            let mut value = styled.clone();
            value.interactions.click = Some(click);
            fixtures.push(value);
        }
        for hover in [
            HoverEvent::ShowText {
                value: Box::new(styled.clone()),
            },
            HoverEvent::ShowItem {
                id: "minecraft:stone".into(),
                count: 2,
                components: None,
            },
            HoverEvent::ShowEntity {
                name: Some(Box::new(styled.clone())),
                id: "minecraft:pig".into(),
                uuid: uuid::Uuid::from_u128(12345),
            },
        ] {
            let mut value = styled.clone();
            value.interactions.hover = Some(hover);
            value.interactions.insertion = Some("insert".into());
            fixtures.push(value);
        }
        fixtures.push(styled.clone());
        let mut mixed = styled.clone();
        mixed.children = vec![TextComponent::plain("child"), styled];
        fixtures.push(mixed);
        let mut homogeneous = TextComponent::plain("root");
        homogeneous.children = vec![TextComponent::plain("a"), TextComponent::plain("b")];
        fixtures.push(homogeneous);
        for value in fixtures {
            let mut expected = Vec::new();
            value.to_codec_nbt().write(&mut expected);
            let mut actual = Vec::new();
            write_bounded(&value, expected.len(), &mut actual).expect("fits exact wire budget");
            assert_eq!(actual, expected);
            assert!(write_bounded(&value, expected.len() - 1, &mut sink()).is_err());
        }
    }
}

#[cfg(test)]
mod brewing_rereview_depth_tests {
    use super::*;
    use crate::serial::nbt_encode;
    use simdnbt::borrow::read_tag;
    use std::{io::Cursor, thread::Builder};

    #[test]
    fn brewing_rereview_text_container_depth_matches_persistent_reader() {
        Builder::new()
            .stack_size(32 << 20)
            .spawn(|| {
                for hover in [false, true] {
                    for count in [253, 254, 255, 256] {
                        let mut text = TextComponent::plain("leaf");
                        for _ in 0..count {
                            let mut parent = TextComponent::plain("");
                            if hover {
                                parent.interactions.hover = Some(HoverEvent::ShowText {
                                    value: Box::new(text),
                                });
                            } else {
                                parent.children.push(text);
                            }
                            text = parent;
                        }
                        let mut bytes = Vec::new();
                        text.to_codec_nbt().write(&mut bytes);
                        let accepted = read_tag(&mut Cursor::new(bytes.as_slice())).is_ok();
                        let actual =
                            nbt_encode::write_bounded(&text, 256 * 1024, &mut sink()).is_ok();
                        assert_eq!(actual, accepted, "hover={hover}, nesting={count}");
                    }
                }
            })
            .expect("worker")
            .join()
            .expect("text depth");
    }
}

#[cfg(test)]
mod brewing_union_custom_depth {
    use super::*;
    use crate::serial::nbt_encode;
    use simdnbt::{
        borrow::read_tag as read_borrowed,
        owned::{NbtCompound, read_tag as read_owned},
    };
    use std::{io::Cursor, thread::Builder};

    #[test]
    fn brewing_union_custom_wrapper_depth_matches_both_readers() {
        Builder::new().stack_size(32 << 20).spawn(|| {
            let mut mismatches = Vec::new();
            for count in [508, 509, 510, 511, 512] {
                let mut payload = NbtTag::Int(1);
                for _ in 0..count {
                    let mut parent = NbtCompound::new();
                    parent.insert("child", payload);
                    payload = NbtTag::Compound(parent);
                }
                let text: TextComponent = Content::Custom(CustomData {
                    id: "test:custom".into(), payload: Payload::Nbt(payload.into()),
                }).into();
                let mut bytes = Vec::new();
                text.to_codec_nbt().write(&mut bytes);
                let persistent = read_borrowed(&mut Cursor::new(bytes.as_slice())).is_ok();
                let network = read_owned(&mut Cursor::new(bytes.as_slice())).is_ok();
                let actual_persistent = nbt_encode::write_bounded(&text, 65536, &mut sink()).is_ok();
                let actual_network = write_bounded(&text, 65536, &mut sink()).is_ok();
                eprintln!("custom payload compounds={count}, persistent={persistent}/{actual_persistent}, network={network}/{actual_network}");
                if (persistent, network) != (actual_persistent, actual_network) { mismatches.push(count); }
            }
            assert!(mismatches.is_empty(), "custom wrapper depth mismatch: {mismatches:?}");
        }).expect("worker").join().expect("depth oracle");
    }
}
