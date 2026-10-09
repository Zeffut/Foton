//! Vanilla data command.
//!
//! Vanilla parity: `DataCommands` and its three accessors (`EntityDataAccessor`,
//! `BlockDataAccessor`, `StorageDataAccessor`).

use std::{io::Cursor, iter, sync::Arc};

use foton_utils::{
    BlockPos, Identifier,
    nbt::{
        NbtPath, NbtPathMutationError, merge_nbt_compounds, nbt_collection_values,
        nbt_compounds_equal, to_canonical_snbt,
    },
    text::command_nbt_component,
    translations,
};
use simdnbt::{
    borrow::{BaseNbtCompound as BorrowedNbtCompound, read_compound},
    owned::{NbtCompound, NbtTag},
};
use text_components::TextComponent;

use super::{
    super::{
        brigadier::{ArgumentType, CommandNodeBuilder, CommandSyntaxError},
        execution::{
            CommandSource, FotonArgumentType, FotonCommandContext, FotonCommandRuntime, argument,
            literal,
        },
        registration::CommandRegistration,
        storage::CommandStorage,
    },
    fill::loaded_block_pos,
};
use crate::{
    block_entity::SharedBlockEntity,
    entity::{SharedEntity, nbt_load::load_live_entity},
    world::World,
};

type Builder = CommandNodeBuilder<CommandSource, FotonCommandRuntime>;
type CommandResult = Result<i32, CommandSyntaxError>;

/// Vanilla's `DataCommands.DataProvider`: where a `/data` argument points.
#[derive(Clone, Copy)]
enum Provider {
    Entity,
    Block,
    Storage,
}

const PROVIDERS: [Provider; 3] = [Provider::Entity, Provider::Block, Provider::Storage];

/// Whether a provider is the command's `target` or, for `modify`, its `source`.
#[derive(Clone, Copy)]
enum Role {
    Target,
    Source,
}

impl Role {
    const fn prefix(self) -> &'static str {
        match self {
            Self::Target => "target",
            Self::Source => "source",
        }
    }

    const fn pos_name(self) -> &'static str {
        match self {
            Self::Target => "targetPos",
            Self::Source => "sourcePos",
        }
    }
}

impl Provider {
    /// Vanilla parity: `DataProvider.wrap`. `build` receives the provider's
    /// final argument node, the one an executor or further children hang on.
    fn wrap(self, role: Role, build: impl FnOnce(Builder) -> Builder) -> Builder {
        match self {
            Self::Entity => {
                literal("entity").then(build(argument(role.prefix(), FotonArgumentType::entity())))
            }
            Self::Block => literal("block").then(build(argument(
                role.pos_name(),
                FotonArgumentType::block_pos(),
            ))),
            Self::Storage => literal("storage").then(build(argument(
                role.prefix(),
                FotonArgumentType::storage_key(),
            ))),
        }
    }

    /// Vanilla parity: `DataProvider.access`.
    fn access(
        self,
        context: &FotonCommandContext<CommandSource>,
        role: Role,
    ) -> Result<DataAccessor<'_>, CommandSyntaxError> {
        match self {
            Self::Entity => Ok(DataAccessor::Entity(context.entity(role.prefix())?)),
            Self::Block => {
                let pos = loaded_block_pos(context, role.pos_name())?;
                let world = context.source().world();
                let entity = world.get_block_entity(pos).ok_or_else(|| {
                    CommandSyntaxError::dynamic(TextComponent::from(
                        &translations::COMMANDS_DATA_BLOCK_INVALID,
                    ))
                })?;
                Ok(DataAccessor::Block { entity, world, pos })
            }
            Self::Storage => {
                let source = context.source();
                let storage = source
                    .server()
                    .command_storage
                    .get(source.world().domain())
                    .ok_or_else(|| {
                        CommandSyntaxError::dynamic(format!(
                            "Domain '{}' has no command storage",
                            source.world().domain()
                        ))
                    })?;
                Ok(DataAccessor::Storage {
                    storage,
                    id: context.identifier(role.prefix())?.clone(),
                })
            }
        }
    }
}

