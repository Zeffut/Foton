//! Adapted from `TextComponents` `src/parse/nbt.rs`, revision 04d60d0d9c65ab1cb05154105e184d5904e8990b.
//! Upstream: <https://github.com/Steel-Foundation/TextComponents> (see `Cargo.lock`).
//! Licensed under GPL-3.0; the original license is retained in LICENSE.upstream.
//! Foton changes borrow typed text-list descendants once and reject oversized
//! shadow-color/property lists before conversion; content precedence and validators stay pinned.
use crate::nbt::nbt_list_len;
use std::{borrow::Cow, convert::Infallible};

use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use text_components::custom::{CustomData, Payload};
use text_components::{
    EncodedNbt, TextComponent,
    content::{
        Content, NbtSource, Object, ObjectPlayer, PlayerModel, PlayerProperties, Resolvable,
    },
    format::{Color, Format},
    interactivity::{ClickEvent, Dialog, HoverEvent, Interactivity},
    translation::TranslatedMessage,
};
use uuid::Uuid;

use helpers::{
    as_bool, as_i32, as_string, invalid, is_allowed_url, is_identifier, optional_bool,
    optional_identifier, optional_string, parse_color, parse_shadow_color, parse_uuid,
    required_chat_string, required_compound, required_i32, required_identifier, required_string,
    validate_player_name,
};
use text_components::parse::nbt::ComponentDecodeError;
mod helpers;

trait DecodeCompound: Sized {
    fn try_from_compound(compound: &NbtCompound) -> Result<Self, ComponentDecodeError>;
}

pub(super) fn component(tag: &NbtTag) -> Result<TextComponent, ComponentDecodeError> {
    match tag {
        NbtTag::String(value) => Ok(TextComponent::plain(value.to_string())),
        NbtTag::List(list) => component_from_list(list),
        NbtTag::Compound(compound) => component_from_compound(compound),
        _ => Err(ComponentDecodeError::ExpectedComponent),
    }
}

fn components(list: &NbtList) -> Result<Vec<TextComponent>, ComponentDecodeError> {
    match list {
        NbtList::Empty => Ok(Vec::new()),
        NbtList::String(values) => Ok(values
            .iter()
            .map(|value| TextComponent::plain(value.to_string()))
            .collect()),
        NbtList::Compound(values) => values.iter().map(component_from_compound).collect(),
        NbtList::List(values) => values.iter().map(component_from_list).collect(),
        _ if nbt_list_len(list) == 0 => Ok(Vec::new()),
        _ => Err(ComponentDecodeError::ExpectedComponent),
    }
}

fn component_from_list(list: &NbtList) -> Result<TextComponent, ComponentDecodeError> {
    let mut values = components(list)?.into_iter();
    let Some(mut component) = values.next() else {
        return Err(ComponentDecodeError::EmptyComponentList);
    };
    component.children.extend(values);
    Ok(component)
}

fn component_from_compound(compound: &NbtCompound) -> Result<TextComponent, ComponentDecodeError> {
    #[cfg(any(test, feature = "codec-test-support"))]
    super::decode_work::visit();
    if compound.len() == 1
        && let Some(value) = compound.get("")
    {
        return component(value);
    }

    let content = Content::try_from_compound(compound)?;
    let children = match compound.get("extra") {
        None => Vec::new(),
        Some(NbtTag::List(list)) => {
            let values = components(list)?;
            if values.is_empty() {
                return Err(ComponentDecodeError::EmptyComponentList);
            }
            values
        }
        Some(_) => return Err(invalid("extra", "a non-empty component list")),
    };

    Ok(TextComponent {
        content,
        children,
        format: Format::try_from_compound(compound)?,
        interactions: Interactivity::try_from_compound(compound)?,
    })
}

