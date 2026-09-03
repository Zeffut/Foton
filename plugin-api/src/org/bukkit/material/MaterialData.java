package org.bukkit.material;

import org.bukkit.Material;

/** Legacy material plus its four-bit data value. */
@Deprecated
public class MaterialData implements Cloneable {
    private final Material itemType;
    private byte data;

    public MaterialData(Material type) {
        this(type, (byte) 0);
    }

    public MaterialData(Material type, byte data) {
        this.itemType = type == null ? Material.AIR : type;
        this.data = data;
    }

    public Material getItemType() { return itemType; }
    public byte getData() { return data; }
    public void setData(byte data) { this.data = data; }

    @Override
    public MaterialData clone() {
        try {
            return (MaterialData) super.clone();
        } catch (CloneNotSupportedException impossible) {
            throw new AssertionError(impossible);
        }
    }
}
