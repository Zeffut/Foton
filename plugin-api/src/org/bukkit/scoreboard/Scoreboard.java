package org.bukkit.scoreboard;

import java.util.Set;

/** Scoreboard view for one Foton domain: the scoreboard {@code /team} uses there. */
public interface Scoreboard {
    Team getEntryTeam(String entry);

    default Objective getObjective(DisplaySlot slot) { return null; }

    /** The team with this name, or null. */
    Team getTeam(String name);

    /** Every team, as they are now. */
    Set<Team> getTeams();

    /** Creates a team; throws when the name is already one, as Paper does. */
    Team registerNewTeam(String name);

    default Team getPlayerTeam(org.bukkit.OfflinePlayer player) {
        return player == null || player.getName() == null ? null : getEntryTeam(player.getName());
    }
}