impl DecodeCompound for Content {
    fn try_from_compound(compound: &NbtCompound) -> Result<Self, ComponentDecodeError> {
        if let Some(content_type) = compound.get("type") {
            let content_type = as_string(content_type)
                .ok_or_else(|| invalid("type", "a component type string"))?;
            return match content_type.as_str() {
                "text" => parse_text(compound),
                "translatable" => parse_translatable(compound),
                "keybind" => parse_keybind(compound),
                "score" => parse_score(compound),
                "selector" => parse_selector(compound, &mut false),
                "nbt" => parse_nbt(compound, false),
                "object" => parse_object(compound),
                _ => Err(ComponentDecodeError::UnknownContentType(content_type)),
            };
        }

        if let Ok(content) = parse_text(compound) {
            return Ok(content);
        }
        if let Ok(content) = parse_translatable(compound) {
            return Ok(content);
        }
        if let Ok(content) = parse_keybind(compound) {
            return Ok(content);
        }
        if let Ok(content) = parse_score(compound) {
            return Ok(content);
        }
        // A successful selector ends fallback selection. Only a failed separator
        // can reach the NBT alternative, where the pinned parser ignores that error.
        // Remember it so recursive conversion uses its prepaid allowance just once.
        let mut invalid_separator = false;
        if let Ok(content) = parse_selector(compound, &mut invalid_separator) {
            return Ok(content);
        }
        if let Ok(content) = parse_nbt(compound, invalid_separator) {
            return Ok(content);
        }
        if let Ok(content) = parse_object(compound) {
            return Ok(content);
        }
        if let Ok(content) = parse_custom_content(compound) {
            return Ok(content);
        }

        Err(ComponentDecodeError::NoMatchingContent)
    }
}

fn parse_text(compound: &NbtCompound) -> Result<Content, ComponentDecodeError> {
    Ok(Content::Text {
        text: required_string(compound, "text")?.into(),
    })
}

fn parse_translatable(compound: &NbtCompound) -> Result<Content, ComponentDecodeError> {
    let key = required_string(compound, "translate")?;
    let fallback = compound.get("fallback").and_then(as_string).map(Cow::Owned);
    let args = match compound.get("with") {
        None => None,
        Some(NbtTag::List(list)) => Some(components(list)?.into_boxed_slice()),
        Some(_) => return Err(invalid("with", "a component list")),
    };
    Ok(Content::Translate(TranslatedMessage {
        key: key.into(),
        fallback,
        args,
    }))
}

fn parse_keybind(compound: &NbtCompound) -> Result<Content, ComponentDecodeError> {
    Ok(Content::Keybind {
        keybind: required_string(compound, "keybind")?.into(),
    })
}

fn parse_score(compound: &NbtCompound) -> Result<Content, ComponentDecodeError> {
    let score = required_compound(compound, "score")?;
    Ok(Content::Resolvable(Resolvable::Scoreboard {
        selector: required_string(score, "name")?.into(),
        objective: required_string(score, "objective")?.into(),
    }))
}

fn parse_selector(
    compound: &NbtCompound,
    invalid_separator: &mut bool,
) -> Result<Content, ComponentDecodeError> {
    let selector = required_string(compound, "selector")?;
    let separator = match compound.get("separator") {
        Some(tag) => Some(Box::new(component(tag).inspect_err(|_| {
            *invalid_separator = true;
        })?)),
        None => None,
    };
    Ok(Content::Resolvable(Resolvable::Entity {
        selector: selector.into(),
        separator,
    }))
}

fn parse_nbt(
    compound: &NbtCompound,
    invalid_separator: bool,
) -> Result<Content, ComponentDecodeError> {
    let path = required_string(compound, "nbt")?;
    let interpret = compound.get("interpret").and_then(as_bool).unwrap_or(false);
    let plain = compound.get("plain").and_then(as_bool).unwrap_or(false);
    if interpret && plain {
        return Err(ComponentDecodeError::ConflictingNbtFlags);
    }
    let separator = compound
        .get("separator")
        .filter(|_| !invalid_separator)
        .and_then(|tag| component(tag).ok())
        .map(Box::new);
    Ok(Content::Resolvable(Resolvable::NBT {
        path: path.into(),
        interpret,
        plain,
        separator,
        source: parse_nbt_source(compound)?,
    }))
}

