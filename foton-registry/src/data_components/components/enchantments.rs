use crate::data_components::registry::ValidatePersistentComponent;
use foton_utils::Identifier;
use foton_utils::codec::VarInt;
use foton_utils::hash::{ComponentHasher, HashComponent, HashEntry, sort_map_entries};
use foton_utils::nbt::NbtNumeric as _;
use foton_utils::serial::budget;
use foton_utils::serial::nbt_encode;
use foton_utils::serial::nbt_stream::NbtWrite;
use foton_utils::serial::nbt_stream::{LimitedWriter, ScratchWriter};
use foton_utils::serial::{ReadFrom, WriteTo};
use rustc_hash::FxHashMap;
use simdnbt::owned::{NbtCompound, NbtTag};
use simdnbt::{FromNbtTag, ToNbtTag};
use std::io;

use crate::{REGISTRY, RegistryExt};

#[cfg(test)]
mod lookup_work {
    use std::cell::Cell;

    thread_local! { static LOOKUPS: Cell<usize> = const { Cell::new(0) }; }

    pub(super) fn visit() {
        LOOKUPS.set(LOOKUPS.get() + 1);
    }

    pub(super) fn measure(operation: impl FnOnce()) -> usize {
        LOOKUPS.set(0);
        operation();
        LOOKUPS.get()
    }
}

/// Enchantments stored on an item. Maps enchantment key to level.
///
/// Used by both the `minecraft:enchantments` component (on enchanted items)
/// and the `minecraft:stored_enchantments` component (on enchanted books).
///
/// Vanilla moved tooltip visibility to the separate `TOOLTIP_DISPLAY` component.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemEnchantments {
    pub levels: FxHashMap<Identifier, u32>,
}

impl ItemEnchantments {
    pub(crate) fn write_bounded(
        &self,
        budget: usize,
        writer: &mut dyn io::Write,
    ) -> io::Result<()> {
        let mut writer = LimitedWriter::new(writer, budget);
        writer.ensure_remaining(self.levels.len().saturating_mul(2).saturating_add(1))?;
        // Known keys cap the scratch vector at the native registry's size.
        if self.levels.len() > REGISTRY.enchantments.len() {
            return Err(io::Error::other("Too many enchantments"));
        }
        let required = self.levels.len().saturating_mul(size_of::<(usize, u32)>());
        let mut writer = ScratchWriter::new(&mut writer, required)?;
        // IDs carry no identifier bytes. Bound lookup work by registered key
        // lengths without hashing an arbitrarily long unknown identifier.
        let maximum_key_bytes = REGISTRY
            .enchantments
            .iter()
            .map(|(_, value)| {
                value
                    .key
                    .namespace
                    .len()
                    .saturating_add(value.key.path.len())
            })
            .max()
            .unwrap_or(0);
        let mut entries = Vec::with_capacity(self.levels.len());
        for (key, &level) in &self.levels {
            if key.namespace.len().saturating_add(key.path.len()) > maximum_key_bytes {
                return Err(io::Error::other("Unknown enchantment"));
            }
            #[cfg(test)]
            lookup_work::visit();
            let id = REGISTRY
                .enchantments
                .id_from_key(key)
                .ok_or_else(|| io::Error::other("Unknown enchantment"))?;
            if level > 255 {
                return Err(io::Error::other("Invalid enchantment level"));
            }
            entries.push((id, level));
        }
        entries.sort_unstable_by_key(|(id, _)| *id);
        VarInt(i32::try_from(entries.len()).map_err(io::Error::other)?).write(&mut writer)?;
        for (id, level) in entries {
            VarInt(i32::try_from(id).map_err(io::Error::other)?).write(&mut writer)?;
            VarInt(level as i32).write(&mut writer)?;
        }
        Ok(())
    }

    #[must_use]
    pub fn empty() -> Self {
        Self {
            levels: FxHashMap::default(),
        }
    }

    #[must_use]
    pub fn get_level(&self, enchantment: &Identifier) -> u32 {
        self.levels.get(enchantment).copied().unwrap_or(0)
    }

    pub fn set(&mut self, enchantment: Identifier, level: u32) {
        if level == 0 {
            self.levels.remove(&enchantment);
        } else {
            self.levels.insert(enchantment, level.min(255));
        }
    }

    /// Vanilla `Mutable.upgrade`: keeps the higher of existing vs new level.
    pub fn upgrade(&mut self, enchantment: Identifier, level: u32) {
        if level > 0 {
            let existing = self.get_level(&enchantment);
            self.levels
                .insert(enchantment, existing.max(level).min(255));
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.levels.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.levels.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Identifier, &u32)> {
        self.levels.iter()
    }
}

impl IntoIterator for ItemEnchantments {
    type Item = (Identifier, u32);

    type IntoIter = <FxHashMap<Identifier, u32> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.levels.into_iter()
    }
}

impl Default for ItemEnchantments {
    fn default() -> Self {
        Self::empty()
    }
}

