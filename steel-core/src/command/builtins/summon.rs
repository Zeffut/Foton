//! Entity summoning command.

use std::io::Cursor;
use std::sync::Arc;

use glam::DVec3;
use simdnbt::borrow::read_compound as read_borrowed_compound;
use simdnbt::owned::NbtCompound;
use steel_registry::entity_type::EntityTypeRef;
use steel_utils::{BlockPos, Identifier, translations, types::Difficulty};
use text_components::{TextComponent, translation::Translation};
use uuid::Uuid;

use super::super::{
    brigadier::{CommandNodeBuilder, CommandSyntaxError},
    execution::{
        CommandSource, SteelArgumentType, SteelCommandContext, SteelCommandRuntime, argument,
        literal,
    },
    registration::CommandRegistration,
};
use crate::{
    entity::{
        AddEntityError, ENTITIES, EntityBase, EntityLoadRequest, EntitySpawnReason, SharedEntity,
        nbt_load::read_entity_nbt,
    },
    world::World,
};

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("summon"), |_| command())
}

fn command() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    literal("summon").then(
        argument("entity", SteelArgumentType::summonable_entity())
            .executes(|context| {
                summon_entity(
                    context,
                    context.source().position(),
                    &NbtCompound::new(),
                    true,
                )
            })
            .then(
                argument("pos", SteelArgumentType::vec3(true))
                    .executes(|context| {
                        let position = context.coordinates("pos")?;
                        summon_entity(
                            context,
                            position.position(context.source()),
                            &NbtCompound::new(),
                            true,
                        )
                    })
                    // Vanilla hangs the compound off `pos`, not off `entity`,
                    // so `/summon pig {...}` without a position is not a thing.
                    .then(
                        argument("nbt", SteelArgumentType::nbt_compound_tag()).executes(
                            |context| {
                                let position = context.coordinates("pos")?;
                                let nbt = context.nbt_compound("nbt")?.clone();
                                summon_entity(
                                    context,
                                    position.position(context.source()),
                                    &nbt,
                                    false,
                                )
                            },
                        ),
                    ),
            ),
    )
}

fn summon_entity(
    context: &SteelCommandContext<CommandSource>,
    position: DVec3,
    nbt: &NbtCompound,
    finalize: bool,
) -> Result<i32, CommandSyntaxError> {
    let entity_type = context.entity_type("entity")?;
    let entity = create_entity(context, entity_type, position, nbt, finalize)?;
    let message = translations::COMMANDS_SUMMON_SUCCESS
        .message([entity.display_name()])
        .component();
    context.source().send_success(&message, true);
    Ok(1)
}

/// Builds one entity and everything riding it, then puts the tree in the world.
///
/// Vanilla parity: `SummonCommand.createEntity`. `finalize` is vanilla's own
/// flag and is false for the NBT branch even when the compound is empty:
/// `/summon zombie ~ ~ ~ {}` gets no equipment, no random armour and no baby
/// roll, because which node executed is what decides, not what the compound
/// happens to contain.
pub(super) fn create_entity(
    context: &SteelCommandContext<CommandSource>,
    entity_type: EntityTypeRef,
    position: DVec3,
    nbt: &NbtCompound,
    finalize: bool,
) -> Result<SharedEntity, CommandSyntaxError> {
    if !World::is_in_spawnable_bounds(BlockPos::from(position)) {
        return Err(command_failed(
            &translations::COMMANDS_SUMMON_INVALID_POSITION,
        ));
    }

    let world = context.source().world();
    if world.difficulty() == Difficulty::Peaceful && !entity_type.allowed_in_peaceful {
        return Err(command_failed(
            &translations::COMMANDS_SUMMON_FAILED_PEACEFUL,
        ));
    }

    // Vanilla parity: `entityTag.putString("id", ...)` on a *copy*. The
    // argument-supplied type wins over any `id` the caller wrote, and the
    // caller's compound is not modified.
    //
    // The removal is not decoration. `NbtCompound::insert` appends instead of
    // replacing and the readers return the first match, so writing `id` over a
    // compound that already has one leaves the caller's in front:
    // `/summon minecraft:pig ~ ~ ~ {id:"minecraft:cow"}` would produce a cow.
    // Vanilla's `CompoundTag` is a map and cannot express the duplicate at all.
    let mut tag = nbt.clone();
    while tag.remove("id").is_some() {}
    tag.insert("id", entity_type.key.to_string());

    let mut tree = Vec::new();
    let entity = load_entity_tree(&tag, position, world, &mut tree)
        .ok_or_else(|| command_failed(&translations::COMMANDS_SUMMON_FAILED))?;

    if finalize && let Some(mob) = entity.as_mob() {
        let _ = mob.finalize_spawn(world, EntitySpawnReason::Command, None);
    }

    match world.try_add_entity_with_passengers(&tree) {
        Ok(()) => Ok(entity),
        Err(AddEntityError::DuplicateUuid { .. }) => {
            Err(command_failed(&translations::COMMANDS_SUMMON_FAILED_UUID))
        }
        Err(_) => Err(command_failed(&translations::COMMANDS_SUMMON_FAILED)),
    }
}

