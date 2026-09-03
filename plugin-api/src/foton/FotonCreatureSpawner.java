package foton;

import org.bukkit.entity.EntityType;

/** Live-backed view of a vanilla mob spawner block entity. */
public final class FotonCreatureSpawner extends FotonTileState implements org.bukkit.block.CreatureSpawner {
    public FotonCreatureSpawner(org.bukkit.block.Block block, org.bukkit.block.data.BlockData data) {
        super(block, data);
    }
    @Override public EntityType getSpawnedType() {
        String key = Native.spawnerEntityType(getWorld().getName(), getX(), getY(), getZ());
        return key == null ? null : EntityType.fromName(key);
    }
    @Override public void setSpawnedType(EntityType type) {
        if (type != null) Native.setSpawnerEntityType(getWorld().getName(), getX(), getY(), getZ(), type.getName());
    }
    @Override public int getDelay() { return Native.spawnerDelay(getWorld().getName(), getX(), getY(), getZ()); }
    @Override public void setDelay(int delay) { Native.setSpawnerDelay(getWorld().getName(), getX(), getY(), getZ(), delay); }
    @Override public int getMinSpawnDelay() { return Native.spawnerMinSpawnDelay(getWorld().getName(), getX(), getY(), getZ()); }
    @Override public void setMinSpawnDelay(int delay) { Native.setSpawnerMinSpawnDelay(getWorld().getName(), getX(), getY(), getZ(), delay); }
    @Override public int getMaxSpawnDelay() { return Native.spawnerMaxSpawnDelay(getWorld().getName(), getX(), getY(), getZ()); }
    @Override public void setMaxSpawnDelay(int delay) { Native.setSpawnerMaxSpawnDelay(getWorld().getName(), getX(), getY(), getZ(), delay); }
}
