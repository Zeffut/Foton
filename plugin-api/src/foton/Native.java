package foton;

import java.util.UUID;

/** Everything the Java side asks Foton for.
 *
 * One class, all static, all native. The Rust side registers these with
 * `RegisterNatives` when it starts the runtime rather than relying on symbol
 * lookup, because Foton is a single binary and its symbols are its own
 * business.
 *
 * Players are named by UUID rather than by a pointer. A pointer would have to
 * stay valid for as long as a plugin kept the object, and plugins keep them for
 * a long time; a UUID that no longer resolves is a player who logged out, which
 * is a thing Bukkit's own API already has an answer for.
 *
 * A position crosses as five doubles in one array rather than five calls. Five
 * calls could each see a different tick, and a plugin that read x from one tick
 * and z from the next would get a point the player was never at.
 */
public final class Native {
    private Native() {}

    public static native String serverName();
    public static native String serverMotd();
    public static native boolean isTagged(String registry, String tag, String value);
    public static native String[] tagValues(String registry, String tag);
    public static native int dyeFireworkColor(int dyeOrdinal);
    public static native boolean enchantmentCanEnchant(String enchantment, String item);

    /** Merges a Vanilla SNBT compound into an item opaque component. */
    public static native String mergeItemSnbt(String existing, String patch);

    public static native String serverVersion();
    public static native String minecraftVersion();

    /** How the server describes itself, the way `/version` prints it. */
    public static native String serverBrand();

    /** Returns tab-separated datapack snapshots: name, compatibility, enabled. */
    public static native String[] datapacks(boolean enabledOnly);

    public static native boolean onlineMode();

    public static native int maxPlayers();
    public static native int serverViewDistance();
    public static native int serverSimulationDistance();
    public static native boolean serverAllowFlight();
    public static native String serverDefaultGameMode();
    public static native double[] serverTps();
    public static native double serverAverageTickTime();

    /** Whether this call is running on Foton's serialized game-tick thread. */
    public static native boolean isPrimaryThread();

    /** Requests the server's normal graceful shutdown sequence. */
    public static native void shutdown();
    public static native void savePlayers();

