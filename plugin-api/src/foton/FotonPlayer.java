package foton;

import java.util.Set;
import java.util.UUID;
import org.bukkit.Location;
import org.bukkit.World;
import org.bukkit.entity.Player;
import org.bukkit.plugin.Plugin;

/** A player, as a plugin holds one.
 *
 * Nothing but a UUID and a route back into Foton. Bukkit's own Player objects
 * behave the same way once their player has left -- they keep answering, and
 * what they answer stops meaning anything -- so a handle that stops resolving
 * is not a new hazard for a plugin to learn.
 */
public final class FotonPlayer implements Player, org.bukkit.projectiles.ProjectileSource, net.kyori.adventure.audience.Audience {
    @Override public void playEffect(org.bukkit.EntityEffect effect) { if (effect != null) Native.playerEntityEffect(getUniqueId().toString(), effect.name()); }

    @Override public Player getKiller() {
        String uuid = Native.playerKiller(id.toString());
        try { return uuid == null ? null : new FotonPlayer(UUID.fromString(uuid)); }
        catch (IllegalArgumentException ignored) { return null; }
    }
    @Override public boolean isSprinting() { return Native.entitySprinting(id.toString()); }
    @Override public boolean isSwimming() { return Native.entitySwimming(id.toString()); }
    @Override public void hideEntity(Plugin plugin, org.bukkit.entity.Entity entity) {
        if (entity != null) Native.playerHideEntity(id.toString(), entity.getUniqueId().toString(), true);
    }
    @Override public void showEntity(Plugin plugin, org.bukkit.entity.Entity entity) {
        if (entity != null) Native.playerHideEntity(id.toString(), entity.getUniqueId().toString(), false);
    }
    @Override public boolean canSee(Player other) {
        return other != null && other.isOnline()
            && Native.playerCanSeeEntity(id.toString(), other.getUniqueId().toString());
    }
    @Override public void hidePlayer(Player other) {
        if (other != null) Native.playerHideEntity(id.toString(), other.getUniqueId().toString(), true);
    }
    @Override public void showPlayer(Player other) {
        if (other != null) Native.playerHideEntity(id.toString(), other.getUniqueId().toString(), false);
    }
    @Override public java.util.Set<org.bukkit.entity.Entity> getTrackedBy() {
        String[] ids = Native.entityTrackedBy(id.toString());
        java.util.LinkedHashSet<org.bukkit.entity.Entity> result = new java.util.LinkedHashSet<>();
        if (ids != null) for (String value : ids) try { result.add(new FotonPlayer(UUID.fromString(value))); }
        catch (IllegalArgumentException ignored) { }
        return java.util.Collections.unmodifiableSet(result);
    }
    private static final java.util.concurrent.ConcurrentHashMap<UUID, FotonPersistentDataContainer> DATA =
        new java.util.concurrent.ConcurrentHashMap<>();
    private final UUID id;
    private final java.util.concurrent.CopyOnWriteArrayList<org.bukkit.permissions.PermissionAttachment> attachments = new java.util.concurrent.CopyOnWriteArrayList<>();
    @Override public int getPing() { return Native.playerPing(id.toString()); }
    @Override public float getWalkSpeed() { return Native.playerWalkSpeed(id.toString()); }
    @Override public void setWalkSpeed(float speed) { Native.setPlayerWalkSpeed(id.toString(), speed); }
    @Override public float getFlySpeed() { return Native.playerFlySpeed(id.toString()); }
    @Override public void setFlySpeed(float speed) { Native.setPlayerFlySpeed(id.toString(), speed); }
    @Override public boolean addPotionEffect(org.bukkit.potion.PotionEffect effect) {
        if (effect == null || effect.getType() == null) return false;
        org.bukkit.potion.PotionEffect old = getPotionEffect(effect.getType());
        String action = old == null ? "ADDED" : "CHANGED";
        if (!EventBridge.firePotionEffect(id.toString(), effect.getType().getName(),
                old == null ? -1 : old.getDuration(), old == null ? -1 : old.getAmplifier(),
                effect.getDuration(), effect.getAmplifier(), action)) return false;
        return Native.addPotionEffect(id.toString(), effect.getType().getName(), effect.getDuration(), effect.getAmplifier());
    }
    @Override public void removePotionEffect(org.bukkit.potion.PotionEffectType type) {
        if (type == null) return;
        org.bukkit.potion.PotionEffect old = getPotionEffect(type);
        if (old == null) return;
        if (EventBridge.firePotionEffect(id.toString(), type.getName(),
                old.getDuration(), old.getAmplifier(), -1, -1, "REMOVED"))
            Native.removePotionEffect(id.toString(), type.getName());
    }