/// Vanilla's `DataAccessor`: one NBT compound that can be read and written back.
enum DataAccessor<'a> {
    Entity(SharedEntity),
    Block {
        entity: SharedBlockEntity,
        world: &'a Arc<World>,
        pos: BlockPos,
    },
    Storage {
        storage: &'a CommandStorage,
        id: Identifier,
    },
}

impl DataAccessor<'_> {
    fn data(&self) -> NbtCompound {
        match self {
            Self::Entity(entity) => entity.nbt_for_data_compare(),
            Self::Block { entity, .. } => entity.save_with_full_metadata(),
            Self::Storage { storage, id } => storage.get(id),
        }
    }

    fn set_data(&self, data: NbtCompound) -> Result<(), CommandSyntaxError> {
        match self {
            Self::Entity(entity) => {
                if entity.as_player().is_some() {
                    return Err(CommandSyntaxError::dynamic(TextComponent::from(
                        &translations::COMMANDS_DATA_ENTITY_INVALID,
                    )));
                }
                with_borrowed(&data, |borrowed| {
                    load_live_entity(entity.as_ref(), borrowed.into());
                })
            }
            Self::Block { entity, world, pos } => {
                with_borrowed(&data, |borrowed| entity.load_with_components(borrowed))?;
                entity.set_changed();
                world.send_block_updated(*pos);
                Ok(())
            }
            Self::Storage { storage, id } => {
                storage.set(id.clone(), data);
                Ok(())
            }
        }
    }

    fn modified_success(&self) -> TextComponent {
        match self {
            Self::Entity(entity) => translations::COMMANDS_DATA_ENTITY_MODIFIED
                .message([TextComponent::plain(entity.plain_text_name())])
                .component(),
            Self::Block { pos, .. } => translations::COMMANDS_DATA_BLOCK_MODIFIED
                .message(block_coordinates(*pos))
                .component(),
            Self::Storage { id, .. } => translations::COMMANDS_DATA_STORAGE_MODIFIED
                .message([TextComponent::plain(id.to_string())])
                .component(),
        }
    }

    fn print_success(&self, data: &NbtTag) -> TextComponent {
        let pretty = command_nbt_component(data, false);
        match self {
            Self::Entity(entity) => translations::COMMANDS_DATA_ENTITY_QUERY
                .message([TextComponent::plain(entity.plain_text_name()), pretty])
                .component(),
            Self::Block { pos, .. } => {
                let [x, y, z] = block_coordinates(*pos);
                translations::COMMANDS_DATA_BLOCK_QUERY
                    .message([x, y, z, pretty])
                    .component()
            }
            Self::Storage { id, .. } => translations::COMMANDS_DATA_STORAGE_QUERY
                .message([TextComponent::plain(id.to_string()), pretty])
                .component(),
        }
    }

    fn print_success_scaled(&self, path: &NbtPath, scale: f64, value: i32) -> TextComponent {
        let path = TextComponent::plain(path.as_str().to_owned());
        let scale = TextComponent::plain(format_scale(scale));
        let value = TextComponent::plain(value.to_string());
        match self {
            Self::Entity(entity) => translations::COMMANDS_DATA_ENTITY_GET
                .message([
                    path,
                    TextComponent::plain(entity.plain_text_name()),
                    scale,
                    value,
                ])
                .component(),
            Self::Block { pos, .. } => {
                let [x, y, z] = block_coordinates(*pos);
                translations::COMMANDS_DATA_BLOCK_GET
                    .message([path, x, y, z, scale, value])
                    .component()
            }
            Self::Storage { id, .. } => translations::COMMANDS_DATA_STORAGE_GET
                .message([path, TextComponent::plain(id.to_string()), scale, value])
                .component(),
        }
    }
}

fn block_coordinates(pos: BlockPos) -> [TextComponent; 3] {
    [
        TextComponent::plain(pos.x().to_string()),
        TextComponent::plain(pos.y().to_string()),
        TextComponent::plain(pos.z().to_string()),
    ]
}