    /** The UUIDs of everyone online, in no promised order. */
    public static native String[] onlinePlayerIds();
    public static native String[] knownPlayerIds();
    public static native String knownPlayerIdByName(String name);
    public static native String[] worldPlayerIds(String world);
    public static native String[] worldEntityIds(String world);
    public static native String requestChunk(String world, int x, int z);
    public static native boolean chunkRequestReady(String request);
    public static native boolean worldChunkLoaded(String world, int x, int z);
    public static native boolean worldChunkGenerated(String world, int x, int z);
    public static native String[] chunkBlockEntities(String world, int x, int z);
    public static native String areaEffectCloudSource(String uuid);
    public static native String areaEffectCloudBasePotionType(String uuid);
    public static native float areaEffectCloudRadius(String uuid);
    public static native void setAreaEffectCloudRadius(String uuid, float radius);
    public static native int areaEffectCloudDuration(String uuid);
    public static native void setAreaEffectCloudDuration(String uuid, int ticks);
    public static native int areaEffectCloudWaitTime(String uuid);
    public static native void setAreaEffectCloudWaitTime(String uuid, int ticks);
    public static native int areaEffectCloudReapplicationDelay(String uuid);
    public static native void setAreaEffectCloudReapplicationDelay(String uuid, int ticks);
    public static native float areaEffectCloudRadiusPerTick(String uuid);
    public static native void setAreaEffectCloudRadiusPerTick(String uuid, float amount);
    public static native float areaEffectCloudRadiusOnUse(String uuid);
    public static native void setAreaEffectCloudRadiusOnUse(String uuid, float amount);
    public static native void setFireworkMeta(String uuid, int power, String effects);
    public static native String fireworkMeta(String uuid);
    public static native String worldGameRule(String world, String rule);
    public static native String worldGameRuleDefault(String world, String rule);
    public static native boolean setWorldGameRule(String world, String rule, String value);
    public static native void setWorldSpawnLimit(String world, String category, int limit);
    public static native void setWorldSpawnTicks(String world, String category, int ticks);
    public static native int worldSpawnLimit(String world, String category);
    public static native boolean worldKeepSpawnInMemory(String world);
    public static native void setWorldKeepSpawnInMemory(String world, boolean value);
    public static native boolean worldStorm(String world);
    public static native void setWorldStorm(String world, boolean storm);
    public static native boolean worldHasBonusChest(String world);
    public static native int worldWeatherDuration(String world);
    public static native void setWorldWeatherDuration(String world, int ticks);
    public static native int worldThunderDuration(String world);
    public static native void setWorldThunderDuration(String world, int ticks);
    public static native boolean worldThundering(String world);
    public static native void setWorldThundering(String world, boolean thundering);
    public static native String spawnEntity(String world, double x, double y, double z, String type);
    public static native String[] signLines(String world, int x, int y, int z);
    public static native String hopperCustomName(String world, int x, int y, int z);
    public static native String hopperInventorySlot(String world, int x, int y, int z, int slot);
    public static native boolean jukeboxIsPlaying(String world, int x, int y, int z);
    public static native String jukeboxRecord(String world, int x, int y, int z);
    public static native void jukeboxSetRecord(String world, int x, int y, int z, String item);
    public static native void hopperSetInventorySlot(String world, int x, int y, int z, int slot, String item);
    public static native void hopperSetCustomName(String world, int x, int y, int z, String name);
    public static native void signSetLine(String world, int x, int y, int z, String line, int index);
    public static native void signSetColor(String world, int x, int y, int z, int color);
    public static native int signColor(String world, int x, int y, int z, boolean front);
    public static native void signSetGlowing(String world, int x, int y, int z, boolean front, boolean glowing);
    public static native boolean signGlowing(String world, int x, int y, int z, boolean front);
    public static native boolean signIsWaxed(String world, int x, int y, int z);
    public static native void signSetWaxed(String world, int x, int y, int z, boolean waxed);
    public static native String spawnerEntityType(String world, int x, int y, int z);
    public static native void setSpawnerEntityType(String world, int x, int y, int z, String type);
    public static native int spawnerDelay(String world, int x, int y, int z);
    public static native void setSpawnerDelay(String world, int x, int y, int z, int delay);
    public static native int spawnerMinSpawnDelay(String world, int x, int y, int z);
    public static native void setSpawnerMinSpawnDelay(String world, int x, int y, int z, int delay);
    public static native int spawnerMaxSpawnDelay(String world, int x, int y, int z);
    public static native void setSpawnerMaxSpawnDelay(String world, int x, int y, int z, int delay);
    public static native String horseMarkings(String uuid);
    public static native void setHorseMarkings(String uuid, String markings);
    public static native String horseVariant(String uuid);
    public static native void setHorseVariant(String uuid, String variant);
    public static native String wolfVariant(String uuid);
    public static native void setWolfVariant(String uuid, String variant);
    public static native boolean wolfSitting(String uuid);
    public static native void setWolfSitting(String uuid, boolean sitting);
    public static native int wolfCollarColor(String uuid);
    public static native void setWolfCollarColor(String uuid, int value);
    public static native String catVariant(String uuid);
    public static native void setCatVariant(String uuid, String variant);
    public static native boolean catSitting(String uuid);
    public static native void setCatSitting(String uuid, boolean sitting);
    public static native int catCollarColor(String uuid);
    public static native void setCatCollarColor(String uuid, int value);
    public static native boolean endCrystalShowsBottom(String uuid);
    public static native void setEndCrystalShowsBottom(String uuid, boolean showing);
    public static native int beeAnger(String uuid);
    public static native void setBeeAnger(String uuid, int anger);
    public static native boolean beeHasNectar(String uuid);
    public static native void setBeeHasNectar(String uuid, boolean value);
    public static native void armorStandSetArms(String uuid, boolean value);
    public static native boolean beeHasStung(String uuid);
    public static native void setBeeHasStung(String uuid, boolean value);
    public static native int horseTemper(String uuid);
    public static native void setHorseTemper(String uuid, int value);
    public static native int horseMaxTemper(String uuid);
    public static native String pandaMainGene(String uuid);
    public static native void setPandaMainGene(String uuid, String gene);
    public static native String pandaHiddenGene(String uuid);
    public static native void setPandaHiddenGene(String uuid, String gene);
    public static native boolean raiderPatrolLeader(String uuid);
    public static native void setRaiderPatrolLeader(String uuid, boolean leader);
    public static native int phantomSize(String uuid);
    public static native void setPhantomSize(String uuid, int size);
    public static native String llamaVariant(String uuid);
    public static native void setLlamaVariant(String uuid, String variant);
    public static native boolean generateTree(String world, int x, int y, int z, String type);
    public static native String[] signSideLines(String world, int x, int y, int z, boolean front);
    public static native void signSideSetLine(String world, int x, int y, int z, String line, int index, boolean front);
    public static native String[] bannerPatterns(String world, int x, int y, int z);
    public static native boolean setBannerPatterns(String world, int x, int y, int z, String encoded);
    public static native String[] worldLoadedChunkCoords(String world);
    public static native String worldFolder(String world);
    public static native boolean worldAutoSave(String world);
    public static native void setWorldAutoSave(String world, boolean value);
    public static native void saveWorld(String world);
    public static native String worldDropItem(String world, double x, double y, double z, String item);
    public static native String[] scoreboardTeamEntries(String world, String team);
    public static native String scoreboardEntryTeam(String world, String entry);

