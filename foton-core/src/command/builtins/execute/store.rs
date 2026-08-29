//! `/execute store` result consumers.

use std::{error::Error, fmt, io::Cursor, sync::Arc};

use foton_utils::{
    Identifier,
    nbt::{NbtPath, NbtPathMutationError},
};
use simdnbt::{
    borrow::read_compound as read_borrowed_compound,
    owned::{NbtCompound, NbtTag},
};

use super::super::super::{
    brigadier::{ArgumentType, CommandNodeBuilder, CommandRedirectTarget, CommandSyntaxError},
    execution::{
        CommandResultCallback, CommandSource, ExecutionCommandSource as _, FotonArgumentType,
        FotonCommandContext, FotonCommandRuntime, ScoreHolderWildcard, argument, literal,
    },
};
use super::{
    condition::{invalid_block_data_source, loaded_block_position},
    objective, source_command_storage, source_scoreboard,
};
use crate::command::builtins::bossbar::boss_bar;
use crate::entity::{SharedEntity, nbt_load::load_live_entity};
use crate::scoreboard::{ScoreHolder, Scoreboard, ScoreboardError, ScoreboardObjective};
use crate::{block_entity::SharedBlockEntity, command::storage::CommandStorage};

type Builder = CommandNodeBuilder<CommandSource, FotonCommandRuntime>;

const EXECUTE_ROOT: CommandRedirectTarget = CommandRedirectTarget::CommandRoot;

pub(super) fn target(name: &'static str, store_result: bool) -> Builder {
    literal(name)
        .then(
            literal("score").then(
                argument("targets", FotonArgumentType::score_holders()).then(
                    argument("objective", FotonArgumentType::objective())
                        .redirects_with(EXECUTE_ROOT, move |context| {
                            store_score(context, store_result)
                        }),
                ),
            ),
        )
        .then(
            literal("bossbar").then(
                argument("id", FotonArgumentType::boss_bar_id())
                    .then(
                        literal("value").redirects_with(EXECUTE_ROOT, move |context| {
                            store_boss_bar(context, BossBarField::Value, store_result)
                        }),
                    )
                    .then(literal("max").redirects_with(EXECUTE_ROOT, move |context| {
                        store_boss_bar(context, BossBarField::Max, store_result)
                    })),
            ),
        )
        .then(
            literal("entity").then(
                argument("target", FotonArgumentType::entity())
                    .then(data_path(StoreDataTarget::Entity, store_result)),
            ),
        )
        .then(
            literal("block").then(
                argument("targetPos", FotonArgumentType::block_pos())
                    .then(data_path(StoreDataTarget::Block, store_result)),
            ),
        )
        .then(
            literal("storage").then(
                argument("target", FotonArgumentType::storage_key())
                    .then(data_path(StoreDataTarget::Storage, store_result)),
            ),
        )
}

fn data_path(target: StoreDataTarget, store_result: bool) -> Builder {
    argument("path", FotonArgumentType::nbt_path())
        .then(data_type("int", StoreDataType::Int, target, store_result))
        .then(data_type(
            "float",
            StoreDataType::Float,
            target,
            store_result,
        ))
        .then(data_type(
            "short",
            StoreDataType::Short,
            target,
            store_result,
        ))
        .then(data_type("long", StoreDataType::Long, target, store_result))
        .then(data_type(
            "double",
            StoreDataType::Double,
            target,
            store_result,
        ))
        .then(data_type("byte", StoreDataType::Byte, target, store_result))
}

fn data_type(
    name: &'static str,
    data_type: StoreDataType,
    target: StoreDataTarget,
    store_result: bool,
) -> Builder {
    literal(name).then(
        argument("scale", ArgumentType::double(f64::MIN, f64::MAX))
            .redirects_with(EXECUTE_ROOT, move |context| {
                store_data(context, target, data_type, store_result)
            }),
    )
}

#[derive(Clone, Copy)]
enum StoreDataTarget {
    Block,
    Entity,
    Storage,
}

#[derive(Clone, Copy, Debug)]
enum StoreDataType {
    Byte,
    Short,
    Int,
    Long,
    Float,
    Double,
}

impl StoreDataType {
    fn tag(self, value: i32, scale: f64) -> NbtTag {
        let scaled = f64::from(value) * scale;
        match self {
            Self::Byte => NbtTag::Byte((scaled as i32) as i8),
            Self::Short => NbtTag::Short((scaled as i32) as i16),
            Self::Int => NbtTag::Int(scaled as i32),
            Self::Long => NbtTag::Long(scaled as i64),
            Self::Float => NbtTag::Float(scaled as f32),
            Self::Double => NbtTag::Double(scaled),
        }
    }
}

