package org.bukkit;

import org.bukkit.block.Block;
import org.bukkit.entity.Entity;

/** The data a vibration particle travels with: where it goes, and how fast. */
public class Vibration {
    private final Location origin;
    private final Destination destination;
    private final int arrivalTime;

    public Vibration(Destination destination, int arrivalTime) {
        this.destination = destination;
        this.arrivalTime = arrivalTime;
        this.origin = new Location(null, 0, 0, 0);
    }

    @Deprecated public Vibration(Location origin, Destination destination, int arrivalTime) {
        this.origin = origin;
        this.destination = destination;
        this.arrivalTime = arrivalTime;
    }

    @Deprecated public Location getOrigin() { return origin; }
    public Destination getDestination() { return destination; }
    public int getArrivalTime() { return arrivalTime; }

    public interface Destination {
        class EntityDestination implements Destination {
            private final Entity entity;
            public EntityDestination(Entity entity) { this.entity = entity; }
            public Entity getEntity() { return entity; }
        }

        class BlockDestination implements Destination {
            private final Location block;
            public BlockDestination(Location block) { this.block = block.clone(); }
            public BlockDestination(Block block) { this(block.getLocation()); }
            public Location getLocation() { return block.clone(); }
            public Block getBlock() { return block.getBlock(); }
        }
    }
}
