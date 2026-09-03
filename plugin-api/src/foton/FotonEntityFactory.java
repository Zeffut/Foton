package foton;

/** Parses and creates Steel's compact entity snapshot representation. */
public final class FotonEntityFactory implements org.bukkit.entity.EntityFactory {
    @Override public org.bukkit.entity.EntitySnapshot createEntitySnapshot(String data) {
        if (data == null) return null;
        int at = data.indexOf('@');
        if (at <= 0) return null;
        String typeName = data.substring(0, at);
        String[] fields = data.substring(at + 1).split(",", -1);
        if (fields.length != 4) return null;
        org.bukkit.entity.EntityType type = org.bukkit.entity.EntityType.fromName(typeName);
        org.bukkit.World world = org.bukkit.Bukkit.getWorld(fields[0]);
        if (type == null || world == null) return null;
        try {
            return new FotonEntitySnapshot(type, new org.bukkit.Location(world,
                Double.parseDouble(fields[1]), Double.parseDouble(fields[2]), Double.parseDouble(fields[3])));
        } catch (NumberFormatException ignored) { return null; }
    }
}
