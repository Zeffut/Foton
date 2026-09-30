package foton;

/** One reading of vanilla's {@code Input} flags, as the client sent them. */
public record FotonInput(int flags) implements org.bukkit.Input {
    @Override public boolean isForward() { return (flags & 0x01) != 0; }
    @Override public boolean isBackward() { return (flags & 0x02) != 0; }
    @Override public boolean isLeft() { return (flags & 0x04) != 0; }
    @Override public boolean isRight() { return (flags & 0x08) != 0; }
    @Override public boolean isJump() { return (flags & 0x10) != 0; }
    @Override public boolean isSneak() { return (flags & 0x20) != 0; }
    @Override public boolean isSprint() { return (flags & 0x40) != 0; }
    @Override public String toString() {
        return "FotonInput{forward=" + isForward() + ", backward=" + isBackward() + ", left=" + isLeft()
            + ", right=" + isRight() + ", jump=" + isJump() + ", sneak=" + isSneak() + ", sprint=" + isSprint() + "}";
    }
}
