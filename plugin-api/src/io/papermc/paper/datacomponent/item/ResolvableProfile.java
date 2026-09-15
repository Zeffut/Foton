package io.papermc.paper.datacomponent.item;

import com.destroystokyo.paper.profile.ProfileProperty;
import io.papermc.paper.datacomponent.DataComponentBuilder;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.UUID;

/** Immutable modern Paper profile component. */
public interface ResolvableProfile {
    UUID uuid();
    String name();
    Collection<ProfileProperty> properties();
    SkinPatch skinPatch();

    static Builder resolvableProfile() { return ResolvableProfileSupport.builder(); }

    interface Builder extends DataComponentBuilder<ResolvableProfile> {
        Builder uuid(UUID value);
        Builder name(String value);
        Builder addProperty(ProfileProperty value);
        Builder skinPatch(SkinPatch value);
    }

    /** Optional skin patch values carried by a profile component. */
    final class SkinPatch {
        private final String body;
        public SkinPatch(String body) { this.body = body; }
        public String body() { return body; }
    }
}

final class ResolvableProfileSupport {
    private ResolvableProfileSupport() {}

    static ResolvableProfile.Builder builder() { return new BuilderImpl(); }

    private static final class BuilderImpl implements ResolvableProfile.Builder {
        private UUID uuid; private String name;
        private final Collection<ProfileProperty> properties = new ArrayList<>();
        private ResolvableProfile.SkinPatch skinPatch;
        @Override public ResolvableProfile.Builder uuid(UUID value) { uuid = value; return this; }
        @Override public ResolvableProfile.Builder name(String value) { name = value; return this; }
        @Override public ResolvableProfile.Builder addProperty(ProfileProperty value) {
            if (value != null) properties.add(value);
            return this;
        }
        @Override public ResolvableProfile.Builder skinPatch(ResolvableProfile.SkinPatch value) {
            skinPatch = value;
            return this;
        }
        @Override public ResolvableProfile build() {
            return new ResolvableProfileImpl(uuid, name, properties, skinPatch);
        }
    }

    private static final class ResolvableProfileImpl implements ResolvableProfile {
        private final UUID uuid;
        private final String name;
        private final Collection<ProfileProperty> properties;
        private final ResolvableProfile.SkinPatch skinPatch;

        private ResolvableProfileImpl(UUID uuid, String name,
                Collection<ProfileProperty> properties, ResolvableProfile.SkinPatch skinPatch) {
            this.uuid = uuid;
            this.name = name;
            this.properties = Collections.unmodifiableList(new ArrayList<>(properties));
            this.skinPatch = skinPatch;
        }

        @Override public UUID uuid() { return uuid; }
        @Override public String name() { return name; }
        @Override public Collection<ProfileProperty> properties() { return properties; }
        @Override public ResolvableProfile.SkinPatch skinPatch() { return skinPatch; }
    }
}
