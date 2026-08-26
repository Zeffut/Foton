//! The `/raid` debug command.
//!
//! Vanilla parity: `net.minecraft.server.commands.RaidCommand`. It is the only
//! way to start a raid without waiting out a Bad Omen, and `check` is the only
//! way to read a raid's counters from inside the game, so it is what an
//! in-world test drives.
//!
//! Vanilla's `sound local` subcommand is a client-side debug horn played five
//! blocks east of the caller; it is left out because it tests the client's
//! positional audio rather than anything the server does.

use std::sync::Arc;

use steel_utils::{Identifier, translations};
use text_components::TextComponent;

use super::super::{
    brigadier::{ArgumentType, CommandNodeBuilder, CommandSyntaxError},
    execution::{CommandSource, SteelCommandContext, SteelCommandRuntime, argument, literal},
    registration::CommandRegistration,
};
use crate::entity::raider::ominous_banner;
use crate::entity::{
    ENTITIES, Entity, EntitySpawnReason, MobEffectInstance, SharedEntity, next_entity_id,
};
use crate::inventory::equipment::EquipmentSlot;
use crate::player::Player;
use crate::raid::{DEFAULT_MAX_RAID_OMEN_LEVEL, Raid};
use steel_registry::{vanilla_entities, vanilla_mob_effects};

/// Ticks of Glowing `/raid glow` paints the raiders with.
///
/// Vanilla parity: the `new MobEffectInstance(MobEffects.GLOWING, 1000, 1)` of
/// `RaidCommand.glow`.
const GLOW_DURATION: i32 = 1000;
const GLOW_AMPLIFIER: i32 = 1;

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("raid"), |_| command())
}

fn command() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    literal("raid")
        .then(
            literal("start")
                .then(argument("omenlvl", ArgumentType::integer(0, i32::MAX)).executes(start_raid)),
        )
        .then(literal("stop").executes(stop_raid))
        .then(literal("check").executes(check_raid))
        .then(literal("spawnleader").executes(spawn_leader))
        .then(
            literal("setomen")
                .then(argument("level", ArgumentType::integer(0, i32::MAX)).executes(set_omen)),
        )
        .then(literal("glow").executes(glow_raiders))
}

fn source_player(
    context: &SteelCommandContext<CommandSource>,
) -> Result<&Arc<Player>, CommandSyntaxError> {
    context.source().player().ok_or_else(|| {
        CommandSyntaxError::dynamic(TextComponent::from(
            &translations::PERMISSIONS_REQUIRES_PLAYER,
        ))
    })
}

/// Vanilla parity: `RaidCommand.getRaid`.
fn raid_here(
    context: &SteelCommandContext<CommandSource>,
) -> Result<Option<Arc<Raid>>, CommandSyntaxError> {
    let player = source_player(context)?;
    Ok(context
        .source()
        .world()
        .get_raid_at(player.block_position()))
}

/// Vanilla parity: `RaidCommand.start`.
fn start_raid(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let raid_omen_level = context.integer("omenlvl")?;
    let player = source_player(context)?;
    let world = context.source().world();
    let pos = player.block_position();

    if world.is_raided(pos) {
        context
            .source()
            .send_failure(TextComponent::plain("Raid already started close by"));
        return Ok(-1);
    }

    let Some(raid) = world.raids().create_or_extend_raid(world, player, pos) else {
        context.source().send_failure(TextComponent::plain(
            "Failed to create a raid in your local village",
        ));
        return Ok(1);
    };

    raid.set_raid_omen_level(raid_omen_level);
    context.source().send_success(
        &TextComponent::plain("Created a raid in your local village"),
        false,
    );
    Ok(1)
}

/// Vanilla parity: `RaidCommand.stop`.
fn stop_raid(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let Some(raid) = raid_here(context)? else {
        context
            .source()
            .send_failure(TextComponent::plain("No raid here"));
        return Ok(-1);
    };
    raid.stop();
    context
        .source()
        .send_success(&TextComponent::plain("Stopped raid"), false);
    Ok(1)
}

/// Vanilla parity: `RaidCommand.check`.
fn check_raid(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let Some(raid) = raid_here(context)? else {
        context
            .source()
            .send_failure(TextComponent::plain("Found no started raids"));
        return Ok(0);
    };

    let world = context.source().world();
    context
        .source()
        .send_success(&TextComponent::plain("Found a started raid! "), false);
    context.source().send_success(
        &TextComponent::plain(format!(
            "Num groups spawned: {} Raid omen level: {} Num mobs: {} Raid health: {} / {}",
            raid.groups_spawned(),
            raid.raid_omen_level(),
            raid.total_raiders_alive(),
            raid.health_of_living_raiders(world),
            raid.total_health(),
        )),
        false,
    );
    Ok(1)
}

/// Vanilla parity: `RaidCommand.setRaidOmenLevel`.
fn set_omen(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let level = context.integer("level")?;
    let Some(raid) = raid_here(context)? else {
        context
            .source()
            .send_failure(TextComponent::plain("No raid found here"));
        return Ok(1);
    };

    let max = DEFAULT_MAX_RAID_OMEN_LEVEL;
    if level > max {
        context.source().send_failure(TextComponent::plain(format!(
            "Sorry, the max raid omen level you can set is {max}"
        )));
        return Ok(1);
    }

    let before = raid.raid_omen_level();
    raid.set_raid_omen_level(level);
    context.source().send_success(
        &TextComponent::plain(format!(
            "Changed village's raid omen level from {before} to {level}"
        )),
        false,
    );
    Ok(1)
}

/// Vanilla parity: `RaidCommand.glow`.
fn glow_raiders(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let Some(raid) = raid_here(context)? else {
        return Ok(1);
    };
    let world = context.source().world();
    for raider_id in raid.all_raider_ids() {
        let Some(entity) = world.get_entity_by_id(raider_id) else {
            continue;
        };
        let Some(raider) = entity.as_living_entity() else {
            continue;
        };
        raider.add_mob_effect(MobEffectInstance::with_duration(
            vanilla_mob_effects::GLOWING,
            GLOW_DURATION,
            GLOW_AMPLIFIER,
        ));
    }
    Ok(1)
}

/// Vanilla parity: `RaidCommand.spawnLeader`.
#[expect(
    clippy::unnecessary_wraps,
    reason = "Command executors use a shared fallible callback signature."
)]
fn spawn_leader(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let world = context.source().world();
    let position = context.source().anchor_position();

    let Some(entity) = ENTITIES.create(
        &vanilla_entities::PILLAGER,
        next_entity_id(),
        position,
        Arc::downgrade(world),
    ) else {
        context
            .source()
            .send_failure(TextComponent::plain("Pillager failed to spawn"));
        return Ok(0);
    };
    let Some(raider) = entity.as_raider() else {
        context
            .source()
            .send_failure(TextComponent::plain("Pillager failed to spawn"));
        return Ok(0);
    };

    raider.set_patrol_leader(true);
    raider
        .living_base()
        .equipment()
        .lock()
        .set(EquipmentSlot::Head, ominous_banner());
    if let Some(mob) = entity.as_mob() {
        let _ = mob.finalize_spawn(world, EntitySpawnReason::Command, None);
    }
    if world.try_add_entity(SharedEntity::clone(&entity)).is_err() {
        context
            .source()
            .send_failure(TextComponent::plain("Pillager failed to spawn"));
        return Ok(0);
    }

    context
        .source()
        .send_success(&TextComponent::plain("Spawned a raid captain"), false);
    Ok(1)
}
