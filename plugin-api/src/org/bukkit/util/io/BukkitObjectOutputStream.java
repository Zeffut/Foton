package org.bukkit.util.io;

import java.io.IOException;
import java.io.ObjectOutputStream;
import java.io.OutputStream;

/** Bukkit-compatible object output stream. */
public class BukkitObjectOutputStream extends ObjectOutputStream {
    public BukkitObjectOutputStream(OutputStream output) throws IOException { super(output); }
    @Override public void flush() throws IOException { super.flush(); }
    @Override public void close() throws IOException { super.close(); }
}
