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
public final class FotonPlayer implements Player {
    private final UUID id;

    public FotonPlayer(UUID id) {
        this.id = id;
    }

    @Override
    public UUID getUniqueId() {
        return id;
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

    @Override
    public org.bukkit.GameMode getGameMode() {
        org.bukkit.GameMode mode = org.bukkit.GameMode.byName(Native.gameMode(id.toString()));
        // A player who has gone is not in any mode; survival is the answer
        // that surprises a plugin least, and Bukkit's own handle to a departed
        // player answers just as arbitrarily.
        return mode == null ? org.bukkit.GameMode.SURVIVAL : mode;
    }

    @Override
    public boolean isOp() {
        return Native.isOperator(id.toString());
    }

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

    @Override
    public org.bukkit.scoreboard.Scoreboard getScoreboard() {
        World world = getWorld();
        return new FotonScoreboard(world == null ? "" : world.getName());
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
        // Foton does not hand out network entity ids to plugins: they are a
        // protocol detail that changes on respawn, and a plugin using one as
        // an identity would be wrong in a way that is hard to see.
        return -1;
    }

    @Override
    public boolean isDead() {
        return false;
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

    @Override public double getHealth() { return Native.health(id.toString()); }
    @Override public void setHealth(double health) { Native.setHealth(id.toString(), health); }
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
        Native.kickPlayer(id.toString(), message == null ? "" : message);
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
    public boolean hasPermission(String permission) {
        return Native.hasPermission(id.toString(), permission);
    }

    @Override public boolean isPermissionSet(String permission) {
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
