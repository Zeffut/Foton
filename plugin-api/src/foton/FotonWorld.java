package foton;

import java.util.UUID;
import org.bukkit.Chunk;
import org.bukkit.Location;
import org.bukkit.NamespacedKey;
import org.bukkit.World;
import org.bukkit.block.Block;

/** A world, as a plugin holds one.
 *
 * A name and a route back into Foton, for the same reason a player is a UUID:
 * a handle that outlives what it names should stop answering rather than keep
 * a dead thing alive.
 */
public final class FotonWorld implements World {
    @Override public long getSeed() { return Native.worldSeed(getName()); }
    @Override public double getCoordinateScale() { return Native.worldCoordinateScale(getName()); }
    @Override public void setDifficulty(org.bukkit.Difficulty difficulty) { if (difficulty != null) Native.setWorldDifficulty(getName(), difficulty.name()); }
    @Override public boolean canGenerateStructures() { return Native.worldCanGenerateStructures(getName()); }
    @Override public void setTicksPerSpawns(org.bukkit.entity.SpawnCategory category, int ticks) { if (category != null) Native.setWorldSpawnTicks(name, category.name(), ticks); }
    @Override public void setSpawnLimit(org.bukkit.entity.SpawnCategory category, int limit) { if (category != null) Native.setWorldSpawnLimit(name, category.name(), limit); }
    @Override public int getMonsterSpawnLimit() { return Native.worldSpawnLimit(name, "MONSTER"); }
    @Override public int getAnimalSpawnLimit() { return Native.worldSpawnLimit(name, "CREATURE"); }
    @Override public int getWaterAnimalSpawnLimit() { return Native.worldSpawnLimit(name, "WATER_CREATURE"); }
    @Override public int getAmbientSpawnLimit() { return Native.worldSpawnLimit(name, "AMBIENT"); }
    @Override public boolean getKeepSpawnInMemory() { return Native.worldKeepSpawnInMemory(name); }
    @Override public void setKeepSpawnInMemory(boolean value) { Native.setWorldKeepSpawnInMemory(name, value); }
    @Override public boolean getPVP() { return Native.worldPvp(getName()); }
    @Override public boolean getAllowMonsters() { return Native.worldAllowMonsters(name); }
    @Override public boolean getAllowAnimals() { return Native.worldAllowAnimals(name); }
    @Override public void setSpawnFlags(boolean monsters, boolean animals) { Native.setWorldAllowMonsters(name, monsters); Native.setWorldAllowAnimals(name, animals); }
    @Override public void setPVP(boolean enabled) { Native.setWorldPvp(getName(), enabled); }
    @Override public org.bukkit.Difficulty getDifficulty() {
        String value = Native.worldDifficulty(getName());
        if (value == null) return org.bukkit.Difficulty.NORMAL;
        try { return org.bukkit.Difficulty.valueOf(value.toUpperCase(java.util.Locale.ROOT)); }
        catch (IllegalArgumentException ignored) { return org.bukkit.Difficulty.NORMAL; }
    }
    @Override public java.util.Collection<org.bukkit.entity.Entity> getNearbyEntities(Location location, double x, double y, double z, java.util.function.Predicate<org.bukkit.entity.Entity> filter) {
        if (location == null) return java.util.List.of();
        String[] ids = Native.worldNearby(name, location.getX(), location.getY(), location.getZ(), x, y, z);
        java.util.ArrayList<org.bukkit.entity.Entity> result = new java.util.ArrayList<>();
        if (ids != null) for (String value : ids) try {
            org.bukkit.entity.Entity entity = FotonEntity.handle(UUID.fromString(value));
            if (filter == null || filter.test(entity)) result.add(entity);
        } catch (IllegalArgumentException ignored) { }
        return java.util.Collections.unmodifiableList(result);
    }
    private final String name;

    public FotonWorld(String name) {
        this.name = name;
    }

    @Override
    public String getName() {
        return name;
    }

