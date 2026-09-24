package org.bukkit.scoreboard;

import java.util.Collection;
import java.util.List;
import java.util.Set;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.format.NamedTextColor;
import net.kyori.adventure.text.format.TextColor;

/** A scoreboard team: entries, the rules between them, and what the client draws around their names.
 *
 * <p>Paper's Team is also an Adventure {@code ForwardingAudience}; Foton's
 * players are not audiences, so it is not one here.</p>
 *
 * <p>Every method of a team that has been unregistered throws
 * {@link IllegalStateException}, as Paper's does.</p>
 */
public interface Team {
    String getName();

    Component displayName();

    void displayName(Component displayName);

    /** Drawn before each member's name; empty when none. */
    Component prefix();

    void prefix(Component prefix);

    Component suffix();

    void suffix(Component suffix);

    boolean hasColor();

    /** The colour of members' names; throws when the team has none, as Paper does. */
    TextColor color();

    void color(NamedTextColor color);

    /** The prefix as section-sign text. */
    String getPrefix();

    void setPrefix(String prefix);

    String getSuffix();

    void setSuffix(String suffix);

    String getDisplayName();

    void setDisplayName(String displayName);

    org.bukkit.ChatColor getColor();

    void setColor(org.bukkit.ChatColor color);

    boolean allowFriendlyFire();

    void setAllowFriendlyFire(boolean enabled);

    boolean canSeeFriendlyInvisibles();

    void setCanSeeFriendlyInvisibles(boolean enabled);

    Set<String> getEntries();

    int getSize();

    Scoreboard getScoreboard();

    /** Puts the entry on this team, taking it off any other. */
    void addEntry(String entry);

    default void addEntries(String... entries) {
        addEntries(List.of(entries));
    }

    void addEntries(Collection<String> entries);

    boolean removeEntry(String entry);

    default boolean removeEntries(String... entries) {
        return removeEntries(List.of(entries));
    }

    boolean removeEntries(Collection<String> entries);

    boolean hasEntry(String entry);

    void addPlayer(org.bukkit.OfflinePlayer player);

    boolean removePlayer(org.bukkit.OfflinePlayer player);

    boolean hasPlayer(org.bukkit.OfflinePlayer player);

    /** An entity's entry: a player's name, any other entity's UUID. */
    void addEntity(org.bukkit.entity.Entity entity);

    boolean removeEntity(org.bukkit.entity.Entity entity);

    boolean hasEntity(org.bukkit.entity.Entity entity);

    void unregister();

    OptionStatus getOption(Option option);

    void setOption(Option option, OptionStatus status);

    enum Option {
        NAME_TAG_VISIBILITY,
        /** Foton has no per-team death message rule: it is always shown. */
        DEATH_MESSAGE_VISIBILITY,
        COLLISION_RULE
    }

    enum OptionStatus {
        ALWAYS,
        NEVER,
        FOR_OTHER_TEAMS,
        FOR_OWN_TEAM
    }
}
