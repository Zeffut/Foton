package foton;

import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.LinkedHashSet;
import java.util.Locale;
import java.util.Set;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.format.NamedTextColor;
import net.kyori.adventure.text.format.TextColor;
import org.bukkit.ChatColor;
import org.bukkit.OfflinePlayer;
import org.bukkit.entity.Entity;
import org.bukkit.entity.Player;
import org.bukkit.scoreboard.Scoreboard;
import org.bukkit.scoreboard.Team;

/** A handle on one team of a domain scoreboard; every read and write goes to the server.
 *
 * <p>A handle holds only the team's name, as Paper's does: two handles on one
 * team see each other's changes, and once the team is unregistered -- by a
 * plugin or by {@code /team remove} -- every call throws.</p>
 */
final class FotonTeam implements Team {
    private final String world;
    private final String name;

    FotonTeam(String world, String name) { this.world = world; this.name = name; }

    @Override public String getName() { return name; }

    private String property(String property) {
        String value = Native.scoreboardTeamProperty(world, name, property);
        if (value == null) throw new IllegalStateException("Unregistered scoreboard component");
        return value;
    }

    private void set(String property, String value) {
        if (!Native.scoreboardSetTeamProperty(world, name, property, value)) {
            property(property);
            throw new IllegalArgumentException("invalid " + property + ": " + value);
        }
    }

    private static Component component(String json) {
        return json.isEmpty() ? Component.empty() : ComponentJson.parse(json);
    }

    private static String json(Component component) {
        return component == null || component.equals(Component.empty()) ? "" : ComponentJson.json(component);
    }

    @Override public Component displayName() { return component(property("displayName")); }

    /** Null restores the default, the team's own name. */
    @Override public void displayName(Component value) { set("displayName", json(value)); }

    @Override public Component prefix() { return component(property("prefix")); }
    @Override public void prefix(Component value) { set("prefix", json(value)); }
    @Override public Component suffix() { return component(property("suffix")); }
    @Override public void suffix(Component value) { set("suffix", json(value)); }

    @Override public boolean hasColor() { return !"reset".equals(property("color")); }

    @Override
    public TextColor color() {
        String color = property("color");
        NamedTextColor named = NamedTextColor.NAMES.value(color);
        if (named == null) throw new IllegalStateException("Team colors must have hex values");
        return named;
    }

    @Override
    public void color(NamedTextColor color) {
        set("color", color == null ? "reset" : NamedTextColor.NAMES.key(color));
    }

    @Override public String getPrefix() { return ComponentJson.legacy(prefix()); }
    @Override public void setPrefix(String prefix) { prefix(legacyText(prefix)); }
    @Override public String getSuffix() { return ComponentJson.legacy(suffix()); }
    @Override public void setSuffix(String suffix) { suffix(legacyText(suffix)); }
    @Override public String getDisplayName() { return ComponentJson.legacy(displayName()); }
    @Override public void setDisplayName(String displayName) { displayName(legacyText(displayName)); }

    /** Section-sign text as a component: the client draws those codes in literal text. */
    private static Component legacyText(String text) {
        if (text == null) throw new IllegalArgumentException("text cannot be null");
        return text.isEmpty() ? Component.empty() : Component.text(text);
    }

    @Override
    public ChatColor getColor() {
        String color = property("color");
        try {
            return ChatColor.valueOf(color.toUpperCase(Locale.ROOT));
        } catch (IllegalArgumentException unknown) {
            return ChatColor.RESET;
        }
    }

    @Override
    public void setColor(ChatColor color) {
        if (color == null) throw new IllegalArgumentException("color cannot be null");
        NamedTextColor named = NamedTextColor.NAMES.value(color.name().toLowerCase(Locale.ROOT));
        if (named == null && color != ChatColor.RESET) {
            throw new IllegalArgumentException("a team colour must be a colour, not " + color);
        }
        color(named);
    }

    @Override public boolean allowFriendlyFire() { return Boolean.parseBoolean(property("friendlyFire")); }
    @Override public void setAllowFriendlyFire(boolean enabled) { set("friendlyFire", Boolean.toString(enabled)); }
    @Override public boolean canSeeFriendlyInvisibles() { return Boolean.parseBoolean(property("seeFriendlyInvisibles")); }
    @Override public void setCanSeeFriendlyInvisibles(boolean enabled) { set("seeFriendlyInvisibles", Boolean.toString(enabled)); }