/// Runs `apply` on `data` re-read as borrowed NBT, which is what the entity
/// and block-entity loaders take.
fn with_borrowed<T>(
    data: &NbtCompound,
    apply: impl FnOnce(&BorrowedNbtCompound<'_>) -> T,
) -> Result<T, CommandSyntaxError> {
    let mut bytes = Vec::new();
    data.write(&mut bytes);
    let borrowed = read_compound(&mut Cursor::new(bytes.as_slice()))
        .map_err(|_| CommandSyntaxError::dynamic("Modified NBT could not be re-read"))?;
    Ok(apply(&borrowed))
}

/// Java's `String.format(Locale.ROOT, "%.2f", scale)`. Java rounds the shortest
/// decimal that identifies the double, half up -- so `1.005` prints `1.01`.
fn format_scale(scale: f64) -> String {
    if scale.is_nan() {
        return "NaN".to_owned();
    }
    if scale.is_infinite() {
        return if scale < 0.0 { "-Infinity" } else { "Infinity" }.to_owned();
    }

    let shortest = format!("{}", scale.abs());
    let (whole, fraction) = shortest.split_once('.').unwrap_or((&shortest, ""));
    let mut digits: Vec<u8> = whole.bytes().chain(fraction.bytes().take(2)).collect();
    let fraction_len = fraction.len().min(2);
    digits.extend(iter::repeat_n(b'0', 2 - fraction_len));
    if fraction
        .as_bytes()
        .get(2)
        .is_some_and(|digit| *digit >= b'5')
    {
        let mut index = digits.len();
        loop {
            if index == 0 {
                digits.insert(0, b'1');
                break;
            }
            index -= 1;
            if digits[index] == b'9' {
                digits[index] = b'0';
            } else {
                digits[index] += 1;
                break;
            }
        }
    }
    let split = digits.len() - 2;
    let (whole, fraction) = digits.split_at(split);
    let sign = if scale.is_sign_negative() { "-" } else { "" };
    format!(
        "{sign}{}.{}",
        String::from_utf8_lossy(whole),
        String::from_utf8_lossy(fraction)
    )
}

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("data"), |_| command())
}

fn command() -> Builder {
    let mut merge = literal("merge");
    let mut get = literal("get");
    let mut remove = literal("remove");
    for provider in PROVIDERS {
        merge = merge.then(provider.wrap(Role::Target, |node| {
            node.then(
                argument("nbt", FotonArgumentType::nbt_compound_tag())
                    .executes(move |c| merge_data(c, provider)),
            )
        }));
        get = get.then(provider.wrap(Role::Target, |node| {
            node.executes(move |c| get_data(c, provider)).then(
                argument("path", FotonArgumentType::nbt_path())
                    .executes(move |c| get_path(c, provider))
                    .then(
                        argument("scale", ArgumentType::double(f64::MIN, f64::MAX))
                            .executes(move |c| get_numeric(c, provider)),
                    ),
            )
        }));
        remove = remove.then(provider.wrap(Role::Target, |node| {
            node.then(
                argument("path", FotonArgumentType::nbt_path())
                    .executes(move |c| remove_data(c, provider)),
            )
        }));
    }
    literal("data")
        .then(merge)
        .then(get)
        .then(remove)
        .then(modify())
}

/// How `modify` combines the source tags with the target path.
#[derive(Clone, Copy)]
enum Manipulation {
    Insert,
    Prepend,
    Append,
    Set,
    Merge,
}

/// Where the tags a `modify` applies come from.
#[derive(Clone, Copy)]
enum SourceKind {
    /// `from <provider> [<sourcePath>]`
    From(Provider),
    /// `string <provider> [<sourcePath> [<start> [<end>]]]`
    Str(Provider),
    /// `value <value>`
    Value,
}

