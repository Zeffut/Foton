package org.bukkit.inventory.meta;

import java.util.List;

/** Title, author and pages carried by a writable or written book. */
public interface BookMeta extends ItemMeta {
    boolean hasTitle();

    String getTitle();

    boolean setTitle(String title);

    boolean hasAuthor();

    String getAuthor();

    void setAuthor(String author);

    boolean hasPages();

    String getPage(int page);

    void setPage(int page, String data);

    List<String> getPages();

    void setPages(List<String> pages);

    void setPages(String... pages);

    void addPage(String... pages);

    int getPageCount();

    Generation getGeneration();

    void setGeneration(Generation generation);

    @Override
    BookMeta clone();

    enum Generation {
        ORIGINAL,
        COPY_OF_ORIGINAL,
        COPY_OF_COPY,
        TATTERED
    }
}
