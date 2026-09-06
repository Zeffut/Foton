package com.destroystokyo.paper.profile;

/** A signed profile property such as a skin texture. */
public final class ProfileProperty {
    private final String name;
    private final String value;
    private final String signature;
    public ProfileProperty(String name, String value) { this(name, value, null); }
    public ProfileProperty(String name, String value, String signature) {
        this.name = name;
        this.value = value;
        this.signature = signature;
    }
    public String getName() { return name; }
    public String getValue() { return value; }
    public String getSignature() { return signature; }
    public boolean isSigned() { return signature != null; }

    // Value semantics, because these live in a `Set`. Without them the set
    // dedupes by identity and the same property added twice is two entries.
    @Override public boolean equals(Object other) {
        return other instanceof ProfileProperty p
                && java.util.Objects.equals(name, p.name)
                && java.util.Objects.equals(value, p.value)
                && java.util.Objects.equals(signature, p.signature);
    }

    @Override public int hashCode() {
        return java.util.Objects.hash(name, value, signature);
    }

    @Override public String toString() {
        return "ProfileProperty[name=" + name + ", signed=" + isSigned() + "]";
    }
}
