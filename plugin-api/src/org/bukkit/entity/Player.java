package org.bukkit.entity;

import java.util.Set;
import org.bukkit.plugin.Plugin;
import org.bukkit.command.CommandSender;

/** A player on the server, as a plugin sees one. */
public interface Player extends Entity {
    @Override
    String getName();

    Set<String> getListeningPluginChannels();

    void sendPluginMessage(Plugin source, String channel, byte[] message);

    boolean isOnline();
    void kickPlayer(String message);
    void setPlayerListHeader(String header);
    void setPlayerListFooter(String footer);
    void setPlayerListHeaderFooter(String header, String footer);
    void sendActionBar(String message);

    org.bukkit.inventory.PlayerInventory getInventory();

    org.bukkit.GameMode getGameMode();

    boolean isOp();

    String getDisplayName();

    void setDisplayName(String name);

    /** The big text in the middle of the screen. Times are in ticks. */
    void sendTitle(String title, String subtitle, int fadeIn, int stay, int fadeOut);

    void playSound(org.bukkit.Location at, org.bukkit.Sound sound, float volume, float pitch);

    void playSound(org.bukkit.Location at, String sound, float volume, float pitch);
    void playSound(org.bukkit.Location at, org.bukkit.Sound sound, org.bukkit.SoundCategory category, float volume, float pitch);
    void playSound(org.bukkit.Location at, String sound, org.bukkit.SoundCategory category, float volume, float pitch);
    void stopSound(String sound, org.bukkit.SoundCategory category);

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
