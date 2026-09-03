package foton;

import java.util.UUID;

/** Live Bukkit view of a vanilla panda. */
public final class FotonPanda extends FotonLivingEntity implements org.bukkit.entity.Panda {
    public FotonPanda(UUID id) { super(id); }
    private static Gene gene(String value) {
        if (value == null) return Gene.NORMAL;
        try { return Gene.valueOf(value.toUpperCase(java.util.Locale.ROOT)); }
        catch (IllegalArgumentException ignored) { return Gene.NORMAL; }
    }
    @Override public Gene getMainGene() { return gene(Native.pandaMainGene(getUniqueId().toString())); }
    @Override public void setMainGene(Gene gene) { if (gene != null) Native.setPandaMainGene(getUniqueId().toString(), gene.name()); }
    @Override public Gene getHiddenGene() { return gene(Native.pandaHiddenGene(getUniqueId().toString())); }
    @Override public void setHiddenGene(Gene gene) { if (gene != null) Native.setPandaHiddenGene(getUniqueId().toString(), gene.name()); }
}
