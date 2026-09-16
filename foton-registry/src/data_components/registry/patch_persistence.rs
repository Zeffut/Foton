use super::{
    BorrowedNbtTag, Component, ComponentData, ComponentHasher, ComponentPatchEntry,
    DataComponentPatch, DataComponentType, DowncastType, EmbeddedNbtCodec, FromNbtTag,
    HashComponent, HashEntry, Identifier, NbtCompound, OwnedNbtTag, Result, ToNbtTag,
    sort_map_entries,
};
use foton_utils::serial::budget;
use foton_utils::serial::nbt_encode;
use foton_utils::serial::nbt_stream::NbtWrite;
use std::io;

#[cfg(test)]
mod persistent_filter_work {
    use std::cell::Cell;

    thread_local! { static VISITS: Cell<usize> = const { Cell::new(0) }; }

    pub(super) fn visit() {
        VISITS.set(VISITS.get() + 1);
    }

    pub(super) fn measure(operation: impl FnOnce()) -> usize {
        VISITS.set(0);
        operation();
        VISITS.get()
    }
}

impl DataComponentPatch {
    /// Whether the persistent patch contains any registered set or removal.
    pub(crate) fn has_persistent_entries(&self) -> bool {
        use crate::{REGISTRY, RegistryExt};
        self.entries.keys().any(|key| {
            REGISTRY
                .data_components
                .by_key(key)
                .is_some_and(super::codecs::ComponentEntry::is_persistent)
        })
    }

    // Owned compounds expose their complete cardinality, so this map cannot
    // grow after its one reservation. Charge one table, without growth overlap.
    fn with_persistent_capacity(count: usize) -> Option<Self> {
        let mut patch = Self::new();
        if !budget::is_active() || count == 0 {
            return Some(patch);
        }
        budget::charge::<u8>(Self::persistent_capacity_bytes(count)?).ok()?;
        patch.entries.try_reserve(count).ok()?;
        Some(patch)
    }

    fn persistent_capacity_bytes(count: usize) -> Option<usize> {
        if count == 0 {
            return Some(0);
        }
        // std's pinned hashbrown uses 7/8 load and at most 16-byte control
        // groups. Include aligned entry storage, all controls, and a tail group.
        let buckets = count
            .checked_mul(8)?
            .checked_div(7)?
            .checked_add(1)?
            .checked_next_power_of_two()?
            .max(4);
        let alignment = align_of::<(Identifier, ComponentPatchEntry)>().max(16);
        let data = size_of::<(Identifier, ComponentPatchEntry)>().checked_mul(buckets)?;
        let controls = data.checked_add(alignment - 1)? & !(alignment - 1);
        controls.checked_add(buckets)?.checked_add(16)
    }

    pub(crate) fn from_owned_nbt(tag: &OwnedNbtTag) -> Option<Self> {
        use crate::{REGISTRY, RegistryExt};
        let compound = tag.compound()?;
        let mut patch = Self::with_persistent_capacity(compound.len())?;
        for (key, value) in compound.iter() {
            let key = key.to_str();
            let (key, removed) = match key.strip_prefix('!') {
                Some(key) => (key, true),
                None => (key.as_ref(), false),
            };
            let key = key.parse::<Identifier>().ok()?;
            let entry = REGISTRY.data_components.by_key(&key)?;
            if !entry.is_persistent() {
                return None;
            }
            let value = if removed {
                value.compound()?;
                ComponentPatchEntry::Removed
            } else {
                ComponentPatchEntry::Set(entry.read_nbt_owned(value)?)
            };
            patch.entries.insert(key, value);
        }
        Some(patch)
    }

    pub(crate) fn validate_borrowed(&self) -> Result<()> {
        let mut sink = io::sink();
        nbt_encode::write_bounded(self, 256 * 1024, &mut sink)
    }

    /// Computes Vanilla's `HashOps` value for the persistent patch codec.
    pub fn compute_persistent_hash(&self) -> Result<i32> {
        use crate::{REGISTRY, RegistryExt};

        let mut entries = Vec::new();
        for (key, patch_entry) in &self.entries {
            let Some(component) = REGISTRY.data_components.by_key(key) else {
                continue;
            };
            if !component.is_persistent() {
                continue;
            }

            let (encoded_key, value_hash) = match patch_entry {
                ComponentPatchEntry::Set(data) => (key.to_string(), component.compute_hash(data)?),
                ComponentPatchEntry::Removed => (format!("!{key}"), ().compute_hash()),
            };
            entries.push(hash_entry(encoded_key.compute_hash(), value_hash));
        }
        sort_map_entries(&mut entries);

        let mut hasher = ComponentHasher::new();
        hasher.start_map();
        for entry in &entries {
            hasher.put_raw_bytes(&entry.key_bytes);
            hasher.put_raw_bytes(&entry.value_bytes);
        }
        hasher.end_map();
        Ok(hasher.finish())
    }