    /** A stable id for this world, derived from its name.
     *
     * Foton identifies a world by its key rather than by a UUID, so there is
     * no stored one to hand back. Deriving it from the name gives a plugin
     * what it actually wants -- an identity that is the same across restarts
     * and different between worlds -- without inventing a number and calling
     * it saved.
     */
    @Override
    public UUID getUID() {
        return UUID.nameUUIDFromBytes(
            ("foton:world:" + name).getBytes(java.nio.charset.StandardCharsets.UTF_8));
    }

    @Override
    public NamespacedKey getKey() {
        return NamespacedKey.fromString(name);
    }

    @Override
    public boolean setSpawnLocation(Location location) {
        if (location == null || location.getWorld() != this) return false;
        return Native.setWorldSpawn(name, location.getBlockX(), location.getBlockY(), location.getBlockZ());
    }

    @Override
    public Location getSpawnLocation() {
        double[] at = Native.worldSpawn(name);
        return at == null ? null : new Location(this, at[0], at[1], at[2], (float) at[3],
            (float) at[4]);
    }

    @Override public void playSound(Location location, String sound, float volume, float pitch) {
        if (location != null && sound != null) Native.playSound(name, location.getX(), location.getY(), location.getZ(), sound, volume, pitch);
    }
    @Override public void playSound(Location location, String sound, org.bukkit.SoundCategory category, float volume, float pitch) {
        if (location != null && sound != null) Native.playSoundCategory(name, location.getX(), location.getY(), location.getZ(), sound,
            category == null ? "MASTER" : category.name(), volume, pitch);
    }

    @Override
    public Block getBlockAt(int x, int y, int z) {
        return new FotonBlock(this, x, y, z);
    }

    @Override
    public Block getBlockAt(Location location) {
        return getBlockAt(location.getBlockX(), location.getBlockY(), location.getBlockZ());
    }

    @Override
    public Chunk getChunkAt(int x, int z) {
        return new FotonChunk(this, x, z);
    }

    @Override
    public Chunk getChunkAt(Location location) {
        // A chunk is sixteen blocks wide, and the shift is an arithmetic one so
        // that negative coordinates land in the chunk below rather than
        // rounding toward zero into their neighbour.
        return getChunkAt(location.getBlockX() >> 4, location.getBlockZ() >> 4);
    }

    @Override public org.bukkit.WorldBorder getWorldBorder() {
        return new FotonWorldBorder(this);
    }

    @Override public boolean isChunkLoaded(int x, int z) {
        return Native.worldChunkLoaded(name, x, z);
    }
    @Override public boolean isChunkGenerated(int x, int z) {
        return Native.worldChunkGenerated(name, x, z);
    }
    @Override public String getGameRuleValue(String rule) { return Native.worldGameRule(name, rule); }
    @Override public <T> T getGameRuleDefault(org.bukkit.GameRule<T> rule) { return rule == null ? null : rule.parse(Native.worldGameRuleDefault(name, rule.getName())); }
    @Override public <T> boolean setGameRule(org.bukkit.GameRule<T> rule, T value) { return rule != null && value != null && Native.setWorldGameRule(name, rule.getName(), String.valueOf(value)); }
    @Override public boolean hasStorm() { return Native.worldStorm(name); }
    @Override public boolean isClearWeather() { return !hasStorm() && !isThundering(); }
    @Override public void setStorm(boolean storm) { Native.setWorldStorm(name, storm); }
    @Override public boolean isThundering() { return Native.worldThundering(name); }
    @Override public void setThundering(boolean thundering) { Native.setWorldThundering(name, thundering); }
    @Override public boolean isAutoSave() { return Native.worldAutoSave(name); }
    @Override public void setAutoSave(boolean value) { Native.setWorldAutoSave(name, value); }
    @Override public void save() { Native.saveWorld(name); }

    @Override public java.io.File getWorldFolder() {
        String path = Native.worldFolder(name);
        return path == null ? null : new java.io.File(path);
    }

