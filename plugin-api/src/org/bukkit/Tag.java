package org.bukkit;

import java.util.Set;

/** A vanilla tag: a named set of registry entries, as the data pack defines it.
 *
 * <p>An interface, as in Paper: {@code Tag.LOGS.isTagged(material)} is an
 * {@code invokeinterface}, which a class answers with
 * {@code IncompatibleClassChangeError}. Membership is asked of the server's
 * live registry (see {@link foton.FotonTag}).</p>
 */
public interface Tag<T extends Keyed> extends Keyed {
    String REGISTRY_BLOCKS = "blocks";
    String REGISTRY_ITEMS = "items";
    String REGISTRY_FLUIDS = "fluids";
    String REGISTRY_ENTITY_TYPES = "entity_types";
    String REGISTRY_GAME_EVENTS = "game_events";

    /** Vanilla button block tag. */
    Tag<Material> BUTTONS = new foton.FotonTag<>(REGISTRY_BLOCKS, "buttons", Material.class);
    /** Vanilla door block tag. */
    Tag<Material> DOORS = new foton.FotonTag<>(REGISTRY_BLOCKS, "doors", Material.class);
    /** Vanilla pressure plate block tag. */
    Tag<Material> PRESSURE_PLATES = new foton.FotonTag<>(REGISTRY_BLOCKS, "pressure_plates", Material.class);
    Tag<Material> CARPETS = new foton.FotonTag<>(REGISTRY_BLOCKS, "carpets", Material.class);
    Tag<Material> SAND = new foton.FotonTag<>(REGISTRY_BLOCKS, "sand", Material.class);
    Tag<Material> VALID_SPAWN = new foton.FotonTag<>(REGISTRY_BLOCKS, "valid_spawn", Material.class);
    Tag<Material> FENCES = new foton.FotonTag<>(REGISTRY_BLOCKS, "fences", Material.class);
    Tag<Material> CORAL_BLOCKS = new foton.FotonTag<>(REGISTRY_BLOCKS, "coral_blocks", Material.class);
    Tag<Material> SIGNS = new foton.FotonTag<>(REGISTRY_BLOCKS, "signs", Material.class);
    Tag<Material> ALL_SIGNS = new foton.FotonTag<>(REGISTRY_BLOCKS, "all_signs", Material.class);
    Tag<Material> WALL_SIGNS = new foton.FotonTag<>(REGISTRY_BLOCKS, "wall_signs", Material.class);
    Tag<Material> CEILING_HANGING_SIGNS = new foton.FotonTag<>(REGISTRY_BLOCKS, "ceiling_hanging_signs", Material.class);
    Tag<Material> ICE = new foton.FotonTag<>(REGISTRY_BLOCKS, "ice", Material.class);
    Tag<Material> FENCE_GATES = new foton.FotonTag<>(REGISTRY_BLOCKS, "fence_gates", Material.class);
    Tag<Material> ITEMS_BOOKSHELF_BOOKS = new foton.FotonTag<>(REGISTRY_ITEMS, "bookshelf_books", Material.class);
    Tag<Material> WOODEN_TRAPDOORS = new foton.FotonTag<>(REGISTRY_BLOCKS, "wooden_trapdoors", Material.class);
    Tag<Material> TRAPDOORS = new foton.FotonTag<>(REGISTRY_BLOCKS, "trapdoors", Material.class);
    Tag<Material> SLABS = new foton.FotonTag<>(REGISTRY_BLOCKS, "slabs", Material.class);
    Tag<Material> STAIRS = new foton.FotonTag<>(REGISTRY_BLOCKS, "stairs", Material.class);
    Tag<Material> LOGS = new foton.FotonTag<>(REGISTRY_BLOCKS, "logs", Material.class);
    Tag<Material> PLANKS = new foton.FotonTag<>(REGISTRY_BLOCKS, "planks", Material.class);
    Tag<Material> WOOL = new foton.FotonTag<>(REGISTRY_BLOCKS, "wool", Material.class);
    Tag<Material> LEAVES = new foton.FotonTag<>(REGISTRY_BLOCKS, "leaves", Material.class);
    Tag<Material> SAPLINGS = new foton.FotonTag<>(REGISTRY_BLOCKS, "saplings", Material.class);
    Tag<Material> FLOWERS = new foton.FotonTag<>(REGISTRY_BLOCKS, "flowers", Material.class);
    Tag<Material> RAILS = new foton.FotonTag<>(REGISTRY_BLOCKS, "rails", Material.class);
    Tag<Material> ITEMS_SHOVELS = new foton.FotonTag<>(REGISTRY_ITEMS, "shovels", Material.class);
    Tag<Material> ENDERMAN_HOLDABLE = new foton.FotonTag<>(REGISTRY_BLOCKS, "enderman_holdable", Material.class);
    Tag<Material> ANVIL = new foton.FotonTag<>(REGISTRY_BLOCKS, "anvil", Material.class);
    Tag<Material> BANNERS = new foton.FotonTag<>(REGISTRY_BLOCKS, "banners", Material.class);
    Tag<Material> BARS = new foton.FotonTag<>(REGISTRY_BLOCKS, "bars", Material.class);
    Tag<Material> BEDS = new foton.FotonTag<>(REGISTRY_BLOCKS, "beds", Material.class);
    Tag<Material> CANDLES = new foton.FotonTag<>(REGISTRY_BLOCKS, "candles", Material.class);
    Tag<Material> CANDLE_CAKES = new foton.FotonTag<>(REGISTRY_BLOCKS, "candle_cakes", Material.class);
    Tag<Material> CAULDRONS = new foton.FotonTag<>(REGISTRY_BLOCKS, "cauldrons", Material.class);
    Tag<Material> CAVE_VINES = new foton.FotonTag<>(REGISTRY_BLOCKS, "cave_vines", Material.class);
    Tag<Material> CHAINS = new foton.FotonTag<>(REGISTRY_BLOCKS, "chains", Material.class);
    Tag<Material> COAL_ORES = new foton.FotonTag<>(REGISTRY_BLOCKS, "coal_ores", Material.class);
    Tag<Material> COPPER_CHESTS = new foton.FotonTag<>(REGISTRY_BLOCKS, "copper_chests", Material.class);
    Tag<Material> COPPER_GOLEM_STATUES = new foton.FotonTag<>(REGISTRY_BLOCKS, "copper_golem_statues", Material.class);
    Tag<Material> COPPER_ORES = new foton.FotonTag<>(REGISTRY_BLOCKS, "copper_ores", Material.class);
    Tag<Material> CORALS = new foton.FotonTag<>(REGISTRY_BLOCKS, "corals", Material.class);
    Tag<Material> CORAL_PLANTS = new foton.FotonTag<>(REGISTRY_BLOCKS, "coral_plants", Material.class);
    Tag<Material> CROPS = new foton.FotonTag<>(REGISTRY_BLOCKS, "crops", Material.class);
    Tag<Material> DIAMOND_ORES = new foton.FotonTag<>(REGISTRY_BLOCKS, "diamond_ores", Material.class);
    Tag<Material> EMERALD_ORES = new foton.FotonTag<>(REGISTRY_BLOCKS, "emerald_ores", Material.class);
    Tag<Material> FLOWER_POTS = new foton.FotonTag<>(REGISTRY_BLOCKS, "flower_pots", Material.class);
    Tag<Material> GOLD_ORES = new foton.FotonTag<>(REGISTRY_BLOCKS, "gold_ores", Material.class);
    Tag<Material> IRON_ORES = new foton.FotonTag<>(REGISTRY_BLOCKS, "iron_ores", Material.class);
    Tag<Material> ITEMS_BANNERS = new foton.FotonTag<>(REGISTRY_ITEMS, "banners", Material.class);
    Tag<Material> ITEMS_BOATS = new foton.FotonTag<>(REGISTRY_ITEMS, "boats", Material.class);
    Tag<Material> ITEMS_BUNDLES = new foton.FotonTag<>(REGISTRY_ITEMS, "bundles", Material.class);
    Tag<Material> ITEMS_CHEST_ARMOR = new foton.FotonTag<>(REGISTRY_ITEMS, "chest_armor", Material.class);
    Tag<Material> ITEMS_CHEST_BOATS = new foton.FotonTag<>(REGISTRY_ITEMS, "chest_boats", Material.class);
    Tag<Material> ITEMS_DECORATED_POT_SHERDS = new foton.FotonTag<>(REGISTRY_ITEMS, "decorated_pot_sherds", Material.class);
    Tag<Material> ITEMS_FOOT_ARMOR = new foton.FotonTag<>(REGISTRY_ITEMS, "foot_armor", Material.class);
    Tag<Material> ITEMS_HARNESSES = new foton.FotonTag<>(REGISTRY_ITEMS, "harnesses", Material.class);
    Tag<Material> ITEMS_HEAD_ARMOR = new foton.FotonTag<>(REGISTRY_ITEMS, "head_armor", Material.class);
    Tag<Material> ITEMS_LEG_ARMOR = new foton.FotonTag<>(REGISTRY_ITEMS, "leg_armor", Material.class);
    Tag<Material> ITEMS_SKULLS = new foton.FotonTag<>(REGISTRY_ITEMS, "skulls", Material.class);
    Tag<Material> ITEMS_TRIMMABLE_ARMOR = new foton.FotonTag<>(REGISTRY_ITEMS, "trimmable_armor", Material.class);
    Tag<Material> ITEMS_SPEARS = new foton.FotonTag<>(REGISTRY_ITEMS, "spears", Material.class);
    Tag<Material> LANTERNS = new foton.FotonTag<>(REGISTRY_BLOCKS, "lanterns", Material.class);
    Tag<Material> LAPIS_ORES = new foton.FotonTag<>(REGISTRY_BLOCKS, "lapis_ores", Material.class);
    Tag<Material> LIGHTNING_RODS = new foton.FotonTag<>(REGISTRY_BLOCKS, "lightning_rods", Material.class);
    Tag<Material> REDSTONE_ORES = new foton.FotonTag<>(REGISTRY_BLOCKS, "redstone_ores", Material.class);
    Tag<Material> SHULKER_BOXES = new foton.FotonTag<>(REGISTRY_BLOCKS, "shulker_boxes", Material.class);
    Tag<Material> SMALL_FLOWERS = new foton.FotonTag<>(REGISTRY_BLOCKS, "small_flowers", Material.class);
    Tag<Material> STANDING_SIGNS = new foton.FotonTag<>(REGISTRY_BLOCKS, "standing_signs", Material.class);
    Tag<Material> WALLS = new foton.FotonTag<>(REGISTRY_BLOCKS, "walls", Material.class);
    Tag<Material> WALL_CORALS = new foton.FotonTag<>(REGISTRY_BLOCKS, "wall_corals", Material.class);
    Tag<Material> WOODEN_PRESSURE_PLATES = new foton.FotonTag<>(REGISTRY_BLOCKS, "wooden_pressure_plates", Material.class);
    Tag<Material> WOODEN_SHELVES = new foton.FotonTag<>(REGISTRY_BLOCKS, "wooden_shelves", Material.class);
    Tag<Material> WOOL_CARPETS = new foton.FotonTag<>(REGISTRY_BLOCKS, "wool_carpets", Material.class);
    Tag<Material> UNDERWATER_BONEMEAL = new foton.FotonTag<>(REGISTRY_BLOCKS, "underwater_bonemeals", Material.class);
    Tag<Material> UNDERWATER_BONEMEALS = UNDERWATER_BONEMEAL;

    /** Whether the value is in this tag. */
    boolean isTagged(T item);

    /** Every value in this tag. */
    Set<T> getValues();
}