    /// Iterates over removed component keys.
    pub fn iter_removed(&self) -> impl Iterator<Item = &Identifier> {
        self.entries.iter().filter_map(|(k, v)| {
            if matches!(v, ComponentPatchEntry::Removed) {
                Some(k)
            } else {
                None
            }
        })
    }

    fn encode_nbt(&self, validate: bool) -> (OwnedNbtTag, Vec<io::Error>) {
        use crate::{REGISTRY, RegistryExt};

        let mut compound = NbtCompound::new();
        let mut errors = Vec::new();

        for (key, entry) in &self.entries {
            let Some(component) = REGISTRY.data_components.by_key(key) else {
                continue;
            };
            if !component.is_persistent() {
                continue;
            }
            match entry {
                ComponentPatchEntry::Set(data) => {
                    let encoded = if validate {
                        component.validate_persistent_encoding(data)
                    } else {
                        component.write_nbt(data)
                    };
                    match encoded {
                        Ok(nbt) => {
                            compound.insert(key.to_string(), nbt);
                        }
                        Err(error) => errors.push(io::Error::other(format!(
                            "failed to encode component {key}: {error}"
                        ))),
                    }
                }
                ComponentPatchEntry::Removed => {
                    compound.insert(format!("!{key}"), NbtCompound::new());
                }
            }
        }

        (OwnedNbtTag::Compound(compound), errors)
    }

    /// Strictly encodes this component patch through its persistent codecs.
    ///
    /// This is the equivalent of Vanilla encoding an untrusted stack through
    /// `ItemStack.CODEC` before accepting it into server state.
    pub fn try_to_nbt_tag_ref(&self) -> Result<OwnedNbtTag> {
        let (tag, errors) = self.encode_nbt(true);
        match errors.into_iter().next() {
            Some(error) => Err(error),
            None => Ok(tag),
        }
    }

    /// Converts this component patch to NBT without consuming it.
    ///
    /// Save-time encoding mirrors Vanilla's `TagValueOutput`: invalid fields
    /// are reported and omitted from the partial result rather than aborting
    /// the owner save.
    #[must_use]
    pub fn to_nbt_tag_ref(&self) -> OwnedNbtTag {
        let (tag, errors) = self.encode_nbt(false);
        for error in errors {
            log::warn!("Item component serialization error: {error}");
        }
        tag
    }
}

pub(super) fn hash_entry(key_hash: i32, value_hash: i32) -> HashEntry {
    let key_hash = key_hash as u32;
    let value_hash = value_hash as u32;
    HashEntry {
        key_hash: i64::from(key_hash),
        value_hash: i64::from(value_hash),
        key_bytes: key_hash.to_le_bytes(),
        value_bytes: value_hash.to_le_bytes(),
    }
}
impl ToNbtTag for DataComponentPatch {
    fn to_nbt_tag(self) -> OwnedNbtTag {
        self.to_nbt_tag_ref()
    }
}

impl DataComponentPatch {
    pub(crate) fn write_optional_nbt_field(
        &self,
        writer: &mut dyn NbtWrite,
        depth: usize,
    ) -> Result<()> {
        self.write_persistent_nbt(writer, depth + 1, Some("components"))
    }