    @Override public Chunk[] getLoadedChunks() {
        String[] coordinates = Native.worldLoadedChunkCoords(name);
        if (coordinates == null) return new Chunk[0];
        java.util.ArrayList<Chunk> chunks = new java.util.ArrayList<>(coordinates.length);
        for (String coordinate : coordinates) {
            String[] parts = coordinate.split(",", -1);
            try {
                chunks.add(getChunkAt(Integer.parseInt(parts[0]), Integer.parseInt(parts[1])));
            } catch (RuntimeException ignored) { }
        }
        return chunks.toArray(new Chunk[0]);
    }

    @Override
    public org.bukkit.entity.Item dropItem(Location location, org.bukkit.inventory.ItemStack item) {
        if (location == null || item == null) return null;
        String id = Native.worldDropItem(name, location.getX(), location.getY(), location.getZ(), FotonInventory.encode(item));
        if (id == null) return null;
        try { return new FotonItem(UUID.fromString(id)); }
        catch (IllegalArgumentException ignored) { return null; }
    }

    @Override public boolean generateTree(Location location, org.bukkit.TreeType type) {
        if (location == null || type == null || location.getWorld() != this) return false;
        return Native.generateTree(name, location.getBlockX(), location.getBlockY(), location.getBlockZ(), type.name());
    }

    @Override public org.bukkit.entity.LightningStrike strikeLightning(Location location) {
        org.bukkit.entity.Entity entity = spawnEntity(location, org.bukkit.entity.EntityType.fromName("lightning_bolt"));
        return entity instanceof org.bukkit.entity.LightningStrike strike ? strike : null;
    }
    @Override public org.bukkit.entity.LightningStrike strikeLightningEffect(Location location) {
        return strikeLightning(location);
    }

    @Override public org.bukkit.entity.Entity spawnEntity(Location location, org.bukkit.entity.EntityType type) {
        if (location == null || type == null) return null;
        String id = Native.spawnEntity(name, location.getX(), location.getY(), location.getZ(), type.getName());
        try { return id == null ? null : FotonEntity.handle(UUID.fromString(id)); }
        catch (IllegalArgumentException ignored) { return null; }
    }

    @Override public <T extends org.bukkit.entity.Entity> T spawn(Location location, Class<T> clazz) {
        String name = clazz == null ? null : clazz.getSimpleName().replaceFirst("Entity$", "");
        org.bukkit.entity.Entity entity = spawnEntity(location, name == null ? null : org.bukkit.entity.EntityType.fromName(name));
        return clazz != null && clazz.isInstance(entity) ? clazz.cast(entity) : null;
    }

    @Override
    public long getTime() {
        long full = getFullTime();
        return full < 0 ? 0 : full % 24000L;
    }

    @Override
    public void setTime(long time) {
        Native.setWorldTime(name, time);
    }

    @Override
    public long getFullTime() {
        return Native.worldTime(name);
    }

    @Override public int getMinHeight() { return Native.worldMinHeight(name); }
    @Override public int getMaxHeight() { return Native.worldMaxHeight(name); }

    @Override
    public java.util.List<org.bukkit.entity.Player> getPlayers() {
        String[] ids = Native.worldPlayerIds(name);
        if (ids == null) return java.util.List.of();
        java.util.ArrayList<org.bukkit.entity.Player> players = new java.util.ArrayList<>(ids.length);
        for (String id : ids) {
            players.add(new FotonPlayer(UUID.fromString(id)));
        }
        return java.util.Collections.unmodifiableList(players);
    }

    @Override
    public java.util.List<org.bukkit.entity.Entity> getEntities() {
        String[] ids = Native.worldEntityIds(name);
        if (ids == null) return java.util.List.of();
        java.util.ArrayList<org.bukkit.entity.Entity> entities = new java.util.ArrayList<>(ids.length);
        for (String id : ids) {
            try {
                UUID uuid = UUID.fromString(id);
                entities.add(wrapEntity(uuid, id));
            } catch (IllegalArgumentException ignored) {
                // Native UUIDs are validated before they cross this boundary.
            }
        }
        return java.util.Collections.unmodifiableList(entities);
    }