/// Network format: `VarInt` count, then (`VarInt` `enchantment_id`, `VarInt` level) pairs.
impl WriteTo for ItemEnchantments {
    fn write(&self, writer: &mut impl io::Write) -> io::Result<()> {
        let count = i32::try_from(self.levels.len()).map_err(|_| {
            io::Error::other(format!(
                "Enchantment map too large: {} entries",
                self.levels.len()
            ))
        })?;
        VarInt(count).write(writer)?;
        for (key, &level) in &self.levels {
            let id = REGISTRY
                .enchantments
                .id_from_key(key)
                .ok_or_else(|| io::Error::other("Unknown enchantment"))?;
            let id = i32::try_from(id).map_err(|_| {
                io::Error::other(format!("Enchantment id out of protocol range: {id}"))
            })?;
            if level > 255 {
                return Err(io::Error::other("Invalid enchantment level"));
            }
            VarInt(id).write(writer)?;
            VarInt(level as i32).write(writer)?;
        }
        Ok(())
    }
}

impl ReadFrom for ItemEnchantments {
    fn read(data: &mut std::io::Cursor<&[u8]>) -> io::Result<Self> {
        let count = VarInt::read(data)?.0;
        let count = usize::try_from(count)
            .map_err(|_| io::Error::other(format!("Negative enchantment count: {count}")))?;
        budget::check_collection_input(data, count, 2)?;
        budget::charge_map::<Identifier, u32>(count)?;
        let mut levels = FxHashMap::default();
        levels.reserve(count.min(65_536));
        for _ in 0..count {
            let id = VarInt::read(data)?.0;
            let id = usize::try_from(id)
                .map_err(|_| io::Error::other(format!("Negative enchantment id: {id}")))?;
            let level = VarInt::read(data)?.0;
            if !(0..=255).contains(&level) {
                return Err(io::Error::other(format!(
                    "Enchantment level out of range: {level}"
                )));
            }
            let enchantment = REGISTRY
                .enchantments
                .by_id(id)
                .ok_or_else(|| io::Error::other(format!("Unknown enchantment id: {id}")))?;
            levels.insert(enchantment.key.clone(), level as u32);
        }
        Ok(Self { levels })
    }
}

/// NBT format: compound with enchantment identifiers as keys and int levels as values.
impl ToNbtTag for ItemEnchantments {
    fn to_nbt_tag(self) -> NbtTag {
        let mut compound = NbtCompound::new();
        for (key, level) in &self.levels {
            compound.insert(key.to_string(), NbtTag::Int(*level as i32));
        }
        NbtTag::Compound(compound)
    }
}

impl FromNbtTag for ItemEnchantments {
    fn from_nbt_tag(tag: simdnbt::borrow::NbtTag) -> Option<Self> {
        let compound = tag.compound()?;
        let mut levels = FxHashMap::default();
        for (key, value) in compound.iter() {
            let ident = key.to_str().parse::<Identifier>().ok()?;
            REGISTRY.enchantments.by_key(&ident)?;
            let level = value.codec_i32()?;
            if !(1..=255).contains(&level) {
                return None;
            }
            if levels.insert(ident, level as u32).is_some() {
                return None;
            }
        }
        Some(Self { levels })
    }
}

impl HashComponent for ItemEnchantments {
    fn hash_component(&self, hasher: &mut ComponentHasher) {
        hasher.start_map();
        let mut entries: Vec<_> = self
            .levels
            .iter()
            .map(|(key, &level)| {
                let mut key_hasher = ComponentHasher::new();
                key_hasher.put_string(&key.to_string());
                let mut value_hasher = ComponentHasher::new();
                value_hasher.put_int(level as i32);
                HashEntry::new(key_hasher, value_hasher)
            })
            .collect();
        sort_map_entries(&mut entries);
        for entry in &entries {
            hasher.put_raw_bytes(&entry.key_bytes);
            hasher.put_raw_bytes(&entry.value_bytes);
        }
        hasher.end_map();
    }
}

impl ValidatePersistentComponent for ItemEnchantments {
    fn validate_persistent(&self) -> io::Result<()> {
        for (key, level) in &self.levels {
            if !(1..=255).contains(level) || REGISTRY.enchantments.by_key(key).is_none() {
                return Err(io::Error::other("Invalid persistent enchantment"));
            }
        }
        Ok(())
    }
}