fn parse_nbt_source(compound: &NbtCompound) -> Result<NbtSource, ComponentDecodeError> {
    if let Some(source) = compound.get("source") {
        let source = as_string(source).ok_or_else(|| invalid("source", "a source type string"))?;
        return match source.as_str() {
            "entity" => Ok(NbtSource::Entity(
                required_string(compound, "entity")?.into(),
            )),
            "block" => Ok(NbtSource::Block(required_string(compound, "block")?.into())),
            "storage" => Ok(NbtSource::Storage(
                required_identifier(compound, "storage")?.into(),
            )),
            _ => Err(ComponentDecodeError::UnknownDataSource(source)),
        };
    }

    if let Ok(entity) = required_string(compound, "entity") {
        return Ok(NbtSource::Entity(entity.into()));
    }
    if let Ok(block) = required_string(compound, "block") {
        return Ok(NbtSource::Block(block.into()));
    }
    if let Ok(storage) = required_identifier(compound, "storage") {
        return Ok(NbtSource::Storage(storage.into()));
    }
    Err(ComponentDecodeError::MissingField(
        "entity, block, or storage",
    ))
}

fn parse_object(compound: &NbtCompound) -> Result<Content, ComponentDecodeError> {
    let fallback = match compound.get("fallback") {
        Some(value) => Some(Box::new(component(value)?)),
        None => None,
    };
    let object_type = optional_string(compound, "object")?;
    match object_type.as_deref() {
        Some("atlas") => parse_atlas(compound, fallback),
        Some("player") => parse_player(compound, fallback),
        Some(value) => Err(ComponentDecodeError::UnknownObjectType(value.to_owned())),
        None if compound.contains("sprite") => parse_atlas(compound, fallback),
        None if compound.contains("player") => parse_player(compound, fallback),
        None => Err(ComponentDecodeError::NoMatchingContent),
    }
}

fn parse_atlas(
    compound: &NbtCompound,
    fallback: Option<Box<TextComponent>>,
) -> Result<Content, ComponentDecodeError> {
    let atlas = optional_identifier(compound, "atlas")?
        .map_or(Cow::Borrowed("minecraft:blocks"), Cow::Owned);
    let sprite = required_identifier(compound, "sprite")?;
    Ok(Content::Object(Object::Atlas {
        atlas,
        sprite: sprite.into(),
        fallback,
    }))
}

fn parse_player(
    compound: &NbtCompound,
    fallback: Option<Box<TextComponent>>,
) -> Result<Content, ComponentDecodeError> {
    let player = compound
        .get("player")
        .ok_or(ComponentDecodeError::MissingField("player"))?;
    let player = match player {
        NbtTag::String(name) => {
            let name = name.to_string();
            validate_player_name(&name)?;
            ObjectPlayer::name(name)
        }
        NbtTag::Compound(profile) => parse_player_profile(profile)?,
        _ => return Err(invalid("player", "a player name or profile")),
    };
    let hat = compound.get("hat").and_then(as_bool).unwrap_or(true);
    Ok(Content::Object(Object::Player {
        player,
        hat,
        fallback,
    }))
}

fn parse_player_profile(profile: &NbtCompound) -> Result<ObjectPlayer, ComponentDecodeError> {
    let name = optional_string(profile, "name")?;
    if let Some(name) = &name {
        validate_player_name(name)?;
    }
    let id = match profile.get("id") {
        None => None,
        Some(NbtTag::IntArray(values) | NbtTag::List(NbtList::Int(values)))
            if values.len() == 4 =>
        {
            Some([values[0], values[1], values[2], values[3]])
        }
        Some(_) => return Err(invalid("id", "a four-integer UUID")),
    };
    let texture = optional_identifier(profile, "texture")?.map(Cow::Owned);
    let cape = optional_identifier(profile, "cape")?.map(Cow::Owned);
    let elytra = optional_identifier(profile, "elytra")?.map(Cow::Owned);
    let model = match optional_string(profile, "model")?.as_deref() {
        None => None,
        Some("slim") => Some(PlayerModel::Slim),
        Some("wide") => Some(PlayerModel::Wide),
        Some(_) => return Err(invalid("model", "`slim` or `wide`")),
    };

    Ok(ObjectPlayer {
        name: name.map(Cow::Owned),
        id,
        texture,
        cape,
        elytra,
        model,
        properties: parse_properties(profile.get("properties"))?,
    })
}

