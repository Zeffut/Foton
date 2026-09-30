use super::*;
use crate::data_components::DataComponentPatch;
use crate::data_components::PersistentValidationScope;
use crate::item_stack::ItemStack;
use foton_utils::serial::budget;
use foton_utils::serial::nbt_encode;
use foton_utils::serial::nbt_stream;
use foton_utils::serial::nbt_stream::NbtWrite;

/// Typed payload behavior for a registered partial component predicate.
pub trait DataComponentPredicateCodec:
    DowncastType
    + Clone
    + Debug
    + PartialEq
    + HashComponent
    + Send
    + Sync
    + nbt_encode::NbtEncode
    + 'static
{
    fn from_nbt_value(tag: &NbtTag) -> Option<Self>;
    fn to_nbt_value(&self) -> NbtTag;
}

trait ErasedDataComponentPredicate: ErasedType + Debug + Send + Sync + nbt_encode::NbtEncode {
    fn clone_predicate(&self) -> Box<dyn ErasedDataComponentPredicate>;
    fn predicate_eq(&self, other: &dyn ErasedDataComponentPredicate) -> bool;
}

impl<T: DataComponentPredicateCodec> ErasedDataComponentPredicate for T {
    fn clone_predicate(&self) -> Box<dyn ErasedDataComponentPredicate> {
        Box::new(self.clone())
    }

    fn predicate_eq(&self, other: &dyn ErasedDataComponentPredicate) -> bool {
        other.downcast_ref::<T>() == Some(self)
    }
}

type PredicateReader = fn(&NbtTag) -> Option<Box<dyn ErasedDataComponentPredicate>>;
type PredicateWriter = fn(&dyn ErasedDataComponentPredicate) -> NbtTag;
type PredicateHasher = fn(&dyn ErasedDataComponentPredicate, &mut ComponentHasher);

/// Registered discriminator and codecs for one concrete predicate value.
pub struct DataComponentPredicateType {
    pub key: Identifier,
    expected_type_key: DowncastTypeKey,
    reader: PredicateReader,
    writer: PredicateWriter,
    hasher: PredicateHasher,
}

impl DataComponentPredicateType {
    #[must_use]
    pub const fn of<T: DataComponentPredicateCodec>(key: Identifier) -> Self {
        Self {
            key,
            expected_type_key: T::TYPE_KEY,
            reader: read_predicate::<T>,
            writer: write_predicate::<T>,
            hasher: hash_predicate::<T>,
        }
    }
}

impl Debug for DataComponentPredicateType {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DataComponentPredicateType")
            .field("key", &self.key)
            .field("expected_type_key", &self.expected_type_key)
            .finish_non_exhaustive()
    }
}

pub type DataComponentPredicateTypeRef = &'static DataComponentPredicateType;

#[derive(Clone, Copy, PartialEq, Eq)]
enum PredicateDiscriminator {
    Concrete(DataComponentPredicateTypeRef),
    Any(ComponentEntryRef),
}

impl PredicateDiscriminator {
    const fn key(&self) -> &Identifier {
        match *self {
            Self::Concrete(predicate_type) => &predicate_type.key,
            Self::Any(component) => &component.key,
        }
    }
}

impl Debug for PredicateDiscriminator {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("PredicateDiscriminator")
            .field(self.key())
            .finish()
    }
}

/// Type-erased predicate value retaining Foton's deterministic concrete key.
pub struct DataComponentPredicateData {
    discriminator: PredicateDiscriminator,
    value: Option<Box<dyn ErasedDataComponentPredicate>>,
}

impl DataComponentPredicateData {
    #[must_use]
    pub fn new<T: DataComponentPredicateCodec>(
        predicate_type: DataComponentPredicateTypeRef,
        value: T,
    ) -> Self {
        assert_eq!(
            predicate_type.expected_type_key,
            T::TYPE_KEY,
            "component predicate value does not match its registered type"
        );
        Self {
            discriminator: PredicateDiscriminator::Concrete(predicate_type),
            value: Some(Box::new(value)),
        }
    }

    #[must_use]
    pub const fn any(component: ComponentEntryRef) -> Self {
        Self {
            discriminator: PredicateDiscriminator::Any(component),
            value: None,
        }
    }

