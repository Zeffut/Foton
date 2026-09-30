package org.bukkit.entity;

import java.util.Set;
import org.bukkit.plugin.Plugin;
import org.bukkit.command.CommandSender;

/** A player on the server, as a plugin sees one. */
public interface Player extends HumanEntity, org.bukkit.OfflinePlayer {
    default void playEffect(org.bukkit.EntityEffect effect) { }
    default void playEffect(org.bukkit.Location location, org.bukkit.Effect effect, Object data) {
        if (location != null && location.getWorld() != null && data instanceof Number number)
            location.getWorld().playEffect(location, effect, number.intValue());
    }
    default long getFirstPlayed() { return 0L; }
    default long getLastPlayed() { return 0L; }
    default boolean isInWater() { return false; }
    default void sendBlockChange(org.bukkit.Location location, org.bukkit.block.data.BlockData block) {
        if (location != null && location.getWorld() != null && block != null)
            foton.Native.sendBlockChange(getUniqueId().toString(), location.getWorld().getName(), location.getBlockX(), location.getBlockY(), location.getBlockZ(), block.getAsString());
    }
    default void sendSignChange(org.bukkit.Location location, String[] lines, org.bukkit.DyeColor color) {
        if (location != null && location.getWorld() != null)
            foton.Native.sendSignChange(getUniqueId().toString(), location.getWorld().getName(), location.getBlockX(), location.getBlockY(), location.getBlockZ(), lines == null ? new String[0] : lines, color == null ? 0 : color.ordinal());
    }
    default float getExp() { return 0.0f; }
    default void setLevel(int level) { }
    default int getTotalExperience() { return 0; }
    default void giveExp(int amount) { }
    default void setTotalExperience(int total) { }
    /** Vanilla LivingEntity freeze threshold in ticks. */
    default int getMaxFreezeTicks() { return 140; }
    default void setExp(float exp) { }
    @Override default boolean isHandRaised() { return false; }
    @Override default void clearActiveItem() { }
    default org.bukkit.inventory.EquipmentSlot getHandRaised() { return isHandRaised() ? org.bukkit.inventory.EquipmentSlot.HAND : null; }
    default boolean isInsideVehicle() { return getVehicle() != null; }
    default boolean leaveVehicle() { return eject(); }
    default boolean isSprinting() { return false; }
    default boolean isSwimming() { return false; }
    default boolean isGlowing() { return false; }
    default void setGlowing(boolean glowing) { }
    default float getFallDistance() { return 0.0f; }
    default int getRemainingAir() { return getAir(); }
    default void setRemainingAir(int ticks) { setAir(ticks); }
    default void resetPlayerWeather() { }
    default org.bukkit.WeatherType getPlayerWeather() { return org.bukkit.WeatherType.CLEAR; }
    default void setPlayerWeather(org.bukkit.WeatherType type) { }
    default long getPlayerTime() { return 0L; }
    default long getPlayerTimeOffset() { return 0L; }
    default boolean isPlayerTimeRelative() { return true; }
    default void setPlayerTime(long time, boolean relative) { }
    default void resetPlayerTime() { }
    default void setCompassTarget(org.bukkit.Location location) { }
    default org.bukkit.block.Block getTargetBlock(java.util.Set<org.bukkit.Material> transparent, int maxDistance) {
        if (maxDistance <= 0 || getLocation() == null || getWorld() == null) return null;
        org.bukkit.Location origin = getLocation().clone().add(0.0, getEyeHeight(), 0.0);
        org.bukkit.util.Vector direction = origin.getDirection().normalize();
        for (int step = 0; step <= maxDistance * 10; step++) {
            double distance = step / 10.0;
            org.bukkit.Location point = origin.clone().add(direction.clone().multiply(distance));
            org.bukkit.block.Block block = getWorld().getBlockAt(point.getBlockX(), point.getBlockY(), point.getBlockZ());
            org.bukkit.Material material = block.getType();
            if (!material.isAir() && (transparent == null || !transparent.contains(material))) return block;
        }
        return null;
    }
    /** Returns the first non-air block along the player's view ray. */
    default org.bukkit.block.Block getTargetBlockExact(int maxDistance) {
        return getTargetBlock(null, maxDistance);
    }
    default java.util.Set<Entity> getTrackedBy() { return java.util.Collections.emptySet(); }
    default void hideEntity(Plugin plugin, Entity entity) { }
    default void showEntity(Plugin plugin, Entity entity) { }
    default Entity getSpectatorTarget() { return null; }
    default Player getPlayer() { return this; }
    default int getPing() { return 0; }
    default float getWalkSpeed() { return 0.1f; }
    default void setWalkSpeed(float speed) { }
    default float getFlySpeed() { return 0.1f; }
    default void setFlySpeed(float speed) { }
    default boolean addPotionEffect(org.bukkit.potion.PotionEffect effect) { return false; }
    default void removePotionEffect(org.bukkit.potion.PotionEffectType type) { }
    default org.bukkit.attribute.AttributeInstance getAttribute(org.bukkit.attribute.Attribute attribute) { return null; }
    default void updateCommands() { }
    /** This player's progress on an advancement. */
    org.bukkit.advancement.AdvancementProgress getAdvancementProgress(org.bukkit.advancement.Advancement advancement);

