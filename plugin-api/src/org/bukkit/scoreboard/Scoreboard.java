package org.bukkit.scoreboard;

import java.util.Set;

/** Scoreboard view for one Foton domain. */
public interface Scoreboard {
    Team getEntryTeam(String entry);
    Set<Team> getTeams();
    Team getTeam(String name);
    Team registerNewTeam(String name);
    default Objective getObjective(DisplaySlot slot) { return null; }
}
