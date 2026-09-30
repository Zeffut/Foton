use foton_utils::serial::budget;
use foton_utils::serial::nbt_encode;
use foton_utils::serial::nbt_stream;
use foton_utils::serial::nbt_stream::NbtWrite;
use foton_utils::serial::text_stream;
use io::Write;
use std::io;

use super::{
    BorrowedNbtTag, Component, ComponentData, Cursor, DowncastType, DowncastTypeKey, FromNbtTag,
    HashComponent, Identifier, OwnedNbtTag, ReadFrom, Result, ToNbtTag, WriteTo, read_tag,
};

pub type NetworkReader = fn(&mut Cursor<&[u8]>) -> Result<ComponentData>;

/// Writer function for serializing a component to network format.
pub type NetworkWriter = fn(&ComponentData, &mut dyn Write) -> Result<()>;

/// Reader function for deserializing a component from NBT format.
pub type NbtReader = fn(BorrowedNbtTag) -> Option<ComponentData>;

/// Writer function for serializing a component to NBT format.
pub type NbtWriter = fn(&ComponentData) -> Result<OwnedNbtTag>;

/// Function for hashing a component through its persistent codec shape.
pub(super) type ComponentHash = fn(&ComponentData) -> Result<i32>;
pub(super) type ComponentValidator = fn(&ComponentData) -> Result<()>;
pub(super) type PersistentCodecFns = (
    NbtReader,
    NbtWriter,
    BorrowedNbtValue,
    ComponentHash,
    Option<ComponentValidator>,
);

/// Additional source-value validation required before persistent encoding.
pub(crate) trait ValidatePersistentComponent {
    fn validate_persistent(&self) -> Result<()>;
}

pub(super) fn hash_component<T: DowncastType + HashComponent>(data: &ComponentData) -> Result<i32> {
    let Some(value) = data.downcast_ref::<T>() else {
        return Err(io::Error::other("Component type mismatch"));
    };
    Ok(value.compute_hash())
}

pub(super) fn validate_component<T: DowncastType + ValidatePersistentComponent>(
    data: &ComponentData,
) -> Result<()> {
    let Some(value) = data.downcast_ref::<T>() else {
        return Err(io::Error::other("Component type mismatch"));
    };
    value.validate_persistent()
}

pub(super) fn read_typed_network<T: Component + ReadFrom>(
    cursor: &mut Cursor<&[u8]>,
) -> Result<ComponentData> {
    Ok(ComponentData::new(T::read(cursor)?))
}

pub(crate) fn write_to_network<T: WriteTo>(value: &T, writer: &mut dyn Write) -> Result<()> {
    value.write(&mut DynWriter(writer))
}

pub(super) fn write_typed_network<T: DowncastType + WriteTo>(
    data: &ComponentData,
    writer: &mut dyn Write,
) -> Result<()> {
    let Some(value) = data.downcast_ref::<T>() else {
        return Err(io::Error::other("Component type mismatch"));
    };
    write_to_network(value, writer)
}

struct DynWriter<'a>(&'a mut dyn Write);

impl Write for DynWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

pub(super) fn read_typed_nbt<T: Component + FromNbtTag>(
    tag: BorrowedNbtTag,
) -> Option<ComponentData> {
    T::from_nbt_tag(tag).map(ComponentData::new)
}

pub(super) fn write_typed_nbt<T: DowncastType + ToNbtTag + Clone>(
    data: &ComponentData,
) -> Result<OwnedNbtTag> {
    let Some(value) = data.downcast_ref::<T>() else {
        return Err(io::Error::other("Component type mismatch"));
    };
    Ok(value.clone().to_nbt_tag())
}

struct NetworkCodecs {
    reader: NetworkReader,
    writer: NetworkWriter,
}

type BorrowedNbtValue = fn(&ComponentData) -> Result<&dyn nbt_encode::NbtEncode>;

pub(super) fn borrowed_nbt<T: DowncastType + nbt_encode::NbtEncode>(
    data: &ComponentData,
) -> Result<&dyn nbt_encode::NbtEncode> {
    let value = data
        .downcast_ref::<T>()
        .ok_or_else(|| io::Error::other("Component type mismatch"))?;
    Ok(value)
}

struct PersistentCodecs {
    reader: NbtReader,
    writer: NbtWriter,
    borrowed_value: BorrowedNbtValue,
    hash: ComponentHash,
    validator: Option<fn(&ComponentData) -> Result<()>>,
}