    #[must_use]
    pub const fn predicate_type(&self) -> Option<DataComponentPredicateTypeRef> {
        match self.discriminator {
            PredicateDiscriminator::Concrete(predicate_type) => Some(predicate_type),
            PredicateDiscriminator::Any(_) => None,
        }
    }

    #[must_use]
    pub const fn any_component(&self) -> Option<ComponentEntryRef> {
        match self.discriminator {
            PredicateDiscriminator::Concrete(_) => None,
            PredicateDiscriminator::Any(component) => Some(component),
        }
    }

    #[must_use]
    pub fn downcast_ref<T: DowncastType>(&self) -> Option<&T> {
        self.value.as_deref()?.downcast_ref::<T>()
    }

    #[must_use]
    pub const fn key(&self) -> &Identifier {
        self.discriminator.key()
    }

    fn validate_persistent_discriminator(&self) -> Result<()> {
        let valid = match self.discriminator {
            PredicateDiscriminator::Concrete(ty) => REGISTRY
                .data_component_predicate_types
                .by_key(&ty.key)
                .is_some_and(|registered| registered.expected_type_key == ty.expected_type_key),
            PredicateDiscriminator::Any(component) => {
                // The persistent codec resolves concrete predicate keys first. An
                // any-value predicate emits {}, which that reader must accept.
                REGISTRY.data_components.by_key(&component.key).is_some()
                    && REGISTRY
                        .data_component_predicate_types
                        .by_key(&component.key)
                        .is_none_or(|ty| {
                            (ty.reader)(&NbtTag::Compound(NbtCompound::new())).is_some()
                        })
            }
        };
        if !valid {
            return Err(Error::other("Invalid persistent predicate discriminator"));
        }
        Ok(())
    }

    pub(crate) fn write_nbt_stream(&self, writer: &mut dyn NbtWrite, depth: usize) -> Result<()> {
        if let Some(value) = &self.value {
            if value.nbt_id() == 10 {
                nbt_encode::check_depth(depth)?;
            }
            writer.write_all(&[value.nbt_id()])?;
            return value.write_nbt_payload(writer, depth);
        }
        nbt_encode::check_depth(depth)?;
        writer.write_all(&[10, 0])
    }

    pub(super) fn from_persistent_entry(key: &Identifier, tag: &NbtTag) -> Option<Self> {
        if let Some(predicate_type) = REGISTRY.data_component_predicate_types.by_key(key) {
            return Some(Self {
                discriminator: PredicateDiscriminator::Concrete(predicate_type),
                value: Some((predicate_type.reader)(tag)?),
            });
        }
        let component = REGISTRY.data_components.by_key(key)?;
        tag.compound()?;
        Some(Self::any(component))
    }

    #[cfg_attr(
        not(test),
        expect(
            clippy::panic,
            reason = "the discriminator and the payload are set together by the registration that made this entry"
        )
    )]
    fn to_nbt_value(&self) -> NbtTag {
        match (self.discriminator, self.value.as_deref()) {
            (PredicateDiscriminator::Concrete(predicate_type), Some(value)) => {
                (predicate_type.writer)(value)
            }
            (PredicateDiscriminator::Any(_), None) => NbtTag::Compound(NbtCompound::new()),
            _ => panic!("component predicate discriminator and value disagree"),
        }
    }

    #[cfg_attr(
        not(test),
        expect(
            clippy::panic,
            reason = "the discriminator and the payload are set together by the registration that made this entry"
        )
    )]
    fn hash_value(&self, hasher: &mut ComponentHasher) {
        match (self.discriminator, self.value.as_deref()) {
            (PredicateDiscriminator::Concrete(predicate_type), Some(value)) => {
                (predicate_type.hasher)(value, hasher);
            }
            (PredicateDiscriminator::Any(_), None) => {
                hasher.start_map();
                hasher.end_map();
            }
            _ => panic!("component predicate discriminator and value disagree"),
        }
    }
}

impl Clone for DataComponentPredicateData {
    fn clone(&self) -> Self {
        Self {
            discriminator: self.discriminator,
            value: self.value.as_ref().map(|value| value.clone_predicate()),
        }
    }
}