    /** A player's name, or null once they are gone. */
    public static native String playerName(String uuid);
    public static native String playerLocale(String uuid);
    public static native boolean hasPlayedBefore(String uuid);
    public static native long firstPlayed(String uuid);
    public static native long lastPlayed(String uuid);
    public static native String playerKiller(String uuid);
    public static native String customName(String uuid);
    public static native void setCustomName(String uuid, String name);
    public static native int playerFoodLevel(String uuid);
    public static native long worldSeed(String world);
    public static native double worldCoordinateScale(String world);
    public static native boolean worldCanGenerateStructures(String world);
    public static native String worldDifficulty(String world);
    public static native void setWorldDifficulty(String world, String difficulty);
    public static native boolean worldPvp(String world);
    public static native boolean worldAllowMonsters(String world);
    public static native void setWorldAllowMonsters(String world, boolean value);
    public static native boolean worldAllowAnimals(String world);
    public static native void setWorldAllowAnimals(String world, boolean value);
    public static native void setWorldPvp(String world, boolean enabled);
    public static native float entityFallDistance(String uuid);
    public static native void setEntityFallDistance(String uuid, float distance);
    public static native void setCompassTarget(String uuid, String world, int x, int y, int z);
    public static native float playerFoodSaturation(String uuid);
    public static native float playerFoodExhaustion(String uuid);
    public static native void setPlayerFood(String uuid, int food, float saturation, float exhaustion);
    public static native int playerPing(String uuid);
    public static native void setPlayerOperator(String uuid, boolean operator);
    public static native float playerWalkSpeed(String uuid);
    public static native void setPlayerWalkSpeed(String uuid, float speed);
    public static native float playerFlySpeed(String uuid);
    public static native void setPlayerFlySpeed(String uuid, float speed);
    public static native boolean addPotionEffect(String uuid, String type, int duration, int amplifier);
    public static native void removePotionEffect(String uuid, String type);
    public static native double health(String uuid);
    public static native void setHealth(String uuid, double health);
    public static native double maxHealth(String uuid);
    public static native void setAttributeBase(String uuid, String attribute, double value);
    public static native int airSupply(String uuid);
    public static native void setAirSupply(String uuid, int ticks);
    public static native int maxAirSupply(String uuid);
    public static native String[] entityPotionEffects(String uuid);
    public static native String[] areaEffectCloudEffects(String uuid);
    public static native boolean addAreaEffectCloudEffect(String uuid, String type, int duration, int amplifier, boolean ambient, boolean particles, boolean icon, boolean override);
    public static native void clearAreaEffectCloudEffects(String uuid);
    public static native String[] arrowCustomEffects(String uuid);
    public static native String arrowPotion(String uuid);
    public static native int arrowPotionColor(String uuid);
    public static native boolean entityRemoveWhenFarAway(String uuid);
    public static native boolean entityPersistent(String uuid);
    public static native void setEntityPersistent(String uuid, boolean persistent);
    public static native void setEntityRemoveWhenFarAway(String uuid, boolean remove);
    public static native float entityDropChance(String uuid, int slot);
    public static native void setEntityDropChance(String uuid, int slot, float chance);
    public static native int experienceLevel(String uuid);
    public static native float experienceProgress(String uuid);
    public static native void setExperienceLevel(String uuid, int level);
    public static native void setExperienceProgress(String uuid, float progress);
    public static native int totalExperience(String uuid);
    public static native void setTotalExperience(String uuid, int total);
    public static native void giveExperience(String uuid, int amount);

    /** The UUID of an online player with this name, or null. */
    public static native String playerIdByName(String name);

    /** Sends a player a chat message. Silently does nothing once they are gone. */
    public static native void sendMessage(String uuid, String message);

    /** Submits an unsigned chat message through the normal player-chat pipeline. */
    public static native void chat(String uuid, String message);

    /** Disconnects an online player with the supplied message. */
    public static native void kickPlayer(String uuid, String message);

    public static native void setPlayerListName(String uuid, String name);
    public static native void setPlayerListHeader(String uuid, String header);
    public static native void setPlayerListFooter(String uuid, String footer);
    public static native void setPlayerListHeaderFooter(String uuid, String header, String footer);

    public static native void sendActionBar(String uuid, String message);
    public static native void sendTitle(String uuid, String title, String subtitle,
            int fadeIn, int stay, int fadeOut);
    public static native void clearTitle(String uuid);

    /** Sends one custom payload packet to one online player. */
    public static native void sendPluginMessage(String uuid, String channel, byte[] message);
    public static native void sendBlockChange(String uuid, String world, int x, int y, int z, String block);
    public static native void sendSignChange(String uuid, String world, int x, int y, int z, String[] lines, int color);

    /** Sends everyone a chat message, and says how many heard it. */
    public static native int broadcast(String message);