struct ComponentCodecs {
    expected_type_key: DowncastTypeKey,
    value_size: usize,
    network: NetworkCodecs,
    persistent: Option<PersistentCodecs>,
}

/// Metadata for a registered component type.
///
/// Contains the component's key and all serialization functions needed
/// to read/write the component for network and persistent storage.
pub struct ComponentEntry {
    /// The component's identifier (e.g., "minecraft:damage")
    pub key: Identifier,
    codecs: ComponentCodecs,
    ignore_swap_animation: bool,
}

impl ComponentEntry {
    #[must_use]
    pub(super) fn implemented<T: DowncastType>(
        key: Identifier,
        network_reader: NetworkReader,
        network_writer: NetworkWriter,
        persistent_codecs: Option<PersistentCodecFns>,
        ignore_swap_animation: bool,
    ) -> Self {
        Self {
            key,
            codecs: ComponentCodecs {
                expected_type_key: T::TYPE_KEY,
                value_size: std::mem::size_of::<T>(),
                network: NetworkCodecs {
                    reader: network_reader,
                    writer: network_writer,
                },
                persistent: persistent_codecs.map(
                    |(reader, writer, borrowed_value, hash, validator)| PersistentCodecs {
                        reader,
                        writer,
                        borrowed_value,
                        hash,
                        validator,
                    },
                ),
            },
            ignore_swap_animation,
        }
    }

    /// Validates that a `ComponentData` value matches the concrete type for this component.
    ///
    /// Returns `true` if the data is valid for this component type, `false` otherwise.
    /// This prevents plugins from setting wrong types on vanilla components.
    #[must_use]
    pub fn validates(&self, data: &ComponentData) -> bool {
        data.type_key() == self.codecs.expected_type_key
    }

    /// Decodes this component's network value.
    pub fn read_network(&self, data: &mut Cursor<&[u8]>) -> Result<ComponentData> {
        budget::charge::<u8>(self.codecs.value_size)?;
        let ComponentCodecs {
            network,
            expected_type_key,
            ..
        } = &self.codecs;
        let value = (network.reader)(data)?;
        if value.type_key() != *expected_type_key {
            return Err(io::Error::other(format!(
                "Network codec returned the wrong value type for {}",
                self.key
            )));
        }
        Ok(value)
    }

    /// Encodes this component's network value after validating its concrete type.
    pub fn write_network(&self, data: &ComponentData, writer: &mut Vec<u8>) -> Result<()> {
        self.write_network_to(data, writer)
    }

    /// Encodes this component directly to a caller-supplied network sink.
    pub fn write_network_to(&self, data: &ComponentData, writer: &mut dyn Write) -> Result<()> {
        if !self.validates(data) {
            return Err(io::Error::other(format!(
                "Component value type does not match {}",
                self.key
            )));
        }
        (self.codecs.network.writer)(data, writer)
    }

    /// Encodes with an explicit caller byte budget, including NBT/text temporaries.
    pub fn write_network_bounded(
        &self,
        data: &ComponentData,
        budget: usize,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let mut writer = nbt_stream::LimitedWriter::new(writer, budget);
        self.write_network_in(data, &mut writer)
    }

    pub(crate) fn write_network_in(
        &self,
        data: &ComponentData,
        writer: &mut dyn NbtWrite,
    ) -> Result<()> {
        if !self.validates(data) {
            return Err(io::Error::other("Component type mismatch"));
        }
        if let Some(value) = data.downcast_ref::<crate::item_predicate::AdventureModePredicate>() {
            return value.write_bounded(writer);
        }
        if let Some(value) =
            data.downcast_ref::<crate::data_components::components::CustomModelData>()
        {
            return value.write_bounded(writer);
        }
        if let Some(result) = super::recursive_codec::write(data, writer) {
            return result;
        }
        self.write_network_leaf(data, writer.remaining(), writer)
    }

