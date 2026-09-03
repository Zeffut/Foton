package org.bukkit;

import foton.Native;

/** A vanilla registry tag. Membership is evaluated against the live registry. */
public final class Tag<T extends Keyed> {
    /** Vanilla button block tag. */
    public static final Tag<Material> BUTTONS = new Tag<>(
        NamespacedKey.minecraft("buttons"), Material.class, "blocks");
    /** Vanilla door block tag. */
    public static final Tag<Material> DOORS = new Tag<>(
        NamespacedKey.minecraft("doors"), Material.class, "blocks");
    /** Vanilla pressure plate block tag. */
    public static final Tag<Material> PRESSURE_PLATES = new Tag<>(
        NamespacedKey.minecraft("pressure_plates"), Material.class, "blocks");
    public static final Tag<Material> CARPETS = new Tag<>(NamespacedKey.minecraft("carpets"), Material.class, "blocks");
    public static final Tag<Material> SAND = new Tag<>(NamespacedKey.minecraft("sand"), Material.class, "blocks");
    public static final Tag<Material> VALID_SPAWN = new Tag<>(NamespacedKey.minecraft("valid_spawn"), Material.class, "blocks");
    public static final Tag<Material> FENCES = new Tag<>(NamespacedKey.minecraft("fences"), Material.class, "blocks");
    public static final Tag<Material> CORAL_BLOCKS = new Tag<>(NamespacedKey.minecraft("coral_blocks"), Material.class, "blocks");
    public static final Tag<Material> SIGNS = new Tag<>(NamespacedKey.minecraft("signs"), Material.class, "blocks");
    public static final Tag<Material> ALL_SIGNS = new Tag<>(NamespacedKey.minecraft("all_signs"), Material.class, "blocks");
    public static final Tag<Material> WALL_SIGNS = new Tag<>(NamespacedKey.minecraft("wall_signs"), Material.class, "blocks");
    public static final Tag<Material> CEILING_HANGING_SIGNS = new Tag<>(NamespacedKey.minecraft("ceiling_hanging_signs"), Material.class, "blocks");
    public static final Tag<Material> ICE = new Tag<>(NamespacedKey.minecraft("ice"), Material.class, "blocks");
    public static final Tag<Material> FENCE_GATES = new Tag<>(NamespacedKey.minecraft("fence_gates"), Material.class, "blocks");
    public static final Tag<Material> ITEMS_BOOKSHELF_BOOKS = new Tag<>(NamespacedKey.minecraft("bookshelf_books"), Material.class, "items");
    public static final Tag<Material> WOODEN_TRAPDOORS = new Tag<>(NamespacedKey.minecraft("wooden_trapdoors"), Material.class, "blocks");
    public static final Tag<Material> TRAPDOORS = new Tag<>(NamespacedKey.minecraft("trapdoors"), Material.class, "blocks");
    public static final Tag<Material> SLABS = new Tag<>(NamespacedKey.minecraft("slabs"), Material.class, "blocks");
    public static final Tag<Material> STAIRS = new Tag<>(NamespacedKey.minecraft("stairs"), Material.class, "blocks");
    public static final Tag<Material> LOGS = new Tag<>(NamespacedKey.minecraft("logs"), Material.class, "blocks");
    public static final Tag<Material> PLANKS = new Tag<>(NamespacedKey.minecraft("planks"), Material.class, "blocks");
    public static final Tag<Material> WOOL = new Tag<>(NamespacedKey.minecraft("wool"), Material.class, "blocks");
    public static final Tag<Material> LEAVES = new Tag<>(NamespacedKey.minecraft("leaves"), Material.class, "blocks");
    public static final Tag<Material> SAPLINGS = new Tag<>(NamespacedKey.minecraft("saplings"), Material.class, "blocks");
    public static final Tag<Material> FLOWERS = new Tag<>(NamespacedKey.minecraft("flowers"), Material.class, "blocks");
    public static final Tag<Material> RAILS = new Tag<>(NamespacedKey.minecraft("rails"), Material.class, "blocks");
    public static final Tag<Material> ITEMS_SHOVELS = new Tag<>(NamespacedKey.minecraft("shovels"), Material.class, "items");
    public static final Tag<Material> ENDERMAN_HOLDABLE = new Tag<>(NamespacedKey.minecraft("enderman_holdable"), Material.class, "blocks");
    public static final Tag<Material> ANVIL = new Tag<>(NamespacedKey.minecraft("anvil"), Material.class, "blocks");
    public static final Tag<Material> BANNERS = new Tag<>(NamespacedKey.minecraft("banners"), Material.class, "blocks");
    public static final Tag<Material> BARS = new Tag<>(NamespacedKey.minecraft("bars"), Material.class, "blocks");
    public static final Tag<Material> BEDS = new Tag<>(NamespacedKey.minecraft("beds"), Material.class, "blocks");
    public static final Tag<Material> CANDLES = new Tag<>(NamespacedKey.minecraft("candles"), Material.class, "blocks");
    public static final Tag<Material> CANDLE_CAKES = new Tag<>(NamespacedKey.minecraft("candle_cakes"), Material.class, "blocks");
    public static final Tag<Material> CAULDRONS = new Tag<>(NamespacedKey.minecraft("cauldrons"), Material.class, "blocks");
    public static final Tag<Material> CAVE_VINES = new Tag<>(NamespacedKey.minecraft("cave_vines"), Material.class, "blocks");
    public static final Tag<Material> CHAINS = new Tag<>(NamespacedKey.minecraft("chains"), Material.class, "blocks");
    public static final Tag<Material> COAL_ORES = new Tag<>(NamespacedKey.minecraft("coal_ores"), Material.class, "blocks");
    public static final Tag<Material> COPPER_CHESTS = new Tag<>(NamespacedKey.minecraft("copper_chests"), Material.class, "blocks");
    public static final Tag<Material> COPPER_GOLEM_STATUES = new Tag<>(NamespacedKey.minecraft("copper_golem_statues"), Material.class, "blocks");
    public static final Tag<Material> COPPER_ORES = new Tag<>(NamespacedKey.minecraft("copper_ores"), Material.class, "blocks");
    public static final Tag<Material> CORALS = new Tag<>(NamespacedKey.minecraft("corals"), Material.class, "blocks");
    public static final Tag<Material> CORAL_PLANTS = new Tag<>(NamespacedKey.minecraft("coral_plants"), Material.class, "blocks");
    public static final Tag<Material> CROPS = new Tag<>(NamespacedKey.minecraft("crops"), Material.class, "blocks");
    public static final Tag<Material> DIAMOND_ORES = new Tag<>(NamespacedKey.minecraft("diamond_ores"), Material.class, "blocks");
    public static final Tag<Material> EMERALD_ORES = new Tag<>(NamespacedKey.minecraft("emerald_ores"), Material.class, "blocks");
    public static final Tag<Material> FLOWER_POTS = new Tag<>(NamespacedKey.minecraft("flower_pots"), Material.class, "blocks");
    public static final Tag<Material> GOLD_ORES = new Tag<>(NamespacedKey.minecraft("gold_ores"), Material.class, "blocks");
    public static final Tag<Material> IRON_ORES = new Tag<>(NamespacedKey.minecraft("iron_ores"), Material.class, "blocks");
    public static final Tag<Material> ITEMS_BANNERS = new Tag<>(NamespacedKey.minecraft("banners"), Material.class, "items");
    public static final Tag<Material> ITEMS_BOATS = new Tag<>(NamespacedKey.minecraft("boats"), Material.class, "items");
    public static final Tag<Material> ITEMS_BUNDLES = new Tag<>(NamespacedKey.minecraft("bundles"), Material.class, "items");
    public static final Tag<Material> ITEMS_CHEST_ARMOR = new Tag<>(NamespacedKey.minecraft("chest_armor"), Material.class, "items");
    public static final Tag<Material> ITEMS_CHEST_BOATS = new Tag<>(NamespacedKey.minecraft("chest_boats"), Material.class, "items");
    public static final Tag<Material> ITEMS_DECORATED_POT_SHERDS = new Tag<>(NamespacedKey.minecraft("decorated_pot_sherds"), Material.class, "items");
    public static final Tag<Material> ITEMS_FOOT_ARMOR = new Tag<>(NamespacedKey.minecraft("foot_armor"), Material.class, "items");
    public static final Tag<Material> ITEMS_HARNESSES = new Tag<>(NamespacedKey.minecraft("harnesses"), Material.class, "items");
    public static final Tag<Material> ITEMS_HEAD_ARMOR = new Tag<>(NamespacedKey.minecraft("head_armor"), Material.class, "items");
    public static final Tag<Material> ITEMS_LEG_ARMOR = new Tag<>(NamespacedKey.minecraft("leg_armor"), Material.class, "items");
    public static final Tag<Material> ITEMS_SKULLS = new Tag<>(NamespacedKey.minecraft("skulls"), Material.class, "items");
    public static final Tag<Material> ITEMS_SPEARS = new Tag<>(NamespacedKey.minecraft("spears"), Material.class, "items");
    public static final Tag<Material> LANTERNS = new Tag<>(NamespacedKey.minecraft("lanterns"), Material.class, "blocks");
    public static final Tag<Material> LAPIS_ORES = new Tag<>(NamespacedKey.minecraft("lapis_ores"), Material.class, "blocks");
    public static final Tag<Material> LIGHTNING_RODS = new Tag<>(NamespacedKey.minecraft("lightning_rods"), Material.class, "blocks");
    public static final Tag<Material> REDSTONE_ORES = new Tag<>(NamespacedKey.minecraft("redstone_ores"), Material.class, "blocks");
    public static final Tag<Material> SHULKER_BOXES = new Tag<>(NamespacedKey.minecraft("shulker_boxes"), Material.class, "blocks");
    public static final Tag<Material> SMALL_FLOWERS = new Tag<>(NamespacedKey.minecraft("small_flowers"), Material.class, "blocks");
    public static final Tag<Material> STANDING_SIGNS = new Tag<>(NamespacedKey.minecraft("standing_signs"), Material.class, "blocks");
    public static final Tag<Material> WALLS = new Tag<>(NamespacedKey.minecraft("walls"), Material.class, "blocks");
    public static final Tag<Material> WALL_CORALS = new Tag<>(NamespacedKey.minecraft("wall_corals"), Material.class, "blocks");
    public static final Tag<Material> WOODEN_PRESSURE_PLATES = new Tag<>(NamespacedKey.minecraft("wooden_pressure_plates"), Material.class, "blocks");
    public static final Tag<Material> WOODEN_SHELVES = new Tag<>(NamespacedKey.minecraft("wooden_shelves"), Material.class, "blocks");
    public static final Tag<Material> WOOL_CARPETS = new Tag<>(NamespacedKey.minecraft("wool_carpets"), Material.class, "blocks");
    public static final Tag<Material> UNDERWATER_BONEMEAL = new Tag<>(NamespacedKey.minecraft("underwater_bonemeals"), Material.class, "blocks");
    public static final Tag<Material> UNDERWATER_BONEMEALS = UNDERWATER_BONEMEAL;

    private final NamespacedKey key;
    private final Class<T> type;
    private final String registry;

    public Tag(NamespacedKey key, Class<T> type, String registry) {
        this.key = key; this.type = type; this.registry = registry;
    }
    public NamespacedKey getKey() { return key; }
    public boolean isTagged(T value) {
        if (value == null || !type.isInstance(value)) return false;
        return Native.isTagged(registry, key.toString(), value.getKey().toString());
    }

    /** Returns the live registry entries currently in this vanilla tag. */
    @SuppressWarnings("unchecked")
    public java.util.Set<T> getValues() {
        java.util.LinkedHashSet<T> values = new java.util.LinkedHashSet<>();
        String[] keys = Native.tagValues(registry, key.toString());
        if (keys == null) return java.util.Collections.unmodifiableSet(values);
        for (String value : keys) {
            try {
                T entry = (T) type.getMethod("valueOf", String.class)
                    .invoke(null, value.substring(value.indexOf(':') + 1).toUpperCase(java.util.Locale.ROOT));
                if (entry != null) values.add(entry);
            } catch (ReflectiveOperationException | RuntimeException ignored) { }
        }
        return java.util.Collections.unmodifiableSet(values);
    }
}
