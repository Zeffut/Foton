package foton;

import java.util.UUID;

/** Live Bukkit view of a mannequin. */
public final class FotonMannequin extends FotonLivingEntity implements org.bukkit.entity.Mannequin {
    public FotonMannequin(UUID id) { super(id); }

    @Override public io.papermc.paper.datacomponent.item.ResolvableProfile getProfile() {
        String[] profile = Native.mannequinProfile(getUniqueId().toString());
        var builder = io.papermc.paper.datacomponent.item.ResolvableProfile.resolvableProfile();
        if (profile == null || profile.length < 2) return (io.papermc.paper.datacomponent.item.ResolvableProfile) builder.build();
        if (!profile[0].isEmpty()) builder.uuid(UUID.fromString(profile[0]));
        if (!profile[1].isEmpty()) builder.name(profile[1]);
        for (int i = 2; i + 2 < profile.length; i += 3) {
            builder.addProperty(new com.destroystokyo.paper.profile.ProfileProperty(profile[i], profile[i + 1],
                profile[i + 2].isEmpty() ? null : profile[i + 2]));
        }
        return (io.papermc.paper.datacomponent.item.ResolvableProfile) builder.build();
    }

    /** Shows the profile's skin: a name and id make a full profile, anything
     * less a partial one, exactly as vanilla's profile component reads. */
    @Override public void setProfile(io.papermc.paper.datacomponent.item.ResolvableProfile profile) {
        if (profile == null) throw new IllegalArgumentException("profile");
        java.util.ArrayList<String> properties = new java.util.ArrayList<>();
        for (com.destroystokyo.paper.profile.ProfileProperty property : profile.properties()) {
            properties.add(property.getName());
            properties.add(property.getValue());
            properties.add(property.getSignature() == null ? "" : property.getSignature());
        }
        Native.setMannequinProfile(getUniqueId().toString(),
            profile.uuid() == null ? null : profile.uuid().toString(), profile.name(),
            properties.toArray(new String[0]));
    }

    @Override public boolean isImmovable() { return Native.mannequinImmovable(getUniqueId().toString()); }
    @Override public void setImmovable(boolean immovable) { Native.setMannequinImmovable(getUniqueId().toString(), immovable); }

    @Override public void setDescription(net.kyori.adventure.text.Component description) {
        Native.setMannequinDescription(getUniqueId().toString(), FotonComponents.toJson(description));
    }
}