    /** The name of the world a player is in, or null once they are gone. */
    public static native String playerWorld(String uuid);
    public static native void playerEntityEffect(String uuid, String effect);
    public static native String playerAddress(String uuid);
    public static native String[] advancementCriteria(String key);
    public static native String[] advancementDisplay(String key);
    public static native String playerRespawnWorld(String uuid);
    public static native double[] playerRespawnPosition(String uuid);
    public static native void setPlayerRespawnPosition(String uuid, String world, int x, int y, int z, float yaw, float pitch);
    public static native String entityWorld(String uuid);
    public static native void removeEntity(String uuid);
    public static native String spellcasterSpell(String uuid);
    public static native void setSpellcasterSpell(String uuid, String spell);
    public static native String projectileShooter(String uuid);
    public static native void setProjectileShooter(String uuid, String owner);
    public static native String entityType(String uuid);
    public static native String hangingFacing(String uuid);
    public static native boolean setHangingFacing(String uuid, String face, boolean force);
    public static native String paintingArt(String uuid);
    public static native boolean setPaintingArt(String uuid, String art, boolean force);
    public static native String endermanCarriedBlock(String uuid);
    public static native void setEndermanCarriedBlock(String uuid, String block);
    public static native String entityItemStack(String uuid);
    public static native String entityTntSource(String uuid);
    public static native void setEntityItemStack(String uuid, String item);
    public static native void setItemUnlimitedLifetime(String uuid, boolean unlimited);
    public static native int itemAge(String uuid);
    public static native void setItemAge(String uuid, int age);
    public static native void resetWorldBorder(String world);
    public static native double[] worldBorder(String world);
    public static native void setWorldBorderCenter(String world, double x, double z);
    public static native void setWorldBorderSize(String world, double size);
    public static native void setWorldBorderLerp(String world, double oldSize, double newSize, long ticks);
    public static native int worldBorderWarningDistance(String world);
    public static native void setWorldBorderWarningDistance(String world, int distance);
    public static native int worldBorderWarningTime(String world);
    public static native void setWorldBorderWarningTime(String world, int ticks);
    public static native double worldBorderDamageAmount(String world);
    public static native void setWorldBorderDamageAmount(String world, double amount);
    public static native double worldBorderDamageBuffer(String world);
    public static native void setWorldBorderDamageBuffer(String world, double distance);
    public static native boolean entityIsLiving(String uuid);
    public static native String entityTarget(String uuid);
    public static native void setEntityTarget(String uuid, String target);
    public static native boolean entityIsFallFlying(String uuid);
    public static native int experienceOrbExperience(String uuid);
    public static native void setExperienceOrbExperience(String uuid, int experience);
    public static native boolean entityIsTamed(String uuid);
    public static native boolean wolfAngry(String uuid);
    public static native void setWolfAngry(String uuid, boolean angry);
    public static native void setEntityTamed(String uuid, boolean tamed);
    public static native String entityOwner(String uuid);
    public static native void setEntityOwner(String uuid, String owner);
    public static native String villagerType(String uuid);
    public static native void setVillagerType(String uuid, String type);
    public static native String villagerProfession(String uuid);
    public static native int villagerExperience(String uuid);
    public static native String[] villagerMemory(String uuid, String key);
    public static native boolean setVillagerMemory(String uuid, String key, String world, int x, int y, int z);
    public static native void clearVillagerMemory(String uuid, String key);
    public static native void setVillagerExperience(String uuid, int experience);
    public static native int villagerLevel(String uuid);
    public static native void setVillagerLevel(String uuid, int level);
    public static native void resetVillagerOffers(String uuid);
    public static native void setVillagerOffers(String uuid, String[] offers);
    public static native String zombieVillagerProfession(String uuid);
    public static native void setZombieVillagerProfession(String uuid, String profession);
    public static native void setZombieVillager(String uuid, boolean villager);
    public static native String foxType(String uuid);
    public static native void setFoxType(String uuid, String type);
    public static native boolean foxSitting(String uuid);
    public static native void setFoxSitting(String uuid, boolean sitting);
    public static native int tropicalFishPatternColor(String uuid);
    public static native void setTropicalFishPatternColor(String uuid, int color);
    public static native void setTropicalFishPattern(String uuid, String pattern);
    public static native String tropicalFishPattern(String uuid);
    public static native void setAxolotlVariant(String uuid, String variant);
    public static native String axolotlVariant(String uuid);
    public static native void setParrotVariant(String uuid, String variant);
    public static native String parrotVariant(String uuid);
    public static native void setMushroomCowVariant(String uuid, String variant);
    public static native String mushroomCowVariant(String uuid);
    public static native void setFrogVariant(String uuid, String variant);
    public static native String frogVariant(String uuid);
    public static native void setChickenVariant(String uuid, String variant);
    public static native String chickenVariant(String uuid);
    public static native void setPigVariant(String uuid, String variant);
    public static native String pigVariant(String uuid);
    public static native void setZombieNautilusVariant(String uuid, String variant);
    public static native String zombieNautilusVariant(String uuid);
    public static native int tropicalFishBodyColor(String uuid);
    public static native void setTropicalFishBodyColor(String uuid, int color);
    public static native int slimeSize(String uuid);
    public static native void setSlimeSize(String uuid, int size);
    public static native void setCreeperPowered(String uuid, boolean powered);
    public static native boolean creeperPowered(String uuid);
    public static native void setGoatScreaming(String uuid, boolean screaming);
    public static native boolean goatLeftHorn(String uuid);
    public static native void setGoatLeftHorn(String uuid, boolean present);
    public static native boolean goatRightHorn(String uuid);
    public static native void setGoatRightHorn(String uuid, boolean present);
    public static native boolean goatScreaming(String uuid);
    public static native int sheepColor(String uuid);
    public static native void setSheepColor(String uuid, int color);
    public static native boolean sheepSheared(String uuid);
    public static native void setSheepSheared(String uuid, boolean sheared);
    public static native boolean entityIsBaby(String uuid);
    public static native boolean entityCanBreed(String uuid);
    public static native void setEntityBreed(String uuid, boolean breed);
    public static native int entityAge(String uuid);
    public static native void setEntityAge(String uuid, int age);
    public static native boolean entityCanPickupItems(String uuid);
    public static native void setEntityCanPickupItems(String uuid, boolean pickup);
    public static native int enchantmentMaxLevel(String key);
    public static native boolean entityHasChest(String uuid);
    public static native void entitySetChest(String uuid, boolean carrying);
    public static native void entitySetBaby(String uuid, boolean baby);
    public static native boolean entityAgeLock(String uuid);
    public static native void setEntityAgeLock(String uuid, boolean locked);
    public static native boolean pigHasSaddle(String uuid);
    public static native void pigSetSaddle(String uuid, boolean saddled);
    public static native String horseInventorySlot(String uuid, int slot);
    public static native String mountInventorySlot(String uuid, int slot);
    public static native void setMountInventorySlot(String uuid, int slot, String item);
    public static native void setHorseInventorySlot(String uuid, int slot, String item);
    public static native void setBlockDisplayBlock(String uuid, String state);
    public static native String boatType(String uuid);
    public static native void setBoatType(String uuid, String type);
    public static native boolean entityEject(String uuid);
    public static native String entityVehicle(String uuid);
    public static native boolean entityLeaveVehicle(String uuid);
    public static native String entityPassengers(String uuid);
    public static native boolean entityAddPassenger(String vehicle, String passenger);
    public static native boolean entityRemovePassenger(String vehicle, String passenger);
    public static native String entitySpawnCategory(String uuid);
    public static native String entitySpawnReason(String uuid);
    public static native double[] entityPosition(String uuid);
    public static native double[] entityOrigin(String uuid);
    public static native double[] entityBoundingBox(String uuid);
    public static native boolean entityOnGround(String uuid);
    public static native boolean entityInWater(String uuid);
    public static native boolean entityInvisible(String uuid);
    public static native boolean entityInvulnerable(String uuid);
    public static native void setEntityInvulnerable(String uuid, boolean invulnerable);
    public static native boolean entityGlowing(String uuid);
    public static native void setEntityGlowing(String uuid, boolean glowing);
    public static native int entityNoDamageTicks(String uuid);
    public static native int entityFreezeTicks(String uuid);
    public static native void setEntityFreezeTicks(String uuid, int ticks);
    public static native boolean entitySprinting(String uuid);
    public static native boolean entitySwimming(String uuid);
    public static native boolean entityIsUsingItem(String uuid);
    public static native void entityClearActiveItem(String uuid);
    public static native void entitySetNoDamageTicks(String uuid, int ticks);
    public static native String[] entityNearby(String uuid, double x, double y, double z);
    public static native String[] entityTrackedBy(String uuid);
    public static native String[] worldNearby(String world, double x, double y, double z, double radiusX, double radiusY, double radiusZ);
    public static native void playerHideEntity(String player, String entity, boolean hidden);
    public static native boolean playerCanSeeEntity(String player, String entity);
    public static native double[] entityVelocity(String uuid);
    public static native void setEntityVelocity(String uuid, double x, double y, double z);
    public static native int entityFireTicks(String uuid);
    public static native void setEntityFireTicks(String uuid, int ticks);
    public static native double entityEyeHeight(String uuid);
    public static native int entityPortalCooldown(String uuid);
    public static native void setEntityPortalCooldown(String uuid, int ticks);
    public static native int entityId(String uuid);
    public static native String entityProjectileOwner(String uuid);
    public static native boolean setEntityProjectileOwner(String uuid, String owner);
    public static native String entityCustomName(String uuid);
    public static native boolean entityCustomNameVisible(String uuid);
    public static native void setEntityCustomNameVisible(String uuid, boolean visible);
    public static native boolean ironGolemPlayerCreated(String uuid);
    public static native void setIronGolemPlayerCreated(String uuid, boolean value);
    public static native String[] entityMerchantRecipes(String uuid);
    public static native boolean entitySetMerchantOfferUses(String uuid, int index, int uses);
    public static native boolean entitySetMerchantOfferMaxUses(String uuid, int index, int maxUses);
    public static native boolean entitySetMerchantOfferDemand(String uuid, int index, int demand);