    @Override public org.bukkit.attribute.AttributeInstance getAttribute(org.bukkit.attribute.Attribute attribute) {
        if (attribute == null) return null;
        String value = Native.playerAttribute(id.toString(), attribute.name());
        if (value == null) return null;
        String[] fields = value.split("\\|", -1);
        if (fields.length != 2) return null;
        try { return new org.bukkit.attribute.AttributeInstance(attribute, Double.parseDouble(fields[0]), Double.parseDouble(fields[1])); }
        catch (NumberFormatException ignored) { return null; }
    }

    public FotonPlayer(UUID id) {
        this.id = id;
    }

    @Override public com.destroystokyo.paper.profile.PlayerProfile getPlayerProfile() {
        return new FotonPlayerProfile(id, getName());
    }

    @Override
    public UUID getUniqueId() {
        return id;
    }

    @Override
    public org.bukkit.persistence.PersistentDataContainer getPersistentDataContainer() {
        return DATA.computeIfAbsent(id, ignored -> new FotonPersistentDataContainer());
    }

    @Override
    public String getName() {
        String name = Native.playerName(id.toString());
        return name == null ? "" : name;
    }

    @Override
    public String getLocale() {
        String locale = Native.playerLocale(id.toString());
        return locale == null ? "en_us" : locale;
    }

    @Override
    public World getWorld() {
        String name = Native.playerWorld(id.toString());
        return name == null ? null : new FotonWorld(name);
    }
    @Override
    public java.net.InetSocketAddress getAddress() {
        String address = Native.playerAddress(id.toString());
        if (address == null) return null;
        int separator = address.lastIndexOf(':');
        if (separator <= 0 || separator == address.length() - 1) return null;
        try {
            return new java.net.InetSocketAddress(
                address.substring(0, separator),
                Integer.parseInt(address.substring(separator + 1)));
        } catch (NumberFormatException error) {
            return null;
        }
    }

    /** Where the player is, as of the moment this was asked.
     *
     * The five numbers arrive together rather than one call each, so a plugin
     * cannot read x from one tick and z from the next and end up with a point
     * the player was never at.
     */
    @Override
    public Location getLocation() {
        double[] at = Native.playerPosition(id.toString());
        if (at == null) {
            return null;
        }
        return new Location(getWorld(), at[0], at[1], at[2], (float) at[3], (float) at[4]);
    }

    @Override
    public void setBedSpawnLocation(org.bukkit.Location location) {
        if (location != null && location.getWorld() != null)
            Native.setPlayerRespawnPosition(id.toString(), location.getWorld().getName(), location.getBlockX(), location.getBlockY(), location.getBlockZ(), location.getYaw(), location.getPitch());
    }

    @Override
    public org.bukkit.Location getBedSpawnLocation() {
        String world = Native.playerRespawnWorld(id.toString());
        double[] pos = Native.playerRespawnPosition(id.toString());
        if (world == null || pos == null || pos.length < 5) return null;
        return new org.bukkit.Location(new FotonWorld(world), pos[0], pos[1], pos[2], (float) pos[3], (float) pos[4]);
    }

    @Override
    public org.bukkit.Location getPotentialBedLocation() {
        return getBedSpawnLocation();
    }

    @Override
    public org.bukkit.inventory.EntityEquipment getEquipment() {
        return new FotonEntityEquipment(id.toString());
    }

