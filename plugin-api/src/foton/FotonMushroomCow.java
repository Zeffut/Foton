package foton;

import java.util.UUID;

/** Live Bukkit view of a Steel mooshroom. */
public final class FotonMushroomCow extends FotonAnimals implements org.bukkit.entity.MushroomCow {
    public FotonMushroomCow(UUID id) { super(id); }
    @Override public Variant getVariant() {
        return "brown".equalsIgnoreCase(Native.mushroomCowVariant(getUniqueId().toString())) ? Variant.BROWN : Variant.RED;
    }
    @Override public void setVariant(Variant variant) {
        Native.setMushroomCowVariant(getUniqueId().toString(),
            java.util.Objects.requireNonNull(variant, "variant").name());
    }

    @Override public boolean hasEffectsForNextStew() { return !getStewEffects().isEmpty(); }

    @Override public java.util.List<org.bukkit.potion.PotionEffect> getEffectsForNextStew() {
        java.util.ArrayList<org.bukkit.potion.PotionEffect> effects = new java.util.ArrayList<>();
        for (io.papermc.paper.potion.SuspiciousEffectEntry entry : getStewEffects()) {
            effects.add(new org.bukkit.potion.PotionEffect(entry.effect(), entry.duration(), 0));
        }
        return java.util.Collections.unmodifiableList(effects);
    }

    @Override public boolean addEffectToNextStew(
            org.bukkit.potion.PotionEffect effect, boolean overwrite) {
        java.util.Objects.requireNonNull(effect, "effect");
        return addEffectToNextStew(io.papermc.paper.potion.SuspiciousEffectEntry.create(
            effect.getType(), effect.getDuration()), overwrite);
    }

    @Override public boolean addEffectToNextStew(
            io.papermc.paper.potion.SuspiciousEffectEntry entry, boolean overwrite) {
        java.util.Objects.requireNonNull(entry, "entry");
        java.util.ArrayList<io.papermc.paper.potion.SuspiciousEffectEntry> effects =
            new java.util.ArrayList<>(getStewEffects());
        if (!overwrite && hasEffectForNextStew(entry.effect())) {
            return false;
        }
        if (overwrite) {
            removeEffectFromNextStew(entry.effect());
        }
        effects.add(entry);
        setStewEffects(effects);
        return true;
    }

    @Override public boolean removeEffectFromNextStew(org.bukkit.potion.PotionEffectType type) {
        java.util.Objects.requireNonNull(type, "type");
        java.util.ArrayList<io.papermc.paper.potion.SuspiciousEffectEntry> effects =
            new java.util.ArrayList<>(getStewEffects());
        boolean changed = effects.removeIf(entry -> entry.effect().equals(type));
        if (changed) setStewEffects(effects);
        return changed;
    }

    @Override public boolean hasEffectForNextStew(org.bukkit.potion.PotionEffectType type) {
        java.util.Objects.requireNonNull(type, "type");
        return getStewEffects().stream().anyMatch(entry -> entry.effect().equals(type));
    }

    @Override public void clearEffectsForNextStew() { setStewEffects(java.util.List.of()); }

    @Override public java.util.List<io.papermc.paper.potion.SuspiciousEffectEntry> getStewEffects() {
        String[] encoded = Native.mushroomCowStewEffects(getUniqueId().toString());
        if (encoded == null || encoded.length == 0) return java.util.List.of();
        java.util.ArrayList<io.papermc.paper.potion.SuspiciousEffectEntry> effects =
            new java.util.ArrayList<>();
        for (String value : encoded) {
            String[] fields = value.split("\\|", -1);
            if (fields.length != 2) continue;
            org.bukkit.potion.PotionEffectType type =
                org.bukkit.potion.PotionEffectType.getByName(fields[0]);
            try {
                if (type != null) effects.add(io.papermc.paper.potion.SuspiciousEffectEntry.create(
                    type, Integer.parseInt(fields[1])));
            } catch (NumberFormatException ignored) { }
        }
        return java.util.Collections.unmodifiableList(effects);
    }

    @Override public void setStewEffects(
            java.util.List<io.papermc.paper.potion.SuspiciousEffectEntry> effects) {
        java.util.Objects.requireNonNull(effects, "effects");
        String[] encoded = new String[effects.size()];
        for (int index = 0; index < effects.size(); index++) {
            io.papermc.paper.potion.SuspiciousEffectEntry entry =
                java.util.Objects.requireNonNull(effects.get(index), "effect");
            encoded[index] = entry.effect().getKey().getKey() + "|" + entry.duration();
        }
        if (!Native.setMushroomCowStewEffects(getUniqueId().toString(), encoded)) {
            throw new IllegalArgumentException("Invalid suspicious stew effects");
        }
    }

    @Override public void shear(net.kyori.adventure.sound.Sound.Source source) {
        Native.shearMushroomCow(getUniqueId().toString(),
            java.util.Objects.requireNonNull(source, "source").name());
    }

    @Override public boolean readyToBeSheared() {
        return Native.mushroomCowReadyToShear(getUniqueId().toString());
    }
}
