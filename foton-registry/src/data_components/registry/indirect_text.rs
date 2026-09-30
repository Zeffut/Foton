//! Network codecs whose nested text must retain the caller's sink context.
use std::io::{Error, Result};

use foton_utils::codec::VarInt;
use foton_utils::serial::{
    WriteTo,
    nbt_stream::{LimitedWriter, NbtWrite},
    text_stream,
};
use text_components::TextComponent;

use crate::data_components::{
    ComponentData,
    components::{
        InstrumentComponent, ItemAttributeModifierDisplay, ItemAttributeModifiers, JukeboxPlayable,
        PaintingVariantComponent,
    },
};
use crate::trim_pattern::TrimPattern;
use crate::{RegistryEntry, RegistryHolder, RegistryHolderEntry};

fn holder<T: RegistryHolderEntry, W: NbtWrite>(
    value: &RegistryHolder<T>,
    writer: &mut W,
    direct: impl FnOnce(&T::Value, &mut W) -> Result<()>,
) -> Result<()> {
    let RegistryHolder::Direct(value) = value else {
        return value.write(writer);
    };
    VarInt(0).write(writer)?;
    direct(value, writer)
}

pub(super) fn pattern(
    value: &RegistryHolder<TrimPattern>,
    writer: &mut impl NbtWrite,
) -> Result<()> {
    holder(value, writer, |value, writer| {
        value.asset_id().write(writer)?;
        text_stream::write(value.description(), writer)?;
        value.decal().write(writer)
    })
}

fn optional_text(value: Option<&TextComponent>, writer: &mut impl NbtWrite) -> Result<()> {
    value.is_some().write(writer)?;
    if let Some(value) = value {
        text_stream::write(value, writer)?;
    }
    Ok(())
}

fn attributes(value: &ItemAttributeModifiers, writer: &mut impl NbtWrite) -> Result<()> {
    writer.ensure_remaining(value.modifiers.len().saturating_mul(13).saturating_add(1))?;
    VarInt(i32::try_from(value.modifiers.len()).map_err(Error::other)?).write(writer)?;
    for entry in &value.modifiers {
        let id = entry
            .attribute
            .try_id()
            .ok_or_else(|| Error::other("Unknown attribute"))?;
        VarInt(i32::try_from(id).map_err(Error::other)?).write(writer)?;
        entry.id.write(writer)?;
        entry.amount.write(writer)?;
        entry.operation.write(writer)?;
        VarInt(entry.slot.id()).write(writer)?;
        VarInt(entry.display.id()).write(writer)?;
        if let ItemAttributeModifierDisplay::OverrideText(text) = &entry.display {
            text_stream::write(text, writer)?;
        }
    }
    Ok(())
}

pub(super) fn write(data: &ComponentData, writer: &mut LimitedWriter<'_>) -> Option<Result<()>> {
    if let Some(value) = data.downcast_ref::<InstrumentComponent>() {
        return Some(holder(value.instrument(), writer, |value, writer| {
            value.sound_event().write(writer)?;
            value.use_duration().write(writer)?;
            value.range().write(writer)?;
            text_stream::write(value.description(), writer)
        }));
    }
    if let Some(value) = data.downcast_ref::<ItemAttributeModifiers>() {
        return Some(attributes(value, writer));
    }
    if let Some(value) = data.downcast_ref::<JukeboxPlayable>() {
        return Some(holder(value.song(), writer, |value, writer| {
            value.sound_event.write(writer)?;
            text_stream::write(&value.description, writer)?;
            value.length_in_seconds.write(writer)?;
            VarInt(value.comparator_output).write(writer)
        }));
    }
    if let Some(value) = data.downcast_ref::<PaintingVariantComponent>() {
        return Some(holder(value.variant(), writer, |value, writer| {
            VarInt(value.width).write(writer)?;
            VarInt(value.height).write(writer)?;
            value.asset_id.write(writer)?;
            optional_text(value.title.as_ref(), writer)?;
            optional_text(value.author.as_ref(), writer)
        }));
    }
    None
}
