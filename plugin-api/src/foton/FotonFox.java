package foton;

import java.util.UUID;

/** Live Bukkit view of a Steel fox. */
public final class FotonFox extends FotonLivingEntity implements org.bukkit.entity.Fox {
    public FotonFox(UUID id) { super(id); }

    @Override public Type getFoxType() {
        String type = Native.foxType(getUniqueId().toString());
        return "snow".equalsIgnoreCase(type) ? Type.SNOW : Type.RED;
    }

    @Override public void setFoxType(Type type) {
        if (type != null) Native.setFoxType(getUniqueId().toString(), type.name());
    }

    @Override public boolean isSitting() { return Native.foxSitting(getUniqueId().toString()); }
    @Override public void setSitting(boolean sitting) { Native.setFoxSitting(getUniqueId().toString(), sitting); }
}
