#!/usr/bin/env python3
"""Generate Bukkit's `GameRule` and Paper's `GameRules` from the extracted registry.

26.x made game rules a registry keyed in snake_case: `doInsomnia` became
`spawn_phantoms`, `doDaylightCycle` became `advance_time`. Paper's `GameRules`
holds one constant per registry key, named by upper-casing it, so that half is
generated straight from `game_rules.json` and cannot name a rule Foton lacks.

The old camelCase constants on `GameRule` are what most plugins still compile
against, and their mapping is not derivable from the key names. It is taken
from Paper's own `org/bukkit/GameRule.java` (1.21.11), which declares each one
as an alias of a `GameRules` constant -- and, for five of them, as a bridge
that inverts or converts the value. `LEGACY` below is that table, transcribed
and checked here against the registry so an alias to a missing rule fails the
build instead of failing a plugin.
"""
import json
import sys
from pathlib import Path

repo = Path(__file__).resolve().parents[1]
rules = json.loads((repo / "foton-registry/build_assets/game_rules.json").read_text())["game_rules"]
if not rules:
    raise SystemExit("no game rules found")
out = Path(sys.argv[1]) / "org/bukkit"
out.mkdir(parents=True, exist_ok=True)

JAVA_TYPES = {"bool": "Boolean", "int": "Integer"}
types = {}
for rule in rules:
    kind = JAVA_TYPES.get(rule["type"])
    if kind is None:
        raise SystemExit(f"unknown game rule type {rule['type']} for {rule['name']}")
    types[rule["name"]] = kind

# Paper GameRule.java: constant -> (registry key, bridge). A bridge names how a
# legacy value is stored in the renamed rule and read back from it.
INVERT = ("value -> !value", "value -> !value")
LEGACY = {
    "ANNOUNCE_ADVANCEMENTS": ("show_advancement_messages", None),
    "COMMAND_BLOCK_OUTPUT": ("command_block_output", None),
    "DISABLE_PLAYER_MOVEMENT_CHECK": ("player_movement_check", INVERT),
    "DISABLE_ELYTRA_MOVEMENT_CHECK": ("elytra_movement_check", INVERT),
    "DO_DAYLIGHT_CYCLE": ("advance_time", None),
    "DO_ENTITY_DROPS": ("entity_drops", None),
    "DO_FIRE_TICK": ("fire_spread_radius_around_player",
                     ("value -> value ? 128 : 0", "value -> value != 0")),
    "DO_LIMITED_CRAFTING": ("limited_crafting", None),
    "PROJECTILES_CAN_BREAK_BLOCKS": ("projectiles_can_break_blocks", None),
    "DO_MOB_LOOT": ("mob_drops", None),
    "DO_MOB_SPAWNING": ("spawn_mobs", None),
    "DO_TILE_DROPS": ("block_drops", None),
    "DO_WEATHER_CYCLE": ("advance_weather", None),
    "KEEP_INVENTORY": ("keep_inventory", None),
    "LOG_ADMIN_COMMANDS": ("log_admin_commands", None),
    "MOB_GRIEFING": ("mob_griefing", None),
    "NATURAL_REGENERATION": ("natural_health_regeneration", None),
    "REDUCED_DEBUG_INFO": ("reduced_debug_info", None),
    "SEND_COMMAND_FEEDBACK": ("send_command_feedback", None),
    "SHOW_DEATH_MESSAGES": ("show_death_messages", None),
    "SPECTATORS_GENERATE_CHUNKS": ("spectators_generate_chunks", None),
    "DISABLE_RAIDS": ("raids", INVERT),
    "DO_INSOMNIA": ("spawn_phantoms", None),
    "DO_IMMEDIATE_RESPAWN": ("immediate_respawn", None),
    "DROWNING_DAMAGE": ("drowning_damage", None),
    "FALL_DAMAGE": ("fall_damage", None),
    "FIRE_DAMAGE": ("fire_damage", None),
    "FREEZE_DAMAGE": ("freeze_damage", None),
    "DO_PATROL_SPAWNING": ("spawn_patrols", None),
    "DO_TRADER_SPAWNING": ("spawn_wandering_traders", None),
    "DO_WARDEN_SPAWNING": ("spawn_wardens", None),
    "FORGIVE_DEAD_PLAYERS": ("forgive_dead_players", None),
    "UNIVERSAL_ANGER": ("universal_anger", None),
    "BLOCK_EXPLOSION_DROP_DECAY": ("block_explosion_drop_decay", None),
    "MOB_EXPLOSION_DROP_DECAY": ("mob_explosion_drop_decay", None),
    "TNT_EXPLOSION_DROP_DECAY": ("tnt_explosion_drop_decay", None),
    "WATER_SOURCE_CONVERSION": ("water_source_conversion", None),
    "LAVA_SOURCE_CONVERSION": ("lava_source_conversion", None),
    "GLOBAL_SOUND_EVENTS": ("global_sound_events", None),
    "DO_VINES_SPREAD": ("spread_vines", None),
    "ENDER_PEARLS_VANISH_ON_DEATH": ("ender_pearls_vanish_on_death", None),
    "ALLOW_FIRE_TICKS_AWAY_FROM_PLAYER": ("fire_spread_radius_around_player",
                                          ("value -> value ? -1 : 128", "value -> value == -1")),
    "TNT_EXPLODES": ("tnt_explodes", None),
    "LOCATOR_BAR": ("locator_bar", None),
    "PVP": ("pvp", None),
    "SPAWN_MONSTERS": ("spawn_monsters", None),
    "ALLOW_ENTERING_NETHER_USING_PORTALS": ("allow_entering_nether_using_portals", None),
    "COMMAND_BLOCKS_ENABLED": ("command_blocks_work", None),
    "SPAWNER_BLOCKS_ENABLED": ("spawner_blocks_work", None),
    "RANDOM_TICK_SPEED": ("random_tick_speed", None),
    "SPAWN_RADIUS": ("respawn_radius", None),
    "MAX_ENTITY_CRAMMING": ("max_entity_cramming", None),
    "MAX_COMMAND_CHAIN_LENGTH": ("max_command_sequence_length", None),
    "MAX_COMMAND_FORK_COUNT": ("max_command_forks", None),
    "COMMAND_MODIFICATION_BLOCK_LIMIT": ("max_block_modifications", None),
    "PLAYERS_SLEEPING_PERCENTAGE": ("players_sleeping_percentage", None),
    "SNOW_ACCUMULATION_HEIGHT": ("max_snow_accumulation_height", None),
    "PLAYERS_NETHER_PORTAL_DEFAULT_DELAY": ("players_nether_portal_default_delay", None),
    "PLAYERS_NETHER_PORTAL_CREATIVE_DELAY": ("players_nether_portal_creative_delay", None),
    "MINECART_MAX_SPEED": ("max_minecart_speed", None),
}