impl Debug for DataComponentPredicateData {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DataComponentPredicateData")
            .field("key", self.key())
            .field("value", &self.value)
            .finish()
    }
}

impl PartialEq for DataComponentPredicateData {
    fn eq(&self, other: &Self) -> bool {
        if self.discriminator != other.discriminator {
            return false;
        }
        match (self.value.as_deref(), other.value.as_deref()) {
            (Some(left), Some(right)) => left.predicate_eq(right),
            (None, None) => true,
            _ => false,
        }
    }
}

/// Registry of concrete partial component predicate types.
pub struct DataComponentPredicateTypeRegistry {
    types_by_id: Vec<DataComponentPredicateTypeRef>,
    types_by_key: FxHashMap<Identifier, usize>,
    allows_registering: bool,
}

impl DataComponentPredicateTypeRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            types_by_id: Vec::new(),
            types_by_key: FxHashMap::default(),
            allows_registering: true,
        }
    }
}

crate::impl_standard_methods!(
    DataComponentPredicateTypeRegistry,
    DataComponentPredicateTypeRef,
    types_by_id,
    types_by_key,
    allows_registering
);
crate::impl_registry!(
    DataComponentPredicateTypeRegistry,
    DataComponentPredicateType,
    types_by_id,
    types_by_key,
    data_component_predicate_types
);

fn read_predicate<T: DataComponentPredicateCodec>(
    tag: &NbtTag,
) -> Option<Box<dyn ErasedDataComponentPredicate>> {
    // Container/bundle predicates borrow recursive item descendants. Other
    // predicate leaves may retain copied text/list suffixes during conversion.
    if T::TYPE_KEY != ContainerPredicate::TYPE_KEY && T::TYPE_KEY != BundlePredicate::TYPE_KEY {
        foton_utils::serial::nbt_preflight::check_owned(tag, 256 * 1024).ok()?;
    }
    T::from_nbt_value(tag).map(|value| Box::new(value) as Box<dyn ErasedDataComponentPredicate>)
}

#[cfg_attr(
    not(test),
    expect(
        clippy::panic,
        reason = "the discriminator and the payload are set together by the registration that made this entry"
    )
)]
fn write_predicate<T: DataComponentPredicateCodec>(
    value: &dyn ErasedDataComponentPredicate,
) -> NbtTag {
    let Some(value) = value.downcast_ref::<T>() else {
        panic!("registered component predicate writer received the wrong concrete type");
    };
    value.to_nbt_value()
}

#[cfg_attr(
    not(test),
    expect(
        clippy::panic,
        reason = "the discriminator and the payload are set together by the registration that made this entry"
    )
)]
fn hash_predicate<T: DataComponentPredicateCodec>(
    value: &dyn ErasedDataComponentPredicate,
    hasher: &mut ComponentHasher,
) {
    let Some(value) = value.downcast_ref::<T>() else {
        panic!("registered component predicate hasher received the wrong concrete type");
    };
    value.hash_component(hasher);
}

/// Exact component values required by a component matcher.
#[derive(Clone, PartialEq)]
pub struct DataComponentExactPredicate {
    values: Vec<(ComponentEntryRef, ComponentData)>,
}

impl Debug for DataComponentExactPredicate {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_list()
            .entries(self.values.iter().map(|(entry, value)| (&entry.key, value)))
            .finish()
    }
}

impl DataComponentExactPredicate {
    pub const EMPTY: Self = Self { values: Vec::new() };

    /// Creates an exact predicate only when every persistent value can round-trip.
    ///
    /// Vanilla's direct stream codec can admit values rejected by a component's
    /// persistent codec. Foton rejects those here because exact predicates are
    /// nested in item components and must not make their containing stack unsavable.
    #[must_use]
    pub fn new(values: Vec<(ComponentEntryRef, ComponentData)>) -> Option<Self> {
        budget::charge_map::<Identifier, ()>(values.len()).ok()?;
        let mut keys = FxHashSet::default();
        if !values
            .iter()
            .all(|(entry, value)| entry.validates(value) && keys.insert(&entry.key))
        {
            return None;
        }
        if budget::is_active() {
            if !PersistentValidationScope::is_deferred() {
                let mut sink = std::io::sink();
                let mut writer = nbt_stream::LimitedWriter::persistent(&mut sink, 256 * 1024);
                for (entry, value) in &values {
                    if entry.is_persistent() {
                        entry.write_nbt_bounded(value, &mut writer, 0).ok()?;
                    }
                }
            }
        } else {
            for (entry, value) in &values {
                if entry.is_persistent() {
                    entry.validate_persistent_encoding(value).ok()?;
                }
            }
        }
        Some(Self { values })
    }

