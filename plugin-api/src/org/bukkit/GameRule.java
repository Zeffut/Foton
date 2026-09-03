package org.bukkit;

/** Typed handle for a vanilla gamerule. */
public final class GameRule<T> {
    private final String name;
    private final Class<T> type;
    private GameRule(String name, Class<T> type) { this.name = name; this.type = type; }
    public String getName() { return name; }
    public Class<T> getType() { return type; }

    public static GameRule<?> getByName(String name) {
        if (name == null) return null;
        for (GameRule<?> rule : values()) if (rule.name.equalsIgnoreCase(name)) return rule;
        return null;
    }
    public T parse(String value) {
        if (value == null) return null;
        try {
            if (type == Boolean.class) return type.cast(Boolean.valueOf(value));
            if (type == Integer.class) return type.cast(Integer.valueOf(value));
        } catch (NumberFormatException ignored) { }
        return null;
    }
    private static GameRule<Boolean> bool(String name) { return new GameRule<>(name, Boolean.class); }
    private static GameRule<Integer> integer(String name) { return new GameRule<>(name, Integer.class); }
    public static final GameRule<Boolean> DO_DAYLIGHT_CYCLE = bool("doDaylightCycle");
    public static final GameRule<Boolean> DO_WEATHER_CYCLE = bool("doWeatherCycle");
    public static final GameRule<Boolean> KEEP_INVENTORY = bool("keepInventory");
    public static final GameRule<Boolean> MOB_GRIEFING = bool("mobGriefing");
    public static final GameRule<Boolean> PVP = bool("pvp");
    public static final GameRule<Boolean> SHOW_DEATH_MESSAGES = bool("showDeathMessages");
    public static final GameRule<Boolean> DO_MOB_SPAWNING = bool("doMobSpawning");
    public static final GameRule<Boolean> DO_FIRE_TICK = bool("doFireTick");
    public static final GameRule<Integer> RANDOM_TICK_SPEED = integer("randomTickSpeed");
    public static GameRule<?>[] values() {
        return new GameRule<?>[]{DO_DAYLIGHT_CYCLE, DO_WEATHER_CYCLE, KEEP_INVENTORY, MOB_GRIEFING, PVP, SHOW_DEATH_MESSAGES, DO_MOB_SPAWNING, DO_FIRE_TICK, RANDOM_TICK_SPEED};
    }
}