impl nbt_encode::NbtEncode for ItemEnchantments {
    fn nbt_id(&self) -> u8 {
        10
    }
    fn write_nbt_payload(&self, writer: &mut dyn NbtWrite, depth: usize) -> io::Result<()> {
        use foton_utils::serial::nbt_encode::{check_depth, end};
        check_depth(depth)?;
        let mut minimum = self.levels.len().saturating_mul(8).saturating_add(1);
        writer.ensure_remaining(minimum)?;
        if self.levels.len() > REGISTRY.enchantments.len() {
            return Err(io::Error::other("Too many enchantments"));
        }
        for key in self.levels.keys() {
            minimum = minimum
                .saturating_add(key.namespace.len())
                .saturating_add(key.path.len());
            writer.ensure_remaining(minimum)?;
        }
        let required = self
            .levels
            .len()
            .saturating_mul(size_of::<(&Identifier, &u32)>());
        let mut writer = ScratchWriter::new(writer, required)?;
        self.validate_persistent()?;
        let mut entries: Vec<_> = self.levels.iter().collect();
        entries.sort_unstable_by_key(|(key, _)| (key.namespace.as_ref(), key.path.as_ref()));
        for (key, value) in entries {
            nbt_encode::identifier_field(key, &(*value as i32), &mut *writer, depth)?;
        }
        end(&mut *writer)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use simdnbt::FromNbtTag;
    use simdnbt::borrow::{NbtTag as BorrowedNbtTag, read_tag};
    use simdnbt::owned::{NbtCompound, NbtTag};

    use super::ItemEnchantments;
    use crate::init_vanilla_registry;

    fn with_borrowed_tag<R>(tag: NbtTag, visitor: impl FnOnce(BorrowedNbtTag<'_, '_>) -> R) -> R {
        let mut bytes = Vec::new();
        tag.write(&mut bytes);
        let borrowed =
            read_tag(&mut Cursor::new(bytes.as_slice())).expect("owned test tag should parse");
        visitor(borrowed.as_tag())
    }

    fn parse_enchantments(compound: NbtCompound) -> Option<ItemEnchantments> {
        with_borrowed_tag(NbtTag::Compound(compound), ItemEnchantments::from_nbt_tag)
    }

    #[test]
    fn enchantment_nbt_requires_known_keys_and_vanilla_levels() {
        init_vanilla_registry();
        let mut valid = NbtCompound::new();
        valid.insert("minecraft:efficiency", 5_i8);
        assert!(parse_enchantments(valid).is_some());

        let mut unknown = NbtCompound::new();
        unknown.insert("minecraft:not_an_enchantment", 1);
        assert!(parse_enchantments(unknown).is_none());

        let mut out_of_range = NbtCompound::new();
        out_of_range.insert("minecraft:efficiency", 256);
        assert!(parse_enchantments(out_of_range).is_none());
    }
}

#[cfg(test)]
mod brewing_task12_tests {
    use super::*;
    use foton_utils::serial::nbt_encode::NbtEncode;
    use foton_utils::serial::nbt_stream::ScratchWriter;
    use std::io::{Write, sink};

    #[test]
    fn brewing_task12_enchantment_tiny_caps_precede_resolution_and_sorting() {
        crate::init_vanilla_registry();
        let value = ItemEnchantments {
            levels: REGISTRY
                .enchantments
                .iter()
                .map(|(_, v)| (v.key.clone(), 1))
                .collect(),
        };
        let unknown = ItemEnchantments {
            levels: [(Identifier::new("test", "x".repeat(2 * 1024 * 1024)), 1)]
                .into_iter()
                .collect(),
        };
        for value in [&unknown, &value] {
            let stats = allocation_counter::measure(|| {
                let error = value
                    .write_bounded(2, &mut sink())
                    .expect_err("tiny network cap");
                assert!(
                    error.to_string().contains("budget"),
                    "resolved before rejecting: {error}"
                );
                let error = nbt_encode::write_bounded(value, 2, &mut sink())
                    .expect_err("tiny persistent cap");
                assert!(
                    error.to_string().contains("budget"),
                    "resolved before rejecting: {error}"
                );
            });
            eprintln!("task12 enchantment tiny-cap {stats:?}");
            assert!(
                stats.bytes_max < 256,
                "index allocated before cap check: {stats:?}"
            );
        }
    }
    #[test]
    fn brewing_task12_unknown_network_key_work_is_bounded() {
        crate::init_vanilla_registry();
        let value = ItemEnchantments {
            levels: [(Identifier::new("test", "x".repeat(32 * 1024 * 1024)), 1)]
                .into_iter()
                .collect(),
        };
        let stats = allocation_counter::measure(|| {
            let lookups = super::lookup_work::measure(|| {
                assert!(value.write_bounded(64, &mut sink()).is_err());
            });
            assert_eq!(lookups, 0, "oversized key reached registry hashing");
        });
        eprintln!("task12 unknown network key {stats:?}");
        assert!(stats.bytes_max < 1024, "{stats:?}");
    }
    #[test]
    fn brewing_task12_enchantment_persistent_shares_sorting_scratch() {
        crate::init_vanilla_registry();
        let value = ItemEnchantments {
            levels: REGISTRY
                .enchantments
                .iter()
                .map(|(_, v)| (v.key.clone(), 1))
                .collect(),
        };
        let mut sink = sink();
        let mut writer = LimitedWriter::persistent(&mut sink, 65536);
        let scratch = writer.scratch_remaining();
        let mut writer =
            ScratchWriter::new(&mut writer, scratch).expect("reserve enclosing workspace");
        writer.write_all(&[10]).expect("tag");
        assert!(
            value.write_nbt_payload(&mut *writer, 0).is_err(),
            "map must respect enclosing scratch reservation"
        );
    }
}
