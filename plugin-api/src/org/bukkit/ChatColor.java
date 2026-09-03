package org.bukkit;

/** The section-sign color codes, which are still how most plugins color text.
 *
 * `toString` is the code itself, because that is how plugins use it: string
 * concatenation, not a formatting call.
 */
public enum ChatColor {
    BLACK('0'),
    DARK_BLUE('1'),
    DARK_GREEN('2'),
    DARK_AQUA('3'),
    DARK_RED('4'),
    DARK_PURPLE('5'),
    GOLD('6'),
    GRAY('7'),
    DARK_GRAY('8'),
    BLUE('9'),
    GREEN('a'),
    AQUA('b'),
    RED('c'),
    LIGHT_PURPLE('d'),
    YELLOW('e'),
    WHITE('f'),
    MAGIC('k'),
    BOLD('l'),
    STRIKETHROUGH('m'),
    UNDERLINE('n'),
    ITALIC('o'),
    RESET('r');

    /** The character Minecraft reads a color code after. */
    public static final char COLOR_CHAR = '\u00A7';

    private final char code;
    private final String text;

    ChatColor(char code) {
        this.code = code;
        this.text = new String(new char[] {COLOR_CHAR, code});
    }

    /** Looks up a color or formatting code (case-insensitive). */
    public static ChatColor getByChar(String code) {
        if (code == null || code.isEmpty()) return null;
        char value = Character.toLowerCase(code.charAt(0));
        for (ChatColor color : values()) if (color.code == value) return color;
        return null;
    }

    public char getChar() {
        return code;
    }

    @Override
    public String toString() {
        return text;
    }

    /** Turns `&a` into a real color code, which is what a config file holds. */
    public static String translateAlternateColorCodes(char alternate, String text) {
        if (text == null) {
            return null;
        }
        char[] chars = text.toCharArray();
        for (int i = 0; i < chars.length - 1; i++) {
            if (chars[i] == alternate && "0123456789AaBbCcDdEeFfKkLlMmNnOoRr"
                    .indexOf(chars[i + 1]) > -1) {
                chars[i] = COLOR_CHAR;
                chars[i + 1] = Character.toLowerCase(chars[i + 1]);
            }
        }
        return new String(chars);
    }

    /** Removes every color code, for a log line or a length check. */
    public static String stripColor(String text) {
        if (text == null) {
            return null;
        }
        StringBuilder out = new StringBuilder(text.length());
        for (int i = 0; i < text.length(); i++) {
            char c = text.charAt(i);
            if (c == COLOR_CHAR && i + 1 < text.length()) {
                i++;
                continue;
            }
            out.append(c);
        }
        return out.toString();
    }

    /** Returns the formatting codes active at the end of a string. */
    public static String getLastColors(String input) {
        if (input == null || input.isEmpty()) return "";
        StringBuilder result = new StringBuilder(2);
        for (int i = input.length() - 2; i >= 0; i--) {
            if (input.charAt(i) != COLOR_CHAR) continue;
            char code = Character.toLowerCase(input.charAt(i + 1));
            if ("0123456789abcdef".indexOf(code) >= 0) {
                result.insert(0, COLOR_CHAR).insert(1, code);
                break;
            }
            if ("klmno".indexOf(code) >= 0) result.insert(0, COLOR_CHAR).insert(1, code);
            else if (code == 'r') { result.setLength(0); break; }
        }
        return result.toString();
    }
}
