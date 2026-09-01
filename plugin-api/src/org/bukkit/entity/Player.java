package org.bukkit.entity;

import java.util.Set;
import org.bukkit.plugin.Plugin;
import org.bukkit.command.CommandSender;

/** A player on the server, as a plugin sees one. */
public interface Player extends HumanEntity {
    @Override
    String getName();

    /** Returns the client locale, such as {@code en_us}. */
    default String getLocale() { return "en_us"; }

    /** Returns the server hosting this player. */
    default org.bukkit.Server getServer() {
        return org.bukkit.Bukkit.getServer();
    }

    Set<String> getListeningPluginChannels();

    void sendPluginMessage(Plugin source, String channel, byte[] message);

    boolean isOnline();
    /** Visibility is unrestricted until a hide-player service is registered. */
    default boolean canSee(Player other) { return other != null && other.isOnline(); }
    int getLevel();
    org.bukkit.scoreboard.Scoreboard getScoreboard();
    default boolean isSneaking() { return false; }
    void openBook(org.bukkit.inventory.ItemStack book);
    boolean teleport(org.bukkit.Location location);
    void kickPlayer(String message);
    void kick(net.kyori.adventure.text.Component message);
    void setPlayerListHeader(String header);
    void setPlayerListFooter(String footer);
    void setPlayerListHeaderFooter(String header, String footer);
    default void spawnParticle(org.bukkit.Particle particle, org.bukkit.Location location, int count, Object data) { }

    void sendActionBar(String message);

    default void sendActionBar(net.kyori.adventure.text.Component message) {
        sendActionBar(message == null ? "" : net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer.plainText().serialize(message));
    }

    default void showTitle(net.kyori.adventure.title.Title title) { }

    org.bukkit.inventory.PlayerInventory getInventory();

    default org.bukkit.Location getBedSpawnLocation() { return null; }
    default org.bukkit.Location getPotentialBedLocation() { return getBedSpawnLocation(); }

    org.bukkit.GameMode getGameMode();

    boolean isOp();

    String getDisplayName();

    void setDisplayName(String name);

    /** The big text in the middle of the screen. Times are in ticks. */
    void sendTitle(String title, String subtitle, int fadeIn, int stay, int fadeOut);
    void sendTitle(com.destroystokyo.paper.Title title);
    void hideTitle();
    void resetTitle();

    void playSound(org.bukkit.Location at, org.bukkit.Sound sound, float volume, float pitch);

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
        protected Spigot(Player player) { super(player); }
        public void sendMessage(String message) {}
        public void sendMessage(net.md_5.bungee.api.ChatMessageType position,
                net.md_5.bungee.api.chat.BaseComponent... components) {
            super.sendMessage(components);
        }
    }
}