    fn write_network_leaf(
        &self,
        data: &ComponentData,
        budget: usize,
        writer: &mut dyn Write,
    ) -> Result<()> {
        if let Some(value) = data.downcast_ref::<crate::data_components::components::ArmorTrim>() {
            let mut writer = nbt_stream::LimitedWriter::new(writer, budget);
            value
                .material()
                .write_bounded(writer.remaining(), &mut writer)?;
            return super::indirect_text::pattern(value.pattern(), &mut writer);
        }
        if let Some(value) =
            data.downcast_ref::<crate::data_components::components::ProvidesTrimMaterial>()
        {
            return value.material().write_bounded(budget, writer);
        }
        if let Some(value) =
            data.downcast_ref::<crate::data_components::components::ItemEnchantments>()
        {
            return value.write_bounded(budget, writer);
        }
        if let Some(value) = data.downcast_ref::<text_components::TextComponent>() {
            return text_stream::write_bounded(value, budget, writer);
        }
        if let Some(value) = data.downcast_ref::<crate::data_components::components::ItemLore>() {
            return value.write_bounded(budget, writer);
        }
        if let Some(value) =
            data.downcast_ref::<crate::data_components::components::WrittenBookContent>()
        {
            return value.write_bounded(budget, writer);
        }
        if let Some(value) = data.downcast_ref::<crate::item_predicate::LockCode>() {
            return value.write_bounded(budget, writer);
        }
        if let Some(value) = data.downcast_ref::<crate::data_components::components::CustomData>() {
            return nbt_stream::write_compound_bounded(value.as_compound(), budget, writer);
        }
        let mut writer = nbt_stream::LimitedWriter::new(writer, budget);
        if let Some(result) = super::indirect_text::write(data, &mut writer) {
            return result;
        }
        if let Some(value) = data.downcast_ref::<crate::resolvable_profile::ResolvableProfile>() {
            return value.write_canonical(&mut writer);
        }
        if let Some(value) = data.downcast_ref::<crate::data_components::components::EntityData>() {
            return value.write_bounded(writer.remaining(), &mut writer);
        }
        if let Some(value) =
            data.downcast_ref::<crate::data_components::components::BlockEntityData>()
        {
            return value.write_bounded(writer.remaining(), &mut writer);
        }
        if let Some(value) = data.downcast_ref::<crate::data_components::components::Bees>() {
            return value.write_bounded(writer.remaining(), &mut writer);
        }
        // These network codecs use their persistent NBT representation.
        if matches!(
            self.key.path.as_ref(),
            "container_loot" | "debug_stick_state" | "map_decorations" | "recipes"
        ) {
            return self.write_nbt_bounded(data, &mut writer, 0);
        }
        self.write_network_to(data, &mut writer)
    }

    pub(crate) fn write_nbt_bounded(
        &self,
        data: &ComponentData,
        writer: &mut dyn NbtWrite,
        depth: usize,
    ) -> Result<()> {
        let value = self.nbt_value(data)?;
        if value.nbt_id() == 10 {
            nbt_encode::check_depth(depth)?;
        }
        writer.write_all(&[value.nbt_id()])?;
        value.write_nbt_payload(writer, depth)
    }