canonical = []
for name in sorted(types):
    kind = types[name]
    canonical.append(
        f'    public static final GameRule<{kind}> {name.upper()} = '
        f'GameRule.register("{name}", {kind}.class);'
    )

legacy = []
for constant, (key, bridge) in LEGACY.items():
    if key not in types:
        raise SystemExit(f"GameRule.{constant} aliases {key}, which the registry does not have")
    stored = types[key]
    if bridge is None:
        legacy.append(
            f'    public static final GameRule<{stored}> {constant} = '
            f'alias("{key}", {stored}.class);'
        )
        continue
    to_stored, from_stored = bridge
    legacy.append(
        f'    public static final GameRule<Boolean> {constant} = bridge("{key}", '
        f'{stored}.class, '
        f'(java.util.function.Function<Boolean, {stored}>) {to_stored}, '
        f'(java.util.function.Function<{stored}, Boolean>) {from_stored});'
    )

(out / "GameRules.java").write_text(
    "package org.bukkit;\n\n"
    "/** Every vanilla game rule, generated from Foton's extracted registry. */\n"
    "public final class GameRules {\n"
    "    private GameRules() {}\n\n"
    "    /** Loads this class, which is what registers every rule. */\n"
    "    static void load() {}\n\n"
    + "\n".join(canonical)
    + "\n}\n",
    encoding="utf-8",
)

template = (repo / "dev/game-rule.java.in").read_text(encoding="utf-8")
(out / "GameRule.java").write_text(
    template.replace("    // @LEGACY@\n", "\n".join(legacy) + "\n"), encoding="utf-8"
)
