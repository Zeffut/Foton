package foton;

import java.util.UUID;

/** Boat entity handle; boat variants currently share Steel's vehicle state. */
public final class FotonBoat extends FotonVehicle implements org.bukkit.entity.Boat {
    public FotonBoat(UUID id) { super(id); }
    @Override public org.bukkit.entity.Boat.Type getBoatType() {
        try { return org.bukkit.entity.Boat.Type.valueOf(Native.boatType(getUniqueId().toString())); } catch (RuntimeException e) { return org.bukkit.entity.Boat.Type.OAK; }
    }
    @Override public org.bukkit.TreeSpecies getWoodType() {
        String type = Native.boatType(getUniqueId().toString());
        if ("SPRUCE".equals(type)) return org.bukkit.TreeSpecies.REDWOOD;
        if ("BIRCH".equals(type)) return org.bukkit.TreeSpecies.BIRCH;
        if ("JUNGLE".equals(type)) return org.bukkit.TreeSpecies.JUNGLE;
        if ("ACACIA".equals(type)) return org.bukkit.TreeSpecies.ACACIA;
        if ("DARK_OAK".equals(type)) return org.bukkit.TreeSpecies.DARK_OAK;
        return org.bukkit.TreeSpecies.GENERIC;
    }
    @Override public void setBoatType(org.bukkit.entity.Boat.Type type) {
        if (type != null) Native.setBoatType(getUniqueId().toString(), type.name());
    }
    @Override public void setWoodType(org.bukkit.TreeSpecies species) {
        if (species != null) Native.setBoatType(getUniqueId().toString(), species.name());
    }
}