fn store_data(
    context: &FotonCommandContext<CommandSource>,
    target: StoreDataTarget,
    data_type: StoreDataType,
    store_result: bool,
) -> Result<CommandSource, CommandSyntaxError> {
    match target {
        StoreDataTarget::Block => store_block_data(context, data_type, store_result),
        StoreDataTarget::Entity => store_entity_data(context, data_type, store_result),
        StoreDataTarget::Storage => store_storage_data(context, data_type, store_result),
    }
}

/// Which of a named bar's two numbers a store writes.
#[derive(Clone, Copy)]
enum BossBarField {
    Value,
    Max,
}

/// Vanilla parity: the `CustomBossEvent` overload of
/// `ExecuteCommand.storeValue`.
fn store_boss_bar(
    context: &FotonCommandContext<CommandSource>,
    field: BossBarField,
    store_result: bool,
) -> Result<CommandSource, CommandSyntaxError> {
    // Vanilla resolves the bar when the `store` clause is parsed, so naming a
    // bar that does not exist fails the whole command rather than running it
    // and dropping the result on the floor.
    let bar = boss_bar(context)?;
    let source = context.source();
    let callback = CommandResultCallback::new(move |success, result| {
        let value = stored_value(store_result, success, result);
        match field {
            BossBarField::Value => bar.set_value(value),
            BossBarField::Max => bar.set_max(value),
        }
    });
    let callback = CommandResultCallback::chain(source.callback(), callback);
    Ok(source.with_callback(callback))
}

fn store_entity_data(
    context: &FotonCommandContext<CommandSource>,
    data_type: StoreDataType,
    store_result: bool,
) -> Result<CommandSource, CommandSyntaxError> {
    // Vanilla resolves the accessor when the `store` clause is parsed, so a
    // selector that matches no entity fails the whole command rather than
    // failing quietly once the stored command has already run.
    let entity = context.entity("target")?;
    let path = parsed_path(context)?;
    let scale = parsed_scale(context)?;
    let source = context.source();
    let callback = CommandResultCallback::new(move |success, result| {
        // Vanilla parity: `EntityDataAccessor.setData` refuses a player, and
        // `ExecuteCommand.storeData` swallows that refusal. The store is a
        // no-op, not a command failure.
        if entity.as_player().is_some() {
            return;
        }
        let value = stored_value(store_result, success, result);
        if let Err(error) = store_entity_data_value(&entity, &path, data_type.tag(value, scale)) {
            tracing::warn!(%error, "failed to store execute result in entity data");
        }
    });
    let callback = CommandResultCallback::chain(source.callback(), callback);
    Ok(source.with_callback(callback))
}

fn store_block_data(
    context: &FotonCommandContext<CommandSource>,
    data_type: StoreDataType,
    store_result: bool,
) -> Result<CommandSource, CommandSyntaxError> {
    let position = loaded_block_position(context, "targetPos")?;
    let source = context.source();
    let block_entity = source
        .world()
        .get_block_entity(position)
        .ok_or_else(invalid_block_data_source)?;
    let path = parsed_path(context)?;
    let scale = parsed_scale(context)?;
    let world = Arc::clone(source.world());
    let callback = CommandResultCallback::new(move |success, result| {
        let value = stored_value(store_result, success, result);
        if store_block_data_value(&block_entity, &path, data_type.tag(value, scale)).is_ok() {
            world.send_block_updated(position);
        }
    });
    let callback = CommandResultCallback::chain(source.callback(), callback);
    Ok(source.with_callback(callback))
}

fn store_storage_data(
    context: &FotonCommandContext<CommandSource>,
    data_type: StoreDataType,
    store_result: bool,
) -> Result<CommandSource, CommandSyntaxError> {
    source_command_storage(context)?;
    let target = context.identifier("target")?.clone();
    let path = parsed_path(context)?;
    let scale = parsed_scale(context)?;
    let source = context.source();
    let server = Arc::clone(source.server());
    let domain = source.world().domain().to_owned();
    let callback = CommandResultCallback::new(move |success, result| {
        let Some(storage) = server.command_storage.get(&domain) else {
            return;
        };
        let value = stored_value(store_result, success, result);
        let _ = store_storage_data_value(storage, &target, &path, data_type.tag(value, scale));
    });
    let callback = CommandResultCallback::chain(source.callback(), callback);
    Ok(source.with_callback(callback))
}

fn parsed_path(
    context: &FotonCommandContext<CommandSource>,
) -> Result<NbtPath, CommandSyntaxError> {
    context.nbt_path("path").cloned()
}

fn parsed_scale(context: &FotonCommandContext<CommandSource>) -> Result<f64, CommandSyntaxError> {
    context.double("scale")
}

