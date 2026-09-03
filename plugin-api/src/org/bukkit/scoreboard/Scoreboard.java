package org.bukkit.scoreboard;

/** Scoreboard view for one Foton domain. */
public interface Scoreboard {
    Team getEntryTeam(String entry);
    default Objective getObjective(DisplaySlot slot) { return null; }
}