    @Override
    public org.bukkit.inventory.PlayerInventory getInventory() {
        return new FotonInventory(id.toString());
    }

    @Override public org.bukkit.inventory.Inventory getEnderChest() {
        return new FotonEnderChestInventory(id.toString());
    }

    @Override
    public org.bukkit.inventory.InventoryView getOpenInventory() {
        return new FotonInventoryView(this);
    }

    @Override
    public void updateInventory() {
        Native.updateInventory(id.toString());
    }

    @Override
    public void closeInventory() {
        Native.closeInventory(id.toString());
    }

    @Override
    public org.bukkit.GameMode getGameMode() {
        org.bukkit.GameMode mode = org.bukkit.GameMode.byName(Native.gameMode(id.toString()));
        // A player who has gone is not in any mode; survival is the answer
        // that surprises a plugin least, and Bukkit's own handle to a departed
        // player answers just as arbitrarily.
        return mode == null ? org.bukkit.GameMode.SURVIVAL : mode;
    }

    @Override public long getFirstPlayed() { return Native.firstPlayed(id.toString()); }
    @Override public long getLastPlayed() { return Native.lastPlayed(id.toString()); }

    @Override public int getStatistic(org.bukkit.Statistic statistic) {
        return statistic == null ? 0 : Native.statisticValue(id.toString(), statistic.name());
    }

    @Override
    public void setGameMode(org.bukkit.GameMode mode) {
        if (mode != null) Native.setGameMode(id.toString(), mode.name());
    }

    @Override public boolean getAllowFlight() { return Native.allowFlight(id.toString()); }
    @Override public void setAllowFlight(boolean value) { Native.setAllowFlight(id.toString(), value); }
    @Override public boolean isFlying() { return Native.isFlying(id.toString()); }
    @Override public boolean isGliding() { return Native.entityIsFallFlying(id.toString()); }
    @Override public boolean isInWater() { return Native.entityInWater(id.toString()); }
    @Override public void setFlying(boolean value) { Native.setFlying(id.toString(), value); }
    @Override public boolean isSleepingIgnored() { return Native.isSleepingIgnored(id.toString()); }
    @Override public void setSleepingIgnored(boolean value) { Native.setSleepingIgnored(id.toString(), value); }
    @Override public org.bukkit.inventory.InventoryView openInventory(org.bukkit.inventory.Inventory inventory) {
        if (!(inventory instanceof FotonCustomInventory custom)) return null;
        custom.attachViewer(id.toString());
        Native.openGenericInventory(id.toString(), custom.getSize(), custom.getTitle(), custom.encodeContents());
        if (Native.openMenuTopSlotCount(id.toString()) != custom.getSize()) {
            custom.detachViewer();
            return null;
        }
        return getOpenInventory();
    }

    @Override
    public org.bukkit.inventory.InventoryView openSmithingTable(org.bukkit.Location location, boolean force) {
        if (location == null || location.getWorld() == null) return null;
        if (!location.getWorld().getName().equals(getWorld().getName())) return null;
        if (!Native.openSmithingTable(id.toString(), location.getWorld().getName(),
                location.getBlockX(), location.getBlockY(), location.getBlockZ())) return null;
        return getOpenInventory();
    }

    @Override
    public org.bukkit.inventory.InventoryView openLoom(org.bukkit.Location location, boolean force) {
        if (location == null || location.getWorld() == null) return null;
        if (!location.getWorld().getName().equals(getWorld().getName())) return null;
        if (!Native.openLoom(id.toString(), location.getWorld().getName(),
                location.getBlockX(), location.getBlockY(), location.getBlockZ())) return null;
        return getOpenInventory();
    }

    @Override
    public void damage(double amount, org.bukkit.entity.Entity source) {
        if (amount > 0.0 && Double.isFinite(amount))
            Native.damagePlayer(id.toString(), amount, source == null ? null : source.getUniqueId().toString());
    }

