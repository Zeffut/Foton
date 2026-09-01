package org.bukkit.entity;

import java.util.UUID;
import org.bukkit.Location;
import org.bukkit.World;
import org.bukkit.command.CommandSender;

/** Anything in a world that has a position. */
public interface Entity extends CommandSender {
    UUID getUniqueId();

    Location getLocation();

    World getWorld();

    int getEntityId();

    boolean isDead();

    /** The scheduler for work that follows this entity. */
    io.papermc.paper.threadedregions.scheduler.EntityScheduler getScheduler();
}
