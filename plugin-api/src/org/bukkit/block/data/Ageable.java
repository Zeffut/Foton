package org.bukkit.block.data;

/** Block data with vanilla's {@code age} property: crops, stems, vines, cacti... */
public interface Ageable extends BlockData {
    int getAge();

    /** Throws when the age is outside what this block allows. */
    void setAge(int age);

    /** The largest age this block takes, which differs by block (wheat 7, beetroots 3). */
    int getMaximumAge();
}