    #[must_use]
    pub fn all_of(components: &DataComponentMap) -> Option<Self> {
        let values = components
            .keys()
            .map(|key| {
                Some((
                    REGISTRY.data_components.by_key(key)?,
                    components.get_raw(key)?.clone(),
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        Self::new(values)
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    #[must_use]
    pub fn values(&self) -> &[(ComponentEntryRef, ComponentData)] {
        &self.values
    }

    fn from_owned_nbt(tag: &NbtTag) -> Option<Self> {
        let compound = tag.compound()?;
        let scope = PersistentValidationScope::enter();
        let mut values = Vec::with_capacity(compound.len());
        for (key, value) in compound.iter() {
            let key = key.to_owned().try_into_string().ok()?.parse().ok()?;
            let entry = REGISTRY.data_components.by_key(&key)?;
            if !entry.is_persistent() {
                return None;
            }
            values.push((entry, entry.read_nbt_owned(value)?));
        }
        drop(scope);
        Self::new(values)
    }

    /// Builds the component patch this predicate stands for.
    ///
    /// Vanilla parity: `DataComponentExactPredicate.asPatch`, which `ItemCost`
    /// uses to build the stack the trade screen draws for a price.
    #[must_use]
    pub fn as_patch(&self) -> DataComponentPatch {
        let mut patch = DataComponentPatch::new();
        for (entry, value) in &self.values {
            patch.set_raw(entry.key.clone(), value.clone());
        }
        patch
    }

    /// Returns `true` if `stack` carries exactly these component values.
    ///
    /// Vanilla parity: `DataComponentExactPredicate.test`. It compares against
    /// the stack's *effective* value, so an item whose prototype already has the
    /// required value matches without carrying a patch of its own.
    #[must_use]
    pub fn test(&self, stack: &ItemStack) -> bool {
        self.values
            .iter()
            .all(|(entry, value)| stack.get_effective_value_raw(&entry.key) == Some(value))
    }

    #[cfg_attr(
        not(test),
        expect(
            clippy::panic,
            reason = "the codec that produced this value is the one reading it back, so its shape is settled a few lines up"
        )
    )]
    fn to_nbt_value(&self) -> NbtTag {
        let mut compound = NbtCompound::new();
        for (entry, value) in &self.values {
            if !entry.is_persistent() {
                continue;
            }
            let Ok(value) = entry.write_nbt(value) else {
                panic!("validated exact component predicate failed to encode");
            };
            compound.insert(entry.key.to_string(), value);
        }
        NbtTag::Compound(compound)
    }
}

impl WriteTo for DataComponentExactPredicate {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        write_len(self.values.len(), writer)?;
        for (entry, value) in &self.values {
            write_registry_id(*entry, writer, "data component")?;
            let mut encoded = Vec::new();
            entry.write_network(value, &mut encoded)?;
            writer.write_all(&encoded)?;
        }
        Ok(())
    }
}

impl ReadFrom for DataComponentExactPredicate {
    fn read(data: &mut Cursor<&[u8]>) -> Result<Self> {
        let count = read_len(data, usize::MAX, "exact component predicate")?;
        budget::check_collection_input(data, count, 1)?;
        let scope = PersistentValidationScope::enter();
        let mut values = budget::read_vec(count, count.min(1024))?;
        for _ in 0..count {
            let entry = read_component_entry(data)?;
            values.push((entry, entry.read_network(data)?));
        }
        drop(scope);
        Self::new(values)
            .ok_or_else(|| Error::other("duplicate or mismatched exact component predicate"))
    }
}

impl HashComponent for DataComponentExactPredicate {
    #[cfg_attr(
        not(test),
        expect(
            clippy::panic,
            reason = "the codec that produced this value is the one reading it back, so its shape is settled a few lines up"
        )
    )]
    fn hash_component(&self, hasher: &mut ComponentHasher) {
        let mut entries = Vec::new();
        for (entry, value) in &self.values {
            if !entry.is_persistent() {
                continue;
            }
            let Ok(value_hash) = entry.compute_hash(value) else {
                panic!("validated exact component predicate failed to hash");
            };
            entries.push(HashEntry::from_hashes(
                entry.key.compute_hash() as u32,
                value_hash as u32,
            ));
        }
        hash_entries(hasher, &mut entries);
    }
}