/// Vanilla parity: `DataCommands.decorateModification`.
fn modify() -> Builder {
    let mut modify = literal("modify");
    for target in PROVIDERS {
        modify = modify.then(target.wrap(Role::Target, |node| {
            node.then(
                argument("targetPath", FotonArgumentType::nbt_path())
                    .then(literal("insert").then(with_sources(
                        argument("index", ArgumentType::integer(i32::MIN, i32::MAX)),
                        target,
                        Manipulation::Insert,
                    )))
                    .then(with_sources(
                        literal("prepend"),
                        target,
                        Manipulation::Prepend,
                    ))
                    .then(with_sources(
                        literal("append"),
                        target,
                        Manipulation::Append,
                    ))
                    .then(with_sources(literal("set"), target, Manipulation::Set))
                    .then(with_sources(literal("merge"), target, Manipulation::Merge)),
            )
        }));
    }
    modify
}

/// Hangs the `from` / `string` / `value` branches of one manipulation on `parent`.
fn with_sources(parent: Builder, target: Provider, manipulation: Manipulation) -> Builder {
    let mut from = literal("from");
    let mut string = literal("string");
    for source in PROVIDERS {
        from = from.then(source.wrap(Role::Source, |node| {
            node.executes(move |c| {
                modify_data(c, target, manipulation, SourceKind::From(source), false)
            })
            .then(
                argument("sourcePath", FotonArgumentType::nbt_path()).executes(move |c| {
                    modify_data(c, target, manipulation, SourceKind::From(source), true)
                }),
            )
        }));
        string = string.then(source.wrap(Role::Source, |node| {
            node.executes(move |c| {
                modify_data(c, target, manipulation, SourceKind::Str(source), false)
            })
            .then(
                argument("sourcePath", FotonArgumentType::nbt_path())
                    .executes(move |c| {
                        modify_data(c, target, manipulation, SourceKind::Str(source), true)
                    })
                    .then(
                        argument("start", ArgumentType::integer(i32::MIN, i32::MAX))
                            .executes(move |c| {
                                modify_data(c, target, manipulation, SourceKind::Str(source), true)
                            })
                            .then(
                                argument("end", ArgumentType::integer(i32::MIN, i32::MAX))
                                    .executes(move |c| {
                                        modify_data(
                                            c,
                                            target,
                                            manipulation,
                                            SourceKind::Str(source),
                                            true,
                                        )
                                    }),
                            ),
                    ),
            )
        }));
    }
    let value = literal("value").then(
        argument("value", FotonArgumentType::nbt_tag())
            .executes(move |c| modify_data(c, target, manipulation, SourceKind::Value, false)),
    );
    parent.then(from).then(string).then(value)
}

fn mutation_error(error: &NbtPathMutationError) -> CommandSyntaxError {
    CommandSyntaxError::dynamic(error.component())
}

fn merge_unchanged() -> CommandSyntaxError {
    CommandSyntaxError::dynamic(TextComponent::from(
        &translations::COMMANDS_DATA_MERGE_FAILED,
    ))
}

/// Vanilla parity: `DataCommands.mergeData`.
fn merge_data(context: &FotonCommandContext<CommandSource>, provider: Provider) -> CommandResult {
    let accessor = provider.access(context, Role::Target)?;
    let nbt = context.nbt_compound("nbt")?;
    let old = accessor.data();
    if NbtPath::is_too_deep(&NbtTag::Compound(nbt.clone()), 0) {
        return Err(mutation_error(&NbtPathMutationError::TooDeep));
    }
    let mut result = old.clone();
    merge_nbt_compounds(&mut result, nbt);
    if nbt_compounds_equal(&old, &result) {
        return Err(merge_unchanged());
    }
    accessor.set_data(result)?;
    context
        .source()
        .send_success(&accessor.modified_success(), true);
    Ok(1)
}

/// Vanilla parity: `DataCommands.getData(source, accessor)`.
fn get_data(context: &FotonCommandContext<CommandSource>, provider: Provider) -> CommandResult {
    let accessor = provider.access(context, Role::Target)?;
    let data = NbtTag::Compound(accessor.data());
    context
        .source()
        .send_success(&accessor.print_success(&data), false);
    Ok(1)
}

