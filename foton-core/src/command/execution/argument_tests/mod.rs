use crate::chunk::heightmap::HeightmapType;
use crate::command::{
    brigadier::{
        CommandDispatcher, CommandSyntaxError, CommandSyntaxErrorKind, StringReader, Suggestion,
    },
    execution::{
        BiomeOrTag, BlockPredicate, CommandArgumentSource, CommandPermissionSource,
        CommandResultCallback, Coordinates, ExecutionCommandSource, FotonArgumentType,
        FotonCommandRuntime, GameProfileArgument, ScoreHolderArgument, StructureOrTagKey,
        WorldArgument, argument,
        coordinates::{LocalCoordinates, WorldCoordinate, WorldCoordinates},
        literal,
    },
};
use foton_protocol::packets::game::{
    ArgumentType as ProtocolArgumentType, SuggestionType as ProtocolSuggestionType,
};
use foton_registry::{
    AxolotlVariant, DyeColor, FoxVariant, HorseVariant, ItemStackTemplate, LlamaVariant,
    MooshroomVariant, ParrotVariant, RabbitVariant, RegistryEntry as _, SalmonVariant,
    TropicalFishPattern,
    data_components::{ComponentPatchEntry, vanilla_components},
    init_vanilla_registry,
    item_stack::ItemStack,
    vanilla_attributes, vanilla_biomes, vanilla_blocks, vanilla_damage_types, vanilla_enchantments,
    vanilla_entities, vanilla_items, vanilla_world_clocks,
    world_clock::WorldClockRef,
};
use foton_utils::codec::VarInt;
use foton_utils::serial::{ReadFrom as _, WriteTo as _};
use foton_utils::{DowncastType, DowncastTypeKey, Identifier, types::GameType};
use glam::DVec3;
use simdnbt::owned::NbtCompound;
use std::io::Cursor;
use text_components::{TextComponent, content::Content};

use crate::bootstrap::init_globals_once;
use crate::entity::EntityAnchor;
use crate::permission::{PermissionExpr, PermissionState};

use super::argument::FotonArgumentParser;

struct TestSource {
    callback: CommandResultCallback,
}

impl TestSource {
    const fn new() -> Self {
        Self {
            callback: CommandResultCallback::empty(),
        }
    }
}

impl ExecutionCommandSource for TestSource {
    fn with_callback(&self, callback: CommandResultCallback) -> Self {
        Self { callback }
    }

    fn callback(&self) -> CommandResultCallback {
        self.callback.clone()
    }

    fn handle_error(&self, _error: &CommandSyntaxError, _forked: bool) {}
}

impl CommandArgumentSource for TestSource {
    fn default_world_clock(&self) -> Option<WorldClockRef> {
        Some(&vanilla_world_clocks::OVERWORLD)
    }

    fn domain_exists(&self, domain: &str) -> bool {
        matches!(domain, "alpha" | "beta")
    }

    fn domain_names(&self) -> Vec<&str> {
        vec!["alpha", "beta"]
    }

    fn command_world_names(&self) -> Vec<String> {
        [
            "alpha:overworld",
            "overworld",
            "alpha:arena",
            "arena",
            "beta:lobby",
        ]
        .map(str::to_owned)
        .to_vec()
    }

    fn permission_context_world_names(&self) -> Vec<String> {
        vec!["alpha:arena".to_owned(), "alpha:overworld".to_owned()]
    }

    fn command_storage_keys(&self) -> Vec<String> {
        vec!["minecraft:global".to_owned(), "foton:data".to_owned()]
    }

    fn permission_rule_suggestions(&self) -> Vec<String> {
        vec![
            "minecraft.command.gamemode".to_owned(),
            "foton.build{plugin:region=spawn}".to_owned(),
        ]
    }

    fn permission_metadata_suggestions(&self) -> Vec<String> {
        vec!["plugin:max_homes{plugin:region=spawn}".to_owned()]
    }

    fn user_permission_rule_suggestions(&self, _targets: &GameProfileArgument) -> Vec<String> {
        vec!["foton.user_owned".to_owned()]
    }

    fn user_permission_metadata_suggestions(&self, _targets: &GameProfileArgument) -> Vec<String> {
        vec!["plugin:user_owned".to_owned()]
    }

    fn group_permission_rule_suggestions(&self, group: &str) -> Vec<String> {
        if group == "builder" {
            vec!["foton.group_owned".to_owned()]
        } else {
            Vec::new()
        }
    }

    fn group_permission_metadata_suggestions(&self, group: &str) -> Vec<String> {
        if group == "builder" {
            vec!["plugin:group_owned".to_owned()]
        } else {
            Vec::new()
        }
    }

    fn permission_group_names(&self) -> Vec<String> {
        vec!["builder".to_owned(), "default".to_owned()]
    }

    fn selector_player_names(&self) -> Vec<String> {
        vec!["Steve".to_owned()]
    }

    fn scoreboard_objective_names(&self) -> Vec<String> {
        vec!["kills".to_owned(), "points".to_owned()]
    }

    fn allows_entity_selectors(&self) -> bool {
        true
    }

    fn allows_advanced_entity_selectors(&self) -> bool {
        true
    }
}

impl CommandPermissionSource for TestSource {
    fn permission_state(&self, _permission: &PermissionExpr) -> Option<PermissionState> {
        Some(PermissionState::Allow)
    }
}

type TestDispatcher = CommandDispatcher<TestSource, FotonCommandRuntime>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ExtensionParser;

fn resource_dispatcher(argument_type: FotonArgumentType) -> TestDispatcher {
    let mut dispatcher = TestDispatcher::new();
    let command = literal("resource").then(argument("value", argument_type).executes(|_| Ok(1)));
    assert!(dispatcher.register(command).is_ok());
    dispatcher
}

mod coordinates;
mod core_permissions;
mod general;
mod item_predicate;
mod item_stack;
mod resources_world;
mod selector_time;