/// Exact and partial component conditions flattened into block/item predicates.
#[derive(Debug, Clone, PartialEq)]
pub struct DataComponentMatchers {
    exact: DataComponentExactPredicate,
    partial: Vec<DataComponentPredicateData>,
}

impl DataComponentMatchers {
    pub const ANY: Self = Self {
        exact: DataComponentExactPredicate::EMPTY,
        partial: Vec::new(),
    };

    #[must_use]
    pub fn new(
        exact: DataComponentExactPredicate,
        partial: Vec<DataComponentPredicateData>,
    ) -> Option<Self> {
        budget::charge_map::<Identifier, ()>(partial.len()).ok()?;
        let mut keys = FxHashSet::default();
        partial
            .iter()
            .all(|predicate| keys.insert(predicate.key().clone()))
            .then_some(Self { exact, partial })
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.exact.is_empty() && self.partial.is_empty()
    }

    #[must_use]
    pub const fn exact(&self) -> &DataComponentExactPredicate {
        &self.exact
    }

    #[must_use]
    pub fn partial(&self) -> &[DataComponentPredicateData] {
        &self.partial
    }

    pub(crate) fn from_fields(compound: &NbtCompound) -> Option<Self> {
        let exact = compound.get("components").map_or(
            Some(DataComponentExactPredicate::EMPTY),
            DataComponentExactPredicate::from_owned_nbt,
        )?;
        let partial = if let Some(tag) = compound.get("predicates") {
            let values = tag.compound()?;
            let mut predicates = Vec::with_capacity(values.len());
            for (key, value) in values.iter() {
                let key = key.to_owned().try_into_string().ok()?.parse().ok()?;
                predicates.push(DataComponentPredicateData::from_persistent_entry(
                    &key, value,
                )?);
            }
            predicates
        } else {
            Vec::new()
        };
        Self::new(exact, partial)
    }

    pub(crate) fn write_fields_stream(
        &self,
        writer: &mut dyn NbtWrite,
        depth: usize,
    ) -> Result<()> {
        use foton_utils::serial::nbt_encode::{check_depth, field_with};
        let count = self
            .exact
            .values()
            .iter()
            .filter(|(entry, _)| entry.is_persistent())
            .count();
        if count != 0 {
            check_depth(depth)?;
            field_with("components", 10, writer, |writer| {
                writer.ensure_remaining(count.saturating_mul(4).saturating_add(1))?;
                let required =
                    count.saturating_mul(size_of::<&(ComponentEntryRef, ComponentData)>());
                let mut writer = nbt_stream::ScratchWriter::new(writer, required)?;
                let mut entries = Vec::new();
                entries.try_reserve_exact(count).map_err(Error::other)?;
                entries.extend(
                    self.exact
                        .values()
                        .iter()
                        .filter(|(entry, _)| entry.is_persistent()),
                );
                entries.sort_unstable_by_key(|(entry, _)| {
                    (entry.key.namespace.as_ref(), entry.key.path.as_ref())
                });
                for (entry, value) in entries {
                    nbt_encode::identifier_field(
                        &entry.key,
                        entry.nbt_value(value)?,
                        &mut *writer,
                        depth,
                    )?;
                }
                writer.write_all(&[0])
            })?;
        }
        if !self.partial.is_empty() {
            check_depth(depth)?;
            field_with("predicates", 10, writer, |writer| {
                writer.ensure_remaining(self.partial.len().saturating_mul(4).saturating_add(1))?;
                let mut minimum = 1usize;
                for value in &self.partial {
                    minimum = minimum
                        .saturating_add(value.key().namespace.len())
                        .saturating_add(value.key().path.len())
                        .saturating_add(5);
                    writer.ensure_remaining(minimum)?;
                    value.validate_persistent_discriminator()?;
                }
                let required = self
                    .partial
                    .len()
                    .saturating_mul(size_of::<&DataComponentPredicateData>());
                let mut writer = nbt_stream::ScratchWriter::new(writer, required)?;
                let mut entries: Vec<_> = self.partial.iter().collect();
                entries.sort_unstable_by_key(|value| {
                    (value.key().namespace.as_ref(), value.key().path.as_ref())
                });
                for value in entries {
                    let payload: &dyn nbt_encode::NbtEncode = match &value.value {
                        Some(value) => value.as_ref(),
                        None => &(),
                    };
                    nbt_encode::identifier_field(value.key(), payload, &mut *writer, depth)?;
                }
                writer.write_all(&[0])
            })?;
        }
        Ok(())
    }