    public static native void setEntityCustomName(String uuid, String name);
    public static native void entitySendMessage(String uuid, String message);

    /** Whether a player holds a permission. */
    public static native boolean hasPermission(String uuid, String permission);
    public static native String[] effectivePermissions(String uuid);
    public static native boolean isPermissionSet(String uuid, String permission);

    /** A player's position as {x, y, z, yaw, pitch}, or null once they are gone. */
    public static native double[] playerPosition(String uuid);

    /** A player's game mode, lower case, or null once they are gone. */
    public static native int openMenuSlotCount(String uuid);
    public static native int openMenuTopSlotCount(String uuid);
    public static native String openMenuSlot(String uuid, int slot);
    public static native boolean setOpenMenuSlot(String uuid, int slot, String item);
    public static native String openMenuType(String uuid);
    public static native String openMenuTitle(String uuid);
    public static native void updateInventory(String uuid);
    public static native void closeInventory(String uuid);
    public static native String gameMode(String uuid);
    public static native boolean setGameMode(String uuid, String mode);
    public static native boolean allowFlight(String uuid);
    public static native void setAllowFlight(String uuid, boolean value);
    public static native boolean isFlying(String uuid);
    public static native void setFlying(String uuid, boolean value);
    public static native boolean isSleepingIgnored(String uuid);
    public static native void setSleepingIgnored(String uuid, boolean value);
    public static native void openGenericInventory(String uuid, int size, String title, String contents);
    public static native boolean openSmithingTable(String uuid, String world, int x, int y, int z);
    public static native boolean openLoom(String uuid, String world, int x, int y, int z);
    public static native boolean openWorkbench(String uuid, String world, int x, int y, int z);
    public static native boolean openGrindstone(String uuid, String world, int x, int y, int z);
    public static native boolean openStonecutter(String uuid, String world, int x, int y, int z);
    public static native boolean openAnvil(String uuid, String world, int x, int y, int z);
    public static native boolean openCartographyTable(String uuid, String world, int x, int y, int z);