/// Vanilla parity: `DataCommands.getSingleTag`.
fn single_tag(path: &NbtPath, accessor: &DataAccessor<'_>) -> Result<NbtTag, CommandSyntaxError> {
    let mut tags = path
        .try_get(&NbtTag::Compound(accessor.data()))
        .map_err(|error| mutation_error(&error))?
        .into_iter();
    let (Some(tag), None) = (tags.next(), tags.next()) else {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            &translations::COMMANDS_DATA_GET_MULTIPLE,
        )));
    };
    Ok(tag)
}

/// Vanilla parity: `DataCommands.getData(source, accessor, path)`.
fn get_path(context: &FotonCommandContext<CommandSource>, provider: Provider) -> CommandResult {
    let accessor = provider.access(context, Role::Target)?;
    let path = context.nbt_path("path")?;
    let tag = single_tag(path, &accessor)?;

    let result = match &tag {
        NbtTag::Byte(value) => floor(f64::from(*value)),
        NbtTag::Short(value) => floor(f64::from(*value)),
        NbtTag::Int(value) => floor(f64::from(*value)),
        #[expect(
            clippy::cast_precision_loss,
            reason = "vanilla reads a long through `doubleValue()`"
        )]
        NbtTag::Long(value) => floor(*value as f64),
        NbtTag::Float(value) => floor(f64::from(*value)),
        NbtTag::Double(value) => floor(*value),
        NbtTag::String(value) => {
            // Java's `String.length()` counts UTF-16 code units.
            let units = value.to_string().encode_utf16().count();
            i32::try_from(units).unwrap_or(i32::MAX)
        }
        NbtTag::Compound(compound) => i32::try_from(compound.iter().count()).unwrap_or(i32::MAX),
        collection => {
            let len = nbt_collection_values(collection).map_or(0, |v| v.len());
            i32::try_from(len).unwrap_or(i32::MAX)
        }
    };
    context
        .source()
        .send_success(&accessor.print_success(&tag), false);
    Ok(result)
}

/// Vanilla parity: `DataCommands.getNumeric`.
fn get_numeric(context: &FotonCommandContext<CommandSource>, provider: Provider) -> CommandResult {
    let accessor = provider.access(context, Role::Target)?;
    let path = context.nbt_path("path")?;
    let scale = context.double("scale")?;
    let tag = single_tag(path, &accessor)?;
    let Some(value) = numeric_value(&tag) else {
        return Err(CommandSyntaxError::dynamic(
            translations::COMMANDS_DATA_GET_INVALID
                .message([path.as_str().to_owned()])
                .component(),
        ));
    };
    let result = floor(value * scale);
    context
        .source()
        .send_success(&accessor.print_success_scaled(path, scale, result), false);
    Ok(result)
}

/// Vanilla parity: `NumericTag.doubleValue`.
fn numeric_value(tag: &NbtTag) -> Option<f64> {
    #[expect(
        clippy::cast_precision_loss,
        reason = "vanilla reads a long through `doubleValue()`"
    )]
    Some(match tag {
        NbtTag::Byte(value) => f64::from(*value),
        NbtTag::Short(value) => f64::from(*value),
        NbtTag::Int(value) => f64::from(*value),
        NbtTag::Long(value) => *value as f64,
        NbtTag::Float(value) => f64::from(*value),
        NbtTag::Double(value) => *value,
        _ => return None,
    })
}

/// Vanilla parity: `Mth.floor(double)`, including its wrap at `i32::MIN`.
#[expect(
    clippy::cast_possible_truncation,
    reason = "Mth.floor narrows with a Java (int) cast, which saturates like `as`"
)]
fn floor(value: f64) -> i32 {
    let truncated = value as i32;
    if value < f64::from(truncated) {
        truncated.wrapping_sub(1)
    } else {
        truncated
    }
}

