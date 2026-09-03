package org.bukkit.potion;

import java.util.Locale;
import java.util.Objects;

/** Stable Bukkit handle for a named vanilla mob effect. */
public final class PotionEffectType {
    public static final PotionEffectType SPEED = named("speed");
    public static final PotionEffectType SLOW = named("slowness");
    /** Modern Bukkit spelling retained alongside the legacy {@link #SLOW}. */
    public static final PotionEffectType SLOWNESS = SLOW;
    public static final PotionEffectType FAST_DIGGING = named("haste");
    public static final PotionEffectType SLOW_DIGGING = named("mining_fatigue");
    public static final PotionEffectType INCREASE_DAMAGE = named("strength");
    public static final PotionEffectType HEAL = named("instant_health");
    public static final PotionEffectType HARM = named("instant_damage");
    public static final PotionEffectType INSTANT_DAMAGE = HARM;
    public static final PotionEffectType MINING_FATIGUE = SLOW_DIGGING;
    public static final PotionEffectType JUMP = named("jump_boost");
    public static final PotionEffectType CONFUSION = named("nausea");
    public static final PotionEffectType NAUSEA = CONFUSION;
    public static final PotionEffectType REGENERATION = named("regeneration");
    public static final PotionEffectType DAMAGE_RESISTANCE = named("resistance");
    public static final PotionEffectType FIRE_RESISTANCE = named("fire_resistance");
    public static final PotionEffectType WATER_BREATHING = named("water_breathing");
    public static final PotionEffectType INVISIBILITY = named("invisibility");
    public static final PotionEffectType BLINDNESS = named("blindness");
    public static final PotionEffectType NIGHT_VISION = named("night_vision");
    public static final PotionEffectType HUNGER = named("hunger");
    public static final PotionEffectType WEAKNESS = named("weakness");
    public static final PotionEffectType POISON = named("poison");
    public static final PotionEffectType WITHER = named("wither");
    public static final PotionEffectType HEALTH_BOOST = named("health_boost");
    public static final PotionEffectType ABSORPTION = named("absorption");
    public static final PotionEffectType SATURATION = named("saturation");
    public static final PotionEffectType GLOWING = named("glowing");
    public static final PotionEffectType LEVITATION = named("levitation");
    public static final PotionEffectType LUCK = named("luck");
    public static final PotionEffectType UNLUCK = named("unluck");
    public static final PotionEffectType SLOW_FALLING = named("slow_falling");
    public static final PotionEffectType CONDUIT_POWER = named("conduit_power");
    public static final PotionEffectType DOLPHINS_GRACE = named("dolphins_grace");
    public static final PotionEffectType BAD_OMEN = named("bad_omen");
    public static final PotionEffectType HERO_OF_THE_VILLAGE = named("hero_of_the_village");
    public static final PotionEffectType DARKNESS = named("darkness");
    public static final PotionEffectType WIND_CHARGED = named("wind_charged");
    public static final PotionEffectType WEAVING = named("weaving");
    public static final PotionEffectType OOZING = named("oozing");
    public static final PotionEffectType INFESTED = named("infested");
    public static final PotionEffectType BREATH_OF_THE_NAUTILUS = named("breath_of_the_nautilus");
    public static final PotionEffectType TRIAL_OMEN = named("trial_omen");
    private final String name;
    private final int id;

    private PotionEffectType(String name, int id) { this.name = name; this.id = id; }

    private static PotionEffectType named(String name) {
        return new PotionEffectType(name, idForName(name));
    }

    private static int idForName(String name) {
        return switch (name) {
            case "speed" -> 0;
            case "slowness" -> 1;
            case "haste" -> 2;
            case "mining_fatigue" -> 3;
            case "strength" -> 4;
            case "instant_health" -> 5;
            case "instant_damage" -> 6;
            case "jump_boost" -> 7;
            case "nausea" -> 8;
            case "regeneration" -> 9;
            case "resistance" -> 10;
            case "fire_resistance" -> 11;
            case "water_breathing" -> 12;
            case "invisibility" -> 13;
            case "blindness" -> 14;
            case "night_vision" -> 15;
            case "hunger" -> 16;
            case "weakness" -> 17;
            case "poison" -> 18;
            case "wither" -> 19;
            case "health_boost" -> 20;
            case "absorption" -> 21;
            case "saturation" -> 22;
            case "glowing" -> 23;
            case "levitation" -> 24;
            case "luck" -> 25;
            case "unluck" -> 26;
            case "slow_falling" -> 27;
            case "conduit_power" -> 28;
            case "dolphins_grace" -> 29;
            case "bad_omen" -> 30;
            case "hero_of_the_village" -> 31;
            case "darkness" -> 32;
            case "trial_omen" -> 33;
            case "breath_of_the_nautilus" -> 34;
            default -> -1;
        };
    }

    public static PotionEffectType getById(int id) {
        return switch (id) {
            case 0 -> SPEED;
            case 1 -> SLOWNESS;
            case 2 -> FAST_DIGGING;
            case 3 -> SLOW_DIGGING;
            case 4 -> INCREASE_DAMAGE;
            case 5 -> HEAL;
            case 6 -> HARM;
            case 7 -> JUMP;
            case 8 -> CONFUSION;
            case 9 -> REGENERATION;
            case 10 -> DAMAGE_RESISTANCE;
            case 11 -> FIRE_RESISTANCE;
            case 12 -> WATER_BREATHING;
            case 13 -> INVISIBILITY;
            case 14 -> BLINDNESS;
            case 15 -> NIGHT_VISION;
            case 16 -> HUNGER;
            case 17 -> WEAKNESS;
            case 18 -> POISON;
            case 19 -> WITHER;
            case 20 -> HEALTH_BOOST;
            case 21 -> ABSORPTION;
            case 22 -> SATURATION;
            case 23 -> GLOWING;
            case 24 -> LEVITATION;
            case 25 -> LUCK;
            case 26 -> UNLUCK;
            case 27 -> SLOW_FALLING;
            case 28 -> CONDUIT_POWER;
            case 29 -> DOLPHINS_GRACE;
            case 30 -> BAD_OMEN;
            case 31 -> HERO_OF_THE_VILLAGE;
            case 32 -> DARKNESS;
            case 33 -> TRIAL_OMEN;
            case 34 -> BREATH_OF_THE_NAUTILUS;
            default -> null;
        };
    }

    public static PotionEffectType getByName(String name) {
        if (name == null || name.isEmpty()) return null;
        String normalized = name.toLowerCase(Locale.ROOT);
        return new PotionEffectType(normalized, idForName(normalized));
    }
    public String getName() { return name; }
    public int getId() { return id; }
    public PotionEffect createEffect(int duration, int amplifier) {
        return new PotionEffect(this, duration, amplifier);
    }
    @Override public boolean equals(Object other) {
        return other instanceof PotionEffectType type && name.equals(type.name);
    }
    @Override public int hashCode() { return Objects.hash(name); }
    @Override public String toString() { return name; }
}