fn parse_properties(tag: Option<&NbtTag>) -> Result<Vec<PlayerProperties>, ComponentDecodeError> {
    let Some(tag) = tag else {
        return Ok(Vec::new());
    };
    let count = match tag {
        NbtTag::List(NbtList::Compound(properties)) => properties.len(),
        NbtTag::Compound(properties) => {
            let mut count = 0_usize;
            for (_, values) in properties.iter() {
                let NbtTag::List(NbtList::String(values)) = values else {
                    return Err(invalid("properties", "a property map or list"));
                };
                count = count.saturating_add(values.len());
                if count > 16 {
                    return Err(invalid("properties", "at most 16 properties"));
                }
            }
            count
        }
        _ => return Err(invalid("properties", "a property map or list")),
    };
    if count > 16 {
        return Err(invalid("properties", "at most 16 properties"));
    }
    let properties = match tag {
        NbtTag::List(NbtList::Compound(properties)) => properties
            .iter()
            .map(|property| {
                Ok(PlayerProperties {
                    name: required_string(property, "name")?.into(),
                    value: required_string(property, "value")?.into(),
                    signature: optional_string(property, "signature")?.map(Cow::Owned),
                })
            })
            .collect::<Result<Vec<_>, ComponentDecodeError>>()?,
        NbtTag::Compound(properties) => {
            let mut result = Vec::with_capacity(count);
            for (name, values) in properties.iter() {
                let NbtTag::List(NbtList::String(values)) = values else {
                    return Err(invalid("properties", "a property map or list"));
                };
                for value in values {
                    result.push(PlayerProperties {
                        name: name.to_string().into(),
                        value: value.to_string().into(),
                        signature: None,
                    });
                }
            }
            result
        }
        _ => return Err(invalid("properties", "a property map or list")),
    };
    Ok(properties)
}

fn parse_custom_content(compound: &NbtCompound) -> Result<Content, ComponentDecodeError> {
    let custom = required_compound(compound, "custom")?;
    Ok(Content::Custom(parse_custom_data(custom)?))
}

impl DecodeCompound for Format {
    fn try_from_compound(compound: &NbtCompound) -> Result<Self, ComponentDecodeError> {
        let color = match optional_string(compound, "color")? {
            Some(value) => Some(parse_color(&value)?),
            None => None,
        };
        let font = optional_identifier(compound, "font")?.map(Cow::Owned);
        Ok(Self {
            color,
            font,
            bold: optional_bool(compound, "bold")?,
            italic: optional_bool(compound, "italic")?,
            underlined: optional_bool(compound, "underlined")?,
            strikethrough: optional_bool(compound, "strikethrough")?,
            obfuscated: optional_bool(compound, "obfuscated")?,
            shadow_color: match compound.get("shadow_color") {
                Some(tag) => Some(parse_shadow_color(tag)?),
                None => None,
            },
        })
    }
}

impl DecodeCompound for Interactivity {
    fn try_from_compound(compound: &NbtCompound) -> Result<Self, ComponentDecodeError> {
        Ok(Self {
            insertion: optional_string(compound, "insertion")?.map(Cow::Owned),
            click: match compound.get("click_event") {
                Some(NbtTag::Compound(event)) => Some(ClickEvent::try_from_compound(event)?),
                Some(_) => return Err(invalid("click_event", "a click event")),
                None => None,
            },
            hover: match compound.get("hover_event") {
                Some(NbtTag::Compound(event)) => Some(HoverEvent::try_from_compound(event)?),
                Some(_) => return Err(invalid("hover_event", "a hover event")),
                None => None,
            },
        })
    }
}