    static org.bukkit.entity.Entity wrapEntity(UUID uuid, String id) {
        String type = Native.entityType(id);
        if ("lightning_bolt".equalsIgnoreCase(type)) return new FotonLightningStrike(uuid);
        if ("player".equalsIgnoreCase(type)) return new FotonPlayer(uuid);
        if ("iron_golem".equalsIgnoreCase(type)) return new FotonIronGolem(uuid);
        if ("copper_golem".equalsIgnoreCase(type)) return new FotonCopperGolem(uuid);
        if ("villager".equalsIgnoreCase(type)) return new FotonVillager(uuid);
        if ("nautilus".equalsIgnoreCase(type)) return new FotonNautilus(uuid);
        if ("zombie_nautilus".equalsIgnoreCase(type)) return new FotonZombieNautilus(uuid);
        if ("ocelot".equalsIgnoreCase(type)) return new FotonOcelot(uuid);
        if ("chicken".equalsIgnoreCase(type)) return new FotonChicken(uuid);
        if ("cow".equalsIgnoreCase(type)) return new FotonCow(uuid);
        if ("piglin".equalsIgnoreCase(type)) return new FotonPiglin(uuid);
        if ("zoglin".equalsIgnoreCase(type)) return new FotonZoglin(uuid);
        if ("tadpole".equalsIgnoreCase(type)) return new FotonTadpole(uuid);
        if ("zombie_villager".equalsIgnoreCase(type)) return new FotonZombieVillager(uuid);
        if ("zombie".equalsIgnoreCase(type)) return new FotonZombie(uuid);
        if ("pig".equalsIgnoreCase(type)) return new FotonPig(uuid);
        if ("fox".equalsIgnoreCase(type)) return new FotonFox(uuid);
        if ("tropical_fish".equalsIgnoreCase(type)) return new FotonTropicalFish(uuid);
        if ("cod".equalsIgnoreCase(type) || "salmon".equalsIgnoreCase(type)
            || "pufferfish".equalsIgnoreCase(type)) return new FotonFish(uuid);
        if ("snow_golem".equalsIgnoreCase(type)) return new FotonGolem(uuid);
        if ("slime".equalsIgnoreCase(type)) return new FotonSlime(uuid);
        if ("creeper".equalsIgnoreCase(type)) return new FotonCreeper(uuid);
        if ("goat".equalsIgnoreCase(type)) return new FotonGoat(uuid);
        if ("axolotl".equalsIgnoreCase(type)) return new FotonAxolotl(uuid);
        if ("parrot".equalsIgnoreCase(type)) return new FotonParrot(uuid);
        if ("donkey".equalsIgnoreCase(type) || "mule".equalsIgnoreCase(type))
            return new FotonChestedHorse(uuid);
        if ("llama".equalsIgnoreCase(type) || "trader_llama".equalsIgnoreCase(type))
            return new FotonLlama(uuid);
        if ("mooshroom".equalsIgnoreCase(type)) return new FotonMushroomCow(uuid);
        if ("frog".equalsIgnoreCase(type)) return new FotonFrog(uuid);
        if ("camel".equalsIgnoreCase(type) || "camel_husk".equalsIgnoreCase(type)) return new FotonCamel(uuid);
        if ("horse".equalsIgnoreCase(type)) return new FotonHorse(uuid);
        if ("sheep".equalsIgnoreCase(type)) return new FotonSheep(uuid);
        if ("wolf".equalsIgnoreCase(type)) return new FotonWolf(uuid);
        if ("cat".equalsIgnoreCase(type)) return new FotonCat(uuid);
        if ("panda".equalsIgnoreCase(type)) return new FotonPanda(uuid);
        if ("phantom".equalsIgnoreCase(type)) return new FotonPhantom(uuid);
        if ("enderman".equalsIgnoreCase(type)) return new FotonEnderman(uuid);
        if ("zombified_piglin".equalsIgnoreCase(type)) return new FotonPigZombie(uuid);
        if (type != null && switch (type.toLowerCase(java.util.Locale.ROOT)) {
            case "evoker", "illusioner" -> true;
            default -> false;
        }) return new FotonSpellcaster(uuid);
        if (type != null && switch (type.toLowerCase(java.util.Locale.ROOT)) {
            case "pillager", "ravager", "vindicator" -> true;
            default -> false;
        }) return new FotonRaider(uuid);
        if ("item_frame".equalsIgnoreCase(type) || "glow_item_frame".equalsIgnoreCase(type))
            return new FotonItemFrame(uuid);
        if ("painting".equalsIgnoreCase(type)) return new FotonPainting(uuid);
        if ("leash_knot".equalsIgnoreCase(type)) return new FotonHanging(uuid);
        if ("armor_stand".equalsIgnoreCase(type)) return new FotonArmorStand(uuid);
        if ("block_display".equalsIgnoreCase(type)) return new FotonBlockDisplay(uuid);
        if ("firework_rocket".equalsIgnoreCase(type)) return new FotonFirework(uuid);
        if ("end_crystal".equalsIgnoreCase(type)) return new FotonEnderCrystal(uuid);
        if ("bee".equalsIgnoreCase(type)) return new FotonBee(uuid);
        if ("experience_orb".equalsIgnoreCase(type)) return new FotonExperienceOrb(uuid);
        if ("item".equalsIgnoreCase(type)) return new FotonItem(uuid);
        if (isBoatType(type)) return new FotonBoat(uuid);
        if ("hopper_minecart".equalsIgnoreCase(type)) return new FotonHopperMinecart(uuid);
        if (isMinecartType(type)) return new FotonMinecart(uuid);
        if ("tnt".equalsIgnoreCase(type)) return new FotonTNTPrimed(uuid);
        if ("experience_bottle".equalsIgnoreCase(type)) return new FotonThrownExpBottle(uuid);
        if ("fishing_bobber".equalsIgnoreCase(type)) return new FotonFishHook(uuid);
        if ("area_effect_cloud".equalsIgnoreCase(type)) return new FotonAreaEffectCloud(uuid);
        if ("splash_potion".equalsIgnoreCase(type) || "lingering_potion".equalsIgnoreCase(type)) return new FotonThrownPotion(uuid);
        if (isVehicleType(type)) return new FotonVehicle(uuid);
        if ("arrow".equalsIgnoreCase(type) || "spectral_arrow".equalsIgnoreCase(type)) return new FotonArrow(uuid);
        if (type != null && switch (type.toLowerCase(java.util.Locale.ROOT)) {
            case "fireball", "small_fireball", "dragon_fireball", "wither_skull" -> true;
            default -> false;
        }) return new FotonFireball(uuid);
        if (isProjectileType(type)) return new FotonProjectile(uuid);
        if (Native.entityIsLiving(id)) {
            if (isFlyingMonsterType(type)) return new FotonFlyingMonster(uuid);
            if (isMonsterType(type)) return new FotonMonster(uuid);
            if (isTameableType(type)) return new FotonTameableEntity(uuid);
            if (isAnimalType(type)) return new FotonAnimal(uuid);
            if (isFlyingType(type)) return new FotonFlying(uuid);
            return new FotonCreature(uuid);
        }
        return new FotonEntity(uuid);
    }