    pub(crate) fn nbt_value<'a>(
        &self,
        data: &'a ComponentData,
    ) -> Result<&'a dyn nbt_encode::NbtEncode> {
        if !self.validates(data) {
            return Err(io::Error::other("Component type mismatch"));
        }
        if let Some(value) = data.downcast_ref::<i32>() {
            let range = match self.key.path.as_ref() {
                "max_stack_size" => 1..=99,
                "max_damage" => 1..=i32::MAX,
                "damage" | "repair_cost" => 0..=i32::MAX,
                _ => i32::MIN..=i32::MAX,
            };
            if !range.contains(value) {
                return Err(io::Error::other(
                    "Component integer outside persistent range",
                ));
            }
        }
        if let Some(value) = data.downcast_ref::<f32>()
            && (!value.is_finite()
                || value.is_sign_negative()
                || (self.key.path == "minimum_attack_charge" && *value > 1.0))
        {
            return Err(io::Error::other("Component float outside persistent range"));
        }
        let persistent = self
            .codecs
            .persistent
            .as_ref()
            .ok_or_else(|| io::Error::other("Transient component has no persistent codec"))?;
        // Each borrowed codec checks its local persistent invariants while
        // streaming; recursive prevalidation would revisit descendant subtrees.
        (persistent.borrowed_value)(data)
    }

    /// Decodes this component's persistent NBT value.
    #[must_use]
    pub fn read_nbt(&self, tag: BorrowedNbtTag) -> Option<ComponentData> {
        budget::charge::<u8>(self.codecs.value_size).ok()?;
        let Some(persistent) = &self.codecs.persistent else {
            return None;
        };
        let value = (persistent.reader)(tag)?;
        (value.type_key() == self.codecs.expected_type_key).then_some(value)
    }

    /// Encodes this component's persistent NBT value after validating its concrete type.
    pub fn write_nbt(&self, data: &ComponentData) -> Result<OwnedNbtTag> {
        if !self.validates(data) {
            return Err(io::Error::other(format!(
                "Component value type does not match {}",
                self.key
            )));
        }
        let Some(persistent) = &self.codecs.persistent else {
            return Err(io::Error::other(format!(
                "Transient component {} has no persistent codec",
                self.key
            )));
        };
        (persistent.writer)(data)
    }

    /// Checks that a value accepted by the stream codec is also accepted by
    /// the persistent codec.
    pub fn validate_persistent_encoding(&self, data: &ComponentData) -> Result<OwnedNbtTag> {
        if let Some(validator) = self
            .codecs
            .persistent
            .as_ref()
            .and_then(|persistent| persistent.validator)
        {
            validator(data)?;
        }
        let tag = self.write_nbt(data)?;
        if self.read_nbt_owned(&tag).is_none() {
            return Err(io::Error::other(format!(
                "Persistent codec for component {} rejected its encoded value",
                self.key
            )));
        }
        Ok(tag)
    }

    /// Computes the vanilla `HashOps` value through this component's persistent codec.
    pub fn compute_hash(&self, data: &ComponentData) -> Result<i32> {
        if !self.validates(data) {
            return Err(io::Error::other(format!(
                "Component value type does not match {}",
                self.key
            )));
        }
        if !self.is_persistent() {
            return Err(io::Error::other(format!(
                "Transient component {} has no persistent hash codec",
                self.key
            )));
        }
        let Some(persistent) = &self.codecs.persistent else {
            return Err(io::Error::other(format!(
                "Transient component {} has no persistent hash codec",
                self.key
            )));
        };
        self.validate_persistent_encoding(data)?;
        (persistent.hash)(data)
    }

    /// Returns whether vanilla defines this as a persistent component.
    #[must_use]
    pub const fn is_persistent(&self) -> bool {
        self.codecs.persistent.is_some()
    }

    /// Returns whether changes to this component are ignored for held-item swap animation.
    #[must_use]
    pub const fn ignore_swap_animation(&self) -> bool {
        self.ignore_swap_animation
    }

    /// Decodes an owned NBT value with this component's registered persistent codec.
    #[must_use]
    pub fn read_nbt_owned(&self, tag: &OwnedNbtTag) -> Option<ComponentData> {
        if !self.is_persistent() {
            return None;
        }
        if self.codecs.expected_type_key == crate::resolvable_profile::ResolvableProfile::TYPE_KEY {
            // Owned numeric lists expose their count without the borrowed parser's Vec conversion.
            crate::resolvable_profile::ResolvableProfile::check_owned_property_count(tag)?;
        }
        let owned_reader = super::recursive_codec::owned_reader(self.codecs.expected_type_key);
        if super::PersistentValidationScope::is_deferred()
            && let Some(read) = owned_reader
        {
            // The owner validates once after its scope ends. Leaf adapters still
            // use the registered reader; recursive descendants borrow this tree.
            budget::charge::<u8>(self.codecs.value_size).ok()?;
            return read(tag);
        }
        let mut bytes = if budget::is_active() {
            let length = nbt_stream::wire_size(tag, 256 * 1024).ok()?;
            // simdnbt 0.10 starts its borrowed main tape with 1024 u64 entries.
            // Variable tape growth is included by the NBT node preflight.
            budget::charge::<u64>(1024).ok()?;
            budget::read_vec(length, length).ok()?
        } else {
            Vec::new()
        };
        if budget::is_active() {
            nbt_stream::write_tag(
                tag,
                &mut nbt_stream::LimitedWriter::new(&mut bytes, 256 * 1024),
                0,
            )
            .ok()?;
        } else {
            tag.write(&mut bytes);
        }
        if owned_reader.is_some() {
            foton_utils::serial::nbt_preflight::check_single_owner(&Cursor::new(bytes.as_slice()))
                .ok()?;
        } else {
            foton_utils::serial::nbt_preflight::check(&Cursor::new(bytes.as_slice()), true).ok()?;
        }
        let borrowed = read_tag(&mut Cursor::new(bytes.as_slice())).ok()?;
        self.read_nbt(borrowed.as_tag())
    }
}

pub type ComponentEntryRef = &'static ComponentEntry;