/// Vanilla parity: `DataCommands.removeData`.
fn remove_data(context: &FotonCommandContext<CommandSource>, provider: Provider) -> CommandResult {
    let accessor = provider.access(context, Role::Target)?;
    let path = context.nbt_path("path")?;
    let mut data = NbtTag::Compound(accessor.data());
    let count = path.remove(&mut data);
    if count == 0 {
        return Err(merge_unchanged());
    }
    let NbtTag::Compound(data) = data else {
        return Err(merge_unchanged());
    };
    accessor.set_data(data)?;
    context
        .source()
        .send_success(&accessor.modified_success(), true);
    i32::try_from(count).map_err(|_| {
        CommandSyntaxError::dynamic("Removed tag count exceeds the command result range")
    })
}

/// Vanilla parity: `DataCommands.manipulateData`, with the source tags
/// resolved first as vanilla's argument evaluation order does.
fn modify_data(
    context: &FotonCommandContext<CommandSource>,
    target: Provider,
    manipulation: Manipulation,
    source: SourceKind,
    has_source_path: bool,
) -> CommandResult {
    let tags = source_tags(context, source, has_source_path)?;

    let accessor = target.access(context, Role::Target)?;
    let path = context.nbt_path("targetPath")?;
    let mut data = NbtTag::Compound(accessor.data());
    let changed = match manipulation {
        Manipulation::Insert => path.insert(context.integer("index")?, &mut data, &tags),
        Manipulation::Prepend => path.insert(0, &mut data, &tags),
        Manipulation::Append => path.insert(-1, &mut data, &tags),
        Manipulation::Set => {
            let Some(last) = tags.last() else {
                return Err(merge_unchanged());
            };
            path.set(&mut data, last.clone())
        }
        Manipulation::Merge => {
            combined_sources(&tags).and_then(|combined| path.merge_compound(&mut data, &combined))
        }
    }
    .map_err(|error| mutation_error(&error))?;

    if changed == 0 {
        return Err(merge_unchanged());
    }
    let NbtTag::Compound(data) = data else {
        return Err(merge_unchanged());
    };
    accessor.set_data(data)?;
    context
        .source()
        .send_success(&accessor.modified_success(), true);
    i32::try_from(changed).map_err(|_| {
        CommandSyntaxError::dynamic("Changed tag count exceeds the command result range")
    })
}

/// The `merge` manipulation folds every source into one compound first.
fn combined_sources(tags: &[NbtTag]) -> Result<NbtCompound, NbtPathMutationError> {
    let mut combined = NbtCompound::new();
    for tag in tags {
        if NbtPath::is_too_deep(tag, 0) {
            return Err(NbtPathMutationError::TooDeep);
        }
        let NbtTag::Compound(compound) = tag else {
            return Err(NbtPathMutationError::ExpectedObject(
                to_canonical_snbt(tag).unwrap_or_default(),
            ));
        };
        merge_nbt_compounds(&mut combined, compound);
    }
    Ok(combined)
}

fn source_tags(
    context: &FotonCommandContext<CommandSource>,
    source: SourceKind,
    has_source_path: bool,
) -> Result<Vec<NbtTag>, CommandSyntaxError> {
    let (provider, stringify) = match source {
        SourceKind::Value => return Ok(vec![context.nbt_tag("value")?.clone()]),
        SourceKind::From(provider) => (provider, false),
        SourceKind::Str(provider) => (provider, true),
    };

    let accessor = provider.access(context, Role::Source)?;
    let data = NbtTag::Compound(accessor.data());
    let tags = if has_source_path {
        context
            .nbt_path("sourcePath")?
            .try_get(&data)
            .map_err(|error| mutation_error(&error))?
    } else {
        vec![data]
    };
    if !stringify {
        return Ok(tags);
    }

    let start = context.integer("start").ok();
    let end = context.integer("end").ok();
    tags.iter()
        .map(|tag| {
            let text = tag_text(tag)?;
            let text = match (start, end) {
                (Some(start), Some(end)) => substring(&text, start, Some(end))?,
                (Some(start), None) => substring(&text, start, None)?,
                _ => text,
            };
            Ok(NbtTag::String(text.into()))
        })
        .collect()
}