    /** One inventory slot as `minecraft:name count`, or the empty string.
     *
     * A string rather than an object: building a Java object from Rust means
     * naming a constructor by signature, and a signature that drifts is a
     * NoSuchMethodError at the worst possible moment.
     */
    public static native String inventorySlot(String uuid, int slot);

    /** Writes one inventory slot. An empty string empties it. */
    public static native void setInventorySlot(String uuid, int slot, String item);
    public static native String enderChestSlot(String uuid, int slot);
    public static native void setEnderChestSlot(String uuid, int slot, String item);

    /** Which hotbar slot a player is holding, or -1 once they are gone. */
    public static native int heldSlot(String uuid);

    /** Whether a player is an operator. */
    public static native boolean isOperator(String uuid);
    public static native int statisticValue(String uuid, String statistic);
    public static native int offlineStatistic(String uuid, String statistic);
    public static native boolean offlineIsOperator(String uuid);
    public static native boolean offlineIsWhitelisted(String uuid);
    public static native boolean isWhitelisted(String uuid);
    public static native void setPlayerWhitelisted(String uuid, boolean value);
    public static native boolean isSneaking(String uuid);
    public static native void openBook(String uuid);
    public static native boolean teleport(String uuid, String world, double x, double y,
            double z, float yaw, float pitch);
    public static native boolean teleportEntity(String uuid, String world, double x, double y, double z, float yaw, float pitch);

    /** Creates a native boss event and returns its opaque UUID handle. */
    public static native String createBossBar(String title, int color, int style, int flags);

    public static native void releaseBossBar(String id);
    public static native void bossBarSetTitle(String id, String title);
    public static native void bossBarSetColor(String id, int color);
    public static native void bossBarSetStyle(String id, int style);
    public static native void bossBarSetFlags(String id, int flags);
    public static native void bossBarSetProgress(String id, double progress);
    public static native void bossBarAddPlayer(String id, String player);
    public static native void bossBarRemovePlayer(String id, String player);
    public static native void bossBarRemoveAll(String id);
    public static native String[] bossBarPlayerIds(String id);
    public static native void bossBarSetVisible(String id, boolean visible);


    /** Plays a sound at a point in a world, for everyone who can hear it. */
    public static native void playSound(
        String world, double x, double y, double z, String sound, float volume, float pitch);
    public static native void playSoundCategory(
        String world, double x, double y, double z, String sound, String category, float volume, float pitch);
    public static native void stopSound(String uuid, String sound, String category);

    /** One block as `minecraft:name[state=value]`, or null if unreadable. */
    public static native String blockPistonReaction(String world, int x, int y, int z);
    public static native String blockState(String world, int x, int y, int z);
    public static native String biomeKey(String world, int x, int y, int z);
    public static native String recipeResult(String key);
    public static native String[] recipeList();
    public static native String itemTranslationKey(String item);
    public static native boolean recipeRemove(String key);
    public static native boolean recipeAddShapeless(String key, String result, int count, String[] ingredients);
    public static native boolean recipeAddShaped(String key, String result, int count, String[] shape, String[] ingredients);
    public static native byte blockLight(String world, int x, int y, int z);
    public static native boolean blockIndirectlyPowered(String world, int x, int y, int z);
    public static native byte skyLight(String world, int x, int y, int z);
    public static native boolean blockPassable(String world, int x, int y, int z);
    public static native String lecternBook(String world, int x, int y, int z);
    public static native String[] lecternBookPages(String world, int x, int y, int z);
    public static native void lecternClearBook(String world, int x, int y, int z);
    public static native boolean lecternSetBook(String world, int x, int y, int z, String item);