    fn write_persistent_nbt(
        &self,
        writer: &mut dyn NbtWrite,
        depth: usize,
        optional_name: Option<&str>,
    ) -> Result<()> {
        use crate::{REGISTRY, RegistryExt};
        use foton_utils::serial::nbt_encode::{check_depth, end, field_with};
        // Unknown and transient entries consume no output or sorting workspace.
        // Filter them once; repeatedly finding the next key would be quadratic.
        let mut entries = Vec::new();
        let mut minimum = optional_name.map_or(1, |name| name.len().saturating_add(4));
        let max_entries = writer.scratch_remaining()
            / size_of::<(&Identifier, &ComponentPatchEntry, super::ComponentEntryRef)>();
        for (key, value) in &self.entries {
            #[cfg(test)]
            persistent_filter_work::visit();
            let Some(component) = REGISTRY.data_components.by_key(key) else {
                continue;
            };
            if !component.is_persistent() {
                continue;
            }
            minimum = minimum
                .saturating_add(key.namespace.len())
                .saturating_add(key.path.len())
                .saturating_add(5);
            writer.ensure_remaining(minimum)?;
            if entries.len() == max_entries {
                return Err(io::Error::other(
                    "Persistent patch sorting workspace exceeds limit",
                ));
            }
            if entries.len() == entries.capacity() {
                let capacity = entries
                    .capacity()
                    .saturating_mul(2)
                    .max(4)
                    .min(self.entries.len())
                    .min(max_entries);
                entries
                    .try_reserve_exact(capacity - entries.len())
                    .map_err(io::Error::other)?;
            }
            entries.push((key, value, component));
        }
        if optional_name.is_some() && entries.is_empty() {
            return Ok(());
        }
        check_depth(depth)?;
        let used = entries.capacity().saturating_mul(size_of::<(
            &Identifier,
            &ComponentPatchEntry,
            super::ComponentEntryRef,
        )>());
        let mut writer = foton_utils::serial::nbt_stream::ScratchWriter::new(writer, used)?;
        entries.sort_unstable_by(|(a, av, _), (b, bv, _)| {
            let removed = |v: &ComponentPatchEntry| !matches!(v, ComponentPatchEntry::Removed);
            removed(av).cmp(&removed(bv)).then_with(|| {
                a.namespace
                    .bytes()
                    .chain(*b":")
                    .chain(a.path.bytes())
                    .cmp(b.namespace.bytes().chain(*b":").chain(b.path.bytes()))
            })
        });
        let write_entries = |writer: &mut dyn NbtWrite| {
            for (key, value, component) in entries {
                match value {
                    ComponentPatchEntry::Set(value) => {
                        nbt_encode::identifier_field(
                            key,
                            component.nbt_value(value)?,
                            writer,
                            depth,
                        )?;
                    }
                    ComponentPatchEntry::Removed => {
                        check_depth(depth + 1)?;
                        // The key's length was charged before collecting the index.
                        field_with(&format!("!{key}"), 10, writer, |writer| {
                            writer.write_all(&[0])
                        })?;
                    }
                }
            }
            end(writer)
        };
        if let Some(name) = optional_name {
            field_with(name, 10, &mut *writer, write_entries)
        } else {
            write_entries(&mut *writer)
        }
    }
}

impl nbt_encode::NbtEncode for DataComponentPatch {
    fn nbt_id(&self) -> u8 {
        10
    }
    fn write_nbt_payload(&self, writer: &mut dyn NbtWrite, depth: usize) -> Result<()> {
        self.write_persistent_nbt(writer, depth, None)
    }
}

impl EmbeddedNbtCodec for &DataComponentPatch {
    type Error = io::Error;

    fn encode_embedded_nbt(self) -> Result<OwnedNbtTag> {
        self.try_to_nbt_tag_ref()
    }
}

impl FromNbtTag for DataComponentPatch {
    fn from_nbt_tag(tag: BorrowedNbtTag) -> Option<Self> {
        use crate::{REGISTRY, RegistryExt};

        let compound = tag.compound()?;
        budget::charge_map::<Identifier, ComponentPatchEntry>(compound.len()).ok()?;
        let mut patch = Self::new();

        for (key, value) in compound.iter() {
            let key_str = key.to_str();

            if let Some(stripped) = key_str.strip_prefix('!') {
                let id = stripped.parse::<Identifier>().ok()?;
                let entry = REGISTRY.data_components.by_key(&id)?;
                if !entry.is_persistent() || value.compound().is_none() {
                    return None;
                }
                patch.entries.insert(id, ComponentPatchEntry::Removed);
            } else {
                let id = key_str.parse::<Identifier>().ok()?;
                let entry = REGISTRY.data_components.by_key(&id)?;
                if !entry.is_persistent() {
                    return None;
                }
                let component_data = entry.read_nbt(value)?;
                patch
                    .entries
                    .insert(id, ComponentPatchEntry::Set(component_data));
            }
        }

        Some(patch)
    }
}

/// Attempts to extract a typed component from `ComponentData`.
#[must_use]
pub fn component_try_into<T: Component + DowncastType>(
    data: &ComponentData,
    _component: DataComponentType<T>,
) -> Option<&T> {
    data.downcast_ref::<T>()
}