    private static boolean isMinecartType(String type) {
        if (type == null) return false;
        return switch (type.toLowerCase(java.util.Locale.ROOT)) {
            case "minecart", "chest_minecart", "command_block_minecart", "furnace_minecart", "hopper_minecart", "tnt_minecart" -> true;
            default -> false;
        };
    }

    private static boolean isProjectileType(String type) {
        if (type == null) return false;
        return switch (type.toLowerCase(java.util.Locale.ROOT)) {
            case "arrow", "spectral_arrow", "trident", "snowball", "egg", "ender_pearl",
                "fireball", "small_fireball", "dragon_fireball", "wither_skull", "llama_spit",
                "shulker_bullet", "fishing_bobber", "firework_rocket", "wind_charge", "breeze_wind_charge",
                "potion", "experience_bottle", "eye_of_ender", "evoker_fangs" -> true;
            default -> false;
        };
    }

    private static boolean isVehicleType(String type) {
        if (type == null) return false;
        return switch (type.toLowerCase(java.util.Locale.ROOT)) {
            case "acacia_boat", "birch_boat", "cherry_boat", "dark_oak_boat", "jungle_boat",
                "mangrove_boat", "oak_boat", "pale_oak_boat", "spruce_boat", "bamboo_raft",
                "acacia_chest_boat", "birch_chest_boat", "cherry_chest_boat", "dark_oak_chest_boat",
                "jungle_chest_boat", "mangrove_chest_boat", "oak_chest_boat", "pale_oak_chest_boat",
                "spruce_chest_boat", "chest_minecart", "command_block_minecart", "furnace_minecart",
                "hopper_minecart", "minecart", "spawner_minecart", "tnt_minecart" -> true;
            default -> false;
        };
    }

