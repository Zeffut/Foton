package org.bukkit.attribute;

/** Entity or object exposing mutable vanilla attributes. */
public interface Attributable {
    AttributeInstance getAttribute(Attribute attribute);
}
