package org.bukkit.block.data;

/** A block state with an {@code age} property, read from the state string itself.
 *
 * <p>The maximum is the server's: the property's range comes from the block
 * registry rather than a table here, since it differs from block to block.</p>
 */
public final class SimpleAgeableData extends SimpleBlockData implements Ageable {
    public SimpleAgeableData(String text) { super(text); }

    @Override
    public int getAge() {
        try {
            return Integer.parseInt(propertyValue("age"));
        } catch (NumberFormatException missing) {
            return 0;
        }
    }

    @Override
    public void setAge(int age) {
        if (age < 0 || age > getMaximumAge()) {
            throw new IllegalArgumentException("age must be between 0 and " + getMaximumAge() + ", got " + age);
        }
        property("age", Integer.toString(age));
    }

    @Override
    public int getMaximumAge() {
        String[] values = foton.Native.blockPropertyValues(getMaterial().getKey().toString(), "age");
        int maximum = 0;
        if (values != null) {
            for (String value : values) {
                try { maximum = Math.max(maximum, Integer.parseInt(value)); } catch (NumberFormatException ignored) { }
            }
        }
        return maximum;
    }

    @Override
    public BlockData clone() { return new SimpleAgeableData(getAsString()); }
}