    @Override
    public org.bukkit.inventory.InventoryView openCartographyTable(org.bukkit.Location location, boolean force) {
        if (location == null || location.getWorld() == null) return null;
        if (!location.getWorld().getName().equals(getWorld().getName())) return null;
        if (!Native.openCartographyTable(id.toString(), location.getWorld().getName(),
                location.getBlockX(), location.getBlockY(), location.getBlockZ())) return null;
        return getOpenInventory();
    }

    @Override
    public org.bukkit.inventory.InventoryView openAnvil(org.bukkit.Location location, boolean force) {
        if (location == null || location.getWorld() == null) return null;
        if (!location.getWorld().getName().equals(getWorld().getName())) return null;
        if (!Native.openAnvil(id.toString(), location.getWorld().getName(),
                location.getBlockX(), location.getBlockY(), location.getBlockZ())) return null;
        return getOpenInventory();
    }

    @Override
    public org.bukkit.inventory.InventoryView openStonecutter(org.bukkit.Location location, boolean force) {
        if (location == null || location.getWorld() == null) return null;
        if (!location.getWorld().getName().equals(getWorld().getName())) return null;
        if (!Native.openStonecutter(id.toString(), location.getWorld().getName(),
                location.getBlockX(), location.getBlockY(), location.getBlockZ())) return null;
        return getOpenInventory();
    }

    @Override
    public org.bukkit.inventory.InventoryView openGrindstone(org.bukkit.Location location, boolean force) {
        if (location == null || location.getWorld() == null) return null;
        if (!Native.openGrindstone(id.toString(), location.getWorld().getName(),
                location.getBlockX(), location.getBlockY(), location.getBlockZ())) return null;
        return getOpenInventory();
    }

    @Override
    public org.bukkit.inventory.InventoryView openWorkbench(org.bukkit.Location location, boolean force) {
        if (location == null || location.getWorld() == null) return null;
        if (!location.getWorld().getName().equals(getWorld().getName())) return null;
        if (!Native.openWorkbench(id.toString(), location.getWorld().getName(),
                location.getBlockX(), location.getBlockY(), location.getBlockZ())) return null;
        return getOpenInventory();
    }

    @Override
    public boolean isOp() {
        return Native.isOperator(id.toString());
    }
    @Override public void setOp(boolean value) { Native.setPlayerOperator(id.toString(), value); }
    @Override public boolean isWhitelisted() { return Native.isWhitelisted(id.toString()); }
    @Override public void setWhitelisted(boolean value) { Native.setPlayerWhitelisted(id.toString(), value); }

    @Override public float getExp() { return Native.experienceProgress(id.toString()); }
    @Override public void setExp(float exp) {
        if (!Float.isFinite(exp) || exp < 0.0f || exp > 1.0f) {
            throw new IllegalArgumentException("Experience progress must be between 0 and 1");
        }
        Native.setExperienceProgress(id.toString(), exp);
    }
    @Override public void setLevel(int level) { Native.setExperienceLevel(id.toString(), level); }
    @Override public int getTotalExperience() { return Native.totalExperience(id.toString()); }
    @Override public void setTotalExperience(int total) { Native.setTotalExperience(id.toString(), total); }
    @Override public void giveExp(int amount) { Native.giveExperience(id.toString(), amount); }

    @Override
    public int getLevel() {
        return Native.experienceLevel(id.toString());
    }

    @Override
    public void kick(net.kyori.adventure.text.Component message) {
        String text = message == null
            ? ""
            : net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer.plainText()
                .serialize(message);
        kickPlayer(text);
    }

    private static final java.util.Map<UUID, org.bukkit.scoreboard.Scoreboard> SCOREBOARDS =
        new java.util.concurrent.ConcurrentHashMap<>();

    @Override
    public org.bukkit.scoreboard.Scoreboard getScoreboard() {
        return SCOREBOARDS.computeIfAbsent(id, ignored -> {
            World world = getWorld();
            return new FotonScoreboard(world == null ? "" : world.getName());
        });
    }