    /** Writes one block from the same text. */
    public static native void setBlock(String world, int x, int y, int z, String state);
    public static native boolean breakBlock(String world, int x, int y, int z);

    /** Every loaded world's key, in no promised order. */
    public static native boolean unloadWorld(String world, boolean save);
    public static native String[] worldNames();
    /** Starts asynchronous world creation; returns request id, or -1 on validation failure. */
    public static native long requestWorldCreation(String name, String generator, long seed, boolean bonusChest);
    /** Returns 0=pending, 1=ready, 2=failed, -1=unknown request. */
    public static native int worldCreationState(long requestId);

    /** A world's spawn as {x, y, z, yaw, pitch}, or null if there is no such world. */
    public static native double[] worldSpawn(String world);
    public static native boolean setWorldSpawn(String world, int x, int y, int z);

    /** A world's time of day, or -1 if there is no such world. */
    public static native long worldTime(String world);
    public static native void setWorldTime(String world, long time);
    public static native boolean createExplosion(String name, double x, double y, double z, float power);
    public static native boolean createExplosionAdvanced(String name, double x, double y, double z, float power, boolean fire, boolean breakBlocks);
    public static native int worldMinHeight(String world);
    public static native int worldMaxHeight(String world);

    // Entities, players and world queries (plugin compatibility, lot B).
    // Attributes are named by registry key, modifiers by their full key.
    public static native double[] attributeValues(String uuid, String attribute);
    public static native String[] attributeModifierList(String uuid, String attribute);
    public static native boolean addAttributeModifierKeyed(String uuid, String attribute, String key, double amount, String operation, boolean persistent);
    public static native boolean removeAttributeModifierKeyed(String uuid, String attribute, String key);

    /** Particles in a world: to every player in range when {@code receivers} is null. */
    public static native void spawnParticles(String world, String[] receivers, String particle, double x, double y, double z, int count, double offsetX, double offsetY, double offsetZ, double extra, String data, boolean force);
    /** Particles sent to one player, wherever they are. */
    public static native void playerParticles(String uuid, String particle, double x, double y, double z, int count, double offsetX, double offsetY, double offsetZ, double extra, String data, boolean force);

    /** An entity's container -- a mob's carried inventory or a chest boat's
     * chest: -1 when it has none. */
    public static native int entityContainerSize(String uuid);
    public static native String entityContainerSlot(String uuid, int slot);
    public static native void setEntityContainerSlot(String uuid, int slot, String item);
    /** A mannequin's profile as {@code [id, name, (name, value, signature)...]}. */
    public static native String[] mannequinProfile(String uuid);
    public static native void setMannequinProfile(String uuid, String profileId, String name, String[] properties);
    public static native boolean mannequinImmovable(String uuid);
    public static native void setMannequinImmovable(String uuid, boolean immovable);
    public static native void setMannequinDescription(String uuid, String json);
    /** {@code {raw peek, 0, dye colour id or -1}}. */
    public static native double[] shulkerState(String uuid);
    public static native String shulkerAttachedFace(String uuid);
    public static native void setShulkerAttachedFace(String uuid, String face);
    public static native void setShulkerPeek(String uuid, int peek);
    public static native void setShulkerColor(String uuid, int color);
    /** Every {@code Display} field at once; see {@code FotonDisplay} for the layout. */
    public static native double[] displayState(String uuid);
    public static native void setDisplayInterpolationDuration(String uuid, int ticks);
    public static native void setDisplayInterpolationDelay(String uuid, int ticks);
    public static native void setDisplayTeleportDuration(String uuid, int ticks);
    public static native void setDisplayBillboard(String uuid, int billboard);
    public static native void setDisplayBrightness(String uuid, int block, int sky);
    public static native void setDisplayGlowColor(String uuid, int argb);
    public static native void setDisplayFloat(String uuid, int field, float value);
    public static native void setDisplayTransformation(String uuid, float tx, float ty, float tz,
            float sx, float sy, float sz, float lx, float ly, float lz, float lw,
            float rx, float ry, float rz, float rw);
    public static native String blockDisplayBlock(String uuid);
    public static native String itemDisplayItem(String uuid);
    public static native void setItemDisplayItem(String uuid, String item);
    public static native String itemDisplayTransform(String uuid);
    public static native void setItemDisplayTransform(String uuid, String transform);
    public static native void setTextDisplayText(String uuid, String json);
    public static native String textDisplayPlainText(String uuid);
    /** {@code {line width, background ARGB, text opacity, style flags}}. */
    public static native double[] textDisplayState(String uuid);
    public static native void setTextDisplayLineWidth(String uuid, int width);
    public static native void setTextDisplayBackground(String uuid, int argb);
    public static native void resetTextDisplayBackground(String uuid);
    public static native void setTextDisplayOpacity(String uuid, byte opacity);
    public static native void setTextDisplayFlag(String uuid, String flag, boolean enabled);
    public static native void setTextDisplayAlignment(String uuid, String alignment);
    /** {@code {width, height, responsive}}. */
    public static native double[] interactionState(String uuid);
    /** Width (0), height (1) or responsiveness (2). */
    public static native void setInteractionValue(String uuid, int field, float value);