    private static boolean isBoatType(String type) {
        if (type == null) return false;
        return type.toLowerCase(java.util.Locale.ROOT).endsWith("_boat")
            || type.equalsIgnoreCase("bamboo_raft");
    }

    private static boolean isFlyingMonsterType(String type) {
        if (type == null) return false;
        return switch (type.toLowerCase(java.util.Locale.ROOT)) {
            case "blaze", "ghast", "vex" -> true;
            default -> false;
        };
    }

    private static boolean isFlyingType(String type) {
        if (type == null) return false;
        return switch (type.toLowerCase(java.util.Locale.ROOT)) {
            case "allay", "bat" -> true;
            default -> false;
        };
    }

    private static boolean isAnimalType(String type) {
        if (type == null) return false;
        return switch (type.toLowerCase(java.util.Locale.ROOT)) {
            case "armadillo", "chicken", "ocelot", "polar_bear", "rabbit", "sniffer", "strider", "turtle" -> true;
            default -> false;
        };
    }

    private static boolean isMonsterType(String type) {
        if (type == null) return false;
        return switch (type.toLowerCase(java.util.Locale.ROOT)) {
            case "blaze", "breeze", "cave_spider", "drowned", "elder_guardian",
                 "ender_dragon", "endermite", "evoker", "ghast", "guardian",
                 "hoglin", "husk", "magma_cube", "piglin_brute", "silverfish",
                 "skeleton", "spider", "stray", "vex", "warden", "witch",
                 "wither", "wither_skeleton", "zoglin", "zombie", "zombie_villager" -> true;
            default -> false;
        };
    }

    private static boolean isTameableType(String type) {
        if (type == null) return false;
        return switch (type.toLowerCase(java.util.Locale.ROOT)) {
            case "cat", "camel", "camel_husk", "donkey", "horse", "llama", "mule",
                "nautilus", "parrot", "skeleton_horse", "trader_llama", "wolf",
                "zombie_horse" -> true;
            default -> false;
        };
    }

    @Override
    public Environment getEnvironment() {
        return switch (name) {
            case "minecraft:overworld" -> Environment.NORMAL;
            case "minecraft:the_nether" -> Environment.NETHER;
            case "minecraft:the_end" -> Environment.THE_END;
            default -> Environment.CUSTOM;
        };
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof FotonWorld world && name.equals(world.name);
    }

    @Override
    public int hashCode() {
        return name.hashCode();
    }

    @Override
    public String toString() {
        return "FotonWorld{" + name + "}";
    }
}