    /** The name a plugin may have changed, falling back to the real one.
     *
     * Bukkit stores this per player and Foton has nowhere to put it, so it is
     * kept beside the handle. A plugin that sets it on one handle and reads it
     * from another gets the real name back -- which is wrong, and is the
     * honest consequence of a handle that is only a UUID. Foton needs a place
     * to store it before this can be right.
     */
    @Override
    public String getDisplayName() {
        String chosen = DISPLAY_NAMES.get(id);
        return chosen == null ? getName() : chosen;
    }

    @Override
    public void setPlayerListName(String name) {
        Native.setPlayerListName(id.toString(), name);
    }

    @Override
    public void setDisplayName(String name) {
        if (name == null) {
            DISPLAY_NAMES.remove(id);
        } else {
            DISPLAY_NAMES.put(id, name);
        }
    }

    private static final java.util.Map<UUID, String> DISPLAY_NAMES =
        new java.util.concurrent.ConcurrentHashMap<>();

    @Override
    public void chat(String message) {
        if (message != null && !message.isEmpty()) Native.chat(getUniqueId().toString(), message);
    }

    @Override
    public void sendTitle(String title, String subtitle, int fadeIn, int stay, int fadeOut) {
        Native.sendTitle(id.toString(), title == null ? "" : title,
            subtitle == null ? "" : subtitle, fadeIn, stay, fadeOut);
    }

    @Override
    public void sendTitle(com.destroystokyo.paper.Title title) {
        if (title == null) { hideTitle(); return; }
        Native.sendTitle(id.toString(), legacy(title.getTitle()), legacy(title.getSubtitle()),
            title.getFadeIn(), title.getStay(), title.getFadeOut());
    }

    @Override
    public void hideTitle() { Native.clearTitle(id.toString()); }

    @Override
    public void resetTitle() { hideTitle(); }

    private static String legacy(net.md_5.bungee.api.chat.BaseComponent[] components) {
        if (components == null) return "";
        StringBuilder text = new StringBuilder();
        for (net.md_5.bungee.api.chat.BaseComponent component : components)
            if (component != null) text.append(component.toLegacyText());
        return text.toString();
    }

    @Override
    public void playSound(org.bukkit.Location at, org.bukkit.Sound sound, float volume,
            float pitch) {
        playSound(at, sound == null ? null : sound.getKey(), volume, pitch);
    }

    @Override
    public void playSound(org.bukkit.Location at, String sound, float volume, float pitch) {
        org.bukkit.Location where = at == null ? getLocation() : at;
        if (where == null || where.getWorld() == null || sound == null) {
            return;
        }
        Native.playSound(where.getWorld().getName(), where.getX(), where.getY(), where.getZ(),
            sound, volume, pitch);
    }

    @Override
    public void playSound(org.bukkit.Location at, org.bukkit.Sound sound,
            org.bukkit.SoundCategory category, float volume, float pitch) {
        playSound(at, sound == null ? null : sound.getKey(), category, volume, pitch);
    }

    @Override
    public void playSound(org.bukkit.Location at, String sound, org.bukkit.SoundCategory category,
            float volume, float pitch) {
        org.bukkit.Location where = at == null ? getLocation() : at;
        if (where == null || where.getWorld() == null || sound == null || category == null) return;
        Native.playSoundCategory(where.getWorld().getName(), where.getX(), where.getY(),
            where.getZ(), sound, category.name(), volume, pitch);
    }

    @Override
    public void stopSound(String sound, org.bukkit.SoundCategory category) {
        Native.stopSound(id.toString(), sound == null ? "" : sound, category == null ? "" : category.name());
    }

    @Override
    public void stopSound(String sound) {
        stopSound(sound, null);
    }

    @Override
    public io.papermc.paper.threadedregions.scheduler.EntityScheduler getScheduler() {
        return FotonRegionSchedulers.forEntity();
    }

    @Override
    public Spigot spigot() {
        return spigot;
    }