    @Override
    public Set<String> getEntries() {
        property("color");
        String[] entries = Native.scoreboardTeamEntries(world, name);
        if (entries == null) return Collections.emptySet();
        return Collections.unmodifiableSet(new LinkedHashSet<>(Arrays.asList(entries)));
    }

    @Override public int getSize() { return getEntries().size(); }
    @Override public Scoreboard getScoreboard() { return new FotonScoreboard(world); }

    @Override
    public void addEntry(String entry) {
        if (entry == null) throw new IllegalArgumentException("entry cannot be null");
        property("color");
        if (!Native.scoreboardAddTeamEntry(world, name, entry)) {
            throw new IllegalArgumentException("cannot add entry '" + entry + "' to team " + name);
        }
    }

    @Override public void addEntries(Collection<String> entries) { for (String entry : entries) addEntry(entry); }

    @Override
    public boolean removeEntry(String entry) {
        if (entry == null) throw new IllegalArgumentException("entry cannot be null");
        property("color");
        return Native.scoreboardRemoveTeamEntry(world, name, entry);
    }

    @Override
    public boolean removeEntries(Collection<String> entries) {
        boolean removed = false;
        for (String entry : entries) removed |= removeEntry(entry);
        return removed;
    }

    @Override public boolean hasEntry(String entry) { return entry != null && getEntries().contains(entry); }

    private static String entry(OfflinePlayer player) {
        if (player == null || player.getName() == null) throw new IllegalArgumentException("player has no name");
        return player.getName();
    }

    private static String entry(Entity entity) {
        if (entity == null) throw new IllegalArgumentException("entity cannot be null");
        return entity instanceof Player player ? player.getName() : entity.getUniqueId().toString();
    }

    @Override public void addPlayer(OfflinePlayer player) { addEntry(entry(player)); }
    @Override public boolean removePlayer(OfflinePlayer player) { return removeEntry(entry(player)); }
    @Override public boolean hasPlayer(OfflinePlayer player) { return hasEntry(entry(player)); }
    @Override public void addEntity(Entity entity) { addEntry(entry(entity)); }
    @Override public boolean removeEntity(Entity entity) { return removeEntry(entry(entity)); }
    @Override public boolean hasEntity(Entity entity) { return hasEntry(entry(entity)); }

    @Override
    public void unregister() {
        if (!Native.scoreboardUnregisterTeam(world, name)) throw new IllegalStateException("Unregistered scoreboard component");
    }

    @Override
    public OptionStatus getOption(Option option) {
        return switch (option) {
            case NAME_TAG_VISIBILITY -> status(property("nameTagVisibility"));
            case COLLISION_RULE -> status(property("collisionRule"));
            case DEATH_MESSAGE_VISIBILITY -> {
                property("color");
                yield OptionStatus.ALWAYS;
            }
        };
    }

    @Override
    public void setOption(Option option, OptionStatus status) {
        if (status == null) throw new IllegalArgumentException("status cannot be null");
        switch (option) {
            case NAME_TAG_VISIBILITY -> set("nameTagVisibility", name(status, "hide_for"));
            case COLLISION_RULE -> set("collisionRule", name(status, "push"));
            case DEATH_MESSAGE_VISIBILITY -> {
                if (status != OptionStatus.ALWAYS) {
                    throw new UnsupportedOperationException("Foton shows every death message; a team cannot hide them");
                }
            }
        }
    }

    /** The server's name for a status: hide_for_other_teams, push_own_team... */
    private static String name(OptionStatus status, String verb) {
        return switch (status) {
            case ALWAYS -> "always";
            case NEVER -> "never";
            case FOR_OTHER_TEAMS -> verb + "_other_teams";
            case FOR_OWN_TEAM -> verb + "_own_team";
        };
    }

    private static OptionStatus status(String name) {
        if (name.endsWith("_other_teams")) return OptionStatus.FOR_OTHER_TEAMS;
        if (name.endsWith("_own_team")) return OptionStatus.FOR_OWN_TEAM;
        return "never".equals(name) ? OptionStatus.NEVER : OptionStatus.ALWAYS;
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof FotonTeam team && world.equals(team.world) && name.equals(team.name);
    }

    @Override
    public int hashCode() {
        return 31 * world.hashCode() + name.hashCode();
    }
}
