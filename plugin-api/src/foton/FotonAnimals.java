package foton;

import java.util.UUID;

/** Shared live implementation of Paper's Animals contract. */
abstract class FotonAnimals extends FotonLivingEntity implements org.bukkit.entity.Animals {
    FotonAnimals(UUID id) { super(id); }

    @Override public boolean canBreed() {
        return Native.entityCanBreed(getUniqueId().toString());
    }

    @Override public void setBreed(boolean breed) {
        Native.setEntityBreed(getUniqueId().toString(), breed);
    }

    @Override public UUID getBreedCause() {
        String value = Native.animalBreedCause(getUniqueId().toString());
        return value == null ? null : UUID.fromString(value);
    }

    @Override public void setBreedCause(UUID breedCause) {
        Native.setAnimalBreedCause(getUniqueId().toString(),
            breedCause == null ? "" : breedCause.toString());
    }

    @Override public boolean isLoveMode() {
        return Native.animalLoveTicks(getUniqueId().toString()) > 0;
    }

    @Override public int getLoveModeTicks() {
        return Native.animalLoveTicks(getUniqueId().toString());
    }

    @Override public void setLoveModeTicks(int ticks) {
        if (ticks < 0) {
            throw new IllegalArgumentException("Love mode ticks cannot be negative");
        }
        Native.setAnimalLoveTicks(getUniqueId().toString(), ticks);
    }

    @Override public boolean isBreedItem(org.bukkit.inventory.ItemStack stack) {
        java.util.Objects.requireNonNull(stack, "stack");
        return isBreedItem(stack.getType());
    }

    @Override public boolean isBreedItem(org.bukkit.Material material) {
        java.util.Objects.requireNonNull(material, "material");
        return Native.animalIsBreedItem(getUniqueId().toString(), material.getKey().toString());
    }
}