    /** Spigot's extra surface, which for Foton is the ordinary one. */
    private final Spigot spigot = new Spigot(this) {
        @Override
        public void sendMessage(String message) {
            FotonPlayer.this.sendMessage(message);
        }
    };

    @Override
    public int getEntityId() {
        return Native.entityId(id.toString());
    }

    @Override
    public boolean isDead() {
        return Native.entityWorld(id.toString()) == null;
    }

    @Override public org.bukkit.entity.EntityType getType() {
        return org.bukkit.entity.EntityType.PLAYER;
    }

    @Override
    public String getCustomName() {
        return Native.customName(id.toString());
    }

    @Override
    public void setCustomName(String name) {
        Native.setCustomName(id.toString(), name);
    }

    @Override public int getFoodLevel() { return Native.playerFoodLevel(id.toString()); }
    @Override public void setFoodLevel(int level) { Native.setPlayerFood(id.toString(), level, getSaturation(), getExhaustion()); }
    @Override public float getSaturation() { return Native.playerFoodSaturation(id.toString()); }
    @Override public void setSaturation(float value) { Native.setPlayerFood(id.toString(), getFoodLevel(), value, getExhaustion()); }
    @Override public float getExhaustion() { return Native.playerFoodExhaustion(id.toString()); }
    @Override public void setExhaustion(float value) { Native.setPlayerFood(id.toString(), getFoodLevel(), getSaturation(), value); }

    @Override public double getHealth() { return Native.health(id.toString()); }
    @Override public void setHealth(double health) { Native.setHealth(id.toString(), health); }
    @Override public void setCompassTarget(org.bukkit.Location location) {
        if (location != null && location.getWorld() != null) Native.setCompassTarget(id.toString(), location.getWorld().getName(), location.getBlockX(), location.getBlockY(), location.getBlockZ());
    }
    @Override public double getMaxHealth() { return Native.maxHealth(id.toString()); }

    @Override
    public boolean isOnline() {
        return Native.playerName(id.toString()) != null;
    }

    @Override public boolean isSneaking() { return Native.isSneaking(id.toString()); }

    @Override public void openBook(org.bukkit.inventory.ItemStack book) {
        if (book != null) Native.openBook(id.toString());
    }

    @Override public boolean teleport(org.bukkit.Location location) {
        if (location == null || location.getWorld() == null) return false;
        org.bukkit.event.player.PlayerTeleportEvent event =
            new org.bukkit.event.player.PlayerTeleportEvent(this, getLocation(), location,
                org.bukkit.event.player.PlayerTeleportEvent.TeleportCause.PLUGIN);
        EventBridge.dispatch(event);
        if (event.isCancelled() || event.getTo() == null || event.getTo().getWorld() == null) return false;
        location = event.getTo();
        return Native.teleport(id.toString(), location.getWorld().getName(), location.getX(),
            location.getY(), location.getZ(), location.getYaw(), location.getPitch());
    }

    @Override
    public void kickPlayer(String message) {
        String reason = message == null ? "" : message;
        if (EventBridge.firePlayerKick(id.toString(), reason)) {
            Native.kickPlayer(id.toString(), reason);
        }
    }

    @Override
    public void setPlayerListHeader(String header) {
        Native.setPlayerListHeader(id.toString(), header == null ? "" : header);
    }

    @Override
    public void setPlayerListFooter(String footer) {
        Native.setPlayerListFooter(id.toString(), footer == null ? "" : footer);
    }

    @Override
    public void setPlayerListHeaderFooter(String header, String footer) {
        Native.setPlayerListHeaderFooter(id.toString(), header == null ? "" : header,
            footer == null ? "" : footer);
    }

    @Override
    public void sendSignChange(org.bukkit.Location location, String[] lines, org.bukkit.DyeColor color) {
        Player.super.sendSignChange(location, lines, color);
    }

    @Override
    public void sendActionBar(net.kyori.adventure.text.Component message) {
        Player.super.sendActionBar(message);
    }