    pub(crate) fn write_fields(&self, compound: &mut NbtCompound) {
        if self
            .exact
            .values()
            .iter()
            .any(|(entry, _)| entry.is_persistent())
        {
            compound.insert("components", self.exact.to_nbt_value());
        }
        if !self.partial.is_empty() {
            let mut predicates = NbtCompound::new();
            for predicate in &self.partial {
                predicates.insert(predicate.key().to_string(), predicate.to_nbt_value());
            }
            compound.insert("predicates", predicates);
        }
    }

    pub(crate) fn hash_fields(&self, entries: &mut Vec<HashEntry>) {
        if self
            .exact
            .values()
            .iter()
            .any(|(entry, _)| entry.is_persistent())
        {
            push_hash_entry(entries, "components", &self.exact);
        }
        if !self.partial.is_empty() {
            let mut value_hasher = ComponentHasher::new();
            self.hash_partial(&mut value_hasher);
            crate::item_predicate::push_prehashed_entry(entries, "predicates", value_hasher);
        }
    }

    fn hash_partial(&self, hasher: &mut ComponentHasher) {
        let mut entries = self
            .partial
            .iter()
            .map(|predicate| {
                let mut key_hasher = ComponentHasher::new();
                predicate.key().hash_component(&mut key_hasher);
                let mut value_hasher = ComponentHasher::new();
                predicate.hash_value(&mut value_hasher);
                HashEntry::new(key_hasher, value_hasher)
            })
            .collect::<Vec<_>>();
        hash_entries(hasher, &mut entries);
    }
}

impl WriteTo for DataComponentMatchers {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        self.exact.write(writer)?;
        if self.partial.len() > 64 {
            return Err(Error::other("partial component predicate count exceeds 64"));
        }
        write_len(self.partial.len(), writer)?;
        for predicate in &self.partial {
            match predicate.discriminator {
                PredicateDiscriminator::Concrete(predicate_type) => {
                    true.write(writer)?;
                    write_registry_id(predicate_type, writer, "component predicate type")?;
                }
                PredicateDiscriminator::Any(component) => {
                    false.write(writer)?;
                    write_registry_id(component, writer, "data component")?;
                }
            }
            let mut encoded = Vec::new();
            predicate.to_nbt_value().write(&mut encoded);
            writer.write_all(&encoded)?;
        }
        Ok(())
    }
}