fn store_score(
    context: &FotonCommandContext<CommandSource>,
    store_result: bool,
) -> Result<CommandSource, CommandSyntaxError> {
    let scoreboard = source_scoreboard(context)?;
    let objective = objective(context, scoreboard, "objective")?;
    let holders = context.score_holders("targets", ScoreHolderWildcard::Tracked)?;
    let source = context.source();
    let server = Arc::clone(source.server());
    let domain = source.world().domain().to_owned();
    let callback = CommandResultCallback::new(move |success, result| {
        let Some(scoreboard) = server.scoreboards.get(&domain) else {
            tracing::warn!(%domain, "execute store score domain is no longer available");
            return;
        };
        let value = stored_value(store_result, success, result);
        if let Err(error) = store_score_value(scoreboard, &holders, &objective, value) {
            tracing::warn!(%error, "failed to store execute result in scoreboard");
        }
    });
    let callback = CommandResultCallback::chain(source.callback(), callback);
    Ok(source.with_callback(callback))
}

fn stored_value(store_result: bool, success: bool, result: i32) -> i32 {
    if store_result {
        result
    } else {
        i32::from(success)
    }
}

fn store_score_value(
    scoreboard: &Scoreboard,
    holders: &[ScoreHolder],
    objective: &ScoreboardObjective,
    value: i32,
) -> Result<(), ScoreboardError> {
    for holder in holders {
        scoreboard.set_score(holder, objective, value)?;
    }
    Ok(())
}

fn store_storage_data_value(
    storage: &CommandStorage,
    id: &Identifier,
    path: &NbtPath,
    value: NbtTag,
) -> Result<(), StoreDataMutationError> {
    let data = mutate_compound_path(storage.get(id), path, value)?;
    storage.set(id.clone(), data);
    Ok(())
}

fn store_block_data_value(
    block_entity: &SharedBlockEntity,
    path: &NbtPath,
    value: NbtTag,
) -> Result<(), StoreDataMutationError> {
    let data = mutate_compound_path(block_entity.save_with_full_metadata(), path, value)?;
    let mut bytes = Vec::new();
    data.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(bytes.as_slice()))
        .map_err(|_| StoreDataMutationError::InvalidWrittenNbt)?;
    block_entity.load_additional(&borrowed);
    block_entity.set_changed();
    Ok(())
}

fn store_entity_data_value(
    entity: &SharedEntity,
    path: &NbtPath,
    value: NbtTag,
) -> Result<(), StoreDataMutationError> {
    let data = mutate_compound_path(entity.nbt_for_data_compare(), path, value)?;
    let mut bytes = Vec::new();
    data.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(bytes.as_slice()))
        .map_err(|_| StoreDataMutationError::InvalidWrittenNbt)?;
    load_live_entity(entity.as_ref(), (&borrowed).into());
    Ok(())
}

fn mutate_compound_path(
    data: NbtCompound,
    path: &NbtPath,
    value: NbtTag,
) -> Result<NbtCompound, StoreDataMutationError> {
    let mut root = NbtTag::Compound(data);
    path.set(&mut root, value)
        .map_err(StoreDataMutationError::Path)?;
    match root {
        NbtTag::Compound(data) => Ok(data),
        _ => Err(StoreDataMutationError::ExpectedCompoundRoot),
    }
}

#[derive(Debug)]
enum StoreDataMutationError {
    Path(NbtPathMutationError),
    ExpectedCompoundRoot,
    InvalidWrittenNbt,
}

impl fmt::Display for StoreDataMutationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Path(error) => write!(formatter, "{error}"),
            Self::ExpectedCompoundRoot => write!(formatter, "NBT mutation replaced compound root"),
            Self::InvalidWrittenNbt => write!(formatter, "mutated NBT could not be reborrowed"),
        }
    }
}

impl Error for StoreDataMutationError {}

#[cfg(test)]
mod tests {
    use std::sync::Weak;

    use foton_registry::{init_vanilla_registry, vanilla_entities};
    use foton_utils::nbt::parse_nbt_path_argument;
    use glam::DVec3;
    use text_components::TextComponent;

    use super::*;
    use crate::entity::{ENTITIES, init_entities, next_entity_id};

    /// Builds one live slime with a name, a tag and a known amount of health.
    ///
    /// Everything here is deliberately not a default, so the store below can be
    /// asked what it left alone.
    fn dressed_slime() -> SharedEntity {
        init_vanilla_registry();
        init_entities();
        let Some(entity) = ENTITIES.create(
            &vanilla_entities::SLIME,
            next_entity_id(),
            DVec3::ZERO,
            Weak::new(),
        ) else {
            panic!("a slime should have an entity factory");
        };
        entity.set_custom_name(Some(TextComponent::from("Blob")));
        assert!(entity.add_tag("store_test".to_owned()));
        let Some(living) = entity.as_living_entity() else {
            panic!("a slime is a living entity");
        };
        living.set_health(7.0);
        entity
    }

