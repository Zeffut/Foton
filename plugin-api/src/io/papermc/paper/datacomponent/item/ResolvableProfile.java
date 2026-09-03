package io.papermc.paper.datacomponent.item;

import com.destroystokyo.paper.profile.ProfileProperty;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.UUID;

/** Immutable modern Paper profile component. */
public final class ResolvableProfile {
    private final UUID uuid;
    private final String name;
    private final Collection<ProfileProperty> properties;
    private final SkinPatch skinPatch;

    private ResolvableProfile(UUID uuid, String name, Collection<ProfileProperty> properties, SkinPatch skinPatch) {
        this.uuid = uuid; this.name = name;
        this.properties = Collections.unmodifiableList(new ArrayList<>(properties));
        this.skinPatch = skinPatch;
    }
    public UUID uuid() { return uuid; }
    public String name() { return name; }
    public Collection<ProfileProperty> properties() { return properties; }
    public SkinPatch skinPatch() { return skinPatch; }

    public static Builder resolvableProfile() { return new Builder(); }

    public static final class Builder {
        private UUID uuid; private String name;
        private final Collection<ProfileProperty> properties = new ArrayList<>();
        private SkinPatch skinPatch;
        public Builder uuid(UUID value) { uuid = value; return this; }
        public Builder name(String value) { name = value; return this; }
        public Builder addProperty(ProfileProperty value) { if (value != null) properties.add(value); return this; }
        public Builder skinPatch(SkinPatch value) { skinPatch = value; return this; }
        public Object build() { return new ResolvableProfile(uuid, name, properties, skinPatch); }
    }

    /** Optional skin patch values carried by a profile component. */
    public static final class SkinPatch {
        private final String body;
        public SkinPatch(String body) { this.body = body; }
        public String body() { return body; }
    }
}