    public static native String[] entityTags(String uuid);
    public static native boolean addEntityTag(String uuid, String tag);
    public static native boolean removeEntityTag(String uuid, String tag);
    /** Sets a custom name from vanilla component JSON; null clears it. */
    public static native void setEntityCustomNameComponent(String uuid, String json);
    public static native boolean entityGravity(String uuid);
    public static native void setEntityGravity(String uuid, boolean gravity);
    public static native boolean entitySilent(String uuid);
    public static native void setEntitySilent(String uuid, boolean silent);
    public static native void setEntityRotation(String uuid, float yaw, float pitch);
    public static native int entityPose(String uuid);
    public static native void damageEntity(String uuid, double amount, String source);
    public static native boolean entityHasAi(String uuid);
    public static native void setEntityAi(String uuid, boolean ai);
    public static native boolean mobAware(String uuid);
    public static native void setMobAware(String uuid, boolean aware);
    public static native boolean entityCollidable(String uuid);
    public static native void setEntityCollidable(String uuid, boolean collidable);
    public static native String itemThrower(String uuid);
    public static native String itemOwner(String uuid);
    public static native void setItemOwner(String uuid, String owner);
    public static native int itemPickupDelay(String uuid);
    public static native void setItemPickupDelay(String uuid, int delay);
    public static native void setItemFrameItem(String uuid, String item, boolean playSound);
    public static native String fireworkAttachedTo(String uuid);
    /** An equipment slot by Bukkit {@code EquipmentSlot} ordinal. */
    public static native String entityEquipmentItem(String uuid, int slot);
    public static native void setEntityEquipmentItem(String uuid, int slot, String item);

    /** The entity as Paper's serializeEntity writes it: gzipped vanilla NBT. */
    public static native byte[] serializeEntity(String uuid);
    /** Builds an unspawned entity from such a blob; the UUID it is held under. */
    public static native String deserializeEntity(byte[] data, String world, boolean preserveUuid);
    public static native boolean entityIsPending(String uuid);
    public static native boolean spawnPendingEntity(String uuid, String world, double x, double y, double z, float yaw, float pitch, String reason);
    public static native boolean mobMoveTo(String uuid, double x, double y, double z, double speed);
    public static native void mobStopPathfinding(String uuid);
    public static native boolean mobHasPath(String uuid);
    /** {@code {next index, reaches (0/1), x, y, z...}} or null. */
    public static native double[] mobCurrentPath(String uuid);

    /** The stack the living entity is using, inventory-encoded; "" for none. */
    public static native String activeItem(String uuid);
    /** Bits: 1 climbing, 2 in lava, 4 in water, 8 riptiding, 16 in rain. */
    public static native int entitySurroundings(String uuid);
    public static native void clearPlayerTitle(String uuid);
    public static native int playerCooldown(String uuid, String group);
    public static native void setPlayerCooldown(String uuid, String group, int ticks);
    public static native void setPlayerItemCooldown(String uuid, String item, int ticks);
    /** Vanilla's Input flags from the player's last input packet. */
    public static native int playerInput(String uuid);
    public static native String playerCursor(String uuid);
    public static native void setPlayerCursor(String uuid, String item);
    public static native void givePlayerExperience(String uuid, int amount, boolean mending);
    public static native void sendActionBarComponent(String uuid, String json);
    public static native void setPlayerListNameComponent(String uuid, String json);
    public static native void setPlayerListHeaderFooterComponents(String uuid, String header, String footer);
    public static native void setEntityGliding(String uuid, boolean gliding);
    public static native void setEntitySwimming(String uuid, boolean swimming);
    /** A new plugin merchant titled with a component's JSON; its handle. */
    public static native String createMerchant(String titleJson);
    public static native void releaseMerchant(String handle);
    public static native String[] merchantOffers(String handle);
    public static native boolean setMerchantOffers(String handle, String[] offers);
    public static native boolean setMerchantOffer(String handle, int index, String offer);
    /** The trader of a plugin merchant (by handle) or of a villager or wandering trader (by UUID). */
    public static native String merchantTrader(String handle);
    public static native boolean openMerchant(String uuid, String handle, boolean force);

    /** A block's outline (kind 0) or collision (kind 1) boxes, six values each, block-local. */
    public static native double[] blockShapeBoxes(String world, int x, int y, int z, int kind);
    /** Flags of a block data string: 1 liquid, 2 occluding. */
    public static native int blockStateFlags(String data);
    /** {hit x, y, z, block x, y, z, BlockFace ordinal} or null on a miss. */
    public static native double[] rayTraceBlocks(String world, double startX, double startY, double startZ,
        double endX, double endY, double endZ, int fluidMode, boolean ignorePassable);

    public static UUID parse(String uuid) {
        if (uuid == null) return null;
        try {
            return UUID.fromString(uuid);
        } catch (IllegalArgumentException error) {
            return null;
        }
    }
}
