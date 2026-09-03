package org.bukkit.util.io;

import java.io.IOException;
import java.io.InputStream;
import java.io.ObjectInputStream;

/** Bukkit-compatible object input stream. */
public class BukkitObjectInputStream extends ObjectInputStream {
    public BukkitObjectInputStream(InputStream input) throws IOException { super(input); }
    @Override public void close() throws IOException { super.close(); }
}
