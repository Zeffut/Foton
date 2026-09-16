//! Bridge codecs carry bounds through every component containing item templates.
use foton_utils::serial::nbt_stream::NbtWrite;
use std::io::{Error, Result};

use foton_utils::codec::VarInt;
use foton_utils::serial::WriteTo;
use foton_utils::serial::nbt_encode::{self, NbtEncode, check_depth, end, field};

use crate::ItemStackTemplate;
use crate::data_components::ComponentData;
use crate::data_components::components::{
    BundleContents, ChargedProjectiles, ItemContainerContents, SulfurCubeContent, UseRemainder,
};

/// Owned persistent descendants must not serialize and reparse their suffixes.
/// The enclosing exact/template read owns depth and semantic validation.
pub(super) fn owned_reader(
    key: foton_utils::DowncastTypeKey,
) -> Option<fn(&simdnbt::owned::NbtTag) -> Option<ComponentData>> {
    use crate::item_predicate::{AdventureModePredicate, ItemPredicate, LockCode};
    use foton_utils::DowncastType;
    macro_rules! read {
        ($ty:ty, $tag:ident, $value:expr) => {
            if key == <$ty>::TYPE_KEY {
                return Some(|$tag| {
                    #[cfg(test)]
                    tests::record_visit();
                    $value.map(ComponentData::new)
                });
            }
        };
    }
    read!(
        UseRemainder,
        tag,
        ItemStackTemplate::from_owned_nbt(tag).map(UseRemainder::new)
    );
    read!(
        SulfurCubeContent,
        tag,
        ItemStackTemplate::from_owned_nbt(tag).map(SulfurCubeContent::new)
    );
    read!(
        ChargedProjectiles,
        tag,
        ChargedProjectiles::from_owned_nbt(tag)
    );
    read!(BundleContents, tag, BundleContents::from_owned_nbt(tag));
    read!(
        ItemContainerContents,
        tag,
        ItemContainerContents::from_owned_nbt(tag)
    );
    read!(
        AdventureModePredicate,
        tag,
        AdventureModePredicate::from_owned_nbt(tag)
    );
    read!(
        LockCode,
        tag,
        ItemPredicate::from_owned_nbt(tag).map(LockCode::new)
    );
    None
}

#[cfg(test)]
#[path = "recursive_decode_tests.rs"]
mod tests;

pub(super) fn write(data: &ComponentData, mut writer: &mut dyn NbtWrite) -> Option<Result<()>> {
    if let Some(value) = data.downcast_ref::<UseRemainder>() {
        return Some(value.convert_into().write_bounded(writer));
    }
    if let Some(value) = data.downcast_ref::<SulfurCubeContent>() {
        return Some(value.absorbed_block_item_stack().write_bounded(writer));
    }
    if let Some(value) = data.downcast_ref::<BundleContents>() {
        return Some(write_list(value.items(), writer));
    }
    if let Some(value) = data.downcast_ref::<ChargedProjectiles>() {
        return Some(write_list(value.items(), writer));
    }
    if let Some(value) = data.downcast_ref::<ItemContainerContents>() {
        return Some((|| {
            VarInt(i32::try_from(value.items().len()).map_err(Error::other)?).write(&mut writer)?;
            for item in value.items() {
                item.is_some().write(&mut writer)?;
                if let Some(item) = item {
                    item.write_bounded(writer)?;
                }
            }
            Ok(())
        })());
    }
    None
}

fn write_list(items: &[ItemStackTemplate], mut writer: &mut dyn NbtWrite) -> Result<()> {
    VarInt(i32::try_from(items.len()).map_err(Error::other)?).write(&mut writer)?;
    for item in items {
        item.write_bounded(writer)?;
    }
    Ok(())
}

macro_rules! template_wrapper {
    ($ty:ty, $item:ident) => {
        impl NbtEncode for $ty {
            fn nbt_id(&self) -> u8 {
                10
            }
            fn write_nbt_payload(&self, writer: &mut dyn NbtWrite, depth: usize) -> Result<()> {
                self.$item().write_nbt_payload(writer, depth)
            }
        }
    };
}
template_wrapper!(UseRemainder, convert_into);
template_wrapper!(SulfurCubeContent, absorbed_block_item_stack);

macro_rules! template_list {
    ($ty:ty) => {
        impl NbtEncode for $ty {
            fn nbt_id(&self) -> u8 {
                9
            }
            fn write_nbt_payload(&self, writer: &mut dyn NbtWrite, depth: usize) -> Result<()> {
                if self.items().is_empty() {
                    return writer.write_all(&[0, 0, 0, 0, 0]);
                }
                nbt_encode::list(self.items(), writer, depth)
            }
        }
    };
}
template_list!(BundleContents);
template_list!(ChargedProjectiles);

impl NbtEncode for ItemContainerContents {
    fn nbt_id(&self) -> u8 {
        9
    }
    fn write_nbt_payload(&self, writer: &mut dyn NbtWrite, depth: usize) -> Result<()> {
        let count = self.items().iter().flatten().count();
        if count != 0 {
            check_depth(depth)?;
        }
        writer.write_all(&[if count == 0 { 0 } else { 10 }])?;
        writer.write_all(&i32::try_from(count).map_err(Error::other)?.to_be_bytes())?;
        for (slot, item) in self.items().iter().enumerate() {
            if let Some(item) = item {
                field(
                    "slot",
                    &i32::try_from(slot).map_err(Error::other)?,
                    writer,
                    depth + 1,
                )?;
                field("item", item, writer, depth + 1)?;
                end(writer)?;
            }
        }
        Ok(())
    }
}
