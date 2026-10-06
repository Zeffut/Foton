package foton;

import com.destroystokyo.paper.profile.ProfileProperty;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.net.MalformedURLException;
import java.net.URI;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.util.Base64;
import java.util.Collection;
import java.util.LinkedHashSet;
import java.util.Set;
import java.util.UUID;
import org.bukkit.profile.PlayerTextures;

/** A game profile: id, name and authlib properties. Textures are a view of the
 * unsigned-or-signed {@code textures} property, so the two never disagree. */
public final class FotonPlayerProfile implements com.destroystokyo.paper.profile.PlayerProfile {
    private final UUID id; private final String name;
    private final PlayerTextures textures = new Textures();
    private final Set<ProfileProperty> properties = new LinkedHashSet<>();
    public FotonPlayerProfile(UUID id, String name) { this.id = id; this.name = name; }
    @Override public UUID getUniqueId() { return id; }
    @Override public String getName() { return name; }
    @Override public PlayerTextures getTextures() { return textures; }
    @Override public void setTextures(PlayerTextures value) {
        textures.clear();
        if (value == null) return;
        textures.setSkin(value.getSkin());
        textures.setCape(value.getCape());
        if (value.getSkinModel() != null) textures.setSkinModel(value.getSkinModel());
    }
    @Override public boolean isComplete() { return id != null || name != null; }

    /** Name, id, properties and the textures they carry; later edits stay apart. */
    public FotonPlayerProfile copy() {
        FotonPlayerProfile copy = new FotonPlayerProfile(id, name);
        copy.properties.addAll(properties);
        return copy;
    }

    /** Any Bukkit profile as one of ours, so later edits to the original do not reach it. */
    public static FotonPlayerProfile copyOf(org.bukkit.profile.PlayerProfile profile) {
        if (profile instanceof FotonPlayerProfile ours) return ours.copy();
        FotonPlayerProfile copy = new FotonPlayerProfile(profile.getUniqueId(), profile.getName());
        if (profile instanceof com.destroystokyo.paper.profile.PlayerProfile paper) copy.properties.addAll(paper.getProperties());
        copy.setTextures(profile.getTextures());
        return copy;
    }

    /** A player's profile with the properties they logged in with. */
    public static FotonPlayerProfile of(UUID id, String name, String[] encodedProperties) {
        FotonPlayerProfile profile = new FotonPlayerProfile(id, name);
        if (encodedProperties == null) return profile;
        for (String encoded : encodedProperties) {
            String[] parts = encoded.split(",", -1);
            if (parts.length != 3) continue;
            String signature = ItemComponents.unhex(parts[2]);
            profile.properties.add(new ProfileProperty(ItemComponents.unhex(parts[0]), ItemComponents.unhex(parts[1]),
                signature.isEmpty() ? null : signature));
        }
        return profile;
    }

    @Override public Set<ProfileProperty> getProperties() { return properties; }

    @Override public void setProperty(ProfileProperty property) {
        if (property == null) return;
        removeProperty(property.getName());
        properties.add(property);
    }

    @Override public void setProperties(Collection<ProfileProperty> incoming) {
        if (incoming == null) return;
        for (ProfileProperty property : incoming) setProperty(property);
    }

    @Override public boolean removeProperty(String name) {
        return properties.removeIf(property -> property.getName().equals(name));
    }

    @Override public void clearProperties() { properties.clear(); }

    @Override public boolean equals(Object other) {
        return other instanceof FotonPlayerProfile that && java.util.Objects.equals(id, that.id)
            && java.util.Objects.equals(name, that.name) && properties.equals(that.properties);
    }

    @Override public int hashCode() { return java.util.Objects.hash(id, name, properties); }

    /** Reads and rewrites the {@code textures} property; a change drops its signature. */
    private final class Textures implements PlayerTextures {
        private JsonObject read() {
            for (ProfileProperty property : properties) {
                if (!property.getName().equals("textures")) continue;
                try {
                    JsonObject root = JsonParser.parseString(new String(
                        Base64.getDecoder().decode(property.getValue()), StandardCharsets.UTF_8)).getAsJsonObject();
                    return root.has("textures") ? root.getAsJsonObject("textures") : new JsonObject();
                } catch (RuntimeException malformed) {
                    return new JsonObject();
                }
            }
            return new JsonObject();
        }

        private void write(JsonObject textures) {
            removeProperty("textures");
            if (textures.size() == 0) return;
            JsonObject root = new JsonObject();
            root.addProperty("timestamp", System.currentTimeMillis());
            if (id != null) root.addProperty("profileId", id.toString().replace("-", ""));
            if (name != null) root.addProperty("profileName", name);
            root.add("textures", textures);
            properties.add(new ProfileProperty("textures",
                Base64.getEncoder().encodeToString(root.toString().getBytes(StandardCharsets.UTF_8))));
        }

        private URL url(String kind) {
            JsonObject textures = read();
            if (!textures.has(kind) || !textures.getAsJsonObject(kind).has("url")) return null;
            try { return URI.create(textures.getAsJsonObject(kind).get("url").getAsString()).toURL(); }
            catch (MalformedURLException | IllegalArgumentException malformed) { return null; }
        }

        private void put(String kind, URL url) {
            JsonObject textures = read();
            if (url == null) textures.remove(kind);
            else {
                JsonObject entry = new JsonObject();
                entry.addProperty("url", url.toString());
                textures.add(kind, entry);
            }
            write(textures);
        }

        @Override public URL getSkin() { return url("SKIN"); }
        @Override public void setSkin(URL value) { put("SKIN", value); }
        @Override public URL getCape() { return url("CAPE"); }
        @Override public void setCape(URL value) { put("CAPE", value); }

        @Override public String getSkinModel() {
            JsonObject textures = read();
            if (!textures.has("SKIN")) return null;
            JsonObject skin = textures.getAsJsonObject("SKIN");
            boolean slim = skin.has("metadata") && skin.getAsJsonObject("metadata").has("model")
                && skin.getAsJsonObject("metadata").get("model").getAsString().equals("slim");
            return slim ? "slim" : "classic";
        }

        @Override public void setSkinModel(String value) {
            JsonObject textures = read();
            if (!textures.has("SKIN")) return;
            JsonObject skin = textures.getAsJsonObject("SKIN");
            skin.remove("metadata");
            if ("slim".equalsIgnoreCase(value)) {
                JsonObject metadata = new JsonObject();
                metadata.addProperty("model", "slim");
                skin.add("metadata", metadata);
            }
            write(textures);
        }

        @Override public void clear() { removeProperty("textures"); }
    }
}