    default com.destroystokyo.paper.profile.PlayerProfile getPlayerProfile() {
        return null;
    }
    @Override
    String getName();

    java.net.InetSocketAddress getAddress();

    /** Returns the client locale, such as {@code en_us}. */
    default String getLocale() { return "en_us"; }
    default java.util.Locale locale() {
        String value = getLocale();
        return value == null || value.isBlank() ? java.util.Locale.US : java.util.Locale.forLanguageTag(value.replace('_', '-'));
    }

    /** Returns the server hosting this player. */
    default org.bukkit.Server getServer() {
        return org.bukkit.Bukkit.getServer();
    }

    Set<String> getListeningPluginChannels();

    void sendPluginMessage(Plugin source, String channel, byte[] message);
    default void sendMessage(String[] messages) {
        if (messages == null) return;
        for (String message : messages) sendMessage(message);
    }

    boolean isOnline();
    /** Whether this player's persistent profile has appeared on the server before. */
    default boolean hasPlayedBefore() {
        return getUniqueId() != null && org.bukkit.Bukkit.getOfflinePlayer(getUniqueId()).hasPlayedBefore();
    }
    /** Visibility is unrestricted until a hide-player service is registered. */
    default boolean canSee(Player other) { return other != null && other.isOnline(); }
    default void hidePlayer(Player player) { }
    default void showPlayer(Player player) { }

    /** Hides a player from this one on behalf of a plugin.
     *
     * <p>The plugin argument is how Bukkit refcounts visibility: two plugins
     * hiding the same player must both show them again before they reappear.
     * Foton does not refcount yet, so the argument is accepted and the
     * single-argument behavior applies -- which is the same answer as long as
     * one plugin is doing the hiding, and a link error otherwise. */
    default void hidePlayer(org.bukkit.plugin.Plugin plugin, Player player) { hidePlayer(player); }

