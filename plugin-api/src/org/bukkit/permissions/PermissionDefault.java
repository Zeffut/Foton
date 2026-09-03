package org.bukkit.permissions;

/**
 * Default assignment for a permission when no explicit value is configured.
 *
 * <p>The values and operator semantics match Bukkit/Paper.</p>
 */
public enum PermissionDefault {
    FALSE,
    TRUE,
    OP,
    NOT_OP;

    /** Returns the default value for a sender with the supplied operator status. */
    public boolean getValue(boolean op) {
        return switch (this) {
            case FALSE -> false;
            case TRUE -> true;
            case OP -> op;
            case NOT_OP -> !op;
        };
    }
}