/// Builds one entity and its passengers, collecting the tree in place order.
///
/// Vanilla parity: `EntityType.loadEntityRecursive` with `SummonCommand`'s
/// post-load processor. That processor is `snapTo(pos.x, pos.y, pos.z,
/// e.getYRot(), e.getXRot())` and it runs on the vehicle *and* on every
/// passenger, so the command's position beats any `Pos` in the compound while
/// the rotation the compound asked for survives.
fn load_entity_tree(
    nbt: &NbtCompound,
    position: DVec3,
    world: &Arc<World>,
    tree: &mut Vec<SharedEntity>,
) -> Option<SharedEntity> {
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(bytes.as_slice())).ok()?;
    let loaded = read_entity_nbt(&(&borrowed).into())?;
    // The `summonable_entity` argument already refuses a type Steel has no
    // implementation for, but a `Passengers` entry names its own type and
    // nothing has checked it. Without this the raw fallback would seat an
    // inert entity and report success.
    if !ENTITIES.has_load_factory(loaded.entity_type) {
        return None;
    }

    // The type's own reader only ever sees what is left after the base fields,
    // which is the same split structure placement makes.
    let mut remainder_bytes = Vec::new();
    loaded.remainder.write(&mut remainder_bytes);
    let remainder = read_borrowed_compound(&mut Cursor::new(remainder_bytes.as_slice())).ok()?;

    let entity = ENTITIES.create_and_load_or_raw(
        EntityLoadRequest {
            entity_type: loaded.entity_type,
            position,
            uuid: loaded.uuid.unwrap_or_else(Uuid::new_v4),
            velocity: loaded.velocity,
            rotation: loaded.rotation,
            fall_distance: loaded.fall_distance,
            fire_freeze: loaded.fire_freeze,
            on_ground: loaded.on_ground,
            save_data: loaded.save_data,
            world: Arc::downgrade(world),
        },
        &remainder,
    );
    tree.push(Arc::clone(&entity));

    for passenger_nbt in &loaded.passengers {
        let Some(passenger) = load_entity_tree(passenger_nbt, position, world, tree) else {
            // Vanilla's `loadPassengersRecursive` skips a passenger it cannot
            // build and keeps the vehicle. A rider the server does not know
            // about is not a reason to lose the horse.
            continue;
        };
        EntityBase::start_riding_relationship(&entity, &passenger);
    }

    Some(entity)
}

fn command_failed(translation: &'static Translation<0>) -> CommandSyntaxError {
    CommandSyntaxError::dynamic(TextComponent::from(translation))
}

#[cfg(test)]
mod tests {
    use super::super::create_dispatcher;
    use crate::bootstrap::init_globals_once;
    use crate::command::{
        brigadier::{CommandDispatcher, NodeId},
        execution::{CommandSource, SteelArgumentType, SteelCommandRuntime},
    };

    type Dispatcher = CommandDispatcher<CommandSource, SteelCommandRuntime>;

    fn child(dispatcher: &Dispatcher, parent: NodeId, name: &str) -> NodeId {
        let Some(children) = dispatcher.children(parent) else {
            panic!("parent node should exist");
        };
        let Some(child) = children.iter().copied().find(|child| {
            dispatcher
                .node(*child)
                .is_some_and(|node| node.name() == name)
        }) else {
            panic!("child {name} should exist");
        };
        child
    }

    #[test]
    fn summon_graph_uses_typed_entity_and_deferred_position_arguments() {
        init_globals_once();
        let Ok(dispatcher) = create_dispatcher() else {
            panic!("built-in commands should register");
        };
        let root = child(&dispatcher, dispatcher.root(), "summon");
        let entity = child(&dispatcher, root, "entity");
        assert_eq!(
            dispatcher
                .node(entity)
                .and_then(|node| node.argument_type()),
            Some(&SteelArgumentType::summonable_entity())
        );
        assert!(matches!(
            dispatcher.node(entity),
            Some(node) if node.is_executable()
        ));

        let position = child(&dispatcher, entity, "pos");
        assert_eq!(
            dispatcher
                .node(position)
                .and_then(|node| node.argument_type()),
            Some(&SteelArgumentType::vec3(true))
        );
        assert!(matches!(
            dispatcher.node(position),
            Some(node) if node.is_executable()
        ));

        // Vanilla hangs the compound off `pos`. Putting it under `entity`
        // would be a syntax Steel invented and a client would not suggest.
        let nbt = child(&dispatcher, position, "nbt");
        assert_eq!(
            dispatcher.node(nbt).and_then(|node| node.argument_type()),
            Some(&SteelArgumentType::nbt_compound_tag())
        );
        assert!(matches!(
            dispatcher.node(nbt),
            Some(node) if node.is_executable()
        ));
        assert!(dispatcher.children(nbt).is_some_and(<[_]>::is_empty));
    }
}