/// Vanilla parity: `DataCommands.getAsText`.
fn tag_text(tag: &NbtTag) -> Result<String, CommandSyntaxError> {
    match tag {
        NbtTag::String(value) => Ok(value.to_string()),
        NbtTag::Byte(_)
        | NbtTag::Short(_)
        | NbtTag::Int(_)
        | NbtTag::Long(_)
        | NbtTag::Float(_)
        | NbtTag::Double(_) => Ok(to_canonical_snbt(tag).unwrap_or_default()),
        _ => Err(CommandSyntaxError::dynamic(
            translations::COMMANDS_DATA_MODIFY_EXPECTED_VALUE
                .message([to_canonical_snbt(tag).unwrap_or_default()])
                .component(),
        )),
    }
}

/// Vanilla parity: `DataCommands.substring`, over Java's UTF-16 indexing.
fn substring(input: &str, start: i32, end: Option<i32>) -> Result<String, CommandSyntaxError> {
    let units: Vec<u16> = input.encode_utf16().collect();
    let length = i64::try_from(units.len()).unwrap_or(i64::MAX);
    let absolute = |index: i32| {
        if index >= 0 {
            i64::from(index)
        } else {
            length + i64::from(index)
        }
    };
    let (start, end) = (absolute(start), end.map_or(length, absolute));
    if start < 0 || end > length || start > end {
        return Err(CommandSyntaxError::dynamic(
            translations::COMMANDS_DATA_MODIFY_INVALID_SUBSTRING
                .message([start.to_string(), end.to_string()])
                .component(),
        ));
    }
    // Both bounds are within `0..=length` here.
    let slice = &units[start as usize..end as usize];
    Ok(String::from_utf16_lossy(slice))
}

#[cfg(test)]
mod tests {
    use super::{floor, format_scale, substring, tag_text};
    use simdnbt::owned::{NbtList, NbtTag};

    #[test]
    fn scale_is_formatted_like_java_half_up_on_the_shortest_decimal() {
        // C would print 1.00 for 1.005 and 0.12 for 0.125; Java prints 1.01 and 0.13.
        assert_eq!(format_scale(1.005), "1.01");
        assert_eq!(format_scale(0.125), "0.13");
        assert_eq!(format_scale(1.0), "1.00");
        assert_eq!(format_scale(0.1), "0.10");
        assert_eq!(format_scale(9.999), "10.00");
        assert_eq!(format_scale(-2.5), "-2.50");
        assert_eq!(format_scale(f64::NAN), "NaN");
        assert_eq!(format_scale(f64::NEG_INFINITY), "-Infinity");
    }

    #[test]
    fn substring_indices_count_back_from_the_end_and_must_be_ordered() {
        assert_eq!(substring("hello", 1, Some(3)).as_deref().ok(), Some("el"));
        assert_eq!(substring("hello", -3, None).as_deref().ok(), Some("llo"));
        assert_eq!(
            substring("hello", 0, Some(-1)).as_deref().ok(),
            Some("hell")
        );
        assert!(substring("hello", 3, Some(1)).is_err());
        assert!(substring("hello", 0, Some(6)).is_err());
        assert!(substring("hello", -6, None).is_err());
        // Java strings are UTF-16: an emoji is two units, not one.
        assert_eq!(substring("a😀b", 3, None).as_deref().ok(), Some("b"));
    }

    #[test]
    fn only_strings_and_numbers_can_become_text() {
        assert_eq!(
            tag_text(&NbtTag::String("x".into())).ok().as_deref(),
            Some("x")
        );
        assert_eq!(tag_text(&NbtTag::Byte(5)).ok().as_deref(), Some("5b"));
        assert_eq!(tag_text(&NbtTag::Int(5)).ok().as_deref(), Some("5"));
        assert!(tag_text(&NbtTag::List(NbtList::default())).is_err());
    }

    #[test]
    fn floor_rounds_toward_negative_infinity() {
        assert_eq!(floor(2.9), 2);
        assert_eq!(floor(-2.1), -3);
        assert_eq!(floor(f64::NAN), 0);
    }
}
