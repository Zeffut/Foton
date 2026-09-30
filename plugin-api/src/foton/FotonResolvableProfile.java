package foton;

import com.destroystokyo.paper.profile.ProfileProperty;
import io.papermc.paper.datacomponent.item.ResolvableProfile;
import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.Objects;
import java.util.UUID;
import net.kyori.adventure.key.Key;
import net.kyori.adventure.text.object.PlayerHeadObjectContents;

/** An immutable profile component, and the builder Paper hands out for one. */
public final class FotonResolvableProfile implements ResolvableProfile {
    public static final SkinPatch EMPTY_PATCH = new Patch(null, null, null);

    private final UUID uuid;
    private final String name;
    private final List<ProfileProperty> properties;
    private final SkinPatch skinPatch;

    private FotonResolvableProfile(UUID uuid, String name, List<ProfileProperty> properties, SkinPatch skinPatch) {
        this.uuid = uuid;
        this.name = name;
        this.properties = List.copyOf(properties);
        this.skinPatch = skinPatch;
    }

    @Override public UUID uuid() { return uuid; }
    @Override public String name() { return name; }
    @Override public Collection<ProfileProperty> properties() { return properties; }
    @Override public SkinPatch skinPatch() { return skinPatch; }

    @Override
    public void applySkinToPlayerHeadContents(PlayerHeadObjectContents.Builder builder) {
        if (name != null) builder.name(name);
        if (uuid != null) builder.id(uuid);
        for (ProfileProperty property : properties) {
            builder.profileProperty(property.getSignature() == null
                ? PlayerHeadObjectContents.property(property.getName(), property.getValue())
                : PlayerHeadObjectContents.property(property.getName(), property.getValue(), property.getSignature()));
        }
        if (skinPatch.body() != null) builder.texture(skinPatch.body());
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof FotonResolvableProfile that && Objects.equals(uuid, that.uuid)
            && Objects.equals(name, that.name) && properties.equals(that.properties)
            && skinPatch.equals(that.skinPatch);
    }

    @Override
    public int hashCode() {
        return Objects.hash(uuid, name, properties, skinPatch);
    }

    public record Patch(Key body, Key cape, Key elytra) implements SkinPatch { }

    public static final class Builder implements ResolvableProfile.Builder {
        private UUID uuid;
        private String name;
        private final List<ProfileProperty> properties = new ArrayList<>();
        private SkinPatch skinPatch = EMPTY_PATCH;

        @Override public Builder name(String value) { name = value; return this; }
        @Override public Builder uuid(UUID value) { uuid = value; return this; }

        @Override
        public Builder addProperty(ProfileProperty property) {
            properties.add(Objects.requireNonNull(property, "property"));
            return this;
        }

        @Override
        public Builder addProperties(Collection<ProfileProperty> values) {
            for (ProfileProperty property : values) addProperty(property);
            return this;
        }

        @Override
        public Builder skinPatch(SkinPatch patch) {
            skinPatch = Objects.requireNonNull(patch, "patch");
            return this;
        }

        @Override
        public ResolvableProfile build() {
            return new FotonResolvableProfile(uuid, name, properties, skinPatch);
        }
    }
}
