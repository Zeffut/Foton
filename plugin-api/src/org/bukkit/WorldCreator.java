package org.bukkit;

import java.util.concurrent.CompletableFuture;
import org.bukkit.generator.BiomeProvider;
import org.bukkit.generator.ChunkGenerator;

/** Stateful world creation options; actual loading is owned by the server. */
public class WorldCreator {
    private String name;
    private long seed;
    private World.Environment environment = World.Environment.NORMAL;
    private WorldType type = WorldType.NORMAL;
    private ChunkGenerator generator;
    private String generatorName;
    private String generatorSettings;
    private BiomeProvider biomeProvider;
    private boolean generateStructures = true;
    private boolean bonusChest;

    public WorldCreator(String name) { this.name = name; }
    public static WorldCreator name(String name) { return new WorldCreator(name); }
    public static WorldCreator ofKey(NamespacedKey key) { return new WorldCreator(key == null ? null : key.toString()); }
    public static WorldCreator ofNameAndKey(String name, NamespacedKey key) {
        return new WorldCreator(name).key(key);
    }
    public WorldCreator key(NamespacedKey key) { this.name = key == null ? name : key.toString(); return this; }
    public String name() { return name; }
    /** JavaBean alias retained by Paper-facing plugins. */
    public String getName() { return name; }
    public WorldCreator nameValue(String value) { this.name = value; return this; }
    public long seed() { return seed; }
    /** JavaBean alias retained by Paper-facing plugins. */
    public long getSeed() { return seed; }
    public WorldCreator seed(long value) { this.seed = value; return this; }
    public World.Environment environment() { return environment; }
    /** JavaBean alias retained by Paper-facing plugins. */
    public World.Environment getEnvironment() { return environment; }
    public WorldCreator environment(World.Environment value) { this.environment = value; return this; }
    public WorldType type() { return type; }
    /** JavaBean alias retained by Paper-facing plugins. */
    public WorldType getType() { return type; }
    public WorldCreator type(WorldType value) { this.type = value; return this; }
    public ChunkGenerator generator() { return generator; }
    /** JavaBean alias retained by Paper-facing plugins. */
    public ChunkGenerator getGenerator() { return generator; }
    public WorldCreator generator(ChunkGenerator value) { this.generator = value; return this; }
    public WorldCreator generator(String value) { this.generatorName = value; return this; }
    /** Returns the named generator selected by {@link #generator(String)}, if any. */
    public String generatorName() { return generatorName; }
    /** JavaBean alias retained by Paper-facing plugins. */
    public String getGeneratorName() { return generatorName; }
    public String generatorSettings() { return generatorSettings; }
    /** JavaBean alias retained by Paper-facing plugins. */
    public String getGeneratorSettings() { return generatorSettings; }
    public WorldCreator generatorSettings(String value) { this.generatorSettings = value; return this; }
    public BiomeProvider biomeProvider() { return biomeProvider; }
    /** JavaBean alias retained by Paper-facing plugins. */
    public BiomeProvider getBiomeProvider() { return biomeProvider; }
    public WorldCreator biomeProvider(BiomeProvider value) { this.biomeProvider = value; return this; }
    public WorldCreator generateStructures(boolean value) { this.generateStructures = value; return this; }
    public boolean generateStructures() { return generateStructures; }
    /** JavaBean alias retained by Paper-facing plugins. */
    public boolean shouldGenerateStructures() { return generateStructures; }
    public WorldCreator bonusChest(boolean value) { this.bonusChest = value; return this; }
    public boolean bonusChest() { return bonusChest; }
    /** JavaBean alias retained by Paper-facing plugins. */
    public boolean shouldCreateBonusChest() { return bonusChest; }

    /**
     * Starts creation without blocking the server thread. Custom Java
     * generators are rejected until a Rust generator bridge exists.
     */
    public WorldCreationRequest createWorldRequest() {
        if (generator != null || biomeProvider != null || generatorSettings != null)
            throw new UnsupportedOperationException("Java generators and biome providers are not bridged by Foton");
        String selected = generatorName;
        if (selected == null || selected.isEmpty()) {
            selected = environment == World.Environment.NETHER ? "minecraft:the_nether"
                : environment == World.Environment.THE_END ? "minecraft:the_end" : "minecraft:overworld";
        } else if (selected.indexOf(':') < 0) {
            selected = "minecraft:" + selected;
        }
        long id = foton.Native.requestWorldCreation(name, selected, seed, bonusChest);
        if (id < 0) throw new IllegalStateException("world creation request was rejected: " + name);
        return new WorldCreationRequest(id, name);
    }

    /** Convenience asynchronous API for plugins such as Multiverse. */
    public CompletableFuture<World> createWorldAsync() {
        return createWorldRequest().future();
    }

    /**
     * Preserves Bukkit's synchronous contract only when creation has already
     * completed; pending creation must use {@link #createWorldAsync()}.
     */
    /** Looks up a biome provider a plugin named in a config file.
     *
     * Bukkit resolves `plugin:id` against the plugin that registered it.
     * Foton has no registry of plugin biome providers, so nothing is found
     * and the caller is told rather than left guessing -- which is also what
     * Bukkit does for a name no plugin claimed.
     */
    public static BiomeProvider getBiomeProviderForName(
            String world, String name, org.bukkit.command.CommandSender output) {
        if (name == null || name.isEmpty()) {
            return null;
        }
        if (output != null) {
            output.sendMessage("No biome provider is registered under " + name
                + "; " + world + " will use its generator's biomes.");
        }
        return null;
    }

    public World createWorld() {
        WorldCreationRequest request = createWorldRequest();
        int state = foton.Native.worldCreationState(request.id());
        if (state == 1) return Bukkit.getWorld(name);
        // The asynchronous poller may have consumed the terminal state
        // between request creation and this synchronous observation.
        World alreadyPublished = Bukkit.getWorld(name);
        if (alreadyPublished != null) return alreadyPublished;
        if (state == 2) throw new IllegalStateException("world creation failed: " + name);
        throw new IllegalStateException("world creation is asynchronous; use createWorldAsync() (request " + request.id() + ")");
    }
}