    default void showPlayer(org.bukkit.plugin.Plugin plugin, Player player) { showPlayer(player); }
    int getLevel();
    default int getFoodLevel() { return 20; }
    default void setFoodLevel(int level) { }
    default float getSaturation() { return 5.0f; }
    default void setSaturation(float saturation) { }
    default float getExhaustion() { return 0.0f; }
    default void setExhaustion(float exhaustion) { }
    default java.util.Set<org.bukkit.permissions.PermissionAttachmentInfo> getEffectivePermissions() {
        return java.util.Collections.emptySet();
    }
    default void updateInventory() { }
    void closeInventory();
    org.bukkit.scoreboard.Scoreboard getScoreboard();
    default boolean isSneaking() { return false; }
    void openBook(org.bukkit.inventory.ItemStack book);
    default org.bukkit.inventory.InventoryView openSmithingTable(org.bukkit.Location location, boolean force) { return null; }
    default org.bukkit.inventory.InventoryView openLoom(org.bukkit.Location location, boolean force) { return null; }
    default org.bukkit.inventory.InventoryView openWorkbench(org.bukkit.Location location, boolean force) { return null; }
    default org.bukkit.inventory.InventoryView openGrindstone(org.bukkit.Location location, boolean force) { return null; }
    default org.bukkit.inventory.InventoryView openStonecutter(org.bukkit.Location location, boolean force) { return null; }
    default org.bukkit.inventory.InventoryView openAnvil(org.bukkit.Location location, boolean force) { return null; }
    default org.bukkit.inventory.InventoryView openCartographyTable(org.bukkit.Location location, boolean force) { return null; }
    boolean teleport(org.bukkit.Location location);
    void kickPlayer(String message);
    void kick(net.kyori.adventure.text.Component message);
    default void kick(net.kyori.adventure.text.Component message, org.bukkit.event.player.PlayerKickEvent.Cause cause) { kick(message); }
    void setPlayerListHeader(String header);
    void setPlayerListFooter(String footer);
    void setPlayerListHeaderFooter(String header, String footer);
    // Paper's overloads, down to the force = false a player-targeted spawn
    // defaults to and the extra = 1 of the overloads that take data but no extra.
    default void spawnParticle(org.bukkit.Particle particle, org.bukkit.Location location, int count) {
        this.spawnParticle(particle, location.getX(), location.getY(), location.getZ(), count);
    }
    default void spawnParticle(org.bukkit.Particle particle, double x, double y, double z, int count) {
        this.spawnParticle(particle, x, y, z, count, null);
    }
    default <T> void spawnParticle(org.bukkit.Particle particle, org.bukkit.Location location, int count, T data) {
        this.spawnParticle(particle, location.getX(), location.getY(), location.getZ(), count, data);
    }
    default <T> void spawnParticle(org.bukkit.Particle particle, double x, double y, double z, int count, T data) {
        this.spawnParticle(particle, x, y, z, count, 0, 0, 0, data);
    }
    default void spawnParticle(org.bukkit.Particle particle, org.bukkit.Location location, int count, double offsetX, double offsetY, double offsetZ) {
        this.spawnParticle(particle, location.getX(), location.getY(), location.getZ(), count, offsetX, offsetY, offsetZ);
    }
    default void spawnParticle(org.bukkit.Particle particle, double x, double y, double z, int count, double offsetX, double offsetY, double offsetZ) {
        this.spawnParticle(particle, x, y, z, count, offsetX, offsetY, offsetZ, null);
    }
    default <T> void spawnParticle(org.bukkit.Particle particle, org.bukkit.Location location, int count, double offsetX, double offsetY, double offsetZ, T data) {
        this.spawnParticle(particle, location.getX(), location.getY(), location.getZ(), count, offsetX, offsetY, offsetZ, data);
    }
    default <T> void spawnParticle(org.bukkit.Particle particle, double x, double y, double z, int count, double offsetX, double offsetY, double offsetZ, T data) {
        this.spawnParticle(particle, x, y, z, count, offsetX, offsetY, offsetZ, 1, data);
    }
    default void spawnParticle(org.bukkit.Particle particle, org.bukkit.Location location, int count, double offsetX, double offsetY, double offsetZ, double extra) {
        this.spawnParticle(particle, location.getX(), location.getY(), location.getZ(), count, offsetX, offsetY, offsetZ, extra);
    }
    default void spawnParticle(org.bukkit.Particle particle, double x, double y, double z, int count, double offsetX, double offsetY, double offsetZ, double extra) {
        this.spawnParticle(particle, x, y, z, count, offsetX, offsetY, offsetZ, extra, null);
    }
    default <T> void spawnParticle(org.bukkit.Particle particle, org.bukkit.Location location, int count, double offsetX, double offsetY, double offsetZ, double extra, T data) {
        this.spawnParticle(particle, location.getX(), location.getY(), location.getZ(), count, offsetX, offsetY, offsetZ, extra, data);
    }
    default <T> void spawnParticle(org.bukkit.Particle particle, double x, double y, double z, int count, double offsetX, double offsetY, double offsetZ, double extra, T data) {
        this.spawnParticle(particle, x, y, z, count, offsetX, offsetY, offsetZ, extra, data, false);
    }
    default <T> void spawnParticle(org.bukkit.Particle particle, org.bukkit.Location location, int count, double offsetX, double offsetY, double offsetZ, double extra, T data, boolean force) {
        this.spawnParticle(particle, location.getX(), location.getY(), location.getZ(), count, offsetX, offsetY, offsetZ, extra, data, force);
    }
    /** Sends particles to this player alone, wherever they are; {@code force}
     * asks the client to show them past its particle setting. */
    default <T> void spawnParticle(org.bukkit.Particle particle, double x, double y, double z, int count, double offsetX, double offsetY, double offsetZ, double extra, T data, boolean force) {
        foton.FotonParticles.spawn(this, particle, x, y, z, count, offsetX, offsetY, offsetZ, extra, data, force);
    }

    void sendActionBar(String message);

    @Override
    default void sendActionBar(net.kyori.adventure.text.Component message) {
        if (message != null) foton.Native.sendActionBarComponent(getUniqueId().toString(), foton.FotonComponents.toJson(message));
    }

    /** Clears the title and subtitle, keeping the fade times. */
    default void clearTitle() { foton.Native.clearPlayerTitle(getUniqueId().toString()); }

    /** The movement keys the client last reported. */
    default org.bukkit.Input getCurrentInput() { return new foton.FotonInput(foton.Native.playerInput(getUniqueId().toString())); }

    /** Experience as an orb gives it: with {@code applyMending}, damaged
     * mending gear soaks it up first. */
    default void giveExp(int amount, boolean applyMending) {
        foton.Native.givePlayerExperience(getUniqueId().toString(), amount, applyMending);
    }