#[cfg(test)]
mod brewing_union_patch_tests {
    use super::*;
    use crate::data_component_predicate::{DataComponentExactPredicate, DataComponentMatchers};
    use crate::data_components::{components::UseRemainder, vanilla_components::USE_REMAINDER};
    use crate::item_predicate::{IntBounds, ItemPredicate, LockCode};
    use crate::{ItemStackTemplate, REGISTRY, RegistryExt, init_vanilla_registry, vanilla_items};
    use foton_utils::serial::ReadFrom;
    use std::io::{Cursor, sink};

    fn recursive_lock(count: usize) -> LockCode {
        let mut patch = DataComponentPatch::new();
        for i in 0..count {
            patch.remove(DataComponentType::<()>::new(Identifier::new(
                "test",
                format!("unregistered_{i:05}"),
            )));
        }
        use crate::data_components::vanilla_components::{CREATIVE_SLOT_LOCK, DAMAGE, ITEM_NAME};
        patch.set(DAMAGE, 7);
        patch.remove(ITEM_NAME);
        patch.remove(CREATIVE_SLOT_LOCK);
        patch.set(
            DataComponentType::<i32>::new(Identifier::new("test", "unknown_set")),
            3,
        );
        let item = ItemStackTemplate::try_with_count_and_patch(&vanilla_items::STONE, 1, patch)
            .expect("unknown removals omitted");
        let component = REGISTRY
            .data_components
            .by_key(USE_REMAINDER.key())
            .expect("use remainder");
        let exact = DataComponentExactPredicate::new(vec![(
            component,
            ComponentData::new(UseRemainder::new(item)),
        )])
        .expect("exact");
        LockCode::new(ItemPredicate::new(
            None,
            IntBounds::ANY,
            DataComponentMatchers::new(exact, vec![]).expect("matchers"),
        ))
    }

    #[test]
    fn brewing_union_persistent_patch_filters_once_through_recursive_lock() {
        init_vanilla_registry();
        let value = recursive_lock(6000);
        let compact = recursive_lock(0);
        let compact_visits = super::persistent_filter_work::measure(|| {
            compact
                .write_bounded(1024, &mut sink())
                .expect("compact lock");
        });
        let mut source_visits = 0;
        let stats = allocation_counter::measure(|| {
            source_visits = super::persistent_filter_work::measure(|| {
                value
                    .write_bounded(1024, &mut sink())
                    .expect("unknown entries are omitted");
            });
        });
        eprintln!("union 6000 persistent entries: {source_visits} visits, {stats:?}");
        assert_eq!(
            source_visits,
            compact_visits + 6000,
            "persistent filtering did not visit each added source entry exactly once"
        );
        assert!(
            stats.bytes_max < 64 * 1024,
            "workspace must omit unknowns: {stats:?}"
        );
        let mut actual = Vec::new();
        value.write_bounded(1024, &mut actual).expect("lock");
        assert!(LockCode::read(&mut Cursor::new(actual.as_slice())).is_ok());
        let mut compact_bytes = Vec::new();
        compact
            .write_bounded(1024, &mut compact_bytes)
            .expect("compact lock");
        assert_eq!(
            actual, compact_bytes,
            "unknown/transient entries must not change persistent bytes"
        );
    }
}

#[cfg(test)]
mod task13_capacity_tests {
    use super::*;
    use foton_utils::serial::budget::DecodeBudget;

    #[test]
    fn brewing_task13_owned_patch_capacity_covers_actual_allocation() {
        for count in [0, 1, 3, 4, 7, 8, 14, 15, 28, 29, 1000] {
            let charged = DataComponentPatch::persistent_capacity_bytes(count).expect("capacity");
            let stats = allocation_counter::measure(|| {
                DecodeBudget::new(charged)
                    .decode(|| {
                        let patch = DataComponentPatch::with_persistent_capacity(count)
                            .expect("prepaid table");
                        assert!(patch.entries.capacity() >= count);
                        Ok(())
                    })
                    .expect("scope");
            });
            assert!(
                stats.bytes_total <= charged as u64,
                "undercharged count {count}: {stats:?}, charged={charged}"
            );
        }
        let stats = allocation_counter::measure(|| {
            DecodeBudget::new(16)
                .decode(|| {
                    budget::charge::<u8>(16)?;
                    assert!(DataComponentPatch::with_persistent_capacity(1).is_none());
                    Ok(())
                })
                .expect("enclosing quota");
        });
        assert!(
            stats.bytes_max < 256,
            "exhausted quota allocated a map: {stats:?}"
        );
    }
}