impl DecodeCompound for ClickEvent {
    fn try_from_compound(compound: &NbtCompound) -> Result<Self, ComponentDecodeError> {
        let action = required_string(compound, "action")?;
        match action.as_str() {
            "open_url" => {
                let url = required_string(compound, "url")?;
                if !is_allowed_url(&url) {
                    return Err(invalid("url", "an HTTP or HTTPS URI"));
                }
                Ok(Self::OpenUrl { url: url.into() })
            }
            "run_command" => Ok(Self::RunCommand {
                command: required_chat_string(compound, "command")?.into(),
            }),
            "suggest_command" => Ok(Self::SuggestCommand {
                command: required_chat_string(compound, "command")?.into(),
            }),
            "change_page" => {
                let page = required_i32(compound, "page")?;
                if page <= 0 {
                    return Err(invalid("page", "a positive integer"));
                }
                Ok(Self::ChangePage { page })
            }
            "copy_to_clipboard" => Ok(Self::CopyToClipboard {
                value: required_string(compound, "value")?.into(),
            }),
            "show_dialog" => {
                let dialog = compound
                    .get("dialog")
                    .ok_or(ComponentDecodeError::MissingField("dialog"))?;
                let dialog = match dialog {
                    NbtTag::String(value) => {
                        let value = value.to_string();
                        if !is_identifier(&value) {
                            return Err(invalid("dialog", "a dialog identifier or definition"));
                        }
                        Dialog::Reference(value.into())
                    }
                    NbtTag::Compound(_) => Dialog::Inline(encoded_payload(dialog)),
                    _ => return Err(invalid("dialog", "a dialog identifier or definition")),
                };
                Ok(Self::ShowDialog { dialog })
            }
            "custom" => Ok(Self::Custom(parse_custom_data(compound)?)),
            _ => Err(ComponentDecodeError::UnknownClickAction(action)),
        }
    }
}

impl DecodeCompound for HoverEvent {
    fn try_from_compound(compound: &NbtCompound) -> Result<Self, ComponentDecodeError> {
        let action = required_string(compound, "action")?;
        match action.as_str() {
            "show_text" => Ok(Self::ShowText {
                value: Box::new(component(
                    compound
                        .get("value")
                        .ok_or(ComponentDecodeError::MissingField("value"))?,
                )?),
            }),
            "show_item" => {
                let count = match compound.get("count") {
                    None => 1,
                    Some(tag) => {
                        let count = as_i32(tag).ok_or_else(|| invalid("count", "an integer"))?;
                        if !(1..=99).contains(&count) {
                            return Err(invalid("count", "an integer from 1 through 99"));
                        }
                        count
                    }
                };
                let components = match compound.get("components") {
                    None => None,
                    Some(NbtTag::Compound(components)) if components.is_empty() => None,
                    Some(tag @ NbtTag::Compound(_)) => Some(encoded_payload(tag)),
                    Some(_) => return Err(invalid("components", "a data component patch")),
                };
                Ok(Self::ShowItem {
                    id: required_identifier(compound, "id")?.into(),
                    count,
                    components,
                })
            }
            "show_entity" => {
                let uuid = parse_uuid(
                    compound
                        .get("uuid")
                        .ok_or(ComponentDecodeError::MissingField("uuid"))?,
                )?;
                let name = match compound.get("name") {
                    Some(name) => Some(Box::new(component(name)?)),
                    None => None,
                };
                Ok(Self::ShowEntity {
                    name,
                    id: required_identifier(compound, "id")?.into(),
                    uuid,
                })
            }
            _ => Err(ComponentDecodeError::UnknownHoverAction(action)),
        }
    }
}

fn parse_custom_data(compound: &NbtCompound) -> Result<CustomData, ComponentDecodeError> {
    let payload = compound
        .get("payload")
        .map_or(Payload::Empty, |value| Payload::Nbt(value.clone().into()));
    Ok(CustomData {
        id: required_identifier(compound, "id")?.into(),
        payload,
    })
}

/// This decoder owns the raw embedded-payload schema boundary. The payload is
/// copied once into the resulting text graph, never once per text ancestor.
fn encoded_payload(tag: &NbtTag) -> EncodedNbt {
    struct ParsedPayload<'a>(&'a NbtTag);
    impl text_components::EmbeddedNbtCodec for ParsedPayload<'_> {
        type Error = Infallible;
        fn encode_embedded_nbt(self) -> Result<NbtTag, Self::Error> {
            Ok(self.0.clone())
        }
    }
    match EncodedNbt::encode(ParsedPayload(tag)) {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

#[cfg(test)]
mod tests;