    @Override
    public void showTitle(net.kyori.adventure.title.Title title) {
        if (title == null) return;
        String main = net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer.plainText().serialize(title.title());
        String sub = net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer.plainText().serialize(title.subtitle());
        int fadeIn = 10, stay = 70, fadeOut = 20;
        net.kyori.adventure.title.Title.Times t = title.times();
        if (t != null) {
            fadeIn = ticks(t.fadeIn()); stay = ticks(t.stay()); fadeOut = ticks(t.fadeOut());
        }
        sendTitle(main, sub, fadeIn, stay, fadeOut);
    }

    private static int ticks(java.time.Duration duration) {
        return (int) Math.max(0L, duration.toMillis() / 50L);
    }

    @Override
    public void sendActionBar(String message) {
        Native.sendActionBar(id.toString(), message == null ? "" : message);
    }

    @Override
    public void sendMessage(String message) {
        Native.sendMessage(id.toString(), message);
    }
    @Override
    public void sendMessage(net.kyori.adventure.text.Component message) {
        if (message != null) {
            Native.sendMessage(id.toString(),
                net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer
                    .plainText().serialize(message));
        }
    }

    @Override
    public boolean hasPermission(String permission) {
        if (permission == null) return false;
        for (int index = attachments.size() - 1; index >= 0; index--) {
            Boolean value = attachments.get(index).getPermissions().get(permission);
            if (value != null) return value;
        }
        return Native.hasPermission(id.toString(), permission);
    }

    @Override public org.bukkit.permissions.PermissionAttachment addAttachment(Plugin plugin) {
        org.bukkit.permissions.PermissionAttachment attachment = new org.bukkit.permissions.PermissionAttachment(plugin);
        attachments.add(attachment);
        return attachment;
    }
    @Override public void removeAttachment(org.bukkit.permissions.PermissionAttachment attachment) {
        if (attachment != null && attachments.remove(attachment)) attachment.remove();
    }
    @Override public void recalculatePermissions() { }

    @Override public java.util.Set<org.bukkit.permissions.PermissionAttachmentInfo> getEffectivePermissions() {
        java.util.LinkedHashSet<org.bukkit.permissions.PermissionAttachmentInfo> result = new java.util.LinkedHashSet<>();
        String[] entries = Native.effectivePermissions(id.toString());
        if (entries != null) for (String entry : entries) {
            if (entry == null) continue;
            int separator = entry.lastIndexOf('|');
            if (separator <= 0) continue;
            result.add(new org.bukkit.permissions.PermissionAttachmentInfo(this, entry.substring(0, separator), null, "1".equals(entry.substring(separator + 1))));
        }
        for (org.bukkit.permissions.PermissionAttachment attachment : attachments)
            for (java.util.Map.Entry<String, Boolean> entry : attachment.getPermissions().entrySet())
                result.removeIf(info -> info.getPermission().equalsIgnoreCase(entry.getKey()));
        for (org.bukkit.permissions.PermissionAttachment attachment : attachments)
            for (java.util.Map.Entry<String, Boolean> entry : attachment.getPermissions().entrySet())
                result.add(new org.bukkit.permissions.PermissionAttachmentInfo(this, entry.getKey(), attachment, entry.getValue()));
        return java.util.Collections.unmodifiableSet(result);
    }

    @Override public boolean isPermissionSet(String permission) {
        if (permission == null) return false;
        for (int index = attachments.size() - 1; index >= 0; index--)
            if (attachments.get(index).getPermissions().containsKey(permission)) return true;
        return Native.isPermissionSet(id.toString(), permission);
    }

    @Override
    public Set<String> getListeningPluginChannels() {
        return FotonMessenger.listening(id);
    }

    @Override
    public void sendPluginMessage(Plugin source, String channel, byte[] message) {
        FotonMessenger.send(this, source, channel, message);
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof FotonPlayer player && id.equals(player.id);
    }

    @Override
    public int hashCode() {
        return id.hashCode();
    }

    @Override
    public String toString() {
        return "FotonPlayer{" + id + "}";
    }
}