    /** The name shown in the tab list; null restores the player's own. */
    default void playerListName(net.kyori.adventure.text.Component name) {
        foton.Native.setPlayerListNameComponent(getUniqueId().toString(), name == null ? null : foton.FotonComponents.toJson(name));
    }

    /** Sets both halves of the tab list decoration. */
    default void sendPlayerListHeaderAndFooter(net.kyori.adventure.text.Component header, net.kyori.adventure.text.Component footer) {
        foton.Native.setPlayerListHeaderFooterComponents(getUniqueId().toString(),
            header == null ? null : foton.FotonComponents.toJson(header),
            footer == null ? null : foton.FotonComponents.toJson(footer));
    }
    default void sendPlayerListHeader(net.kyori.adventure.text.Component header) {
        sendPlayerListHeaderAndFooter(header, null);
    }

    default void showTitle(net.kyori.adventure.title.Title title) { }

    org.bukkit.inventory.PlayerInventory getInventory();
    org.bukkit.inventory.Inventory getEnderChest();
    org.bukkit.inventory.InventoryView getOpenInventory();
    default org.bukkit.inventory.InventoryView openInventory(org.bukkit.inventory.Inventory inventory) { return null; }
    default org.bukkit.inventory.ItemStack getItemInHand() { return getInventory().getItemInHand(); }
    default void setItemInHand(org.bukkit.inventory.ItemStack item) { getInventory().setItemInHand(item); }
    default boolean performCommand(String command) {
        return command != null && getServer().dispatchCommand(this, command);
    }

    default void chat(String message) {}

    default org.bukkit.Location getBedSpawnLocation() { return null; }
    default void setBedSpawnLocation(org.bukkit.Location location) { }
    default org.bukkit.Location getPotentialBedLocation() { return getBedSpawnLocation(); }

    org.bukkit.GameMode getGameMode();
    default void setGameMode(org.bukkit.GameMode mode) { }
    default org.bukkit.entity.Player getKiller() { return null; }
    default int getStatistic(org.bukkit.Statistic statistic) { return 0; }
    default void setStatistic(org.bukkit.Statistic statistic, int value) { }
    default boolean getAllowFlight() { return false; }
    default void setAllowFlight(boolean value) { }
    default boolean isFlying() { return false; }
    default boolean isGliding() { return false; }
    default void setFlying(boolean value) { }
    default boolean isSleepingIgnored() { return false; }
    default void setSleepingIgnored(boolean value) { }

    boolean isOp();
    default void setOp(boolean value) { }
    default boolean isWhitelisted() { return false; }
    default void setWhitelisted(boolean value) { }

    String getDisplayName();

    void setDisplayName(String name);
    default void setPlayerListName(String name) { }

    /** The big text in the middle of the screen. Times are in ticks. */
    void sendTitle(String title, String subtitle, int fadeIn, int stay, int fadeOut);
    void sendTitle(com.destroystokyo.paper.Title title);
    void hideTitle();
    void resetTitle();

    void playSound(org.bukkit.Location at, org.bukkit.Sound sound, float volume, float pitch);

    default void playSound(org.bukkit.entity.Entity source, org.bukkit.Sound sound, float volume, float pitch) {
        if (source != null) playSound(source.getLocation(), sound, volume, pitch);
    }

    void playSound(org.bukkit.Location at, String sound, float volume, float pitch);
    void playSound(org.bukkit.Location at, org.bukkit.Sound sound, org.bukkit.SoundCategory category, float volume, float pitch);
    void playSound(org.bukkit.Location at, String sound, org.bukkit.SoundCategory category, float volume, float pitch);
    void stopSound(String sound, org.bukkit.SoundCategory category);
    void stopSound(String sound);

    /** The scheduler for work that follows this player. */
    io.papermc.paper.threadedregions.scheduler.EntityScheduler getScheduler();

    /** Spigot's extra surface. A plugin reaches for it to send an action bar
     * or a component message, and reaching for something that is not there is
     * a NoSuchMethodError at load rather than a feature it does without. */
    Spigot spigot();

    /** What `player.spigot()` answers. */
    abstract class Spigot extends CommandSender.Spigot {
        private final Player player;
        protected Spigot(Player player) { super(player); this.player = player; }
        public int getPing() { return player.getPing(); }
        public String getLocale() { return player.getLocale(); }
        public void sendMessage(String message) {}
        public void sendMessage(net.md_5.bungee.api.ChatMessageType position,
                net.md_5.bungee.api.chat.BaseComponent... components) {
            super.sendMessage(components);
        }
    }
}