impl ReadFrom for DataComponentMatchers {
    fn read(data: &mut Cursor<&[u8]>) -> Result<Self> {
        let exact = DataComponentExactPredicate::read(data)?;
        let count = read_len(data, 64, "partial component predicate")?;
        budget::check_collection_input(data, count, 3)?;
        let mut partial = budget::read_vec(count, count)?;
        for _ in 0..count {
            let discriminator = if bool::read(data)? {
                let id = read_registry_id(data, "component predicate type")?;
                PredicateDiscriminator::Concrete(
                    REGISTRY
                        .data_component_predicate_types
                        .by_id(id)
                        .ok_or_else(|| {
                            Error::other(format!("unknown component predicate type id: {id}"))
                        })?,
                )
            } else {
                PredicateDiscriminator::Any(read_component_entry(data)?)
            };
            let tag = read_network_nbt(data)?;
            let value = match discriminator {
                PredicateDiscriminator::Concrete(predicate_type) => Some(
                    (predicate_type.reader)(&tag)
                        .ok_or_else(|| Error::other("invalid component predicate payload"))?,
                ),
                PredicateDiscriminator::Any(_) => {
                    if tag.compound().is_none() {
                        return Err(Error::other(
                            "any-value predicate payload is not a compound",
                        ));
                    }
                    None
                }
            };
            partial.push(DataComponentPredicateData {
                discriminator,
                value,
            });
        }
        Self::new(exact, partial)
            .ok_or_else(|| Error::other("duplicate partial component predicate"))
    }
}

impl HashComponent for DataComponentMatchers {
    fn hash_component(&self, hasher: &mut ComponentHasher) {
        let mut entries = Vec::new();
        self.hash_fields(&mut entries);
        hash_entries(hasher, &mut entries);
    }
}

pub(super) fn write_registry_id(
    entry: &impl RegistryEntry,
    writer: &mut impl Write,
    name: &str,
) -> Result<()> {
    let id = entry
        .try_id()
        .ok_or_else(|| Error::other("Unknown registry discriminator"))?;
    let id = i32::try_from(id).map_err(|_| Error::other(format!("{name} id out of range")))?;
    VarInt(id).write(writer)
}

fn read_registry_id(data: &mut Cursor<&[u8]>, name: &str) -> Result<usize> {
    let id = VarInt::read(data)?.0;
    usize::try_from(id).map_err(|_| Error::other(format!("negative {name} id: {id}")))
}

fn read_component_entry(data: &mut Cursor<&[u8]>) -> Result<ComponentEntryRef> {
    let id = read_registry_id(data, "data component")?;
    REGISTRY
        .data_components
        .by_id(id)
        .ok_or_else(|| Error::other(format!("unknown data component id: {id}")))
}
impl DataComponentMatchers {
    pub(crate) fn write_bounded(&self, mut writer: &mut dyn NbtWrite) -> Result<()> {
        if self.partial.len() > 64 {
            return Err(Error::other("partial component predicate count exceeds 64"));
        }
        write_len(self.exact.values.len(), &mut writer)?;
        {
            let required = self
                .exact
                .values
                .len()
                .saturating_mul(size_of::<&(ComponentEntryRef, ComponentData)>());
            let mut writer = nbt_stream::ScratchWriter::new(writer, required)?;
            let mut entries = Vec::new();
            entries
                .try_reserve_exact(self.exact.values.len())
                .map_err(Error::other)?;
            entries.extend(self.exact.values.iter());
            entries.sort_unstable_by_key(|(entry, _)| {
                REGISTRY.data_components.id_from_key(&entry.key)
            });
            for (entry, value) in entries {
                write_registry_id(*entry, &mut writer, "data component")?;
                entry.write_network_in(value, &mut *writer)?;
            }
        }
        write_len(self.partial.len(), &mut writer)?;
        let required = self
            .partial
            .len()
            .saturating_mul(size_of::<&DataComponentPredicateData>());
        let mut writer = nbt_stream::ScratchWriter::new(writer, required)?;
        let mut partial = Vec::new();
        partial
            .try_reserve_exact(self.partial.len())
            .map_err(Error::other)?;
        partial.extend(self.partial.iter());
        partial.sort_unstable_by_key(|value| {
            (value.key().namespace.as_ref(), value.key().path.as_ref())
        });
        for predicate in partial {
            match predicate.discriminator {
                PredicateDiscriminator::Concrete(ty) => {
                    true.write(&mut writer)?;
                    write_registry_id(ty, &mut writer, "component predicate type")?;
                }
                PredicateDiscriminator::Any(component) => {
                    false.write(&mut writer)?;
                    write_registry_id(component, &mut writer, "data component")?;
                }
            }
            predicate.write_nbt_stream(&mut *writer, 0)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "persistent_union_tests.rs"]
mod persistent_union_tests;