    /// A store writes one value and has to leave the rest of the entity alone.
    ///
    /// The read-modify-write is the whole trick: a store that handed the entity
    /// a compound containing only the stored path would pass an assertion on
    /// `Air` and quietly wipe the name, the tag and the health with it.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "an exact value that survived an NBT round trip"
    )]
    fn storing_into_an_entity_replaces_only_the_path_it_was_given() {
        let entity = dressed_slime();
        let (path, _) = parse_nbt_path_argument("Air").expect("path should parse");
        assert_ne!(entity.air_supply(), 42);

        assert!(store_entity_data_value(&entity, &path, NbtTag::Short(42)).is_ok());

        assert_eq!(entity.air_supply(), 42);
        assert_eq!(entity.custom_name(), Some(TextComponent::from("Blob")));
        assert_eq!(entity.tags(), vec!["store_test".to_owned()]);
        let Some(living) = entity.as_living_entity() else {
            panic!("a slime is a living entity");
        };
        assert_eq!(living.get_health(), 7.0);
    }

    /// A path vanilla creates on the way down works here too: `data` is the one
    /// compound a datapack owns outright, and it starts out absent.
    #[test]
    fn storing_into_an_entity_creates_the_compounds_a_path_walks_through() {
        let entity = dressed_slime();
        assert!(entity.custom_data().is_empty());
        let (path, _) = parse_nbt_path_argument("data.counter").expect("path should parse");

        assert!(store_entity_data_value(&entity, &path, NbtTag::Int(9)).is_ok());

        assert_eq!(entity.custom_data().int("counter"), Some(9));
    }

    #[test]
    fn score_store_creates_and_updates_each_holder() {
        let scoreboard = Scoreboard::new();
        let Ok(objective) = scoreboard.add_objective("result") else {
            panic!("objective should be created");
        };
        let holders = [ScoreHolder::new("one"), ScoreHolder::new("two")];

        assert!(store_score_value(&scoreboard, &holders, &objective, 7).is_ok());
        assert_eq!(scoreboard.score(&holders[0], &objective), Some(7));
        assert_eq!(scoreboard.score(&holders[1], &objective), Some(7));
    }

    #[test]
    fn stored_value_distinguishes_numeric_results_from_success() {
        assert_eq!(stored_value(true, true, 17), 17);
        assert_eq!(stored_value(true, false, 0), 0);
        assert_eq!(stored_value(false, true, 17), 1);
        assert_eq!(stored_value(false, false, 17), 0);
    }

    #[test]
    fn data_types_match_java_numeric_narrowing() {
        assert_eq!(StoreDataType::Byte.tag(128, 1.0), NbtTag::Byte(-128));
        assert_eq!(StoreDataType::Byte.tag(i32::MAX, 1e20), NbtTag::Byte(-1));
        assert_eq!(StoreDataType::Byte.tag(i32::MIN, 1e20), NbtTag::Byte(0));
        assert_eq!(
            StoreDataType::Short.tag(32_768, 1.0),
            NbtTag::Short(-32_768)
        );
        assert_eq!(
            StoreDataType::Int.tag(i32::MAX, 1e20),
            NbtTag::Int(i32::MAX)
        );
        assert_eq!(
            StoreDataType::Long.tag(i32::MAX, 1e20),
            NbtTag::Long(i64::MAX)
        );
        assert_eq!(StoreDataType::Float.tag(3, 0.5), NbtTag::Float(1.5));
        assert_eq!(StoreDataType::Double.tag(3, 0.5), NbtTag::Double(1.5));
    }

    #[test]
    fn compound_path_mutation_creates_and_replaces_values() {
        let (path, _) = parse_nbt_path_argument("result.value").expect("path should parse");
        let data = mutate_compound_path(NbtCompound::new(), &path, NbtTag::Int(7))
            .expect("path should create missing compounds");
        let result = data
            .compound("result")
            .expect("result compound should exist");
        assert_eq!(result.int("value"), Some(7));

        let data = mutate_compound_path(data, &path, NbtTag::Byte(1))
            .expect("path should replace existing value");
        let result = data
            .compound("result")
            .expect("result compound should exist");
        assert_eq!(result.byte("value"), Some(1));
    }

    #[test]
    fn storage_mutation_reads_and_writes_the_current_compound() {
        let storage = CommandStorage::new();
        let key = Identifier::from_foton("store_test");
        let (first_path, _) = parse_nbt_path_argument("first").expect("first path should parse");
        let (second_path, _) = parse_nbt_path_argument("second").expect("second path should parse");

        assert!(store_storage_data_value(&storage, &key, &first_path, NbtTag::Int(4)).is_ok());
        assert!(
            store_storage_data_value(&storage, &key, &second_path, NbtTag::Double(2.5)).is_ok()
        );

        let stored = storage.get(&key);
        assert_eq!(stored.int("first"), Some(4));
        assert_eq!(stored.double("second"), Some(2.5));
    }
}
